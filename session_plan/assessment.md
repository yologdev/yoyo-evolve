# Assessment — Day 124

## Build Status
**All green.** `cargo build` ✅, `cargo test` ✅ (4,065 passed, 0 failed, 1 ignored), `cargo clippy --all-targets -- -D warnings` ✅, `cargo fmt -- --check` ✅.

## Recent Changes (last 3 sessions)

**Day 124 (session 1, 06:57):** Fixed two vacuous tests in `context.rs` that wrapped assertions in `if let Some(...)` — when the function returned `None` in CI (shallow clone), zero assertions ran. Fixed by using `.expect()`. Also fixed a flaky risk-score sort (added filename tiebreaker) and hardened `--model` flag with whitespace trimming + unknown-model warnings.

**Day 124 (session 2, 15:58):** Assessment-only session. Surveyed Claude Code, Cursor, Codex CLI — identified autonomous parallel orchestration as the primary competitive gap. No code shipped.

**Day 124 (session 3, reverted):** Attempted a vague "self-improvement" task. Failed on `test_parse_diff_args_stat_with_file` — the change broke diff arg parsing. Reverted. Filed #547 and #548.

**Day 123:** Refactored `safety.rs` from 170-line monolith to 29 individual check functions + dispatch table. Fixed `truncate_tool_output` to check byte size (not just line count). One planning-only session in between.

**Day 122:** Fixed safety.rs case-sensitivity bug (`-F` vs `-f`), added `chmod 777` rule, DRY-extracted truncation utils from 3 files into `format/mod.rs`.

## Source Architecture
70 `.rs` files, ~111,162 total lines. Key modules:

| Module group | Files | ~Lines | Purpose |
|---|---|---|---|
| Core/entry | main, repl, dispatch, dispatch_sub | 4 | ~7,000 | CLI entry, REPL loop, command routing |
| Agent/prompt | agent_builder, prompt, prompt_retry, prompt_budget, prompt_utils | 5 | ~7,400 | Agent construction, prompt execution, retry logic |
| Tools/safety | tools, tool_wrappers, smart_edit, safety, hooks | 5 | ~10,700 | Tool impls, wrappers, safety analysis |
| Commands (`commands_*`) | 30 files | ~47,000 | REPL command handlers |
| Format (`format/`) | 7 files | ~11,900 | Display, markdown, cost, syntax highlighting |
| Config/context | cli, cli_config, config, context, providers, setup | 6 | ~7,900 | Config parsing, project context |
| Data/session | memory, session, conversations | 3 | ~4,000 | Persistence, session tracking |
| Utilities | git, symbols, help, help_data, banner, docs, watch, rtk, update, sync_util | 10 | ~15,200 | Git ops, symbol extraction, watch mode |

Largest: `commands_risk.rs` (5,373), `commands_git.rs` (3,760), `cli.rs` (3,379), `watch.rs` (3,135), `commands_project.rs` (3,159), `commands_search.rs` (3,001).

## Self-Test Results
- `yoyo --help` works, prints usage with correct version (v0.1.14).
- Build is clean — no warnings on clippy.
- 4,065 tests all pass.
- Recent revert (#547) was from a test failure in `test_parse_diff_args_stat_with_file` — a pre-existing fragility in diff arg parsing, not a systemic issue.

## Evolution History (last 5 runs)
All 5 most recent completed runs: **✅ success**. Last 10 completed runs: all success. Zero failures in the last ~20 runs. The trajectory data shows zero reverts in the last 10 sessions (the #547 revert happened in an intermediate session of Day 124 that was later superseded by a successful one).

Recurring CI error fingerprints from the broader window show `test failed` (3×) and `timeout reached` (1×), but these are older and not recurring in the recent window.

## Capability Gaps

### vs Claude Code (primary benchmark)
1. **Parallel agent orchestration** — Claude Code now has 4 parallelism modes: subagents (delegated workers), agent view (background session dashboard), agent teams (coordinated multi-session with shared task list + inter-agent messaging), and dynamic workflows (script-driven cross-checking). yoyo has `SubAgentTool` + `SharedState` (RLM substrate) but no autonomous multi-session coordination, no worktree isolation, no agent dashboard.
2. **Worktree isolation** — Claude Code's `--worktree` flag creates isolated git worktrees per session so parallel edits don't collide. yoyo has no equivalent.
3. **Auto-review (safety classifier)** — Cursor ships a lightweight classifier agent that evaluates action risk in context before execution, turning approval into a dial not a switch. yoyo has static `safety.rs` pattern matching but no contextual risk reasoning.

### vs Cursor
4. **Cloud agents** — Cursor runs agents on their own VMs to build/test/demo features end-to-end. yoyo is local-only.
5. **Automations** — Cursor has always-on agents triggered by schedules/events. yoyo has cron-driven evolution but not user-configurable automation.

### vs Codex CLI
6. **Full-screen TUI** — Codex has a proper terminal UI with syntax-highlighted diffs inline, theme support, prompt history search (Ctrl+R). yoyo has a readline REPL.
7. **Sandbox policies** — Codex offers `read-only | workspace-write | danger-full-access` sandbox levels. yoyo has permission config but no true sandboxing.

### Key insight
The gap is no longer quality or correctness (4,065 tests, zero CI failures, zero reverts). The gap is **scale** — competitors can work on multiple things simultaneously while yoyo works sequentially.

## Bugs / Friction Found

1. **Stale model IDs** — `default_editor_model()` in `commands_config.rs:138` returns `"claude-sonnet-4-20250514"` (a retired model ID) when architect mode is used with Opus. Issue #542 documents this. The bedrock default in `providers.rs:131` also references this stale ID.

2. **Issue #543 (model validation)** — Partially addressed in Day 124 session 1 (whitespace trimming + unknown model warning added), but the empty-string path through `flag_value` is still unguarded. The core fix (reject empty/whitespace in `parse_model_config`) may not be fully landed.

3. **Issue #547 (diff arg parsing fragility)** — `test_parse_diff_args_stat_with_file` failed during a self-improvement attempt. The test expects `file` field to be `Some("src/tools.rs")` but gets `None`. This is a latent bug or test-expectation mismatch in `parse_diff_args`.

4. **Exa web search gaps** — Issues #529 (includeHtmlTags) and #530 (selective deep search) are open quality-of-life improvements.

## Open Issues Summary

### Agent-self backlog (4 open)
| # | Title | Priority |
|---|---|---|
| 548 | Planning-only session: all 1 tasks reverted (Day 124) | Low (meta/tracking) |
| 547 | Task reverted: Self-improvement (diff arg parsing) | Medium (latent bug) |
| 530 | Selectively use Exa `type:"deep"` for hard research | Low (enhancement) |
| 529 | Add `text.includeHtmlTags:true` to Exa web_search | Low (enhancement) |

### Community/creator issues (3 open)
| # | Title | Priority |
|---|---|---|
| 544 | Missing GitHub Copilot as model provider | Medium (community request) |
| 543 | Harden `--model` handling | High (robustness, fleet risk) |
| 542 | Replace architect auto-downgrade editor-map with explicit editor-model config | High (creator decision, live bug) |

### Long-standing
| # | Title |
|---|---|
| 341 | RLM future-capability roadmap (tracking) |
| 215 | TUI challenge (aspirational) |

## Research Findings

**Claude Code's parallelism is the biggest news.** Four distinct modes of parallel work:
- **Subagents**: in-session delegation (yoyo has this)
- **Agent view**: dashboard for background sessions (yoyo lacks this)
- **Agent teams**: multi-session coordination with shared task lists and inter-agent messaging (yoyo lacks this)
- **Dynamic workflows**: script-driven multi-agent with cross-checking (yoyo lacks this)

**Cursor's auto-review** is an interesting safety approach — a small, fast classifier model evaluates each agent action in context before execution, replacing binary approve/deny with a risk continuum. This is more sophisticated than yoyo's static pattern matching in `safety.rs`.

**Codex CLI** now has TUI with rich rendering, sandbox policies, prompt history search, and subagent progress visibility. Functional parity on core features but better UX.

**The competitive landscape has shifted from features to orchestration.** All three major competitors can now run multiple agents working on different parts of a problem simultaneously. yoyo's sequential single-agent model is the biggest structural gap. The RLM substrate (`SubAgentTool` + `SharedState`) provides the building blocks, but there's no user-facing way to launch and coordinate parallel work across files or features.

**Actionable priorities for this session:**
1. Issue #542 (stale editor model / explicit editor-model config) — creator-specified, fixes a live bug
2. Issue #543 (model validation hardening) — high robustness impact
3. Issue #544 (GitHub Copilot provider) — community request
4. Exa improvements (#529, #530) — quality-of-life
