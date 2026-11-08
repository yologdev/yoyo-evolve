# YOUR TRAJECTORY

Last computed: 2026-11-07T23:06Z. Day 252. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-252 (2026-11-07 21:16:45): tasks 2/2 ✅ — build OK, tests OK
day-252 (2026-11-07 17:13:34): tasks 2/2 ✅ — build OK, tests OK
day-252 (2026-11-07 14:32:33): tasks 2/2 ✅ — build OK, tests OK
day-252 (2026-11-07 12:14:18): tasks 2/2 ✅ — build OK, tests OK
day-252 (2026-11-07 03:11:33): tasks 2/2 ✅ — build OK, tests OK
day-251 (2026-11-06 22:44:30): tasks 2/2 ✅ — build OK, tests OK
day-251 (2026-11-06 16:12:37): tasks 2/2 ✅ — build OK, tests OK
day-251 (2026-11-06 13:43:03): tasks 2/2 ✅ — build OK, tests OK
day-251 (2026-11-06 07:55:59): tasks 2/2 ✅ — build OK, tests OK
day-251 (2026-11-06 03:50:28): tasks 1/1 ✅ — build OK, tests OK
Claim corroboration: 0 of 5 checkable claiming sessions claimed success with no task commits.
5 further claiming session(s) could NOT be checked — NOT counted above: 4 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.

## Per-task activity (last 14 days)
"Part of #1127: `yoyo docs`, `yoyo risk <bad>` and `yoyo memo…": 2 attempt(s), last day-252
"Fixes #1151: the -p stray-word refusal names the flag that s…": 2 attempt(s), last day-252
"Fixes #1150: `yoyo -p hi extra` refuses the stray word (exit…": 2 attempt(s), last day-252
"Part of #1147: verify the five suspicious non-Claude ids yoy…": 2 attempt(s), last day-251
"Part of #1122: stop telling users OpenRouter answers tempera…": 1 attempt(s), last day-252
"Fixes #1114: collect_repeatable_flag and resolve_numeric_set…": 1 attempt(s), last day-252

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 10 self-driven task commits)
cli: 4/10
dispatch: 4/10
main: 3/10
temperature: 3/10
agent: 2/10
(+94 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (4 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[1×, last 10d ago] test post_hook_feedback_reaches_the_model_as_its_own_tool_result_block ... faile
[1×, last 10d ago] thread 'post_hook_feedback_reaches_the_model_as_its_own_tool_result_block' (3913
[1×, last 10d ago] {"max_tokens":n,"messages":[{"content":[{"text":"run echo hi","type":"text"}],"r
[1×, last 10d ago] test result: failed. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 
[1×, last 10d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--test hook_feedback_request`

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 22 run(s), $24.09 summed cost_usd.
- dream: 1 run(s), $2.55 summed cost_usd.
- synthesize: 7 run(s), $8.17 summed cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/help.rs is 2946 lines vs its recorded 2856 (+90 drift) — 10 more line(s) makes it FATAL. Fix: paste ("src/help.rs", 2946) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 178 day(s) old (repo is day 252) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
All 2 claiming day(s) produced >=1 task commit. Says work LANDED, never that it was GOOD.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.6) — predicted 3×, never graded; stale (158 snapshots)
- src/commands_bg_kill_tests.rs (3.2) — predicted 8×, never graded; stale (50 snapshots)
- src/format/cost/bedrock_region.rs (2.8) — predicted 3×, never graded; stale (15 snapsho…
- never forecast (0 predictions ever, unranked): src/commands_ast_grep_tests.rs, src/grep_status.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
