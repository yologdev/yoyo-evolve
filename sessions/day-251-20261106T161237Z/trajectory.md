# YOUR TRAJECTORY

Last computed: 2026-11-06T14:57Z. Day 251. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-251 (2026-11-06 13:43:03): tasks 2/2 ✅ — build OK, tests OK
day-251 (2026-11-06 07:55:59): tasks 2/2 ✅ — build OK, tests OK
day-251 (2026-11-06 03:50:28): tasks 1/1 ✅ — build OK, tests OK
day-250 (2026-11-06 01:16:33): tasks 2/2 ✅ — build OK, tests OK
day-250 (2026-11-05 23:26:07): tasks 2/2 ✅ — build OK, tests OK
day-250 (2026-11-05 13:09:02): tasks 2/2 ✅ — build OK, tests OK
day-250 (2026-11-05 08:59:39): tasks 2/2 ✅ — build OK, tests OK
day-250 (2026-11-05 02:55:03): tasks 2/2 ✅ — build OK, tests OK
day-249 (2026-11-05 00:22:30): tasks 2/2 ✅ — build OK, tests OK
day-249 (2026-11-04 22:49:31): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 6 checkable claiming sessions claimed success with no task commits.
4 further claiming session(s) could NOT be checked — NOT counted above: 3 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.

## Per-task activity (last 14 days)
"Fixes #1119: every provider default resolves through a prese…": 2 attempt(s), last day-250
"Fixes #1117: OpenRouter Anthropic ids yoagent 0.27.2 now kno…": 1 attempt(s), last day-251
"Part of #1121, Part of #1122: price Bedrock Claude ids throu…": 1 attempt(s), last day-251
"Fixes #1143: stop accepting GIF in /add, @path and --image, …": 1 attempt(s), last day-251
"Fixes #1026: json result object and stream-json say when the…": 1 attempt(s), last day-251
"Part of #1012: /cost prices through yoagent's PriceTable fir…": 1 attempt(s), last day-251

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 12 self-driven task commits)
prompt: 5/12
providers: 5/12
agent: 4/12
format: 4/12
main: 4/12
(+92 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (4 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[1×, last 9d ago] test post_hook_feedback_reaches_the_model_as_its_own_tool_result_block ... faile
[1×, last 9d ago] thread 'post_hook_feedback_reaches_the_model_as_its_own_tool_result_block' (3913
[1×, last 9d ago] {"max_tokens":n,"messages":[{"content":[{"text":"run echo hi","type":"text"}],"r
[1×, last 9d ago] test result: failed. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 
[1×, last 9d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--test hook_feedback_request`

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 22 run(s), $24.38 summed cost_usd.
- dream: 1 run(s), $2.55 summed cost_usd.
- synthesize: 7 run(s), $8.38 summed cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/help.rs is 2946 lines vs its recorded 2856 (+90 drift) — 10 more line(s) makes it FATAL. Fix: paste ("src/help.rs", 2946) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 177 day(s) old (repo is day 251) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-249 claimed 4 success(es), but the 13 task commit(s) in this window carry day-250..day-251 (2 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.6) — predicted 3×, never graded; stale (151 snapshots)
- src/commands_bg_kill_tests.rs (3.1) — predicted 8×, never graded; stale (43 snapshots)
- src/format/cost/bedrock_region.rs (2.6) — predicted 3×, never graded; stale (8 snapshot…
- never forecast (0 predictions ever, unranked): src/tool_wrappers_cancel_tests.rs, src/tls_preflight.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
