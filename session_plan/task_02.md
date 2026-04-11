Title: Add /drop command for selective message pruning
Files: src/commands_session.rs, src/repl.rs, src/commands.rs
Issue: none (competitive gap: "No context editing / message pruning" vs Claude Code)

## Problem

Claude Code can surgically edit conversation context — removing specific messages to free up context window space. yoyo only has `/compact` which summarizes everything into a single message. There's no way to selectively remove a specific turn or range of turns from the conversation without compacting the entire history.

## What to Build

A `/drop` command with these forms:
- `/drop last` — remove the last assistant+user turn pair (most common use case: "that response was unhelpful, forget it")
- `/drop N` — remove turn pair at position N (1-indexed, where 1 = first user+assistant exchange)
- `/drop N-M` — remove a range of turn pairs (inclusive)
- `/drop` (no args) — show the current message count and usage hint

Each "turn" is conceptually a user message + assistant response pair. The command should:
1. Parse the argument to determine which turn pairs to remove
2. Filter them out of `agent.messages()`
3. Call `agent.replace_messages(filtered)` with the remaining messages
4. Print a summary: "Dropped N turns (X → Y messages, ~Z tokens freed)"

Use `total_tokens()` from `format/cost.rs` to calculate token savings before/after.

## Implementation

### In `commands_session.rs`:
Add `pub fn handle_drop(agent: &mut Agent, args: &str)`:
- Parse args: "last", single number, or "N-M" range
- Get `agent.messages().to_vec()` 
- Identify turn boundaries: walk the messages and group them into turns (each turn = consecutive user messages + consecutive assistant messages)
- Calculate which turn indices to remove based on the parsed args
- Filter out those messages
- Call `agent.replace_messages(filtered)`
- Print summary with before/after message count and token delta

### In `repl.rs`:
Add `/drop` to the command dispatch — look for where `/compact` is dispatched and add `/drop` nearby. Route to `handle_drop(agent, args)`.

### In `commands.rs`:
Add `"/drop"` to the `KNOWN_COMMANDS` array (alphabetical order).
Add arg completions if appropriate (e.g., "last" as a completion for `/drop`).

### In `help.rs` (bonus — small addition):
Add help text for `/drop` in the command help function and the short description in the help listing.
Add "drop" to `help_command_completions`.

NOTE: This touches 4 files but help.rs changes are just adding strings. The core 3 files are commands_session.rs, repl.rs, commands.rs. If the 3-file limit is strict, help.rs can be deferred.

## Tests (in commands_session.rs)

Add unit tests:
- `test_drop_parse_args` — test parsing "last", "3", "2-5", "", invalid input
- Test the turn-boundary identification logic with mock message sequences
- Test that dropping preserves the correct remaining messages

Note: Full integration tests with a real Agent may not be possible in unit tests. Focus on testing the parsing and message filtering logic as pure functions, then wire them into handle_drop.

## Verification

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```
