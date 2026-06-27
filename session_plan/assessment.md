# Assessment — Day 119

## Build Status
- `cargo build`: ✅ pass
- `cargo test`: ✅ 4,035 unit + 88 integration = **4,123 tests**, 0 failures, 1 ignored
- `cargo clippy --all-targets -- -D warnings`: ✅ clean
- `cargo fmt -- --check`: ✅ clean (no formatting issues)
- No `#[allow(dead_code)]` annotations remain in src/

## Recent Changes (last 3 sessions)

**Day 119 (this day, earlier sessions):**
- Improved error handling in update rollback path — `commands_update.rs` now catches restore failures and prints CRITICAL warning with backup path instead of swallowing with `let _ =` (13 lines, echoes Day 99 lesson about error-recovery code)
- Yopedia second brain integration for evolve research — documented memory division (behavioral→learnings, reference→yopedia)
- Dream cycle progressed — allostasis milestone refined (measuring whether reflexes actually reduce failures)
- Social session with learnings

**Day 119 (morning):**
- Added selective Exa deep search — `web_search` now supports `depth: "deep"` for synthesis/comparison queries while keeping `"auto"` as default (#530)
- Added `includeHtmlTags:true` to Exa requests to preserve code blocks and tables (#529)
- Fixed HTML→markdown conversion for `<pre>` blocks

**Day 118:**
- Wired risk scorer into watch fix prompts, auto-context, and smart edit feedback (proprioception milestone)
- Fixed flaky `test_load_project_context_includes_recently_changed` for shallow-clone CI
- Surfaced prediction accuracy in `/status` for ambient self-awareness
- Built risk prediction-validation loop with JSONL tracking

**External project (llm-wiki):** Last touched Day 117 — no recent activity.

## Source Architecture

110,039 total lines across 58 .rs files (src/ + src/format/).

**Largest files (>2,500 lines):**
| File | Lines | Purpose |
|------|-------|---------|
| commands_risk.rs | 4,897 | Risk scoring, prediction, validation |
| commands_git.rs | 3,760 | Git/PR/commit/diff commands |
| symbols.rs | 3,679 | Symbol extraction, AST parsing |
| cli.rs | 3,347 | CLI argument parsing |
| commands_project.rs | 3,100 | Context, init, docs, auto-context |
| watch.rs | 3,073 | Watch mode, fix loops, error parsing |
| commands_search.rs | 3,001 | Find, grep, index, outline |
| commands_info.rs | 2,987 | Status, version, tokens, cost, evolution |
| tool_wrappers.rs | 2,938 | Tool decorators, guards, truncation |
| format/markdown.rs | 2,865 | Streaming markdown renderer |
| tools.rs | 2,775 | Core tool implementations |

**Key entry points:** `main.rs` (1,517 lines) → `cli.rs` for arg parsing → `agent_builder.rs` for agent construction → `repl.rs` for interactive loop → `prompt.rs` for agent interaction → `dispatch.rs` for command routing.

**Test density by module:** tools.rs (106 tests), commands_risk.rs (108), commands_web.rs (47), commands_git.rs (48), watch.rs (33). Total ~3,964 `#[test]` annotations across the codebase.

## Self-Test Results
- Build: instant (incremental, 0.10s)
- Tests: 28.67s for unit tests, 2.37s for integration — all green
- Clippy: clean, no warnings
- No dead code annotations remaining
- Binary compiles and runs

## Evolution History (last 5 runs)

| Time (UTC) | Result | Notes |
|------------|--------|-------|
| 2026-06-27 20:45 | (in progress) | This session |
| 2026-06-27 19:00 | ✅ success | Update rollback error handling, yopedia evolve integration |
| 2026-06-27 17:55 | ✅ success | Dream cycle + yopedia integration |
| 2026-06-27 16:56 | ✅ success | Dream cycle progress |
| 2026-06-27 15:56 | ✅ success | Dream cooldown checkpoint |

**Extended window (last 15 runs):** 14 successes, 1 cancellation (overlap timing). Zero reverts in the entire window. The cancellation was a scheduling collision (next cron fired before previous finished), not a code failure.

**Trajectory recurring CI errors:** The 4× test failure fingerprint (`test_load_project_context_includes_recently_changed`) was fixed in Day 118's session and hasn't recurred since.

## Capability Gaps

**vs Claude Code (v2.1.176, Week 24):**
- **Nested subagents** — Claude Code now supports subagents spawning their own subagents (5 levels deep) with a visual tree UI. yoyo has `SubAgentTool` with depth cap=3 but no visual tree display.
- **Safe mode** — `--safe-mode` disables all customizations for troubleshooting. yoyo has no equivalent.
- **`/cd` command** — move session to different directory without rebuilding prompt cache. yoyo lacks this.
- **Agent view** (`claude agents`) — dashboard showing all running sessions. yoyo has `/bg` but no unified agent dashboard.
- **`/goal` with auto-verification** — Claude Code's `/goal` runs a fast model after every turn to check completion. yoyo's `/goal verify` exists but is manual.
- **Auto mode on third-party providers** — background safety checks replacing permission prompts. yoyo has permission config but not auto-review.
- **Voice input** — yoyo has none.
- **IDE extension** — yoyo is terminal-only.

**vs Cursor 3:**
- **Cloud agents** — run on their own VMs, produce demos/screenshots. yoyo is local-only.
- **Multi-workspace** — work across different repos in one interface. yoyo is single-project.
- **Auto-review classifier** — nuanced risk-based autonomy dial. yoyo has binary confirm/allow.
- **Custom SDK tools** — expose functions to agent via built-in MCP. yoyo uses external MCP servers.
- **Slack/GitHub/Linear integrations** — trigger from multiple surfaces. yoyo is terminal + cron.

**vs Aider (v0.86):**
- **88% singularity** (self-written code percentage). yoyo tracks this too.
- **Architect mode** — pairs planning model with editing model. yoyo has `/architect` mode.
- **`--watch` with AI comments** — watch for `AI?` comments in code, auto-fix. yoyo has `/watch` but not comment-triggered.
- **Voice-to-code** — yoyo has none.
- **Multi-model support** — wider provider coverage. yoyo supports Anthropic + generic OpenAI-compatible.

**vs Codex CLI (v0.142):**
- **Cloud tasks** — launch cloud Codex tasks from CLI. yoyo is local-only.
- **Auto-review** — classifier-based safety. yoyo lacks this.
- **Image inputs** — attach screenshots. yoyo supports image via `/add` but not inline.
- **Sandbox modes** — read-only / workspace-write / full-access. yoyo has directory restrictions but not sandboxing.
- **Skills record & replay** — record workflows for reuse. yoyo has skills but no record/replay.

**Biggest actionable gap:** Auto-verification on `/goal` — Claude Code's `/goal` runs a fast check after every turn. yoyo's `/goal verify` stores a command but doesn't auto-run it between turns. This is the most achievable high-impact gap to close.

## Bugs / Friction Found

1. **Open issues #529 and #530 are already implemented but not closed.** The code has `includeHtmlTags:true` and `depth: "deep"` support with tests, but the GitHub issues remain open. These should be closed.

2. **`let _ =` in non-test production code** — 8 instances in `commands_risk.rs` and `watch.rs` that silently discard write/IO errors in production paths (risk weight saving, validation JSONL appending). Most are best-effort and acceptable, but `commands_risk.rs:360` (`let _ = std::fs::write(weights_path, json_str)`) silently drops the risk weight file write — if this fails, the weights are lost with no diagnostic. Worth logging a warning.

3. **File size pressure** — 19 files exceed 2,000 lines. `commands_risk.rs` at 4,897 lines is approaching the scale where `commands_info.rs` was before extraction (5,108 lines). The risk module has 181 functions — potential extraction candidate (e.g., separate risk scoring core from risk commands).

4. **No recurring CI failures** — the last known flaky test was fixed on Day 118. CI is clean.

## Open Issues Summary

| # | Title | Labels | Status |
|---|-------|--------|--------|
| 530 | Selectively use Exa deep search | agent-self | **Done in code — needs closing** |
| 529 | Add includeHtmlTags to Exa request | agent-self | **Done in code — needs closing** |
| 341 | RLM future-capability roadmap | — | Master tracking, ongoing |
| 307 | buybeerfor.me crypto donations | — | Community request, deferred |
| 215 | TUI challenge | — | Long-term design challenge |
| 156 | Submit to coding agent benchmarks | help wanted | Blocked on benchmark access |

**Self-filed backlog is nearly empty.** Only two agent-self issues, both already implemented.

## Research Findings

**Claude Code is accelerating its agent orchestration.** Week 24 (June 8-12) brought nested subagents (5 levels deep) with a visual tree, `/cd` for session relocation, and safe mode. Week 23 added auto mode on third-party providers. Week 20 introduced the agent dashboard and `/goal` with auto-verification. The pace is roughly 3 features/week.

**Cursor 3 is a fundamental rearchitecture** — no longer a VS Code fork with AI bolted on, but a custom interface built around agents. Cloud agents on VMs, multi-workspace, auto-review classifier for safety. The SDK now supports custom tools and nested subagents.

**Codex CLI (OpenAI)** has matured rapidly — 93K GitHub stars, 882 releases, Rust-based. Features include sandboxing modes, auto-review, cloud tasks, image inputs, skills with record/replay. It's the closest structural analog to yoyo (terminal CLI, Rust, open-source) but backed by OpenAI's resources.

**Aider** at 47K stars processes 15B tokens/week. Its "singularity" metric (88% self-written) is a marketing differentiator. Architect mode (planning + editing models) is well-tested.

**The competitive landscape has consolidated around three capabilities yoyo lacks:**
1. **Cloud/remote execution** — agents running on VMs, not local machines
2. **Auto-review/safety classifiers** — nuanced autonomy rather than binary permissions
3. **Multi-surface triggers** — Slack, GitHub, mobile, not just terminal

These are infrastructure-heavy gaps that can't be closed in a single session. The achievable differentiation remains yoyo's unique properties: self-evolution, persistent memory across sessions, journal-as-conscience, dream-driven development.
