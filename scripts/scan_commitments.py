#!/usr/bin/env python3
"""Detect outstanding forward-looking commitments yoyo made on GitHub issues.

Each evolve.sh session, this script makes ONE batched call to Claude to
triage all open issues: which of yoyo's last-bot-comments are unfulfilled
commitments to act in a future session, and which have already been
satisfied by a recent git commit? Outstanding commitments are surfaced at
the top of the Phase A prompt so yoyo sees its broken promises before
choosing new work.

The call goes through yoyo itself (`--safe-mode --no-tools --max-turns 1`),
not a hand-rolled HTTP client, so it uses exactly the provider, model,
credential handling and retries every other loop call uses, read from the
same .yoyo.toml. Its own copy of that layer broke three times: a hardcoded
model 401'd (Jul 15-16), OAuth-token auth went unhandled (~Jul 24 to Aug 8),
and the scan returned 429 or 404 on every session from at least Aug 24 to
Sep 27 while yoyo's own calls with the same token succeeded. Still stdlib
only: no `pip install` step to fail silently in CI.

Usage (from evolve.sh):
    cat reply_issues.json | BOT_LOGIN=yoyo-evolve \\
        GIT_LOG_RECENT="$(git log ...)" \\
        python3 scripts/scan_commitments.py

Input on stdin: JSON array of issues with `{number, title, comments[]}`.
Output on stdout: zero or more `### Issue #N — title\n...\n---` blocks.

Exit codes:
  0 — ran cleanly (may have emitted zero blocks)
  2 — config or auth failure (missing BOT_LOGIN, yoyo binary not found, or
       yoyo reporting an auth error or an HTTP 400/401/403/404); the bash
       wrapper surfaces this as a louder banner so a broken cron does not
       silently lose commitment visibility for hours.
  3 — transient or unusable result (429, 5xx, network, timeout, no parseable
       JSON) after yoyo's own retries; the
       session continues without the commitment block, but the wrapper says
       "commitments UNKNOWN this session", never "No outstanding commitments"
       — exit 0 here made the harness assert a fact it never established
       (observed 2026-08-08: three 429s, then "No outstanding commitments").
"""

import json
import os
import re
import subprocess
import sys
import tempfile

# yoyo retries transient provider errors itself; this bounds the whole call.
YOYO_TIMEOUT_SECS = 600

# The system prompt is passed to yoyo with --system-file; volatile per-session
# data (issue bodies, git log) goes in the user message on stdin.
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

Only include issues that are TRULY outstanding commitments — skip
non-promises and skip fulfilled ones.

Respond with exactly one JSON object and nothing else — no prose, no code
fences — matching this JSON Schema:
"""

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


def _yoyo_bin():
    """YOYO_BIN if set (evolve.sh passes its fresh build), else the repo's debug
    build, else `yoyo` on PATH."""
    if os.environ.get("YOYO_BIN"):
        return os.environ["YOYO_BIN"]
    local = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "target", "debug", "yoyo")
    return local if os.path.exists(local) else "yoyo"


def _yoyo_argv(system_file):
    """One chat-only turn: no tools, no project context/skills/MCP (--safe-mode),
    response text only on stdout. Provider and model come from .yoyo.toml like
    every other loop call; MODEL overrides one run, as in the harness scripts."""
    argv = [
        _yoyo_bin(), "--safe-mode", "--no-tools", "--max-turns", "1",
        "--system-file", system_file, "--print", "--no-update-check",
    ]
    if os.environ.get("MODEL"):
        argv += ["--model", os.environ["MODEL"]]
    return argv


# yoyo's terminal failure line, e.g. `  error: API error: HTTP 429 ...` or
# `  error: Auth error: HTTP 401 ...` (ANSI colour optional).
_YOYO_ERROR_RE = re.compile(r"^(?:\x1b\[[0-9;]*m)?\s*error: (API|Auth) error: (.*)$", re.M)
_CONFIG_HTTP = {400, 401, 403, 404}


def _classify_failure(stderr):
    """(exit_code, message) for a failed yoyo call, or None if stderr carries no
    terminal provider error. Auth errors and 400/401/403/404 are config (2);
    429, 5xx and anything else are transient (3)."""
    m = _YOYO_ERROR_RE.search(stderr or "")
    if not m:
        return None
    kind, detail = m.group(1), m.group(2).strip()
    code = re.search(r"HTTP (\d{3})", detail)
    status = int(code.group(1)) if code else None
    exit_code = 2 if kind == "Auth" or status in _CONFIG_HTTP else 3
    return exit_code, f"{kind} error: {detail[:500]}"


def _call_yoyo(user_text):
    """Run one yoyo turn and return its response text. Never returns on
    failure: exits 2 (config) or 3 (transient/unusable), so a caller can treat
    a return value as a real answer. yoyo's exit status is not trusted on its
    own — it has been seen to exit 0 after an auth failure — so failure is
    read from its error line and from an empty response."""
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as f:
        f.write(SYSTEM_PROMPT + json.dumps(OUTPUT_SCHEMA))
        system_file = f.name
    try:
        proc = subprocess.run(
            _yoyo_argv(system_file), input=user_text, capture_output=True,
            text=True, timeout=YOYO_TIMEOUT_SECS,
        )
    except FileNotFoundError:
        _warn(f"yoyo binary not found ({_yoyo_bin()}); set YOYO_BIN")
        sys.exit(2)
    except subprocess.TimeoutExpired:
        _warn(f"yoyo timed out after {YOYO_TIMEOUT_SECS}s")
        sys.exit(3)
    finally:
        os.unlink(system_file)

    failure = _classify_failure(proc.stderr)
    if failure:
        exit_code, message = failure
        _warn(message)
        sys.exit(exit_code)
    if not proc.stdout.strip():
        _warn(f"yoyo returned no response (exit {proc.returncode}); stderr tail: {proc.stderr[-300:]!r}")
        sys.exit(3)
    return proc.stdout


def _parse_assistant_json(text):
    """Extract the JSON object from the model's response text. Tolerates a
    code fence or stray prose around it, since no schema is enforced
    server-side. Returns None if there is no parseable object."""
    text = (text or "").strip()
    start, end = text.find("{"), text.rfind("}")
    if start == -1 or end <= start:
        _warn("response had no JSON object")
        return None
    try:
        parsed = json.loads(text[start:end + 1])
    except json.JSONDecodeError as e:
        _warn(f"response was not valid JSON: {e}")
        return None
    return parsed if isinstance(parsed, dict) else None


def scan(issues, bot_login, git_log_recent):
    """Ask the configured model once, via yoyo, and return formatted commitment blocks."""
    trimmed_issues, git_log = _build_payload(issues, bot_login, git_log_recent)
    if not trimmed_issues:
        return []

    # JSON-as-string (not free-form markdown) so the model gets unambiguous
    # field boundaries and we can re-parse it in tests.
    user_payload = {
        "issues": trimmed_issues,
        "recent_commits": git_log,
    }

    # _call_yoyo exits the process on failure (2 config, 3 transient) —
    # reaching here means we have a response.
    response = _call_yoyo(json.dumps(user_payload, separators=(",", ":")))

    parsed = _parse_assistant_json(response)
    if parsed is None:
        # An answer we cannot read is "couldn't check", never "none found".
        sys.exit(3)

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
    # A missing BOT_LOGIN is a config regression, not a runtime condition —
    # exit non-zero so the bash wrapper surfaces it. Credentials, provider and
    # model are yoyo's to resolve.
    bot_login = os.environ.get("BOT_LOGIN", "")
    if not bot_login:
        _warn("BOT_LOGIN unset — workflow config regression?")
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

    blocks = scan(issues, bot_login, git_log)
    print("\n".join(blocks))


if __name__ == "__main__":
    main()
