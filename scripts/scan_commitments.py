#!/usr/bin/env python3
"""Detect outstanding forward-looking commitments yoyo made on GitHub issues.

Each evolve.sh session, this script makes ONE batched call to Claude to
triage all open issues: which of yoyo's last-bot-comments are unfulfilled
commitments to act in a future session, and which have already been
satisfied by a recent git commit? Outstanding commitments are surfaced at
the top of the Phase A prompt so yoyo sees its broken promises before
choosing new work.

Uses urllib only — no third-party dependencies — because evolve.sh runs in
GitHub Actions where reaching for `pip install anthropic` would add another
step that can fail silently.

Usage (from evolve.sh):
    cat reply_issues.json | BOT_LOGIN=yoyo-evolve \\
        GIT_LOG_RECENT="$(git log ...)" \\
        python3 scripts/scan_commitments.py

Input on stdin: JSON array of issues with `{number, title, comments[]}`.
Output on stdout: zero or more `### Issue #N — title\n...\n---` blocks.

Exit codes:
  0 — ran cleanly (may have emitted zero blocks)
  2 — config or auth failure (missing key, missing BOT_LOGIN, 401/403/400);
       the bash wrapper surfaces this as a louder banner so a broken cron
       does not silently lose commitment visibility for hours.

Transient failures (429, 5xx, network, timeout) stay on the silent-fail-soft
path: warn to stderr, retry with backoff, then return empty. The session
continues without the commitment block.
"""

import json
import os
import sys
import time
import urllib.error
import urllib.request

API_URL = "https://api.anthropic.com/v1/messages"
API_VERSION = "2023-06-01"
# Follow the fleet-wide MODEL contract (the same secret every loop reads)
# instead of hardcoding a model the key may not be scoped for — the
# hardcoded claude-opus-4-6 401'd every session for two days (Jul 15-16)
# while the fleet model worked fine in the same runs.
MODEL = os.environ.get("MODEL", "claude-opus-4-6")
MAX_TOKENS = 4096
TIMEOUT_SECS = 60
MAX_RETRIES = 3
RETRY_BASE_DELAY = 2.0  # seconds; doubled each attempt

# Static across sessions, marked ephemeral so it becomes a cacheable prefix
# once it crosses the model's minimum cacheable token count (~1024 for Opus).
# At current length it may fall below the threshold; `cache_control` is a
# forward-compatible no-op if so. Volatile per-session data (issue bodies,
# git log) goes in the user message, after this prefix.
SYSTEM_PROMPT = """\
You are a triage assistant for an autonomous coding agent named yoyo.

yoyo runs hourly on a GitHub repository. Each cycle it picks issues to work
on, comments on them, and ships code. Sometimes yoyo's comment is a
forward-looking commitment — "Picking this up next session", "I'll
implement this", "On it" — that promises future action.

You are given:
1. A list of open GitHub issues. Each issue's `last_bot_comment` is yoyo's
   most recent comment on that issue. Older comments may also be included
   for context.
2. The subjects and bodies of recent git commits (last 30 days).

For each issue, decide:

A) IS yoyo's last_bot_comment a forward-looking commitment to act in a
   future session? A commitment requires:
   - A clear statement of intent to do specific work, by yoyo, in a future
     session (not the current one).
   - NOT just acknowledging the issue, asking for clarification, reporting
     completed work, or general musing.

   Examples of commitments:
     "Picking this up next session."
     "I'll add the missing test in the next cycle."
     "Will implement the fix once the upstream lands."

   Examples that are NOT commitments:
     "Done — landed in commit abc123."
     "Thanks for reporting; closing as wontfix."
     "I'll be honest, this is tricky."  (rhetorical "I'll")
     "Looking into it now."  (current session, not future)
     "Could you clarify what you mean by X?"

B) IF it is a commitment, has it been fulfilled by any of the recent git
   commits? A commit fulfills a commitment when its subject or body shows
   the promised work has shipped — typically by referencing the issue
   number (`#N` or `issue N`) or by clearly describing the same change the
   commitment promised. Be conservative: prefer false (unfulfilled) when
   uncertain.

Return your judgment as structured JSON matching the provided schema. Only
include issues that are TRULY outstanding commitments — skip non-promises
and skip fulfilled ones."""

OUTPUT_SCHEMA = {
    "type": "object",
    "properties": {
        "outstanding_commitments": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "issue_number": {"type": "integer"},
                    "promise_quote": {
                        "type": "string",
                        "description": "The exact substring of yoyo's last_bot_comment that constitutes the commitment.",
                    },
                    "rationale": {
                        "type": "string",
                        "description": "One sentence on why this is outstanding (a commitment with no fulfilling commit).",
                    },
                },
                "required": ["issue_number", "promise_quote", "rationale"],
                "additionalProperties": False,
            },
        }
    },
    "required": ["outstanding_commitments"],
    "additionalProperties": False,
}


def _warn(msg):
    print(f"scan_commitments: {msg}", file=sys.stderr)


def _build_payload(issues, bot_login, git_log_recent):
    """Trim each issue to its last bot comment + prior 2 comments (for context),
    capping bodies at 1500 chars to bound token spend. Returns (issues, git_log).
    """
    trimmed_issues = []
    for issue in issues:
        comments = issue.get("comments", []) or []
        if not comments:
            continue
        last_bot_idx = -1
        for i, c in enumerate(comments):
            if (c.get("author") or {}).get("login", "") == bot_login:
                last_bot_idx = i
        if last_bot_idx == -1:
            continue
        last_bot = comments[last_bot_idx]
        prior = comments[max(0, last_bot_idx - 2):last_bot_idx]

        def trim(c):
            body = (c.get("body") or "")
            if len(body) > 1500:
                body = body[:1500] + "…"
            return {
                "author": (c.get("author") or {}).get("login", "unknown"),
                "created_at": c.get("createdAt", ""),
                "body": body,
            }

        # Source distinguishes issues from discussions (same stdin shape;
        # only the rendered header noun differs). Default/unknown → "issue"
        # so existing callers that pass no `source` are unchanged.
        source = issue.get("source", "issue")
        if source != "discussion":
            source = "issue"

        trimmed_issues.append({
            "number": issue.get("number"),
            "title": issue.get("title", ""),
            "source": source,
            "prior_comments": [trim(c) for c in prior],
            "last_bot_comment": trim(last_bot),
        })

    # Cap git log at ~30KB to keep the call cheap.
    git_log = (git_log_recent or "")[:30000]

    return trimmed_issues, git_log


def _auth_headers(api_key):
    """Auth headers matching yoagent's provider logic (anthropic.rs:119):
    an OAuth access token (sk-ant-oat...) must go on `Authorization: Bearer`
    with the oauth beta header — sent as x-api-key it 401s instantly, which
    is exactly how this scan silently died every session after the repo
    secret became an OAuth token (~Jul 24): the binary auto-detected the
    token type, this script didn't. (The Jul 16 fix 2b37bf9b blamed the
    model string; the auth path was the real difference.)"""
    if "sk-ant-oat" in api_key:
        return {
            "authorization": f"Bearer {api_key}",
            "anthropic-beta": "oauth-2025-04-20",
        }
    return {"x-api-key": api_key}


def _post(api_key, body_bytes):
    """POST to the Messages API, returning the parsed JSON body."""
    headers = {
        "Content-Type": "application/json",
        "anthropic-version": API_VERSION,
    }
    headers.update(_auth_headers(api_key))
    req = urllib.request.Request(
        API_URL,
        data=body_bytes,
        headers=headers,
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=TIMEOUT_SECS) as resp:
        return json.loads(resp.read().decode("utf-8"))


def _call_api_with_retries(api_key, body_bytes):
    """Call the API with exponential backoff on transient failures.

    Returns the parsed response on success, or None on transient failure
    (caller treats this as silent fail-soft). On auth/config/request-shape
    errors (401, 403, 400) this calls `sys.exit(2)` directly — those are
    config regressions, not runtime conditions, and must surface loudly.
    """
    last_err = None
    for attempt in range(MAX_RETRIES):
        try:
            return _post(api_key, body_bytes)
        except urllib.error.HTTPError as e:
            # Retry on 429 and 5xx; everything else is fatal.
            if e.code == 429 or e.code >= 500:
                last_err = f"HTTP {e.code}: {e.reason}"
            else:
                try:
                    detail = e.read().decode("utf-8")[:500]
                except (OSError, UnicodeDecodeError):
                    detail = ""
                # 401/403/400 are config bugs (revoked key, lost permission,
                # request-shape drift). Exit non-zero so evolve.sh surfaces it
                # rather than letting the cron drift silently for hours.
                if e.code in (401, 403, 400):
                    _warn(f"HTTP {e.code} {e.reason} — config/auth failure; {detail}")
                    sys.exit(2)
                _warn(f"HTTP {e.code} {e.reason} (no retry); {detail}")
                return None
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            last_err = f"network/timeout: {e}"
        except json.JSONDecodeError as e:
            last_err = f"invalid JSON response: {e}"

        if attempt < MAX_RETRIES - 1:
            delay = RETRY_BASE_DELAY * (2 ** attempt)
            _warn(f"attempt {attempt + 1}/{MAX_RETRIES} failed ({last_err}); retry in {delay:.0f}s")
            time.sleep(delay)

    _warn(f"all {MAX_RETRIES} attempts failed: {last_err}")
    return None


def _parse_assistant_json(response):
    """Extract the structured JSON from the assistant's first text block."""
    content = response.get("content") or []
    for block in content:
        if block.get("type") == "text":
            text = block.get("text", "")
            try:
                return json.loads(text)
            except json.JSONDecodeError as e:
                _warn(f"assistant text was not valid JSON: {e}")
                return None
    _warn("response had no text block")
    return None


def scan(issues, bot_login, git_log_recent, api_key):
    """Call Claude once and return formatted commitment blocks."""
    trimmed_issues, git_log = _build_payload(issues, bot_login, git_log_recent)
    if not trimmed_issues:
        return []

    # JSON-as-string (not free-form markdown) so the model gets unambiguous
    # field boundaries and we can re-parse it in tests.
    user_payload = {
        "issues": trimmed_issues,
        "recent_commits": git_log,
    }

    request_body = {
        "model": MODEL,
        "max_tokens": MAX_TOKENS,
        "system": [
            {
                "type": "text",
                "text": SYSTEM_PROMPT,
                "cache_control": {"type": "ephemeral"},
            }
        ],
        "messages": [
            {
                "role": "user",
                "content": json.dumps(user_payload, separators=(",", ":")),
            }
        ],
        "output_config": {
            "format": {
                "type": "json_schema",
                "schema": OUTPUT_SCHEMA,
            }
        },
    }

    body_bytes = json.dumps(request_body).encode("utf-8")
    response = _call_api_with_retries(api_key, body_bytes)
    if response is None:
        return []

    parsed = _parse_assistant_json(response)
    if parsed is None:
        return []

    items = parsed.get("outstanding_commitments") or []
    if not isinstance(items, list):
        _warn("`outstanding_commitments` was not a list")
        return []

    # Build a lookup so we can render title alongside the LLM's verdict.
    # Issues and discussions can share a number (issue #5 vs discussion #5),
    # so the value is a LIST of items keyed on number. The LLM verdict only
    # carries `issue_number` (we don't churn the prompt schema), so we match
    # each verdict back to the next unconsumed item with that number, in
    # input order — this keeps distinct-source same-number items from
    # overwriting each other.
    by_number = {}
    for i in trimmed_issues:
        by_number.setdefault(i.get("number"), []).append(i)

    blocks = []
    for item in items:
        num = item.get("issue_number")
        if num is None:
            _warn(f"item missing issue_number: {item!r}")
            continue
        candidates = by_number.get(num)
        if not candidates:
            # LLM hallucinated an issue number not in our input, or every
            # item with this number was already consumed by an earlier
            # verdict. Drop it (we can't render it) but warn — repeated
            # hallucinations are a signal the prompt or model is misbehaving.
            _warn(f"LLM returned unknown issue #{num} (not in input); dropping")
            continue
        promise = (item.get("promise_quote") or "").strip()
        # When multiple items share a number (issue #5 + discussion #5), the
        # verdict's `issue_number` alone is ambiguous. Disambiguate on the
        # promise_quote — it's lifted verbatim from one specific item's
        # comment — and fall back to input order if no text match is found.
        matched = None
        if len(candidates) > 1 and promise:
            for idx, cand in enumerate(candidates):
                if promise in (cand.get("last_bot_comment") or {}).get("body", ""):
                    matched = candidates.pop(idx)
                    break
        if matched is None:
            matched = candidates.pop(0)
        source = matched.get("source", "issue")
        noun = "Discussion" if source == "discussion" else "Issue"
        title = matched.get("title", "(no title)")
        rationale = (item.get("rationale") or "").strip()
        if len(promise) > 200:
            promise = promise[:200] + "…"
        blocks.append(
            f"### {noun} #{num} — {title}\n"
            f'You said: "{promise}"\n'
            f"Why outstanding: {rationale}\n"
            f"**Status: UNFULFILLED.**\n"
            f"---"
        )
    return blocks


def main():
    # Missing BOT_LOGIN / ANTHROPIC_API_KEY are config regressions, not
    # runtime conditions — exit non-zero so the bash wrapper surfaces them.
    bot_login = os.environ.get("BOT_LOGIN", "")
    if not bot_login:
        _warn("BOT_LOGIN unset — workflow config regression?")
        sys.exit(2)
    api_key = os.environ.get("ANTHROPIC_API_KEY", "")
    if not api_key:
        _warn("ANTHROPIC_API_KEY unset — workflow config regression?")
        sys.exit(2)

    git_log = os.environ.get("GIT_LOG_RECENT", "")

    raw = sys.stdin.read().strip()
    if not raw:
        # No issues piped in (e.g., gh returned []) — clean exit, nothing to do.
        return
    try:
        issues = json.loads(raw)
    except json.JSONDecodeError as e:
        _warn(f"invalid JSON on stdin: {e}; first 200 chars: {raw[:200]!r}")
        return
    if not isinstance(issues, list):
        _warn("stdin was not a JSON array")
        return

    blocks = scan(issues, bot_login, git_log, api_key)
    print("\n".join(blocks))


if __name__ == "__main__":
    main()
