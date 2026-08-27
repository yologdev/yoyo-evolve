# YOUR TRAJECTORY

Last computed: 2026-08-27T19:36Z. Day 180. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-180 (2026-08-27 15:20:00): tasks 2/2 ✅ — build OK, tests OK
day-180 (2026-08-27 08:45:07): tasks 2/2 ✅ — build OK, tests OK
day-180 (2026-08-27 01:25:49): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 22:40:58): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 21:13:55): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 17:20:02): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 14:13:18): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 10:57:30): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 07:50:39): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 05:10:48): tasks 2/2 ✅ — build OK, tests OK

## Per-task activity (last 14 days)
"Close the MCP collision guard's fail-open branch (#841)": 1 attempt(s), last day-180
"Bound ShellHook stderr before it enters the conversation (#8…": 1 attempt(s), last day-180
"Sub-agent model fallback — the `sub_agent` tool has no fallb…": 1 attempt(s), last day-180
"Green-probe staleness detector — stop asserting "CI is red" …": 1 attempt(s), last day-180
"Blind round 83 on src/agent_builder.rs — the #1 dark room (4…": 1 attempt(s), last day-180
"Correct the stale "the sidecar is still live" claims — #683 …": 1 attempt(s), last day-180

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 12 self-driven task commits)
agent: 3/12
format: 3/12
safety: 2/12
cli: 1/12
config: 1/12
(+6 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[5×, last 1d ago] ##[error]process completed with exit code 101.
[3×, last 1d ago] test four_call_session_finishes_its_own_run_last ... failed
[3×, last 1d ago] session-start failed: ✗ gasp: this build has no gasp recorder — rebuild with `--
[3×, last 1d ago] test result: failed. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 
[3×, last 1d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--test gasp_cli_run_ordering`

## Provider/API health
10 sessions, no provider errors detected.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/commands_risk_snapshots.rs (1.1) — stale (34 snapshots)
- src/repl.rs (0.7) — stale (14 snapshots)
- src/commands_risk.rs (0.7) — stale (14 snapshots)
- never forecast (0 predictions ever, unranked): src/format/highlight_lang.rs, src/main_tests.rs (+2 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
