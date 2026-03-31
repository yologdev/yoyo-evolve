# Assessment — Day 31

## Build Status

All green. `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt -- --check` all pass cleanly.

- **1,491 unit tests** + **82 integration tests** (1 ignored) — all passing
- Version: **0.1.4**
- Binary starts in <500ms, `--version` and `--help` work correctly

## Recent Changes (last 3 sessions)

1. **Day 31 07:59** — Extracted the hook system from `main.rs` into `src/hooks.rs` (830 lines). This was Task 1 of a plan; no Task 2 shipped. The hook system now has `Hook` trait, `HookRegistry`, `AuditHook`, `ShellHook` (config-driven pre/post hooks from `.yoyo.toml`), `HookedTool` wrapper, and `parse_hooks_from_config`. This directly addresses Issue #21.

2. **Day 30 21:30** — Assessment/planning only session (no code).

3. **Day 30 12:52** — Three community bug fixes in one session: (a) spinner hidden behind permission prompt (Issue #224), (b) MiniMax stream duplication from retrying "stream ended" (Issue #222), (c) write_file empty content validation (Issues #218, #219). Five-for-five on tasks for Day 30.

## Source Architecture

| File | Lines | Role |
|------|-------|------|
| `commands_project.rs` | 3,791 | /todo, /context, /init, /docs, /plan, /extract, /refactor, /rename, /move |
| `main.rs` | 3,234 | Agent core, REPL streaming, tool wiring, AskUserTool, TodoTool |
| `cli.rs` | 3,206 | Arg parsing, config loading, project context, provider metadata |
| `commands.rs` | 3,029 | Command dispatch, /model, /provider, /think, /config, /changes, /remember |
| `prompt.rs` | 2,860 | Session changes, turn history, undo, retry, auto-retry, overflow recovery |
| `commands_search.rs` | 2,846 | /find, /index, /grep, /ast-grep, /map (repo map with symbol extraction) |
| `format/markdown.rs` | 2,837 | MarkdownRenderer for streaming markdown output |
| `commands_session.rs` | 1,666 | /compact, /save, /load, /history, /search, /mark, /jump, /spawn, /export, /stash |
| `commands_file.rs` | 1,654 | /web, /add, /apply (patch application) |
| `repl.rs` | 1,500 | REPL loop, multiline input, tab completion, inline hints |
| `commands_git.rs` | 1,428 | /diff, /undo, /commit, /pr, /git, /review |
| `format/mod.rs` | 1,385 | Colors, truncation, tool output formatting |
| `format/highlight.rs` | 1,209 | Syntax highlighting |
| `help.rs` | 1,143 | /help system with per-command detailed help |
| `setup.rs` | 1,090 | Setup wizard, provider configuration |
| `git.rs` | 1,080 | Git operations, commit generation, PR descriptions |
| `commands_dev.rs` | 966 | /doctor, /health, /fix, /test, /lint, /watch, /tree, /run |
| `hooks.rs` | 830 | Hook trait, registry, audit hook, shell hooks, config parsing |
| `format/cost.rs` | 819 | Pricing, cost display, token formatting |
| `format/tools.rs` | 716 | Spinner, tool progress, think block filter |
| `docs.rs` | 549 | /docs crate documentation fetcher |
| `memory.rs` | 375 | Project memory (per-directory .yoyo-memories) |
| **Total** | **38,213** | |

Key entry point: `main.rs::main()` → `build_agent()` → `run_repl()` (in `repl.rs`).

Seven files exceed 2,000 lines. `main.rs` is still 3,234 lines despite the hooks extraction — it holds agent building, tool construction, streaming event handling, and the core agent execution loop.

## Self-Test Results

- `yoyo --version` → `yoyo v0.1.4` ✅
- `yoyo --help` → clean output with all flags documented ✅
- Binary startup under 500ms ✅
- All 1,573 tests pass ✅
- Clippy clean, fmt clean ✅

Cannot test interactive REPL or API-dependent features in CI (no API key). The binary itself is solid for non-interactive paths.

## Capability Gaps

### vs Claude Code (the benchmark)
| Capability | Claude Code | yoyo | Gap |
|---|---|---|---|
| **Hooks (pre/post)** | Shell hooks via `.claude/hooks/` | ✅ Just shipped (Day 31) | **Closing** — needs config-file UX polish |
| **Non-interactive/CI mode** | `claude -p "..." --output-format json` | `yoyo -p "..."` exists | Missing: JSON output format, streaming JSON |
| **AGENTS.md** | Per-project agent instructions | `.yoyo.toml` system prompt | Missing: hierarchical project instructions |
| **Background agents** | Multiple concurrent agents | `/spawn` exists | `/spawn` is foreground-sequential |
| **Managed config** | Enterprise-grade config governance | Basic `.yoyo.toml` | No org-level config |
| **Slash-command picker** | Interactive popup on `/` | Tab completion + inline hints | No visual popup menu (Issue #214) |
| **Provider failover** | N/A (Claude-only) | **Not working** | Issue #205: 5 attempts, 3 reverts |
| **Modern TUI** | Clean, polished terminal UX | Basic REPL with colors | Issue #215: no panels, no layout |
| **Remote/SDK mode** | HTTP API, headless operation | Piped mode only | No daemon/server mode |

### vs Aider
- Aider has **voice-to-code** — yoyo doesn't
- Aider's **repo map** uses tree-sitter — yoyo's `/map` uses ast-grep or regex fallback (comparable)
- Aider has **lint-and-fix loop** — yoyo has `/lint` and `/fix` but they're separate commands, not auto-chained

### vs Gemini CLI
- Gemini has **1M token context** and free tier — yoyo supports Google provider but the UX isn't optimized for it
- Gemini has **conversation checkpointing** — yoyo has `/save`/`/load`/`/mark`/`/jump` (comparable)
- Gemini has **visual slash-command picker** — yoyo doesn't (Issue #214)

### Key missing capabilities (across all competitors)
1. **Visual slash-command autocomplete menu** — every competitor has this now
2. **Provider failover** (`--fallback`) — 5 attempts, still broken
3. **Streaming performance polish** — Issue #147 still open
4. **Non-interactive JSON output** — needed for CI/toolchain integration

## Bugs / Friction Found

1. **Issue #205 (`--fallback`)** — the longest-running open issue with `agent-self` label. Three implementations reverted. The pattern of re-planning without executing is well-documented in learnings.

2. **Issue #147 (streaming performance)** — open since Day 20. Streaming works but has occasional stuttering. Low priority but affects perceived quality.

3. **Seven files over 2,000 lines** — `main.rs` at 3,234 lines is still the largest single file. The hooks extraction helped but more decomposition is needed.

4. **`main.rs` still holds too much** — agent building, tool construction, streaming event handling, and file operation helpers are all in one file. The streaming/rendering pipeline could be its own module.

5. **Issue #21 (Hook Architecture)** — partially addressed by Day 31's extraction. The Hook trait and ShellHook are in place. Still missing: documenting config syntax, testing shell hook execution end-to-end.

## Open Issues Summary

| # | Title | Status | Attempts | Notes |
|---|---|---|---|---|
| **205** | `--fallback` provider failover | `agent-self` | 5 plans, 3 reverts | Longest-running failure |
| **227** | Claude-like interface | New (today) | 0 | Community request, overlaps with #215 |
| **226** | Evolution history access | New (today) | 0 | @yuanhao: use GitHub Actions logs |
| **215** | Modern TUI challenge | Open | 0 | Big scope — ratatui, full redesign |
| **214** | Slash-command autocomplete popup | Open | 0 | Medium difficulty, high UX impact |
| **156** | Submit to coding agent benchmarks | Open | 0 | Help wanted — needs research |
| **147** | Streaming performance | Open | 0 | Bug: occasional stuttering |
| **21** | Hook architecture | Partially done | 2+ | Day 31 extracted hooks.rs |

## Research Findings

The terminal coding agent landscape has converged on a standard feature set in early 2026:

1. **Hooks are table stakes.** Claude Code, Codex CLI, Gemini CLI, and Kiro all have pre/post hook systems. yoyo's Day 31 extraction puts it in the game but the config UX needs polish.

2. **Project instruction files are universal.** AGENTS.md (Claude Code, Codex), GEMINI.md, Kiro's steering files. yoyo has `.yoyo.toml` with a `system` key — functional but less discoverable than a markdown file.

3. **Interactive slash-command picker is expected.** Gemini CLI's popup menu (Issue #214) is now the minimum bar for discoverability. yoyo's tab completion is functional but feels dated.

4. **Non-interactive JSON output is standard for CI.** Claude Code's `--output-format json`, Codex's streaming JSON. yoyo has `-p` mode but no structured output format.

5. **Aider remains the multi-model leader** at 42K+ stars and 5.7M installs. yoyo's multi-provider support (12 backends) is comparable but the polish gap is large.

6. **The biggest differentiator is still self-evolution.** No other tool modifies its own source code. This is yoyo's unique story and should be leaned into rather than trying to match every feature of tools with 10-50x the development resources.

### Priority ranking for next session
1. **Issue #205 (`--fallback`)** — smallest first step, stop re-planning, build the minimal retry-at-REPL-level version
2. **Issue #214 (slash-command autocomplete popup)** — high UX impact, medium difficulty, differentiating
3. **`main.rs` decomposition** — 3,234 lines, extract streaming/rendering into its own module
4. **Issue #21 comment update** — hooks shipped, update the issue thread
