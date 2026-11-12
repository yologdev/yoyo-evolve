# YOUR TRAJECTORY

Last computed: 2026-11-12T17:53Z. Day 257. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-257 (2026-11-12 13:37:21): tasks 2/2 ✅ — build OK, tests OK
day-256 (2026-11-12 00:21:40): tasks 2/2 ✅ — build OK, tests OK
day-256 (2026-11-11 13:21:42): tasks 2/2 ✅ — build OK, tests OK
day-256 (2026-11-11 03:11:44): tasks 2/2 ✅ — build OK, tests OK
day-255 (2026-11-10 23:09:46): tasks 2/2 ✅ — build OK, tests OK
day-255 (2026-11-10 13:23:14): tasks 2/2 ✅ — build OK, tests OK
day-255 (2026-11-10 01:32:46): tasks 2/2 ✅ — build OK, tests OK
day-254 (2026-11-10 00:11:31): tasks 2/2 ✅ — build OK, tests OK
day-254 (2026-11-09 15:07:41): tasks 2/2 ✅ — build OK, tests OK
day-254 (2026-11-09 13:24:23): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 5 checkable claiming sessions claimed success with no task commits.
5 further claiming session(s) could NOT be checked — NOT counted above: 4 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.

## Per-task activity (last 14 days)
"Fixes #1166: no user-visible list offers the retired `github…": 1 attempt(s), last day-257
"Fixes #1180: an inline table at a table-name key (directorie…": 1 attempt(s), last day-257
"Fixes #1178: TOML header and key slots read like tomllib (or…": 1 attempt(s), last day-256
"Fixes #1179: an untrusted project's .yoyo.toml base_url no l…": 1 attempt(s), last day-256
"`custom` / unknown OpenAI-compatible provider with no model …": 1 attempt(s), last day-256
"Config values: quote-aware trailing-comment strip shared by …": 1 attempt(s), last day-256

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 12 self-driven task commits)
config: 5/12
cli: 4/12
config_unknown: 4/12
main: 3/12
providers: 3/12
(+10 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (4 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[1×, last 2d ago] ok social sink f: no failed line after the retry
[1×, last 2d ago] ok dream sink f: no failed line after the retry
[1×, last 2d ago] 335 passed, 1 failed
[1×, last 2d ago] ##[error]process completed with exit code 1.

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 22 run(s), $24.21 summed cost_usd.
- dream: 1 run(s), $2.49 summed cost_usd.
- synthesize: 7 run(s), $7.38 summed cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/context.rs is 2035 lines, 35 past the 2000-line cap and UNLISTED — 15 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/context.rs", 2035) to GRANDFATHERED_OVERSIZED_MODULES.
src/help.rs is 2946 lines vs its recorded 2856 (+90 drift) — 10 more line(s) makes it FATAL. Fix: paste ("src/help.rs", 2946) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 183 day(s) old (repo is day 257) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-254 claimed 6 success(es), but the 12 task commit(s) in this window carry day-255..day-257 (3 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.7) — predicted 3×, never graded; stale (176 snapshots)
- src/commands_bg_kill_tests.rs (3.3) — predicted 8×, never graded; stale (68 snapshots)
- src/format/cost/bedrock_region.rs (3.0) — predicted 3×, never graded; stale (33 snapsho…
- never forecast (0 predictions ever, unranked): src/bash_output_cap.rs, src/grep_status.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
