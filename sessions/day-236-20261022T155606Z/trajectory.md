# YOUR TRAJECTORY

Last computed: 2026-10-22T14:56Z. Day ?. Window: last 10 sessions / 14 days.

## Per-task activity (last 14 days)
"Part of #1068: bash guard refusals count as deterministic re…": 2 attempt(s), last day-236
"Part of #1014: /spawn write tap, first cut — count paths a w…": 2 attempt(s), last day-236
"Part of #1064: key presence reads yoagent's resolver, so `GE…": 2 attempt(s), last day-235
"Fixes #1011: with a deny list set, refuse eval of a runtime-…": 2 attempt(s), last day-235
"Part of #1064: opt-in YOYO_SUBPROCESS_ENV_SCRUB=1 strips pro…": 2 attempt(s), last day-235
"Part of #944: social.sh writes its per-run spend row to the …": 1 attempt(s), last day-236

## Subsystem concentration (last 8 self-driven task commits)
main: 4/8
cli: 3/8
git: 3/8
providers: 3/8
tools: 3/8
(+79 other subsystem(s) with fewer)
⚠️ main took 4 of the last 8 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## CI: no failed runs in last 14 days

## Provider/API health: not checked — YOYO_AUDIT_DIR is unset, so there is no audit-log to scan (this is normal for a hand-run).

## Usage records: not checked — YOYO_AUDIT_DIR is unset, so there is no audit-log to scan (normal for a hand-run). Not a clean bill.

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/config.rs is 4851 lines vs its recorded 4762 (+89 drift) — 11 more line(s) makes it FATAL. Fix: paste ("src/config.rs", 4851) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 162 day(s) old (repo is day 236) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity: not checked — no claimed successes to read, or no task commits found in the git window at all. This is a REFUSAL, not 'every claimed success landed'.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.4) — predicted 3×, never graded; stale (91 snapshots)
- src/commands_risk_unhittable.rs (1.4) — stale (84 snapshots)
- src/stream_leading_blank.rs (1.4) — stale (84 snapshots)
- never forecast (0 predictions ever, unranked): src/spawn_worktree.rs, src/grep_status.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
