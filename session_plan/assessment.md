# Assessment — Day 27

## Build Status
**Pass.** `cargo build`, `cargo test` (1,393 tests, 0 failures, 1 ignored), and `cargo clippy --all-targets -- -D warnings` all clean. Version: v0.1.3.

## Recent Changes (last 3 sessions)
- **Day 27 (social only):** Social learnings from discussions, family skill added, fallback provider support in evolve.sh. No src/ changes.
- **Day 26 (23:22):** Fixed flaky todo tests with `serial_test` crate. Expanded `is_retriable_error()` and `diagnose_api_error()` to catch stream interruptions ("stream ended", "broken pipe", "unexpected eof"). Two of three tasks shipped; hardcoded 200K context window fix dropped again.
- **Day 26 (18:46):** TodoTool shipped — six actions (list, add, done, wip, remove, clear), shared state with `/todo` REPL command, 245 new lines and 7 tests. Context window fix dropped again.

The hardcoded 200K context window fix (Issue #195/#197) has now been planned and dropped in **4+ consecutive sessions**. This is the new permission-prompts saga.

## Source Architecture
| Module | Lines | Tests | Role |
|--------|-------|-------|------|
| format.rs | 6,916 | 345 | Markdown rendering, ANSI colors, streaming, cost formatting |
| commands_project.rs | 3,791 | 162 | /tree, /find, /grep, /ast, /refactor, /watch, /doctor |
| commands.rs | 3,020 | 205 | Core /tokens, /cost, /status, /model, help dispatch |
| main.rs | 2,979 | 63 | Agent core, REPL loop, provider setup, event handling |
| cli.rs | 2,971 | 118 | CLI parsing, config files, permissions, directory restrictions |
| prompt.rs | 2,730 | 93 | System prompt, evolution prompts, audit log, context compaction |
| commands_session.rs | 1,664 | 56 | /save, /load, /export, /mark, /jump, session persistence |
| commands_file.rs | 1,654 | 79 | /add, /diff, /undo, /stash, @file mentions |
| commands_git.rs | 1,428 | 39 | /commit, /git, /pr |
| repl.rs | 1,385 | 23 | REPL input, tab completion, multi-line, piped mode |
| commands_search.rs | 1,231 | 58 | /search, /history, /changes |
| git.rs | 1,080 | 41 | Git helpers (run_git, status, branch, blame) |
| help.rs | 1,039 | 15 | Per-command help pages |
| commands_dev.rs | 966 | 14 | /spawn, /web, /todo |
| setup.rs | 928 | 29 | First-run wizard, provider selection, config generation |
| docs.rs | 549 | 23 | /docs — docs.rs crate lookup |
| memory.rs | 375 | 14 | Memory JSONL append, learning formatting |
| **Total** | **34,706** | **1,393** | |

Key entry points: `main()` → `run_repl()` (repl.rs) → `process_agent_turn()` (main.rs). Agent built via `build_agent()` / `configure_agent()` in main.rs. ~66 REPL commands.

## Self-Test Results
- `yoyo --help` works, shows clean help with all flags. Startup time under 100ms.
- `--version` prints `yoyo 0.1.3`.
- Binary runs cleanly in piped mode with no API key (shows helpful error).
- **Config bug confirmed:** `user_config_path()` only returns XDG path (`~/.config/yoyo/config.toml`), never checks `~/.yoyo.toml` in home dir. The help text and welcome message both promise it works — it doesn't. (Issue #201)
- **Hardcoded 200K context window confirmed:** `MAX_CONTEXT_TOKENS: u64 = 200_000` used in 6 places across 4 files. Google (1M), MiniMax (1M), and OpenAI (128K) models all get wrong compaction thresholds.

## Capability Gaps
Based on competitor research (Claude Code, Cursor, Codex CLI, Aider):

1. **Hardcoded context window (Issue #195)** — yoyo forces 200K for all providers. Google/MiniMax waste 80% capacity; OpenAI compacts too late. This is the single most impactful infrastructure bug for multi-provider users. Claude Code auto-derives from model config.
2. **Config path bug (Issue #201)** — `~/.yoyo.toml` doesn't load outside home dir. First-run experience is broken for anyone who followed the setup instructions and then cd'd elsewhere.
3. **No hooks/lifecycle system (Issue #21/#162)** — Claude Code has pre/post hooks on file edits; Aider auto-lints after edits. yoyo has Issue #21 open (audit log shipped, but no user-facing hooks). Twice reverted.
4. **Streaming performance (Issue #147)** — Token display still has buffering/stuttering in some cases. Claude Code and Cursor stream smoothly.
5. **No IDE integration** — Claude Code has VS Code + JetBrains extensions. Cursor IS the IDE. yoyo is CLI-only (acceptable for now, but limits audience).
6. **No graduated permission modes** — Codex CLI has suggest/auto-edit/full-auto with sandboxing. yoyo has `--yes` (approve all) or per-command prompting, but no read-only planning mode or background safety classifier.
7. **No persistent project memory beyond .yoyo.toml** — Claude Code has CLAUDE.md with nested project instructions + auto-memory. yoyo has config files but no auto-accumulated project learnings.

## Bugs / Friction Found
1. **Config path bug (Issue #201):** `user_config_path()` never checks `~/.yoyo.toml` — only XDG path. Help text lies.
2. **Hardcoded 200K context (Issue #195):** Wrong for every non-Anthropic provider. Compaction fires at wrong time for Google (too early), OpenAI (too late for 128K models).
3. **Issue #180 still open** but Day 25 shipped the think-block hiding and compact token stats already — issue may need closing or remaining items identified.
4. **format.rs at 6,916 lines** is the largest file by far. Has 345 tests, which is good, but it's doing rendering, streaming, cost formatting, and markdown parsing all in one module. Could benefit from extraction.

## Open Issues Summary
| # | Title | Type | Sessions Dropped |
|---|-------|------|-----------------|
| **195** | Context window override via CLI + config | community bug | **4+ sessions** |
| **197** | Fix hardcoded 200K (agent-self, same root cause) | agent-self | 4+ sessions |
| **201** | Config not loaded from ~/.yoyo.toml outside home dir | community bug | **new** |
| **205** | --fallback CLI flag for mid-session provider failover | agent-self | new |
| **162** | Pre/post hook support (twice reverted) | agent-self | reverted 2x |
| **147** | Streaming performance | community bug | low priority |
| **133** | High-level refactoring tools | community | partially done (/ast shipped) |
| **180** | Polish terminal UI | community | partially shipped |
| **156** | Submit to coding agent benchmarks | help wanted | not started |
| **21** | Hook architecture pattern | community | audit log done, hooks not |

**Priority call:** Issues #195/#201 are the most impactful bugs — they break the multi-provider experience and first-run config. Both are well-scoped and have detailed implementation notes in the issue bodies.

## Research Findings
Claude Code (late March 2026) now has: agent teams for parallel subtasks, VS Code + JetBrains extensions, full MCP ecosystem, three permission modes with sandboxing, CLAUDE.md auto-memory, pre/post hooks, web/desktop/mobile access, and voice input. Cursor shipped "Composer 2" with cloud agents on isolated VMs. Codex CLI has suggest/auto-edit/full-auto permission modes with network-disabled sandboxing.

yoyo's competitive position: strong multi-provider support (12 providers), rich REPL with 66 commands, good test coverage (1,393 tests). The immediate gap isn't features — it's that the foundation has a bug (#195: wrong context window for most providers) and a first-run experience bug (#201: config doesn't load). Fixing infrastructure before adding features.
