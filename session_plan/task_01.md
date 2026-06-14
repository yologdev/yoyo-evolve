Title: Add commit/branch range support to /diff
Files: src/commands_git.rs
Issue: none

## What

The `/diff` command currently only shows working tree changes (staged + unstaged). It has no support for comparing between branches, commits, or tags — a fundamental git workflow need. Every developer compares branches regularly (`git diff main..feature`, `git diff HEAD~3`, `git diff v1.0..HEAD`).

## Why

This is a real capability gap vs Claude Code and every other coding agent that can run arbitrary git commands. While a user *can* run `bash: git diff main..feature`, the `/diff` command should handle this natively with its existing features: `--stat`, `--name-only`, `--functions`, `--explain`, and colored output.

## Implementation

1. **Extend `DiffOptions`** — Add an optional `ref_range: Option<String>` field. This captures anything that looks like a git ref: `main`, `main..feature`, `HEAD~3`, `v1.0..HEAD`, etc.

2. **Update `parse_diff_args`** — After processing known flags (`--staged`, `--name-only`, etc.), check remaining args for ref-like patterns. A ref is anything that's not a flag and not an existing file path. If a `..` is present, it's definitely a ref range. If it's ambiguous, try `git rev-parse` to disambiguate. When a ref_range is present, `--staged` is ignored (it doesn't make sense when comparing commits).

3. **Update `handle_diff`** — When `ref_range` is set, use it in the `git diff` command instead of the implicit HEAD comparison. All existing modes (`--stat`, `--name-only`, `--functions`, `--explain`) should work with ref ranges:
   - `/diff main..feature` — full diff between branches
   - `/diff main..feature --stat` — stat summary
   - `/diff main..feature --name-only` — changed file names
   - `/diff main..feature --functions` — symbol-level changes
   - `/diff HEAD~3` — diff from 3 commits ago to working tree
   - `/diff v1.0..HEAD --stat` — changes since v1.0

4. **Update `handle_diff_functions`** — When a ref range is present, pass it to the `git diff` commands that list changed files and generate diffs, instead of comparing working tree.

5. **Tests** — Add tests for:
   - `parse_diff_args` recognizing ref ranges with `..`
   - `parse_diff_args` with ref + flags combined
   - `parse_diff_args` distinguishing file paths from refs (when ambiguous, file takes precedence if it exists on disk)
   - `DiffOptions` correctly stores ref_range

6. **Update help text** in `help_data.rs` for `/diff` to document the new syntax.

## Scope guard
Only modify `src/commands_git.rs` (parser + handler changes). Help text update in `help_data.rs` is optional if time permits but not required for the task to succeed.

## Verification
`cargo build && cargo test && cargo clippy --all-targets -- -D warnings`
