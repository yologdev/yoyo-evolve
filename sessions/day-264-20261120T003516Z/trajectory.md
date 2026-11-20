# YOUR TRAJECTORY

Last computed: 2026-11-19T23:06Z. Day 264. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-264 (2026-11-19 13:58:16): tasks 2/2 ✅ — build OK, tests OK
day-263 (2026-11-19 00:30:49): tasks 2/2 ✅ — build OK, tests OK
day-263 (2026-11-18 14:41:52): tasks 2/2 ✅ — build OK, tests OK
day-262 (2026-11-18 01:30:39): tasks 2/2 ✅ — build OK, tests OK
day-262 (2026-11-17 14:12:17): tasks 2/2 ✅ — build OK, tests OK
day-262 (2026-11-17 02:20:26): tasks 2/2 ✅ — build OK, tests OK
day-261 (2026-11-16 13:58:33): tasks 2/2 ✅ — build OK, tests OK
day-261 (2026-11-16 02:08:13): tasks 0/1 ⚠️ — 1 task(s) did not reach a verdict (tree green, no revert recorded)
day-260 (2026-11-16 00:39:14): tasks 2/2 ✅ — build OK, tests OK
day-260 (2026-11-15 22:38:32): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 4 checkable claiming sessions claimed success with no task commits.
5 further claiming session(s) could NOT be checked — NOT counted above: 4 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.

## Per-task activity (last 14 days)
"Part of #1216: an unparseable .yoyo/memory.json is never ove…": 2 attempt(s), last day-264
"Part of #1211: /save refuses to replace an existing file it …": 2 attempt(s), last day-263
"Release v0.5.0: CHANGELOG for v0.4.0..HEAD, version bump, ga…": 1 attempt(s), last day-264
"Fixes #1211: /spawn -o refuses an existing output file befor…": 1 attempt(s), last day-263
"Part of #1211: /spawn -o refuses an existing output file bef…": 1 attempt(s), last day-263
"Fixes #1209: a literal bash -c / sh -c body is checked as th…": 1 attempt(s), last day-263

## Reverts in window
0 task reverts in last ~10 sessions. 1 task(s) did not reach a verdict (tree green, no revert recorded) across 1 of the last ~10 sessions.
0 whole-session revert commit(s) in last 14 days.

## Subsystem concentration (last 8 self-driven task commits)
help: 4/8
session: 4/8
bash: 3/8
safety: 3/8
safety_quoted_subst: 3/8
(+109 other subsystem(s) with fewer)
⚠️ help took 4 of the last 8 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## Recurring CI errors (failed runs, last 14 days) (2 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[2×, last 6d ago] ##[error]process completed with exit code 101.
[1×, last 6d ago] ^[[1m^[[91merror^[[0m^[[1m: this `if` guard returns a bool literal and is follow
[1×, last 6d ago] ^[[1m^[[94m712^[[0m ^[[1m^[[94m|^[[0m ^[[1m^[[91m/^[[0m if t.contains("error ts"
[1×, last 6d ago] ^[[1m^[[94m|^[[0m ^[[1m^[[91m|_________^^[[0m ^[[1m^[[91mhelp: you can reduce it
[1×, last 6d ago] ^[[1m^[[91merror^[[0m: could not compile `yoyo-agent` (bin "yoyo") due to 3 prev

## Provider/API health
10 sessions, 1 provider error hit(s) in audit.jsonl, transcripts/*.log. 0 session(s) ended on a terminal give-up — every hit above was retried (survived, not died).

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 18 run(s), $20.63 summed cost_usd.
- dream: 1 run(s), $2.54 summed cost_usd.
- synthesize: 7 run(s), $7.96 summed cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/context.rs is 2035 lines, 35 past the 2000-line cap and UNLISTED — 15 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/context.rs", 2035) to GRANDFATHERED_OVERSIZED_MODULES.
src/format/mod.rs is 2874 lines vs its recorded 2788 (+86 drift) — 14 more line(s) makes it FATAL. Fix: paste ("src/format/mod.rs", 2874) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 190 day(s) old (repo is day 264) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-260, day-261 claimed 6 success(es), but the 12 task commit(s) in this window carry day-262..day-264 (3 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.7) — predicted 3×, never graded; stale (196 snapshots)
- src/commands_bg_kill_tests.rs (3.4) — predicted 8×, never graded; stale (88 snapshots)
- src/format/cost/bedrock_region.rs (3.2) — predicted 3×, never graded; stale (53 snapsho…
- never forecast (0 predictions ever, unranked): src/tools_child_bash_tests.rs, src/grep_status.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
