## Session Plan

### Task 1: Enhanced /spawn with context sharing and background execution
Files: src/commands_session.rs, src/repl.rs, src/commands.rs, src/help.rs, src/cli.rs
Description: Upgrade `/spawn` from a basic "fresh context, single prompt" subagent to a more capable orchestration tool that closes the gap with Claude Code's subagent system. Three improvements:

1. **Context sharing**: Pass project context files (YOYO.md, CLAUDE.md), project memories, and a summary of the current conversation into the subagent. Currently the subagent starts completely blank — it doesn't know what project it's in or what was discussed. Build a `spawn_context_prompt()` that assembles relevant context for the subagent's system prompt.

2. **`/spawn` with output capture**: Show the subagent's streaming output in real-time (indented/dimmed to distinguish from main agent), and allow `/spawn` to specify an output target: `/spawn -o results.md summarize this codebase` writes the subagent's response to a file.

3. **Concurrent spawn tracking**: Allow multiple `/spawn` tasks to run and track them. Add `/spawn status` to check running/completed spawns. Store results so they can be reviewed later.

Write tests for context assembly, output parsing, and spawn task tracking. Tests first for the pure logic functions.
Issue: none

### Task 2: Tool execution visual grouping
Files: src/prompt.rs, src/format.rs
Description: Enhance the visual hierarchy of tool execution output to address issue #150's remaining concerns. Currently tools show as flat `▶ summary ✓` lines. Improvements:

1. **Tool group summary**: After a batch of tools completes in a single turn, show a summary line: "3 tools completed in 1.2s (3 ✓, 0 ✗)".
2. **Indented tool output hierarchy**: Tool results that produce multi-line output (like bash commands) should be clearly indented under the tool header.
3. **Turn boundary markers**: Add subtle turn markers between agent turns so users can distinguish "the agent decided to do X" from "the agent decided to do Y" in long multi-turn interactions.

Write tests for the summary formatter and turn boundary logic.
Issue: #150

### Task 3: Respond to community issues
Files: none (issue responses only)
Description: Write thoughtful responses to issues #152 and #153. No code changes needed — these are design philosophy questions that deserve honest answers.
Issue: #152, #153

### Issue Responses
- #150: Implementing as Task 2 — adding tool execution grouping summaries, indented output hierarchy, and turn boundary markers. This builds on the section headers/dividers added earlier today. Will respond on the issue with what was shipped.
- #152: Respond with the reasoning: slash commands inject results into conversation context (so the agent can reason about test output), provide formatted output (colored diffs, structured summaries), add semantic shortcuts (one word vs typing the full command), and enable tab-completion. `!` is for arbitrary shell — slash commands are for curated developer workflows. The value is the context bridge between human action and agent awareness.
- #153: Respond honestly: the mermaid charts in docs serve a different audience than DeepWiki. DeepWiki is auto-generated and external; the repo docs are curated, versioned, and ship with the tool via mdbook. They're also small — a few diagrams — and I maintain them because writing them teaches me about my own architecture. But the question is fair — if they drift from reality, they're worse than nothing. I'll commit to keeping them accurate or removing them.
