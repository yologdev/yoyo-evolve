## Session Plan

### Task 1: Remove benchmarks directory (Issue #155)
Files: benchmarks/offline.sh, benchmarks/README.md, CLAUDE_CODE_GAP.md
Description: Delete the entire `benchmarks/` directory (offline.sh and README.md). Update CLAUDE_CODE_GAP.md to remove any references to the benchmarks. This is a clean removal — we'll rely on official leaderboards (SWE-bench, etc.) instead of maintaining our own benchmark suite. Also close #17 as noted in the issue.
Issue: #155

### Task 2: Rewrite architecture docs from mermaid diagrams to design rationale (Issue #154)
Files: docs/src/architecture.md, docs/mermaid-init.js
Description: Replace the current architecture.md (229 lines of mermaid dependency diagrams that duplicate what DeepWiki auto-generates) with prose about architectural *reasoning*: why 13 modules instead of 3, why the layered design, why format.rs is the largest file, what trade-offs were made, what invariants the code relies on. Keep the page useful for contributors who want to understand the codebase without reading 23k lines. Remove docs/mermaid-init.js since it was only needed for the mermaid diagrams. Keep the page roughly the same length but make every line earn its place by explaining *why*, not *what*. Update docs/book.toml if it references mermaid-init.js.
Issue: #154

### Task 3: Streaming performance investigation and improvement (Issue #147)
Files: src/prompt.rs, src/format.rs
Description: Investigate and improve streaming latency. The core issue is the gap between token arrival and display. Three areas to investigate: (1) The MarkdownRenderer's `render_delta()` buffers tokens at line boundaries for fence/header detection — audit whether the buffering window is minimal or could be tightened. (2) Check if `io::stdout().flush()` is called after every delta or batched. (3) Profile whether the spinner stop sequence introduces visible delay before first token appears. The goal is to reduce perceived latency between the API sending a token and the user seeing it. Write tests for any rendering changes. Add a `render_latency_budget` comment documenting the expected flush behavior so future changes don't regress.
Issue: #147

### Task 4: Bash tool live output streaming
Files: src/prompt.rs, src/format.rs
Description: Improve the user experience during long-running bash commands. Currently, bash tool output only appears after the command completes (via ToolExecutionEnd). The ToolExecutionUpdate events exist but show minimal partial results. Enhance the ToolExecutionUpdate handler in the event loop to: (1) Show a running line count or progress indicator for commands that produce many lines of output. (2) Display the last few lines of partial output in real-time (dimmed, updating) so users can see build progress, test output, etc. as it happens instead of staring at a spinner. (3) Add a time indicator showing how long the current tool has been running (e.g., "bash ⏱ 12s"). This is the single biggest experiential gap vs Claude Code — when `cargo test` runs for 30 seconds, Claude Code users see live output; yoyo users see nothing. Write tests for any new formatting functions.
Issue: none

### Issue Responses
- #155: Implementing as Task 1 — removing benchmarks/ directory entirely. Will also close #17 as suggested. Official leaderboards like SWE-bench are the right approach. 🐙
- #154: Implementing as Task 2 — replacing mermaid diagrams with design rationale prose. DeepWiki handles the "what calls what" automatically; architecture docs should explain *why* the code is shaped the way it is.
- #147: Implementing as Task 3 — doing actual streaming latency investigation this session. I've commented three times saying "it's on my list" and it's time to stop saying that and start profiling. Also Task 4 addresses the live bash output gap which is the biggest remaining UX issue.
