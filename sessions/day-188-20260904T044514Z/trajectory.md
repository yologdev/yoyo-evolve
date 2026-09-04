# YOUR TRAJECTORY

Last computed: 2026-09-04T03:29Z. Day 188. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-187 (2026-09-04 01:28:21): tasks 2/2 ✅ — build OK, tests OK
day-187 (2026-09-03 22:06:34): tasks 2/2 ✅ — build OK, tests OK
day-187 (2026-09-03 17:05:53): tasks 2/2 ✅ — build OK, tests OK
day-187 (2026-09-03 12:20:39): tasks 1/2 ⚠️ — 1 task(s) reverted
day-187 (2026-09-03 05:08:26): tasks 2/2 ✅ — build OK, tests OK
day-186 (2026-09-03 00:18:13): tasks 2/2 ✅ — build OK, tests OK
day-186 (2026-09-02 21:45:55): tasks 2/2 ✅ — build OK, tests OK
day-186 (2026-09-02 18:22:20): tasks 2/2 ✅ — build OK, tests OK
day-186 (2026-09-02 12:11:10): tasks 2/2 ✅ — build OK, tests OK
day-186 (2026-09-02 04:53:33): tasks 2/2 ✅ — build OK, tests OK

## Per-task activity (last 14 days)
"#879 slice 2 (re-plan) — `--restricted` removes the command-…": 1 attempt(s), last day-187
"#870 slice 2 — wire the `#[cfg(test)]` splicer into the coun…": 1 attempt(s), last day-187
"#886 — `yoyo model list` spends a billed LLM turn; route `mo…": 1 attempt(s), last day-187
"#885 — the module-size gate gives growth 100 lines of grace …": 1 attempt(s), last day-187
"#883 — /model list and /model info are routed but two discov…": 1 attempt(s), last day-187
"#870 slice 1 — a pure `#[cfg(test)]` splicer, so the counter…": 1 attempt(s), last day-187

## Reverts in window
1 task(s) reverted across 1 of the last ~10 sessions (per-task resets, no commit).
0 whole-session revert commit(s) in last 14 days.

## Subsystem concentration (last 6 self-driven task commits)
cli: 3/6
dispatch: 3/6
commands: 2/6
config: 2/6
dispatch_near: 2/6
(+51 other subsystem(s) with fewer)
⚠️ cli took 3 of the last 6 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## Recurring CI errors (failed runs, last 14 days)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[5×, last 2d ago] ##[error]process completed with exit code 101.
[3×, last 8d ago] test four_call_session_finishes_its_own_run_last ... failed
[3×, last 8d ago] session-start failed: ✗ gasp: this build has no gasp recorder — rebuild with `--
[3×, last 8d ago] test result: failed. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; 
[3×, last 8d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--test gasp_cli_run_ordering`

## Provider/API health
10 sessions, no provider errors detected.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Module sizes (the size gate warns but only fails on the exit code)
src/cli.rs is 6620 lines vs its recorded 6520 (+100 drift) — 0 more line(s) makes it FATAL. Fix: paste ("src/cli.rs", 6620) over its entry.

... (truncated to fit token budget)
