Title: Extract banner.rs from cli.rs — move banner/welcome logic to dedicated module
Files: src/banner.rs (new), src/cli.rs, src/main.rs
Issue: none

## What to do

`cli.rs` at 3,113 lines is the largest non-extraction file. It mixes CLI argument parsing with display logic (banner, welcome text, git status). Extract the banner/welcome cluster into a new `src/banner.rs` module:

### Functions to extract:
- `print_banner()` — the startup banner display
- `banner_project_line()` — formats the "📁 Rust project (name) on branch" line
- `parse_git_status_counts()` — parses `git status --porcelain` output
- `git_status_summary()` — builds "2 modified, 1 staged" string
- `get_welcome_text()` — the first-run welcome message
- `print_welcome()` — prints the welcome text

### Steps:
1. Create `src/banner.rs` with all six functions and their imports
2. Move all associated tests from `cli.rs` to `banner.rs` (tests for `banner_project_line`, `parse_git_status_counts`, `git_status_summary`, `print_banner`, `get_welcome_text`, `print_welcome`)
3. Update `cli.rs` to `pub mod banner;` or add `mod banner` in `main.rs` and re-export via `use crate::banner::*` where needed
4. Update `main.rs` to call `banner::print_banner()` and `banner::print_welcome()` instead of `cli::print_banner()` and `cli::print_welcome()`
5. Run `cargo build && cargo test` to verify everything still works
6. Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt`

### Key constraints:
- Don't move `parse_args`, `parse_thinking_level`, `clamp_temperature`, or any CLI parsing logic — only banner/welcome display
- Keep the `enable_verbose`/`is_verbose` functions in `cli.rs` since they're used throughout CLI parsing
- The new module needs access to `ProjectType` from `commands_project.rs` and git utilities
- The `DAY_COUNT` and `GIT_HASH` compile-time env vars are used in `print_banner` — bring those references along

### Update CLAUDE.md:
Add `banner.rs` to the Architecture section with description: "startup banner, welcome text, git status summary display"
