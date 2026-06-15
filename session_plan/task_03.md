Title: Extract dispatch_file_command helper from dispatch_command
Files: src/dispatch.rs
Issue: none

## Goal

Extract file, search, and navigation commands into a sixth dispatch helper: `dispatch_file_command`. After this + task_02, the remaining inline match arms will be only the truly unique ones (Quit, Help, Clear, Spawn, Plan, Extended, Side, Quick, etc.) — the irreducible core.

## What to do

1. **Create `async fn dispatch_file_command`** following the same pattern as existing helpers.

2. **Move these command arms** from the main match block:
   - `CommandRoute::Add` — file addition (this is the largest, ~30 lines inline including related-file suggestions and content block injection)
   - `CommandRoute::Apply` — patch application
   - `CommandRoute::Open` — editor open
   - `CommandRoute::Docs` — docs handler
   - `CommandRoute::Find` — file finder
   - `CommandRoute::Grep` — grep
   - `CommandRoute::Index` — project index
   - `CommandRoute::Map` — repo map
   - `CommandRoute::Outline` — outline
   - `CommandRoute::Tree` — tree
   - `CommandRoute::Web` — web search
   - `CommandRoute::Rename` — rename symbol
   - `CommandRoute::Move` — move method
   - `CommandRoute::Extract` — extract refactor
   - `CommandRoute::Refactor` — refactor
   - `CommandRoute::Copy` — copy output
   - `CommandRoute::Ast` — ast-grep
   - `CommandRoute::Search` — (if present as a distinct route)

3. **Add the delegation call** in `dispatch_command` after `dispatch_config_command`:
   ```rust
   if let Some(result) = dispatch_file_command(&route, ctx).await {
       return result;
   }
   ```

4. **Add the `unreachable!` arm** at the bottom for extracted routes.

5. **Pure mechanical extraction** — no logic changes whatsoever.

## Verification

- `cargo build` && `cargo test` — all pass
- `cargo clippy --all-targets -- -D warnings` — clean
- After both tasks 2 and 3, count the remaining inline match arms. Goal: the main match should have <20 arms (down from ~50).
