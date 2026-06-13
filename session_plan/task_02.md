Title: Extract session-command dispatch from dispatch_command into dispatch_session_command
Files: src/dispatch.rs
Issue: none

## What to do

Continue the dispatch.rs extraction pattern by pulling session/conversation management commands into a `dispatch_session_command` helper. This is the second-largest group of simple delegations in the match block.

### Commands to extract

These `CommandRoute` variants should move into `async fn dispatch_session_command`:

- `CommandRoute::Save` (~3 lines)
- `CommandRoute::Load` (~4 lines — calls `reset_compact_thrash`)
- `CommandRoute::Stash` (~4 lines)
- `CommandRoute::Fork` (~4 lines)
- `CommandRoute::Checkpoint` (~3 lines)
- `CommandRoute::History` (~8 lines — has detail sub-dispatch)
- `CommandRoute::Search` (~3 lines — session search, not file search)
- `CommandRoute::Changes` (~7 lines — has summary sub-dispatch, async)
- `CommandRoute::Export` (~3 lines)
- `CommandRoute::Mark` (~3 lines)
- `CommandRoute::Jump` (~3 lines)
- `CommandRoute::Marks` (~3 lines)
- `CommandRoute::Compact` (~3 lines)

### Pattern to follow

1. Create `async fn dispatch_session_command(route: &CommandRoute, ctx: &mut DispatchContext<'_>) -> Option<CommandResult>` — same signature as `dispatch_info_command` and `dispatch_git_command` (from Task 1).

2. Move each match arm into this new function, wrapping returns in `Some(...)`. The `_ => None` fallthrough handles non-session routes.

3. In `dispatch_command`, add the delegation call after the git command delegation:
   ```rust
   if let Some(result) = dispatch_session_command(&route, ctx).await {
       return result;
   }
   ```

4. Add extracted variants to the `unreachable!()` arm at the bottom.

### Important details

- `Changes` has an async `handle_changes_summary` path — the function is already async so this works.
- `Load` calls `reset_compact_thrash()` after the handler — keep this in the extracted arm.
- `History` has the `detail` sub-dispatch logic — move the entire arm as-is.
- `Search` here is session search (`handle_search` on agent), NOT file search (grep/find).

### Verification

- `cargo build` must pass
- `cargo test` must pass
- `cargo clippy --all-targets -- -D warnings` must pass
- The match block in `dispatch_command` should shrink by ~50 lines
