Title: Extract refactoring commands from commands_project.rs into commands_refactor.rs
Files: src/commands_refactor.rs (new), src/commands_project.rs, src/commands.rs
Issue: none

## Context

`commands_project.rs` is the largest file in the project at 3,791 lines. The assessment calls it
a "grab-bag" — it contains /todo, /context, /init, /plan, /extract, /refactor, /rename, AND /move.
These are mostly unrelated. The /extract, /rename, /move, and /refactor commands form a cohesive
"refactoring" group that should live in their own module.

This extraction follows the same pattern as Day 31's hooks.rs extraction from main.rs — move code
to a focused module, update imports, verify everything compiles and tests pass.

## What to extract

Move these items from `commands_project.rs` to a new `src/commands_refactor.rs`:

### Functions to move:
- `parse_extract_args` (line ~639)
- `find_symbol_block` (line ~660)
- `extract_symbol` (line ~784)
- `handle_extract` (line ~867)
- `handle_refactor` (line ~957)
- `is_word_boundary_char` (line ~1021)
- `is_word_start` (line ~1028)
- `is_word_end` (line ~1038)
- `rename_in_project` (line ~1066)
- `find_rename_matches` (line ~1115)
- `find_word_boundary_matches` (line ~1150)
- `list_git_files` (line ~1179)
- `format_rename_preview` (line ~1198)
- `apply_rename` (line ~1242)
- `replace_word_boundary` (line ~1286)
- `parse_rename_args` (line ~1310)
- `handle_rename` (line ~1322)
- `parse_move_args` (line ~1388)
- `find_impl_blocks` (line ~1430)
- `find_method_in_impl` (line ~1531)
- `move_method` (line ~1616)
- `reindent_method` (line ~1789)
- `handle_move` (line ~1822)
- `find_file_with_impl` (line ~1966)

### Structs to move:
- `RenameMatch` (line ~1047)
- `RenameResult` (line ~1056)
- `MoveArgs` (line ~1380)

### Tests to move:
All tests related to rename, move, extract, refactor, word_boundary, symbol_block, impl_blocks
(these start around line ~1990 in the #[cfg(test)] block — look for test functions that reference
the moved types/functions).

## Steps

1. Create `src/commands_refactor.rs` with all the above items. Add necessary imports at the top
   (std::fs, std::path, std::collections, and any format:: utilities used).

2. In `commands_project.rs`:
   - Remove the moved functions, structs, and their tests
   - Add `pub use crate::commands_refactor::*;` at the top OR keep specific re-exports for
     backward compatibility. Since `main.rs` references `commands_project::rename_in_project`
     and `commands_project::RenameResult`, the simplest approach is to add re-exports:
     ```rust
     pub use crate::commands_refactor::{
         handle_extract, handle_move, handle_refactor, handle_rename,
         rename_in_project, RenameMatch, RenameResult,
     };
     ```
     This means `main.rs` doesn't need changes — it still sees these through `commands_project::`.

3. In `commands.rs`:
   - Update the `pub use crate::commands_project` block to remove the moved items
   - Add a new `pub use crate::commands_refactor` block for the moved items
   - OR keep using commands_project's re-exports (simpler, less churn)

4. In `main.rs`:
   - Add `mod commands_refactor;` declaration
   - No other changes needed if commands_project re-exports the symbols

## Key constraint

Do NOT touch `repl.rs` or `main.rs` import paths — use re-exports from `commands_project.rs`
to maintain backward compatibility. This keeps the change to 3 files:
- `commands_refactor.rs` (new)
- `commands_project.rs` (items removed, re-exports added)
- `main.rs` (just add `mod commands_refactor;`)

## Verification

- `cargo build` — must pass
- `cargo test` — ALL existing tests must pass (including the moved ones in their new location)
- `cargo clippy --all-targets -- -D warnings` — no warnings
- `cargo fmt -- --check` — formatted
- Line count of `commands_project.rs` should drop by ~1300+ lines
- Line count of `commands_refactor.rs` should be ~1300+ lines

## Update CLAUDE.md

Update the Architecture section in CLAUDE.md:
- Add `commands_refactor.rs` entry: `/extract, /rename, /move, /refactor commands`
- Update `commands_project.rs` description to remove mention of rename/move/extract/refactor
- Update the Repository Structure section's function listing
