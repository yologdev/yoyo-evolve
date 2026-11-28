# YOUR TRAJECTORY

Last computed: 2026-11-28T02:12Z. Day 273. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-272 (2026-11-28 01:22:47): tasks 2/2 ✅ — build OK, tests OK
day-272 (2026-11-27 14:44:55): tasks 2/2 ✅ — build OK, tests OK
day-272 (2026-11-27 11:35:35): tasks 2/2 ✅ — build OK, tests OK
day-272 (2026-11-27 01:32:06): tasks 2/2 ✅ — build OK, tests OK
day-271 (2026-11-26 15:47:47): tasks 2/2 ✅ — build OK, tests OK
day-271 (2026-11-26 01:38:44): tasks 2/2 ✅ — build OK, tests OK
day-270 (2026-11-25 15:33:47): tasks 1/1 ✅ — build OK, tests OK
day-270 (2026-11-25 02:27:10): tasks 2/2 ✅ — build OK, tests OK
day-269 (2026-11-24 15:58:22): tasks 2/2 ✅ — build OK, tests OK
day-269 (2026-11-24 04:16:17): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 4 checkable claiming sessions claimed success with no task commits.
6 further claiming session(s) could NOT be checked — NOT counted above: 5 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.
Shape, not a per-run accident: 5 of 10 claiming sessions open before the clone. Window = last 10 sessions; clone = 52 commits, oldest 2026-11-26 15:44 UTC; 4 of the 10 sessions read open their window after it. The window is this script's; the fetch depth is set in .github/workflows/evolve.yml (protected).

## Per-task activity (last 14 days)
"Part of #1250: resolve an agent file's `tools` / `disallowed…": 2 attempt(s), last day-272
"A timed-out hook's error carries the stderr it had already p…": 2 attempt(s), last day-272
"Fixes #1247: a hook that writes more than ~64 KiB to stderr …": 2 attempt(s), last day-272
"Part of #1250: agent discovery searches subfolders, as Claud…": 1 attempt(s), last day-272
"Part of #1250: docs for the agent tool and its trust gate": 1 attempt(s), last day-272
"Part of #1250: the model can delegate to a named agent, with…": 1 attempt(s), last day-272

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 9 self-driven task commits)
agent: 5/9
agent_defs: 4/9
hooks_stderr_drain: 3/9
main: 3/9
hooks: 2/9
(+12 other subsystem(s) with fewer)
⚠️ agent took 5 of the last 9 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## CI: no failed runs in last 14 days (5 older failure(s) outside the window, not shown)

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 16 run(s), $17.86 summed cost_usd.
- dream: 1 run(s), $2.35 summed cost_usd.
- synthesize: 7 run(s), $7.77 summed cost_usd; $1.68 of it sub-agent spend.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/context.rs is 2035 lines, 35 past the 2000-line cap and UNLISTED — 15 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/context.rs", 2035) to GRANDFATHERED_OVERSIZED_MODULES.
src/format/mod.rs is 2874 lines vs its recorded 2788 (+86 drift) — 14 more line(s) makes it FATAL. Fix: paste ("src/format/mod.rs", 2874) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 198 day(s) old (repo is day 272) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-269, day-270, day-271 claimed 11 success(es), but the 13 task commit(s) in this window carry day-272 — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.7) — predicted 3×, never graded; stale (216 snapshots)
- src/commands_bg_kill_tests.rs (3.5) — predicted 8×, never graded; stale (108 snapshots)
- src/format/cost/bedrock_region.rs (3.4) — predicted 3×, never graded; stale (73 snapsho…
- never forecast (0 predictions ever, unranked): src/tools_child_bash_tests.rs, src/bash_refusal_tests.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
