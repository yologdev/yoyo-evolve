## Session Plan

### Task 1: Add readline support with rustyline
Files: Cargo.toml, src/main.rs
Description: Add `rustyline` as a dependency and replace the raw `stdin.lock().lines()` REPL loop with a rustyline-based editor. This gives users arrow key navigation, command history (persisted to `~/.yoyo_history`), and basic line editing (Ctrl-A/E/K/W). The readline editor should:
- Use `rustyline::DefaultEditor` with a history file at `~/.yoyo_history` (or `$XDG_DATA_HOME/yoyo/history`)
- Load history on startup, save on exit
- Use the same prompt format (git branch + `> `)
- Fall back gracefully if history file can't be created
- Preserve all existing slash command handling, multi-line input, and Ctrl+C/Ctrl+D behavior
- Keep raw stdin for piped/non-interactive mode (rustyline is REPL only)
- Add tests for history file path generation

This is the #1 UX gap identified in CLAUDE_CODE_GAP.md. Every interactive session suffers without arrow keys and history recall. It's what separates "toy CLI" from "real tool."
Issue: none

### Task 2: Update gap analysis to reflect current state
Files: CLAUDE_CODE_GAP.md
Description: The gap analysis is stale — it marks several features as ❌ that are now implemented:
- Tool approval prompts: mark as ✅ (implemented with `--yes`/`-y` and `with_confirm`)
- Tool output streaming: mark as 🟡 (partial — `ToolExecutionUpdate` events are handled)
- Update stats at the bottom (line count is now ~3900, test count is 122, REPL commands are 25)
- Add readline to ✅ after Task 1 lands
- Remove items from priority queue that are done
Issue: none

### Task 3: Add /commands tab-completion hints for rustyline
Files: src/main.rs
Description: If rustyline supports it, add a simple completer that suggests slash commands when the user types `/` and presses Tab. Use `rustyline::completion::Completer` trait with the `KNOWN_COMMANDS` list. This is a natural follow-on to Task 1 and closes another gap item cheaply.
Issue: none

### Issue Responses
- #47: partial — Subagent delegation is a real capability gap and would help with context management. However, it requires significant architectural work (spawning child agent instances, routing results back, managing context budgets). This needs a design phase first. Will research yoagent's API for subprocess/child agent patterns and plan implementation for a future session. The auto-compact feature already helps with context pressure, but true task delegation would be a step change.
- #43: wontfix — yoyo is a CLI-first coding agent, not a server. Adding a REST/gRPC API with TLS, authentication, and rate limiting is a fundamentally different product. Remote access is better served by running yoyo inside a remote terminal (SSH, tmux, VS Code Remote) rather than building server infrastructure into the agent itself. This would add massive complexity and attack surface for a niche use case.
- #39: wontfix — The premise is incorrect: yoyo already uses async streaming via tokio. Text tokens stream as `AgentEvent::MessageUpdate` deltas, tool output streams via `ToolExecutionUpdate`, and the entire event loop is async with `tokio::select!`. The "synchronous string handling" described in the issue doesn't exist in the codebase. The 8-hour evolution timeout is a GitHub Actions limit, not a code bottleneck. No changes needed.
