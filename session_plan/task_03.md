Title: Add --parallel flag to /spawn for concurrent multi-task dispatch
Files: src/commands_spawn.rs, src/help_data.rs
Issue: none

## What to do

Add a `--parallel` flag to `/spawn` that allows dispatching multiple tasks concurrently. This closes the "parallel sub-agents" competitive gap identified in the assessment. Claude Code runs up to 1,000 sub-agents simultaneously; yoyo currently dispatches them one at a time.

### Syntax

```
/spawn --parallel "task one" "task two" "task three"
/spawn --parallel -m claude-sonnet-4-20250514 "task A" "task B"
```

Tasks are separated as quoted arguments. The `--parallel` flag triggers parallel dispatch mode.

### Implementation

1. **In `parse_spawn_args()`**: Detect `--parallel` flag. When present, parse all remaining quoted strings (or `---`-separated segments) as separate tasks. Return a new field in `SpawnArgs`:
   ```rust
   pub parallel_tasks: Option<Vec<String>>,  // None for normal spawn, Some for parallel
   ```

2. **In `handle_spawn()`**: When `args.parallel_tasks` is `Some(tasks)`:
   - Register all tasks in the tracker immediately
   - For each task, build the agent config (same model, same system prompt as main)
   - Use `tokio::task::JoinSet` to spawn all sub-agents concurrently
   - Each sub-agent runs as a background task (same as `--bg` but all launched together)
   - Print a summary showing all spawned task IDs
   - Return `None` (no context message needed — all tasks are background)

3. **Parsing approach**: After `--parallel`, collect tasks using one of:
   - Quoted strings: `"task one" "task two"` — split on quote boundaries
   - Triple-dash separator: `task one --- task two --- task three`
   
   Use the triple-dash approach since it's simpler to parse and more natural:
   ```
   /spawn --parallel fix the auth tests --- write docs for the parser --- add error handling to main
   ```

4. **Reuse existing infrastructure**: The `--bg` spawn logic already creates background tasks with `tokio::spawn`. Parallel mode is essentially "launch multiple --bg spawns at once." Reuse `handle_spawn_bg` internally for each task.

### Output format

```
🐙 spawning 3 parallel subagents...
  #1: fix the auth tests
  #2: write docs for the parser  
  #3: add error handling to main

Use /spawn status to check progress.
Use /spawn collect <id> to retrieve results.
```

### Tests to add

- `test_parse_spawn_parallel_flag` — verify `--parallel` is detected
- `test_parse_spawn_parallel_tasks` — verify triple-dash splitting produces correct task list
- `test_parse_spawn_parallel_with_model` — verify `--parallel -m model` works with tasks
- `test_parse_spawn_parallel_no_tasks` — error when `--parallel` but no tasks given

### Help update

In `src/help_data.rs`, add to the spawn help entry:
```
/spawn --parallel <task1> --- <task2> [--- <task3>...]
                              Run multiple tasks concurrently
```

And add a note: "Parallel spawns launch all tasks as background agents simultaneously. Use /spawn status to monitor and /spawn collect <id> to retrieve results."

### Important constraints

- Each parallel task reuses the existing `handle_spawn_bg` infrastructure — no new agent-building logic needed
- Maximum parallel tasks: 10 (hard limit, print error if exceeded)
- The `--parallel` flag is incompatible with `--bg` (it's implicitly background) and `-o` (output goes to each task's default location)
- If `--parallel` is combined with `--model` or `--system`, those apply to ALL parallel tasks
