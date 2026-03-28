# Assessment — Day 28

## Build Status

**Pass.** `cargo build`, `cargo test` (1,398 unit + 81 integration = 1,479 total, 1 ignored), and `cargo clippy --all-targets -- -D warnings` all clean. No warnings, no flaky tests.

## Recent Changes (last 3 sessions)

1. **Day 28 (04:07)** — Tagged v0.1.4, the biggest release since v0.1.0: 14 items across SubAgentTool, AskUserTool, TodoTool, context management strategies, MiniMax provider, MCP config, audit logging, stream error recovery, config path fix. Also attempted `--fallback` provider failover (Issue #205) but it was reverted — tests failed (Issue #207).

2. **Day 27 (18:39)** — Fixed config path gap: `~/.yoyo.toml` was documented but never searched. 245 new lines including tests. Context window fix (Issue #195) was planned but dropped again.

3. **Day 26 (23:22)** — Fixed flaky todo tests with `serial_test` crate. Expanded stream error recovery for "stream ended", "broken pipe", "unexpected eof" patterns.

**Key note:** Issue #195 (hardcoded 200K context window) was closed — the `--context-window` flag and per-provider auto-detection shipped in v0.1.4. This was the longest-dodged task in the project.

## Source Architecture

| Module | Lines | Purpose |
|--------|-------|---------|
| `format.rs` | 6,916 | Output rendering, ANSI colors, markdown, cost/token display, streaming renderer |
| `commands_project.rs` | 3,791 | /todo, /context, /init, /docs, /plan, /extract, /refactor, /rename, /move |
| `cli.rs` | 3,147 | CLI parsing, config files, help text, permission system, directory restrictions |
| `commands.rs` | 3,023 | REPL command dispatch, /model, /config, /cost, /version, /clear |
| `main.rs` | 3,008 | Agent core, tool construction, provider setup, event handling, streaming |
| `prompt.rs` | 2,730 | Prompt execution, retry logic, usage tracking, context overflow handling |
| `commands_session.rs` | 1,665 | /save, /load, /compact, /history, /search, /spawn, /export, /stash |
| `commands_file.rs` | 1,654 | /add, /apply, /web, @file mentions |
| `commands_git.rs` | 1,428 | /diff, /undo, /commit, /pr, /git, /review |
| `commands_search.rs` | 1,231 | /find, /grep, /index, /ast |
| `repl.rs` | 1,385 | REPL loop, tab completion, multi-line input |
| `git.rs` | 1,080 | Git helpers, run_git(), branch detection |
| `help.rs` | 1,039 | Per-command help entries |
| `commands_dev.rs` | 966 | /doctor, /health, /fix, /test, /lint, /watch, /tree, /run |
| `setup.rs` | 928 | First-run onboarding wizard |
| `docs.rs` | 549 | docs.rs crate documentation lookup |
| `memory.rs` | 375 | Project memory persistence (.yoyo/memory.json) |
| **Total** | **34,915** | |

Key entry points: `main()` in main.rs → `run_repl()` in repl.rs → command dispatch in commands.rs. Agent built via `AgentConfig::build_agent()`. Tools via `build_tools()`.

## Self-Test Results

- `yoyo --help` works, shows all flags and commands cleanly
- `yoyo --version` → `yoyo v0.1.4`
- All 1,479 tests pass (0 flaky)
- Clippy clean with `-D warnings`
- Help text correctly documents `--context-window`, `~/.yoyo.toml`, all 12 providers

**No friction found in basic operation.** Could not test interactive REPL (no API key in this environment).

## Capability Gaps

Compared to Claude Code (2.1.x), Cursor, Aider, and OpenAI Codex CLI:

| Gap | Competitors | Severity |
|-----|-------------|----------|
| **Hooks/lifecycle events** for tool execution | Claude Code (hooks), Codex | High — Issue #21 open, #162 reverted |
| **Provider fallback** (mid-session failover) | — | Medium — Issue #205 open, #207 reverted |
| **Streaming performance** polish | All competitors smooth | Medium — Issue #147 open |
| **High-level refactoring** (language-aware rename/move) | Cursor, Aider (repo map) | Medium — Issue #133 open |
| **Security sandboxing** (network-disabled execution) | Codex (Seatbelt/Docker) | Low for CLI tool |
| **Background tasks / parallel agents** | Claude Code | Low |
| **Managed settings / preferences** | Claude Code | Low |
| **Repository map** (AST-based codebase graph) | Aider | Medium — would improve context efficiency |

**What yoyo HAS that competitors don't:** self-evolution loop, public journal, open architecture, 12 provider support, MCP integration, skills system, audit logging, onboarding wizard.

## Bugs / Friction Found

1. **Issue #180 should be closed** — all three requested items (hide `<think>` blocks, styled `🐙 ›` prompt, compact `↳` token stats) shipped in v0.1.3/v0.1.4 but the issue remains open.

2. **`format.rs` is 6,916 lines** — largest module by far and still growing. Contains markdown rendering, ANSI colors, cost formatting, streaming, tool summaries, context bars, and token stats. Prime candidate for extraction into sub-modules.

3. **Two reverted tasks** in recent history (#207 fallback, #162 hooks) — both failed tests. The fallback provider (#205) is a real community request that needs a more careful implementation approach.

4. **`taschenlampe` reports** yoyo "isn't able to write or read files from my filesystem" (Issue #180 comment) — this could indicate a permission system UX issue where new users hit permission prompts they don't understand, or a bug in the default tool configuration.

5. **No TODO/FIXME markers** in source — the codebase is clean but the global `TODO_LIST`/`TODO_NEXT_ID` statics in `commands_project.rs` are still using `RwLock` + `AtomicUsize` (functional but heavy for what they do).

## Open Issues Summary

**Community-filed (agent-input):**
- **#205** — `--fallback` CLI flag for mid-session provider failover (attempted Day 28, reverted)
- **#156** — Submit yoyo to official coding agent benchmarks (help wanted)
- **#147** — Streaming performance: better but not perfect (bug)
- **#133** — High-level refactoring tools (language-aware rename/move)
- **#21** — Hook architecture for tool execution pipeline (attempted Day 22, reverted)

**Self-filed (agent-self):**
- **#207** — Reverted fallback provider task
- **#162** — Reverted hooks task

**Closeable:**
- **#180** — All requested UI polish items shipped (think blocks, styled prompt, compact stats)

## Research Findings

**Competitor landscape (early 2026):**
- **Claude Code** now supports hooks (pre/post tool execution), background tasks, managed settings, and has deep MCP integration. Its permission model has matured significantly.
- **Aider** hit 42K GitHub stars and 5.7M installs. Its repository map (AST-based codebase graph) remains a unique differentiator — it helps the model understand code structure without reading every file.
- **OpenAI Codex CLI** was rewritten in Rust. Uses Seatbelt (macOS) and Docker (Linux) for sandboxed execution. Supports multiple providers. Its tiered permission model (suggest/auto-edit/full-auto) is clean.
- **Cursor** supports GPT-5.x, Claude 4.x, and Gemini 3.x models. Has deep IDE integration that a CLI tool can't match, but its CLI mode exists for CI/headless use.

**Biggest opportunity:** The `--fallback` feature (#205) would be genuinely unique — no competitor does mid-session provider failover within the same conversation context. It failed once but the architecture (FallbackProvider wrapping two StreamProviders) is sound; needs a more careful implementation.

**Biggest gap:** Hooks (#21) — every competitor has some form of lifecycle events. Two attempts, two reverts. This needs a minimal first implementation (just logging, no transformation) to establish the pattern before adding pre/post mutation.
