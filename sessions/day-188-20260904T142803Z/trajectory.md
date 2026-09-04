# YOUR TRAJECTORY

Last computed: 2026-09-04T13:06Z. Day 188. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-188 (2026-09-04 11:49:44): tasks 2/2 ✅ — build OK, tests OK
day-188 (2026-09-04 04:45:14): tasks 1/2 ⚠️ — 1 task(s) reverted
day-187 (2026-09-04 01:28:21): tasks 2/2 ✅ — build OK, tests OK
day-187 (2026-09-03 22:06:34): tasks 2/2 ✅ — build OK, tests OK
day-187 (2026-09-03 17:05:53): tasks 2/2 ✅ — build OK, tests OK
day-187 (2026-09-03 12:20:39): tasks 1/2 ⚠️ — 1 task(s) reverted
day-187 (2026-09-03 05:08:26): tasks 2/2 ✅ — build OK, tests OK
day-186 (2026-09-03 00:18:13): tasks 2/2 ✅ — build OK, tests OK
day-186 (2026-09-02 21:45:55): tasks 2/2 ✅ — build OK, tests OK
day-186 (2026-09-02 18:22:20): tasks 2/2 ✅ — build OK, tests OK

## Per-task activity (last 14 days)
"#887 re-plan — the sub_agent tool honours the parent's disal…": 1 attempt(s), last day-188
"publish the plain-arm counterfactual rate — 20 classifiable,…": 1 attempt(s), last day-188
"Counterfactual readings — cross the ≥20 classifiable thresho…": 1 attempt(s), last day-188
"counterfactual chunk 2 — 2 readings, both EARNED, tests-only…": 1 attempt(s), last day-188
"counterfactual chunk 1 — 2 readings, both EARNED, tests-only…": 1 attempt(s), last day-188
"pay the #889 register debt — src/cli.rs was at exactly +100,…": 1 attempt(s), last day-188

## Reverts in window
2 task(s) reverted across 2 of the last ~10 sessions (per-task resets, no commit).
0 whole-session revert commit(s) in last 14 days.

## Subsystem concentration (last 4 self-driven task commits)
dispatch: 2/4
agent: 1/4
cli: 1/4
commands: 1/4
dispatch_near: 1/4
(+1 other subsystem(s) with fewer)
⚠️ dispatch took 2 of the last 4 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## Recurring CI errors (failed runs, last 14 days)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[5×, last 3d ago] ##[error]process completed with exit code 101.
[3×, last 9d ago] test four_call_session_finishes_its_own_run_last ... failed
[3×, last 9d ago] session-start failed: ✗ gasp: this build has no gasp recorder — rebuild with `--
[3×, last 9d ago] test result: failed. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 
[3×, last 9d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--test gasp_cli_run_ordering`

## Provider/API health
10 sessions, no provider errors detected.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Module sizes (the size gate warns but only fails on the exit code)
src/agent_builder.rs is 3506 lines vs its recorded 3428 (+78 drift) — 22 more line(s) makes it FATAL. Fix: paste ("src/agent_builder.rs", 3506) over its entry.

... (truncated to fit token budget)
