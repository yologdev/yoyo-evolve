# YOUR TRAJECTORY

Last computed: 2026-11-22T02:01Z. Day 267. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-266 (2026-11-21 23:31:46): tasks 2/2 ✅ — build OK, tests OK
day-266 (2026-11-21 14:00:57): tasks 2/2 ✅ — build OK, tests OK
day-265 (2026-11-21 00:25:31): tasks 2/2 ✅ — build OK, tests OK
day-265 (2026-11-20 14:03:52): tasks 2/2 ✅ — build OK, tests OK
day-264 (2026-11-20 00:35:16): tasks 2/2 ✅ — build OK, tests OK
day-264 (2026-11-19 13:58:16): tasks 2/2 ✅ — build OK, tests OK
day-263 (2026-11-19 00:30:49): tasks 2/2 ✅ — build OK, tests OK
day-263 (2026-11-18 14:41:52): tasks 2/2 ✅ — build OK, tests OK
day-262 (2026-11-18 01:30:39): tasks 2/2 ✅ — build OK, tests OK
day-262 (2026-11-17 14:12:17): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 5 checkable claiming sessions claimed success with no task commits.
5 further claiming session(s) could NOT be checked — NOT counted above: 4 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.

## Per-task activity (last 14 days)
"Fixes #1213: /spawn -o honours a quoted path, so "out/a b.md…": 2 attempt(s), last day-266
"Fixes #1224: /spawn --parallel -o <file> is refused before a…": 1 attempt(s), last day-266
"Fixes #1223: hard_deny stops hard-blocking a quoted mention …": 1 attempt(s), last day-266
"Fixes #1210: a dangerous command only mentioned in a git com…": 1 attempt(s), last day-266
"Fixes #1219: --output-format stream-json honours --output (w…": 1 attempt(s), last day-265
"Fixes #1220: an interrupted -p run (exit 130/143) leaves the…": 1 attempt(s), last day-265

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 11 self-driven task commits)
main: 5/11
prompt: 5/11
config: 3/11
config_toml: 3/11
spawn: 3/11
(+109 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (2 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[2×, last 8d ago] ##[error]process completed with exit code 101.
[1×, last 8d ago] ^[[1m^[[91merror^[[0m^[[1m: this `if` guard returns a bool literal and is follow
[1×, last 8d ago] ^[[1m^[[94m712^[[0m ^[[1m^[[94m|^[[0m ^[[1m^[[91m/^[[0m if t.contains("error ts"
[1×, last 8d ago] ^[[1m^[[94m|^[[0m ^[[1m^[[91m|_________^^[[0m ^[[1m^[[91mhelp: you can reduce it
[1×, last 8d ago] ^[[1m^[[91merror^[[0m: could not compile `yoyo-agent` (bin "yoyo") due to 3 prev

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 18 run(s), $21.11 summed cost_usd.
- dream: 1 run(s), $2.54 summed cost_usd.
- synthesize: 7 run(s), $7.08 summed cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/context.rs is 2035 lines, 35 past the 2000-line cap and UNLISTED — 15 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/context.rs", 2035) to GRANDFATHERED_OVERSIZED_MODULES.
src/format/mod.rs is 2874 lines vs its recorded 2788 (+86 drift) — 14 more line(s) makes it FATAL. Fix: paste ("src/format/mod.rs", 2874) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 192 day(s) old (repo is day 266) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-262, day-263 claimed 8 success(es), but the 13 task commit(s) in this window carry day-264..day-266 (3 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.7) — predicted 3×, never graded; stale (201 snapshots)
- src/commands_bg_kill_tests.rs (3.5) — predicted 8×, never graded; stale (93 snapshots)
- src/format/cost/bedrock_region.rs (3.3) — predicted 3×, never graded; stale (58 snapsho…
- never forecast (0 predictions ever, unranked): src/grep_status.rs, src/config_toplevel_tests.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
