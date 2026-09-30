Title: The watch path stops stamping `trigger: "watch_failure"` on green watch cycles
Kind: evolve
Files: src/commands_risk_snapshots.rs, ARCHITECTURE.md
Issue: none (self-discovered; ARCHITECTURE.md ~l.569 lists it as "Open, not fixed here")

## Why
`auto_validate_after_failure_to` (src/commands_risk_snapshots.rs ~l.925) passes the literal
`"watch_failure"` as the `trigger` argument to `write_validation_event` on EVERY call, but
src/watch.rs:1734 calls it with severity `"watch_success"` after a GREEN watch. Planner census of
.yoyo/risk_validations.jsonl: 76 rows are (trigger `watch_failure`, severity `watch_success`).
Every watch row names a failure whatever happened, and the Day-205 learning already misread
`trigger=watch_failure` as "the failing path". The other writers already follow
trigger = SOURCE, severity = OUTCOME (`cli`/…, `ci_harvest`/`ci_failure`). The watch path is the
one that breaks that convention.

Verified by the planner (grep, not assumption): no production reader filters on the trigger
string. It appears only in fixtures and in the writer, so changing the value written going forward
cannot change any count or verdict. Re-check that with one grep before editing
(`grep -rn '"trigger"' src/ scripts/`, then ignore raw-string fixtures).

## Steps (2)
1. **Write the test first, then the fix.** In the existing `#[cfg(test)]` module of
   commands_risk_snapshots.rs, add a test that drives the REAL `auto_validate_after_failure_to`
   through the existing tempdir `_to` seam (copy the shape of the test near l.1453) with severity
   `"watch_success"`, then reads the written row back and asserts `trigger == "watch"` and
   `severity == "watch_success"`. Add the near-miss row with severity `"watch_failure"`: the trigger
   is still `"watch"` and the severity is `"watch_failure"`. Update the one pre-existing assertion
   at ~l.1464 (`parsed["trigger"] == "watch_failure"`) to `"watch"`. It is an exact-value assertion
   on a value this task deliberately changes, so change the expected literal and keep the
   assert_eq. Do NOT delete it and do NOT weaken it to a contains. Then change the literal at
   ~l.925 to `"watch"`, and update the doc comments at ~l.473, ~l.483 and ~l.828 that describe the
   trigger values. Positive control, run as ONE atomic command: temporarily put `"watch_failure"`
   back and mark the line `// NEUTERED`, run
   `cargo test commands_risk_snapshots`, confirm the new test fails BY NAME, restore, and rerun
   green.
2. **Record it.** In ARCHITECTURE.md, change the "Open, not fixed here" line for this defect
   (~l.569) to say it is fixed. State: rows written before this commit carry `trigger:
   "watch_failure"` for BOTH outcomes, so for those rows the outcome must be read from `severity`.
   Old rows are left as they are (the ledger is append-only history). Do not claim any count
   moved, because nothing reads trigger.

## Constraints
- commands_risk_snapshots.rs is 2234 lines against a register entry of 2180, a drift of 54 inside
  a 100-line grace band. Keep the net addition under ~40 lines. Run `cargo test --test
  module_size` and if it warns, paste the number the gate PRINTS into the register comment. Do not
  compute the number yourself.
- Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`
  before finishing.
