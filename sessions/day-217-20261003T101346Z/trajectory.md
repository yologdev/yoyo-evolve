# YOUR TRAJECTORY

Last computed: 2026-10-03T09:51Z. Day ?. Window: last 10 sessions / 14 days.

## Per-task activity (last 14 days)
"Child bash (sub_agent / explore_agent) obeys the parent's ha…": 2 attempt(s), last day-217
"`yoyo test` and `yoyo run` exit nonzero when the thing they …": 2 attempt(s), last day-217
"stream-json acknowledges a restored session: `{"type":"sessi…": 2 attempt(s), last day-216
"A pipe gets ONE answer: stop re-streaming a dead attempt's p…": 2 attempt(s), last day-216
"BEL bytes stop landing on stdout in -p / piped mode (#976, h…": 2 attempt(s), last day-216
"`--save-session <path>`: let `-p` and piped runs write a res…": 1 attempt(s), last day-216

## Subsystem concentration (last 9 self-driven task commits)
main: 5/9
tools: 3/9
cli: 2/9
help: 2/9
prompt: 2/9
(+4 other subsystem(s) with fewer)
⚠️ main took 5 of the last 9 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## Recurring CI errors (failed runs, last 14 days) (4 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[1×, last 4d ago] thread 'prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end' 
[1×, last 4d ago] test result: failed. 5861 passed; 1 failed; 3 ignored; 0 measured; 0 filtered ou
[1×, last 4d ago] ^[[1m^[[91merror^[[0m: test failed, to rerun pass `--bin yoyo`
[1×, last 4d ago] ##[error]process completed with exit code 101.

## Provider/API health: not checked — YOYO_AUDIT_DIR is unset, so there is no audit-log to scan (this is normal for a hand-run).

## Usage records: not checked — YOYO_AUDIT_DIR is unset, so there is no audit-log to scan (normal for a hand-run). Not a clean bill.

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/format/mod.rs is 2874 lines vs its recorded 2788 (+86 drift) — 14 more line(s) makes it FATAL. Fix: paste ("src/format/mod.rs", 2874) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 143 day(s) old (repo is day 217) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity: not checked — no claimed successes to read, or no task commits found in the git window at all. This is a REFUSAL, not 'every claimed success landed'.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
... (truncated to fit token budget)
