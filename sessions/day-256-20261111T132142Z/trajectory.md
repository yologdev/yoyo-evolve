# YOUR TRAJECTORY

Last computed: 2026-11-11T11:47Z. Day 256. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-256 (2026-11-11 03:11:44): tasks 2/2 ✅ — build OK, tests OK
day-255 (2026-11-10 23:09:46): tasks 2/2 ✅ — build OK, tests OK
day-255 (2026-11-10 13:23:14): tasks 2/2 ✅ — build OK, tests OK
day-255 (2026-11-10 01:32:46): tasks 2/2 ✅ — build OK, tests OK
day-254 (2026-11-10 00:11:31): tasks 2/2 ✅ — build OK, tests OK
day-254 (2026-11-09 15:07:41): tasks 2/2 ✅ — build OK, tests OK
day-254 (2026-11-09 13:24:23): tasks 2/2 ✅ — build OK, tests OK
day-254 (2026-11-09 03:04:23): tasks 2/2 ✅ — build OK, tests OK
day-253 (2026-11-09 01:35:41): tasks 2/2 ✅ — build OK, tests OK
day-253 (2026-11-08 21:45:35): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 5 checkable claiming sessions claimed success with no task commits.
5 further claiming session(s) could NOT be checked — NOT counted above: 4 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.

## Per-task activity (last 14 days)
"Fixes #1161: a checkpoint stop in the REPL (and piped --cont…": 2 attempt(s), last day-255
"Fixes #1167: is_overflow_error follows yoagent's overflow cl…": 2 attempt(s), last day-255
"Fixes #1169: git porcelain readers lose the first line's lea…": 2 attempt(s), last day-254
"Fixes #1176: the "treating unknown provider X as OpenAI-comp…": 1 attempt(s), last day-256
"Fixes #1174: a section header with a trailing comment ([dire…": 1 attempt(s), last day-256
"Fixes #1160: bedrock guessed-limits note stops claiming "cap…": 1 attempt(s), last day-255

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 12 self-driven task commits)
prompt: 4/12
agent: 3/12
config: 3/12
main: 3/12
model_limits: 3/12
(+99 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (4 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[1×, last 1d ago] ok social sink f: no failed line after the retry
[1×, last 1d ago] ok dream sink f: no failed line after the retry
[1×, last 1d ago] 335 passed, 1 failed
[1×, last 1d ago] ##[error]process completed with exit code 1.

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 22 run(s), $24.02 summed cost_usd.
- dream: 1 run(s), $2.55 summed cost_usd.
- synthesize: 7 run(s), $8.22 summed cost_usd.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/context.rs is 2035 lines, 35 past the 2000-line cap and UNLISTED — 15 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/context.rs", 2035) to GRANDFATHERED_OVERSIZED_MODULES.
src/help.rs is 2946 lines vs its recorded 2856 (+90 drift) — 10 more line(s) makes it FATAL. Fix: paste ("src/help.rs", 2946) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 182 day(s) old (repo is day 256) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-253 claimed 4 success(es), but the 16 task commit(s) in this window carry day-254..day-256 (3 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.7) — predicted 3×, never graded; stale (173 snapshots)
- src/commands_bg_kill_tests.rs (3.3) — predicted 8×, never graded; stale (65 snapshots)
- src/format/cost/bedrock_region.rs (3.0) — predicted 3×, never graded; stale (30 snapsho…
- never forecast (0 predictions ever, unranked): src/providers_key_tests.rs, src/checkpoint_stop_tests.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
