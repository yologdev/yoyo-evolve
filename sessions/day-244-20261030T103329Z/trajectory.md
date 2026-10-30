# YOUR TRAJECTORY

Last computed: 2026-10-30T10:32Z. Day 244. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-244 (2026-10-30 08:39:31): tasks 2/2 ✅ — build OK, tests OK
day-244 (2026-10-30 02:57:33): tasks 2/2 ✅ — build OK, tests OK
day-243 (2026-10-29 22:23:56): tasks 2/2 ✅ — build OK, tests OK
day-243 (2026-10-29 12:23:26): tasks 2/2 ✅ — build OK, tests OK
day-243 (2026-10-29 02:45:29): tasks 2/2 ✅ — build OK, tests OK
day-242 (2026-10-28 21:55:22): tasks 2/2 ✅ — build OK, tests OK
day-242 (2026-10-28 18:12:25): tasks 2/2 ✅ — build OK, tests OK
day-242 (2026-10-28 16:03:55): tasks 2/2 ✅ — build OK, tests OK
day-242 (2026-10-28 13:06:05): tasks 2/2 ✅ — build OK, tests OK
day-242 (2026-10-28 11:53:00): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 4 checkable claiming sessions claimed success with no task commits.
6 further claiming session(s) could NOT be checked — NOT counted above: 5 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.
Shape, not a per-run accident: 5 of 10 claiming sessions open before the clone. Window = last 10 sessions; clone = 55 commits, oldest 2026-10-29 02:11 UTC; 4 of the 10 sessions read open their window after it. The window is this script's; the fetch depth is set in .github/workflows/evolve.yml (protected).

## Per-task activity (last 14 days)
"A global flag before a subcommand no longer turns the subcom…": 2 attempt(s), last day-244
"Fixes #1103: refuse a mistyped --provider at startup (exit 2…": 2 attempt(s), last day-244
"Fixes #1101: hard_deny tracks quote state across lines, so a…": 2 attempt(s), last day-243
"Mark the four multi-block tool-result sites as reaching the …": 2 attempt(s), last day-243
"Fixes #1104: the trajectory cap fits the whole block, and an…": 1 attempt(s), last day-244
"Fixes #1071: count synthesize in the trajectory's phase spen…": 1 attempt(s), last day-244

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 10 self-driven task commits)
dispatch: 5/10
cli: 3/10
goal: 3/10
hard: 3/10
tools: 3/10
(+87 other subsystem(s) with fewer)
⚠️ dispatch took 5 of the last 10 self-driven diffs — send this session's self-driven slot to a different subsystem; file the in-zone idea instead.

## CI: could not check recent failures (YOYO_REPO unset) — this is NOT a clean bill of health

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 27 run(s), $34.66 summed cost_usd.
- dream: 1 run(s), $2.27 summed cost_usd.
- synthesize: 5 run(s), $7.27 summed cost_usd; 4 usage record(s) without cost_usd.
Totals are a floor: these sums omit sub-agent spend (#1073).

## Module sizes (the size gate warns but only fails on the exit code)
src/commands_risk_unhittable.rs is 2036 lines, 36 past the 2000-line cap and UNLISTED — 14 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/commands_risk_unhittable.rs", 2036) to GRANDFATHERED_OVERSIZED_MODULES.
src/cli.rs is 7869 lines vs its recorded 7774 (+95 drift) — 5 more line(s) makes it FATAL. Fix: paste ("src/cli.rs", 7869) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 170 day(s) old (repo is day 244) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-242 claimed 10 success(es), but the 16 task commit(s) in this window carry day-243..day-244 (2 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.6) — predicted 3×, never graded; stale (123 snapshots)
- src/commands_bg_kill_tests.rs (2.8) — predicted 8×, never graded; stale (15 snapshots)
- src/commands_risk_unhittable.rs (1.5) — stale (116 snapshots)
- never forecast (0 predictions ever, unranked): src/grep_status.rs, src/tools_search_flag_tests.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
