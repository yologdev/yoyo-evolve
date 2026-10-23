# YOUR TRAJECTORY

Last computed: 2026-10-23T15:17Z. Day ?. Window: last 10 sessions / 14 days.

## Per-task activity (last 14 days)
"Part of #1072: one shared branch reader that names an unborn…": 2 attempt(s), last day-237
"Part of #1021: drop gemini-3.0-pro/flash (absent from Google…": 2 attempt(s), last day-237
"Part of #1063: /watch and the auto-check edit hook stop wait…": 2 attempt(s), last day-237
"Part of #1062: the 401 hint names where the key actually cam…": 2 attempt(s), last day-236
"Part of #1068: bash guard refusals count as deterministic re…": 2 attempt(s), last day-236
"Fixes #944: the phase-spend floor line stops blaming a close…": 1 attempt(s), last day-237

## Subsystem concentration (last 5 self-driven task commits)
main: 2/5
prompt: 2/5
bash_refusal: 1/5
cli: 1/5
commands: 1/5
(+7 other subsystem(s) with fewer)

## CI: could not check recent failures (YOYO_REPO unset) — this is NOT a clean bill of health

## Provider/API health: not checked — YOYO_AUDIT_DIR is unset, so there is no audit-log to scan (this is normal for a hand-run).

## Usage records: not checked — YOYO_AUDIT_DIR is unset, so there is no audit-log to scan (normal for a hand-run). Not a clean bill.

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: could not check (no audit-log worktree (YOYO_AUDIT_DIR)) — NOT zero spend.
- dream: could not check (no audit-log worktree (YOYO_AUDIT_DIR)) — NOT zero spend.
Totals are a floor: these sums omit sub-agent spend (#1073).

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/config.rs is 4851 lines vs its recorded 4762 (+89 drift) — 11 more line(s) makes it FATAL. Fix: paste ("src/config.rs", 4851) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 163 day(s) old (repo is day 237) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity: not checked — no claimed successes to read, or no task commits found in the git window at all. This is a REFUSAL, not 'every claimed success landed'.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.5) — predicted 3×, never graded; stale (95 snapshots)
- src/commands_risk_unhittable.rs (1.4) — stale (88 snapshots)
- src/stream_leading_blank.rs (1.4) — stale (88 snapshots)
- never forecast (0 predictions ever, unranked): src/grep_status.rs, src/hard_deny.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
