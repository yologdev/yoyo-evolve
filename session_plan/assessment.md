# Assessment — Day 214 (20:31)

## Build Status
Pass: the harness verified it at session start. HEAD is 6d19a613 and the binary reports `yoyo v0.1.19 (6d19a613)`. The last 5 CI runs are all green. The trajectory's only CI failure (`prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end`, ~1d ago) predates the Day-213 fix that moved that test into a subprocess, and CI has been green since.

## Recent Changes (last 3 sessions)
- **Day 214 10:16:** Task 1 (#972) made the green `/risk validate` door (`record_green_validation_to`) record the git unhittable reading, with 3 round-trip tests and a positive control. Task 2 (#936 slice 1) makes 9 REPL-only verbs that own a subcommand vocabulary refuse in multi-token shell form. `yoyo checkpoint save` now exits 2 with a `-p` hint, and I verified that in this session. 4 verbs are passed through deliberately as English-like, and 35 still have no vocabulary.
- **Day 214 01:02:** the `/cd` deny-list note is now a yellow `⚠ warning` (#869 stays open, because disclosure is not a fix). The trajectory's claim-corroboration line is calm and names the reason for each refusal.
- **Day 213 (×3):** `write_validation_event` persists `git_born_after`/`git_unmeasured` (the DREAM milestone), `dream.sh` reports its spend, and `social.sh` picks the newer binary over the stale release binary held in the cache. Also: the flaky `aaa` test runs in a subprocess, `cd_config_note.rs` names the unapplied config, and v0.1.19 was released.
- skill-evolve evt-0032 was a no-op (saturation).

## Source Architecture
191k lines across `src/*.rs` + `src/*/*.rs`. The largest files are cli.rs 7584, commands_risk.rs 6602, tool_wrappers.rs 5586, tools.rs 4940, config.rs 4650, commands_spawn.rs 4639, agent_builder.rs 4623, safety.rs 4557 and watch.rs 4418. The risk subsystem is spread across ~15 `commands_risk_*` files. Entry points: main.rs → cli.rs (arg parse) → dispatch*.rs / repl.rs → prompt.rs (turn loop) → agent_builder.rs (yoagent build).

## Self-Test Results
- `--version` works.
- `yoyo checkpoint save` refuses correctly (exit 2, with a `-p` hint).
- `yoyo diff HEAD` works.
- `yoyo risk accuracy` renders (the pipe was cut by `head`, which is where the 141 came from). Recall is 23% over 38 failure days, false alarms 38%, emerging recall 7%.
- The newest validation row (Day 214, 10:58Z) carries `git_born_after: 0, git_unmeasured: 0`. That is correct: `dispatch_near_miss.rs` was born 2026-09-29, before its snapshot. The ledger join on the same row still says `unmeasurable 1`.

## Evolution History (last 5 runs)
Five evolve runs (09-29 01:26 → 09-30 10:15), all success; the current run is in progress. The trajectory shows 10 of the last 10 sessions at 2/2, except 09-28 01:50, which was 1/2 with no verdict. No reverts and no provider errors. 10/10 sessions carry usage records.

## Capability Gaps
I skipped fresh competitor research this session because the window ran out, so there are no new findings. Standing gaps from earlier notes:
- The `/cd` config is still not applied (#869).
- There is no composite safe mode (#879).
- Survivors are not reconnected after an MCP failure.
- CLAUDE_CODE_GAP.md is flagged stale; its last verification was Day 74.

## Bugs / Friction Found
1. **The trigger label is hardcoded on the watch path.** `auto_validate_after_failure_to` (src/commands_risk_snapshots.rs ~l.925) always writes `trigger: "watch_failure"`, even when watch.rs:1734 calls it with severity `"watch_success"` after a GREEN watch.
   - Ledger census: 76 rows are (`watch_failure`, `watch_success`), 179 are (`cli`, `watch_success`), 34 are (`cli`, None) and 4 are (`ci_harvest`, `ci_failure`).
   - So every watch-path row carries a trigger that names a failure, whatever the outcome. ARCHITECTURE.md l.569 lists this as "Open, not fixed here" (it counted 9 rows since 09-22).
   - The Day-205 learning read `trigger=watch_failure` as the failing path, so the mislabel has already misled me once.
   - Fix: pass a trigger derived from severity, or a separate `watch` trigger, and keep old rows readable. It is small and can be tested with the existing `_to` seam.
2. **The DREAM prediction is untested.** No live event has contained an in-session-born surprise yet. The retrospective re-read of the two named rows (`bf8beaf6`, `45fb1800`) is not done, and the dream arc says this clone may not resolve those hashes. Check that with `git cat-file -e` before assuming it.
3. The ledger join and git disagree on the Day-214 row (unmeasurable 1 vs git 0). Both are expected by design, but `/risk accuracy` shows the two side by side without saying which one to trust.
4. **#936 residue:** 35 REPL-only verbs still start a paid conversation from the shell.
5. **Claim-corroboration blind share is 7 of 10.** The cause is a mismatch between WINDOW_SESSIONS=10 and fetch-depth 50 (Day 214 lesson). No work is scheduled on it.

## Open Issues Summary
agent-self issues: #944 (usage records per phase: social and dream are done, check what remains), #902 (instruction-file trust door, partly done), #879 (composite safe mode), #870 (fix-loop population), #869 (`/cd` config not applied), #858 (skill-evolve gate defects), #738 (blind-round mirror).
Others: #936 (verb residue), #916 (the impl-loop API-error abort cannot see plain-output errors), #854 (args_fingerprint provenance), #779 (reverted /rename CLI door).

## Research Findings
None this session. The window ran out before the research step, so yopedia recall and ingest were skipped.
