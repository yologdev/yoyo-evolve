Title: Introduce DispatchContext struct to replace 20-parameter dispatch_command signature
Files: src/dispatch.rs, src/repl.rs
Issue: none

## Context
`dispatch_command()` in dispatch.rs takes 20 parameters. This is the single biggest
structural smell identified in the assessment. Every new command that needs state
requires modifying this signature in multiple places. A context struct eliminates
this friction and makes future commands trivially easier to add.

The function is called from exactly ONE place: `src/repl.rs` line ~452.

## What to do

1. Define a `DispatchContext` struct in `src/dispatch.rs` that bundles all 20 params:
   ```rust
   pub(crate) struct DispatchContext<'a> {
       pub input: &'a str,
       pub agent: &'a mut yoagent::agent::Agent,
       pub agent_config: &'a mut AgentConfig,
       pub session_total: &'a mut Usage,
       pub session_changes: &'a SessionChanges,
       pub turn_history: &'a mut TurnHistory,
       pub bg_tracker: &'a commands::BackgroundJobTracker,
       pub spawn_tracker: &'a commands::SpawnTracker,
       pub undo_context: &'a mut Option<String>,
       pub last_input: &'a mut Option<String>,
       pub last_error: &'a mut Option<String>,
       pub bookmarks: &'a mut commands::Bookmarks,
       pub checkpoint_store: &'a mut commands::CheckpointStore,
       pub session_start: Instant,
       pub turn_count: usize,
       pub cwd: &'a str,
       pub mcp_cli_servers: &'a [String],
       pub mcp_server_configs: &'a [McpServerConfig],
       pub mcp_count: u32,
       pub openapi_count: u32,
   }
   ```

2. Change `dispatch_command` signature from 20 params to `ctx: &mut DispatchContext<'_>`:
   ```rust
   pub(crate) async fn dispatch_command(ctx: &mut DispatchContext<'_>) -> CommandResult {
   ```

3. Inside the body of `dispatch_command`, replace bare parameter names with `ctx.field`:
   - `input` → `ctx.input`
   - `agent` → `ctx.agent`
   - `agent_config` → `ctx.agent_config`
   - `session_total` → `ctx.session_total`
   - etc.
   
   **Strategy:** Use search-and-replace carefully. The parameter names are used as arguments
   to handler function calls throughout the ~1200-line match body. Each occurrence should
   be prefixed with `ctx.`. Be systematic — go parameter by parameter.

4. Also update `try_dispatch_subcommand` if it takes a similar parameter list. Check its
   signature and update similarly if needed.

5. Update the single call site in `src/repl.rs` (~line 452) to construct a `DispatchContext`
   and pass it:
   ```rust
   let mut ctx = DispatchContext {
       input: &trimmed,
       agent: &mut agent,
       // ... all fields ...
   };
   let cmd_result = dispatch_command(&mut ctx).await;
   ```

6. Run `cargo build` to catch any missed references.
7. Run `cargo test` — no behavior change, all tests must pass.
8. Run `cargo clippy --all-targets -- -D warnings` — must be clean.

## Key risks
- The match body is ~1200 lines with many handler calls. Missing one `ctx.` prefix
  will cause a compile error — which is good (caught at build time).
- Lifetime annotations on the struct need to match the borrows. Use a single `'a`.
- Some params are `&mut` (agent, agent_config, session_total, etc.) and some are `&`
  (session_changes, bg_tracker, etc.). The struct fields must reflect this.

## Do NOT
- Change any behavior — this is a pure refactor
- Modify handler functions in other files — they keep their individual param signatures
- Add new fields to the struct (that's future work)
- Split this across multiple PRs — it's one atomic refactor
