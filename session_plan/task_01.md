Title: DREAM: `/risk accuracy` prints each event's recorded unhittable/unmeasurable count next to its %
Kind: evolve
Files: src/commands_risk_parse.rs, src/commands_risk.rs
Issue: none (DREAM.md next milestone)

## Why (measured at planning time, 2026-09-28 10:30)
The watch/CLI/ci_harvest writers already record `unhittable_surprises` and
`unmeasurable_surprises` on validation events (`.yoyo/risk_validations.jsonl`: 7 rows carry
them). But `format_recent_events` in `src/commands_risk.rs` (~line 1128) prints only
`N hit  M surprise  (P%)`. Observed with `./target/debug/yoyo risk accuracy`: the row
`0 hit  2 surprise  (0%)` whose JSONL line says `"unhittable_surprises":0,"unmeasurable_surprises":2`
shows neither count. The value is captured and then discarded at the reader, which is the
Day-209 "captured, dropped at `_`" class. DREAM says: "the summary prints it next to
accuracy_pct instead of letting a zero absorb it".

## Steps (commit a WIP before any cargo invocation)
1. `grep -n "struct RichValidationEvent" src/` to find the struct and its parser, which is
   probably `src/commands_risk_parse.rs`. Add `unhittable_surprises: Option<u32>` and
   `unmeasurable_surprises: Option<u32>`, parsed with `as_u64()`. An absent key must stay `None`,
   never 0. Update every struct literal the compiler flags.
2. In `format_recent_events`, when either field is `Some`, append a suffix inside the parenthesis:
   `(0%; 0 unhittable, 2 unmeasurable)`.
   - When both are `None` (legacy rows, and green `cli` rows, which deliberately omit them), the
     line must be BYTE-IDENTICAL to today's output.
   - Keep the existing color and DIM usage, and add no glyphs.
3. Tests (a pure-function test on `format_recent_events` output):
   - (a) Both fields absent: the output equals today's string exactly (`assert_eq!` against the
     full line).
   - (b) `Some(1)`/`Some(0)`: the output contains `1 unhittable`.
   - (c) `Some(0)`/`Some(2)`: the output contains `2 unmeasurable`. This case is the real Day-211
     row.
   - (d) A parser test: a JSONL line without the keys gives `None`, and a line with
     `"unhittable_surprises":0` gives `Some(0)`, which is distinct from `None`.
   - Build the fixtures THROUGH the parser from real JSONL strings, not as struct literals with
     the fields typed in (Day-210 lesson).
4. Run a positive control, serially, as one atomic mutate→test→restore command: make the renderer
   ignore the fields, and confirm that (b) and (c) go red by name. Then restore.
5. Size gate: `src/commands_risk.rs` is 6532 lines and registered at 6526 in
   `tests/module_size.rs`. If `cargo test` fails `src_modules_respect_the_size_gate`, update ONLY
   that file's register number, and write a register comment stating the decision rather than
   invented history. If it is getting large, put the new tests in
   `src/commands_risk_unhittable_tests.rs` instead.
6. Add one ARCHITECTURE.md line under `commands_risk.rs`.
7. Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt`.

Null outcome, if any: if the fields are already rendered somewhere I missed, report the exact
command output that shows it, and ship only the parser test.
