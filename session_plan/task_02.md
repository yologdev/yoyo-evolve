Title: Extract /stash from commands_session.rs into commands_stash.rs
Files: src/commands_stash.rs (new), src/commands_session.rs, src/commands.rs
Issue: none

## Goal

Continuing the `commands_session.rs` decomposition (which Task 1 starts with fork/checkpoint extraction), extract the `/stash` subsystem into its own `commands_stash.rs` module. The stash code is self-contained — it manages a conversation stash stack stored in a `Mutex<Vec<StashEntry>>` and has no dependencies on the fork or checkpoint code.

**IMPORTANT:** This task runs AFTER Task 1, so `commands_session.rs` will already have fork/checkpoint removed. Work with the file as you find it.

## What to extract

From `commands_session.rs`, move these items to `commands_stash.rs`:

**Stash subsystem (~260 lines of impl + tests):**
- `StashEntry` struct (private)
- `STASH` static `Mutex<Vec<StashEntry>>`
- `parse_stash_subcommand`
- `handle_stash_push`
- `handle_stash_pop`
- `handle_stash_list`
- `handle_stash_drop`
- `handle_stash`
- `stash_default_description` (test-only pub fn)

**Tests:** Move all stash-related tests (function names containing `stash`) from the `#[cfg(test)] mod tests` block.

## Steps

1. Create `src/commands_stash.rs` with the extracted code. Add necessary `use` imports.
2. Remove the extracted items from `commands_session.rs`.
3. Update `src/commands.rs` re-exports:
   - Add `pub use crate::commands_stash::{handle_stash};` (and any other public items)
   - Remove `handle_stash` from the `commands_session` re-export block
4. Add `mod commands_stash;` declaration in `main.rs`
5. Run `cargo build && cargo test`
6. Update CLAUDE.md's file listing to add `commands_stash.rs`

## Post-extraction state

After both Task 1 and Task 2, `commands_session.rs` should be reduced from ~2,344 lines to roughly ~900 lines, containing only the session-management core: compact, save/load, history, bookmarks (mark/jump), export, and the `clear_confirmation_message` utility. This is a healthy size for a single module.

## Sizing

Pure extraction, no logic changes. ~260 lines of implementation + associated tests. Touches exactly 3 files.
