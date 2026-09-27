#!/usr/bin/env python3
"""Tests for scripts/scan_commitments.py — the LLM-based commitment scanner.

Run via:  python3 scripts/test_scan_commitments.py
(Pure stdlib unittest — no pytest, no network. The yoyo call is either
mocked at `_call_yoyo` or run against a fake yoyo script via YOYO_BIN.)
"""

import io
import json
import os
import stat
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from scan_commitments import (  # noqa: E402
    _build_payload,
    _call_yoyo,
    _classify_failure,
    _parse_assistant_json,
    _yoyo_argv,
    scan,
)


BOT = "yoyo-evolve"


def _comment(author, body, ts="2026-06-01T00:00:00Z"):
    return {"author": {"login": author}, "body": body, "createdAt": ts}


def _issue(num, title, comments, source=None):
    item = {"number": num, "title": title, "comments": comments}
    if source is not None:
        item["source"] = source
    return item


class BuildPayload(unittest.TestCase):
    def test_skips_issues_with_no_bot_comment(self):
        issue = _issue(1, "X", [_comment("alice", "hi"), _comment("bob", "hello")])
        issues, _ = _build_payload([issue], BOT, "")
        self.assertEqual(issues, [])

    def test_skips_issues_with_no_comments(self):
        issue = _issue(1, "X", [])
        issues, _ = _build_payload([issue], BOT, "")
        self.assertEqual(issues, [])

    def test_finds_last_bot_comment(self):
        issue = _issue(
            418,
            "Test",
            [
                _comment("alice", "first human"),
                _comment(BOT, "first bot"),
                _comment("alice", "human reply"),
                _comment(BOT, "second bot — the latest"),
            ],
        )
        issues, _ = _build_payload([issue], BOT, "")
        self.assertEqual(len(issues), 1)
        self.assertEqual(issues[0]["number"], 418)
        self.assertEqual(issues[0]["last_bot_comment"]["body"], "second bot — the latest")

    def test_includes_up_to_two_prior_comments(self):
        issue = _issue(
            1,
            "X",
            [
                _comment("alice", "c0"),
                _comment("bob", "c1"),
                _comment("carol", "c2"),
                _comment("dave", "c3"),
                _comment(BOT, "bot last"),
            ],
        )
        issues, _ = _build_payload([issue], BOT, "")
        prior = issues[0]["prior_comments"]
        self.assertEqual(len(prior), 2)
        self.assertEqual(prior[0]["body"], "c2")
        self.assertEqual(prior[1]["body"], "c3")

    def test_truncates_long_bodies(self):
        long = "x" * 5000
        issue = _issue(1, "X", [_comment(BOT, long)])
        issues, _ = _build_payload([issue], BOT, "")
        body = issues[0]["last_bot_comment"]["body"]
        self.assertTrue(len(body) < len(long))
        self.assertTrue(body.endswith("…"))

    def test_truncates_long_git_log(self):
        log = "a" * 50000
        _, git_log = _build_payload([], BOT, log)
        self.assertEqual(len(git_log), 30000)


class ParseAssistantJson(unittest.TestCase):
    def test_extracts_json_object(self):
        parsed = _parse_assistant_json('{"outstanding_commitments": []}')
        self.assertEqual(parsed, {"outstanding_commitments": []})

    def test_tolerates_code_fence_and_prose(self):
        # No schema is enforced server-side any more, so wrapping must not
        # turn a real answer into "unreadable".
        text = 'Here you go:\n```json\n{"outstanding_commitments": []}\n```'
        self.assertEqual(_parse_assistant_json(text), {"outstanding_commitments": []})

    def test_returns_none_for_malformed_json(self):
        self.assertIsNone(_parse_assistant_json("{not json}"))

    def test_returns_none_for_no_json_object(self):
        self.assertIsNone(_parse_assistant_json("I could not decide."))

    def test_returns_none_for_empty_or_non_object(self):
        self.assertIsNone(_parse_assistant_json(""))
        self.assertIsNone(_parse_assistant_json(None))


class ScanIntegration(unittest.TestCase):
    """Tests scan() with the yoyo call mocked."""

    def _mock_response(self, outstanding):
        return json.dumps({"outstanding_commitments": outstanding})

    def test_empty_issues_skips_api_call(self):
        with patch("scan_commitments._call_yoyo") as mock_call:
            blocks = scan([], BOT, "")
            self.assertEqual(blocks, [])
            mock_call.assert_not_called()

    def test_issues_without_bot_comment_skip_api(self):
        issue = _issue(1, "X", [_comment("alice", "human only")])
        with patch("scan_commitments._call_yoyo") as mock_call:
            blocks = scan([issue], BOT, "")
            self.assertEqual(blocks, [])
            mock_call.assert_not_called()

    def test_renders_outstanding_block(self):
        issue = _issue(
            418,
            "Use ollama preset",
            [_comment(BOT, "Picking this up next session.")],
        )
        resp = self._mock_response([{
            "issue_number": 418,
            "promise_quote": "Picking this up next session.",
            "rationale": "No commit since references #418.",
        }])
        with patch("scan_commitments._call_yoyo", return_value=resp):
            blocks = scan([issue], BOT, "")
        self.assertEqual(len(blocks), 1)
        self.assertIn("#418", blocks[0])
        self.assertIn("Picking this up next session.", blocks[0])
        self.assertIn("UNFULFILLED", blocks[0])

    def test_no_outstanding_means_no_blocks(self):
        issue = _issue(418, "X", [_comment(BOT, "Done.")])
        resp = self._mock_response([])
        with patch("scan_commitments._call_yoyo", return_value=resp):
            blocks = scan([issue], BOT, "")
        self.assertEqual(blocks, [])

    def test_api_failure_exits_3_not_empty(self):
        # Contract (2026-08): an unusable API response must EXIT 3, never yield
        # an empty result — "couldn't check" must not reach the harness as
        # "checked, none found". _call_yoyo never returns on failure, so a
        # return_value=None mock would simulate an impossible state.
        issue = _issue(418, "X", [_comment(BOT, "Picking this up next session.")])
        with patch("scan_commitments._call_yoyo", side_effect=SystemExit(3)):
            with self.assertRaises(SystemExit) as cm:
                scan([issue], BOT, "")
        self.assertEqual(cm.exception.code, 3)

    def test_unknown_issue_number_in_response_is_skipped(self):
        # Defensive: LLM hallucinates an issue number we didn't pass in.
        issue = _issue(418, "X", [_comment(BOT, "Picking this up.")])
        resp = self._mock_response([
            {"issue_number": 999, "promise_quote": "?", "rationale": "?"},
            {"issue_number": 418, "promise_quote": "Picking this up.", "rationale": "ok"},
        ])
        with patch("scan_commitments._call_yoyo", return_value=resp):
            blocks = scan([issue], BOT, "")
        self.assertEqual(len(blocks), 1)
        self.assertIn("#418", blocks[0])

    def test_unreadable_answer_exits_3_not_empty(self):
        # Same contract for an answer with no parseable JSON: UNKNOWN, not zero.
        issue = _issue(418, "X", [_comment(BOT, "Picking this up next session.")])
        with patch("scan_commitments._call_yoyo", return_value="I am not sure."):
            with self.assertRaises(SystemExit) as cm:
                scan([issue], BOT, "")
        self.assertEqual(cm.exception.code, 3)

    def test_prompt_shape(self):
        """Pins what yoyo is sent: the user message is a JSON-encoded object
        carrying the issues and the git log."""
        captured = {}

        def capture(user_text):
            captured["text"] = user_text
            return '{"outstanding_commitments": []}'

        issue = _issue(1, "X", [_comment(BOT, "Picking this up.")])
        with patch("scan_commitments._call_yoyo", side_effect=capture):
            scan([issue], BOT, "git-log-text")
        inner = json.loads(captured["text"])
        self.assertIn("issues", inner)
        self.assertEqual(inner["recent_commits"], "git-log-text")


class SourceAwareness(unittest.TestCase):
    """Discussions and issues share the stdin shape; only the header noun
    differs. A `source` field ("issue" | "discussion") selects it, defaulting
    to "issue" for backward-compat callers that pass no source.
    """

    def _mock_response(self, outstanding):
        return json.dumps({"outstanding_commitments": outstanding})

    def test_discussion_source_renders_discussion_header(self):
        issue = _issue(
            37,
            "Tag a release every 10-15 days",
            [_comment(BOT, "I'll tag @danstis on the next release.")],
            source="discussion",
        )
        resp = self._mock_response([{
            "issue_number": 37,
            "promise_quote": "I'll tag @danstis on the next release.",
            "rationale": "No release tagged since.",
        }])
        with patch("scan_commitments._call_yoyo", return_value=resp):
            blocks = scan([issue], BOT, "")
        self.assertEqual(len(blocks), 1)
        self.assertIn("### Discussion #37 —", blocks[0])
        self.assertNotIn("### Issue #37", blocks[0])

    def test_missing_source_defaults_to_issue(self):
        issue = _issue(
            418,
            "Use ollama preset",
            [_comment(BOT, "Picking this up next session.")],
        )
        resp = self._mock_response([{
            "issue_number": 418,
            "promise_quote": "Picking this up next session.",
            "rationale": "No commit references #418.",
        }])
        with patch("scan_commitments._call_yoyo", return_value=resp):
            blocks = scan([issue], BOT, "")
        self.assertEqual(len(blocks), 1)
        self.assertIn("### Issue #418 —", blocks[0])
        self.assertNotIn("### Discussion #418", blocks[0])

    def test_issue_and_discussion_same_number_dont_collide(self):
        # Issue #5 and Discussion #5 can coexist; both have outstanding
        # promises and must surface with distinct, correct headers.
        the_issue = _issue(
            5,
            "Issue five title",
            [_comment(BOT, "Will fix the issue next cycle.")],
            source="issue",
        )
        the_discussion = _issue(
            5,
            "Discussion five title",
            [_comment(BOT, "Will ship the discussed feature next cycle.")],
            source="discussion",
        )
        resp = self._mock_response([
            {
                "issue_number": 5,
                "promise_quote": "Will fix the issue next cycle.",
                "rationale": "issue outstanding",
            },
            {
                "issue_number": 5,
                "promise_quote": "Will ship the discussed feature next cycle.",
                "rationale": "discussion outstanding",
            },
        ])
        with patch("scan_commitments._call_yoyo", return_value=resp):
            blocks = scan([the_issue, the_discussion], BOT, "")
        joined = "\n".join(blocks)
        self.assertIn("### Issue #5 — Issue five title", joined)
        self.assertIn("### Discussion #5 — Discussion five title", joined)
        # Neither block should carry the other source's title.
        self.assertNotIn("### Issue #5 — Discussion five title", joined)
        self.assertNotIn("### Discussion #5 — Issue five title", joined)

    def test_unknown_source_treated_as_issue(self):
        issue = _issue(
            9,
            "Weird source",
            [_comment(BOT, "Picking this up next session.")],
            source="banana",
        )
        resp = self._mock_response([{
            "issue_number": 9,
            "promise_quote": "Picking this up next session.",
            "rationale": "outstanding",
        }])
        with patch("scan_commitments._call_yoyo", return_value=resp):
            blocks = scan([issue], BOT, "")
        self.assertEqual(len(blocks), 1)
        self.assertIn("### Issue #9 —", blocks[0])


class PaginationLimitation(unittest.TestCase):
    """Pins the harness-fetch pagination contract raised on issue #589.

    The evolve.sh patch that feeds discussions into this scanner fetches with
    `discussions(first:N)` / `comments(first:M)` WITHOUT pagination. So the
    proving bot comment for a real commitment can fall past the first page —
    e.g. an early bot reply buried under a long thread of later human comments,
    or a recently-active-but-old discussion past the first N discussions.

    The scanner itself is correct: it triages whatever it's fed. But it can
    only triage what reaches it. These fixtures make that boundary an explicit,
    checked contract:

      * When the bot comment IS in the fed input, the scanner correctly
        surfaces the discussion → the scanner is not the failure point.
      * When the bot comment is ABSENT (simulating the un-paginated fetch
        having dropped it past `first:M`), the discussion is NOT surfaced →
        this is the exact silent false-negative the human named.

    CONTRACT: authoritative triage depends on the harness fetch PAGINATING
    comments (and discussions). A `first:N` fetch without pagination starves
    the scanner, and starved input produces a false negative even though the
    scanner is behaving correctly. When the evolve.sh patch is wired, it MUST
    page through comments or this proving bot comment never reaches stdin.
    """

    def _mock_response(self, outstanding):
        return json.dumps({"outstanding_commitments": outstanding})

    def _long_thread(self, include_bot_comment):
        """A long discussion thread whose proving bot comment is the FIRST
        comment (position 0) — i.e. it would fall past the *end* of an
        un-paginated `comments(first:M)` window once the thread grows beyond M,
        because such fetches take the newest/last page or a fixed prefix and
        this proving comment sits before it. When `include_bot_comment` is
        False we model the fetch having dropped it entirely.
        """
        thread = []
        if include_bot_comment:
            thread.append(_comment(BOT, "I'll ship the discussed feature next cycle."))
        # 60 later human comments — past a `first:50` page boundary.
        thread.extend(_comment("human%d" % i, "reply %d" % i) for i in range(60))
        return _issue(37, "Long-running discussion", thread, source="discussion")

    def test_bot_comment_present_is_triaged(self):
        """When the proving bot comment DOES reach the scanner (harness paged
        correctly), the discussion is surfaced as `### Discussion #37`.
        """
        issue = self._long_thread(include_bot_comment=True)
        # Sanity: _build_payload finds the bot comment regardless of thread
        # length — the scanner is correct once the comment is present.
        payload, _ = _build_payload([issue], BOT, "")
        self.assertEqual(len(payload), 1)
        self.assertEqual(
            payload[0]["last_bot_comment"]["body"],
            "I'll ship the discussed feature next cycle.",
        )
        resp = self._mock_response([{
            "issue_number": 37,
            "promise_quote": "I'll ship the discussed feature next cycle.",
            "rationale": "No commit ships the discussed feature since.",
        }])
        with patch("scan_commitments._call_yoyo", return_value=resp):
            blocks = scan([issue], BOT, "")
        self.assertEqual(len(blocks), 1)
        self.assertIn("### Discussion #37 —", blocks[0])

    def test_bot_comment_dropped_by_unpaginated_fetch_is_false_negative(self):
        """The exact failure mode the human named on #589: when the harness
        fetch drops the proving bot comment past its `first:M` window, the
        scanner never sees it and the commitment is silently missed — NOT
        because the scanner is wrong, but because it was starved.
        """
        issue = self._long_thread(include_bot_comment=False)
        # No bot comment reaches the payload → _build_payload skips it and the
        # scanner never calls the API. The commitment is silently invisible.
        payload, _ = _build_payload([issue], BOT, "")
        self.assertEqual(payload, [])
        with patch("scan_commitments._call_yoyo") as mock_call:
            blocks = scan([issue], BOT, "")
            mock_call.assert_not_called()
        self.assertEqual(blocks, [])


class YoyoArgv(unittest.TestCase):
    """The call is one chat-only turn that reads provider and model from
    .yoyo.toml, like every other loop call."""

    def test_one_bare_turn(self):
        with patch.dict(os.environ, {}, clear=False):
            os.environ.pop("MODEL", None)
            argv = _yoyo_argv("/tmp/sys.txt")
        for flag in ("--safe-mode", "--no-tools", "--print"):
            self.assertIn(flag, argv)
        self.assertEqual(argv[argv.index("--max-turns") + 1], "1")
        self.assertEqual(argv[argv.index("--system-file") + 1], "/tmp/sys.txt")
        # No model, provider or key of its own: yoyo resolves them.
        for flag in ("--model", "--provider", "--api-key"):
            self.assertNotIn(flag, argv)

    def test_model_env_overrides_one_run(self):
        with patch.dict(os.environ, {"MODEL": "some-model"}):
            argv = _yoyo_argv("/tmp/sys.txt")
        self.assertEqual(argv[argv.index("--model") + 1], "some-model")


class FailureClassification(unittest.TestCase):
    """Pins how a failed yoyo call maps to exit codes — the contract the
    evolve.sh wrapper reads (2 = loud config banner, 3 = UNKNOWN)."""

    RED, RESET = "\x1b[31m", "\x1b[0m"

    def test_auth_error_is_config(self):
        err = f"\n{self.RED}  error: Auth error: HTTP 401 Unauthorized: {{}}{self.RESET}\n"
        self.assertEqual(_classify_failure(err)[0], 2)

    def test_400_and_404_are_config(self):
        for code in (400, 404):
            err = f"  error: API error: HTTP {code} Bad: {{}}\n"
            self.assertEqual(_classify_failure(err)[0], 2, code)

    def test_429_and_5xx_are_transient(self):
        for code in (429, 503):
            err = f"  error: API error: HTTP {code} Busy: {{}}\n"
            self.assertEqual(_classify_failure(err)[0], 3, code)

    def test_message_carries_the_provider_error(self):
        # The old HTTP client discarded the 429 body; the reason must reach
        # the log now.
        err = '  error: API error: HTTP 429 Too Many Requests: {"message":"why"}\n'
        self.assertIn('"message":"why"', _classify_failure(err)[1])

    def test_no_error_line_is_not_a_failure(self):
        self.assertIsNone(_classify_failure("note: All tools disabled\n"))
        self.assertIsNone(_classify_failure(""))


class YoyoSubprocess(unittest.TestCase):
    """End to end through subprocess against a fake yoyo (YOYO_BIN), so the
    argv, stdin, stdout and exit-code handling are exercised for real."""

    def _fake_yoyo(self, script):
        d = tempfile.mkdtemp()
        path = os.path.join(d, "yoyo")
        with open(path, "w") as f:
            f.write("#!/bin/sh\n" + script)
        os.chmod(path, os.stat(path).st_mode | stat.S_IXUSR)
        return path

    def _run(self, script):
        env = {"YOYO_BIN": self._fake_yoyo(script)}
        with patch.dict(os.environ, env):
            os.environ.pop("MODEL", None)
            return _call_yoyo('{"issues": []}')

    def test_success_returns_stdout(self):
        out = self._run('cat >/dev/null; echo \'{"outstanding_commitments": []}\'\n')
        self.assertEqual(json.loads(out), {"outstanding_commitments": []})

    def test_receives_payload_and_system_file(self):
        # The fake echoes back what it got so the test sees the real wiring.
        out = self._run(
            'while [ "$1" != "--system-file" ]; do shift; done; sys="$2"\n'
            'payload=$(cat)\n'
            'grep -q "outstanding_commitments" "$sys" && echo "{\\"ok\\": \\"$payload\\"}"\n'
        )
        self.assertIn("issues", out)

    def test_auth_error_with_exit_0_still_fails_as_config(self):
        # yoyo has been seen to exit 0 after an auth failure; the error line
        # must decide, not the exit status.
        with self.assertRaises(SystemExit) as cm:
            self._run('cat >/dev/null; echo "  error: Auth error: HTTP 401 Unauthorized: {}" >&2; exit 0\n')
        self.assertEqual(cm.exception.code, 2)

    def test_rate_limit_is_transient(self):
        with self.assertRaises(SystemExit) as cm:
            self._run('cat >/dev/null; echo "  error: API error: HTTP 429 Too Many Requests: {}" >&2; exit 1\n')
        self.assertEqual(cm.exception.code, 3)

    def test_empty_response_is_unknown_not_zero(self):
        with self.assertRaises(SystemExit) as cm:
            self._run("cat >/dev/null; exit 0\n")
        self.assertEqual(cm.exception.code, 3)

    def test_missing_binary_is_config(self):
        with patch.dict(os.environ, {"YOYO_BIN": "/nonexistent/yoyo"}):
            with self.assertRaises(SystemExit) as cm:
                _call_yoyo("{}")
        self.assertEqual(cm.exception.code, 2)


class MainConfig(unittest.TestCase):
    def test_missing_bot_login_is_config(self):
        import scan_commitments

        with patch.dict(os.environ, {"BOT_LOGIN": ""}), \
             patch("scan_commitments._call_yoyo") as call, \
             patch("sys.stdin", io.StringIO("[]")):
            with self.assertRaises(SystemExit) as cm:
                scan_commitments.main()
        self.assertEqual(cm.exception.code, 2)
        call.assert_not_called()

    def test_no_provider_or_key_gate(self):
        # Near-miss guard: with BOT_LOGIN set and no API key in the env, main()
        # proceeds (yoyo owns credentials); empty stdin → clean exit 0.
        import scan_commitments

        env = {"BOT_LOGIN": "yoyo-evolve[bot]"}
        with patch.dict(os.environ, env), \
             patch("sys.stdin", io.StringIO("[]")):
            os.environ.pop("ANTHROPIC_API_KEY", None)
            scan_commitments.main()


if __name__ == "__main__":
    unittest.main()
