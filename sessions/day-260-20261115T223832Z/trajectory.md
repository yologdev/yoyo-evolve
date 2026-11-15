# YOUR TRAJECTORY

Last computed: 2026-11-15T20:58Z. Day 260. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-260 (2026-11-15 17:11:17): tasks 1/1 ✅ — build OK, tests OK
day-260 (2026-11-15 13:16:04): tasks 2/2 ✅ — build OK, tests OK
day-260 (2026-11-15 02:56:41): tasks 2/2 ✅ — build OK, tests OK
day-259 (2026-11-14 21:39:48): tasks 2/2 ✅ — build OK, tests OK
day-259 (2026-11-14 12:47:27): tasks 2/2 ✅ — build OK, tests OK
day-259 (2026-11-14 03:48:36): tasks 2/2 ✅ — build OK, tests OK
day-258 (2026-11-13 23:45:15): tasks 2/2 ✅ — build OK, tests OK
day-258 (2026-11-13 17:30:18): tasks 2/2 ✅ — build OK, tests OK
day-258 (2026-11-13 12:55:29): tasks 2/2 ✅ — build OK, tests OK
day-257 (2026-11-12 19:15:51): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 4 checkable claiming sessions claimed success with no task commits.
6 further claiming session(s) could NOT be checked — NOT counted above: 5 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.
Shape, not a per-run accident: 5 of 10 claiming sessions open before the clone. Window = last 10 sessions; clone = 50 commits, oldest 2026-11-14 03:48 UTC; 4 of the 10 sessions read open their window after it. The window is this script's; the fetch depth is set in .github/workflows/evolve.yml (protected).

## Per-task activity (last 14 days)
"Fixes #1201: /grep reads every word after the pattern as a p…": 2 attempt(s), last day-260
"Fixes #1198: Bedrock maxTokens has one rule on yoagent 0.27.…": 1 attempt(s), last day-260
"Fixes #1199: config show/get mask the value of every key yoy…": 1 attempt(s), last day-260
"Fixes #1173: Bedrock Claude runs with thinking on send an ex…": 1 attempt(s), last day-260
"Fixes #1197: config show/get mask every key the pre-#1194 ru…": 1 attempt(s), last day-260
"Fixes #1194: config get/show stop masking max_tokens; secret…": 1 attempt(s), last day-259

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 9 self-driven task commits)
config: 5/9
agent: 2/9
config_toplevel: 2/9
config_unknown: 2/9
providers: 2/9
(+11 other subsystem(s) with fewer)
⚠️ config took 5 of the last 9 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## Recurring CI errors (failed runs, last 14 days) (2 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[2×, last 2d ago] ##[error]process completed with exit code 101.
[1×, last 2d ago] ^[[1m^[[91merror^[[0m^[[1m: this `if` guard returns a bool literal and is follow
[1×, last 2d ago] ^[[1m^[[94m712^[[0m ^[[1m^[[94m|^[[0m ^[[1m^[[91m/^[[0m if t.contains("error ts"
[1×, last 2d ago] ^[[1m^[[94m|^[[0m ^[[1m^[[91m|_________^^[[0m ^[[1m^[[91mhelp: you can reduce it
[1×, last 2d ago] ^[[1m^[[91merror^[[0m: could not compile `yoyo-agent` (bin "yoyo") due to 3 prev

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 20 run(s), $22.74 summed cost_usd.
- dream: 1 run(s), $2.49 summed cost_usd.
- synthesize: 7 run(s), $7.51 summed cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/context.rs is 2035 lines, 35 past the 2000-line cap and UNLISTED — 15 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/context.rs", 2035) to GRANDFATHERED_OVERSIZED_MODULES.
src/format/mod.rs is 2874 lines vs its recorded 2788 (+86 drift) — 14 more line(s) makes it FATAL. Fix: paste ("src/format/mod.rs", 2874) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 186 day(s) old (repo is day 260) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-257, day-258 claimed 8 success(es), but the 10 task commit(s) in this window carry day-259..day-260 (2 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.7) — predicted 3×, never graded; stale (186 snapshots)
- src/commands_bg_kill_tests.rs (3.4) — predicted 8×, never graded; stale (78 snapshots)
- src/format/cost/bedrock_region.rs (3.1) — predicted 3×, never graded; stale (43 snapsho…
- never forecast (0 predictions ever, unranked): src/grep_status.rs, src/safety_git_glued_tests.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
