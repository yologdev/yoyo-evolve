# YOUR TRAJECTORY

Last computed: 2026-10-20T14:45Z. Day 234. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-234 (2026-10-20 10:11:06): tasks 2/2 ✅ — build OK, tests OK
day-234 (2026-10-20 01:59:38): tasks 2/2 ✅ — build OK, tests OK
day-233 (2026-10-19 17:49:13): tasks 2/2 ✅ — build OK, tests OK
day-233 (2026-10-19 10:59:13): tasks 2/2 ✅ — build OK, tests OK
day-233 (2026-10-19 07:32:48): tasks 2/2 ✅ — build OK, tests OK
day-232 (2026-10-19 01:28:07): tasks 2/2 ✅ — build OK, tests OK
day-232 (2026-10-18 23:52:27): tasks 2/2 ✅ — build OK, tests OK
day-232 (2026-10-18 19:45:17): tasks 2/2 ✅ — build OK, tests OK
day-232 (2026-10-18 15:01:28): tasks 2/2 ✅ — build OK, tests OK
day-232 (2026-10-18 09:44:42): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 4 checkable claiming sessions claimed success with no task commits.
6 further claiming session(s) could NOT be checked — NOT counted above: 5 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.
Shape, not a per-run accident: 5 of 10 claiming sessions open before the clone. Window = last 10 sessions; clone = 50 commits, oldest 2026-10-19 06:29 UTC; 4 of the 10 sessions read open their window after it. The window is this script's; the fetch depth is set in .github/workflows/evolve.yml (protected).

## Per-task activity (last 14 days)
"/spawn no longer force-removes a worktree that still holds u…": 2 attempt(s), last day-234
"Fixes #1053: a read failure no longer becomes an empty file …": 2 attempt(s), last day-233
"Fixes #1052: the write_file diff preview no longer hangs on …": 2 attempt(s), last day-233
"Turn main green: --print-system-prompt no longer demands an …": 2 attempt(s), last day-233
"Close #982's last body rows: bare `yoyo run` and `tree`/`und…": 1 attempt(s), last day-234
"Release v0.3.0: complete CHANGELOG, bump version, run the ga…": 1 attempt(s), last day-234

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 9 self-driven task commits)
dispatch: 3/9
cli: 2/9
main: 2/9
config: 1/9
dev: 1/9
(+8 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (4 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[1×, last 1d ago] test fifo_config_system_file_does_not_hang_and_is_named ... failed
[1×, last 1d ago] test regular_config_system_file_is_still_read ... failed
[1×, last 1d ago] thread 'fifo_config_system_file_does_not_hang_and_is_named' (20525) panicked at 
[1×, last 1d ago] stderr="error: no api key found.\nset anthropic_api_key env var, use --api-key <
[1×, last 1d ago] thread 'regular_config_system_file_is_still_read' (20583) panicked at tests/conf

... (truncated to fit token budget)
