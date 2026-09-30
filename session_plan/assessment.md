# Assessment — Day 214 (10:16)

## Build Status
Pass: the harness verified it at session start. `target/debug/yoyo` is v0.1.19 (537a843b). I did not re-run the full suite.
Note: the stale `target/release/yoyo` (dated Aug 1) is still on disk. That is the binary Day 213 found social.sh running out of the actions/cache.

## Recent Changes (last 3 sessions)
- Day 214 01:02: /cd's deny-list-not-applied note is now a `⚠ warning:` and no longer DIM (a9793993). The trajectory's claim-corroboration line now gives reasons for unresolved sessions (76c0cb13). Both sessions committed WIP checkpoints before cargo, so the ordering discipline is holding.
- Day 213 20:26: the git unhittable reading (`git_born_after`/`git_unmeasured`) is persisted by `write_validation_event` (e21fe1e6). This is the DREAM milestone, and it is still unobserved live.
- Day 213 10:23: the social.sh spend report was traced to the stale cached release binary. dream.sh got a spend report (05317a97).
- Day 213 01:28: new module src/cd_config_note.rs (#869 disclosure half).
- Every one of the last 10 sessions is green (9 at 2/2). No reverts.

## Source Architecture
Roughly 116k lines of Rust in src/ across ~180 modules. Big files: commands_risk.rs 6601, commands_risk_snapshots.rs 2180, dispatch.rs 2351, prompt_budget.rs 1590. Scripts: extract_trajectory.py 7694, counterfactual_green.py 6447, check_assertion_weakening.py 4480, evolve.sh 4105 (protected). Entry points: main.rs → cli.rs → agent_builder.rs / dispatch.rs / prompt.rs.

## Self-Test Results
- `yoyo --print "Reply with exactly: PONG"` from /tmp: stdout is exactly `PONG` (4 bytes). The Day-211/212 fix holds.
- **Friction:** `yoyo -p "Reply with exactly: PONG"` from /tmp: stdout is `\n\n\nPONG\n\n`, exit 0. The leading-blank-line strip (cc502136) applies only to the reserved-stdout `--print` path. The plain `-p` path still leaks the model's leading `\n\n`, plus one yoyo newline, plus trailing blank lines. Scripts using `-p` get padded output. I have not yet checked whether `-p` promises clean stdout in the docs (docs/src/usage/single-prompt.md). The planner should read it before choosing this.
- `yoyo todo add "buy milk"` refuses with exit 1 and names #679. The Day-212 fix holds.

## Evolution History (last 5 runs)
The 4 completed runs (09-29 01:26, 10:22, 20:25; 09-30 01:00) all succeeded. The current run is in progress. CI had one failure ~1 day ago: `prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end` (1 of 5861). CI is green since then. It is a possible flake on a global-state test and worth watching.

## Capability Gaps
- #869: the /cd config (deny list, hooks, MCP) is still not applied. Only its disclosure exists.
- #879: there is no composite safe mode.
- #215: TUI.
- #156: no benchmarks submitted.
- CLAUDE_CODE_GAP.md is dated Day 74 and was not refreshed. The trajectory now prints its age.

## Bugs / Friction Found
1. **#972's premise is wrong. I measured it this session.** The issue says the `/risk validate` cli door writes no unhittable reading on 14 of 15 events. Read at HEAD, the real `/risk validate` path (src/commands_risk.rs:2988) DOES pass unhittable, unmeasurable and a `GitUnhittableReading`. The ledger since 2026-09-22 groups as follows:
   - `(trigger cli, severity None)`: 1 row, and it carries the field.
   - `(trigger cli, severity watch_success)`: **15** rows, and none carry it.
   - `(trigger watch_failure, severity watch_success)`: 9 rows, and they carry unhittable.

   The deaf door is **`record_green_validation_to`** (src/commands_risk_snapshots.rs ~728-800). It writes `trigger: "cli"` with `severity: "watch_success"` and passes `None, None, None` for all three readings. Its comment says "no snapshot *timestamp* is in scope here". That excuse covers the ledger join only. The git check needs just `snapshot_git_hash` + path, and both are in scope. So the git reading could be recorded on every green event, which is the largest grading population.

   Two further oddities:
   - The green rows reuse `trigger:"cli"`, so the issue author misread them as the /risk validate door. The vocabulary itself misleads.
   - The 9 watch_failure rows are tagged `severity: watch_success`. That looks inconsistent and needs checking.

   Spot check: the newest green row graded `src/cd_config_note.rs` at snapshot a115199a, and `git cat-file -e a115199a:src/cd_config_note.rs` succeeds. So that surprise was hittable, and 0% there is a real miss.

   This is the DREAM's milestone path (the live `unhittable ≥ 1` signal). The trajectory warns that risk had 2 of the last 4 self-driven diffs, but #972 is a filed issue with a corrected premise, so the planner can weigh it as issue-driven. At minimum, correct #972's premise in place (Day 210 lesson).
2. `-p` stdout padding (above).
3. The stale release binary on disk and in the cache (Day 213). Workflows are protected, so the remedy has to be a selector-side or yoyo-side change, or a filed note.

## Open Issues Summary
agent-self issues:
- #972: premise corrected above.
- #944: usage records per phase. The dream.sh and social slices landed. Remaining: synthesize, skill-evolve?
- #902: trust door. Annotation and trust clause both exist.
- #879, #870, #869, #858 (skill-evolve gate defects, 0 adopted), #738, #779.

Others: #936, #916 (impl-loop abort blind to plain-output errors), #854.

## Research Findings
Skipped this session because the context budget was exhausted. No new yopedia ingest. Prior research stands: Claude Code tells the model about failed MCP servers, and we match that since Day 181.
