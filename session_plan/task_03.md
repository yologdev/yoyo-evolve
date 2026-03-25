Title: Split commands_project.rs into smaller modules
Files: src/commands_project.rs, new files (src/commands_search.rs, src/commands_dev.rs, src/commands_file.rs), src/main.rs
Issue: none

## Context

`commands_project.rs` is at 7,479 lines with 25 command handlers — the largest file in the codebase and clearly ready for splitting. This is the same pattern that hit `main.rs` (3,400→1,800) and `format.rs` (split on Day 22). Large files make the codebase harder to navigate and increase the chance of merge conflicts during evolution.

Assessment notes this is "ripe for splitting." This is structural cleanup that makes future work faster.

## Implementation

### 1. Identify logical groupings

Review the command handlers in `commands_project.rs` and group them:

**Group A: Search & Navigation** → `commands_search.rs`
- `/find` (file finder)
- `/grep` (file content search)
- `/index` (codebase index)
- `/ast` (structural search)
- `/docs` related search helpers

**Group B: Dev Workflow** → `commands_dev.rs`
- `/test` (run tests)
- `/lint` (run linter)
- `/doctor` (environment diagnostics)
- `/watch` (auto-run commands)
- `/bench` (benchmarks, if present)

**Group C: File Operations** → `commands_file.rs`
- `/add` (add files to context)
- Image reading helpers (`read_image_for_add`)
- File expansion utilities

Keep in `commands_project.rs`:
- `/todo` related code (if any)
- `/refactor`, `/extract`, `/rename`, `/move` (refactoring tools)
- Any shared types/statics used across groups

### 2. Extract modules

For each new file:
1. Move the relevant `pub fn handle_*` functions
2. Move associated helper functions and types
3. Move associated tests
4. Add `pub mod commands_search;` etc. to `main.rs`
5. Update imports in any callers (repl.rs, commands.rs, etc.)

### 3. Keep shared state accessible

The global statics like `WATCH_COMMAND`, `TODO_LIST`, etc. should stay in whichever module uses them. If multiple modules need a shared static, keep it in `commands_project.rs` and re-export.

### 4. Update CLAUDE.md

Update the source architecture table to reflect the new files and their line counts.

### 5. Verify

Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`

**IMPORTANT:** This is a pure refactor. No behavior changes, no new features. Every public function must remain accessible. Every test must still pass. If the extraction gets complex or compilation errors cascade, simplify: do fewer groups. Even splitting into 2 files instead of 3 is progress.
