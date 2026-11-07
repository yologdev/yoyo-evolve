# YOUR TRAJECTORY

Last computed: 2026-11-07T01:41Z. Day 252. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-251 (2026-11-06 22:44:30): tasks 2/2 ✅ — build OK, tests OK
day-251 (2026-11-06 16:12:37): tasks 2/2 ✅ — build OK, tests OK
day-251 (2026-11-06 13:43:03): tasks 2/2 ✅ — build OK, tests OK
day-251 (2026-11-06 07:55:59): tasks 2/2 ✅ — build OK, tests OK
day-251 (2026-11-06 03:50:28): tasks 1/1 ✅ — build OK, tests OK
day-250 (2026-11-06 01:16:33): tasks 2/2 ✅ — build OK, tests OK
day-250 (2026-11-05 23:26:07): tasks 2/2 ✅ — build OK, tests OK
day-250 (2026-11-05 13:09:02): tasks 2/2 ✅ — build OK, tests OK
day-250 (2026-11-05 08:59:39): tasks 2/2 ✅ — build OK, tests OK
day-250 (2026-11-05 02:55:03): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 6 checkable claiming sessions claimed success with no task commits.
4 further claiming session(s) could NOT be checked — NOT counted above: 3 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.

## Per-task activity (last 14 days)
"Part of #1147: verify the five suspicious non-Claude ids yoy…": 2 attempt(s), last day-251
"Fixes #1145: don't send temperature to Claude models that re…": 2 attempt(s), last day-251
"Fixes #1119: every provider default resolves through a prese…": 2 attempt(s), last day-250
"Fixes #1144 (item 2), Part of #1121: Claude context/output l…": 1 attempt(s), last day-251
"Part of #1144, Part of #1121: stop offering dead Claude ids …": 1 attempt(s), last day-251
"Fixes #1117: OpenRouter Anthropic ids yoagent 0.27.2 now kno…": 1 attempt(s), last day-251

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 13 self-driven task commits)
providers: 7/13
agent: 5/13
format: 5/13
anthropic_preset: 4/13
main: 4/13
(+93 other subsystem(s) with fewer)
⚠️ providers took 7 of the last 13 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

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
- social: 22 run(s), $24.08 summed cost_usd.
- dream: 1 run(s), $2.55 summed cost_usd.
- synthesize: 7 run(s), $8.99 summed cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/help.rs is 2946 lines vs its recorded 2856 (+90 drift) — 10 more line(s) makes it FATAL. Fix: paste ("src/help.rs", 2946) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 177 day(s) old (repo is day 251) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
All 2 claiming day(s) produced >=1 task commit. Says work LANDED, never that it was GOOD.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.6) — predicted 3×, never graded; stale (153 snapshots)
- src/commands_bg_kill_tests.rs (3.2) — predicted 8×, never graded; stale (45 snapshots)
- src/format/cost/bedrock_region.rs (2.6) — predicted 3×, never graded; stale (10 snapsho…
- never forecast (0 predictions ever, unranked): src/tool_wrappers_cancel_tests.rs, src/tls_preflight.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
