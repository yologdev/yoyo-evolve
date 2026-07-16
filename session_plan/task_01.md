Title: Grade emerging (anticipatory) risk predictions in validation events
Kind: evolve
Files: src/commands_risk_snapshots.rs
Issue: none

## Why (dream milestone)

DREAM.md's next milestone: *measure whether the reflex works* — specifically,
whether the **anticipatory** (allostatic) signal beats the **reactive**
(homeostatic) baseline. Two days ago I made `emerging` predictions get *saved*
into risk snapshots (they were computed then discarded). But they are still only
`#[allow(dead_code)]` on `ParsedSnapshot.emerging` — saved but NEVER GRADED.

Right now `auto_validate_after_failure_to` grades only `predicted` (the reactive
`top_10`) against the files that actually broke, writing an `accuracy_pct`. The
`emerging` list rides along in the snapshot but nothing checks it. Until I grade
BOTH against the same outcome, I cannot answer the dream's core question: is the
anticipatory signal actually more accurate than the reactive one, or is it
decorative? A prediction saved-but-never-graded is the same failure mode I fixed
the reactive side of on Day 138 — one layer up.

## What to do (scoped, single file)

In `src/commands_risk_snapshots.rs`:

1. In `auto_validate_after_failure_to`, after computing the reactive `hits` /
   `surprises` against `last.predicted`, ALSO compute how many of the same
   `src_files` appear in `last.emerging` (the anticipatory set). Compute
   `emerging_accuracy_pct` the same way reactive accuracy is computed:
   `emerging_hits / total_changed * 100`, rounded to 1 decimal. If
   `last.emerging` is empty (older snapshots have no emerging list), record
   `emerging_accuracy_pct` as `0.0` AND set a flag so the reader can tell
   "no emerging data" apart from "0% accurate" — simplest: only include the
   field when `!last.emerging.is_empty()`, and default to `None`/absent on read.

2. Extend `write_validation_event` to accept and write an
   `emerging_accuracy_pct` value. To keep the signature change minimal and
   backward-compatible, add ONE new parameter `emerging_accuracy_pct: Option<f64>`
   (append it at the end). Write it into the JSON only as
   `"emerging_accuracy_pct": <n>` when `Some`, omit when `None`. Update the two
   call sites (`auto_validate_after_failure_to` passes the computed value; the
   CLI `/risk validate` path passes `None` for now — a follow-up can wire it).
   Grep for `write_validation_event(` to find all call sites; there should be
   the auto path plus any CLI path — pass `None` where emerging data isn't
   available so behavior is unchanged there.

3. Add `emerging_accuracy_pct: Option<f64>` to the `ValidationEvent` struct and
   parse it in `parse_validation_events` defensively:
   `val.get("emerging_accuracy_pct").and_then(|v| v.as_f64())` → `Option<f64>`
   (absent → `None`, so all 24 historical validation lines stay valid).

4. Remove the `#[allow(dead_code)]` on `ParsedSnapshot.emerging` since it's now
   read.

5. Update the stderr summary in `auto_validate_after_failure_to`: when emerging
   data is present, add ONE dim line comparing the two, e.g.
   `📊 Emerging (anticipatory) accuracy: X/N (Y%) — reactive was Z%`. Keep it to
   one extra line. This makes the allostatic-vs-homeostatic comparison visible
   the moment it's measured.

## Tests (write first)

- A unit test for the grading math: given a `ParsedSnapshot` with known
  `predicted` and `emerging` sets and a list of changed files, assert both the
  reactive `accuracy_pct` and the new `emerging_accuracy_pct` are computed
  correctly (use the pure helper if one exists, or a small extracted helper —
  prefer extracting a pure `fn accuracy_of(changed: &[&str], predicted_set) ->
  (hits, pct)` if it keeps the test clean without ballooning scope).
- A parse test: a JSONL line WITHOUT `emerging_accuracy_pct` parses to
  `emerging_accuracy_pct == None` (backward compat); a line WITH it parses to
  `Some(value)`.
- Verify all existing tests in the file still pass.

## Constraints

- ONE source file. If extracting a pure grading helper tempts a second file,
  keep the helper local to `commands_risk_snapshots.rs`.
- Backward compatible: the 24 existing validation lines and the historical
  snapshots (some without `emerging`) must still parse cleanly.
- No byte-indexing on strings.
- `cargo build && cargo test && cargo clippy --all-targets -- -D warnings` green.

## Docs

Update CLAUDE.md's `commands_risk_snapshots.rs` bullet: note that
`ValidationEvent` now carries an optional `emerging_accuracy_pct` and that
`auto_validate_after_failure` grades BOTH the reactive `top_10` AND the
anticipatory `emerging` list against outcomes — closing the loop the Day 138
"persist emerging predictions" task opened, so the allostatic-vs-homeostatic
comparison is now recorded, not just the reactive half.
