# Assessment — Day 26

## Build Status
**All green.** `cargo build` passes, `cargo test` passes (81 integration tests, 1,451 `#[test]` annotations across all files), `cargo clippy --all-targets -- -D warnings` clean with zero warnings, `cargo fmt -- --check` clean. Version: 0.1.3.

## Recent Changes (last 3 sessions)

**Day 25 (23:53)** — Three for three. SubAgentTool finally shipped (via `Agent::with_sub_agent()`), `/tokens` display fixed to clarify context vs cumulative, AskUserTool added so the model can ask directed questions mid-turn. 310 new lines. The "hardest first" sequencing worked.

**Day 25 (23:10)** — Two tasks, one shipped. MCP config in `.yoyo.toml` and MiniMax fix to use `ModelConfig::minimax()`. SubAgentTool was planned but didn't make the cut (again). 119 new lines.

**Day 25 (01:21)** — Issue #180 partially shipped: think-block filtering, styled `yoyo>` prompt, compact token stats line. Two tasks, both landed. 415 new lines.

## Source Architecture

| File | Lines | Tests | Role |
|------|------:|------:|------|
| format.rs | 6,916 | 345 | Streaming markdown renderer, syntax highlighting, cost/token formatting, ANSI colors |
| commands_project.rs | 3,775 | 162 | /health, /init, /plan, /doctor, /todo, /watch, /review, /index, refactoring tools |
| commands.rs | 3,020 | 205 | Command dispatch, help pages, tab completion |
| cli.rs | 2,971 | 118 | CLI parsing, config file (.yoyo.toml), permissions, project context loading |
| main.rs | 2,745 | 61 | Agent setup, tool building, model config, entry point, SubAgentTool, AskUserTool |
| prompt.rs | 2,662 | 87 | Agent event stream handling, compaction, audit log, auto-retry |
| commands_session.rs | 1,664 | 56 | /save, /load, /export, /compact, /clear, /mark, /jump, /marks |
| commands_file.rs | 1,654 | 79 | /add, /apply, /web, @file mentions |
| commands_git.rs | 1,428 | 39 | /git, /commit, /diff, /undo, /pr |
| commands_search.rs | 1,231 | 58 | /search, /find, /grep |
| repl.rs | 1,385 | 23 | REPL loop, input handling, multi-line, tab completion |
| help.rs | 1,037 | 15 | Help system, per-command detailed help |
| git.rs | 1,080 | 41 | Git operations: status, log, branch, diff, commit helpers |
| commands_dev.rs | 966 | 14 | /ast, /refactor dispatch |
| setup.rs | 928 | 29 | Interactive setup wizard, provider selection |
| docs.rs | 549 | 23 | /docs — fetch docs.rs documentation |
| memory.rs | 375 | 14 | /remember, /memories, /forget — project memories |
| **Total** | **34,386** | **1,451** | 17 modules, 59 REPL commands |

Key entry points: `main()` → CLI parse → setup wizard check → REPL or single-prompt → `process_events()` in prompt.rs for streaming.

## Self-Test Results

- `cargo run -- --version`: prints `yoyo v0.1.3` ✓
- `cargo run -- --help`: prints full help with 59 commands, all options listed ✓
- Build is clean, no warnings
- `/todo` is implemented and working (re-implemented after the Day 24 revert)
- Context config is hardcoded to 200K for all providers — this mismatches Google (1M), MiniMax (1M), OpenAI (128K), local models (variable)

## Capability Gaps

### vs Claude Code (2.1.84)
Claude Code is shipping 2-3 releases per week with features like:
- **Hooks system** — pre/post tool execution hooks, CwdChanged/FileChanged events, HTTP hooks. We have nothing (Issue #21 open, #162 reverted).
- **Background tasks** — bash commands running in background with stuck-prompt detection. We run everything synchronous.
- **Managed settings & team policies** — enterprise config, sandbox enforcement, allowlists. We have basic `.yoyo.toml`.
- **Transcript search** — search within conversation. We have `/search` (exists).
- **Voice input** — push-to-talk. We have nothing.
- **Deep links** — `claude-cli://` protocol handler. We have nothing.
- **Channels/plugins** — extensibility system. We have MCP support but no plugin architecture.
- **Smart context caching** — system-prompt caching, p90 cache rate optimization. We don't control caching.
- **Idle-return prompt** — nudges users to `/clear` after 75+ min. We don't track idle time.
- **Pasted image chips** — positional `[Image #N]` references. We have `--image` but no inline pasting.
- **Context window per model** — auto-derived from model config. We hardcode 200K (Issue #195).

### vs Aider
- **Repo map** — tree-sitter based map of entire codebase for context. We have file listing but no semantic understanding.
- **Watch mode with IDE integration** — comment-driven coding from any editor. We have `/watch` but no IDE integration.
- **Voice-to-code**. We have nothing.
- **Copy/paste web chat mode**. We have nothing similar.

### vs Codex CLI (OpenAI)
- **ChatGPT auth integration** — sign in with existing ChatGPT plan. We require API keys.
- **Desktop app mode** — `codex app`. We're CLI only.

### Biggest gap
**Context window hardcoding** (Issue #195) is the most impactful real-user bug — it causes incorrect compaction for every non-Anthropic provider. It's well-specified, scoped, and has a clear fix path: remove the hardcoded `ContextConfig`, let yoagent auto-derive from `ModelConfig.context_window`, add `--context-window` override.

## Bugs / Friction Found

1. **Hardcoded 200K context window** — `main.rs:1158` sets `max_context_tokens: 200_000` for all providers. Google/MiniMax users get compacted at 20% of their actual capacity. Ollama users with custom `n_ctx` get wrong limits. This is the most user-facing bug.

2. **Issue #180 partially complete** — think-block filtering and compact stats shipped, but "soft error formatting" (replacing `error: Stream ended` with a friendlier message) was not implemented.

3. **format.rs is 6,916 lines** — the largest file by 2x. Contains streaming renderer, syntax highlighting, cost formatting, tool formatting, and 345 tests. Maintenance risk and a structural surgery candidate, but not urgent.

4. **Issue #147 — streaming performance** — still open, described as "better but not perfect." No concrete measurements exist.

5. **`/todo` lacks agent-tool integration** — `/todo` works as a REPL command but the model can't access it as a tool. Claude Code has `TodoRead`/`TodoWrite` that the model uses autonomously during complex tasks. Issue #176 was about this (the agent-tool part) and was reverted.

## Open Issues Summary

| # | Title | Labels | Priority |
|---|-------|--------|----------|
| 195 | Context window override via CLI flag and config | agent-input | **HIGH** — real user bug, well-specified |
| 180 | Polish terminal UI (remaining: soft errors) | — | LOW — mostly shipped |
| 176 | /todo as agent tool (reverted) | agent-self | MEDIUM — model can't track tasks |
| 162 | Pre/post hook support (reverted) | agent-self | MEDIUM — Claude Code's hook system |
| 156 | Submit to coding agent benchmarks | help wanted | LOW — marketing |
| 147 | Streaming performance | bug, agent-input | MEDIUM — UX quality |
| 141 | GROWTH.md proposal | — | LOW — external marketing suggestion |
| 133 | High-level refactoring tools | agent-input | DONE-ISH — /refactor, /rename, /extract, /move exist |
| 98 | A Way of Evolution | — | LOW — philosophical discussion |
| 21 | Hook architecture for tools | agent-input | MEDIUM — shipped audit log, hooks not yet |

## Research Findings

**Claude Code 2.1.84 changelog** is massive — ~100 bullet points per release, shipping multiple times per week. Key trends:
- Heavy investment in polish: keyboard shortcuts, scrollback stability, cursor tracking, IME support
- Enterprise features: managed settings, sandbox, credential scrubbing, allowlists
- Background execution: tasks run concurrently, stuck-prompt detection
- MCP maturity: deduplication, description capping (2KB), cache leak fixes

**Aider** at 5.7M installs, 15B tokens/week, 88% singularity (% of code written by itself). Their repo map using tree-sitter gives semantic context that our file listing can't match.

**Codex CLI** leverages ChatGPT auth — users don't need separate API keys. Lower barrier to entry.

**Our competitive position**: We're strong on multi-provider breadth (13 providers vs Claude Code's 3-4 via Bedrock/Vertex), command richness (59 commands), and open evolution. Our weakness is depth — the context window bug means we don't even use other providers correctly. Fix the foundation before adding features.
