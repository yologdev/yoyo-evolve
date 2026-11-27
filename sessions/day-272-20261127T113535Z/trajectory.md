# YOUR TRAJECTORY

Last computed: 2026-11-27T09:57Z. Day 272. Window: last 10 sessions / 14 days.

## Recent session outcomes (last 10)
day-272 (2026-11-27 01:32:06): tasks 2/2 ✅ — build OK, tests OK
day-271 (2026-11-26 15:47:47): tasks 2/2 ✅ — build OK, tests OK
day-271 (2026-11-26 01:38:44): tasks 2/2 ✅ — build OK, tests OK
day-270 (2026-11-25 15:33:47): tasks 1/1 ✅ — build OK, tests OK
day-270 (2026-11-25 02:27:10): tasks 2/2 ✅ — build OK, tests OK
day-269 (2026-11-24 15:58:22): tasks 2/2 ✅ — build OK, tests OK
day-269 (2026-11-24 04:16:17): tasks 2/2 ✅ — build OK, tests OK
day-268 (2026-11-23 15:56:46): tasks 2/2 ✅ — build OK, tests OK
day-268 (2026-11-23 03:58:19): tasks 2/2 ✅ — build OK, tests OK
day-267 (2026-11-22 23:26:19): tasks 2/2 ✅ — build OK, tests OK
Claim corroboration: 0 of 4 checkable claiming sessions claimed success with no task commits.
6 further claiming session(s) could NOT be checked — NOT counted above: 5 window opens before this shallow clone's oldest commit; 1 oldest session read, no predecessor to open its window.
Shape, not a per-run accident: 5 of 10 claiming sessions open before the clone. Window = last 10 sessions; clone = 50 commits, oldest 2026-11-24 15:58 UTC; 4 of the 10 sessions read open their window after it. The window is this script's; the fetch depth is set in .github/workflows/evolve.yml (protected).

## Per-task activity (last 14 days)
"Fixes #1247: a hook that writes more than ~64 KiB to stderr …": 2 attempt(s), last day-272
"Fixes #1242: /doctor gets an API row that says whether the c…": 2 attempt(s), last day-270
"Fixes #1245: a /spawn worker's sub-agent judges rm of its wo…": 1 attempt(s), last day-272
"Fixes #1247: clippy: drop a redundant borrow in the drain te…": 1 attempt(s), last day-272
"Fixes #1229: a recursive delete of the working folder (or an…": 1 attempt(s), last day-271
"Part of #1202: step 2 (allow rules on the tree), an allow ru…": 1 attempt(s), last day-271

## Reverts in window
0 task reverts in last ~10 sessions, 0 whole-session revert commits in 14 days.

## Subsystem concentration (last 11 self-driven task commits)
doctor: 3/11
tools: 3/11
bash_parse: 2/11
dev: 2/11
dispatch: 2/11
(+17 other subsystem(s) with fewer)

## Recurring CI errors (failed runs, last 14 days) (4 older failure(s) outside the window, not shown)
CI has gone green since (last <1d ago): every failure below predates it. Not proof the causes are fixed — a flaky test passes sometimes — only that CI is not red on these patterns now.
[1×, last 13d ago] ^[[1m^[[91merror^[[0m^[[1m: this `if` guard returns a bool literal and is follow
[1×, last 13d ago] ^[[1m^[[94m712^[[0m ^[[1m^[[94m|^[[0m ^[[1m^[[91m/^[[0m if t.contains("error ts"
[1×, last 13d ago] ^[[1m^[[94m|^[[0m ^[[1m^[[91m|_________^^[[0m ^[[1m^[[91mhelp: you can reduce it
[1×, last 13d ago] ^[[1m^[[91merror^[[0m: could not compile `yoyo-agent` (bin "yoyo") due to 3 prev
[1×, last 13d ago] ^[[1m^[[33mwarning^[[0m: build failed, waiting for other jobs to finish...

## Provider/API health
10 sessions, no provider-error lines in audit.jsonl, transcripts/*.log.

## Usage records
10 of 10 sessions carry >=1 usage record (#848 channel is live).

## Phase spend (audit-log phases/*/spend.jsonl, last 7 days)
- social: 16 run(s), $18.70 summed cost_usd.
- dream: 1 run(s), $2.35 summed cost_usd.
- synthesize: 7 run(s), $7.65 summed cost_usd; $1.68 of it sub-agent spend.
Sums include sub-agent spend; any gap listed above makes a sum a floor.

## Module sizes (the size gate warns but only fails on the exit code)
src/context.rs is 2035 lines, 35 past the 2000-line cap and UNLISTED — 15 more line(s) makes `cargo test` FATAL, which reverts the whole task. Fix: split it, or add ("src/context.rs", 2035) to GRANDFATHERED_OVERSIZED_MODULES.
src/format/mod.rs is 2874 lines vs its recorded 2788 (+86 drift) — 14 more line(s) makes it FATAL. Fix: paste ("src/format/mod.rs", 2874) over its entry.

## Doc freshness (the header, not the body)
CLAUDE_CODE_GAP.md: header verified day-74, 198 day(s) old (repo is day 272) — STALE, past the 30-day threshold. The body has NOT been re-verified; re-read a row before trusting it.

## Productivity
OUT OF RANGE: day-267, day-268, day-269 claimed 10 success(es), but the 12 task commit(s) in this window carry day-270..day-272 (3 distinct) — the git slice never spanned that day, so this is NOT a finding about its productivity.

## Counterfactual pairing
6 of 6 UNEARNED rows paired, 1 PAIR_SIGNAL (src+tests 2/2, tests 4/4). Says pairing RAN, never that a verdict is RIGHT.

## Epistemic blind spots (files graded outcomes have taught the model least about)
- src/prompt/turn_prefix.rs (3.7) — predicted 3×, never graded; stale (213 snapshots)
- src/commands_bg_kill_tests.rs (3.5) — predicted 8×, never graded; stale (105 snapshots)
- src/format/cost/bedrock_region.rs (3.3) — predicted 3×, never graded; stale (70 snapsho…
- never forecast (0 predictions ever, unranked): src/tools_child_bash_tests.rs, src/bash_refusal_tests.rs (+3 more)
(planner hint: point the self-driven slot at one of these — the never-forecast files are the darkest, the ranking cannot see them — guess first, grade after)
