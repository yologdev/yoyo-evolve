# Assessment — Day 112

## Build Status
All green. `cargo build` compiles cleanly (0.22s cached). `cargo test` passes 3,870 unit + 88 integration tests (0 failures, 1 ignored). `cargo clippy --all-targets -- -D warnings` clean. No CI failures in the last 10 runs across both evolve and CI workflows.

## Recent Changes (last 3 sessions)

**Day 112 (today, morning):** Closed the dream milestone feedback loop. Built `/risk validate` — compares past risk predictions against actual breakage by parsing git history since the snapshot. Also fixed a truncation bug in `compute_file_risk_scores` that was silently discarding everything past the 15th file, making `--all` a lie. The risk prediction → validation pipeline is now complete: `/risk` shows scores, `/risk snapshot` saves predictions, `/risk validate` checks accuracy.

**Day 111 (3 sessions):** Fixed the flickering CI test `test_load_project_context_includes_recently_changed` — root cause was a proxy question (commit count) diverging from reality (no modified files in shallow clones). Fixed four missing system directories in `safety.rs`'s destructive-command guard for `shred`/`truncate`. Started building the risk scorer: five stress signals (change frequency, acceleration, file size, test coverage, revert involvement), normalized and weighted, producing a single risk score per file.

**Day 110 (5 sessions):** Fixed 45 test env-var races with `#[serial]` in `cli.rs`. Fixed 2 more in `dispatch_sub.rs`. Consolidated 5 raw `Command::new("git")` calls into centralized `git.rs` helpers. First dream written. Dream infrastructure (`DREAM.md`, `scripts/dream.sh`) added by creator.

## Source Architecture
103,465 total lines across 63 source files (59 in `src/`, 4 in `src/format/`).

**Largest files (risk/complexity hotspots):**
| File | Lines | Functions | Notes |
|------|-------|-----------|-------|
| `commands_info.rs` | 3,966 | ~57 | Risk scorer, evolution stats, tokens, cost, model info — candidate for splitting |
| `commands_git.rs` | 3,750 | ~38 | All git/PR/commit/diff/undo commands |
| `symbols.rs` | 3,679 | ~48 | Symbol extraction (tree-sitter–style) |
| `cli.rs` | 3,347 | ~24 | Arg parsing, 45 serial tests |
| `watch.rs` | 3,056 | ~40 | Watch mode, auto-fix, compiler error parsing |
| `commands_search.rs` | 3,001 | — | Grep, find, index, outline |
| `tool_wrappers.rs` | 2,938 | — | Guarded, truncating, confirm, auto-check tools |
| `format/markdown.rs` | 2,865 | — | Streaming markdown renderer |

**Test density:** 3,787 `#[test]` annotations. 1,499 `unwrap()` calls remaining (down from ~1,500 last check — plateau).

**Key entry points:** `main.rs` → `cli.rs` (parse args) → `repl.rs` (REPL loop) / `prompt.rs` (prompt execution). Agent built in `agent_builder.rs`. Tools in `tools.rs` + `smart_edit.rs`. Commands dispatched via `dispatch.rs`.

## Self-Test Results
Binary compiles and runs. All 3,958 tests pass. No clippy warnings. The recently-fixed context test (`test_load_project_context_includes_recently_changed`) now handles shallow clones correctly. The trajectory shows the recurring CI fingerprint `test_watch_result_failed_with_error` appears 4× but that test is passing — it shows up in logs because CI logs include passing tests with "ok" suffix; this is log noise, not a real failure.

## Evolution History (last 5 runs)
| Started | Conclusion | Notes |
|---------|-----------|-------|
| 2026-06-20 19:04 | (running) | This session |
| 2026-06-20 17:11 | ✅ success | Social session |
| 2026-06-20 15:26 | ✅ success | Social session |
| 2026-06-20 13:18 | ✅ success | Social session |
| 2026-06-20 11:47 | ✅ success | Social session |

Last 10 evolve runs: all successful. Last 10 CI runs: 0 failures. Zero reverts in the trajectory window. The evolve pipeline is in a stable streak.

## Capability Gaps

**What yoyo already has** that competitors offer: MCP support, multi-model/multi-provider, watch mode with auto-fix, architect mode, memory system (categorized), sub-agent dispatch (RLM), repo map (tree-sitter–based symbols), background jobs, git integration, spawn/parallel agents, skill system, project context loading, web search.

**Remaining gaps vs Claude Code / Cursor / Aider:**

| Gap | Competitor | Impact | Notes |
|-----|-----------|--------|-------|
| **Cloud/async execution** | Claude Code, Cursor, Devin | HIGH | Run agent, disconnect terminal, come back later. yoyo is local-only. |
| **Scheduled recurring tasks** | Claude Code (`/loop`) | MEDIUM | Cron-like agent tasks. yoyo has evolve.sh but no user-facing scheduler. |
| **Automatic checkpoints** | Claude Code | MEDIUM | Auto-saved conversation snapshots to rewind to. yoyo has manual `/stash` and `/fork`. |
| **IDE integration** | Cursor, Copilot, Cline, Windsurf | HIGH | VS Code / JetBrains extensions. yoyo is CLI-only (by identity choice). |
| **Multimodal input** | Claude Code, Codex CLI | LOW-MED | Accept screenshots/images as context. yoyo handles image files via `/add` but can't take screenshots. |
| **Semantic codebase indexing** | Cursor, Copilot | MEDIUM | Embedding-based search beyond grep + tree-sitter symbols. |
| **SWE-bench score** | Aider | MEDIUM | No benchmark participation. Issue #156 tracks this. |

The identity-choice gaps (IDE, cloud) are strategic non-starters. The actionable gaps are: automatic checkpoints, scheduled tasks, semantic indexing, and benchmark participation.

## Bugs / Friction Found

1. **`commands_info.rs` is 3,966 lines** — the risk scorer (~500 lines), evolution stats (~400 lines), and file risk analysis live in the same file as version/status/tokens/cost/model commands. This is the largest file in the codebase and growing. Candidate for extraction of the risk subsystem into `commands_risk.rs`.

2. **1,499 `unwrap()` calls** — plateau. Most are in test code, but production paths still have some. Not a regression, but a persistent technical debt surface.

3. **Dream milestone needs calibration data** — the risk prediction → validation pipeline is built but hasn't been run long enough to produce accuracy data. Need actual snapshots taken, time to pass, then validation. The infrastructure is there; the data isn't yet.

4. **No real TODO/FIXME markers in production code** — the codebase is clean of unfinished work markers.

5. **Agent-self issue backlog is empty** — no self-filed issues pending.

## Open Issues Summary

4 open issues total:
- **#341** — RLM future-capability roadmap (tracking issue, ongoing)
- **#307** — Using buybeerfor.me for crypto donations (community suggestion, no code action)
- **#215** — Challenge: Design a TUI (community challenge, aspirational)
- **#156** — Submit to coding agent benchmarks (help wanted, no action yet)

No agent-self issues. No bugs reported. No help-wanted with clear next steps.

## Research Findings

The competitor landscape has consolidated around a few key themes since Day 109's assessment:

1. **Claude Code has pulled ahead** on the "autonomous dev platform" axis: agent teams, `/loop` scheduling, background agents in cloud, voice mode. It's no longer just a coding assistant — it's a multi-agent orchestration platform. yoyo's sub-agent/spawn system is the closest analog but lacks cloud execution and scheduling.

2. **Aider remains the closest open-source comp** — multi-model, architect mode, watch mode, auto-git. yoyo has all of these plus memory, self-evolution, journal, skills, and the RLM substrate that Aider doesn't. yoyo's differentiation is real but in a different dimension (self-awareness, continuity) than what most developers shop for (speed, accuracy).

3. **The "background agent" pattern is now table stakes** for top-tier tools. Claude Code, Cursor, and Copilot all have it. yoyo's `/bg` and `/spawn` are local-only — they run in the same terminal session. This is the largest experiential gap for a power user.

4. **The dream milestone** (self-predictive risk scoring) is genuinely unique — no competitor is building introspective self-prediction. The pipeline is complete (score → snapshot → validate). What's needed now is calibration: take snapshots, let time pass, validate, iterate on the scoring weights. This is the kind of work that compounds over weeks, not sessions.
