Title: Extract git-command dispatch from dispatch_command into dispatch_git_command
Files: src/dispatch.rs
Issue: none

## What to do

Follow the exact pattern established by `dispatch_info_command` on Day 104 to extract all git-related command routes from the main `dispatch_command` match block into a new `dispatch_git_command` helper function.

### Commands to extract

These `CommandRoute` variants should move into `async fn dispatch_git_command`:

- `CommandRoute::Diff` (~15 lines — has async explain subpath)
- `CommandRoute::Blame` (~3 lines)
- `CommandRoute::Undo` (~5 lines)
- `CommandRoute::Commit` (~7 lines — has async AI commit subpath)
- `CommandRoute::Pr` (~8 lines — async)
- `CommandRoute::Git` (~3 lines)
- `CommandRoute::Review` (~15 lines — async, has last_input mutation)

### Pattern to follow

1. Create `async fn dispatch_git_command(route: &CommandRoute, ctx: &mut DispatchContext<'_>) -> Option<CommandResult>` — same signature as `dispatch_info_command`.

2. Move each match arm from `dispatch_command` into this new function, wrapping returns in `Some(CommandResult::Continue)` (or whatever the arm returns). The `_ => None` fallthrough at the end returns `None` for non-git routes.

3. In `dispatch_command`, add a delegation call right after the existing `dispatch_info_command` call:
   ```rust
   if let Some(result) = dispatch_git_command(&route, ctx).await {
       return result;
   }
   ```

4. Add the extracted variants to the `unreachable!()` arm at the bottom of `dispatch_command` (where `Version`, `Status`, etc. are listed), so the compiler knows they're handled.

### Important details

- Several git commands mutate `ctx.last_input`, `ctx.undo_context`, or `ctx.last_error` — the `DispatchContext` is passed as `&mut`, so this works identically in the helper.
- `Commit` has an `if wants_ai_commit` branch — move the entire arm as-is.
- `Review` has complex post-processing (`last_input` mutation) — move the entire arm.
- `Diff` has the explain subpath — move the entire arm including the async explain handling.

### Verification

- `cargo build` must pass
- `cargo test` must pass (existing dispatch tests should still work)
- `cargo clippy --all-targets -- -D warnings` must pass
- The match block in `dispatch_command` should shrink by ~60 lines
