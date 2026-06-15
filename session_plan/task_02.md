Title: Extract dispatch_config_command helper from dispatch_command
Files: src/dispatch.rs
Issue: none

## Goal

Continue the dispatch decomposition pattern started on Days 104-106. Four helper dispatchers already exist (`dispatch_info_command`, `dispatch_git_command`, `dispatch_session_command`, `dispatch_dev_command`). Extract configuration and mode-switching commands into a fifth helper: `dispatch_config_command`.

## What to do

1. **Create `async fn dispatch_config_command`** following the exact same pattern as the existing helpers (takes `&CommandRoute` + `&mut DispatchContext<'_>`, returns `Option<CommandResult>`).

2. **Move these command arms** from the main `match route` block into the new helper:
   - `CommandRoute::Model` — the 43-line inline handler (model show, list, info, switch)
   - `CommandRoute::Provider` — provider show/switch
   - `CommandRoute::Think` — thinking level show/set with rebuild
   - `CommandRoute::Config` — config display
   - `CommandRoute::ConfigShow` — config show
   - `CommandRoute::ConfigEdit` — config edit
   - `CommandRoute::ConfigSet` — config set
   - `CommandRoute::ConfigGet` — config get
   - `CommandRoute::Hooks` — hooks display
   - `CommandRoute::Permissions` — permissions display
   - `CommandRoute::Teach` — teach mode
   - `CommandRoute::Effort` — effort level
   - `CommandRoute::Read` — read mode
   - `CommandRoute::Architect` — architect mode
   - `CommandRoute::Mcp` — MCP info

3. **Add the delegation call** in `dispatch_command` after the `dispatch_dev_command` call:
   ```rust
   if let Some(result) = dispatch_config_command(&route, ctx).await {
       return result;
   }
   ```

4. **Add the `unreachable!` arm** at the bottom of the main match for the extracted routes, matching the pattern used for the other groups:
   ```rust
   CommandRoute::Model
   | CommandRoute::Provider
   | ... => unreachable!("handled by dispatch_config_command"),
   ```

5. **Preserve all existing behavior exactly** — this is a pure mechanical extraction, no logic changes.

## Verification

- `cargo build` && `cargo test` — all pass, no behavior change
- `cargo clippy --all-targets -- -D warnings` — clean
- The main `match route` block should shrink by ~150-180 lines
- Count remaining inline arms — should be notably fewer
