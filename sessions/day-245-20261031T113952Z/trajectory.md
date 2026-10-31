# YOUR TRAJECTORY

Last computed: 2026-10-31T10:15Z. Day 245. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-245 (2026-10-31 02:56:25): tasks 2/2 ✅ — build OK, tests OK
day-244 (2026-10-30 21:42:25): tasks 2/2 ✅ — build OK, tests OK
day-244 (2026-10-30 12:16:23): tasks 2/2 ✅ — build OK, tests OK
day-244 (2026-10-30 10:33:29): tasks 2/2 ✅ — build OK, tests OK
day-244 (2026-10-30 08:39:31): tasks 2/2 ✅ — build OK, tests OK
day-244 (2026-10-30 02:57:33): tasks 2/2 ✅ — build OK, tests OK
day-243 (2026-10-29 22:23:56): tasks 2/2 ✅ — build OK, tests OK
day-243 (2026-10-29 12:23:26): tasks 2/2 ✅ — build OK, tests OK
day-243 (2026-10-29 02:45:29): tasks 2/2 ✅ — build OK, tests OK
day-242 (2026-10-28 21:55:22): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 4 checkable claiming sessions claimed success with no task commits.
6 further claiming session(s) could NOT be checked — NOT counted above: 5 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.
Shape, not a per-run accident: 5 of 10 claiming sessions open before the clone. Window = last 10 sessions; clone = 50 commits, oldest 2026-10-30 07:22 UTC; 4 of the 10 sessions read open their window after it. The window is this script's; the fetch depth is set in .github/workflows/evolve.yml (protected).

## Per-task activity (last 14 days)
"Fixes #1106: auto-memory keeps the newest entries instead of…": 2 attempt(s), last day-244
"A global flag before a subcommand no longer turns the subcom…": 2 attempt(s), last day-244
"Fixes #1073: --output-format json, the --cost-warn tally and…": 1 attempt(s), last day-245
"Part of #1073: --output-format json, the --cost-warn tally, …": 1 attempt(s), last day-245
"Opt-in write fence: YOYO_BLOCK_OUTSIDE_WRITES=1 refuses writ…": 1 attempt(s), last day-245
"Fixes #1105: while a fence is active, list_files withholds a…": 1 attempt(s), last day-244

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 8 self-driven task commits)
tools: 3/8
cli: 2/8
dispatch: 2/8
fence_result: 2/8
ast: 1/8
(+7 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (3 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[2×, last 3d ago] ##[error]process completed with exit code 101.
[1×, last 3d ago] test post_hook_feedback_reaches_the_model_as_its_own_tool_result_block ... faile
[1×, last 3d ago] thread 'post_hook_feedback_reaches_the_model_as_its_own_tool_result_block' (3913
[1×, last 3d ago] {"max_tokens":n,"messages":[{"content":[{"text":"run echo hi","type":"text"}],"r
[1×, last 3d ago] test result: failed. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 26 run(s), $32.75 summed cost_usd.
- dream: 1 run(s), $2.27 summed cost_usd.
- synthesize: 6 run(s), $10.39 summed cost_usd; $2.44 of it sub-agent spend; 4 usage record(s) without cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/config.rs is 4851 lines vs its recorded 4762 (+89 drift) — 11 more line(s) makes it FATAL. Fix: paste ("src/config.rs", 4851) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 171 day(s) old (repo is day 245) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-242, day-243 claimed 8 success(es), but the 13 task commit(s) in this window carry day-244..day-245 (2 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.6) — predicted 3×, never graded; stale (127 snapshots)
- src/commands_bg_kill_tests.rs (2.8) — predicted 8×, never graded; stale (19 snapshots)
- src/commands_risk_unhittable.rs (1.5) — stale (120 snapshots)
- never forecast (0 predictions ever, unranked): src/grep_status.rs, src/tools_search_flag_tests.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
