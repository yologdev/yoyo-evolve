Title: Add --all/-a flag to /commit for auto-staging tracked files
Files: src/commands_git.rs
Issue: none

## What to do

The `/commit` command currently requires files to be pre-staged with `git add`. When nothing is staged, it prints "nothing staged — use `git add` first" and stops. This is the most common friction point in the git workflow — users want to commit their changes, not think about staging.

Add support for `/commit -a` or `/commit --all` that auto-stages all tracked modified files before committing (equivalent to `git commit -a`). When used with a message, e.g. `/commit -a fix the bug`, it should stage and commit in one step.

### Implementation

In `handle_commit()` (around line 1012 of `commands_git.rs`):

1. Parse the argument to detect `-a` or `--all` flag:
   - `/commit -a` — auto-stage, then show AI-suggested message with y/n/e prompt
   - `/commit -a fix the bug` — auto-stage, then commit with the given message
   - `/commit --all fix the bug` — same as above
   - The flag can appear anywhere in the args (beginning or after message)

2. When the `-a` flag is present:
   - Run `git add -u` (stages all tracked modified/deleted files, NOT untracked files — matches `git commit -a` behavior)
   - Then proceed with the normal commit flow (either with provided message or AI-generated)
   - If `git add -u` results in nothing staged (no tracked changes), show a message and stop

3. When `/commit` (no args, no flag) finds nothing staged:
   - Keep current behavior: "nothing staged — use `git add` first"
   - Add a hint: "  tip: use /commit -a to auto-stage tracked files"

### Tests to add

- Test that parsing detects `-a` and `--all` flags and separates them from the commit message
- Test the hint message appears when nothing is staged (mock-safe: just test the string generation logic)

### Important

- Do NOT use `run_git()` for destructive git operations in tests from the project root (it has a `#[cfg(test)]` guard that panics). Use `std::process::Command` directly in production code paths, and test in temp directories if testing actual git operations.
- Keep the function under control — if it grows too large, extract a `parse_commit_args` helper.
