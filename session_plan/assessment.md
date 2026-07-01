# Assessment — Day 123

## Build Status
**All green.** `cargo build` ✅, `cargo test` ✅ (4,148 passed, 0 failed, 2 ignored), `cargo clippy --all-targets -- -D warnings` ✅ clean. Binary runs and responds correctly in prompt mode.

## Recent Changes (last 3 sessions)

**Day 123** — Two sessions. (1) Major refactor of `safety.rs`: broke a 170-line monolithic `analyze_bash_command` into 29 focused check functions + a `SAFETY_CHECKS` dispatch table, ~120 lines shorter. (2) Planning-only session (input validation, Copilot provider, risk-reflex report) — no code shipped.

**Day 122** — (1) Extracted 3 duplicated truncation implementations into shared `truncate_at_word_boundary` and `append_tail_preview` in `format/mod.rs`. (2) Fixed case-sensitivity bug in safety checks (`iptables -F` vs `-f` conflation), added `chmod 777` detection, fixed byte-slice panic in reverse shell detection.

**Day 121** — (1) Fixed yopedia skill instructions (keyword search first, not authenticated query). (2) Planned emerging-risk detection (momentum scoring) but only shipped 3 lines fixing `let _ =` patterns. (3) One empty auto-generated session.

**Pattern**: Safety hardening, DRY refactoring, `let _ =` cleanup, and planning sessions that don't produce code. Mature maintenance work, no new user-facing features in this window.

## Source Architecture
72 source files, ~111,000 lines total.

| Category | Files | Lines |
|----------|------:|------:|
| Command handlers (`commands*.rs`) | 28 | 43,383 |
| Formatting (`format/*.rs`) | 7 | 11,907 |
| Core infrastructure (main, cli, config, dispatch, repl, prompt*, session, agent_builder) | 13 | 19,773 |
| Tools & safety (tools, tool_wrappers, smart_edit, hooks, safety) | 5 | 10,678 |
| Support modules (git, memory, symbols, context, etc.) | 19 | 25,278 |

Key entry points: `main.rs` → `cli.rs` (arg parsing) → `agent_builder.rs` (agent construction) → `repl.rs` (interactive loop) / `prompt.rs` (prompt execution). Largest files: `commands_risk.rs` (5,311), `commands_git.rs` (3,760), `cli.rs` (3,367), `commands_project.rs` (3,159), `watch.rs` (3,135), `commands_search.rs` (3,001).

## Self-Test Results
- Binary starts, displays banner, auto-detects watch command (`cargo clippy ... && cargo test`)
- Prompt mode (`-p "say hello"`) works correctly — responds and exits
- Build is fast (0.12s incremental)
- No friction observed in basic flow

## Evolution History (last 5 runs)
All 5 most recent evolve runs: **success**. No failures, no reverts, no timeouts.
- 2026-07-01T20:48 — in progress (this session)
- 2026-07-01T20:43 — success
- 2026-07-01T18:52 — success
- 2026-07-01T16:12 — success
- 2026-07-01T13:28 — success

CI pipeline (ci.yml) also all green — last 5 runs all success.

Trajectory data (wider window): 10 recent sessions, 0 reverts in window, no provider errors. One recurring CI fingerprint: `test_load_project_context_includes_recently_changed` has appeared as a flaky test (2 failures in window), but it's not currently failing.

## Capability Gaps

**vs Claude Code (biggest gaps):**
1. **Dynamic workflows / parallel sub-agent orchestration** — Claude Code ships "ultracode" mode that spawns tens to hundreds of parallel sub-agents for massive tasks (migrations, bug hunts). yoyo has sub-agents but no orchestration layer for fleet-level parallelism.
2. **Nested sub-agents** — Claude Code allows sub-agents to spawn their own sub-agents (5 levels deep). yoyo has a hard depth cap of 3 and no visual tree.
3. **Artifacts** — Live, shareable interactive pages published from sessions (PR walkthroughs, dashboards). yoyo has nothing like this.
4. **Agent view / dashboard** — `claude agents` shows all running sessions in one screen with dispatch/attach/monitor. yoyo has `/bg` but it's primitive.
5. **`/cd` command** — Move a session to a different working directory without restarting.
6. **Claude Sonnet 5** with 1M-token native context window is now default.

**vs Cursor 3:**
1. **Multi-repo agents** — Cursor 3.5 works across multiple repos simultaneously. yoyo is single-repo.
2. **Cloud agents** — Run agents in hosted sandboxes, dispatch from mobile/web/Slack/Linear. yoyo is terminal-only.
3. **312K-token effective context** with 78% cost reduction via prompt caching. yoyo doesn't leverage prompt caching.
4. **No-code automations** (Slack, Stripe, Databricks triggers). Different category entirely.

**vs Codex CLI (OpenAI):**
1. 92K GitHub stars (vs yoyo's ~2K). Massive community.
2. **Codex Cloud tasks** — launch tasks to cloud sandboxes, apply diffs locally.
3. **Image generation** and **image inputs** natively in the CLI.
4. **TUI** — full-screen terminal UI with syntax-highlighted diffs. yoyo uses a basic readline REPL.

**vs Aider:**
1. 46K stars, 6.8M PyPI installs. Mature ecosystem.
2. **Tree-sitter repo map** — structural code analysis. yoyo uses regex-based symbol extraction.
3. **Architect mode** with cost-optimized planning+editing model split. yoyo has `/architect` but it's buggy (stale editor-model map causes 404s per issue #542).
4. **`--watch` mode with AI? comments** for in-editor integration. yoyo has watch mode but no editor integration.

**Biggest overall gap**: Parallel agent orchestration. Every major competitor now ships multi-agent parallel execution for large tasks. yoyo's sub-agent system is sequential and shallow.

## Bugs / Friction Found

1. **#543 — `--model` empty/whitespace passes to API unguarded.** `flag_value` returns raw whitespace, `parse_model_config` doesn't filter it, API 400s. Creator-filed, specific fix described.

2. **#542 — Architect mode editor-model map is stale.** `default_editor_model()` has a hardcoded tiering table (opus→sonnet, etc.) that rots with every new model. `/architect` on Opus currently 404s. Creator wants the map deleted and replaced with explicit `--editor-model` flag.

3. **#544 — GitHub Copilot not available as model provider.** Community request from @mchzimm. Copilot uses OAuth device flow for auth, which is different from API-key providers.

4. **Flaky test** — `test_load_project_context_includes_recently_changed` appeared 2× in CI failures in the trajectory window. Not currently failing but worth investigating.

5. **386 `let _ =` instances remain** (noted Day 120). Declarative knowledge hasn't become procedural habit — the pattern keeps recurring in new code.

## Open Issues Summary

| # | Title | Source | Priority |
|---|-------|--------|----------|
| 543 | Harden --model handling: reject empty/whitespace, warn on unknown model | Creator (agent-input) | High — fleet-wide blast radius |
| 542 | Replace architect auto-downgrade editor-map with explicit editor-model config | Creator (agent-input) | High — `/architect` currently broken on Opus |
| 544 | Missing GitHub Copilot as model provider | Community (@mchzimm) | Medium — onboarding friction |
| 530 | Selectively use Exa type:"deep" for hard research queries | Self | Low |
| 529 | Add text.includeHtmlTags:true to Exa web_search request | Self | Low |
| 341 | RLM future-capability roadmap (master tracking) | — | Tracking |
| 215 | Challenge: Design and build a beautiful modern TUI | — | Aspirational |
| 156 | Submit yoyo to official coding agent benchmarks | — | Aspirational |

**Top priority**: #543 and #542 are creator-filed `agent-input` issues with specific fixes described. Both relate to model handling robustness — a theme that matters now because harness model config was centralized (commit `2f63c4c`), making one bad value fleet-wide.

## Research Findings

The coding agent field has consolidated around a few serious harnesses. The key insight from current landscape analysis:

1. **The harness matters more than the model.** Multiple sources note that the same Claude Opus gives different experiences inside different harnesses. yoyo's harness (context gathering, tool calling, change application, guardrails) is the differentiator, not model choice.

2. **Parallel agent orchestration is table stakes.** Claude Code's "dynamic workflows" (tens to hundreds of sub-agents), Cursor 3's multi-agent sidebar, Codex's sub-agent system — all shipping production parallel execution. yoyo's sequential sub-agent dispatch is a generation behind.

3. **Cloud/hosted execution is the new frontier.** Cursor Cloud agents, Codex Cloud tasks, Claude Code Remote Control — all allow delegating work to sandboxed cloud environments. yoyo is purely local.

4. **Prompt caching delivers 78% cost reduction** on Cursor. yoyo doesn't exploit this at the harness level.

5. **Context windows are expanding.** Claude Sonnet 5 has 1M tokens native. Cursor's effective context is 312K. yoyo's default context is much smaller.

6. **yoyo's unique differentiator remains self-evolution.** No competitor has an autonomous evolution loop, memory system, dream layer, or journal. This is genuine novelty in the space — but it's infrastructure for the agent, not features for users.
