# YOUR TRAJECTORY

Last computed: 2026-11-13T22:00Z. Day 258. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-258 (2026-11-13 17:30:18): tasks 2/2 ✅ — build OK, tests OK
day-258 (2026-11-13 12:55:29): tasks 2/2 ✅ — build OK, tests OK
day-257 (2026-11-12 19:15:51): tasks 2/2 ✅ — build OK, tests OK
day-257 (2026-11-12 13:37:21): tasks 2/2 ✅ — build OK, tests OK
day-256 (2026-11-12 00:21:40): tasks 2/2 ✅ — build OK, tests OK
day-256 (2026-11-11 13:21:42): tasks 2/2 ✅ — build OK, tests OK
day-256 (2026-11-11 03:11:44): tasks 2/2 ✅ — build OK, tests OK
day-255 (2026-11-10 23:09:46): tasks 2/2 ✅ — build OK, tests OK
day-255 (2026-11-10 13:23:14): tasks 2/2 ✅ — build OK, tests OK
day-255 (2026-11-10 01:32:46): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 4 checkable claiming sessions claimed success with no task commits.
6 further claiming session(s) could NOT be checked — NOT counted above: 5 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.
Shape, not a per-run accident: 5 of 10 claiming sessions open before the clone. Window = last 10 sessions; clone = 50 commits, oldest 2026-11-11 16:45 UTC; 4 of the 10 sessions read open their window after it. The window is this script's; the fetch depth is set in .github/workflows/evolve.yml (protected).

## Per-task activity (last 14 days)
"Fixes #1185: compare config rungs by file identity, so runni…": 2 attempt(s), last day-258
"Fixes #1189: keep $(...) and backticks inside double quotes …": 2 attempt(s), last day-258
"Part of #1184: parse every loaded config file with the `toml…": 2 attempt(s), last day-257
"Fixes #1181: recording-stub test that a cross-origin 307/302…": 1 attempt(s), last day-258
"Part of #1187: a unit test that reaches a dispatcher exit fa…": 1 attempt(s), last day-258
"Fixes #1183: REPL /provider applies #1175's named-model refu…": 1 attempt(s), last day-257

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 9 self-driven task commits)
config: 5/9
cli: 3/9
commands: 2/9
config_unknown: 2/9
dispatch: 2/9
(+10 other subsystem(s) with fewer)
⚠️ config took 5 of the last 9 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## Recurring CI errors (failed runs, last 14 days) (2 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[2×, last <1d ago] ##[error]process completed with exit code 101.
[1×, last <1d ago] ^[[1m^[[91merror^[[0m^[[1m: this `if` guard returns a bool literal and is follow
[1×, last <1d ago] ^[[1m^[[94m712^[[0m ^[[1m^[[94m|^[[0m ^[[1m^[[91m/^[[0m if t.contains("error ts"
[1×, last <1d ago] ^[[1m^[[94m|^[[0m ^[[1m^[[91m|_________^^[[0m ^[[1m^[[91mhelp: you can reduce it
[1×, last <1d ago] ^[[1m^[[91merror^[[0m: could not compile `yoyo-agent` (bin "yoyo") due to 3 prev

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 21 run(s), $23.21 summed cost_usd.
- dream: 1 run(s), $2.49 summed cost_usd.
- synthesize: 7 run(s), $7.09 summed cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/context.rs is 2035 lines, 35 past the 2000-line cap and UNLISTED — 15 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/context.rs", 2035) to GRANDFATHERED_OVERSIZED_MODULES.
src/help.rs is 2946 lines vs its recorded 2856 (+90 drift) — 10 more line(s) makes it FATAL. Fix: paste ("src/help.rs", 2946) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 184 day(s) old (repo is day 258) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-255 claimed 6 success(es), but the 13 task commit(s) in this window carry day-256..day-258 (3 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.7) — predicted 3×, never graded; stale (179 snapshots)
- src/commands_bg_kill_tests.rs (3.4) — predicted 8×, never graded; stale (71 snapshots)
- src/format/cost/bedrock_region.rs (3.1) — predicted 3×, never graded; stale (36 snapsho…
- never forecast (0 predictions ever, unranked): src/config_syntax.rs, src/grep_status.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
