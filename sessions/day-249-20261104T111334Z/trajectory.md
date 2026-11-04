# YOUR TRAJECTORY

Last computed: 2026-11-04T09:48Z. Day 249. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-249 (2026-11-04 02:52:00): tasks 2/2 ✅ — build OK, tests OK
day-248 (2026-11-03 23:23:52): tasks 2/2 ✅ — build OK, tests OK
day-248 (2026-11-03 14:35:56): tasks 2/2 ✅ — build OK, tests OK
day-248 (2026-11-03 13:06:46): tasks 2/2 ✅ — build OK, tests OK
day-248 (2026-11-03 11:23:15): tasks 2/2 ✅ — build OK, tests OK
day-247 (2026-11-03 00:49:11): tasks 2/2 ✅ — build OK, tests OK
day-247 (2026-11-02 13:33:07): tasks 2/2 ✅ — build OK, tests OK
day-247 (2026-11-02 11:53:30): tasks 2/2 ✅ — build OK, tests OK
day-246 (2026-11-01 21:10:45): tasks 2/2 ✅ — build OK, tests OK
day-246 (2026-11-01 12:11:36): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 4 checkable claiming sessions claimed success with no task commits.
6 further claiming session(s) could NOT be checked — NOT counted above: 5 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.
Shape, not a per-run accident: 5 of 10 claiming sessions open before the clone. Window = last 10 sessions; clone = 50 commits, oldest 2026-11-03 00:49 UTC; 4 of the 10 sessions read open their window after it. The window is this script's; the fetch depth is set in .github/workflows/evolve.yml (protected).

## Per-task activity (last 14 days)
"Fixes #1111: changelog, memories, docs, def, find, outline a…": 2 attempt(s), last day-248
"Part of #1127: `yoyo grep` / `yoyo def` / `yoyo outline` pri…": 2 attempt(s), last day-248
"Release v0.4.0: complete CHANGELOG, bump version, run the ga…": 1 attempt(s), last day-249
"Fixes #1131: ast-grep detection resolves a binary that reall…": 1 attempt(s), last day-249
"Fixes #1130: `yoyo review -- --quick` reviews the path `--qu…": 1 attempt(s), last day-248
"Part of #1064: MCP stdio servers start with provider keys un…": 1 attempt(s), last day-248

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 9 self-driven task commits)
dispatch: 4/9
verb: 3/9
agent: 2/9
format: 2/9
git: 2/9
(+19 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (4 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[1×, last 7d ago] test post_hook_feedback_reaches_the_model_as_its_own_tool_result_block ... faile
[1×, last 7d ago] thread 'post_hook_feedback_reaches_the_model_as_its_own_tool_result_block' (3913
[1×, last 7d ago] {"max_tokens":n,"messages":[{"content":[{"text":"run echo hi","type":"text"}],"r
[1×, last 7d ago] test result: failed. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 
[1×, last 7d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--test hook_feedback_request`

## Provider/API health
10 sessions, 1 provider error hit(s) in audit.jsonl, transcripts/*.log. 0 session(s) ended on a terminal give-up — every hit above was retried (survived, not died).

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 22 run(s), $24.59 summed cost_usd.
- dream: 1 run(s), $2.27 summed cost_usd.
- synthesize: 7 run(s), $11.32 summed cost_usd; $2.44 of it sub-agent spend.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/config.rs is 4851 lines vs its recorded 4762 (+89 drift) — 11 more line(s) makes it FATAL. Fix: paste ("src/config.rs", 4851) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 175 day(s) old (repo is day 249) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-246, day-247 claimed 10 success(es), but the 12 task commit(s) in this window carry day-248..day-249 (2 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.6) — predicted 3×, never graded; stale (139 snapshots)
- src/commands_bg_kill_tests.rs (3.0) — predicted 8×, never graded; stale (31 snapshots)
- src/commands_risk_unhittable.rs (1.6) — stale (132 snapshots)
- never forecast (0 predictions ever, unranked): src/tool_wrappers_cancel_tests.rs, src/vendor_preset_tests.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
