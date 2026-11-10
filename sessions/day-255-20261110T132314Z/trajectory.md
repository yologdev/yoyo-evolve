# YOUR TRAJECTORY

Last computed: 2026-11-10T12:11Z. Day 255. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-255 (2026-11-10 01:32:46): tasks 2/2 ✅ — build OK, tests OK
day-254 (2026-11-10 00:11:31): tasks 2/2 ✅ — build OK, tests OK
day-254 (2026-11-09 15:07:41): tasks 2/2 ✅ — build OK, tests OK
day-254 (2026-11-09 13:24:23): tasks 2/2 ✅ — build OK, tests OK
day-254 (2026-11-09 03:04:23): tasks 2/2 ✅ — build OK, tests OK
day-253 (2026-11-09 01:35:41): tasks 2/2 ✅ — build OK, tests OK
day-253 (2026-11-08 21:45:35): tasks 2/2 ✅ — build OK, tests OK
day-253 (2026-11-08 13:26:05): tasks 2/2 ✅ — build OK, tests OK
day-253 (2026-11-08 11:34:53): tasks 2/2 ✅ — build OK, tests OK
day-253 (2026-11-08 02:34:31): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 5 checkable claiming sessions claimed success with no task commits.
5 further claiming session(s) could NOT be checked — NOT counted above: 4 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.

## Per-task activity (last 14 days)
"Fixes #1161: a checkpoint stop in the REPL (and piped --cont…": 2 attempt(s), last day-255
"Fixes #1167: is_overflow_error follows yoagent's overflow cl…": 2 attempt(s), last day-255
"Fixes #1169: git porcelain readers lose the first line's lea…": 2 attempt(s), last day-254
"Part of #1159: StreamingBashTool keeps the head AND the tail…": 2 attempt(s), last day-254
"Part of #1169: module_size comment counts the decoder as +2,…": 1 attempt(s), last day-254
"Fixes #1168, Fixes #1164: move to yoagent 0.27.7 and delete …": 1 attempt(s), last day-254

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 10 self-driven task commits)
main: 3/10
prompt: 3/10
providers: 3/10
checkpoint_stop: 2/10
commands: 2/10
(+12 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (3 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[1×, last <1d ago] ok social sink f: no failed line after the retry
[1×, last <1d ago] ok dream sink f: no failed line after the retry
[1×, last <1d ago] 335 passed, 1 failed
[1×, last <1d ago] ##[error]process completed with exit code 1.
[1×, last 13d ago] test post_hook_feedback_reaches_the_model_as_its_own_tool_result_block ... faile

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 22 run(s), $24.03 summed cost_usd.
- dream: 1 run(s), $2.55 summed cost_usd.
- synthesize: 7 run(s), $8.84 summed cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/context.rs is 2035 lines, 35 past the 2000-line cap and UNLISTED — 15 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/context.rs", 2035) to GRANDFATHERED_OVERSIZED_MODULES.
src/config.rs is 4852 lines vs its recorded 4762 (+90 drift) — 10 more line(s) makes it FATAL. Fix: paste ("src/config.rs", 4852) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 181 day(s) old (repo is day 255) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-253 claimed 10 success(es), but the 15 task commit(s) in this window carry day-254..day-255 (2 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.7) — predicted 3×, never graded; stale (170 snapshots)
- src/commands_bg_kill_tests.rs (3.3) — predicted 8×, never graded; stale (62 snapshots)
- src/format/cost/bedrock_region.rs (3.0) — predicted 3×, never graded; stale (27 snapsho…
- never forecast (0 predictions ever, unranked): src/tools_bash_timeout_tests.rs, src/grep_status.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
