# YOUR TRAJECTORY

Last computed: 2026-11-09T12:08Z. Day 254. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-254 (2026-11-09 03:04:23): tasks 2/2 ✅ — build OK, tests OK
day-253 (2026-11-09 01:35:41): tasks 2/2 ✅ — build OK, tests OK
day-253 (2026-11-08 21:45:35): tasks 2/2 ✅ — build OK, tests OK
day-253 (2026-11-08 13:26:05): tasks 2/2 ✅ — build OK, tests OK
day-253 (2026-11-08 11:34:53): tasks 2/2 ✅ — build OK, tests OK
day-253 (2026-11-08 02:34:31): tasks 2/2 ✅ — build OK, tests OK
day-252 (2026-11-08 00:18:31): tasks 2/2 ✅ — build OK, tests OK
day-252 (2026-11-07 21:16:45): tasks 2/2 ✅ — build OK, tests OK
day-252 (2026-11-07 17:13:34): tasks 2/2 ✅ — build OK, tests OK
day-252 (2026-11-07 14:32:33): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 5 checkable claiming sessions claimed success with no task commits.
5 further claiming session(s) could NOT be checked — NOT counted above: 4 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.

## Per-task activity (last 14 days)
"Part of #1159: StreamingBashTool keeps the head AND the tail…": 2 attempt(s), last day-254
"Fixes #1159: pin every consumer of the checkpoint stop on yo…": 1 attempt(s), last day-254
"Fixes #1158: measure what goes on the wire for google with n…": 1 attempt(s), last day-253
"User deny rules vs quoted/escaped command words: probe the f…": 1 attempt(s), last day-253
"`yoyo config <unknown>` refuses as an unknown subcommand (wi…": 1 attempt(s), last day-253
"Fixes #1157: offered Gemini ids without a yoagent preset run…": 1 attempt(s), last day-253

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 10 self-driven task commits)
config: 3/10
tools: 3/10
dispatch: 2/10
info: 2/10
main: 2/10
(+14 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (4 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[1×, last 12d ago] test post_hook_feedback_reaches_the_model_as_its_own_tool_result_block ... faile
[1×, last 12d ago] thread 'post_hook_feedback_reaches_the_model_as_its_own_tool_result_block' (3913
[1×, last 12d ago] {"max_tokens":n,"messages":[{"content":[{"text":"run echo hi","type":"text"}],"r
[1×, last 12d ago] test result: failed. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 
[1×, last 12d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--test hook_feedback_request`

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 22 run(s), $23.96 summed cost_usd.
- dream: 1 run(s), $2.55 summed cost_usd.
- synthesize: 7 run(s), $8.16 summed cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/context.rs is 2035 lines, 35 past the 2000-line cap and UNLISTED — 15 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/context.rs", 2035) to GRANDFATHERED_OVERSIZED_MODULES.
src/config.rs is 4852 lines vs its recorded 4762 (+90 drift) — 10 more line(s) makes it FATAL. Fix: paste ("src/config.rs", 4852) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 180 day(s) old (repo is day 254) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-252 claimed 8 success(es), but the 11 task commit(s) in this window carry day-253..day-254 (2 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.6) — predicted 3×, never graded; stale (166 snapshots)
- src/commands_bg_kill_tests.rs (3.3) — predicted 8×, never graded; stale (58 snapshots)
- src/format/cost/bedrock_region.rs (2.9) — predicted 3×, never graded; stale (23 snapsho…
- never forecast (0 predictions ever, unranked): src/tools_bash_timeout_tests.rs, src/tools_child_bash_tests.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
