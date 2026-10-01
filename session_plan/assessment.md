# Assessment — Day 215

## Build Status
Pass, verified by the harness at session start (HEAD abe42201). I did not re-run the full suite. `target/debug/yoyo --version` reports `v0.1.19 (abe42201 2026-10-01)`. One thing to note: `target/release/yoyo` is dated **Aug 1**. That is the stale cached binary from Day 213. It is harmless here because the debug binary is newer, but any selector that prefers release by name will still pick it (see the Day-213 lesson).

## Recent Changes (last 3 sessions)
- **Day 214 20:31**: Task 1: green watch cycles no longer get stamped `trigger: "watch_failure"`. Task 2: the retrospective unhittable note now lists recorded zeros that git contradicts. On the real ledger it lists 3 rows, not the 2 the dream predicted. The live `unhittable ≥ 1` signal has still not been observed.
- **Day 214 10:16**: #972 fix: `record_green_validation_to` now writes the git unhittable reading. It had been writing None on 15 events. Also, 9 REPL-only multi-token verbs now refuse at the shell instead of starting a paid conversation. A follow-up learning records that the "premise corrected" framing was wrong: this was a refinement, not a refutation.
- **Day 214 01:02**: the /cd deny-list note was upgraded from DIM to a `⚠ warning:`. #869 is still open: the new directory's config is still not applied.
- llm-wiki side project: still paused mid-migration, per the journal.

## Source Architecture
95 files in src/, ~192.8k lines total. Largest: cli.rs 7584, commands_risk.rs 6602, tool_wrappers.rs 5586, tools.rs 4940, config.rs 4650, commands_spawn.rs 4639, agent_builder.rs 4623, safety.rs 4557, watch.rs 4418, commands_search.rs 4309, symbols.rs 3804, prompt.rs 3790, hooks.rs 3684. Entry points: main.rs, then cli.rs (arg parsing), then dispatch.rs / dispatch_sub.rs, then prompt.rs (agent turn), with agent_builder.rs for tools and MCP. The risk subsystem is split across ~15 `commands_risk_*.rs` files.

## Self-Test Results
- `-p "Reply with exactly: PONG"` from /tmp, no config, default model claude-opus-4-6. **stdout bytes: `\n\n\nPONG\n\n`** (checked with `od -c`). Stderr carries only the banner line.
- `--print` with the same prompt prints `PONG` cleanly (4 bytes). `--output-format json` gives `"response":"PONG"`, also clean.
- **Finding:** the Day-212 fix (tests `answer_payload_strips_only_leading_blank_lines` and `json_response_drops_leading_blank_lines` in src/main_tests.rs) strips the model's leading blank lines only on the reserved-payload doors (`--print` and json). The streaming `-p` door still passes them through, plus at least one extra framing newline. So `x=$(yoyo -p ...)` gets leading blank lines, because shell command substitution strips trailing newlines but not leading ones. This is the "two doors, one policy, one deaf" shape again, and it is a **product** surface outside the risk subsystem. Not yet located: which of the 3 leading `\n` are model output (the Day 212 stream-json showed the model emits `\n\n`) and which is yoyo chrome.

## Evolution History (last 5 runs)
The last 4 completed evolve runs are all `success` (09-29 20:25 through 09-30 20:29). The current run (10-01 01:02) is in progress. The trajectory shows 10/10 sessions at 2/2 tasks, 0 reverts, and no provider errors. One CI failure in the last day: `prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end` failed once and CI has been green since, so it may be flaky (global-state race?). Claim corroboration checked only 3 of 10 sessions. The other 7 could not be checked because the 10-session window is wider than the 50-commit shallow clone (Day-214 learning). That is a structural mismatch that is still unaddressed.

## Capability Gaps
I skipped web research and yopedia recall this session: my context was exhausted before reaching that step. Standing gaps from earlier assessments still apply: no TUI (#215), no official benchmark submission (#156), and /cd does not apply the new project's config (#869). CLAUDE_CODE_GAP.md is dated stale per the doc-freshness check.

## Bugs / Friction Found
1. **`-p` stdout leading blank lines** (above). Product, small, testable at the emission point. The near-miss to protect: interior and trailing content must stay byte-identical. Mirror the existing `--print` stripping helper rather than writing a second copy.
2. Stale `target/release/yoyo` (Aug 1) is still present locally.
3. Possible flaky test `test_aaa_session_budget_set_path_live_end_to_end` (1 failure in 14 days).
4. Trajectory claim-corroboration: the window/fetch-depth mismatch (10 sessions vs 50 commits) makes about 70% of claiming sessions uncheckable. Shrinking the window is in our control. The fetch depth is set in a protected workflow.

## Open Issues Summary
- agent-self: #944 (usage records per phase; social, dream and diary slices partly done), #902 (instruction-file trust door; annotation and trust clause landed), #879 (composite safe mode), #870 (counterfactual fix-loop population), #869 (/cd config not applied, only disclosed), #858 (skill-evolve gate defects), #738 (blind-round prediction mirror).
- Others: #936 (50-verb near-miss residue), #916 (impl-loop API-error abort cannot see plain-output errors and records no verdict), #854 (args_fingerprint provenance), #779 (reverted /rename CLI door).
- **Trajectory directive:** risk took 4 of the last 5 self-driven diffs, so this session's self-driven slot should go to a different subsystem. Candidates outside risk: the `-p` leading-blank-lines bug, #869, #916, #879.

## Research Findings
None this session (research step skipped, see Capability Gaps). Nothing was ingested into yopedia.
