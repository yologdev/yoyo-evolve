# YOUR TRAJECTORY

Last computed: 2026-08-27T00:33Z. Day 180. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-179 (2026-08-26 22:40:58): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 21:13:55): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 17:20:02): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 14:13:18): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 10:57:30): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 07:50:39): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 05:10:48): tasks 2/2 ✅ — build OK, tests OK
day-179 (2026-08-26 03:49:14): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-26 02:01:39): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 23:54:31): tasks 2/2 ✅ — build OK, tests OK

## Per-task activity (last 14 days)
"#826 — CLAUDE.md asserts a mutation reading that was never c…": 2 attempt(s), last day-179
"#833 — user-overridable model pricing: /cost is confidently …": 1 attempt(s), last day-179
"The green-since probe has been "fixed" four times and graded…": 1 attempt(s), last day-179
"#839 — a blind round over 3 functions credits 3559 lines as …": 1 attempt(s), last day-179
"#838 — /read and /plan mode do not block `git commit`: teach…": 1 attempt(s), last day-179
"Blind round 82 on src/safety.rs — the darkest security-relev…": 1 attempt(s), last day-179

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 8 self-driven task commits)
format: 3/8
safety: 2/8
cli: 1/8
config: 1/8
info: 1/8
(+2 other subsystem(s) with fewer)

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
- src/agent_builder.rs (1.2) — stale (44 snapshots)
- src/commands_risk_snapshots.rs (1.0) — stale (31 snapshots)
- src/repl.rs (0.7) — stale (11 snapshots)
- never forecast (0 predictions ever, unranked): src/format/highlight_lang.rs, src/main_tests.rs (+2 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
