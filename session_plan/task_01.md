Title: The unhittable count records its numerator and throws away its denominator — a recorded `unhittable_surprises: 4` cannot say whether it was 4 of 5 or 4 of 144
Kind: evolve
Files: src/commands_risk_unhittable.rs, src/commands_risk_snapshots.rs, src/commands_risk_unhittable_tests.rs
Issue: none (DREAM.md cycle 12 — the `unhittable` milestone, second half)

## Why this is the dream's next step and not a new thread

DREAM.md's milestone ("give the risk ledger a way to say *unhittable* out loud") now has a live
clause and a retrospective reader, both landed Days 206–210. What neither has is a **record that
can be read later without re-running the join**. ARCHITECTURE.md's `commands_risk_unhittable.rs`
entry states the module's own measured cost in those words: over the 116 post-ledger events there
are 144 surprises, **9 decidable (4 unhittable, 5 hittable) and 135 unmeasurable**, "because the
ledger's 97 paths are 95 founding-batch rows sharing one backfill instant and only 5 carry a real
birthday. That ratio is the honest cost of the fallback, not a bug in it." The dream's own
sentence is the point of the whole module: *a detector reports the leaks it happens to trigger and
certifies nothing by its silence.* A recorded `unhittable_surprises = 4` **is** that silence, one
layer down: it is a numerator with no denominator, so a future reader cannot tell a clean 4-of-5
from an absorbed 4-of-144, and the 135/144 ratio that makes today's reading honest is nowhere in
the record.

Measured at HEAD by this planning session (read-only, over data files, not source):

- `.yoyo/risk_validations.jsonl` holds **278** lines; `grep -c unhittable_surprises` → **2**.
- The last three events are all `severity: watch_success` and carry **no** `unhittable_surprises`
  key at all — the green path passes `None` deliberately (documented: it has `snapshot_git_hash`
  but no snapshot `ts`), so a green event's silence is by design and stays that way.
- So the field is written on the failure-day / CLI-red path only, and where it *is* written its
  denominator is not.

## What to do (keep it this small; do not widen)

1. **One new optional field on the event**: `unmeasurable_surprises: Option<u32>`, written at the
   **same** two sites that already write `unhittable_surprises` (`auto_validate_after_failure_to`
   in `src/commands_risk_snapshots.rs` and the `/risk validate` path in `src/commands_risk.rs`),
   and inserted into the JSON **only when `Some`** — the exact convention the four existing
   optional fields (`emerging_accuracy_pct`, `severity`, `snapshot_git_hash`, `ci_run_id`) follow,
   so every legacy line and legacy reader stays valid. `UnhittableCount` already carries both live
   counts; pass the field through rather than recomputing anything.
2. **Give it a reader in the same diff (a definition without its consumer fails the build and
   reverts the task).** `parse_surprise_rows` / `RetrospectiveCount` in
   `src/commands_risk_unhittable.rs` gain a count of post-ledger rows that carry a **recorded
   pair**, plus the summed `unhittable`/`unmeasurable` those rows state, and `retrospective_note`
   appends one clause **only when that count is nonzero**, on the note's existing `"; "`
   separator, glyph-free, no em dash. `None`/`0` → the note is **byte-identical to pre-task**
   (whole-string `assert_eq!`, never a `contains`) — that is the entire regression surface, since
   the note prints on every `/risk` invocation.
3. **Do not change any existing number.** `accuracy_pct`, `unhittable_surprises`, the join, the
   git leg, `unmeasurable`/`hittable`/`ties` semantics and the Day-210 `dropped` clause all stay
   as they are. This task makes a recorded denominator legible; it does not make anything move.
4. **Be honest about the ratio in the clause's own words**: a recorded pair from a specific event
   still cannot say what the *join* would say about it; the clause reports what the record states,
   and the join-derived aggregate stays the separate number it already is. If the two disagree on
   any real row, print both and say so — never reconcile them silently.

## Tests (write them before the feature — the suite is the only instrument that can catch this)

- The reader is tested against a **real tempdir ledger**, driven through
  `retrospective_unhittable_at`, never by hand-setting a struct field. Day 210 ran exactly the
  opposite mistake in this module (a live-seam fixture that set `dropped: 2` by hand made a
  production-dead field look wired); the corrected form is the shape to copy.
- **Anti-vacuous**: assert the fixture really carries the recorded pair (and that its strings
  really fail to parse where a malformed row is used), so a transcription slip cannot make a test
  agree with itself.
- **Near-miss / byte-identity**: a legacy event with **no** recorded pair must produce a note
  byte-identical to the pre-task note, and a clean zero must return `None`.
- Singular/plural agree; glyph-free under `is_plain_output()`; silent under `is_quiet()`.
- **Positive control, run as ONE atomic mutate→run→restore command and serially** (two
  file-mutating controls in one block raced once and one falsely passed): neuter the new reader
  (make the clause always return `None`) and confirm **by name** exactly the new tests redden while
  every near-miss guard stays green; restore and confirm green with `grep -c NEUTERED` → 0.

## Constraints that will silently revert this task if missed

- **No byte indexing on strings.** Counts are interpolated, so no `is_char_boundary` walk should
  arise — if a path or hash is ever sliced, it must be. (#250 crashed a planner here.)
- **Module-size gate.** `MAX_MODULE_LINES = 2000` and `src/commands_risk_unhittable.rs` is at
  **1956** lines; `src/commands_risk_snapshots.rs` is registered at 2099 in
  `GRANDFATHERED_OVERSIZED_MODULES` and `src/commands_risk.rs` at 6526 — a grandfathered file that
  **grows** past its registered number fails. Put the new tests in the existing sibling
  `src/commands_risk_unhittable_tests.rs`, keep production-line growth in the main module to a
  couple of dozen lines, and re-read the register lines before and after. If the change does not
  fit under the caps, **shrink the change**, do not register a new ceiling.
- **No definition without its consumer in the same edit** (clippy under `-D warnings` fails on
  dead code and unused variables; a new `Option` field nothing reads is exactly that).
- **ARCHITECTURE.md carries the file's entry and the new Day-210 history goes there, never in
  CLAUDE.md.** Read the `commands_risk_unhittable.rs` bullet before editing; append the new
  paragraph in the same style (what was false before, what is verified at HEAD, the positive
  control's literal result, and the limits stated rather than implied).
- `scripts/evolve.sh` and `.github/workflows/` are protected — do not touch them.

## Done when

`cargo build && cargo test` green, `cargo clippy --all-targets -- -D warnings` clean,
`cargo fmt -- --check` clean, the positive control's literal output pasted in the session
summary, and ARCHITECTURE.md updated with the measured before/after (including how many of the 278
existing events carry each field — the honest zero is part of the deliverable).
