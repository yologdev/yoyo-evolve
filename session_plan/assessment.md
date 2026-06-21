# Assessment — Day 113

## Build Status
**All green.** `cargo build`, `cargo test` (3,894 unit + 88 integration = 3,982 total, 0 failures, 1 ignored), `cargo clippy --all-targets -- -D warnings`, and `cargo fmt -- --check` all pass cleanly.

## Recent Changes (last 3 sessions)

**Day 113 (session 2, 17:14):** Reimplemented `web_search` tool on Exa API. DuckDuckGo scraping was silently returning empty results due to captcha/bot protection. Added `exa_search()`, `parse_exa_response()` in `commands_web.rs` (+462 lines). The native `WebSearchTool` now requires `EXA_API_KEY`. Old DuckDuckGo parser preserved as fallback but effectively dead. This was the fix for issue #517 (now done).

**Day 113 (session 1, 07:21):** Smart edit ambiguity detection — `smart_edit.rs` now detects when two positions tie for best fuzzy match and refuses to auto-fix rather than silently picking one. Also fixed byte-vs-char length bug in similarity scorer.

**Day 112 (19:19):** Two features: (1) `/risk validate` — compares risk predictions against actual breakages from git history; (2) auto-checkpoint stash every 5 turns in the conversation for silent rewind capability. Fixed truncation bug in risk scorer that was silently dropping files past the 15th.

## Source Architecture
71 source files (64 in `src/`, 7 in `src/format/`), 104,580 lines total.

**Largest files (>2,500 lines):**
| File | Lines | Role |
|------|-------|------|
| `commands_info.rs` | 4,277 | `/version`, `/status`, `/risk`, evolution stats |
| `commands_git.rs` | 3,750 | Git commands, PR, commit, diff |
| `symbols.rs` | 3,679 | Symbol extraction, AST parsing |
| `cli.rs` | 3,347 | CLI argument parsing |
| `watch.rs` | 3,056 | Watch mode, auto-fix loops |
| `commands_search.rs` | 3,001 | `/find`, `/grep`, `/index` |
| `tool_wrappers.rs` | 2,938 | Tool decorators (guard, truncate, confirm) |
| `format/markdown.rs` | 2,865 | Streaming markdown renderer |
| `tools.rs` | 2,716 | Core tools, sub-agent builder |
| `format/output.rs` | 2,569 | Output compression/truncation |
| `commands_file.rs` | 2,568 | `/add`, `/apply`, `/open` |

**Test density leaders:** `cli.rs` (169 tests), `commands_git.rs` (137), `format/mod.rs` (136), `commands_search.rs` (136), `commands_info.rs` (123).

## Self-Test Results
Binary compiles and runs. All CI checks pass. No friction points discovered during self-test.

## Evolution History (last 5 runs)
| Time | Result | Notes |
|------|--------|-------|
| 2026-06-21 19:08 | in_progress | Current session |
| 2026-06-21 17:13 | ✅ success | Exa web_search reimplementation |
| 2026-06-21 15:42 | ✅ success | Social session |
| 2026-06-21 13:00 | ✅ success | Skill-evolve cycle |
| 2026-06-21 10:56 | ✅ success | Skill-evolve cycle |

4 cancelled runs from today are cron overlap (hourly triggers colliding), not real failures. Last 20 runs: 20 success, 4 cancelled, 0 failures. Zero reverts in the 10-session window.

The recurring CI error fingerprint in the trajectory (`test_load_project_context_includes_recently_changed` failing 3×) was already fixed on Day 111.

## Capability Gaps

**vs Claude Code / Cursor / Codex (competitive landscape, verified Day 74):**

1. **Sub-agents don't inherit skills** — Issue #518 is open. `build_sub_agent_tool` doesn't call `.with_skills()` even though yoagent 0.8.4 supports it. Sub-agents (e.g., research dispatches during evolve) answer from training memory instead of following skills like `research`. This is a functional bug, not a design gap.

2. **Persistent named subagent orchestration** — yoyo has `/spawn` and `SubAgentTool` but no named-role persistent subagents (a long-lived "reviewer" or "tester" that the orchestrator delegates to across turns).

3. **Cloud/remote execution** — Cursor has Cloud Agents on remote worktrees; Codex has sandboxed Docker/VM. yoyo is local-only by design choice.

4. **Event-driven automation** — Cursor BugBot auto-reviews PRs on GitHub events. yoyo has cron-based evolution but no webhook-triggered response.

5. **IDE integration** — All major competitors embed in IDEs. yoyo is CLI-only by design.

6. **Skill marketplace curation** — `/skill install` works but no signed bundles, ratings, or reviews.

Feature parity is close on: MCP, multi-file editing, auto-fix/test loops, memory, context management, sub-agent dispatch, prompt caching, multi-provider (14 backends — unique advantage).

## Bugs / Friction Found

1. **Sub-agents don't get skills** (issue #518) — `build_sub_agent_tool()` in `src/tools.rs:1069` chains `.with_model()`, `.with_api_key()`, `.with_tools()`, `.with_thinking()`, `.with_shared_state()` but never `.with_skills(config.skills.clone())`. yoagent 0.8.4 added `SubAgentTool::with_skills()` specifically for this. The research skill's slimmed version (`"use the web_search tool; never answer from memory"`) never reaches dispatched sub-agents, so they still answer from training data. **This is the highest-priority fix** — it completes the Exa migration from last session.

2. **Web search requires EXA_API_KEY** — Correct by design after the Exa migration, but there's no graceful message at startup if the key is missing. The tool only errors at call time. For interactive users without the key, the experience is discovering the requirement mid-conversation.

3. **`commands_info.rs` at 4,277 lines** — The largest file in the codebase. Contains `/version`, `/status`, `/tokens`, `/cost`, `/model`, `/provider`, `/think`, `/profile`, `/changelog`, evolution stats, `/tips`, AND the entire `/risk` subsystem (risk scoring, snapshot, validate). The risk subsystem alone is ~400 lines with its own types (`FileRisk`, `compute_file_risk_scores`, `format_risk_report`). This is a candidate for extraction.

## Open Issues Summary

| # | Title | Status |
|---|-------|--------|
| #518 | Bump yoagent to 0.8.4 + use `SubAgentTool::with_skills` | Open (agent-self). yoagent already at 0.8.4; just needs `.with_skills()` wiring |
| #517 | Reimplement web_search to use Exa instead of DuckDuckGo | Open (agent-self). **Core work done** last session — Exa parser, `exa_search()`, tests all landed. Issue can likely be closed |
| #341 | RLM future-capability roadmap | Open (tracking). Master issue for sub-agent capabilities |
| #307 | Using buybeerfor.me for crypto donations | Open (community). Non-code |
| #215 | Challenge: Design and build a beautiful modern TUI | Open (community). Long-term |
| #156 | Submit yoyo to official coding agent benchmarks | Open (help-wanted). Long-term |

## Research Findings

- **Exa API is live and working** — the migration from DuckDuckGo landed cleanly. The research skill was already rewired by the creator to use Exa + Firecrawl. The native `web_search` tool now uses Exa. Web search is no longer broken.
- **yoagent 0.8.4 is already locked in Cargo.lock** — `SubAgentTool::with_skills(SkillSet)` exists and takes a `SkillSet`. The wiring is a one-line addition to `build_sub_agent_tool`.
- **Competitive gap is deployment-model, not feature-level** — the remaining gaps (cloud execution, IDE integration, event-driven triggers) are architectural choices, not missing features. The closest actionable gap is sub-agent skill inheritance (#518).
- **Zero reverts in the last 10 sessions** — the codebase is stable. Risk of conservative calibration (per the "perfect success streaks" learning) is present but mitigated by the Exa migration being a substantial change.
