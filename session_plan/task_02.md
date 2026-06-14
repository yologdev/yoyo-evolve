Title: Git worktree lifecycle module for parallel sub-agent isolation
Files: src/commands_spawn.rs
Issue: none

## What

Add git worktree lifecycle management functions to `commands_spawn.rs` — the foundation for running spawned sub-agents in isolated worktrees so they can make concurrent changes without interfering with each other or the main working tree.

## Why

This is the single biggest competitive gap vs Cursor. Cursor runs up to 8 agents simultaneously on isolated git worktrees. yoyo has `/spawn` and `/bg` but all sub-agents operate in the same working directory, which means concurrent agents can step on each other's file edits. Git worktrees provide lightweight, native isolation — each worktree is a full checkout that shares the same `.git` object store but has its own working directory and index.

This task builds the lifecycle primitives only. Integration with the actual spawn execution path is a separate follow-up task — this keeps the scope achievable and independently testable.

## Implementation

1. **Add a `WorktreeInfo` struct** to `commands_spawn.rs`:
   ```rust
   pub struct WorktreeInfo {
       pub path: PathBuf,      // absolute path to the worktree directory
       pub branch: String,     // the detached/branch name used
       pub created_at: Instant,
   }
   ```

2. **Add `create_spawn_worktree(task_id: usize) -> Result<WorktreeInfo, String>`**:
   - Create a temporary directory for the worktree (under `.yoyo/worktrees/spawn-{task_id}-{timestamp}/`)
   - Run `git worktree add --detach <path>` to create a detached worktree at current HEAD
   - Return the `WorktreeInfo` with the path
   - On failure, clean up the directory and return an error

3. **Add `cleanup_spawn_worktree(info: &WorktreeInfo) -> Result<(), String>`**:
   - Run `git worktree remove --force <path>` to remove the worktree
   - If that fails, fall back to `git worktree remove <path>` without force
   - If that also fails, try manual cleanup: remove the directory and run `git worktree prune`
   - Log the cleanup result

4. **Add `list_spawn_worktrees() -> Vec<WorktreeInfo>`** (optional, time permitting):
   - Run `git worktree list --porcelain` and parse the output
   - Filter to only worktrees under `.yoyo/worktrees/spawn-*`

5. **Add `cleanup_stale_worktrees()`** (optional):
   - Find any worktrees under `.yoyo/worktrees/spawn-*` that are older than 1 hour
   - Clean them up to prevent leaks from crashed sessions

6. **Tests** — All tests MUST use a temp directory (not the project root) because `run_git()` has a `#[cfg(test)]` destructive-command guard. Tests should:
   - Create a temp git repo, create a worktree, verify it exists, clean it up, verify it's gone
   - Test cleanup of a worktree that's already been removed (idempotent cleanup)
   - Test the path format (`.yoyo/worktrees/spawn-{id}-{ts}/`)
   - Verify the worktree is at the correct commit (detached HEAD at parent's HEAD)

7. **Add `.yoyo/worktrees/` to `.gitignore`** if not already present.

## Scope guard
This task adds lifecycle functions and tests ONLY. It does NOT modify the spawn execution path (`handle_spawn`, `run_spawn_task`, etc.). That integration is a separate task. This keeps the diff small and the risk low — if the worktree functions work, the integration can be built on top of them.

## Verification
`cargo build && cargo test && cargo clippy --all-targets -- -D warnings`
