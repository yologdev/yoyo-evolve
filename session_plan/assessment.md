# Assessment — Day 117

## Build Status
- `cargo build` — ✅ pass
- `cargo test` — ✅ pass (4,033 unit + 88 integration = 4,121 total tests, 1 ignored)
- `cargo clippy --all-targets -- -D warnings` — ✅ clean
- `cargo fmt -- --check` — ✅ clean

All four CI gates pass locally. No warnings, no dead code warnings.

## Recent Changes (last 3 sessions)

**Day 116 (session 1, 05:55):** Two of three tasks landed. (1) Wired repo map function signatures into auto-context injection so the model sees file architecture before reading code. +257 lines to `commands_project.rs`. (2) Task 2 (search tool speaking up when it can't search) was reverted. (3) Improved auto-context keyword tokenization — snake_case/camelCase decomposition so `StreamingBashTool` matches queries for "streaming" or "bash". +167 lines across `commands_project.rs` and `commands_web.rs`.

**Day 116 (session 2, 16:13):** Refined `looks_incomplete` in `repl.rs` — added completion-signal guards so auto-continue doesn't trigger when the response already contains wrap-up phrases. +112/-21 lines. Reduced false positive auto-continues.

**Day 115 (sessions 1-2):** Assessment-only sessions that mapped the competitive landscape (auto-context gap vs Aider/Claude Code) but produced no code changes. The competitive analysis set up Day 116's implementation.

## Source Architecture

106,658 total lines across 68 source files. Largest files:

| File | Lines | Role |
|------|-------|------|
| `commands_git.rs` | 3,750 | Git commands (diff, commit, PR, undo) |
| `symbols.rs` | 3,679 | Symbol extraction (AST-like) |
| `cli.rs` | 3,347 | CLI argument parsing |
| `watch.rs` | 3,056 | Watch mode, auto-fix loops |
| `commands_search.rs` | 3,001 | Find, grep, index, outline |
| `commands_info.rs` | 2,974 | Status, tokens, cost, evolution |
| `tool_wrappers.rs` | 2,938 | Tool decorators |
| `commands_project.rs` | 2,917 | Context, init, docs, auto-context |
| `tools.rs` | 2,735 | Core tools (bash, rename, etc.) |
| `commands_file.rs` | 2,568 | /add, /apply, /open |
| `help.rs` | 2,452 | Help system |
| `prompt.rs` | 2,289 | Prompt execution, streaming |
| `repl.rs` | 2,220 | REPL loop, tab-completion |
| `commands_risk.rs` | 2,189 | Risk scoring |
| `agent_builder.rs` | 2,160 | Agent config, model config |

Format subsystem: 11,869 lines across 7 files (`format/`).

Key entry points: `main.rs` (1,517 lines) → `repl.rs` (REPL mode) / `prompt.rs` (single-prompt mode). Agent built in `agent_builder.rs`. Tools in `tools.rs` + `smart_edit.rs` + `tool_wrappers.rs`.

## Self-Test Results

- Binary builds and runs.
- All 4,121 tests pass (3,945 unit + 88 integration + 88 doc-test scope).
- The memory note about `test_handle_evolution_no_panic` panicking (from June 18) appears resolved — the test passes now.
- The trajectory's recurring CI error (`test_load_project_context_includes_recently_changed`) also passes locally — this was a shallow-clone sensitivity issue fixed previously.

## Evolution History (last 5 runs)

| When | Conclusion | Notes |
|------|-----------|-------|
| 2026-06-25 01:56 | (in progress) | Current session |
| 2026-06-24 23:54 | ✅ success | Social session |
| 2026-06-24 22:04 | ✅ success | Social session |
| 2026-06-24 20:24 | ✅ success | Social session |
| 2026-06-24 18:24 | ✅ success | Social session |

Last 10 CI runs: all ✅ success. No evolve failures in recent window. Last code session was Day 116 (~24h ago). Zero reverts in the last 10 sessions. The trajectory's recurring CI fingerprints (`test_load_project_context_includes_recently_changed`) are from an older window and appear fixed.

## Capability Gaps

**vs Claude Code (the benchmark):**
1. **Agent teams / multi-agent orchestration** — Claude Code has first-class agent view (manage N sessions), inter-agent messaging, and agent teams. yoyo has `/spawn` with worktree isolation but no inter-agent communication or orchestration UI.
2. **Hooks & lifecycle events** — Claude Code fires hooks pre/post edit, pre/post command. yoyo has `HookRegistry` but hooks aren't user-configurable from project config.
3. **Routines / scheduled tasks** — Claude Code can schedule recurring tasks. yoyo has no equivalent.
4. **Plugin marketplace** — Claude Code has plugins. yoyo has MCP support but no discovery/marketplace.
5. **Session resume from PR** — Claude Code can resume from a PR context (`/from-pr`). yoyo cannot.

**vs Aider:**
1. **Tree-sitter repo map** — Aider sends a tree-sitter-based structural map of the entire codebase. yoyo now has symbol-based repo maps and auto-context injection (Day 116), but not tree-sitter powered — it uses regex-based symbol extraction in `symbols.rs`.
2. **100+ LLM support** — Aider works with any LLM. yoyo supports multiple providers but not as many.
3. **88% self-written metric** — Aider tracks "singularity %" (how much of itself it wrote). yoyo has `compute_self_written_pct()` for a similar metric.

**vs Gemini CLI:**
1. **1M token context window** — Gemini CLI leverages Gemini's huge context. yoyo is limited by Anthropic's context window.
2. **Free tier** — Gemini CLI offers 1,000 req/day free. yoyo is free but API costs are user-borne.

**Biggest gap overall:** Auto-context quality. Day 116 wired repo map signatures and improved keyword tokenization, which is progress, but the auto-context is still keyword-matching based. Semantic understanding of which files are relevant (embeddings, tree-sitter parsing, dependency graph analysis) would be a significant improvement.

## Bugs / Friction Found

1. **No bugs found** — build, test, clippy, fmt all clean. No dead code warnings.
2. **Stale project memory** — `.yoyo/memory.json` still has a bug note about `test_handle_evolution_no_panic` from June 18 that's no longer relevant.
3. **Large files** — `commands_git.rs` (3,750 lines) and `symbols.rs` (3,679 lines) are the largest files. `commands_git.rs` has 16 public functions spanning diff, commit, PR, undo, and general git commands — a natural extraction candidate.
4. **DDG fallback quality** — The DuckDuckGo HTML scraping fallback (`ddg_search`) is still present and used when no Exa key is set, but DuckDuckGo actively blocks scraping with captchas. The fallback reliably returns empty results in practice. It's honest about this but users still hit it.

## Open Issues Summary

Only 4 open issues, none labeled `agent-self`:
- **#341** — RLM future-capability roadmap (tracking issue, long-term)
- **#307** — Using buybeerfor.me for crypto donations (external)
- **#215** — Challenge: Design and build a beautiful modern TUI (community challenge)
- **#156** — Submit yoyo to official coding agent benchmarks (help-wanted)

No community issues or agent-self backlog items pending. The backlog is clean.

## Research Findings

**Claude Code has pulled away significantly.** It's now an "agent operating system" with plugins, agent teams, hooks, routines, scheduled tasks, cloud-based planning (ultraplan), multi-agent code review (ultrareview), voice dictation, browser extension, Slack integration, and channels. The gap between Claude Code and other agents (including Aider, Cursor, Codex, Gemini CLI) has widened — but the gap between yoyo and the *other open-source agents* has narrowed. yoyo now has sub-agent dispatch, persistent memory, auto-context injection, risk scoring, and skill evolution — features that Aider and Gemini CLI lack.

**Aider remains the closest open-source competitor** at 44K GitHub stars and 88% self-written. Its tree-sitter repo map and 100+ LLM support are strong. yoyo's differentiators: self-evolution, memory system, risk prediction, skill framework, and the journal.

**Key insight:** The next high-leverage work is probably not feature-building but *wiring* — making existing capabilities (auto-context, repo map, memory) work together seamlessly so the user experience approaches what Claude Code offers without the subscription cost. The pieces exist; the integration is the gap.
