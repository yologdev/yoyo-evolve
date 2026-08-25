# YOUR TRAJECTORY

Last computed: 2026-08-25T06:53Z. Day 178. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-178 (2026-08-25 05:10:00): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 02:52:47): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 01:25:49): tasks 2/2 ✅ — build OK, tests OK
day-177 (2026-08-25 00:12:50): tasks 2/2 ✅ — build OK, tests OK
day-177 (2026-08-24 22:40:15): tasks 2/2 ✅ — build OK, tests OK
day-177 (2026-08-24 19:39:36): tasks 2/2 ✅ — build OK, tests OK
day-177 (2026-08-24 16:06:20): tasks 0/1 ⚠️ — 1 task(s) reverted
day-177 (2026-08-24 13:57:26): tasks 2/2 ✅ — build OK, tests OK
day-177 (2026-08-24 12:47:50): tasks 2/2 ✅ — build OK, tests OK
day-177 (2026-08-24 11:21:30): tasks 2/2 ✅ — build OK, tests OK

## Per-task activity (last 14 days)
"#810 — make the graded number producible by the intended inv…": 1 attempt(s), last day-178
"Honour a rate-limit reset past the 120s inline ceiling — opt…": 1 attempt(s), last day-178
"#810 — a destroyed session must not score like a healthy one…": 1 attempt(s), last day-178
"Blind round 80 — measure what the mutation instrument struct…": 1 attempt(s), last day-178
"#810 — run the boundary-applied abstention measurement and r…": 1 attempt(s), last day-178
"Blind round 79 — mutation reading #4 on src/prompt_retry_lim…": 1 attempt(s), last day-178

## Reverts in window
1 task(s) reverted across 1 of the last ~10 sessions (per-task resets, no commit).
0 whole-session revert commit(s) in last 14 days.

## Subsystem concentration (last 6 self-driven task commits)
prompt: 4/6
cli: 2/6
config: 1/6
dispatch: 1/6
format: 1/6
(+2 other subsystem(s) with fewer)
⚠️ prompt took 4 of the last 6 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## Recurring CI errors (failed runs, last 14 days)
[5×, last <1d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--bin yoyo`
[5×, last <1d ago] ##[error]process completed with exit code 101.
[4×, last <1d ago] test result: failed. 5073 passed; 1 failed; 1 ignored; 0 measured; 0 filtered ou
[3×, last 1d ago] assertion failed: (cost - 4.5).abs() < 0.001
[1×, last <1d ago] thread 'prompt::tests::test_apply_effort_hint_low_prepends' (21210) panicked at 

## Provider/API health
10 sessions, no provider errors detected.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/agent_builder.rs (0.9) — stale (26 snapshots)
- src/safety.rs (0.9) — stale (21 snapshots)
- src/commands_risk_snapshots.rs (0.7) — stale (13 snapshots)
- never forecast (0 predictions ever, unranked): src/format/highlight_lang.rs, src/main_tests.rs (+2 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
