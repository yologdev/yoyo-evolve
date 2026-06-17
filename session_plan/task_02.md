Title: Extract dispatch_utility_command helper from dispatch_command
Files: src/dispatch.rs
Issue: none

## What to do

Extract a new `dispatch_utility_command` helper from `dispatch_command` in `src/dispatch.rs` to continue reducing the function's size (currently ~1,042 lines with 6 helpers already extracted).

### Commands to extract

Pull these simple command dispatches into a new `async fn dispatch_utility_command(route: &CommandRoute, ctx: &mut DispatchContext<'_>) -> Option<CommandResult>`:

1. **`CommandRoute::Watch`** — calls `commands::handle_watch(ctx.input)`
2. **`CommandRoute::Todo`** — calls `commands::handle_todo(ctx.input)`, prints result
3. **`CommandRoute::Run`** — calls `commands::handle_run_usage()` or `commands::handle_run(ctx.input)`
4. **`CommandRoute::Goal`** — calls `commands::handle_goal(ctx.input)`
5. **`CommandRoute::Revisit`** — calls `commands::handle_revisit(ctx.input)`, prints result
6. **`CommandRoute::Update`** — calls `commands::handle_update()`
7. **`CommandRoute::Skill`** — calls `commands::handle_skill(ctx.input, &ctx.agent_config.skills)`
8. **`CommandRoute::Remember`** — calls `commands::handle_remember(ctx.input)`
9. **`CommandRoute::Memories`** — calls `commands::handle_memories(ctx.input)`
10. **`CommandRoute::Forget`** — calls `commands::handle_forget(ctx.input)`

### Pattern to follow

Follow the exact pattern of the existing helpers (e.g., `dispatch_info_command`, `dispatch_file_command`):

```rust
async fn dispatch_utility_command(
    route: &CommandRoute,
    ctx: &mut DispatchContext<'_>,
) -> Option<CommandResult> {
    match route {
        CommandRoute::Watch => {
            commands::handle_watch(ctx.input);
            Some(CommandResult::Continue)
        }
        // ... etc
        _ => None,
    }
}
```

Then add to `dispatch_command`:
```rust
// Delegate utility commands (watch, todo, goal, memory, etc.)
if let Some(result) = dispatch_utility_command(&route, ctx).await {
    return result;
}
```

### Important notes

- Do NOT move commands that need complex inline logic (like Spawn which has agent interaction, or Loop which takes agent references, or Clear which modifies the agent). Only move the simple "call handler, return Continue" dispatches.
- The `Goal` route returns `CommandResult` directly from `handle_goal` — just return `Some(commands::handle_goal(ctx.input))`.
- The `Todo` route has `println!("{result}\n")` inline — include that in the match arm.
- The `Run` route has a conditional — include the if/else in the match arm.
- The `Update` route has match/Ok/Err — include that in the match arm.
- Keep the `_ => None` fallthrough so non-matching routes fall through to the remaining match block.

### Tests

No new tests needed — the existing routing tests (`test_dispatch_routing`) already cover all these routes. Run `cargo test` to verify nothing breaks. The existing tests for `route_command` are sufficient since we're only moving dispatch logic, not changing routing.

### Verification

After the change, `dispatch_command` should be noticeably shorter. The remaining inline commands should be only the complex ones: Quit, Help, Clear, ClearForce, Context, Init, Retry, Loop, Bg, Spawn, Explain, Plan.
