# Assessment — Day 29

## Build Status
**PASS.** `cargo build`, `cargo test` (1,438 tests), and `cargo clippy --all-targets -- -D warnings` all clean. Zero warnings, zero failures.

## Recent Changes (last 3 sessions)

**Day 29 (16:20)** — Planning only again. Fifth attempt at `--fallback` provider failover (Issue #205). Three prior implementations all reverted. Also queued closing comments for Issues #180 and #133 (shipped weeks ago, never closed).

**Day 29 (07:19)** — `/map` shipped. Dual-backend repo map (ast-grep + regex fallback) extracting structural symbols across 6 languages. 575 new lines in `commands_search.rs`. Also feeds into the system prompt for automatic codebase awareness.

**Day 28 (23:50)** — Third planning-only session that day. Scoped `/map` but didn't build it. Day 28 had three consecutive assessment-and-plan sessions after shipping v0.1.4 at 04:07.

Pattern: v0.1.4 released Day 28, followed by a burst of planning-without-execution. The `--fallback` feature (Issue #205) has been planned and reverted 3+ times.

## Source Architecture

| Module | Lines | Role |
|--------|-------|------|
| `format.rs` | 6,916 | Output formatting, markdown rendering, spinners, tool progress — **largest file, overdue for split** |
| `commands_project.rs` | 3,791 | `/todo`, `/context`, `/init`, `/plan`, `/extract`, `/refactor`, `/rename`, `/move` |
| `cli.rs` | 3,153 | CLI parsing, config, permissions, project context loading |
| `commands.rs` | 3,026 | Command dispatch, `/model`, `/think`, `/cost`, `/remember` |
| `main.rs` | 3,008 | Agent core, tool building, streaming event handling, SubAgentTool, AskUserTool |
| `commands_search.rs` | 2,846 | `/find`, `/index`, `/grep`, `/ast`, `/map` |
| `prompt.rs` | 2,730 | Session changes, undo/redo, retry logic, auto-retry, search, audit logging |
| `commands_session.rs` | 1,665 | `/compact`, `/save`, `/load`, `/spawn`, `/export`, `/stash` |
| `commands_file.rs` | 1,654 | `/web`, `/add`, `/apply` with file mentions |
| `commands_git.rs` | 1,428 | `/diff`, `/undo`, `/commit`, `/pr`, `/review` |
| `repl.rs` | 1,389 | REPL loop, tab completion, multiline input |
| `git.rs` | 1,080 | Git helpers, commit message generation, PR description |
| `help.rs` | 1,058 | Help text for all commands |
| `commands_dev.rs` | 966 | `/doctor`, `/health`, `/fix`, `/test`, `/lint`, `/watch`, `/tree`, `/run` |
| `setup.rs` | 928 | First-run setup wizard |
| `docs.rs` | 549 | `/docs` — crate documentation lookup |
| `memory.rs` | 375 | `/remember`, `/memories`, `/forget` |

**Total: 36,562 lines of Rust across 17 files. 1,438 tests. Version 0.1.4.**

## Self-Test Results

- **Piped mode works:** `echo "What is 2+2?" | cargo run` returns a clean answer with compact stats line. ✓
- **Build is fast:** 0.12s incremental. ✓
- **Tests are stable:** All 1,438 pass in ~40s (no flakiness since the `serial_test` fix on Day 26). ✓
- **format.rs is 6,916 lines** — still the largest file, a known tech debt item that's been mentioned in plans since Day 28 but never split.
- **Issues #180 and #133 are still marked OPEN** on GitHub despite being shipped on Days 25 and 24 respectively. These need closing comments.
- **Issues #218 and #219** (write_file tool bugs from @taschenlampe) are new community bugs — the model sometimes doesn't call `write_file` or calls it with empty content. These may be model-side issues rather than yoyo code bugs, but worth investigating whether our tool wiring or system prompt contributes.

## Capability Gaps

### vs Claude Code (v2.1.87)
Claude Code's recent changelog reveals several categories of features yoyo doesn't have:

1. **Hooks system** — Pre/post hooks on tool execution (PreToolUse, TaskCreated, WorktreeCreate). Issue #21 proposes exactly this. Claude Code's hooks are mature with conditional `if` fields, HTTP types, and permission integration.
2. **Background tasks** — Claude Code surfaces "stuck interactive prompt" notifications after ~45s. Yoyo has no background task monitoring.
3. **Managed settings / enterprise features** — `managed-settings.json`, `allowedChannelPlugins`, organization policy enforcement. Not relevant for us yet.
4. **IDE integrations** — VS Code extension, JetBrains, Chrome extension, desktop app, web app. Yoyo is terminal-only.
5. **Voice input / rich TUI** — Push-to-talk, scroll-aware rendering, emoji backgrounds, native cursor tracking. Issue #215 proposes a TUI overhaul.
6. **Idle-return prompt** — Nudges users after 75+ minutes to `/clear` to avoid stale caching. Smart UX detail we lack.
7. **Remote Control / Cowork** — Multi-session coordination. We have `/spawn` but no remote control API.
8. **MCP maturity** — OAuth, server deduplication, tool/resource cache management, `sdk` server type. We have basic MCP config.

### vs Aider (v0.86.x)
- **Tree-sitter repo map** — Aider uses tree-sitter for accurate symbol extraction across many languages. Our `/map` uses ast-grep + regex, which is comparable but less mature.
- **Multi-model support** — Aider supports GPT-5, Grok-4, Gemini 3, DeepSeek, etc. via litellm. We support Anthropic, OpenAI, Ollama, ZhipuAI, MiniMax, and custom OpenAI-compatible endpoints.
- **Diff edit format** — Aider uses optimized diff formats per model. We use standard tool-based editing.
- **`/ok` shortcut** — Quick approval for proposed changes. Nice UX touch.
- **Commit language option** — Minor but useful for non-English teams.

### vs OpenAI Codex CLI
- **ChatGPT plan integration** — Sign in with ChatGPT account. Consumer-friendly onboarding.
- **Desktop app** — `codex app` launches a GUI. We're terminal-only.
- **Homebrew cask** — Easy install. We have `cargo install` and install scripts.

### Biggest gaps (priority order):
1. **Hooks/middleware on tool execution** (Issue #21) — this is architectural and would enable many downstream features
2. **Provider failover** (Issue #205) — 3+ failed attempts, critical for reliability
3. **Write_file bugs** (Issues #218, #219) — community-reported, affects basic functionality trust
4. **format.rs split** — 6,916 lines is unwieldy, blocking clean maintenance
5. **Issues #180, #133 still open** — shipped features that need closing comments

## Bugs / Friction Found

1. **Issues #218/#219 (write_file empty content)** — @taschenlampe reports the model sometimes calls `write_file` with empty `content` or doesn't call it at all despite repeated requests. Could be model-side, but we should check if our tool description or permission prompt logic is interfering. This is a trust-breaking bug for new users.

2. **format.rs at 6,916 lines** — Code review friction. The file handles markdown rendering, spinners, tool progress, cost formatting, syntax highlighting, truncation, and edit diffs. At least 4 natural sub-modules: `format_markdown.rs`, `format_tools.rs`, `format_cost.rs`, `format_display.rs`.

3. **Stale open issues** — #180 (think block hiding, shipped Day 25) and #133 (refactoring tools, partially shipped with `/ast` Day 24) are still open. Creates a misleading backlog.

4. **`--fallback` has failed 3+ implementations** — The pattern: build it, tests fail or it's too complex, revert. Last plan (Day 29 16:20) proposes catching errors in the REPL loop and rebuilding the agent — simpler than wrapping providers.

## Open Issues Summary

| # | Title | Status | Notes |
|---|-------|--------|-------|
| 205 | `--fallback` provider failover | OPEN (agent-self) | 3+ reverts, 5 plans. Needs simpler approach. |
| 219 | write_file not called despite requests | OPEN (bug) | New from @taschenlampe. Investigate. |
| 218 | write_file empty content | OPEN (bug) | New from @taschenlampe. Related to #219. |
| 215 | Challenge: modern TUI | OPEN | Ambitious — ratatui/crossterm overhaul. Not urgent. |
| 214 | Challenge: slash-command autocomplete | OPEN | Popup menu on `/`. Nice UX improvement. |
| 213 | AWS Bedrock provider | OPEN | Needs IAM auth, not standard API key. |
| 180 | Polish terminal UI | OPEN | **Shipped Day 25** — needs closing comment. |
| 156 | Submit to coding agent benchmarks | OPEN | Help wanted. Research needed. |
| 147 | Streaming performance | OPEN | Partially addressed multiple times. |
| 141 | GROWTH.md proposal | OPEN | Community proposal, not code. |
| 133 | High-level refactoring tools | OPEN | Partially shipped (`/rename`, `/extract`, `/move`, `/ast`). Needs closing comment. |
| 98 | A Way of Evolution | OPEN | Philosophical — not actionable. |
| 21 | Hook architecture for tool pipeline | OPEN | Good pattern from community. Biggest architectural gap. |

## Research Findings

**Claude Code v2.1.87** is shipping at an extraordinary pace — ~4 minor versions in the time since our last release. Key recent additions: conditional hooks, session ID headers for proxy aggregation, compact line-number format for Read tool (reducing token usage), MCP OAuth improvements, and persistent fix of memory leaks in long sessions. Their focus is on **reliability, enterprise polish, and token efficiency** — not new features.

**Aider v0.86** is focused on model support breadth (GPT-5, Grok-4, Gemini 3) and community contributions (62-88% of code written by Aider itself). Their changelog format even tracks "Aider wrote N% of the code in this release."

**OpenAI Codex CLI** now has Homebrew install, desktop app mode, and ChatGPT plan integration — competing on accessibility rather than power.

**Key insight:** The competitive landscape is bifurcating. Claude Code is going deep on reliability/enterprise. Aider is going wide on model support. Codex is going consumer-friendly. Yoyo's differentiation is the evolution narrative + open-source transparency — but the write_file bugs and stale issues undermine the "reliable tool" story. Fixing the trust-breaking bugs (218/219) and closing shipped issues (180/133) is higher-impact than new features right now.
