Title: Parallel /spawn with --bg flag
Files: src/commands_spawn.rs, src/dispatch.rs, src/help.rs
Issue: none

## What

Add a `--bg` flag to `/spawn` that launches the sub-agent in a background tokio task and returns control to the user immediately, enabling parallel multi-agent execution. This closes the biggest competitive gap against Cursor (which has parallel multi-agent) that we can actually build as a local CLI.

## Current State

`/spawn` currently runs synchronously — `handle_spawn` is awaited inline in `dispatch.rs`, and the user waits for the entire sub-agent to finish before they can interact again. The `SpawnTracker` is already `Arc<Mutex<Vec<SpawnTask>>>` (thread-safe), and the `BackgroundJobTracker` in `commands_bg.rs` provides a proven pattern for background tokio tasks.

## Implementation

### In `commands_spawn.rs`:

1. **Add `--bg` flag parsing** to `parse_spawn_args`:
   - `/spawn --bg <task>` — launch in background
   - `/spawn collect <id>` — retrieve a finished background spawn's result
   - Keep `/spawn status` working (existing, already shows all spawns)

2. **Add `SpawnArgs.background: bool` field**

3. **Store `JoinHandle`** in `SpawnTracker`:
   - Add a `handles: Arc<std::sync::Mutex<HashMap<usize, tokio::task::JoinHandle<String>>>>` field
   - Add `store_handle(id, handle)` method
   - Add `try_collect(id) -> Option<String>` method that checks if a bg spawn is done via `is_finished()`, then retrieves result
   - Update `handle_spawn_status` to show `🔄 running (background)` for bg tasks

4. **Add `handle_spawn_bg` async function**:
   - Clone `agent_config`, `model`, relevant messages (for context prompt)
   - Register task in tracker as `SpawnStatus::Running`
   - Build agent config and context prompt (same as current sync path)
   - `tokio::spawn` the sub-agent work:
     - Inside task: run the prompt, on completion update tracker status to `Completed`, store result
     - If `-o <path>` was specified, also write output file inside the task
   - Print "🐙 spawning subagent #N in background..." and return `None` (no result to inject yet)
   - Note: `session_total` (Usage) tracking for bg spawns — skip it (acceptable for bg, avoids &mut sharing)

5. **Add `handle_spawn_collect` function**:
   - `/spawn collect <id>` checks if spawn #id is finished
   - If finished: returns the formatted result string (same as `format_spawn_result`)
   - If still running: print status message, return None
   - If no such id: error message

6. **Modify `handle_spawn`** to check `args.background`:
   - If `true`: call `handle_spawn_bg` instead of the sync path
   - If `false`: existing behavior unchanged

### In `dispatch.rs`:

7. **Handle bg spawn returns** — when `handle_spawn` returns `None` for a `--bg` spawn, don't try to run a follow-up prompt. The current code already checks `if let Some(context_msg) = ...`, so bg spawns that return `None` will naturally skip the prompt injection. The `/spawn collect <id>` path should inject the result and run a prompt, same as sync spawn.

### In `help.rs`:

8. **Update `/spawn` help entry** in `command_help()`:
   - Add `--bg` flag description
   - Add `collect <id>` subcommand description
   - Add examples: `/spawn --bg analyze test coverage`, `/spawn collect 1`

### Tests to add (in `commands_spawn.rs`):
- `test_parse_spawn_args_bg_flag` — `--bg` flag parsed correctly
- `test_parse_spawn_args_bg_with_output` — `--bg -o out.txt task`
- `test_parse_spawn_collect` — parsing `/spawn collect 3`
- `test_spawn_tracker_store_handle` — store and check handle exists
- `test_spawn_status_display_bg` — background tasks show distinct status

### What NOT to do:
- Don't change the default behavior — `/spawn <task>` stays synchronous
- Don't try to auto-inject results when bg spawns finish (too complex, save for later)
- Don't modify `session_total` tracking for bg spawns
- Don't add notification when bg spawn finishes (nice-to-have, future task)
