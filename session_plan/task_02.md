Title: Consolidate raw git calls in commands_info.rs and commands_dev.rs
Files: src/commands_info.rs, src/commands_dev.rs, src/git.rs
Issue: none (code quality — assessment finding #1)

## Context

The assessment found 54+ raw `std::process::Command::new("git")` calls outside `git.rs`.
Day 110 session 1 already consolidated `commands_spawn.rs` and added `run_git_in_dir` and
`run_git_output` helpers. This task continues that arc for two more files.

**Production code calls to replace:**

### commands_info.rs (4 production calls)

1. **Line ~42** — `compute_self_written_pct_inner()`: `git ls-files 'src/*.rs'`
   → Replace with `run_git(&["ls-files", "src/*.rs"])` or `run_git_output`

2. **Line ~56** — `compute_self_written_pct_inner()`: `git log --oneline --author=...`
   → Replace with `run_git_output(&["log", "--oneline", ...])`

3. **Line ~1027** — `handle_changelog()`: `git log --oneline --no-merges ...`  
   → Replace with `run_git(&["log", "--oneline", "--no-merges", ...])`

4. **Line ~1354** — `handle_evolution()`: `git tag -l 'evolution-*' ...`
   → Replace with `run_git(&["tag", "-l", "evolution-*", ...])`

### commands_dev.rs (3 production calls)

1. **Line ~45** — `run_doctor_checks()`: `git --version`
   → Replace with `run_git(&["--version"])`

2. **Line ~67** — `run_doctor_checks()`: `git remote -v`
   → Replace with `run_git(&["remote", "-v"])`

3. **Line ~72** — `run_doctor_checks()`: `git branch --show-current`
   → Replace with `run_git(&["branch", "--show-current"])` (or use `git_branch()` helper)

## Implementation notes

- Import `crate::git::{run_git, run_git_output}` at the top of each file
- Remove `use std::process::Command` if it becomes unused after the replacement
- The `run_git` helper returns `Result<String, String>` — match on it the same way
  the current code matches on `.output()`
- For cases where `.status.success()` is checked, `run_git` already handles that
  (returns `Err` on non-zero exit)
- For cases needing raw `Output` (stdout + stderr separately), use `run_git_output`
- Do NOT touch test code (`#[cfg(test)]` blocks) — tests in temp dirs legitimately
  need direct Command calls since `run_git` operates on the project root

## Tests

- Existing tests should continue to pass unchanged
- No new tests needed — this is a refactor of call sites, not new logic

## Verification

After changes: `cargo build && cargo test && cargo clippy --all-targets -- -D warnings`
