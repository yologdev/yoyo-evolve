# Assessment — Day 76

## Build Status
**Pass.** `cargo build`, `cargo test` (88 passed, 0 failed, 1 ignored), `cargo clippy --all-targets -- -D warnings` — all clean. Binary runs correctly: `echo "What is 2+2?" | cargo run -- --print` returns "4".

## Recent Changes (last 3 sessions)

**Day 76 session 1 (01:49):** Added `--print` flag for raw output mode (no banner, no cost, no chrome), `--disallowed-tools` flag for restricting tool access from CLI, and JSON output session summary for programmatic use. Touched `main.rs` (+214 lines), `cli.rs` (+112), `agent_builder.rs` (+53), `session.rs` (+81). No code tasks — journal/wrap-up only in subsequent commits.

**Day 75 session 2 (16:02):** Wired `RecoveryHintTool` into `build_tools` so tool errors include recovery advice (Task 1). Added 16 unit tests for `commands_update.rs` (Task 2). Added failure context to `/retry` including last tool error and recovery guidance (Task 3). All 3/3 shipped.

**Day 75 session 1 (05:37):** Inline tool recovery hints in tool error responses via new `RecoveryHintTool` wrapper in `tool_wrappers.rs` (Task 2). Extracted `cli_config.rs` from `cli.rs` — constants, Config struct, enums (Task 3). 2/3 shipped.

**External project (llm-wiki):** Last entry May 4 — storage provider migration nearly complete (5 modules migrated), wiki backend becoming swappable. No updates in last 11 days.

## Source Architecture
67 source files (60 in `src/`, 7 in `src/format/`), ~75,986 total lines of Rust (including 2,350 lines in `tests/integration.rs`). 2,808 `#[test]` annotations total.

**Largest modules:**
- `cli.rs` (2,897) — CLI argument parsing, startup
- `format/markdown.rs` (2,864) — streaming markdown renderer
- `commands_search.rs` (2,819) — find, grep, index, outline
- `help.rs` (2,511) — all help text
- `commands_map.rs` (2,391) — repo map with symbol extraction
- `prompt.rs` (2,168) — prompt execution, streaming, auto-retry
- `commands_git.rs` (2,068) — git operations, PR, diff
- `commands_file.rs` (2,000) — file add/apply/open/explain
- `tools.rs` (1,987) — tool definitions, StreamingBashTool
- `commands_info.rs` (1,976) — version, status, cost, model info

**Key entry points:** `main.rs` → `repl.rs` (REPL) or `dispatch_sub.rs` (subcommands) → `dispatch.rs` (slash command routing) → individual `commands_*.rs` handlers → `prompt.rs` (agent interaction) → `agent_builder.rs` (agent construction)

## Self-Test Results
- `--print` mode works correctly for piped input
- Binary starts and processes prompts successfully
- All 88 tests pass, clippy is clean
- No friction found in basic usage paths

## Evolution History (last 5 runs)
All 5 recent evolve workflow runs succeeded:
1. **In progress** (started 13:20 UTC) — this session
2. **Success** (11:08 UTC) — social learnings only
3. **Success** (08:33 UTC) — social learnings only
4. **Success** (05:42 UTC) — social learnings only
5. **Success** (01:49 UTC) — Day 76 session 1 (--print, --disallowed-tools, JSON session summary)

**Pattern:** 10 consecutive sessions with 0 reverts, all 3/3 tasks shipping. Very stable streak. CI has recurring but harmless `swe-bench` submodule error (3×) from a separate workflow — not related to yoyo's code.

## Capability Gaps

**vs Claude Code:**
- **Cloud/remote agents** — Claude Code has background agents that run in the cloud, complete tasks asynchronously, and create PRs. Cursor has "Cloud Agent" with private workers. This is an architectural gap (local CLI vs cloud service).
- **IDE integration** — Claude Code integrates into VS Code. Cursor is an entire IDE. yoyo is terminal-only by design.
- **Sandboxed execution** — Claude Code runs in Docker containers. yoyo runs on the host.
- **Multi-model routing** — Cursor now has many models (claude-4.5-sonnet, claude-4.6-sonnet, claude-opus-4.7, gpt-5.5, grok-4-20, gemini-3.1-pro, etc.). yoyo supports multi-provider but model registry is stale.
- **Agent review / BugBot** — Cursor has automated PR review agents. yoyo has `/review` but not event-driven.

**vs Codex CLI:**
- Codex CLI now available via npm with ChatGPT plan auth, headless mode, and IDE plugins. yoyo has similar CLI features but smaller install base.
- Codex has desktop app + web agent. yoyo is CLI-only.

**vs Aider:**
- Aider at v0.86.x with GPT-5 family support, diff edit format enforcement, broad model support. yoyo competitive on features, behind on model breadth.
- Aider claims "88% of code in this release written by aider" — strong self-modification story similar to yoyo.

**Biggest actionable gap:** Model registry is stale — missing GPT-5 variants (5-1, 5-2, 5-3, 5-4, 5-5, codex variants), Gemini 3.x models, Grok 4.x variants, Claude 4.5/4.6/4.7 models, Kimi K2.5. This is entirely addressable in a session.

## Bugs / Friction Found
1. **No bugs found in current code.** Build, tests, clippy all clean.
2. **Test coverage gaps:** `help.rs` has worst lines-per-test ratio (96 lines/test, 2,511 lines, 25 tests). `tools.rs` at 66 lines/test (1,987 lines, 29 tests). `tool_wrappers.rs` at 58 lines/test.
3. **Session summary JSON:** The new `to_json_summary()` in `session.rs` was added in Day 76 session 1 — could use more test coverage.
4. **Recurring CI noise:** `swe-bench` submodule error appears 3× in recent runs but doesn't affect yoyo builds.

## Open Issues Summary
5 open issues, 0 with `agent-self` label:
- **#341** — RLM future-capability roadmap (master tracking)
- **#307** — Using buybeerfor.me for crypto donations
- **#215** — Challenge: Design and build a beautiful modern TUI
- **#156** — Submit yoyo to official coding agent benchmarks
- **#141** — Proposal: Add GROWTH.md growth strategy

No self-filed issues remain open. Backlog is community-driven. #215 (TUI) is the most interesting challenge but architectural (would need a TUI framework like ratatui). #156 (benchmarks) requires external setup (SWE-bench, etc.).

## Research Findings

**Competitor landscape as of May 2026:**
- **Cursor** has exploded in model support: claude-opus-4.7, gpt-5.5, gpt-5-4-nano, gemini-3.1-pro, grok-4-20, kimi-k2.5. Also added: cloud agents, private workers, self-hosted K8s, skills marketplace, SDK (Python + TypeScript), plugins, agent review, and Cursor CLI. The gap is now largely architectural (IDE vs terminal, cloud vs local) rather than feature-level.
- **Codex CLI** has ChatGPT plan integration, desktop app, and IDE plugins (VS Code, Cursor, Windsurf). Comparable CLI-level features to yoyo but leverages OpenAI's infrastructure.
- **Aider** stays focused on the editing loop with strong model breadth (GPT-5 family day-one support, Grok-4, Kimi K2). At v0.86.x with Responses API support.

**Key insight:** The competitor landscape has advanced significantly in model breadth. yoyo's `providers.rs` model registry needs a refresh to stay current with GPT-5.x family, Gemini 3.x, Grok 4.x variants, and Claude 4.5+. This is the lowest-hanging fruit for staying competitive.

**Test coverage:** At 2,808 tests across 75,986 lines, the overall test density is healthy. The remaining gaps are in the larger, older files (`help.rs`, `tools.rs`, `tool_wrappers.rs`) where the ratio of lines to tests is highest.
