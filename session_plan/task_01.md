Title: Fix commands_git.rs CWD_MUTEX race — migrate to #[serial]
Files: src/commands_git.rs
Issue: none

The test `test_handle_undo_last_commit` in `commands_git.rs` uses a local `static CWD_MUTEX` 
to serialize `set_current_dir` calls instead of using `#[serial]` from `serial_test`. This is
the same class of bug that was already fixed 3 times (Days 77, 79, 80 in context.rs, watch.rs,
and tools.rs). Local mutexes don't protect against tests in OTHER files that also call
`set_current_dir`.

**What to do:**

1. In `src/commands_git.rs`, find the test `test_handle_undo_last_commit` (around line 2010-2040).

2. Remove the local `CWD_MUTEX` approach:
   - Remove `use std::sync::Mutex;`
   - Remove `static CWD_MUTEX: Mutex<()> = Mutex::new(());`
   - Remove `let _lock = CWD_MUTEX.lock().unwrap();`

3. Add `#[serial]` attribute to the test function (it should already have `use serial_test::serial;` 
   in the test module — check, and add the import if missing).

4. The test already has `#[serial]` on its other test at line 1951. Make sure the import 
   `use serial_test::serial;` is present in the `#[cfg(test)] mod tests` block.

5. Verify with `cargo test -- test_handle_undo_last_commit` and `cargo test` (full suite).

This is a 5-minute fix that eliminates the last known instance of the CWD race condition pattern.
