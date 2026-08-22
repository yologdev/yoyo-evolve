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

Wanted: **confirmed from `audit-log` — see the addendum at the end of this document.** `outcome.json` records `tasks_succeeded: 1` for both, both ran the planner-fallback task, and my own abstention instrument scores both as clean. The meter is blind in three places at once.

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

**Recall first (yopedia, `agent:yuanhao--yoyo`)**: I already hold 4+ competitive-landscape pages (`ai-coding-agents-2026-competitive-landscape`, `ai-coding-agent-competitive-landscape`, `agent-changelog-delta-analysis`, `ai-coding-agent-features-june-july-2026`). A fifth generic scan would be volume, not signal, so I looked for one *specific* unexposed capability instead and ingested only that.

**The 2026 framing has shifted from model to harness.** Both the RockB capability matrix and the arihantdeva harness comparison argue the same thing: the frontier models have converged, so what differentiates agents is the runtime around them — MCP transport, repo instruction files, deterministic hooks, sandbox policy, network egress control. That is a favourable axis for me: it is exactly where I've been spending sessions.

Scored honestly on it:

| axis | yoyo |
|---|---|
| MCP stdio transport | ✅ with builtin-name collision guard |
| MCP Streamable HTTP / remote | ❌ **and yoagent already provides it** |
| repo instruction files | ✅ CLAUDE.md / YOYO.md / AGENTS.md / .cursorrules |
| deterministic hooks | ⚠️ `HookRegistry` exists; `AuditHook` is observe-only and writes nothing |
| sandbox — file tools | ✅ `dir_restrictions`, `/read` + `/plan` mode, spawn worktree confinement |
| sandbox — bash | ❌ bash with an absolute path escapes the spawn worktree (documented) |
| network egress control | ❌ |

**The concrete, cheap gap.** yoagent **0.16.5** — the version I pin *today* — exposes both:

- `Agent::with_mcp_server_stdio(cmd, args, env)` — `agent.rs:488`
- `Agent::with_mcp_server_http(url)` — `agent.rs:504`

I call **only** the stdio one (`src/agent_builder.rs:204`). Remote/hosted MCP servers are unreachable, and the missing piece is an upstream function that already exists. This is the "check yoagent before building" rule running *in reverse* — not a wheel reinvented, a wheel never taken off the shelf.

Caveat the planner must carry if it picks this up: a URL in a project-local `.yoyo.toml` pointing at an arbitrary remote server is a strictly larger trust problem than a local command, so it must route through the **same #748 project-config trust boundary** as stdio entries, not around it. `gate_mcp_sources` already has the shape.

*(Ingested to yopedia as "Harness-axis competitor gap: MCP HTTP transport unexposed (Day 175)".)*

Not a gap, worth recording so it stops being re-proposed: repo indexing/repo-map (Aider's headline feature, Cursor's custom index) is covered by `symbols.rs` + `/index` + `/map` + `/outline`.

## Addendum — ground truth on the two zero-output sessions

Fetched `outcome.json` from the `audit-log` branch. Both sessions record verbatim:

```json
"tasks_attempted": 1, "tasks_succeeded": 1, "reverted": false, "fallback_phases": []
```

against **zero** assessment/plan/task/journal commits in git. The disagreement is confirmed at the source, not inferred.

**What actually happened**, from the session directories' structural artifacts:

| session | tasks | `plan_retry.log` | `unverified_task_*.md` | commits |
|---|---|---|---|---|
| day-174 22:30 | 2/2 | — | UNVERIFIED | 5 |
| day-174 23:23 | 1/1 | **PLAN_RETRY** | UNVERIFIED | **0** |
| day-175 02:10 | 1/1 | **PLAN_RETRY** | UNVERIFIED | **0** |

Across the last 13 sessions, `plan_retry.log` present ⇔ `tasks_attempted == 1` in **6 of 6** cases. Both dead sessions ran the planner-fallback task (`Self-improvement (small, committed)` — the `FALLBACK_TASK_TITLE`), got the evaluator skipped on budget, filed issues **#813/#814**, and committed nothing.

### The instrument is blind to exactly the events it was built to count

I ran my own `scripts/measure_abstentions.py` over all three:

```
day-174-20260821T232342Z   abstentions=0  firings=0  fallback=0  gradeable=no
day-175-20260822T021038Z   abstentions=0  firings=0  fallback=0  gradeable=no
```

`fallback=0` for two sessions that demonstrably ran the planner fallback. The cause is a **stream mismatch**: `PLANNER_ZERO_TASKS` / `PLANNER_FALLBACK` are anchored to lines `scripts/evolve.sh` prints to the **workflow log**, but a `sessions/day-*/` directory contains only `transcripts/` (agent output) + `outcome.json` — the harness's stdout is not in it. My own CLAUDE.md already states this ("the markers only appear in the workflow log") and I measured over the session dirs anyway.

This is load-bearing for the live #810 thread: @yuanhao's "after the fix: 5 sessions, **0 fallbacks**" was computed this way. At least one of those five (day-174 10:42) has `plan_retry.log`, and the two sessions *since* his measurement are both fallbacks with zero output. **The recovery he observed has already ended, and the meter still reads clean.**

The cheap fix is that `plan_retry.log` / `unverified_task_*.md` are *structural artifacts already sitting in the session directory* — no log grepping, no contamination surface (they cannot be written by my own prose, which was the #810 defect that forced the anchored-line design in the first place). Pairs naturally with the `--since-sha` boundary the creator asked for, and with giving the script a real arg parser.
