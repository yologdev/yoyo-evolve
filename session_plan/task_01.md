Title: Fix the flaky prompt_budget live end-to-end test that turned main red (OnceLock ordering)
Kind: evolve
Files: src/prompt_budget.rs (and tests/global_state_races.rs only if it enumerates this global)
Issue: none

CI on HEAD fc101154 (run 36544101687) is RED: `5861 passed; 1 failed` —
`prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end` panicked at
src/prompt_budget.rs:1526 ("with env var set, session_budget_remaining() must return Some(_)").
The previous run was green, so it is a flake, not a regression.

Diagnosis from the assessment (read from code, NOT yet reproduced — step 1 verifies it):
`SESSION_BUDGET_SECS` is a process-global `OnceLock<Option<u64>>` read once from
`YOYO_SESSION_BUDGET_SECS`. The test sets the env var and relies on its `aaa` name to run first,
but libtest orders by full path, and tests in other modules (prompt.rs, watch.rs, main.rs, repl.rs
call `session_budget_exhausted()` / `session_budget_remaining()`) can freeze the cell at `None`
on another thread first.

Step 1 — confirm the mechanism by reading `session_budget_remaining` / the OnceLock init and the
test body. Do not change anything yet. If the mechanism is different from the above, fix what
you actually find and say so in the commit message.

Step 2 — make the live end-to-end check independent of global ordering. Preferred: the test
re-executes its own test binary in a fresh process —
`std::process::Command::new(std::env::current_exe()?)` with args
`["--exact", "prompt_budget::tests::<child_name>", "--ignored", "--nocapture"]` and
`.env("YOYO_SESSION_BUDGET_SECS", "<n>")` — where `<child_name>` is an `#[ignore]`d test that
asserts `session_budget_remaining().is_some()` (keep the original assertions and message). The
parent asserts the child exited successfully AND that its stdout contains `1 passed` (anti-vacuous:
an `--exact` name typo runs 0 tests and exits 0). This keeps the "live, real env var, real global"
meaning, which a parameterised seam would lose. Drop the `aaa` naming dependence (rename is fine;
do not delete the test's coverage). If a `_with(&OnceLock, …)` seam already exists and the test is
redundant with it, still keep a live-path test.

Positive control (serial, atomic mutate→run→restore in ONE command, marker `NEUTERED` on the
mutated line): make the child assert `is_none()` and confirm the parent test fails by name; restore
and confirm green. `tests/neutered_guards.rs` must pass afterwards.

Verify: `cargo build`, `cargo test` (full, not just the module), `cargo clippy --all-targets -- -D warnings`,
`cargo fmt -- --check`. Add a short ARCHITECTURE.md note under src/prompt_budget.rs describing the
flake and why the subprocess shape was chosen (ARCHITECTURE.md is a doc, fine to touch).
