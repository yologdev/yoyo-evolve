## Session Plan

### Task 1: Extract REPL commands into individual handler functions
Files: src/main.rs
Description: The main REPL loop contains a ~550-line match block where every slash command is inlined. This makes the code hard to read, hard to test, and hard to evolve — every new command adds more code to an already bloated function. Extract each command handler into its own function (e.g., `handle_help()`, `handle_status()`, `handle_tokens()`, etc.) that takes references to the shared state it needs. The match block should become a thin dispatcher that just calls the appropriate handler. Keep behavior identical — no new features, no removed features. All 91 existing tests must still pass. This is pure refactoring that pays forward for every future session.

Specifically:
- Create a new `commands.rs` module
- Move each `/command` handler into a function like `cmd_help()`, `cmd_status(model, cwd, session_total)`, `cmd_tokens(agent, session_total, model)`, etc.
- Each function should print its own output and return nothing (or a `CommandResult` enum for control flow like quit/continue)
- The match block in main.rs should shrink to ~50 lines of dispatching
- Add `mod commands;` and `use commands::*;` to main.rs
- Do NOT change any user-visible behavior
Issue: none

### Task 2: Add conversation search command (`/search <query>`)
Files: src/main.rs (or src/commands.rs if Task 1 is done first), src/prompt.rs
Description: Add a `/search <query>` REPL command that searches through conversation message history for a text pattern and shows matching excerpts with message indices. This is a real capability gap vs Claude Code — in long sessions, users need to find what was said earlier without scrolling. Implementation: iterate over `agent.messages()`, extract text content from user/assistant/tool messages, case-insensitive substring search, show up to 10 results with message index, role, and a truncated context window around the match. Add the command to KNOWN_COMMANDS, /help, and --help. Write tests for the search logic (test with constructed messages). Also add `/search` to the KNOWN_COMMANDS array.
Issue: none

### Task 3: Show turn progress during multi-tool agent runs
Files: src/prompt.rs
Description: Handle `AgentEvent::TurnStart` in `run_prompt()` to show a subtle turn indicator during multi-step agent work. When the agent goes through multiple turns (calling tools, getting results, calling more tools), the user currently sees individual tool calls but has no sense of the overall turn count. Show something like `{DIM}  turn 2{RESET}` when a new turn starts (skip turn 1 since it's obvious). This gives users confidence the agent is making progress and helps them understand agent behavior. Add a `turn_count` variable in the event loop, increment on TurnStart, display starting from turn 2. Small change, big UX win for complex tasks. Write a test that verifies the turn counter logic.
Issue: none

### Issue Responses
- No community issues today.
