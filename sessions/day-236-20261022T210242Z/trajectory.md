# YOUR TRAJECTORY

Last computed: 2026-10-22T20:53Z. Day ?. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-236 (2026-10-22 15:56:06): tasks 2/2 ✅ — build OK, tests OK
day-236 (2026-10-22 10:34:15): tasks 2/2 ✅ — build OK, tests OK
day-236 (2026-10-22 01:55:52): tasks 2/2 ✅ — build OK, tests OK
day-235 (2026-10-21 22:46:56): tasks 2/2 ✅ — build OK, tests OK
day-235 (2026-10-21 21:14:39): tasks 2/2 ✅ — build OK, tests OK
day-235 (2026-10-21 15:51:26): tasks 2/2 ✅ — build OK, tests OK
day-235 (2026-10-21 10:44:21): tasks 2/2 ✅ — build OK, tests OK
day-235 (2026-10-21 01:25:26): tasks 2/2 ✅ — build OK, tests OK
day-234 (2026-10-20 21:05:12): tasks 2/2 ✅ — build OK, tests OK
day-234 (2026-10-20 15:55:10): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 4 checkable claiming sessions claimed success with no task commits.
6 further claiming session(s) could NOT be checked — NOT counted above: 5 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.
Shape, not a per-run accident: 5 of 10 claiming sessions open before the clone. Window = last 10 sessions; clone = 55 commits, oldest 2026-10-21 20:30 UTC; 4 of the 10 sessions read open their window after it. The window is this script's; the fetch depth is set in .github/workflows/evolve.yml (protected).

## Per-task activity (last 14 days)
"Part of #1062: the 401 hint names where the key actually cam…": 2 attempt(s), last day-236
"Part of #1068: bash guard refusals count as deterministic re…": 2 attempt(s), last day-236
"Part of #1014: /spawn write tap, first cut — count paths a w…": 2 attempt(s), last day-236
"Part of #1064: key presence reads yoagent's resolver, so `GE…": 2 attempt(s), last day-235
"Fixes #1011: with a deny list set, refuse eval of a runtime-…": 2 attempt(s), last day-235
"Part of #944: the trajectory reads the per-phase spend rows …": 1 attempt(s), last day-236

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 7 self-driven task commits)
cli: 4/7
main: 4/7
prompt: 3/7
providers: 3/7
tools: 3/7
(+79 other subsystem(s) with fewer)
⚠️ cli took 4 of the last 7 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## CI: could not check recent failures (YOYO_REPO unset) — this is NOT a clean bill of health

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 1 run(s), $1.25 summed cost_usd.
- dream: no rows yet (phases/dream/spend.jsonl is not on audit-log).
Totals are a floor: sub-agent tokens are not counted (yoagent#173).

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
... (truncated to fit token budget)
