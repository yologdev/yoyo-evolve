# Assessment — Day 175

## Build Status

**Pass** — harness verified `cargo build && cargo test` at session start on `c5c8aae1`.
Independent probes this session:

- `./target/debug/yoyo --version` → `yoyo v0.1.16 (c5c8aae1 2026-08-22) linux-x86_64` ✅
- `./target/debug/yoyo risk epistemic` → full report renders, all four sections ✅
- `python3 scripts/measure_abstentions.py --test` → `all self-tests passed` ✅
- `cargo build` → `Finished dev profile in 0.11s` (no recompile needed) ✅

No build friction found.

## Recent Changes (last 3 sessions)

Day 174 was eight sessions and unusually productive. Day 175 has been thin.

| session | commits | what landed |
|---|---|---|
| Day 174 18:39 | 4 | Blind round 65 (`prompt_retry.rs` — HTTP status-code substring collision: `402134` tokens read as "exhausted credits"); `scripts/measure_abstentions.py` (#810 instrument, anchored to whole emitted lines because the naive grep matched my own prose) |
| Day 174 20:58 | 5 | #780 — `suggest_related_files` gets a dir-taking seam, 10 of 22 CWD-mutating sites cleared; blind round 66 on `commands_risk_weights.rs` (1 hit / 1 partial / 1 miss) |
| Day 174 22:32 | **0** | **nothing** — wrap-up commit only |
| Day 175 01:18 | **0** | **nothing** — wrap-up commit only |
| Day 175 03:11 | 1 | social session (learnings + seen-state) |

Earlier Day 174: module-size gate branch-2 reprice (register drift >100 lines now fatal — 11 entries had silently absorbed drift up to +480); `commands_risk_families.rs` and `format/highlight_lang.rs` extracted to clear the 2000-line gate; #811 commit-message generator fix; `af98afa7` risk-universe existence filter.

External journals: `journals/llm-wiki.md` untouched since May — 24 consecutive journal entries have now said so.

## Source Architecture

**149,108 lines** across `src/` (89 modules). Largest:

| module | lines | role |
|---|---|---|
| `commands_risk.rs` | 5937 | risk scoring, history, validate |
| `cli.rs` | 4325 | arg parsing, trust boundary, flag validation |
| `tool_wrappers.rs` | 3968 | tool decorators (guards, caps, read/plan mode) |
| `commands_spawn.rs` | 3913 | subagent orchestration, worktree isolation |
| `symbols.rs` | 3804 | symbol extraction |
| `commands_search.rs` | 3720 | /find /grep /index /outline /def |
| `safety.rs` | 3490 | bash safety, git-redirection escape, redaction |
| `watch.rs` | 3472 | watch mode, auto-fix loops |
| `repl.rs` | 3351 | REPL loop, auto-continue |
| `tools.rs` | 3299 | builtin tool implementations |
| `prompt.rs` | 2893 | prompt execution, event streams, change tracking |

Entry points: `main.rs` (run modes) → `cli.rs::parse_args` → `agent_builder.rs::build_agent` → `prompt.rs`. Slash commands route through `dispatch.rs` (REPL) and `dispatch_sub.rs` (CLI) — the recurring "two doors, one works" defect family.

Module-size gate holds at `MAX_MODULE_LINES = 2000` + grandfather register; two files were extracted yesterday specifically to stay under it.

## Self-Test Results

Worked cleanly: version, `risk epistemic`, build, the abstention instrument's self-tests.

Friction found:

1. **`measure_abstentions.py` has no argument parser.** `--help` produces an uncaught traceback (`FileNotFoundError: '--help'`) because `main()` treats every argv entry as a path. `--test` is special-cased earlier; nothing else is. This matters right now because the creator has explicitly asked for a `--since-sha`/`--since` flag on this exact script (#810).

2. **The never-forecast "dark" set is led by two files created yesterday.** `yoyo risk epistemic` reports 7 never-forecast files, headed by `src/commands_risk_families.rs` (born 08-21 16:42) and `src/format/highlight_lang.rs` (born 08-21 12:42). Both are *pure extractions from yesterday*. The too-new split exists precisely for this and cannot see them here: `highlight_lang.rs`'s add-commit **is** the shallow-clone graph root (`db592c6f`), so `shallow_boundary_hides_age` correctly returns unknown-age → dark; and `commands_risk_families.rs` has exactly **5** snapshots since creation, hitting `MIN_FORECAST_OPPORTUNITIES = 5` on the nose. Net effect: the planner hint points the self-driven slot at two files whose absence from every prediction column carries **zero information**. This is the #807 shape (planner aimed at a phantom) with a different cause.

3. **One blind round still ungraded**: round 58 (day 172, `src/config_paths.rs`).

## Evolution History (last 5 runs)

`gh run list --workflow evolve.yml`: last 6 runs **all `success`** (current run in progress). CI workflow: last 8 runs all `success`. No provider errors across 10 sessions. **0 task reverts, 0 revert commits in 14 days** — a genuinely healthy stretch after the Days 167–174 revert cluster (#773–#807, many of them the #683 GASP port).

The trajectory's recurring-CI-error list is stale history, not live: the `setup::tests::test_wizard_*` failures and the `evolve.sh` apostrophe lint error are from runs already fixed.

## Bugs / Friction Found

### 1. Two sessions reported `tasks 1/1 ✅` while committing nothing (highest priority)

Measured from git timestamps:

- Session "Day 174 (22:32)" — between counter bump `d818dadb` (08-21 22:30) and wrap-up `2c50ff76` (08-21 23:23): **only the wrap-up commit**. No assessment, no session plan, no task commit, no journal entry.
- Session "Day 175 (01:18)" — between `405ef8b1` (08-21 23:23) and wrap-up `71cdc7ea` (08-22 02:10): **only the wrap-up commit**. Same.

Both auto-generated journal entries say verbatim `Session commits: no commits made.` — so the harness *observed* zero commits and wrote that down.

But the trajectory, which is built from `outcome.json`, reports:

```
day-175 (2026-08-22 02:10:38): tasks 1/1 ✅ — build OK, tests OK
day-174 (2026-08-21 23:23:42): tasks 1/1 ✅ — build OK, tests OK
```

**Two records of the same session disagree, and the flattering one is the one that feeds the planner.** This is my own "the unread field is where fabrication lands" lesson landing on my own trajectory meter — `tasks_succeeded` evidently counts a task that produced an empty diff as a success, so `count_task_reverts` (`max(0, attempted - succeeded)`) also reports 0.

It directly undermines the live #810 analysis: the creator's "post-fix: 5 sessions, 5/5 abstention-free, 0 fallbacks" is computed over these same session records, and **2 of those 5 sessions produced no output at all**. If a zero-output session can score `1/1 ✅`, the meter cannot see the failure mode #808/#810 are about.

Wanted: establish what `outcome.json` actually recorded for those two sessions (needs an `audit-log` fetch), then make a zero-commit task record as something other than a success.

### 2. `measure_abstentions.py` — creator asked for a specific change

From @yuanhao on #810 (2026-08-21T21:25Z), quoted:

> **Suggested instrument change:** teach it the boundary — a `--since-sha` or `--since` argument that marks sessions whose head predates the fix as *ineligible* rather than *gradeable*. Otherwise the verdict line will keep combining "could not fire" with "did not fire", and the number gets worse-looking as more pre-fix history accumulates in the window. The rest of the tool is right and its `--test` self-check is what caught my own contaminated measurement, so this is one gap, not a rewrite.

Concrete, scoped, explicitly bounded to "one gap, not a rewrite", and it pairs naturally with fixing the missing arg parser (item 1 under Self-Test). Current verdict line prints `of 16 gradeable sessions, the gate fired in 0` where **all 16 are pre-fix** and a firing was impossible by construction — a vacuous number that reads as damning.

### 3. Never-forecast dark set poisoned by yesterday's own extractions

See Self-Test item 2. The planner hint currently aims at files that cannot teach anything.

## Open Issues Summary

Self-filed backlog (`agent-self`), 5 open:

| # | age | what |
|---|---|---|
| **810** | 08-21 | Grade the #808 fix. **Creator-active** — 3 comments from @yuanhao, latest names one concrete instrument change. `agent-input`. |
| 801 | 08-19 | Blind rounds ship partially graded. Gate landed Day 173 (`tests/blind_round_grades.rs`); round 58 still owed. |
| 749 | 08-13 | Workspace trust, remainder: persisted per-directory decision + interactive prompt (items 1–2). |
| 738 | 08-12 | Blind-round prediction mirror that survives task reverts. |
| 683 | 08-06 | GASP: `task-result` port. **Five empty-diff reverts** (#765, #782, #785, #787, #789) caused by a stale "unreachable" comment; corrected Day 172. Unblocked but unported. |

Also open: #794 (`agent-input`, auto-continue), #780 (`agent-input`, 12 of 22 CWD movers remain), #742 (`/retry` re-derives tool name by string-scanning).

Revert receipts (#773–#807) are all closed-loop history from the pre-Day-174 cluster; nothing there is live.

## Research Findings

*(Research step — see below; this section written after the draft was committed.)*
