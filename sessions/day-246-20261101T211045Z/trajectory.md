# YOUR TRAJECTORY

Last computed: 2026-11-01T20:15Z. Day 246. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-246 (2026-11-01 12:11:36): tasks 2/2 ✅ — build OK, tests OK
day-246 (2026-11-01 02:15:43): tasks 2/2 ✅ — build OK, tests OK
day-245 (2026-10-31 11:39:52): tasks 2/2 ✅ — build OK, tests OK
day-245 (2026-10-31 02:56:25): tasks 2/2 ✅ — build OK, tests OK
day-244 (2026-10-30 21:42:25): tasks 2/2 ✅ — build OK, tests OK
day-244 (2026-10-30 12:16:23): tasks 2/2 ✅ — build OK, tests OK
day-244 (2026-10-30 10:33:29): tasks 2/2 ✅ — build OK, tests OK
day-244 (2026-10-30 08:39:31): tasks 2/2 ✅ — build OK, tests OK
day-244 (2026-10-30 02:57:33): tasks 2/2 ✅ — build OK, tests OK
day-243 (2026-10-29 22:23:56): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 4 checkable claiming sessions claimed success with no task commits.
6 further claiming session(s) could NOT be checked — NOT counted above: 5 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.
Shape, not a per-run accident: 5 of 10 claiming sessions open before the clone. Window = last 10 sessions; clone = 50 commits, oldest 2026-10-30 21:40 UTC; 4 of the 10 sessions read open their window after it. The window is this script's; the fetch depth is set in .github/workflows/evolve.yml (protected).

## Per-task activity (last 14 days)
"Fixes #1112: the early flag scans in apply_cli_flags and par…": 2 attempt(s), last day-246
"Fixes #1107: yoyo diff / /diff refuse an unknown flag instea…": 2 attempt(s), last day-245
"Fixes #1109: an unknown top-level flag refuses with exit 2 b…": 2 attempt(s), last day-245
"Part of #1111: yoyo commit / /commit refuse an unknown dash-…": 1 attempt(s), last day-246
"Fixes #1110: a top-level '--' ends flag parsing — never repo…": 1 attempt(s), last day-246
"Content-free auto-memory entries already in the store stop r…": 1 attempt(s), last day-246

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 8 self-driven task commits)
cli: 3/8
git: 2/8
main: 2/8
dispatch: 1/8
info: 1/8
(+5 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (3 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[2×, last 4d ago] ##[error]process completed with exit code 101.
[1×, last 4d ago] test post_hook_feedback_reaches_the_model_as_its_own_tool_result_block ... faile
[1×, last 4d ago] thread 'post_hook_feedback_reaches_the_model_as_its_own_tool_result_block' (3913
[1×, last 4d ago] {"max_tokens":n,"messages":[{"content":[{"text":"run echo hi","type":"text"}],"r
[1×, last 4d ago] test result: failed. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 25 run(s), $30.72 summed cost_usd.
- dream: 1 run(s), $2.27 summed cost_usd.
- synthesize: 8 run(s), $13.05 summed cost_usd; $2.44 of it sub-agent spend; 4 usage record(s) without cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/config.rs is 4851 lines vs its recorded 4762 (+89 drift) — 11 more line(s) makes it FATAL. Fix: paste ("src/config.rs", 4851) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 172 day(s) old (repo is day 246) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-243, day-244 claimed 12 success(es), but the 12 task commit(s) in this window carry day-245..day-246 (2 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.6) — predicted 3×, never graded; stale (130 snapshots)
- src/commands_bg_kill_tests.rs (2.9) — predicted 8×, never graded; stale (22 snapshots)
- src/commands_risk_unhittable.rs (1.6) — stale (123 snapshots)
- never forecast (0 predictions ever, unranked): src/grep_status.rs, src/tools_search_flag_tests.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
