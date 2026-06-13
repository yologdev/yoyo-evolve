Title: Extract dev-command dispatch from dispatch_command into dispatch_dev_command
Files: src/dispatch.rs
Issue: none

## What to do

Continue the dispatch.rs extraction pattern by pulling dev/lint/test commands into a `dispatch_dev_command` helper. These are all synchronous or simple-async commands related to project health and code quality.

### Commands to extract

These `CommandRoute` variants should move into `async fn dispatch_dev_command`:

- `CommandRoute::Health` (~3 lines)
- `CommandRoute::Doctor` (~3 lines)
- `CommandRoute::Test` (~3 lines)
- `CommandRoute::Security` (~3 lines)
- `CommandRoute::LintFix` (~7 lines — async, mutates `last_input`)
- `CommandRoute::Lint` (~9 lines — has conditional `last_input` mutation)
- `CommandRoute::Fix` (~6 lines — async, mutates `last_input`)

### Pattern to follow

1. Create `async fn dispatch_dev_command(route: &CommandRoute, ctx: &mut DispatchContext<'_>) -> Option<CommandResult>` — same signature as the other dispatch helpers.

2. Move each match arm into this new function.

3. In `dispatch_command`, add the delegation call after session command delegation:
   ```rust
   if let Some(result) = dispatch_dev_command(&route, ctx).await {
       return result;
   }
   ```

4. Add extracted variants to the `unreachable!()` arm at the bottom.

### Important details

- `LintFix` and `Fix` are async and mutate `ctx.last_input` — the `&mut DispatchContext` handles this.
- `Lint` conditionally sets `last_input` based on the lint result string — move the entire conditional arm.
- `Doctor` takes `provider` and `model` from `agent_config` — access via `ctx.agent_config`.
- All 7 commands return `CommandResult::Continue`.

### Verification

- `cargo build` must pass
- `cargo test` must pass
- `cargo clippy --all-targets -- -D warnings` must pass
- After all 3 tasks, the main `dispatch_command` match block should be reduced from ~762 lines to ~550 lines (~28% reduction), with the logic organized into 4 focused dispatch helpers (info, git, session, dev) plus the remaining commands.
