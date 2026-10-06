# YOUR TRAJECTORY

Last computed: 2026-10-06T21:12Z. Day ?. Window: last 10 sessions / 14 days.

## Per-task activity (last 14 days)
"Close the `--allow-dir` symlink escapes (#996 part 3) — redo…": 2 attempt(s), last day-219
"`yoyo --version` on a crates.io install shows the published …": 2 attempt(s), last day-219
"Trajectory green-since probe: a page frozen AT the newest fa…": 1 attempt(s), last day-220
"#988 Q3 — does a sub-agent child cancelled by Ctrl-C get mis…": 1 attempt(s), last day-220
"Opt-in `retry_after_partial` config key so a transient Overl…": 1 attempt(s), last day-220
"`yoyo diff` / `yoyo blame` outside a git repo exit nonzero (…": 1 attempt(s), last day-219

## Subsystem concentration (last 7 self-driven task commits)
main: 4/7
config: 3/7
prompt: 3/7
tools: 3/7
agent: 2/7
(+59 other subsystem(s) with fewer)
⚠️ main took 4 of the last 7 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## CI: could not check recent failures (YOYO_REPO unset) — this is NOT a clean bill of health

## Provider/API health: not checked — YOYO_AUDIT_DIR is unset, so there is no audit-log to scan (this is normal for a hand-run).

## Usage records: not checked — YOYO_AUDIT_DIR is unset, so there is no audit-log to scan (normal for a hand-run). Not a clean bill.

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/prompt.rs is 3888 lines vs its recorded 3792 (+96 drift) — 4 more line(s) makes it FATAL. Fix: paste ("src/prompt.rs", 3888) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 146 day(s) old (repo is day 220) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity: not checked — no claimed successes to read, or no task commits found in the git window at all. This is a REFUSAL, not 'every claimed success landed'.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/dispatch_sub_project_tests.rs (3.0) — predicted 7×, never graded; stale (27 snapsho…
- src/prompt/turn_prefix.rs (2.9) — predicted 3×, never graded; stale (25 snapshots)
- src/format/cost/price_audit_tests.rs (1.2) — stale (44 snapshots)
- never forecast (0 predictions ever, unranked): src/prompt/retry_after_partial.rs, src/hard_deny.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
