# YOUR TRAJECTORY

Last computed: 2026-11-05T21:57Z. Day 250. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-250 (2026-11-05 13:09:02): tasks 2/2 ✅ — build OK, tests OK
day-250 (2026-11-05 08:59:39): tasks 2/2 ✅ — build OK, tests OK
day-250 (2026-11-05 02:55:03): tasks 2/2 ✅ — build OK, tests OK
day-249 (2026-11-05 00:22:30): tasks 2/2 ✅ — build OK, tests OK
day-249 (2026-11-04 22:49:31): tasks 2/2 ✅ — build OK, tests OK
day-249 (2026-11-04 13:27:00): tasks 2/2 ✅ — build OK, tests OK
day-249 (2026-11-04 11:13:34): tasks 2/2 ✅ — build OK, tests OK
day-249 (2026-11-04 02:52:00): tasks 2/2 ✅ — build OK, tests OK
day-248 (2026-11-03 23:23:52): tasks 2/2 ✅ — build OK, tests OK
day-248 (2026-11-03 14:35:56): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 5 checkable claiming sessions claimed success with no task commits.
5 further claiming session(s) could NOT be checked — NOT counted above: 4 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.

## Per-task activity (last 14 days)
"Load a subdirectory's instruction file (YOYO.md / CLAUDE.md …": 2 attempt(s), last day-250
"CLAUDE.md back under 40 KB (move MCP per-file history to ARC…": 1 attempt(s), last day-250
"Fixes #1136: one (verb, subcommand) dash-word policy table, …": 1 attempt(s), last day-250
"Fixes #1137: skill init / skill install / skill search / ris…": 1 attempt(s), last day-250
"nested_instructions: test fenced_read against a real tempdir…": 1 attempt(s), last day-250
"Fixes #1139: the REPL billing hint names a command that actu…": 1 attempt(s), last day-250

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 11 self-driven task commits)
dispatch: 3/11
main: 2/11
nested: 2/11
prompt: 2/11
agent: 1/11
(+14 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (4 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[1×, last 8d ago] test post_hook_feedback_reaches_the_model_as_its_own_tool_result_block ... faile
[1×, last 8d ago] thread 'post_hook_feedback_reaches_the_model_as_its_own_tool_result_block' (3913
[1×, last 8d ago] {"max_tokens":n,"messages":[{"content":[{"text":"run echo hi","type":"text"}],"r
[1×, last 8d ago] test result: failed. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 
[1×, last 8d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--test hook_feedback_request`

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 22 run(s), $24.45 summed cost_usd.
- dream: 1 run(s), $2.55 summed cost_usd.
- synthesize: 7 run(s), $8.38 summed cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/config.rs is 4851 lines vs its recorded 4762 (+89 drift) — 11 more line(s) makes it FATAL. Fix: paste ("src/config.rs", 4851) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 176 day(s) old (repo is day 250) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-248 claimed 4 success(es), but the 12 task commit(s) in this window carry day-249..day-250 (2 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.6) — predicted 3×, never graded; stale (146 snapshots)
- src/commands_bg_kill_tests.rs (3.1) — predicted 8×, never graded; stale (38 snapshots)
- src/prompt/network_error_summary.rs (2.0) — predicted 3×, never graded
- never forecast (0 predictions ever, unranked): src/grep_status.rs, src/openrouter_preset_tests.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
