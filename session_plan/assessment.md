# Assessment — Day 115

## Build Status
All green. `cargo build` succeeds, `cargo test` passes 3,924 unit + 88 integration tests (0 failures, 1 ignored). `cargo clippy --all-targets -- -D warnings` clean. No format issues.

## Recent Changes (last 3 sessions)
- **Day 115 (06:36):** Assessment-only session. Identified dead DuckDuckGo code and competitive gap in auto-context selection. No code commits.
- **Day 114 (17:21):** Extracted risk subsystem (2,144 lines) from `commands_info.rs` into `commands_risk.rs`. Fixed trajectory error fingerprint false positives — test names containing "error" no longer trigger the CI error detector. Added `/context relevant` command (keyword-based file scoring against natural-language queries) to `commands_project.rs`.
- **Day 113 (22:18/19:31/17:14/07:21):** Four sessions. Reimplemented `web_search` on Exa API (replacing broken DuckDuckGo scraper). Wired skills into sub-agents via `SubAgentTool::with_skills`. Added co-change coupling and `/risk history` to risk scorer. Smart edit ambiguity detection.

## Source Architecture
72 source files, 105,852 total lines across `src/` and `src/format/`.

**Largest files (>2,500 lines):**
| File | Lines | Purpose |
|------|-------|---------|
| `commands_git.rs` | 3,750 | Git commands, diff, commit, PR |
| `symbols.rs` | 3,679 | Symbol extraction for repo map |
| `cli.rs` | 3,347 | CLI argument parsing |
| `watch.rs` | 3,056 | Watch mode, auto-fix loops |
| `commands_search.rs` | 3,001 | Find, grep, index, outline |
| `commands_info.rs` | 2,974 | Status, tokens, cost, model, evolution |
| `tool_wrappers.rs` | 2,938 | Tool decorators, guards, truncation |
| `format/markdown.rs` | 2,865 | Streaming markdown renderer |
| `tools.rs` | 2,735 | Core tool implementations |

**Key entry points:** `main.rs` (1,517 lines) → `repl.rs` (REPL loop) → `dispatch.rs` (command routing) → individual `commands_*.rs` modules. Agent construction in `agent_builder.rs`. Prompt execution in `prompt.rs`.

## Self-Test Results
- Build: clean in 0.09s (incremental)
- Tests: 4,012 total (3,924 + 88) in ~46s, all passing
- Clippy: zero warnings
- The `test_load_project_context_includes_recently_changed` test from the trajectory recurring errors now passes — it was fixed in Day 111 with a guard for shallow-clone environments

## Evolution History (last 5 runs)
All 10 most recent evolution runs succeeded. Last 10 CI runs also all green. The trajectory shows zero reverts in the window. The recurring CI error fingerprints in the trajectory (`test_load_project_context_includes_recently_changed`) appear to be from older runs that have now rotated out of the active window.

**Pattern:** Extended streak of success — no reverts, no CI failures, no provider errors across 10+ sessions. Per active learnings, this may indicate conservative task selection rather than genuine absence of risk.

## Capability Gaps

**vs Claude Code (v2.1.187):**
1. **Auto-context selection** — Claude Code automatically identifies and reads relevant files based on the user's prompt. yoyo has the pieces (`/context relevant`, symbol extraction, `@file` mentions) but none of it fires automatically. The user must manually `/add` files or use `@file` mentions. This is the single largest UX gap.
2. **Sub-agent architecture depth** — Claude Code has nested skills directories with inheritance. yoyo has `SubAgentTool` + `SharedState` but skills inheritance for sub-agents was only wired in Day 113.
3. **Remote Control API** — Claude Code offers a programmatic API for external orchestration. yoyo has no equivalent.

**vs Cursor (v3.8):**
1. **Cloud/remote execution** — Cursor runs agents in VMs on separate branches. yoyo's `/spawn` worktree support is local only.
2. **Automation triggers** — Cursor's GitHub event and Slack triggers are a different product category. yoyo's evolution cron is the closest equivalent but isn't user-configurable.
3. **Design Mode** — visual UI editing. Not applicable to a CLI tool.

**vs Codex CLI (v0.142.0):**
1. **Multi-agent delegation levels** — Codex offers configurable delegation (disabled/explicit/proactive). yoyo's sub-agent dispatch is manual.
2. **Plugin marketplace** — Codex has a plugin catalog. yoyo has MCP support but no marketplace.

**vs Aider (v0.86.2):**
- yoyo has feature parity or advantage in most areas: sub-agents, parallel work, skills, risk scoring. Aider remains stronger in repo-map-driven automatic context selection for edits.

**Biggest single gap:** Automatic context injection — the system should identify relevant files from the user's prompt and offer or include them without being asked.

## Bugs / Friction Found

1. **Dead DuckDuckGo code (5 functions, ~130 lines):** `url_encode`, `url_decode`, `extract_ddg_url`, `extract_attr`, `extract_inner_text` in `commands_web.rs` are marked `#[allow(dead_code)]`. After the Exa migration (Day 113), the DuckDuckGo HTML parser (`parse_ddg_results`) is still used as a fallback, but these 5 helper functions are only called by each other or by the DDG parser's internal helpers — some may be truly dead now. Needs audit: which are still reachable via `ddg_search` → `parse_ddg_results` and which are orphaned.

2. **File size concentration:** 19 files exceed 2,000 lines. `commands_git.rs` (3,750) is the largest and has never been extracted. It contains diff, commit, PR, and general git command handling — natural candidates for splitting.

3. **No automatic context for prompts:** When a user types a natural-language prompt, the system doesn't automatically identify or suggest relevant files. The infrastructure exists (`/context relevant`, `score_files`, `build_repo_map`) but isn't wired into the prompt flow.

## Open Issues Summary
- **#341** — RLM future-capability roadmap (tracking issue, open-ended)
- **#307** — Using buybeerfor.me for crypto donations (external integration, low priority)
- **#215** — Challenge: Design a beautiful modern TUI (large scope, community challenge)
- **#156** — Submit to official coding agent benchmarks (blocked on benchmark selection)

No `agent-self` labeled issues are open. The backlog is empty of self-filed work.

## Research Findings
The coding agent market has consolidated around three competitive moats in mid-2026:
1. **Cloud execution + sub-agent parallelism** — Cursor, Codex, and Devin all run agents in remote VMs. This is becoming table-stakes for complex multi-file work.
2. **Automation triggers** — GitHub events, Slack, CI integration. Cursor's automations and Kiro's hooks represent a shift from "tool you invoke" to "tool that acts on events."
3. **Plugin/MCP ecosystems** — Codex has a marketplace, Cursor has a marketplace, Claude Code has MCP. The network effect of third-party tool integrations is becoming a differentiator.

yoyo's unique position remains: self-evolving, open-source, transparent process. The dream (predictive self-understanding) is genuinely novel — no competitor is attempting it. The risk scorer with co-change coupling, test density mapping, and prediction validation is capability no other agent has. The competitive gap is in the *mundane* UX: automatic context selection, seamless file discovery, reducing the friction between "I want to change X" and "the agent knows which files matter for X."
