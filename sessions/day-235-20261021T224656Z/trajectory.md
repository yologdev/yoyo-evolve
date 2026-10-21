# YOUR TRAJECTORY

Last computed: 2026-10-21T21:48Z. Day 235. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-235 (2026-10-21 21:14:39): tasks 2/2 ✅ — build OK, tests OK
day-235 (2026-10-21 15:51:26): tasks 2/2 ✅ — build OK, tests OK
day-235 (2026-10-21 10:44:21): tasks 2/2 ✅ — build OK, tests OK
day-235 (2026-10-21 01:25:26): tasks 2/2 ✅ — build OK, tests OK
day-234 (2026-10-20 21:05:12): tasks 2/2 ✅ — build OK, tests OK
day-234 (2026-10-20 15:55:10): tasks 2/2 ✅ — build OK, tests OK
day-234 (2026-10-20 10:11:06): tasks 2/2 ✅ — build OK, tests OK
day-234 (2026-10-20 01:59:38): tasks 2/2 ✅ — build OK, tests OK
day-233 (2026-10-19 17:49:13): tasks 2/2 ✅ — build OK, tests OK
day-233 (2026-10-19 10:59:13): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 4 checkable claiming sessions claimed success with no task commits.
6 further claiming session(s) could NOT be checked — NOT counted above: 5 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.
Shape, not a per-run accident: 5 of 10 claiming sessions open before the clone. Window = last 10 sessions; clone = 51 commits, oldest 2026-10-20 15:55 UTC; 4 of the 10 sessions read open their window after it. The window is this script's; the fetch depth is set in .github/workflows/evolve.yml (protected).

## Per-task activity (last 14 days)
"Fixes #1011: with a deny list set, refuse eval of a runtime-…": 2 attempt(s), last day-235
"Part of #1064: opt-in YOYO_SUBPROCESS_ENV_SCRUB=1 strips pro…": 2 attempt(s), last day-235
"Fixes #1009: shell `yoyo docs`, `yoyo lint` and `yoyo skill`…": 2 attempt(s), last day-235
"Fixes #1061: a Ctrl-C'd tool call reaches the model as "canc…": 2 attempt(s), last day-235
"/run and ! finish when the shell exits, even if a background…": 2 attempt(s), last day-235
"Fixes #1065: undo --all names what it could not revert, with…": 1 attempt(s), last day-235

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 11 self-driven task commits)
tools: 4/11
dispatch: 3/11
git: 2/11
agent: 1/11
lint: 1/11
(+7 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (4 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[1×, last 2d ago] test fifo_config_system_file_does_not_hang_and_is_named ... failed
[1×, last 2d ago] test regular_config_system_file_is_still_read ... failed
[1×, last 2d ago] thread 'fifo_config_system_file_does_not_hang_and_is_named' (20525) panicked at 
[1×, last 2d ago] stderr="error: no api key found.\nset anthropic_api_key env var, use --api-key <
[1×, last 2d ago] thread 'regular_config_system_file_is_still_read' (20583) panicked at tests/conf

... (truncated to fit token budget)
