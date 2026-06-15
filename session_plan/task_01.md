Title: Wire worktree isolation into handle_spawn
Files: src/commands_spawn.rs
Issue: none

## Goal

Day 106 built and fully tested worktree primitives (`create_spawn_worktree`, `cleanup_spawn_worktree`, `list_spawn_worktrees`, `cleanup_stale_worktrees`) but `handle_spawn` doesn't use them yet. The 7 `#[allow(dead_code)]` items prove this gap. Wire the worktree lifecycle into the spawn flow so sub-agents execute in isolated git worktrees, enabling parallel file edits without git conflicts.

## What to do

1. **Remove all `#[allow(dead_code)]` annotations** from `WorktreeInfo`, `create_spawn_worktree`, `cleanup_spawn_worktree`, `list_spawn_worktrees`, `cleanup_stale_worktrees`.

2. **In `handle_spawn`** (the main spawn entry point, around line 443), integrate worktree isolation into the spawn flow:
   - Before launching a spawn task, call `create_spawn_worktree` to create an isolated worktree for the sub-agent.
   - Pass the worktree path to the sub-agent's working directory (the sub-agent should `cd` into the worktree before doing work).
   - Include the worktree path in `spawn_context_prompt` so the sub-agent knows it's working in an isolated directory.
   - After a spawn task completes (in `handle_spawn_collect` or in the result formatting), call `cleanup_spawn_worktree` to clean up.
   - If worktree creation fails (e.g., not a git repo, git worktree not supported), fall back gracefully to the current behavior (same directory) with a warning.

3. **Add stale worktree cleanup** — At the start of `handle_spawn`, call `cleanup_stale_worktrees` with a reasonable max age (e.g., 1 hour) to prevent accumulation from crashed sessions.

4. **Add `/spawn worktrees` subcommand** — Wire `list_spawn_worktrees` into the spawn status/subcommand handling so users can see active worktrees.

5. **Update existing tests** — Ensure the existing worktree tests still pass. Add at least one test that verifies the worktree path is included in the spawn context prompt when available.

## Verification

- `cargo build` — no `#[allow(dead_code)]` warnings
- `cargo test` — all existing worktree tests pass + new test
- `cargo clippy --all-targets -- -D warnings` — clean
