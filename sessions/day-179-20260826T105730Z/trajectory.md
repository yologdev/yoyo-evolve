# YOUR TRAJECTORY

Last computed: 2026-08-26T09:44Z. Day 179. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-179 (2026-08-26 07:50:39): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 05:10:48): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 03:49:14): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-26 02:01:39): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 23:54:31): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 22:40:17): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 20:18:26): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 17:10:50): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 14:43:11): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 11:17:04): tasks 2/2 ✅ — build OK, tests OK

## Per-task activity (last 14 days)
"#826 — CLAUDE.md asserts a mutation reading that was never c…": 2 attempt(s), last day-179
"The green-since probe returns stale pages — `--status succes…": 1 attempt(s), last day-179
"The harness gate cannot see `#![cfg(feature)]` test files — …": 1 attempt(s), last day-179
"#832 — stop `test_handle_evolution_no_panic` spawning a nest…": 1 attempt(s), last day-179
"#828 item 2 — honour `--worker` in the `yoyo gasp` CLI door,…": 1 attempt(s), last day-179
"`/config show` must name the config files it skipped (the re…": 1 attempt(s), last day-179

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 9 self-driven task commits)
gasp: 3/9
git_commit: 3/9
config: 2/9
dispatch: 1/9
dispatch_near: 1/9
(+3 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[5×, last <1d ago] ##[error]process completed with exit code 101.
[3×, last <1d ago] test four_call_session_finishes_its_own_run_last ... failed
[3×, last <1d ago] session-start failed: ✗ gasp: this build has no gasp recorder — rebuild with `--
[3×, last <1d ago] test result: failed. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 
[3×, last <1d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--test gasp_cli_run_ordering`

## Provider/API health
10 sessions, no provider errors detected.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/agent_builder.rs (1.1) — stale (39 snapshots)
- src/safety.rs (1.1) — stale (34 snapshots)
- src/commands_risk_snapshots.rs (0.9) — stale (26 snapshots)
- never forecast (0 predictions ever, unranked): src/format/highlight_lang.rs, src/main_tests.rs (+2 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
