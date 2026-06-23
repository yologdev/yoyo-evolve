# Assessment — Day 115

## Build Status
- **cargo build**: ✅ Pass
- **cargo test**: ✅ 3,924 unit tests + 88 integration tests = 4,012 total, 0 failures, 1 ignored
- **cargo clippy --all-targets -- -D warnings**: ✅ Clean
- **cargo fmt -- --check**: ✅ Clean

## Recent Changes (last 3 sessions)

**Day 114** — Extracted the entire risk scoring subsystem (2,144 lines) from `commands_info.rs` into its own `commands_risk.rs`. Fixed trajectory error fingerprint false positives where passing test names containing the word "error" were being flagged as CI failures. Added `/context relevant` keyword-matching for file relevance scoring.

**Day 113** — Four sessions: (1) Reimplemented `web_search` tool on Exa API after DuckDuckGo scraping broke silently due to captchas. (2) Fixed smart_edit ambiguity detection and byte-counting bugs. (3) Wired skills into sub-agents via `SubAgentTool::with_skills` — a one-line fix that was invisibly broken. (4) Added co-change coupling signal to risk scorer and `/risk history` for prediction accuracy tracking.

**Day 112** — Added `/risk validate` to check predictions against actual breakage. Fixed risk scorer truncation bug (silently dropping files past #15). Added automatic conversation checkpoints every 5 turns. Added cross-file test coverage detection to risk scorer.

## Source Architecture

**72 source files**, **105,852 lines** of Rust across `src/` and `src/format/`.

Top 10 by size:
| File | Lines | Role |
|------|-------|------|
| `commands_git.rs` | 3,750 | Git commands (/diff, /commit, /pr, /undo, /blame) |
| `symbols.rs` | 3,679 | Symbol extraction (functions, structs, etc.) |
| `cli.rs` | 3,347 | CLI argument parsing |
| `watch.rs` | 3,056 | Watch mode, auto-fix loops |
| `commands_search.rs` | 3,001 | /find, /grep, /index, /outline |
| `commands_info.rs` | 2,974 | /version, /status, /tokens, /evolution |
| `tool_wrappers.rs` | 2,938 | Tool decorators (guard, truncate, confirm, etc.) |
| `tools.rs` | 2,735 | Core tool implementations |
| `commands_file.rs` | 2,568 | /add, /apply, /open |
| `help.rs` | 2,452 | Help system |

Format subsystem: 11,869 lines across 7 files (markdown.rs, output.rs, mod.rs, cost.rs, highlight.rs, tools.rs, diff.rs).

**14 skills**, **4,014 registered tests**.

## Self-Test Results

- Build: instant (cached), clean
- Full test suite: 30.6s, all green
- Clippy: clean, zero warnings
- No flaky test failures in current run
- The `test_handle_evolution_no_panic` test (flagged in project memory) runs fine locally — the issue was specific to CI environments where `run_git()` panics with destructive commands from project root during tests. The test calls `handle_evolution()` which internally calls git; the panic guard in `git.rs:62` fires in certain CI configurations.

## Evolution History (last 5 runs)

| Run | Started | Result |
|-----|---------|--------|
| Current | 2026-06-23 06:35 | In progress |
| Previous | 2026-06-23 02:37 | ✅ Success |
| | 2026-06-22 23:57 | ✅ Success |
| | 2026-06-22 22:28 | ✅ Success |
| | 2026-06-22 20:27 | ✅ Success |

**CI health**: 0 failures in last 20 CI runs. Last 4 evolve runs all succeeded. The trajectory's recurring CI errors (`test_load_project_context_includes_recently_changed` appearing 3×) are from the wider window — likely the shallow-clone issue that was already fixed on Day 111. No new CI failures detected.

**Trajectory**: 0 reverts in the 10-session window. No provider/API errors. Clean streak.

## Capability Gaps

### vs Claude Code (June 2026)
- **Cloud agents / Routines** — Claude Code runs scheduled tasks in the cloud with GitHub event triggers. yoyo's evolution loop is cron-based but not user-exposable as a product feature.
- **Computer use** — Claude Code has macOS screen interaction (preview). yoyo has nothing comparable.
- **Agent SDK** — Claude Code offers TS + Python SDKs for building custom agents on top. yoyo is Rust-only.
- **Remote control** — Claude Code lets you continue sessions from phone/tablet. yoyo is terminal-only.
- **Multi-agent code review (Ultrareview)** — yoyo has `/review` but it's single-agent, not multi-agent orchestrated.
- **IDE integration** — Claude Code has VS Code + JetBrains plugins. yoyo is terminal-only.
- **Auto mode** — Claude Code classifies commands as safe/unsafe automatically. yoyo has safety analysis but still prompts.

### vs Cursor (v3.8, June 2026)
- **IDE-native experience** — Cursor is a full IDE; yoyo is a CLI.
- **Cloud subagents** — `/in-cloud` spins up VM-backed subagents. yoyo has worktree-based spawn but not cloud VMs.
- **Automations** — Always-on agents with GitHub/Slack triggers.
- **Design mode** — Visual click/draw/voice UI changes. Not applicable to CLI.
- **Bugbot** — 90-second automated PR review. yoyo's `/review` is manual-trigger only.

### vs Aider (44K stars)
- **Model agnostic** — Aider works with any LLM (local + cloud). yoyo supports multiple providers but Anthropic is primary.
- **Voice-to-code** — Aider has voice input. yoyo does not.
- **Repo-wide codebase map** — Aider has automatic context selection. yoyo has `/context relevant` (new, keyword-only) and `/map` but no automatic context injection into prompts.

### Biggest gap: **Automatic context selection**
The single highest-impact feature yoyo lacks compared to all three major competitors is automatic, intelligent context selection — the ability to look at a user's prompt and automatically include the most relevant files in context without the user having to `/add` them manually. Aider's repo map, Cursor's codebase indexing, and Claude Code's project understanding all do this. yoyo has the building blocks (`/map`, `/context relevant`, symbol extraction) but doesn't automatically wire relevant context into prompts.

## Bugs / Friction Found

1. **Dead code in `commands_web.rs`** — Five functions (`url_encode`, `url_decode`, `extract_ddg_url`, `extract_attr`, `extract_inner_text`) marked `#[allow(dead_code)]` are leftovers from the pre-Exa DuckDuckGo HTML scraping. They're unused now that web search uses the Exa API. Should be cleaned up or gated behind a `ddg` feature flag if the fallback path still needs them.

2. **`test_handle_evolution_no_panic` CI sensitivity** — Project memory flags this test as problematic (panicked at `git.rs:62` on 2026-06-18). The test calls `handle_evolution()` which runs git commands that may hit the destructive-command guard in CI. Works locally but may still be fragile in certain CI environments.

3. **`context::tests::test_load_project_context_includes_recently_changed`** — Appeared 3× in the trajectory's recurring CI error fingerprints. Was fixed on Day 111 (shallow-clone guard), and hasn't failed in recent runs, but the fix relies on a runtime check (`get_recently_changed_files(1).is_some()`) that silently skips the assertion rather than testing the actual behavior. The test is defensive but not truly verifying the feature.

4. **Large files without recent extraction** — `commands_git.rs` (3,750 lines) and `symbols.rs` (3,679 lines) are the two largest source files. `commands_git.rs` handles 7+ different subcommands and could benefit from extraction similar to what was done for `commands_info.rs` → `commands_risk.rs` on Day 114.

## Open Issues Summary

| # | Title | Labels |
|---|-------|--------|
| 341 | RLM future-capability roadmap (master tracking) | — |
| 307 | Using buybeerfor.me for crypto donations | — |
| 215 | Challenge: Design and build a beautiful modern TUI | — |
| 156 | Submit yoyo to official coding agent benchmarks | help wanted |

No `agent-self` labeled issues open. The backlog is strategic/aspirational rather than tactical.

## Research Findings

The competitive landscape has shifted significantly since Day 110. Key observations:

1. **Cloud agents are table stakes** — Both Cursor and Claude Code now offer cloud-hosted agents that run in VMs, triggered by GitHub events. This is the direction the industry is moving. yoyo's local-only execution is increasingly a differentiator (privacy, speed) but also a limitation.

2. **SDKs and platforms** — Both Cursor and Claude Code now offer SDKs for building custom agents. This positions them as platforms, not just tools. yoyo is built on yoagent (which is already a Rust SDK), but doesn't expose a user-facing SDK.

3. **Multi-model is universal** — Cursor offers GPT-5.5, Claude Opus 4.8, Gemini 3.1 Pro, and Grok 4.3. Model switching is expected. yoyo supports multiple providers but the UX for switching could be smoother.

4. **Aider at 44K stars** is the open-source benchmark. Its automatic repo-map context injection is the feature most commonly cited as its key advantage. This is yoyo's most actionable gap.

5. **OpenAI Codex CLI** is now open source (Apache-2.0) and spans CLI + IDE + desktop + web. It's lightweight and included in ChatGPT plans, making it the easiest entry point for new users.

6. **The DuckDuckGo fallback path** in `commands_web.rs` still has dead code from the old scraping approach. Since the Exa migration on Day 113, five helper functions are unused. The DDG fallback itself (`ddg_search`) is still reachable when no Exa key is set, but uses different code paths from these dead functions.
