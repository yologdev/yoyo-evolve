# YOUR TRAJECTORY

Last computed: 2026-11-13T11:36Z. Day 258. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-257 (2026-11-12 19:15:51): tasks 2/2 ✅ — build OK, tests OK
day-257 (2026-11-12 13:37:21): tasks 2/2 ✅ — build OK, tests OK
day-256 (2026-11-12 00:21:40): tasks 2/2 ✅ — build OK, tests OK
day-256 (2026-11-11 13:21:42): tasks 2/2 ✅ — build OK, tests OK
day-256 (2026-11-11 03:11:44): tasks 2/2 ✅ — build OK, tests OK
day-255 (2026-11-10 23:09:46): tasks 2/2 ✅ — build OK, tests OK
day-255 (2026-11-10 13:23:14): tasks 2/2 ✅ — build OK, tests OK
day-255 (2026-11-10 01:32:46): tasks 2/2 ✅ — build OK, tests OK
day-254 (2026-11-10 00:11:31): tasks 2/2 ✅ — build OK, tests OK
day-254 (2026-11-09 15:07:41): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 5 checkable claiming sessions claimed success with no task commits.
5 further claiming session(s) could NOT be checked — NOT counted above: 4 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.

## Per-task activity (last 14 days)
"Part of #1184: parse every loaded config file with the `toml…": 2 attempt(s), last day-257
"Fixes #1183: REPL /provider applies #1175's named-model refu…": 1 attempt(s), last day-257
"Fixes #1166: no user-visible list offers the retired `github…": 1 attempt(s), last day-257
"Fixes #1180: an inline table at a table-name key (directorie…": 1 attempt(s), last day-257
"Fixes #1178: TOML header and key slots read like tomllib (or…": 1 attempt(s), last day-256
"Fixes #1179: an untrusted project's .yoyo.toml base_url no l…": 1 attempt(s), last day-256

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 11 self-driven task commits)
config: 6/11
cli: 4/11
config_unknown: 4/11
commands: 3/11
main: 3/11
(+102 other subsystem(s) with fewer)
⚠️ config took 6 of the last 11 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

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
- social: 21 run(s), $22.95 summed cost_usd.
- dream: 1 run(s), $2.49 summed cost_usd.
- synthesize: 7 run(s), $7.54 summed cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/context.rs is 2035 lines, 35 past the 2000-line cap and UNLISTED — 15 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/context.rs", 2035) to GRANDFATHERED_OVERSIZED_MODULES.
src/help.rs is 2946 lines vs its recorded 2856 (+90 drift) — 10 more line(s) makes it FATAL. Fix: paste ("src/help.rs", 2946) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 183 day(s) old (repo is day 257) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-254 claimed 4 success(es), but the 12 task commit(s) in this window carry day-255..day-257 (3 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.7) — predicted 3×, never graded; stale (177 snapshots)
- src/commands_bg_kill_tests.rs (3.3) — predicted 8×, never graded; stale (69 snapshots)
- src/format/cost/bedrock_region.rs (3.1) — predicted 3×, never graded; stale (34 snapsho…
- never forecast (0 predictions ever, unranked): src/grep_status.rs, src/format/cost/yoagent_price.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
