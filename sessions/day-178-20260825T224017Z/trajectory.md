# YOUR TRAJECTORY

Last computed: 2026-08-25T21:25Z. Day 178. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-178 (2026-08-25 20:18:26): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 17:10:50): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 14:43:11): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 11:17:04): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 08:42:30): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 05:10:00): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 02:52:47): tasks 2/2 ✅ — build OK, tests OK
day-178 (2026-08-25 01:25:49): tasks 2/2 ✅ — build OK, tests OK
day-177 (2026-08-25 00:12:50): tasks 2/2 ✅ — build OK, tests OK
day-177 (2026-08-24 22:40:15): tasks 2/2 ✅ — build OK, tests OK

## Per-task activity (last 14 days)
"mutants.toml is dead config — move it to .cargo/, fix the st…": 1 attempt(s), last day-178
"Mutation repair #2 — kill the 16 recorded survivors in src/g…": 1 attempt(s), last day-178
"#823 — make `[directories]` globs actually work, instead of …": 1 attempt(s), last day-178
"The green-since probe asks the wrong question and trusts a `…": 1 attempt(s), last day-178
"The CI section reports cured failures as live — add "has CI …": 1 attempt(s), last day-178
"#749 item 2 — ask once per folder. The interactive workspace…": 1 attempt(s), last day-178

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 8 self-driven task commits)
cli: 4/8
config: 4/8
help: 2/8
format: 1/8
git_commit: 1/8
(+2 other subsystem(s) with fewer)
⚠️ cli took 4 of the last 8 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## Recurring CI errors (failed runs, last 14 days)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[5×, last 1d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--bin yoyo`
[5×, last 1d ago] ##[error]process completed with exit code 101.
[4×, last 1d ago] test result: failed. 5073 passed; 1 failed; 1 ignored; 0 measured; 0 filtered ou
[3×, last 1d ago] assertion failed: (cost - 4.5).abs() < 0.001
[1×, last 1d ago] thread 'prompt::tests::test_apply_effort_hint_low_prepends' (21210) panicked at 

## Provider/API health
10 sessions, no provider errors detected.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/agent_builder.rs (1.0) — stale (33 snapshots)
- src/safety.rs (1.0) — stale (28 snapshots)
- src/commands_risk_snapshots.rs (0.8) — stale (20 snapshots)
- never forecast (0 predictions ever, unranked): src/format/highlight_lang.rs, src/main_tests.rs (+2 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
