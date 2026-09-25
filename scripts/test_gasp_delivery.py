"""Real Git repositories, real reducer; only injected transport faults are mocked."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("gasp_delivery", Path(__file__).with_name("gasp_delivery.py"))
delivery = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(delivery)
ROOT = Path(__file__).resolve().parent.parent
VALIDATOR = ROOT / "target/gasp-delivery-validator/debug/gasp-delivery-validator"


def event(identity, kind, payload, run_id=None):
    return {"id": identity, "schema_version": 1, "ts_ms": 1,
            "actor": {"kind": "agent", "id": "test"}, "kind": kind,
            "payload": payload, "causation_id": None, "correlation_id": run_id}


def run_events(run_id):
    return [event(run_id + "_start", "run.started", {"run_id": run_id, "task": "test"}, run_id),
            event(run_id + "_ops", "state.ops_applied", [{"CreateNode": {
                "id": run_id, "kind": "run", "props": {}}}], run_id),
            event(run_id + "_end", "run.finished", {"run_id": run_id, "outcome": "done"}, run_id)]


class DeliveryTests(unittest.TestCase):
    def setUp(self):
        if not VALIDATOR.is_file():
            self.fail("Build scripts/gasp-delivery-validator with --target-dir target/gasp-delivery-validator first")
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.remote = self.root / "remote.git"
        self.source = self.root / "source"
        self.other = self.root / "other"
        delivery.git(self.root, "init", "--bare", "--initial-branch=main", str(self.remote))
        delivery.git(self.root, "clone", str(self.remote), str(self.source))
        self.configure(self.source)
        (self.source / "state").mkdir()
        (self.source / "state/events.jsonl").write_text("")
        (self.source / ".gitattributes").write_text("*.jsonl merge=union\n")
        (self.source / "DREAM.md").write_text("base\n")
        self.commit(self.source, "base")
        delivery.git(self.source, "push", "origin", "main")
        delivery.git(self.root, "clone", str(self.remote), str(self.other))
        self.configure(self.other)
        self.append(self.source, run_events("run_test"))
        self.commit(self.source, "boundary")
        self.original = delivery.git(self.source, "rev-parse", "HEAD").stdout.strip()
        self.env = patch.dict(os.environ, {"GASP_PUSH_URL": str(self.remote)})
        self.env.start()
        self.addCleanup(self.env.stop)

    def configure(self, repo):
        delivery.git(repo, "config", "user.name", "Test")
        delivery.git(repo, "config", "user.email", "test@example.invalid")

    def commit(self, repo, title):
        delivery.git(repo, "add", ".")
        delivery.git(repo, "commit", "-m", title)

    def append(self, repo, rows):
        with (repo / "state/events.jsonl").open("a") as file:
            for row in rows:
                file.write(json.dumps(row, ensure_ascii=False) + "\n")

    def deliver(self, **kwargs):
        return delivery.deliver(self.source, "run_test", VALIDATOR, delay=0, **kwargs)

    def remote_rows(self):
        return delivery.events(delivery.git(self.remote, "show", "main:state/events.jsonl").stdout)

    def test_publish_and_repeat_do_not_duplicate(self):
        self.assertEqual(self.deliver()["status"], "published")
        tip = delivery.git(self.remote, "rev-parse", "main").stdout
        self.assertEqual(self.deliver()["status"], "published")
        self.assertEqual(delivery.git(self.remote, "rev-parse", "main").stdout, tip)
        self.assertEqual(len(self.remote_rows()), 3)
        self.assertEqual(delivery.git(self.source, "rev-parse", "HEAD").stdout.strip(), self.original)

    def test_two_concurrent_advances_are_retried_and_preserved(self):
        real_git, races = delivery.git, []

        def racing(repo, *args, **kwargs):
            if args[:2] == ("push", "--quiet") and args[-1] == "HEAD:refs/heads/main" and len(races) < 2:
                rid = "other_" + str(len(races))
                races.append(rid)
                self.append(self.other, run_events(rid))
                self.commit(self.other, rid)
                real_git(self.other, "push", "origin", "main")
            return real_git(repo, *args, **kwargs)

        with patch.object(delivery, "git", side_effect=racing):
            result = self.deliver()
        self.assertEqual(result["status"], "published", result)
        self.assertEqual(len(races), 2)
        self.assertEqual(len(self.remote_rows()), 9)

    def test_acknowledgement_lost_on_last_attempt_is_verified(self):
        real_git = delivery.git

        def lost(repo, *args, **kwargs):
            result = real_git(repo, *args, **kwargs)
            if args[:2] == ("push", "--quiet") and args[-1] == "HEAD:refs/heads/main":
                self.assertEqual(result.returncode, 0)
                return subprocess.CompletedProcess(args, 1, "", "connection reset after acknowledgement")
            return result

        with patch.object(delivery, "git", side_effect=lost):
            result = self.deliver(attempts=1)
        self.assertEqual(result["status"], "published", result)
        self.assertEqual(len(self.remote_rows()), 3)

    def test_mirror_conflict_preserves_original_and_remote_main(self):
        (self.source / "DREAM.md").write_text("session mirror\n")
        delivery.git(self.source, "add", "DREAM.md")
        delivery.git(self.source, "commit", "--amend", "--no-edit")
        original = delivery.git(self.source, "rev-parse", "HEAD").stdout.strip()
        (self.other / "DREAM.md").write_text("concurrent mirror\n")
        self.commit(self.other, "remote mirror")
        delivery.git(self.other, "push", "origin", "main")
        before = delivery.git(self.remote, "rev-parse", "main").stdout
        result = self.deliver()
        self.assertEqual(result["status"], "saved_for_recovery", result)
        self.assertEqual(delivery.git(self.remote, "rev-parse", result["recovery_ref"]).stdout.strip(), original)
        self.assertEqual(delivery.git(self.remote, "rev-parse", "main").stdout, before)
        self.assertEqual(delivery.git(self.source, "status", "--porcelain").stdout, "")

    def test_invalid_state_is_backed_up_but_never_published(self):
        self.append(self.source, [event("invalid", "state.ops_applied", [{"UpdateNode": {
            "id": "missing", "props": {"bad": True}}}])])
        delivery.git(self.source, "add", ".")
        delivery.git(self.source, "commit", "--amend", "--no-edit")
        result = self.deliver()
        self.assertEqual(result["status"], "saved_for_recovery", result)
        self.assertIn("validation failed", result["reason"])
        self.assertEqual(len(self.remote_rows()), 0)

    def test_malformed_envelope_is_backed_up_without_publishing(self):
        self.append(self.source, [[]])
        delivery.git(self.source, "add", ".")
        delivery.git(self.source, "commit", "--amend", "--no-edit")
        result = self.deliver()
        self.assertEqual(result["status"], "saved_for_recovery", result)
        self.assertIn("invalid GASP event envelope", result["reason"])
        self.assertEqual(len(self.remote_rows()), 0)

    def test_denied_main_can_recover_without_reexecuting_run(self):
        hook = self.remote / "hooks/pre-receive"
        hook.write_text('#!/bin/sh\nwhile read old new ref; do\n if [ "$ref" = refs/heads/main ]; then\n echo "permission denied on main" >&2; exit 1; fi\ndone\n')
        hook.chmod(0o755)
        result = self.deliver()
        self.assertEqual(result["status"], "saved_for_recovery", result)
        hook.unlink()
        self.assertEqual(self.deliver(recovery=True)["status"], "published")
        tip = delivery.git(self.remote, "rev-parse", "main").stdout
        self.assertEqual(self.deliver(recovery=True)["status"], "published")
        self.assertEqual(delivery.git(self.remote, "rev-parse", "main").stdout, tip)
        self.assertEqual(len(self.remote_rows()), 3)

    def test_all_remote_writes_denied_is_local_only(self):
        hook = self.remote / "hooks/pre-receive"
        hook.write_text('#!/bin/sh\necho "permission denied" >&2\nexit 1\n')
        hook.chmod(0o755)
        result = self.deliver()
        self.assertEqual(result["status"], "local_only", result)
        self.assertEqual(delivery.git(self.source, "rev-parse", result["local_ref"]).stdout.strip(), self.original)

    def test_reducer_version_is_pinned_to_runtime(self):
        import re
        lock = (ROOT / "Cargo.lock").read_text()
        version = re.search(r'name = "yoagent-state"\nversion = "([^"]+)"', lock).group(1)
        manifest = (ROOT / "scripts/gasp-delivery-validator/Cargo.toml").read_text()
        self.assertIn(f'yoagent-state = "={version}"', manifest)

    def test_published_legacy_skip_does_not_block_new_valid_run(self):
        self.append(self.other, [event("legacy", "state.ops_applied", [{"UpdateNode": {
            "id": "legacy_missing", "props": {"bad": True}}}])])
        self.commit(self.other, "legacy record")
        delivery.git(self.other, "push", "origin", "main")
        self.assertEqual(self.deliver()["status"], "published")

    def test_unreachable_remote_has_bounded_retries_and_local_anchor(self):
        real_git, fetches = delivery.git, []

        def unavailable(repo, *args, **kwargs):
            if args[0] in ("fetch", "push", "ls-remote"):
                if args[0] == "fetch":
                    fetches.append(args)
                return subprocess.CompletedProcess(args, 128, "", "could not resolve host")
            return real_git(repo, *args, **kwargs)

        with patch.object(delivery, "git", side_effect=unavailable):
            result = self.deliver()
        self.assertEqual(result["status"], "local_only")
        self.assertEqual(len(fetches), 6)
        self.assertEqual(delivery.git(self.source, "rev-parse", result["local_ref"]).stdout.strip(), self.original)

    def test_shim_exit_trap_does_not_repeat_session_end_after_failed_delivery(self):
        fake = self.root / "fake-emitter"
        count = self.root / "session-end-count"
        fake.write_text('#!/bin/sh\necho called >> "$GASP_TEST_COUNT"\n')
        fake.chmod(0o755)
        output = self.root / "step-output"
        shell = '''source scripts/gasp_shim.sh
gasp_mirror_memory() { :; }
GASP_ENABLED=true
GASP_STATE_DIR="$GASP_TEST_SOURCE"
GASP_RUN_ID=run_test
GASP_PUSH_URL="$GASP_TEST_REMOTE"
GASP_YOYO_BIN="$GASP_TEST_EMITTER"
GASP_FAIL_COUNTER="$GASP_TEST_COUNTER"
GASP_VALIDATOR_BIN=/nonexistent/validator
gasp_session_end done
gasp_session_end 'exit trap'
test "$GASP_DELIVERY_STATUS" = saved_for_recovery
'''
        env = dict(os.environ, GASP_TEST_COUNT=str(count), GASP_TEST_SOURCE=str(self.source),
                   GASP_TEST_REMOTE=str(self.remote), GASP_TEST_EMITTER=str(fake),
                   GASP_TEST_COUNTER=str(self.root / "counter"), GITHUB_OUTPUT=str(output))
        result = subprocess.run(["bash", "-c", shell], cwd=ROOT, env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(count.read_text(), "called\n")
        self.assertEqual(output.read_text(), "gasp_delivery=saved_for_recovery\n")

    def test_secret_redaction(self):
        with patch.dict(os.environ, {"GH_PAT": "secret-fixture", "GASP_PUSH_URL": "https://u:secret-fixture@example.invalid/repo"}):
            text = delivery.scrub("secret-fixture https://u:another-secret@example.invalid/repo")
        self.assertNotIn("secret-fixture", text)
        self.assertNotIn("another-secret", text)


if __name__ == "__main__":
    unittest.main()
