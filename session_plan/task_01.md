Title: Extract /fork and /checkpoint from commands_session.rs into commands_fork.rs
Files: src/commands_fork.rs (new), src/commands_session.rs, src/commands.rs
Issue: none

## Goal

`commands_session.rs` is at 2,344 lines with at least 3 distinct responsibility groups. Extract the conversation forking (`/fork`) and checkpoint (`/checkpoint`) subsystems into a new `commands_fork.rs`. These are cohesive — both deal with branching/snapshotting conversation state — and together account for ~1,200 lines including their tests.

## What to extract

From `commands_session.rs`, move these items to `commands_fork.rs`:

**Fork subsystem (lines ~818–1117):**
- `ConversationBranch` struct
- `BranchStore` struct
- `with_branch_store_mut`, `with_branch_store` helpers
- `FORK_SUBCOMMANDS` const
- `current_branch_name`, `utc_timestamp`
- `parse_fork_subcommand`
- `handle_fork_create`, `handle_fork_switch`, `handle_fork_list`, `handle_fork_delete`, `handle_fork_rename`
- `fork_help`
- `handle_fork`

**Checkpoint subsystem (lines ~1119–1395+):**
- `clear_confirmation_message`
- `Checkpoint` struct
- `CheckpointStore` struct + all its impl methods (`new`, `save`, `restore`, `list`, `diff`, `delete`, `len`)
- `is_valid_checkpoint_name`
- `format_checkpoint_age`
- `handle_checkpoint`
- `checkpoint_subcommands`

**Tests:** Move all fork and checkpoint tests from the `#[cfg(test)] mod tests` block in `commands_session.rs` to the new file. Identify them by function names containing `fork`, `branch`, `checkpoint`, `clear_confirmation`.

## Steps

1. Create `src/commands_fork.rs` with the extracted code, adding necessary `use` imports (copy from `commands_session.rs` as needed — `yoagent::Agent`, `SessionChanges`, etc.)
2. Remove the extracted items from `commands_session.rs`
3. Update `src/commands.rs` re-exports:
   - Add `pub use crate::commands_fork::{...}` for all public items
   - Remove moved items from the `commands_session` re-export block
4. Add `mod commands_fork;` declaration wherever `commands_session` is declared (likely `main.rs`)
5. Run `cargo build && cargo test` to verify everything compiles and all tests pass
6. Update CLAUDE.md's file listing to add `commands_fork.rs` with its description

## Sizing

This is a pure extraction — no logic changes. The source and destination are clear. Should be straightforward for a focused agent. Touches exactly 3 files (new file + 2 edits).
