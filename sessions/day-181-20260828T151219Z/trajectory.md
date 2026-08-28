# YOUR TRAJECTORY

Last computed: 2026-08-28T13:57Z. Day 181. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-181 (2026-08-28 13:41:09): tasks 2/2 ✅ — build OK, tests OK
day-181 (2026-08-28 02:09:32): tasks 2/2 ✅ — build OK, tests OK
day-180 (2026-08-28 00:52:29): tasks 2/2 ✅ — build OK, tests OK
day-180 (2026-08-27 22:17:56): tasks 2/2 ✅ — build OK, tests OK
day-180 (2026-08-27 20:49:32): tasks 1/2 ⚠️ — 1 task(s) reverted
day-180 (2026-08-27 15:20:00): tasks 2/2 ✅ — build OK, tests OK
day-180 (2026-08-27 08:45:07): tasks 2/2 ✅ — build OK, tests OK
day-180 (2026-08-27 01:25:49): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 22:40:58): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 21:13:55): tasks 2/2 ✅ — build OK, tests OK

## Per-task activity (last 14 days)
"#842 — connect_external_servers reports an mcp_count that su…": 1 attempt(s), last day-181
"The usage-coverage line is emitting a false alarm one day af…": 1 attempt(s), last day-181
"#846 — auto_risk_snapshot's dedup guards the LAST ledger lin…": 1 attempt(s), last day-181
"#848 follow-up — a coverage check on the usage channel: does…": 1 attempt(s), last day-181
"#843 — extract_trajectory.py crashes when YOYO_AUDIT_DIR is …": 1 attempt(s), last day-181
"#849 prevention — session_end must check the session node EX…": 1 attempt(s), last day-181

## Reverts in window
1 task(s) reverted across 1 of the last ~10 sessions (per-task resets, no commit).
0 whole-session revert commit(s) in last 14 days.

## Subsystem concentration (last 7 self-driven task commits)
gasp: 3/7
agent: 2/7
main: 2/7
prompt: 2/7
risk: 2/7
(+51 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[5×, last 2d ago] ##[error]process completed with exit code 101.
[3×, last 2d ago] test four_call_session_finishes_its_own_run_last ... failed
[3×, last 2d ago] session-start failed: ✗ gasp: this build has no gasp recorder — rebuild with `--
[3×, last 2d ago] test result: failed. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 
[3×, last 2d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--test gasp_cli_run_ordering`

## Provider/API health
10 sessions, no provider errors detected.

## Usage records
3 of 3 measurable sessions carry >=1 usage record (#848 channel is live).
7 session(s) predate the #848 producer (8a633cff) and cannot be measured.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/main_tests.rs (2.0) — predicted 1×, never graded
- src/commands_risk.rs (0.8) — stale (20 snapshots)
- src/help.rs (0.7) — stale (12 snapshots)
- never forecast (0 predictions ever, unranked): src/format/highlight_lang.rs, src/commands_tree.rs (+1 more)
... (truncated to fit token budget)
