## Session Plan

### Task 1: Fix CI — clippy failures
Files: src/docs.rs, src/format.rs
Description: Two clippy errors are blocking CI:
1. In `src/docs.rs:179`: collapsible `else { if .. }` block — collapse to `} else if !summary.contains("📝") {`.
2. In `src/format.rs:1264`: `assert!(!SPINNER_FRAMES.is_empty())` is flagged as `const_is_empty` because `SPINNER_FRAMES` is a const and the compiler knows it's never empty. Replace the test assertion with `assert!(SPINNER_FRAMES.len() > 0)` or use `assert_eq!(SPINNER_FRAMES.len(), 10)` which is more precise and avoids the lint.
Run `cargo clippy --all-targets -- -D warnings` after fixing to confirm zero errors.
Issue: none

### Task 2: Extract health/project logic from main.rs into src/health.rs
Files: src/main.rs, src/health.rs (new), src/commands.rs
Description: `main.rs` is still 2,930 lines. Extract the following self-contained blocks into a new `src/health.rs` module:
- `ProjectType` enum and its `Display` impl (lines ~1806-1833)
- `detect_project_type()` (lines ~1835-1856)
- `health_checks_for_project()` (lines ~1858-1903)
- `run_health_check_for_project()` (lines ~1904-1940)
- `run_health_checks_full_output()` (lines ~1941-1977)
- `build_fix_prompt()` (lines ~1978-1995)
- All associated tests for project type detection and health checks

Add `mod health;` to main.rs. Update `commands.rs` to import from `health` where needed. This should remove ~400+ lines from main.rs. Run `cargo build && cargo test` to verify.
Issue: none

### Task 3: Extract tree/shell/multiline helpers from main.rs into src/utils.rs
Files: src/main.rs, src/utils.rs (new)
Description: Extract utility functions that don't depend on the agent into `src/utils.rs`:
- `run_shell_command()` (~1724-1752)
- `needs_continuation()` (~1753-1757)
- `collect_multiline_rl()` — this one depends on rustyline so it stays in main.rs
- `build_project_tree()` (~1997-2024)
- `format_tree_from_paths()` (~2025-2072)
- `is_unknown_command()` (~2073-2081)
- `thinking_level_name()` (~1799-1812)
- All associated tests

Add `mod utils;` to main.rs. This should remove ~300+ lines from main.rs. Run `cargo build && cargo test`.
Issue: none

### Task 4: Add `/grep` command for project-wide code search
Files: src/commands.rs, src/main.rs
Description: Add a `/grep <pattern> [path]` REPL command that runs ripgrep (or falls back to grep -rn) on the project directory — returning results directly to the user without AI round-trips or token cost. This is the "fuzzy file search" gap from the gap analysis. Claude Code can search codebases quickly; yoyo currently requires either using the AI's `search` tool (costs tokens) or `/run grep ...` (works but verbose).

Implementation:
- Add `/grep` to `KNOWN_COMMANDS` array
- Handle `/grep` in the REPL match block — parse pattern and optional path
- Run `rg --line-number --color=never <pattern> [path]` with fallback to `grep -rn <pattern> [path]`
- Display results with `{DIM}` formatting, limit to first 50 matches with a "(N more matches)" note
- Add to `/help` output
- Write tests for argument parsing
Issue: none

### Task 5: Add `/blame <file> [line]` command for quick git blame
Files: src/commands.rs, src/main.rs
Description: Add `/blame <file> [line-range]` that runs `git blame` on a file and shows results. This closes the "Git-aware file selection" gap partially — knowing who changed what and when helps developers without leaving the REPL. Supports:
- `/blame src/main.rs` — full file blame (truncated to ~30 lines with pagination hint)
- `/blame src/main.rs 100-120` — blame specific line range
Add to KNOWN_COMMANDS, /help text, and write tests.
Issue: none

### Issue Responses
No community issues today.
