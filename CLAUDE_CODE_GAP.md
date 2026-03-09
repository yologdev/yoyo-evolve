# Gap Analysis: yoyo vs Claude Code

Last updated: Day 9 (2026-03-09)

This document tracks the feature gap between yoyo and Claude Code, used to inform development priorities when there are no community issues to address.

## Legend
- ✅ **Implemented** — yoyo has this
- 🟡 **Partial** — yoyo has a basic version, Claude Code's is better
- ❌ **Missing** — yoyo doesn't have this yet

---

## Core Agent Loop

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Streaming text output | ✅ | ✅ | Both stream tokens as they arrive |
| Tool execution | ✅ | ✅ | bash, read_file, write_file, edit_file, search, list_files |
| Multi-turn conversation | ✅ | ✅ | Both maintain conversation history |
| Thinking/reasoning display | ✅ | ✅ | yoyo shows thinking dimmed |
| Error recovery / auto-retry | ✅ | ✅ | yoagent retries 3x with exponential backoff by default |
| Parallel tool execution | ❌ | ✅ | Claude Code can run multiple tools in parallel |
| Tool output streaming | 🟡 | ✅ | `ToolExecutionUpdate` events handled; no real-time subprocess streaming yet |

## CLI & UX

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Interactive REPL | ✅ | ✅ | |
| Piped/stdin mode | ✅ | ✅ | |
| Single-shot prompt (-p) | ✅ | ✅ | |
| Output to file (-o) | ✅ | ✅ | |
| Model selection | ✅ | ✅ | --model flag and /model command |
| Session save/load | ✅ | ✅ | /save, /load, --continue |
| Git integration | ✅ | ✅ | Branch in prompt, /diff, /undo |
| Readline / line editing | ✅ | ✅ | rustyline: arrow keys, history (~/.local/share/yoyo/history), Ctrl-A/E/K/W |
| Tab completion | 🟡 | ✅ | Slash commands + file paths; no argument-aware completion yet |
| Fuzzy file search | ❌ | ✅ | Claude Code can fuzzy-find files |
| Syntax highlighting | ❌ | ✅ | Claude Code highlights code in responses |
| Markdown rendering | 🟡 | ✅ | Incremental ANSI: headers, bold, code blocks, inline code; no syntax-aware highlighting yet |
| Progress indicators | ✅ | ✅ | Braille spinner animation during AI responses (Day 8) |
| Multi-line input | ✅ | ✅ | Backslash continuation and code fences |
| Custom system prompts | ✅ | ✅ | --system and --system-file |
| Extended thinking control | ✅ | ✅ | --thinking flag |
| Color control | ✅ | ✅ | --no-color, NO_COLOR env |

## Context Management

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Auto-compaction | ✅ | ✅ | Triggers at 80% context |
| Manual compaction | ✅ | ✅ | /compact command |
| Token usage display | ✅ | ✅ | /tokens with visual bar |
| Cost estimation | ✅ | ✅ | Per-request and session totals |
| Context window awareness | ✅ | ✅ | 200k token limit tracked |

## Permission System

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Tool approval prompts | ✅ | ✅ | `--yes`/`-y` to auto-approve; `with_confirm` for interactive bash approval |
| Allowlist/blocklist | ✅ | ✅ | `--allow`/`--deny` flags with glob matching; `[permissions]` config section; deny overrides allow |
| Directory restrictions | ❌ | ✅ | Claude Code can restrict file access |
| Auto-approve patterns | ✅ | ✅ | `--allow` glob patterns + config file `allow` array; "always" option during confirm |

## Project Understanding

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Project context files | ✅ | ✅ | yoyo reads YOYO.md, CLAUDE.md, and .yoyo/instructions.md |
| Auto-detect project type | 🟡 | ✅ | `detect_project_type` in `/health` and `/fix` (Rust, Node, Python, Go, Make); not yet used for auto-detecting test runner outside those commands |
| Git-aware file selection | ❌ | ✅ | Claude Code prioritizes recently changed files |
| Codebase indexing | ❌ | ✅ | Claude Code indexes for faster search |

## Developer Workflow

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Run tests | 🟡 | ✅ | yoyo can via bash; Claude Code auto-detects test runner |
| Auto-fix lint errors | 🟡 | ✅ | `/fix` runs checks, sends failures to AI for fixing (Day 9); not yet automatic like `clippy --fix` |
| PR description generation | ❌ | ✅ | Claude Code generates PR descriptions |
| Commit message generation | ✅ | ✅ | `/commit` with heuristic-based message generation from staged diff (Day 8) |
| Multi-file refactoring | 🟡 | ✅ | yoyo can via tools; Claude Code is better at coordinating |

## Configuration

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Config file | ✅ | ✅ | yoyo reads .yoyo.toml and ~/.config/yoyo/config.toml |
| Per-project settings | ✅ | ✅ | .yoyo.toml in project directory |
| Custom tool definitions | ✅ | ✅ | yoyo supports MCP servers via `--mcp` (stdio transport) |
| Multi-provider support | ✅ | ❌ | yoyo supports 10+ providers via `--provider` (anthropic, openai, google, ollama, etc.) |
| Skills/plugins | ✅ | ✅ | yoyo has --skills; Claude Code has MCP |
| OpenAPI tool support | ✅ | ❌ | `--openapi <spec>` loads OpenAPI specs and registers API tools (Day 9) |

## Error Handling

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| API error display | ✅ | ✅ | Shows error messages |
| Network retry | ✅ | ✅ | yoagent handles 3 retries with exponential backoff by default |
| Rate limit handling | ✅ | ✅ | yoagent respects retry-after headers on 429s |
| Graceful degradation | ❌ | ✅ | Claude Code falls back on partial failures |
| Ctrl+C handling | ✅ | ✅ | Both handle interrupts |

---

## Priority Queue (what to build next)

Based on this analysis, the highest-impact missing features are:

1. **Syntax-aware code highlighting** — Upgrade markdown rendering with language-specific highlighting in code blocks
2. **Parallel tool execution** — Speed up multi-tool workflows
3. **Argument-aware tab completion** — Complete --model values, file args for /load, etc.
4. **Git-aware file selection** — Prioritize recently changed files for context

Recently completed:
- ✅ OpenAPI tool support (Day 9) — `--openapi <spec>` loads specs and registers API tools
- ✅ yoagent 0.6.0 upgrade (Day 9) — updated to yoagent 0.6 with OpenAPI feature
- ✅ Permission system (Day 9) — `--allow`/`--deny` glob flags, `[permissions]` config, deny-overrides-allow
- ✅ Auto-fix lint errors (Day 9) — `/fix` command runs checks and sends failures to AI
- ✅ Project type detection (Day 9) — `detect_project_type` for Rust, Node, Python, Go, Make
- ✅ Commit message generation (Day 8) — `/commit` with heuristic-based message generation
- ✅ Progress indicators (Day 8) — braille spinner animation during AI responses
- ✅ Multi-provider support (Day 8) — 10+ providers via `--provider` flag
- ✅ MCP server support (Day 8) — connect to MCP servers via `--mcp`
- ✅ Markdown rendering (Day 8) — incremental ANSI formatting for streamed output
- ✅ Tab completion (Day 8) — slash commands + file path completion

## Stats

- yoyo: ~6,900 lines of Rust across 4 source files
- 235 tests passing
- 28 REPL commands
- 20 CLI flags (+ short aliases)
- 10+ provider backends
- MCP server support
- OpenAPI tool loading
- Config file support (.yoyo.toml)
- Permission system (allow/deny globs)
