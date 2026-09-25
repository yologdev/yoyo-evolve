#!/usr/bin/env python3
"""Deliver an existing GASP boundary commit; never execute or finish a run.

Exit 0: published. Exit 20: saved_for_recovery. Exit 21: local_only.
The sole stdout line is JSON. Credentials are read from the environment, never
written to a receipt, recovery branch, or status file. See docs/gasp-delivery.md.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import re
import subprocess
import tempfile
import time


class DeliveryError(Exception):
    pass


def scrub(value):
    for key in ("GASP_PUSH_URL", "GH_PAT", "GH_TOKEN", "GITHUB_TOKEN"):
        secret = os.environ.get(key)
        if secret:
            value = value.replace(secret, "[redacted]")
    return re.sub(r"https?://[^/\s@]+@", "https://[redacted]@", value)


def run(args, cwd, timeout=60):
    env = dict(os.environ, GIT_TERMINAL_PROMPT="0", LC_ALL="C")
    try:
        return subprocess.run(args, cwd=cwd, env=env, capture_output=True,
                              text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return subprocess.CompletedProcess(args, 124, "", "command timed out")


def git(repo, *args, check=True):
    result = run(["git", *args], repo)
    if check and result.returncode:
        raise DeliveryError(scrub(result.stderr[-2000:]))
    return result


def events(raw):
    rows = {}
    for line in raw.split("\n"):
        if not line.strip():
            continue
        event = json.loads(line)
        if not isinstance(event, dict):
            raise DeliveryError("invalid GASP event envelope")
        identity = event.get("id")
        if not isinstance(identity, str) or not identity or identity in rows:
            raise DeliveryError("missing or duplicate GASP event ID")
        rows[identity] = event
    return rows


def retryable(result):
    message = (result.stdout + result.stderr).lower()
    if any(x in message for x in ("authentication failed", "permission denied",
                                  "access denied", "repository not found", "protected branch", "403", "401")):
        return False
    return result.returncode == 124 or any(x in message for x in (
        "non-fast-forward", "fetch first", "timed out",
        "could not resolve", "couldn't connect", "connection reset", "connection refused",
        "remote end hung up", "502", "503", "504", "429", "unable to access"))


def deliver(repo, run_id, validator, attempts=5, delay=1.0, recovery=False):
    repo = Path(repo).resolve()
    remote = os.environ.get("GASP_PUSH_URL") or git(repo, "remote", "get-url", "origin").stdout.strip()
    key = hashlib.sha256(run_id.encode()).hexdigest()
    anchor = f"refs/gasp-delivery/{key}"
    recovery_ref = f"refs/heads/gasp-recovery/{key}"
    receipt_path = f"delivery/{key}.json"
    original = None
    reason = "delivery did not complete"
    status = "local_only"
    # Never modify the caller's checkout or rewrite its run commit.
    with tempfile.TemporaryDirectory(prefix="gasp-delivery-") as temp:
        work = Path(temp) / "work"
        try:
            if git(repo, "status", "--porcelain").stdout.strip():
                raise DeliveryError("source checkout is dirty; commit the completed run before delivery")
            if recovery:
                # Fetch into FETCH_HEAD; keep any prior local anchor unchanged.
                git(repo, "fetch", "--no-tags", remote, recovery_ref)
                original = git(repo, "rev-parse", "FETCH_HEAD").stdout.strip()
            else:
                original = git(repo, "rev-parse", "HEAD").stdout.strip()
            existing = git(repo, "rev-parse", "--verify", anchor, check=False)
            if existing.returncode == 0 and existing.stdout.strip() != original:
                original = None
                raise DeliveryError("run ID already has a different original commit; refusing to overwrite it")
            if existing.returncode:
                git(repo, "update-ref", anchor, original, "0" * 40)
            git(repo, "clone", "--quiet", "--shared", "--no-checkout", str(repo), str(work))
            git(work, "config", "user.name", "yoyo[gasp]")
            git(work, "config", "user.email", "yoyo-gasp@users.noreply.github.com")
            git(work, "checkout", "--quiet", "--detach", original)
            parents = git(work, "rev-list", "--parents", "-n", "1", original).stdout.split()
            if len(parents) != 2:
                raise DeliveryError("expected a single-parent boundary commit")
            base = parents[1]
            before = git(work, "show", f"{base}:state/events.jsonl").stdout
            after = git(work, "show", f"{original}:state/events.jsonl").stdout
            if not after.startswith(before):
                raise DeliveryError("boundary commit rewrites existing GASP events")
            additions = events(after[len(before):])
            for kind in ("run.started", "run.finished"):
                matches = [e for e in additions.values() if e.get("kind") == kind
                           and isinstance(e.get("payload"), dict)
                           and e.get("payload", {}).get("run_id") == run_id]
                if len(matches) != 1:
                    raise DeliveryError(f"boundary must contain exactly one {kind} for this run")
            receipt = {"version": 1, "run_id": run_id, "original_commit": original}

            def is_published(head):
                found = git(work, "show", f"{head}:{receipt_path}", check=False)
                if found.returncode == 0:
                    if json.loads(found.stdout) != receipt:
                        raise DeliveryError("remote receipt conflicts with this run identity")
                    remote_events = events(git(work, "show", f"{head}:state/events.jsonl").stdout)
                    if any(remote_events.get(k) != v for k, v in additions.items()):
                        raise DeliveryError("remote receipt exists but run events differ or are missing")
                    return True
                # Existing direct pushes (before this helper) also acknowledge
                # delivery when the original commit is literally in main.
                return git(work, "merge-base", "--is-ancestor", original, head, check=False).returncode == 0

            def validate(baseline):
                baseline_path = Path(temp) / "published-events.jsonl"
                baseline_path.write_text(baseline)
                result = run([str(Path(validator).resolve()), str(work), str(baseline_path)], work, timeout=120)
                if result.returncode:
                    raise DeliveryError("GASP validation failed: " + scrub(result.stderr[-1000:]))

            for attempt in range(attempts):
                if attempt:
                    time.sleep(delay * min(2 ** (attempt - 1), 8) + random.uniform(0, delay))
                fetched = git(work, "fetch", "--quiet", "--no-tags", remote, "refs/heads/main", check=False)
                if fetched.returncode:
                    reason = scrub(fetched.stderr[-1000:])
                    if retryable(fetched):
                        continue
                    break
                tip = git(work, "rev-parse", "FETCH_HEAD").stdout.strip()
                if is_published(tip):
                    status = "published"
                    reason = "remote main already contains this record"
                    break
                remote_events = events(git(work, "show", f"{tip}:state/events.jsonl").stdout)
                if additions.keys() & remote_events.keys():
                    raise DeliveryError("run events already present without a matching receipt; inspect before recovery")
                if git(work, "merge-base", "--is-ancestor", base, tip, check=False).returncode:
                    raise DeliveryError("remote main no longer contains the boundary's base")
                # Reapply only the immutable boundary, never earlier sessions.
                git(work, "checkout", "--quiet", "--detach", original)
                rebased = git(work, "rebase", "--onto", tip, base, check=False)
                if rebased.returncode:
                    git(work, "rebase", "--abort", check=False)
                    raise DeliveryError("merge conflict; original record preserved for recovery")
                remote_log = git(work, "show", f"{tip}:state/events.jsonl").stdout
                merged_log = (work / "state/events.jsonl").read_text()
                if not merged_log.startswith(remote_log):
                    raise DeliveryError("merge changed the published event prefix")
                merged_events = events(merged_log)
                if any(merged_events.get(k) != v for k, v in additions.items()):
                    raise DeliveryError("merge changed or lost original run events")
                validate(remote_log)
                path = work / receipt_path
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(json.dumps(receipt, sort_keys=True) + "\n")
                git(work, "add", "--", receipt_path)
                git(work, "commit", "--quiet", "--amend", "--no-edit")
                pushed = git(work, "push", "--quiet", remote, "HEAD:refs/heads/main", check=False)
                if pushed.returncode == 0:
                    status, reason = "published", "remote acknowledged the validated record"
                    break
                reason = scrub(pushed.stderr[-1000:])
                if not retryable(pushed):
                    break
            # A final failed/timeout push may have reached the server. Check
            # its receipt before reporting failure or creating a recovery ref.
            if status != "published":
                fetched = git(work, "fetch", "--quiet", "--no-tags", remote, "refs/heads/main", check=False)
                if fetched.returncode == 0 and is_published(git(work, "rev-parse", "FETCH_HEAD").stdout.strip()):
                    status, reason = "published", "verified publication after uncertain acknowledgement"
        except (DeliveryError, OSError, ValueError) as error:
            reason = scrub(str(error))

        if status != "published" and original:
            # Always save the ORIGINAL, even if validation or rebase failed.
            # No force: a different record under the same identity is an error.
            try:
                remote_ref = git(repo, "ls-remote", "--refs", remote, recovery_ref, check=False)
                existing_sha = remote_ref.stdout.split()[0] if remote_ref.stdout.split() else None
                if existing_sha and existing_sha != original:
                    reason += "; recovery ref already names a different commit"
                else:
                    if not existing_sha:
                        git(repo, "push", "--quiet", remote, f"{original}:{recovery_ref}", check=False)
                    verified = git(repo, "ls-remote", "--refs", remote, recovery_ref, check=False)
                    if verified.returncode == 0 and verified.stdout.split() and verified.stdout.split()[0] == original:
                        status = "saved_for_recovery"
            except (DeliveryError, OSError) as error:
                reason += "; recovery backup failed: " + scrub(str(error))

    result = {"status": status, "run_id": run_id, "original_commit": original,
              "local_ref": anchor if original else None,
              "recovery_ref": recovery_ref if status == "saved_for_recovery" else None,
              "reason": reason}
    # Store the result outside the tracked tree; no credentials or raw output.
    metadata = Path(git(repo, "rev-parse", "--absolute-git-dir").stdout.strip()) / "gasp-delivery"
    metadata.mkdir(exist_ok=True)
    temporary = metadata / f"{key}.{os.getpid()}.tmp"
    temporary.write_text(json.dumps(result, sort_keys=True) + "\n")
    os.replace(temporary, metadata / f"{key}.json")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("repo")
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--validator", required=True, help="GASP reducer validator executable")
    parser.add_argument("--recover", action="store_true", help="fetch the run's recovery ref before delivery")
    args = parser.parse_args()
    try:
        result = deliver(args.repo, args.run_id, args.validator, recovery=args.recover)
    except (DeliveryError, OSError, ValueError) as error:
        result = {"status": "local_only", "run_id": args.run_id, "reason": scrub(str(error))}
    print(json.dumps(result))
    return {"published": 0, "saved_for_recovery": 20, "local_only": 21}[result["status"]]


if __name__ == "__main__":
    raise SystemExit(main())
