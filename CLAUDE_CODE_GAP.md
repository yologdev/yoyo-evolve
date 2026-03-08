# Gap Analysis: yoyo vs Claude Code

Last updated: Day 8 (2026-03-08)

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
| Tab completion | ❌ | ✅ | Claude Code completes file paths, commands |
| Fuzzy file search | ❌ | ✅ | Claude Code can fuzzy-find files |
| Syntax highlighting | ❌ | ✅ | Claude Code highlights code in responses |
| Markdown rendering | ❌ | ✅ | Claude Code renders markdown nicely |
| Progress indicators | 🟡 | ✅ | yoyo shows tool names; Claude Code shows spinners |
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
| Allowlist/blocklist | ❌ | ✅ | Claude Code has configurable permissions |
| Directory restrictions | ❌ | ✅ | Claude Code can restrict file access |
| Auto-approve patterns | 🟡 | ✅ | yoyo has "always" option during confirm; no persistent pattern memory |

## Project Understanding

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Project context files | ✅ | ✅ | yoyo reads YOYO.md, CLAUDE.md, and .yoyo/instructions.md |
| Auto-detect project type | ❌ | ✅ | Claude Code detects language, framework, build system |
| Git-aware file selection | ❌ | ✅ | Claude Code prioritizes recently changed files |
| Codebase indexing | ❌ | ✅ | Claude Code indexes for faster search |

## Developer Workflow

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Run tests | 🟡 | ✅ | yoyo can via bash; Claude Code auto-detects test runner |
| Auto-fix lint errors | ❌ | ✅ | Claude Code can fix clippy/eslint warnings |
| PR description generation | ❌ | ✅ | Claude Code generates PR descriptions |
| Commit message generation | ❌ | ✅ | Claude Code suggests commit messages |
| Multi-file refactoring | 🟡 | ✅ | yoyo can via tools; Claude Code is better at coordinating |

## Configuration

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Config file | ✅ | ✅ | yoyo reads .yoyo.toml and ~/.config/yoyo/config.toml |
| Per-project settings | ✅ | ✅ | .yoyo.toml in project directory |
| Custom tool definitions | ❌ | ✅ | Claude Code supports MCP servers |
| Skills/plugins | ✅ | ✅ | yoyo has --skills; Claude Code has MCP |

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

1. **Syntax highlighting / markdown rendering** — Makes output much more readable
2. **Auto-detect project type** — Better default behavior
3. **Parallel tool execution** — Speed up multi-tool workflows
4. **Tab completion** — File paths and commands
5. **Allowlist/blocklist permissions** — Finer-grained safety controls

## Stats

- yoyo: ~4,000 lines of Rust across 4 source files
- 125 tests passing
- 25 REPL commands
- 16 CLI flags (+ short aliases)
- Config file support (.yoyo.toml)
