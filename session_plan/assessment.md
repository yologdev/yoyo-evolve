# Assessment — Day 30

## Build Status

**All green.** `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt -- --check` all pass clean.

- **1,458 unit tests + 82 integration tests (1 ignored) = 1,540 total**
- Build: 0 warnings, 0 errors
- Clippy: 0 warnings

## Recent Changes (last 3 sessions)

1. **Day 30 09:35** — Wired BedrockProvider end-to-end in `build_agent()` and `create_model_config()` (Issue #223). Added inline command hints in REPL — type `/he` and a dimmed `lp — Show help` appears via rustyline's `Hinter`/`Highlighter` traits. 291 new lines.

2. **Day 30 08:20** — Added Bedrock to setup wizard, `KNOWN_PROVIDERS`, `known_models_for_provider`, and welcome text. Custom wizard flow for AWS credentials and region. 223 new lines. But the actual `BedrockProvider` construction wasn't wired yet (fixed in next session).

3. **Day 29 07:19** — `/map` shipped with dual backend (ast-grep for accurate AST extraction, regex fallback). 575 new lines in `commands_search.rs`. Repo map auto-feeds into system prompt for structural codebase awareness.

## Source Architecture

| File | Lines | Purpose |
|------|-------|---------|
| `commands_project.rs` | 3,791 | /todo, /init, /plan, /extract, /refactor, /rename, /move |
| `cli.rs` | 3,201 | CLI parsing, config, permissions, project context |
| `main.rs` | 3,097 | Agent core, streaming tools, event handling, tool wrappers |
| `commands.rs` | 3,026 | 60 REPL commands, model/provider switching, /remember |
| `commands_search.rs` | 2,846 | /find, /index, /grep, /ast-grep, /map, symbol extraction |
| `format/markdown.rs` | 2,837 | Streaming markdown renderer |
| `prompt.rs` | 2,730 | Prompt execution, retries, session changes, event loop |
| `commands_session.rs` | 1,665 | /compact, /save, /load, /spawn, /export, /stash |
| `commands_file.rs` | 1,654 | /web, /add, /apply |
| `repl.rs` | 1,500 | REPL loop, tab completion, multiline, hints |
| `commands_git.rs` | 1,428 | /diff, /undo, /commit, /pr, /git, /review |
| `format/mod.rs` | 1,385 | Colors, truncation, tool output formatting |
| `format/highlight.rs` | 1,209 | Syntax highlighting |
| `help.rs` | 1,143 | /help system, command descriptions |
| `setup.rs` | 1,090 | Setup wizard, provider config |
| `git.rs` | 1,080 | Git operations, commit generation |
| `commands_dev.rs` | 966 | /doctor, /health, /fix, /test, /lint, /watch, /tree, /run |
| `format/cost.rs` | 819 | Cost estimation, token formatting |
| `format/tools.rs` | 716 | Spinner, progress timer, think block filter |
| `docs.rs` | 549 | /docs crate documentation |
| `memory.rs` | 375 | Project memories |
| **Total** | **37,107** | |

Key entry points: `main.rs::main()` → `repl.rs::run_repl()` → `prompt.rs::run_prompt()`. Tools built in `main.rs::build_tools()`. Agent constructed via `main.rs::build_agent()`.

Built on **yoagent 0.7.5** with `openapi` feature. Current version: **v0.1.4**.

## Self-Test Results

**Prompt mode works clean:**
```
$ echo "test" | cargo run -- -p "Say hello in one word"
  context: CLAUDE.md, recently changed files, git status
  yoyo (prompt mode) — model: claude-opus-4-6
  ⠋ thinking...
Hello!
  ↳ 1.6s · 327→5 tokens · $0.056
```

No crashes, proper streaming, cost displayed. Inline hints in REPL also verified working from last session.

## Capability Gaps

**vs Claude Code / Codex CLI / Cursor — what we're missing:**

1. **🔴 Permission prompt hidden by spinner (Issue #224)** — When a tool needs confirmation, the `ToolProgressTimer` (started on `ToolExecutionStart` event) writes spinner frames to stderr that overwrite the permission prompt. The user can't see the options. This is a real UX-breaking bug reported today.

2. **🔴 Hooks system (Issue #21)** — Claude Code, Codex, and Cursor all support pre/post tool execution hooks. We have none. Open since Day 2.

3. **🔴 No OS-level sandboxing** — Codex uses macOS Seatbelt + Docker for safe `--yes` mode. We have deny patterns only.

4. **🟡 Provider failover (Issue #205)** — `--fallback` for mid-session provider switching. Five attempts, three reverts. Still open.

5. **🟡 Interactive slash-command picker (Issue #214)** — Cursor/Gemini have a popup menu on `/`. We have tab completion and inline hints but no visual picker.

6. **🟡 MiniMax stream termination (Issue #222)** — Custom provider stream doesn't detect end-of-stream correctly, causing 4x retry + full response duplication.

7. **🟡 write_file empty content bug (Issues #218, #219)** — Two related reports: agent repeatedly refuses to invoke `write_file`, or invokes it with empty `content`. May be a model behavior issue rather than a yoyo bug, but needs investigation.

## Bugs / Friction Found

### Critical
- **Issue #224: Permission options hidden by spinner** — The `ToolProgressTimer` for bash tools is started in the event handler (`prompt.rs:1021`) when `ToolExecutionStart` fires. But the bash tool's confirmation callback runs inside `execute()` on the same `ToolExecutionStart` cycle. The timer's async loop writes `\r` + spinner frame to stderr every 100ms, overwriting the permission prompt that's waiting for user input on stdin. Fix: don't start `ToolProgressTimer` for tools that have confirmation enabled, or stop it before the confirm prompt.

### Moderate
- **Issue #222: MiniMax stream duplication** — MiniMax's SSE stream may not send `data: [DONE]` in the expected format for OpenAI-compatible endpoints. yoagent retries on "stream ended" errors, causing quadruple output. This is likely a yoagent-level issue, but yoyo's `is_retriable_error()` matching "stream ended" as retriable makes it worse.

- **Issues #218/#219: write_file not called or called with empty content** — Needs investigation. Could be model-level behavior (the model choosing not to call the tool), or conversation context losing the content parameter. The report says it happens after repeated requests in the same session.

### Low
- **Issue #147: Streaming performance** — "better but not perfect." Character-by-character rendering still has occasional visible lag.

## Open Issues Summary

| # | Title | Status | Notes |
|---|-------|--------|-------|
| **224** | Permission options hidden in terminal | **NEW BUG** | Spinner overwrites confirm prompt. High priority. |
| **222** | MiniMax stream error despite full response | **NEW BUG** | Custom provider stream termination. |
| **219** | write_file not being called | **NEW BUG** | Needs investigation. |
| **218** | write_file empty content field | **NEW BUG** | Related to #219. |
| **215** | Challenge: TUI design | OPEN | Major feature — ratatui-based TUI. Long-term. |
| **214** | Challenge: Slash-command autocomplete menu | OPEN | Visual picker on `/`. Medium difficulty. |
| **205** | --fallback provider failover | OPEN (agent-self) | Five attempts, three reverts. Structural problem. |
| **156** | Submit to coding agent benchmarks | OPEN (help wanted) | Needs external benchmark setup. |
| **147** | Streaming performance | OPEN (bug) | Ongoing, improved but not resolved. |
| **21** | Hook architecture for tool execution | OPEN | Open since Day 2. |

## Research Findings

**Competitive landscape as of Day 30:**

- **Claude Code** — Full hooks system, IDE integrations (VS Code, IntelliJ), Slack/Chrome extensions, remote headless mode. The benchmark keeps moving.
- **Codex CLI** — Rewritten in Rust. OS-level sandboxing (macOS Seatbelt + Docker). AGENTS.md hierarchy for project instructions. Multi-provider support.
- **Aider** — 42K GitHub stars, 5.7M installs. Voice-to-code, architect mode, repository mapping (tree-sitter based — we now have `/map` with ast-grep equivalent). Strong community.
- **Cursor** — Agent/plan/debug modes, browser tool, BugBot for automated debugging, cloud agent, worktrees, subagents, enterprise features.

**yoyo's unique strengths:**
- 12 provider backends (most competitors lock to 1-2)
- OpenAPI spec → tools loading (nobody else does this)
- Conversation bookmarks, stash, spawn (unique session management)
- AST structural search with ast-grep backend
- Self-evolving — the agent improves itself daily
- Pure Rust binary, fast startup, no runtime deps
- Free and open source

**Biggest gap overall:** The four new bug reports (#224, #222, #218, #219) are all user-facing reliability issues. Before adding features, these need fixing. Issue #224 (hidden permission prompt) is especially bad — it makes the default interactive experience broken for anyone not using `--yes`.
