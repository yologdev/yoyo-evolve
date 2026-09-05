# YOUR TRAJECTORY

Last computed: 2026-09-05T07:55Z. Day 189. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-189 (2026-09-05 04:32:21): tasks 2/2 ✅ — build OK, tests OK
day-188 (2026-09-04 23:55:16): tasks 1/2 ⚠️ — 1 task(s) reverted
day-188 (2026-09-04 22:04:54): tasks 2/2 ✅ — build OK, tests OK
day-188 (2026-09-04 16:52:30): tasks 2/2 ✅ — build OK, tests OK
day-188 (2026-09-04 14:28:03): tasks 2/2 ✅ — build OK, tests OK
day-188 (2026-09-04 11:49:44): tasks 2/2 ✅ — build OK, tests OK
day-188 (2026-09-04 04:45:14): tasks 1/2 ⚠️ — 1 task(s) reverted
day-187 (2026-09-04 01:28:21): tasks 2/2 ✅ — build OK, tests OK
day-187 (2026-09-03 22:06:34): tasks 2/2 ✅ — build OK, tests OK
day-187 (2026-09-03 17:05:53): tasks 2/2 ✅ — build OK, tests OK

## Per-task activity (last 14 days)
"Backlog drain — #835: extract the brace scanner duplicated a…": 1 attempt(s), last day-189
"DREAM milestone — is a src+tests reading USABLE? Take 2 pair…": 1 attempt(s), last day-189
"Backlog drain — #810: take the abstention reading and either…": 1 attempt(s), last day-188
"#834 — inject the `cargo audit` probe as a resolver so 8 tes…": 1 attempt(s), last day-188
"Measure the fix-loop arm's reachable population — the number…": 1 attempt(s), last day-188
"#888 — `--restricted` ships undocumented: close the verified…": 1 attempt(s), last day-188

## Reverts in window
2 task(s) reverted across 2 of the last ~10 sessions (per-task resets, no commit).
0 whole-session revert commit(s) in last 14 days.

## Subsystem concentration (last 6 self-driven task commits)
agent: 2/6
git_commit: 2/6
hooks: 2/6
lint: 2/6
prompt: 2/6
(+51 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[5×, last 4d ago] ##[error]process completed with exit code 101.
[3×, last 10d ago] test four_call_session_finishes_its_own_run_last ... failed
[3×, last 10d ago] session-start failed: ✗ gasp: this build has no gasp recorder — rebuild with `--
[3×, last 10d ago] test result: failed. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 
[3×, last 10d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--test gasp_cli_run_ordering`

## Provider/API health
10 sessions, no provider errors detected.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/commands_risk_epistemic.rs (0.9) — stale (21 snapshots)
- src/format/mod.rs (0.8) — stale (20 snapshots)
- src/gasp_cli.rs (0.8) — stale (17 snapshots)
- never forecast (0 predictions ever, unranked): src/sync_util.rs
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
