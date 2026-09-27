Title: Stop the compact-thrash tests racing on the global COMPACT_THRASH_COUNT (#963)
Kind: evolve
Files: src/commands_session.rs, ARCHITECTURE.md
Issue: #963

## Why
`test_compact_thrash_detection_increments_on_low_reduction` and
`test_compact_thrash_detection_resets_on_meaningful_reduction` (src/commands_session.rs,
~:975/:989) both mutate the process-global `COMPACT_THRASH_COUNT` (~:23) and call
`reset_compact_thrash()`. libtest runs them in parallel, so they race. Measured by the
creator: 1 failure in a full `cargo test`, and pass/FAIL/pass over three filtered runs.
The evolve gate runs `cargo test` after every task, so this can revert correct work.

## Steps (ordered — do them in this order)
1. `grep -n "COMPACT_THRASH_COUNT\|reset_compact_thrash" -r src/ tests/` — list EVERY
   reader and writer (production and test). Write the list into your notes before editing.
2. Add a `_with` seam: move the counter logic into a function that takes
   `&AtomicU32` (or whatever the counter's actual type is — match it, don't guess) and
   have the existing public function call it with the global. Production behaviour must
   be byte-identical. Rewrite the two tests (and any other test from step 1 that touches
   the global) to drive a LOCAL atomic through the seam. Do not delete either test and do
   not weaken any assertion — each existing assertion must still be present, now aimed
   at the local atomic. This is the remedy `tests/global_state_races.rs` names as best,
   and matches `context_budget_warning_with` / `drain_failure_note`.
   COMMIT HERE (`git add -A && git commit -m "wip: compact-thrash _with seam"`) — before
   any long cargo run.
3. Verify: `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt -- --check`,
   then the positive control from the issue — run
   `for i in $(seq 1 20); do cargo test --bin yoyo test_compact_thrash_detection -q 2>&1 | tail -1; done`
   and report the pass count verbatim (expected 20/20). Then full `cargo test`.
   Add one short entry under src/commands_session.rs in ARCHITECTURE.md: the race, the
   seam, the 20/20 number as measured (not as expected).

## Honest limits to state in the write-up
20/20 is evidence the race is gone, not proof; say so. If step 1 finds tests outside
this file that also touch the global, fix them in the same way only if they are in
src/commands_session.rs; otherwise name them in ARCHITECTURE.md and leave them.
