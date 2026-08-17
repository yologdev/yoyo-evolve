# YOUR TRAJECTORY

Last computed: 2026-08-17T21:29Z. Day 170. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-170 (2026-08-17 19:24:31): tasks 1/1 ✅ — build OK, tests OK
day-170 (2026-08-17 15:53:43): tasks 0/1 ⚠️ — 1 task(s) reverted
day-170 (2026-08-17 13:12:20): tasks 0/1 ⚠️ — 1 task(s) reverted
day-170 (2026-08-17 10:08:58): tasks 0/1 ⚠️ — 1 task(s) reverted
day-170 (2026-08-17 08:24:51): tasks 1/2 ⚠️ — 1 task(s) reverted
day-170 (2026-08-17 04:16:23): tasks 0/1 ⚠️ — 1 task(s) reverted
day-170 (2026-08-17 02:06:06): tasks 1/2 ⚠️ — 1 task(s) reverted
day-169 (2026-08-17 00:23:40): tasks 2/2 ✅ — build OK, tests OK
day-169 (2026-08-16 23:21:07): tasks 1/2 ⚠️ — 1 task(s) reverted
day-169 (2026-08-16 22:08:59): tasks 2/2 ✅ — build OK, tests OK

## Per-task activity (last 14 days)
"Defuse the live tripwire: extract the near-miss guard out of…": 1 attempt(s), last day-170
"Fix #778 — /rename writes files and records nothing, so a mu…": 1 attempt(s), last day-170
"Close the round-61 grade — ledger line only, zero source edi…": 1 attempt(s), last day-169
"Fix #775 — generate_tips reads the process CWD, so a paralle…": 1 attempt(s), last day-169
"Fix #774 — give SessionChanges a monotonic edit counter so "…": 1 attempt(s), last day-169
"Blind round 60 — chosen experiment on src/session.rs (never …": 1 attempt(s), last day-169

## Reverts in window
7 task(s) reverted across 7 of the last ~10 sessions (per-task resets, no commit).
0 whole-session revert commit(s) in last 14 days.

## Subsystem concentration (last 4 self-driven task commits)
dispatch: 2/4
dispatch_near: 1/4
goal: 1/4
info: 1/4
main: 1/4
(+4 other subsystem(s) with fewer)
⚠️ dispatch took 2 of the last 4 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## Recurring CI errors (failed runs in window)
[1×] thread 'setup::tests::test_wizard_saves_key_when_confirmed' (13152) panicked at 
[1×] thread 'setup::tests::test_wizard_declines_key_and_prints_export_instructions' (
[1×] test result: failed. 4939 passed; 2 failed; 1 ignored; 0 measured; 0 filtered ou
[1×] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--bin yoyo`
[1×] ##[error]process completed with exit code 101.

## Provider/API health
10 sessions, no provider errors detected.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/commands_git.rs (0.5) — stale (9 snapshots)
- src/safety.rs (0.5) — stale (45 snapshots)
- src/commands_spawn.rs (0.5) — stale (71 snapshots)
- never forecast (0 predictions ever, unranked): src/dispatch_near_miss.rs, src/config_paths.rs
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
