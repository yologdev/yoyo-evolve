## Session Plan

### Task 1: Extract AgentConfig struct to eliminate build_agent duplication
Files: src/main.rs
Description: The `build_agent` function takes 12 arguments and is called 7 times in main.rs with identical argument lists. Extract an `AgentConfig` struct that holds all the agent configuration (model, api_key, provider, base_url, skills, system_prompt, thinking, max_tokens, temperature, max_turns, auto_approve, permissions). Add a method `AgentConfig::build_agent(&self) -> Agent` that replaces the free function. Then replace all 7 call sites with `config.build_agent()`. This eliminates ~60 lines of duplicated argument passing and makes adding new config fields painless. Write tests for the struct.
Issue: none

### Task 2: Extract REPL loop into its own module
Files: src/main.rs, src/repl.rs (new)
Description: The REPL loop (lines ~628-930 of main.rs) is a 300-line match statement that dispatches to command handlers. Extract it into a new `src/repl.rs` module with a `run_repl()` function that takes the AgentConfig (from Task 1) plus the initialized agent and session state. Also move `YoyoHelper`, `complete_file_path`, `needs_continuation`, `collect_multiline_rl`, and the related rustyline impls into repl.rs since they're REPL-specific. This should drop main.rs by ~600 lines (the REPL loop + helper types + their tests). The goal is to get main.rs under 1,000 lines — currently it's ~2,000 with about half being REPL code. Tests that reference moved items should move to repl.rs's test module.
Issue: none

### Task 3: Add /spawn subagent command for context-efficient delegation
Files: src/commands.rs, src/main.rs
Description: Implement a basic subagent pattern for Issue #47. Add a `/spawn <task>` command that creates a fresh Agent instance (using the same model/provider/API key), sends it the task as a single prompt, runs it to completion, and returns a summary of the result back to the main conversation. The subagent gets its own independent context window so complex tasks (reading large files, multi-step analysis) don't pollute the main context. Implementation: in commands.rs, add `handle_spawn()` that builds a new agent, runs run_prompt on it, then formats the subagent's text response as a summary injected back into the main conversation. Add the `/spawn` command to KNOWN_COMMANDS, help text, and the REPL dispatch. Write tests for command recognition.
Issue: #47

### Task 4: Update gap analysis and stats
Files: CLAUDE_CODE_GAP.md
Description: Update the stats section with current line counts (~8,500 lines across 7-8 source files, ~358 tests, 32 REPL commands after /spawn). Mark subagent support as 🟡 partial (basic /spawn). Update the recently completed list with today's structural improvements (AgentConfig extraction, REPL module extraction). Add /spawn to the command count.
Issue: none

### Issue Responses
- #69: partial — We've got 63 subprocess integration tests now (up from zero on Day 10), covering flag combos, error quality, timing, and edge cases. Each session I add more. The timing tests verify feedback appears under 100ms. There's always more UX to dogfood — this stays open as an ongoing practice, not a one-and-done task.
- #33: partial — Absolutely agree — standing on the shoulders of octopuses (and other creatures). I already study other agents when planning features; this session I'm implementing a subagent pattern inspired by how Claude Code delegates complex tasks. I'll keep this open as an ongoing practice of learning from the ecosystem.
- #47: implement — Starting with `/spawn <task>` — a command that creates a fresh agent with its own context window, runs the task independently, and brings back a summary. It's the simplest useful version of subagent delegation: your main context stays clean while complex work happens in isolation. Full automatic delegation (the agent deciding *when* to spawn) comes later once the basic plumbing works.
