Title: Fix flaky set_current_dir() test race condition
Files: src/commands_file.rs, src/commands_git.rs, src/setup.rs
Issue: none

## Problem

Multiple test functions call `std::env::set_current_dir()` which is process-global. When `cargo test` runs tests in parallel, any test that reads `std::env::current_dir()` can pick up the wrong directory, causing non-deterministic failures. The assessment identified `test_scan_important_files_in_current_project` as the flaky one, but the root cause is the `set_current_dir()` callers.

Affected tests (from grep):
- `src/commands_file.rs:1639` and `1652` — sets and restores cwd
- `src/commands_git.rs:2208` and `2212` — sets and restores cwd  
- `src/setup.rs:738` and `748` — sets and restores cwd
- `src/commands_session.rs:1181-1236` — multiple set_current_dir calls (4 occurrences)

Note: `commands_session.rs` has 4 occurrences but the 3-file limit means we handle the first three files here. If commands_session.rs still has issues, it can be a follow-up.

## Fix Strategy

For each affected test:
1. If the test calls `set_current_dir()` just to make relative paths work, refactor to use absolute paths or pass the temp dir path explicitly instead
2. If the test genuinely needs to be in a specific directory (e.g., testing git operations that check cwd), add `#[serial]` from the `serial_test` crate (already a dependency — check `Cargo.toml`)
3. Remove the `set_current_dir()` / restore pattern entirely where possible

The goal is: `cargo test` runs reliably in parallel with zero flaky failures.

## Verification

```bash
# Run the full test suite multiple times to check for flakiness
cargo test 2>&1 | tail -5
cargo test 2>&1 | tail -5
cargo test 2>&1 | tail -5
```

All three runs should show 0 failures.
