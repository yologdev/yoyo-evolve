Title: Add /diff --stat for compact change overview
Files: src/commands_git.rs, src/help_data.rs
Issue: none

## What to do

Add `--stat` flag to `/diff` that shows a compact `git diff --stat` style summary — per-file change counts with a visual bar. The infrastructure already exists in `commands_git.rs`: `DiffStatEntry`, `DiffStatSummary`, `parse_diff_stat()`, and `format_diff_stat()` are all implemented and tested. This just needs to wire the flag.

### Steps:

1. **Add `--stat` to `DiffOptions`:**
   In `parse_diff_args()`, add a `stat: bool` field to `DiffOptions` struct. Parse `--stat` from the args (same pattern as `--explain`, `--cached`, etc.).

2. **Handle `--stat` in `handle_diff()`:**
   When `opts.stat` is true:
   - Run `git diff --stat` (with `--cached` and `--no-index` if those flags are set)
   - Parse the output with `parse_diff_stat()`
   - Format and display with `format_diff_stat()`
   - Return early (don't show the full diff)

3. **Support combining with other flags:**
   - `/diff --stat` — stat of unstaged changes
   - `/diff --stat --cached` — stat of staged changes
   - `/diff --stat HEAD~3` — stat of last 3 commits
   - `/diff --stat branch` — stat vs another branch

4. **Add help text:**
   - Update `command_help("diff")` in `help_data.rs` to document `--stat`
   - Add to the flags section: `--stat — show compact per-file change summary`

5. **Add tests:**
   - Test that `parse_diff_args("--stat")` sets `stat: true`
   - Test that `parse_diff_args("--stat --cached")` sets both flags
   - Test that the `--stat` flag doesn't interfere with other flags

6. Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt`

### Key constraints:
- This is a display-only feature — no new git operations, just using `git diff --stat` output
- The `parse_diff_stat()` and `format_diff_stat()` functions are already tested; don't duplicate those tests
- Keep the implementation simple: `--stat` is mutually exclusive with `--explain` (if both are passed, `--stat` wins)
