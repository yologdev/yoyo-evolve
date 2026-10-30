# YOUR TRAJECTORY

Last computed: 2026-10-30T08:37Z. Day ?. Window: last 10 sessions / 14 days.

## Per-task activity (last 14 days)
"Fixes #1103: refuse a mistyped --provider at startup (exit 2…": 2 attempt(s), last day-244
"Fixes #1101: hard_deny tracks quote state across lines, so a…": 2 attempt(s), last day-243
"Mark the four multi-block tool-result sites as reaching the …": 2 attempt(s), last day-243
"stream-json: does its collected text join both attempts afte…": 2 attempt(s), last day-243
"Fixes #1099: close every live stdio MCP server on exit (REPL…": 2 attempt(s), last day-242
"Fixes #1071: count synthesize in the trajectory's phase spen…": 1 attempt(s), last day-244

## Subsystem concentration (last 11 self-driven task commits)
dispatch: 4/11
cli: 2/11
dev: 2/11
goal: 2/11
hard: 2/11
(+11 other subsystem(s) with fewer)

## CI: could not check recent failures (YOYO_REPO unset) — this is NOT a clean bill of health

## Provider/API health: not checked — YOYO_AUDIT_DIR is unset, so there is no audit-log to scan (this is normal for a hand-run).

## Usage records: not checked — YOYO_AUDIT_DIR is unset, so there is no audit-log to scan (normal for a hand-run). Not a clean bill.

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: could not check (no audit-log worktree (YOYO_AUDIT_DIR)) — NOT zero spend.
- dream: could not check (no audit-log worktree (YOYO_AUDIT_DIR)) — NOT zero spend.
- synthesize: could not check (no audit-log worktree (YOYO_AUDIT_DIR)) — NOT zero spend.
Totals are a floor: these sums omit sub-agent spend (#1073).

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/config.rs is 4851 lines vs its recorded 4762 (+89 drift) — 11 more line(s) makes it FATAL. Fix: paste ("src/config.rs", 4851) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 170 day(s) old (repo is day 244) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity: not checked — no claimed successes to read, or no task commits found in the git window at all. This is a REFUSAL, not 'every claimed success landed'.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.6) — predicted 3×, never graded; stale (122 snapshots)
- src/commands_bg_kill_tests.rs (2.7) — predicted 8×, never graded; stale (14 snapshots)
- src/commands_risk_unhittable.rs (1.5) — stale (115 snapshots)
- never forecast (0 predictions ever, unranked): src/grep_status.rs, src/tools_search_flag_tests.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
