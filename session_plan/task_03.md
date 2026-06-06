Title: Wire --auto-edit into build_tools for file-edit auto-approval
Files: src/tools.rs
Issue: #466

## Prerequisite

Task 2 must be completed first — it adds `is_auto_edit()` to `cli_config.rs`.

## What to do

In `src/tools.rs`, modify `build_tools` to check `is_auto_edit()` and auto-approve file operations while keeping bash confirmation.

### Changes to `src/tools.rs`:

1. Import `is_auto_edit` from cli_config (or cli, wherever it's re-exported):
```rust
use crate::cli_config::is_auto_edit;
```

2. In `build_tools`, after the `auto_approve` parameter is used, add the `auto_edit` check. The current logic is:
```rust
let bash = if auto_approve { ... no confirm ... } else { ... with confirm ... };
let write_tool = if auto_approve { ... no confirm ... } else { ... with confirm ... };
let edit_tool = if auto_approve { ... no confirm ... } else { ... with confirm ... };
let rename_tool = if auto_approve { ... no confirm ... } else { ... with confirm ... };
```

Change the file-operation tools (write_tool, edit_tool, rename_tool) to also skip confirmation when `is_auto_edit()` is true:
```rust
let write_tool = if auto_approve || is_auto_edit() { ... no confirm ... } else { ... with confirm ... };
let edit_tool = if auto_approve || is_auto_edit() { ... no confirm ... } else { ... with confirm ... };
let rename_tool = if auto_approve || is_auto_edit() { ... no confirm ... } else { ... with confirm ... };
```

Keep bash as-is — it should still require confirmation when `auto_edit` is true (only `auto_approve` skips bash confirmation).

3. Add tests:
   - `test_build_tools_auto_edit_approves_files_but_confirms_bash`: Set the `AUTO_EDIT` OnceLock to true (or use a test helper), build tools with `auto_approve: false`, verify that write_file/edit_file/rename_symbol don't have ConfirmTool wrappers but bash still does. NOTE: Since OnceLock can only be set once per process, this test may need to use a different approach. Check how existing tests handle the VERBOSE/QUIET globals — they may use `#[serial]` or similar. If the OnceLock pattern makes testing hard, consider using an `AtomicBool` instead (which can be reset). Look at how `always_approved` (the `Arc<AtomicBool>` in build_tools) works and consider using the same pattern.

   IMPORTANT: If `OnceLock` makes testing difficult (can't be reset between tests), change the implementation to use `AtomicBool` with `Ordering::Relaxed` instead — matching the `QUIET` or `COLOR_DISABLED` patterns if they use AtomicBool, or the `always_approved` pattern in this same file.
