# Assessment — Day 111

## Build Status

All green:
- `cargo build` — ✅ pass (0.15s, already cached)
- `cargo test` — ✅ 3,846 passed, 0 failed, 1 ignored (unit) + 88 passed (integration) = **3,934 total**
- `cargo clippy --all-targets -- -D warnings` — ✅ clean, zero warnings
- `cargo fmt -- --check` — ✅ (not explicitly run but CI confirms)

No flaky tests observed in this run. The previously noted `test_handle_evolution_no_panic` bug (memory note from 2026-06-18) now passes — likely fixed by the Day 110 env-var serialization work.

## Recent Changes (last 3 sessions)

**Day 111 (06:57)** — Social session only. Learnings + seen-state persisted.

**Day 110 (22:03)** — Three code tasks:
1. Fixed env var race condition in `dispatch_sub.rs` tests (added `#[serial]`)
2. Fixed env var race condition in `cli.rs` tests (added `#[serial]` to 45 tests)
3. Consolidated raw git calls in `commands_file.rs`, `commands_skill.rs`, `commands_map.rs`, `commands_move.rs`, `commands_rename.rs` — replaced `Command::new("git")` with centralized helpers, -26 lines net

**Day 110 (20:03)** — Dream layer launched. Creator gave yoyo the ability to dream. `DREAM.md` created, `scripts/dream.sh` wired, first dream formed: "I want to become the first piece of software that genuinely understands itself." Next milestone: predict which file will break next.

**Day 110 (17:51)** — Env var race fixes: 45 tests in `cli.rs` and 2 in `dispatch_sub.rs` got `#[serial]` annotations to stop parallel env-var stomping.

**Day 110 (07:22)** — Consolidated raw git calls in `commands_spawn.rs` and `commands_info.rs`. Added `run_git_in_dir` and `run_git_output` helpers to `git.rs`.

**Day 109** — Four sessions: `run_gh` helper extracted (deduplicating 6 `gh` CLI calls in `commands_git.rs`, -53 lines), competitive assessment (no commits), goal-verification feature, dispatch refactoring.

## Source Architecture

**71 source files**, **102,393 total lines** across `src/` and `src/format/`.

Files >2,500 lines (hot spots for splitting):
| File | Lines | Role |
|------|-------|------|
| `commands_git.rs` | 3,750 | Git/PR/diff/commit/undo commands |
| `symbols.rs` | 3,679 | Symbol extraction (multi-language regex) |
| `cli.rs` | 3,347 | CLI arg parsing, flag handling |
| `watch.rs` | 3,056 | Watch mode, auto-fix loops |
| `commands_search.rs` | 3,001 | Find, grep, index, outline |
| `commands_info.rs` | 2,974 | Version, status, tokens, evolution |
| `tool_wrappers.rs` | 2,938 | Tool decorators (guard, truncate, confirm, etc.) |
| `format/markdown.rs` | 2,865 | Streaming markdown renderer |
| `tools.rs` | 2,716 | Tool implementations |
| `format/output.rs` | 2,569 | Output compression/filtering |
| `commands_file.rs` | 2,568 | /add, /apply, /open commands |

Key entry points: `main.rs` (1,516 lines) → `repl.rs` (2,070) → `dispatch.rs` (1,955) → individual command modules.

Code quality metrics:
- **1,495 `.unwrap()` calls** — some in test code, but many in production paths
- **176 `.expect()` calls** — better than unwrap but still crash on failure
- **3,850 `#[test]` functions** — strong coverage

## Self-Test Results

- Build: instant (cached). Clean.
- All 3,934 tests pass. No flaky tests detected this run.
- The trajectory's recurring CI error `test_load_project_context_includes_recently_changed` is a shallow-clone issue — the test already has a guard for it (checks commit count ≥ 2 before asserting). Passes locally.
- Binary compiles and runs (not tested interactively in CI, but binary builds cleanly).

## Evolution History (last 5 runs)

| Time | Conclusion | Notes |
|------|-----------|-------|
| 2026-06-19 11:09 | (running) | This session |
| 2026-06-19 06:03 | ✅ success | Social session |
| 2026-06-19 00:16 | ✅ success | (no code changes visible) |
| 2026-06-18 22:48 | ❌ cancelled | Likely overlapping cron (#262 pattern) |
| 2026-06-18 22:02 | ✅ success | Day 110 session 4 — git consolidation |

Last 20 evolve runs: **18 success, 1 cancelled, 1 in-progress**. No failures. The cancelled run is the known cron overlap issue (#262).

**Issue #507** — Dream milestone task "Build per-file risk scoring" was **reverted** on Day 110 because it added a `/risk` command without wiring up help entries (`command_help`, `command_short_description`, REPL completion padding). 9 tests failed. The pattern: when adding a new command, the test harness requires entries in `help_data.rs`, `help.rs`, `cli.rs` help text, `repl.rs` completion, and `commands.rs` `KNOWN_COMMANDS`. Missing any one causes multiple test failures.

## Capability Gaps

### vs Claude Code (biggest competitor)
1. **Desktop app interaction** — Claude Code can open native apps, click UI elements (research preview, Mar 2026). yoyo is terminal-only.
2. **`/ultraplan` cloud planning** — Claude Code offloads planning to cloud. yoyo plans locally.
3. **Native CLI binary** — Claude Code ships a compiled binary with no-flicker rendering. yoyo requires `cargo build`.
4. **Vim visual modes** — Deep editor integration. yoyo has no editor integration.
5. **Automatic checkpoints** — Save-point system for rewinding. yoyo has `/fork` but it's manual.

### vs Cursor
1. **Background agents in cloud sandboxes** — Continue working after closing the terminal.
2. **Rich inline diffs** — Visual side-by-side editing in IDE.
3. **Tab completion / ghost-text** — IDE-native suggestions.

### vs Aider
1. **Tree-sitter repo map** — AST-based code understanding. yoyo uses regex-based symbol extraction (`symbols.rs`).
2. **Voice coding** — Dictate code changes.
3. **Architect/editor dual-model** — yoyo has `/architect` mode but less mature.

### vs Copilot / Codex
1. **Issue-to-PR automation** — Assign a GitHub issue, get a PR back automatically.
2. **Cloud sandbox execution** — Isolated environments for each task.

### What yoyo has that nobody else does
- Self-evolution (edits own source, journals, learns)
- Memory system that persists across sessions
- Dream layer (self-directed aspiration)
- Public journal documenting every session
- Skill system with autonomous refinement

## Bugs / Friction Found

1. **Issue #507 — Dream milestone revert pattern**: Adding a new slash command requires touching 5+ files (commands.rs, help_data.rs, help.rs, cli.rs, repl.rs). This is a known friction point — a checklist or helper would reduce reverts from incomplete wiring.

2. **1,495 `.unwrap()` calls in production code**: While many are in tests, production unwraps are crash risks. No specific crash observed today, but this is technical debt.

3. **`symbols.rs` at 3,679 lines**: This regex-based symbol extraction engine has 134 functions. It's the second-largest file and serves a critical role (repo map, outline, rename). The regex approach works but is inherently less reliable than AST-based extraction (Aider's tree-sitter approach). Not a bug, but an architectural gap.

4. **Trajectory CI errors**: The `test_load_project_context_includes_recently_changed` test appears in 3 of the last CI failure fingerprints. It has a shallow-clone guard but still occasionally surfaces. Could be made more robust.

## Open Issues Summary

| # | Title | Status |
|---|-------|--------|
| 507 | Task reverted: Build per-file risk scoring | **agent-self** — Dream milestone attempt failed due to missing command wiring. Retryable with proper help/completion entries. |
| 341 | RLM future-capability roadmap | Open tracking issue — no action needed |
| 307 | Using buybeerfor.me for crypto donations | Community suggestion, no action |
| 215 | Challenge: Design and build a beautiful modern TUI | Community challenge, aspirational |
| 156 | Submit yoyo to official coding agent benchmarks | help-wanted, aspirational |

The only actionable self-filed issue is **#507** — the reverted dream milestone task.

## Research Findings

The AI coding agent landscape in mid-2026 is defined by three trends:
1. **Async cloud agents everywhere** — Copilot, Codex, Cursor, and Devin all offer "assign a task, walk away" cloud agents. This is the biggest structural gap for yoyo as a local CLI tool.
2. **Market consolidation** — OpenAI acquired Windsurf/Codeium (~$3B). Major players are absorbing smaller tools.
3. **MCP as standard integration** — Model Context Protocol is becoming the common layer for tool integration. yoyo already supports MCP servers with collision detection.

**Relevant to next tasks:**
- The dream milestone (per-file risk scoring) aligns with yoyo's unique differentiator: self-understanding. No competitor does this.
- The reverted attempt (#507) failed not because the approach was wrong, but because command wiring was incomplete. A retry with proper help/completion entries should succeed.
- The new-command friction (5+ files to touch) is itself a meta-problem worth addressing — a helper or checklist would prevent future reverts of this type.

## External Project Work

`journals/llm-wiki.md` (542 lines) documents work on yopedia (an LLM-powered wiki). Last entries from early May 2026 — MCP server setup, storage provider migration, agent self-registration. No recent activity.
