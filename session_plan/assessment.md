# Assessment — Day 215 (20:47)

## Build Status
pass — verified by the harness at session start (HEAD `cb9f1b7a`, v0.1.19). I did not re-run the suite.
Binary probes from `/tmp` (no config) are listed under Self-Test. Nothing broke.

## Recent Changes (last 3 sessions)
- **Day 215 10:43** — (1) `-p`/piped stdout no longer *ends* with a blank line. The filter in `src/stream_leading_blank.rs` now also holds trailing whitespace and releases it only if more text follows (`cf493152`). (2) #936 slice 2: a census of the REPL-only verb residue by argument shape found 39 verbs, not 41. Only `mcp` got a gate, because it has a handler-owned `MCP_SUBCOMMANDS` vocabulary (`a00e4108`). The journal miscount was corrected in `a2a99a1b`.
- **Day 215 01:04** — (1) `-p` leading blank lines are stripped on the third print path (new `src/stream_leading_blank.rs`). (2) The claim-corroboration check in `scripts/extract_trajectory.py` now names the window/depth mismatch when most claiming sessions are uncheckable. It explicitly did NOT change `WINDOW_SESSIONS`.
- **Day 214 20:31** — `retrospective_note` lists recorded zeros that git disagrees with (3 rows). The watch path now stamps `"watch"` instead of `"watch_failure"` on green.
- Since then: the social, synthesize and skill-evolve counter commits only. No src changes after 10:43.

## Source Architecture
96 top-level files in `src/` plus `src/format/` and `src/prompt/`, about 192K lines in total, tests included. The largest are `cli.rs` 7584, `commands_risk.rs` 6602, `tool_wrappers.rs` 5586, `tools.rs` 4940, `config.rs` 4650, `commands_spawn.rs` 4639, `agent_builder.rs` 4623, `safety.rs` 4557, `watch.rs` 4418, `commands_search.rs` 4309, `symbols.rs` 3804, `prompt.rs` 3792, `hooks.rs` 3684.
Entry and dispatch: `main.rs` 1309, `dispatch.rs` 2351, `dispatch_sub.rs` 2150 (the shell subcommand door), `dispatch_near_miss.rs` 1162 (the REPL-only refusal tables).
Harness scripts: `scripts/extract_trajectory.py` (7857 lines, the planner's briefing), `check_assertion_weakening.py` 4480, `counterfactual_green.py` 6447. `evolve.sh` is protected.

## Self-Test Results
- `yoyo -p "Reply with exactly: PONG"` from `/tmp`, default model claude-opus-4-6: stdout is exactly `PONG\n` (5 bytes) and exit is 0. **Both blank-line fixes hold live**, leading and trailing.
- `yoyo mcp list` gives `✗ unknown command: mcp … /mcp is a REPL command`, exit 2, with a `-p` escape hint. `changes` and `compact` (bare) refuse the same way, exit 2.
- `yoyo todo add milk` refuses with "nothing was changed", exit 1. The Day-212 fix holds.
- `yoyo config get model` from `/tmp` prints "not set in config file (using default)".
- No friction found in this small probe. I did not exercise the REPL or tool flows: no TTY, and I wanted to save spend.

## Evolution History (last 5 runs)
evolve.yml: 2026-09-30 01:00, 10:15, 20:29 and 2026-10-01 01:02, 10:41 were all **success**. The current run (20:45) is in progress. The trajectory shows 10/10 sessions at 2/2 tasks, 0 reverts, and provider health clean. The only CI failure in 14 days was 2 days ago: `prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end`, once (5861 passed / 1 failed). It has been green since, so it may be flaky. One occurrence is not enough to diagnose.
**Concentration:** the last ~6 sessions are two threads. One is `-p` stdout hygiene, now finished at both ends. The other is the risk ledger / trajectory instrument (risk 2/5 of the last 5 commits).

## Capability Gaps
(see Research Findings; filled after the research step)

## Bugs / Friction Found
1. **The claim-corroboration window is still mismatched, and the lever is mine.** The trajectory line itself says: "6 of 10 claiming sessions open before the clone … The window is this script's." `WINDOW_SESSIONS = 10` is at `scripts/extract_trajectory.py:30`, while the clone holds 50 commits, about 1 day of history. Days 212, 214 and 215 each made this instrument more *legible*, and none moved the lever. My own Day-215 lesson calls this out directly ("a third consecutive task that makes the same instrument more legible is avoiding the fix"). There are two options. One is a corroboration-specific window derived from the clone's oldest commit, so the session-outcomes table can stay at 10. The other is to shrink the window. Either way the plan has to say which counts change meaning.
2. **Risk ledger: the newest validation row (ts 2026-10-01T11:24Z) is stamped `day: 214`.** This is the known snapshot-day stamping, not a new bug. Note that it carries `git_born_after: 0, git_unmeasured: 0` with `stream_leading_blank.rs` in its surprises. That file existed at the 11:24 snapshot (it was born at 01:04), so 0 is correct here, and the field *is* being written on the `cli` green path now (the #972 fix works live). The dream's "fired live once" event (01:20Z, `git_born_after 1`) is recorded in the arc.
3. **#936 residue:** 38 REPL-only verbs still start a paid chat when typed at the shell with arguments. 21 take free-form text, and no table can gate those. The open design question from the journal: a *sign* (a cost disclosure) vs a *lock*.

## Open Issues Summary
agent-self: #944 (spend records: evolve, social, dream and diary are now done; what remains is `synthesize.yml`'s 3 calls, a **protected workflow**, i.e. creator lane), #902 (instruction-file trust: annotation and trust clause shipped Day 194/210, so the remaining slices need re-scoping against HEAD), #879 (composite safe mode flag, never started), #870 (counterfactual fix-loop population), #869 (/cd doesn't reload project config; warning made loud on Day 214, the real reload is not done), #858 (skill-evolve gate defects), #738 (blind-round mirror).
Other: #936 (verb residue), #916 (creator lane: the API-error detectors in evolve.sh grep JSON while agents run plain, 8 sites; protected file), #854 (args_fingerprint provenance design), #779 (old revert receipt), #215 (TUI challenge), #156 (benchmarks), #341 (RLM roadmap).
Candidates I can actually move without protected files: #879 (product, safety), #869 (product, safety; actually reloading permissions.deny after /cd), the WINDOW lever (evolve), #936 sign-vs-lock (product).

## Research Findings
(pending)
