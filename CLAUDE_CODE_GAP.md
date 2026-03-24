# Gap Analysis: yoyo vs Claude Code

Last updated: Day 24 (2026-03-24)

This document tracks the feature gap between yoyo and Claude Code, used to inform development priorities when there are no community issues to address.

## Legend
- ✅ **Implemented** — yoyo has this
- 🟡 **Partial** — yoyo has a basic version, Claude Code's is better
- ❌ **Missing** — yoyo doesn't have this yet

---

## Core Agent Loop

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Streaming text output | ✅ | ✅ | True token-by-token streaming — mid-line tokens render immediately, line-start briefly buffers for fence/header detection (Day 17, fixed line-buffering bug); streaming flush improvements (Day 23) |
| Tool execution | ✅ | ✅ | bash, read_file, write_file, edit_file, search, list_files, rename_symbol (Day 23) |
| Multi-turn conversation | ✅ | ✅ | Both maintain conversation history |
| Thinking/reasoning display | ✅ | ✅ | yoyo shows thinking dimmed |
| Error recovery / auto-retry | ✅ | ✅ | yoagent retries 3x with exponential backoff by default |
| Subagent / task spawning | 🟡 | ✅ | Basic `/spawn` runs tasks in separate context; Claude Code has richer orchestration |
| Tool output streaming | 🟡 | ✅ | `ToolExecutionUpdate` events handled; markdown streaming fixed (Day 17); no real-time subprocess streaming yet |

## CLI & UX

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Interactive REPL | ✅ | ✅ | |
| Piped/stdin mode | ✅ | ✅ | Improved piped mode handling (Day 23) |
| Single-shot prompt (-p) | ✅ | ✅ | |
| Output to file (-o) | ✅ | ✅ | |
| Model selection | ✅ | ✅ | --model flag and /model command |
| Session save/load | ✅ | ✅ | /save, /load, --continue |
| Git integration | ✅ | ✅ | Branch in prompt, /diff, /undo; git-aware system prompt gives agent branch/dirty state automatically |
| Readline / line editing | ✅ | ✅ | rustyline: arrow keys, history (~/.local/share/yoyo/history), Ctrl-A/E/K/W |
| Tab completion | ✅ | ✅ | Slash commands, file paths, and argument-aware completion (--model values, git subcommands, /pr subcommands) (Day 14) |
| Fuzzy file search | ✅ | ✅ | `/find` with scoring, git-aware file listing, top-10 ranked results (Day 12) |
| Syntax highlighting | ✅ | ✅ | Language-aware ANSI highlighting for Rust, Python, JS/TS, Go, Shell, C/C++, JSON, YAML, TOML |
| Markdown rendering | ✅ | ✅ | Incremental ANSI: headers, bold, code blocks, inline code, syntax-highlighted code blocks |
| Progress indicators | ✅ | ✅ | Braille spinner animation during AI responses (Day 8) |
| Multi-line input | ✅ | ✅ | Backslash continuation and code fences |
| Image input support | ✅ | ✅ | `/add` reads images as base64; `--image` flag for CLI; auto-detects png/jpg/gif/webp/bmp (v0.1.1) |
| Custom system prompts | ✅ | ✅ | --system, --system-file, plus config file `system_prompt`/`system_file` keys (Day 23) |
| Extended thinking control | ✅ | ✅ | --thinking flag |
| Color control | ✅ | ✅ | --no-color, NO_COLOR env |
| Edit diff display | ✅ | ✅ | Colored inline diffs for `edit_file` tool output — red/green removed/added lines (Day 14) |
| Inline @file mentions | ✅ | ✅ | `@path` in prompts expands to file contents; supports line ranges `@file:10-20` and images (Day 21) |
| Conversation bookmarks | ✅ | ❌ | `/mark`, `/jump`, `/marks` — name points in conversation and jump back (Day 14) |
| First-run onboarding | ✅ | ✅ | Detects first run, shows welcome message, guides API key and model configuration (Day 22) |
| Terminal bell notifications | ✅ | ✅ | Bell on long completions; --no-bell flag and YOYO_NO_BELL env to disable (Day 23) |
| Conversation stash | ✅ | ❌ | `/stash` saves/restores conversation context without files (Day 22) |
| File patch application | ✅ | ❌ | `/apply` applies unified diff patches to files (Day 23) |
| AST structural search | ✅ | ❌ | `/ast` searches code by structure using tree-sitter patterns (Day 23) |
| Auto-test watcher | ✅ | ❌ | `/watch` auto-runs tests on file changes (Day 23) |
| Refactoring umbrella | ✅ | ❌ | `/refactor` with subcommands: rename, extract, move (Day 23) |

## Context Management

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Proactive context compaction | ✅ | ✅ | Proactive at 70% + auto-compact at 80% context (Day 23, upgraded from auto-only) |
| Manual compaction | ✅ | ✅ | /compact command |
| Token usage display | ✅ | ✅ | /tokens with visual bar |
| Cost estimation | ✅ | ✅ | Per-request and session totals |
| Context window awareness | ✅ | ✅ | 200k token limit tracked |

## Permission System

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Tool approval prompts | ✅ | ✅ | `--yes`/`-y` to auto-approve; interactive confirm for bash, write_file, and edit_file; "always" persists per-session (Day 15) |
| Allowlist/blocklist | ✅ | ✅ | `--allow`/`--deny` flags with glob matching; `[permissions]` config section; deny overrides allow |
| Directory restrictions | ✅ | ✅ | `--allow-dir`/`--deny-dir` flags + `[directories]` config; canonicalized path checks prevent traversal (Day 14) |
| Auto-approve patterns | ✅ | ✅ | `--allow` glob patterns + config file `allow` array; "always" option during confirm |

## Project Understanding

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Project context files | ✅ | ✅ | yoyo reads YOYO.md, CLAUDE.md, and .yoyo/instructions.md |
| Auto-detect project type | ✅ | ✅ | `detect_project_type` used by `/test`, `/lint`, `/health`, `/fix` (Rust, Node, Python, Go, Make) |
| Project scaffolding | ✅ | ✅ | `/init` scans project and generates a YOYO.md context file (Day 13) |
| Git-aware file selection | ✅ | ✅ | `get_recently_changed_files` appended to project context (Day 12) |
| Git-aware system prompt | ✅ | ✅ | Agent always sees current branch and dirty state in system prompt (Day 23) |
| Codebase indexing | ✅ | ✅ | `/index` builds lightweight project index: file count, language breakdown, key files (Day 14) |

## Developer Workflow

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Run tests | ✅ | ✅ | `/test` auto-detects project type and runs tests (Day 12) |
| Auto-fix lint errors | ✅ | ✅ | `/lint` auto-detects and runs linter; `/fix` sends failures to AI (Day 9+12) |
| PR description generation | ✅ | ✅ | `/pr create [--draft]` generates AI-powered PR descriptions |
| Commit message generation | ✅ | ✅ | `/commit` with heuristic-based message generation from staged diff (Day 8) |
| Code review | ✅ | ✅ | `/review` provides AI-powered code review of staged/unstaged changes (Day 13) |
| Multi-file refactoring | ✅ | ✅ | `/refactor` umbrella command (rename, extract, move); `rename_symbol` agent tool for cross-project renames (Day 23) |

## Configuration

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Config file | ✅ | ✅ | yoyo reads .yoyo.toml and ~/.config/yoyo/config.toml |
| Per-project settings | ✅ | ✅ | .yoyo.toml in project directory |
| Custom tool definitions | ✅ | ✅ | yoyo supports MCP servers via `--mcp` (stdio transport) |
| Multi-provider support | ✅ | ❌ | yoyo supports 12 providers via `--provider` (anthropic, openai, google, ollama, etc.) |
| Skills/plugins | ✅ | ✅ | yoyo has --skills; Claude Code has MCP |
| OpenAPI tool support | ✅ | ❌ | `--openapi <spec>` loads OpenAPI specs and registers API tools (Day 9) |
| Config system_prompt/system_file | ✅ | ✅ | `system_prompt` and `system_file` keys in .yoyo.toml for persistent custom prompts (Day 23) |

## Error Handling

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| API error display | ✅ | ✅ | Shows error messages |
| Network retry | ✅ | ✅ | yoagent handles 3 retries with exponential backoff by default |
| Rate limit handling | ✅ | ✅ | yoagent respects retry-after headers on 429s |
| Context overflow recovery | ✅ | ✅ | Auto-compacts conversation and retries on context overflow errors (Day 20) |
| Graceful degradation | 🟡 | ✅ | Retry logic, error handling, context overflow recovery; not yet full fallback on partial tool failures |
| Ctrl+C handling | ✅ | ✅ | Both handle interrupts |

---

## Priority Queue (what to build next)

Based on this analysis, the highest-impact remaining gaps are:

1. **Richer subagent orchestration** — Better task decomposition and result aggregation for /spawn
2. **Full graceful degradation** — Fallback behavior on partial tool failures
3. **Real-time subprocess streaming** — Stream bash tool output as it runs, not after completion

Recently completed:
- ✅ `/refactor` umbrella + `rename_symbol` agent tool (Day 23) — multi-file refactoring with rename, extract, move subcommands
- ✅ `/watch` auto-test watcher (Day 23) — auto-runs tests on file changes
- ✅ `/ast` structural search (Day 23) — tree-sitter pattern matching for code structure
- ✅ `/apply` patch application (Day 23) — applies unified diff patches to files
- ✅ `/stash` conversation stash (Day 22) — save/restore conversation context
- ✅ Terminal bell notifications (Day 23) — bell on long completions, --no-bell to disable
- ✅ Config `system_prompt`/`system_file` keys (Day 23) — persistent custom prompts in .yoyo.toml
- ✅ Git-aware system prompt (Day 23) — agent always sees branch and dirty state
- ✅ Proactive context compaction (Day 23) — triggers at 70%, auto-compact at 80%
- ✅ Streaming flush improvements (Day 23) — more responsive token rendering
- ✅ Piped mode improvements (Day 23) — better non-interactive input handling
- ✅ First-run welcome & guided setup (Day 22) — detects first run, shows welcome message, guides API key and model config
- ✅ `/diff` visual enhancement (Day 22) — inline colored patches with +/- line highlighting in diff output
- ✅ Inline @file mentions (Day 21) — `@path` in prompts expands to file contents; supports line ranges and images
- ✅ Context overflow auto-recovery (Day 20) — auto-compacts conversation and retries on overflow errors
- ✅ Image input support (v0.1.1) — `/add` reads images as base64; `--image` flag; auto-detects png/jpg/gif/webp/bmp
- ✅ True token-by-token streaming (Day 17) — fixed line-buffering bug; mid-line tokens now render immediately
- ✅ Parallel tool execution (Day 15) — supported via yoagent 0.6's `ToolExecutionStrategy::Parallel`

## Stats

- yoyo: ~32,536 lines of Rust across 14 source files + integration tests
- 1,380 tests passing (1,299 unit + 81 integration)
- 58 REPL commands (including /watch, /ast, /refactor, /apply, /stash, /rename, /move, /spawn, /find, /docs, /fix, /lint, /pr, /review, /init, /mark, /jump, /marks, /index, /changes, /web, /add, /plan, /run, /tree, /memories, /export, /grep, /help)
- 27 CLI flags (+ short aliases)
- 12 provider backends (including z.ai, cerebras, custom)
- **Published:** v0.1.3 on crates.io (`cargo install yoyo-agent`)
- MCP server support
- OpenAPI tool loading
- Config file support (.yoyo.toml)
- Permission system (allow/deny globs + interactive prompts for all tools)
- Directory restrictions (allow-dir/deny-dir)
- Subagent spawning (/spawn)
- Fuzzy file search (/find)
- Git-aware project context + git-aware system prompt
- Syntax highlighting for 8+ languages
- Conversation bookmarks (/mark, /jump, /marks)
- Codebase indexing (/index)
- Argument-aware tab completion
- Inline @file mentions with line ranges and image support
- Image input support (base64 encoding for png/jpg/gif/webp/bmp)
- Context overflow auto-recovery
- First-run welcome & guided setup
- Inline colored diff patches
- `/refactor` umbrella (rename, extract, move) + `rename_symbol` agent tool
- `/watch` auto-test watcher
- `/ast` structural code search
- `/apply` patch application
- `/stash` conversation stash
- Terminal bell notifications
- Config `system_prompt`/`system_file` keys
- Proactive context compaction (70% + 80%)
