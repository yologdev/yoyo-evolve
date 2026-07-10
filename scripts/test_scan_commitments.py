#!/usr/bin/env python3
"""Tests for scripts/scan_commitments.py — the LLM-based commitment scanner.

Run via:  python3 scripts/test_scan_commitments.py
(Pure stdlib unittest — no pytest, no anthropic SDK, no network.)
"""

import io
import json
import os
import sys
import unittest
import urllib.error
from unittest.mock import patch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from scan_commitments import (  # noqa: E402
    MAX_RETRIES,
    MODEL,
    _build_payload,
    _call_api_with_retries,
    _parse_assistant_json,
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
    def test_extracts_first_text_block(self):
        resp = {"content": [{"type": "text", "text": '{"outstanding_commitments": []}'}]}
        parsed = _parse_assistant_json(resp)
        self.assertEqual(parsed, {"outstanding_commitments": []})

    def test_returns_none_for_malformed_json(self):
        resp = {"content": [{"type": "text", "text": "not json"}]}
        self.assertIsNone(_parse_assistant_json(resp))

    def test_returns_none_for_no_text_block(self):
        resp = {"content": [{"type": "image", "source": {}}]}
        self.assertIsNone(_parse_assistant_json(resp))

    def test_returns_none_for_empty_content(self):
        self.assertIsNone(_parse_assistant_json({"content": []}))
        self.assertIsNone(_parse_assistant_json({}))


class ScanIntegration(unittest.TestCase):
    """Tests scan() with the urllib call mocked."""

    def _mock_response(self, outstanding):
        text = json.dumps({"outstanding_commitments": outstanding})
        return {"content": [{"type": "text", "text": text}]}

    def test_empty_issues_skips_api_call(self):
        with patch("scan_commitments._call_api_with_retries") as mock_call:
            blocks = scan([], BOT, "", api_key="sk-fake")
            self.assertEqual(blocks, [])
            mock_call.assert_not_called()

    def test_issues_without_bot_comment_skip_api(self):
        issue = _issue(1, "X", [_comment("alice", "human only")])
        with patch("scan_commitments._call_api_with_retries") as mock_call:
            blocks = scan([issue], BOT, "", api_key="sk-fake")
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
        with patch("scan_commitments._call_api_with_retries", return_value=resp):
            blocks = scan([issue], BOT, "", api_key="sk-fake")
        self.assertEqual(len(blocks), 1)
        self.assertIn("#418", blocks[0])
        self.assertIn("Picking this up next session.", blocks[0])
        self.assertIn("UNFULFILLED", blocks[0])

    def test_no_outstanding_means_no_blocks(self):
        issue = _issue(418, "X", [_comment(BOT, "Done.")])
        resp = self._mock_response([])
        with patch("scan_commitments._call_api_with_retries", return_value=resp):
            blocks = scan([issue], BOT, "", api_key="sk-fake")
        self.assertEqual(blocks, [])

    def test_api_failure_returns_empty(self):
        issue = _issue(418, "X", [_comment(BOT, "Picking this up next session.")])
        with patch("scan_commitments._call_api_with_retries", return_value=None):
            blocks = scan([issue], BOT, "", api_key="sk-fake")
        self.assertEqual(blocks, [])

    def test_unknown_issue_number_in_response_is_skipped(self):
        # Defensive: LLM hallucinates an issue number we didn't pass in.
        issue = _issue(418, "X", [_comment(BOT, "Picking this up.")])
        resp = self._mock_response([
            {"issue_number": 999, "promise_quote": "?", "rationale": "?"},
            {"issue_number": 418, "promise_quote": "Picking this up.", "rationale": "ok"},
        ])
        with patch("scan_commitments._call_api_with_retries", return_value=resp):
            blocks = scan([issue], BOT, "", api_key="sk-fake")
        self.assertEqual(len(blocks), 1)
        self.assertIn("#418", blocks[0])

    def test_request_body_shape(self):
        """Pins the wire format so a refactor that renames `output_config`,
        drops `cache_control`, or moves `system` to a string triggers a test
        failure here instead of an API 400 in production.
        """
        captured = {}

        def capture(api_key, body_bytes):
            captured["body"] = json.loads(body_bytes)
            return {"content": [{"type": "text", "text": '{"outstanding_commitments": []}'}]}

        issue = _issue(1, "X", [_comment(BOT, "Picking this up.")])
        with patch("scan_commitments._call_api_with_retries", side_effect=capture):
            scan([issue], BOT, "git-log-text", api_key="sk-fake")

        body = captured["body"]
        self.assertEqual(body["model"], MODEL)
        self.assertIn("max_tokens", body)
        self.assertEqual(body["system"][0]["cache_control"], {"type": "ephemeral"})
        self.assertEqual(body["output_config"]["format"]["type"], "json_schema")
        self.assertEqual(body["messages"][0]["role"], "user")
        # User content must be a JSON-encoded string (not a list of blocks).
        inner = json.loads(body["messages"][0]["content"])
        self.assertIn("issues", inner)
        self.assertIn("recent_commits", inner)


class SourceAwareness(unittest.TestCase):
    """Discussions and issues share the stdin shape; only the header noun
    differs. A `source` field ("issue" | "discussion") selects it, defaulting
    to "issue" for backward-compat callers that pass no source.
    """

    def _mock_response(self, outstanding):
        text = json.dumps({"outstanding_commitments": outstanding})
        return {"content": [{"type": "text", "text": text}]}

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
        with patch("scan_commitments._call_api_with_retries", return_value=resp):
            blocks = scan([issue], BOT, "", api_key="sk-fake")
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
        with patch("scan_commitments._call_api_with_retries", return_value=resp):
            blocks = scan([issue], BOT, "", api_key="sk-fake")
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
        with patch("scan_commitments._call_api_with_retries", return_value=resp):
            blocks = scan([the_issue, the_discussion], BOT, "", api_key="sk-fake")
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
        with patch("scan_commitments._call_api_with_retries", return_value=resp):
            blocks = scan([issue], BOT, "", api_key="sk-fake")
        self.assertEqual(len(blocks), 1)
        self.assertIn("### Issue #9 —", blocks[0])


class RetryPolicy(unittest.TestCase):
    """Pins the retry classifier in _call_api_with_retries — the riskiest
    untested code in the script. A regression that retries on 401, or stops
    retrying on 5xx, would silently break the cron without these.
    """

    def _http_error(self, code):
        return urllib.error.HTTPError(
            "https://api.anthropic.com/v1/messages",
            code, "x", {}, io.BytesIO(b"{}")
        )

    def test_401_is_fatal_no_retry(self):
        with patch("scan_commitments._post", side_effect=self._http_error(401)) as p, \
             patch("scan_commitments.time.sleep"):
            with self.assertRaises(SystemExit) as cm:
                _call_api_with_retries("sk-fake", b"{}")
            self.assertEqual(cm.exception.code, 2)
            self.assertEqual(p.call_count, 1)  # no retry

    def test_400_is_fatal_no_retry(self):
        with patch("scan_commitments._post", side_effect=self._http_error(400)) as p, \
             patch("scan_commitments.time.sleep"):
            with self.assertRaises(SystemExit) as cm:
                _call_api_with_retries("sk-fake", b"{}")
            self.assertEqual(cm.exception.code, 2)
            self.assertEqual(p.call_count, 1)

    def test_429_retries_then_gives_up(self):
        with patch("scan_commitments._post", side_effect=self._http_error(429)) as p, \
             patch("scan_commitments.time.sleep"):
            self.assertIsNone(_call_api_with_retries("sk-fake", b"{}"))
            self.assertEqual(p.call_count, MAX_RETRIES)

    def test_503_then_success(self):
        responses = [self._http_error(503), {"content": []}]
        with patch("scan_commitments._post", side_effect=responses), \
             patch("scan_commitments.time.sleep"):
            result = _call_api_with_retries("sk-fake", b"{}")
            self.assertEqual(result, {"content": []})


if __name__ == "__main__":
    unittest.main()
