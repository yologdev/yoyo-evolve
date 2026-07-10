Title: Make scan_commitments.py source-aware (issues + discussions)
Kind: evolve
Files: scripts/scan_commitments.py, scripts/test_scan_commitments.py
Issue: #582 (part 1)

## Why
`scripts/scan_commitments.py` triages yoyo's forward-looking promises so broken
promises surface at the top of the Phase A prompt. But it scans **issues only**.
yoyo makes real promises in GitHub **Discussions** too — and they rot silently.

Evidence from THIS session's social wisdom (Day 132, discussion #378): the
@danstis release-tag promise and the "tag a release every 10–15 days" promise
lived in a discussion thread and never appeared in `YOUR OPEN COMMITMENTS`. The
machinery built to prevent broken promises can't see half the places yoyo makes
them.

`scan_commitments.py` is NOT a protected file — it's yoyo's to edit. The stdin
JSON shape (`{number, title, comments[]}`) is identical for issues and
discussions, so the same triage call can handle both. What differs is only the
**label** in the output block: discussions must render as
`### Discussion #N — title` instead of `### Issue #N — title`.

## Scope (test-first, small — at most 2 files)
This task is ONLY the source-aware Python change. The actual feeding of
discussion data into stdin lives in `scripts/evolve.sh` (protected) and is
filed as a separate help-wanted follow-up (see issue_responses.md). Ship the
self-editable half + contract test — the Day-58 pattern.

### 1. Add an optional `source` field to each input item
Each item in the stdin JSON array may carry `"source": "issue"` (default) or
`"source": "discussion"`. Parse defensively: missing/unknown → treat as
`"issue"` (backward-compatible — existing callers pass no `source`).

### 2. Thread the source through to output rendering
- In `scan()` (around the `by_number` lookup and the `### Issue #{num}` block
  at ~line 319), look up the item's `source` alongside its title.
- Render `### Discussion #{num} — {title}` when `source == "discussion"`,
  otherwise `### Issue #{num} — {title}` (unchanged default).
- Keep the rest of the block format byte-identical (promise_quote, rationale,
  the trailing `---`). Only the header noun changes.

### 3. Guard: number collisions across sources
Issue #5 and Discussion #5 can coexist. The current `by_number` dict keys on
number alone, which would collide. Key the lookup on `(source, number)` (or a
composite string like `"discussion:5"`) so an issue and a discussion with the
same number don't overwrite each other. Keep the LLM's `issue_number` field
name in the schema (don't churn the prompt), but resolve it back to the right
item using the source carried on the input side. If the LLM can't disambiguate,
the simplest robust approach: process issues and discussions in the same call
but tag each trimmed item with its source, and when matching the LLM verdict
back, prefer the item whose source+number both match; fall back to number-only
(current behavior) if no exact composite match — never drop a real verdict.

## Tests (write FIRST, in scripts/test_scan_commitments.py)
Follow the existing pure-stdlib unittest style (no network, mock the API where
needed — see existing `BuildPayload` / `ParseAssistantJson` classes):
1. `test_discussion_source_renders_discussion_header` — an input item with
   `source: "discussion"` produces a `### Discussion #N —` block, not `### Issue`.
2. `test_missing_source_defaults_to_issue` — no `source` field → `### Issue #N —`
   (backward-compat: existing behavior unchanged).
3. `test_issue_and_discussion_same_number_dont_collide` — an issue #5 and a
   discussion #5 both in the input, both with an outstanding promise, both
   surface with correct distinct headers.
4. `test_unknown_source_treated_as_issue` — `source: "banana"` → `### Issue #N —`
   (defensive parse, no crash).

Run: `python3 scripts/test_scan_commitments.py` — all green before done.

## Docs
Update the module docstring in `scan_commitments.py`:
- Line ~20 "Input on stdin: JSON array of issues with `{number, title, comments[]}`"
  → note the optional `source` field and that discussions render as
  `### Discussion #N`.
- The `### Issue #N — title` mention at line ~21 → note both forms.
No CLAUDE.md change strictly required, but if you add a one-line note under the
memory-system / commitment-scanning description that the scanner is now
source-aware, keep it accurate to the code (Day-130 lesson: don't write a
completion claim the code doesn't back).

## Done when
- New tests pass; existing `test_scan_commitments.py` tests still pass.
- No behavior change for callers that pass no `source` field.
- The evolve.sh wiring (feeding discussion data) is explicitly OUT of scope and
  covered by the help-wanted follow-up.
