# YOUR TRAJECTORY

Last computed: 2026-10-30T10:51Z. Day 244. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-244 (2026-10-30 10:33:29): tasks 2/2 ✅ — build OK, tests OK
day-244 (2026-10-30 08:39:31): tasks 2/2 ✅ — build OK, tests OK
day-244 (2026-10-30 02:57:33): tasks 2/2 ✅ — build OK, tests OK
day-243 (2026-10-29 22:23:56): tasks 2/2 ✅ — build OK, tests OK
day-243 (2026-10-29 12:23:26): tasks 2/2 ✅ — build OK, tests OK
day-243 (2026-10-29 02:45:29): tasks 2/2 ✅ — build OK, tests OK
day-242 (2026-10-28 21:55:22): tasks 2/2 ✅ — build OK, tests OK
day-242 (2026-10-28 18:12:25): tasks 2/2 ✅ — build OK, tests OK
day-242 (2026-10-28 16:03:55): tasks 2/2 ✅ — build OK, tests OK
day-242 (2026-10-28 13:06:05): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 5 checkable claiming sessions claimed success with no task commits.
5 further claiming session(s) could NOT be checked — NOT counted above: 4 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.

## Per-task activity (last 14 days)
"A global flag before a subcommand no longer turns the subcom…": 2 attempt(s), last day-244
"Fixes #1103: refuse a mistyped --provider at startup (exit 2…": 2 attempt(s), last day-244
"Fixes #1101: hard_deny tracks quote state across lines, so a…": 2 attempt(s), last day-243
"Fixes #1104: the trajectory cap fits the whole block, and an…": 1 attempt(s), last day-244
"Fixes #1071: count synthesize in the trajectory's phase spen…": 1 attempt(s), last day-244
"Shell `yoyo ast` exits nonzero when ast-grep is missing or t…": 1 attempt(s), last day-244

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 8 self-driven task commits)
dispatch: 4/8
cli: 2/8
goal: 2/8
hard: 2/8
agent: 1/8
(+7 other subsystem(s) with fewer)
⚠️ dispatch took 4 of the last 8 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## Recurring CI errors (failed runs, last 14 days) (3 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[2×, last 2d ago] ##[error]process completed with exit code 101.
[1×, last 2d ago] test post_hook_feedback_reaches_the_model_as_its_own_tool_result_block ... faile
[1×, last 2d ago] thread 'post_hook_feedback_reaches_the_model_as_its_own_tool_result_block' (3913
[1×, last 2d ago] {"max_tokens":n,"messages":[{"content":[{"text":"run echo hi","type":"text"}],"r
[1×, last 2d ago] test result: failed. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 27 run(s), $34.66 summed cost_usd.
- dream: 1 run(s), $2.27 summed cost_usd.
- synthesize: 5 run(s), $7.27 summed cost_usd; 4 usage record(s) without cost_usd.
Totals are a floor: these sums omit sub-agent spend (#1073).

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/cli.rs is 7869 lines vs its recorded 7774 (+95 drift) — 5 more line(s) makes it FATAL. Fix: paste ("src/cli.rs", 7869) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 170 day(s) old (repo is day 244) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-242 claimed 8 success(es), but the 13 task commit(s) in this window carry day-243..day-244 (2 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.6) — predicted 3×, never graded; stale (124 snapshots)
- src/commands_bg_kill_tests.rs (2.8) — predicted 8×, never graded; stale (16 snapshots)
- src/commands_risk_unhittable.rs (1.5) — stale (117 snapshots)
- never forecast (0 predictions ever, unranked): src/grep_status.rs, src/tools_search_flag_tests.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
