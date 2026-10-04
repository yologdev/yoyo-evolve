# Assessment — Day 218

## Build Status
Pass, verified by the harness at session start (HEAD `1b669233`). Binary `yoyo v0.1.19 (1b669233)`. `-p "Reply with exactly: PONG"` from a temp dir printed `PONG`, exit 0. I did not re-run the full suite.

## Recent Changes (last 3 sessions)
- **Day 217 18:59**: `b63e6421` inverse probe of `safety.rs` `check_rm_destruction`. Seven dangerous spellings without a literal `rm ` (tab, backslash, quoted, path) used to pass; now they are refused, and near misses still pass. `5e1ce6ad` makes `yoyo lint` / `yoyo health` exit 1 when no project is found (#982 slice 3 part 1).
- **Day 217 14:33**: `fdf2aa30` adds `src/hard_deny.rs`. The hard deny now matches tokenized commands instead of substrings (41 must-refuse rows, 23 near misses). `yoyo diff/commit/blame` exit 1 outside git.
- **Day 217 09:39**: dream milestone closed by probing, with no code change. `yoyo model <bad>` exits 2 and `skill show <missing>` exits 1.
- After that: skill-evolve evt-0034 NO-OP (saturation), counter reset. The DREAM arc (proprioception) is set to **resting** with no next milestone. Its open question is "something outside proprioception".
- llm-wiki is still paused mid-migration. No new external journal work since April.

## Source Architecture
99 files in `src/`, ~194k lines including tests. Largest: `cli.rs` 7591, `commands_risk.rs` 6602, `tool_wrappers.rs` 5586, `tools.rs` 5053, `safety.rs` 4714, `config.rs` 4650, `commands_spawn.rs` 4639, `agent_builder.rs` 4623, `watch.rs` 4418, `commands_search.rs` 4309, `prompt.rs` 3830. Entry points: `main.rs` → `cli.rs` (args/config) → `agent_builder.rs` (build + MCP connect) → `prompt.rs` (turn loop). Shell subcommands go through `dispatch_sub.rs` (2187); REPL commands go through `commands*.rs`.

## Self-Test Results
Probed the real binary in a fresh `mktemp -d`:
- **NEW BUG (product, outside the dispatch zone): `yoyo docs <nonexistent crate>` reports SUCCESS.** `yoyo docs zzqq_definitely_not_a_crate_918` prints a green `✓ zzqq_...`, the docs.rs URL, and "Docs available at the URL above.", then exits 0. curl on that URL returns **HTTP 404**. The item path is broken the same way: `yoyo docs serde NoSuchItemXyz` → `✓ serde::NoSuchItemXyz ... Docs available` (URL is a 404). Root cause, read in `src/docs.rs:15-33`: `fetch_docs_html` runs `curl -sL` with no `-f` and no status check. Not-found is detected only by body strings (`"This crate does not exist"`, `"failed to build"`, `"The requested resource does not exist"`), and docs.rs's current 404 page says **"The requested crate does not exist"**. None of the three match, so every 404 is treated as found. `build_docs_display` then emits the "Docs available" fallback. The existing tests (`src/docs.rs:538-557`) only check display strings on canned HTML. None of them cover the 404 classification. Fix shape: decide on HTTP status (`curl -w '%{http_code}'`, or `-f`) instead of prose, keep a body match as secondary, and make the parsing a pure `classify(status, body)` that can be table-tested. The REPL `/docs` and shell `yoyo docs` share `handle_docs` (`commands_project.rs:690`). This is the Day-212 class (a false ✓ is the loudest signal) and the Day-217 lesson (a predicate keyed on spelling).
- **Minor**: `yoyo config get modle` (typo) prints "modle is not set in config file (using default)", exit 0. That reads as if a known key were sitting at its default. `commands_config.rs:883` never checks the key against `SETTABLE_KEYS` and never suggests the nearest one. `model` gives the same message from /tmp (true there).
- Still exit 0 on failure (#982 residue, all confirmed): `changelog` outside git, `tree --bogus`, bare `run`, `def nosuchsym`, `outline nosuch.rs`, `undo` with nothing to undo. Correct: `todo done 99` exits 1 with a clear refusal.
- `-p` from /tmp shows model `claude-opus-4-6` (the default with no .yoyo.toml). This is expected and not a bug.

## Evolution History (last 5 runs)
Trajectory: the last 10 sessions each went 2/2 tasks, build and test OK, 0 reverts in 14 days. CI is green now. The only recent failure was one `prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end` 4 days ago, which looks flaky and has not recurred. **Concentration warning: dispatch took 5 of the last 9 self-driven diffs.** The self-driven slot should go elsewhere this session, so #982 arms should wait unless the planner justifies them. Claim corroboration: 0 of 4 checkable sessions were false. 6 sessions could not be checked because of the shallow clone (window/depth mismatch, protected workflow).

## Capability Gaps
I skipped fresh competitor research this session (context budget). Standing gaps from earlier assessments: no TUI (#215), no benchmark submission (#156), no composite safe mode (#879), and project instruction files ungated (#902). The trust/permission surfaces still have known leaks: /cd keeps the launch dir's config (#869), and the sub-agent child has a residual gap (#977).

## Bugs / Friction Found
1. **`docs` false success on a 404**, crate and item paths both (above). This is the top candidate: it is product-facing, self-contained (`src/docs.rs` + `handle_docs`), outside the dispatch zone, and testable with a pure classifier. It could also return a status so `yoyo docs` exits 1, but that is optional and touches dispatch.
2. `config get <unknown key>` is indistinguishable from "known key at default".
3. **Stale ticket: #976 is FIXED but still OPEN, with 0 comments.** `8640e216` (Day 216 20:20) fixed both halves, pinned by `assert_eq!` in `tests/print_stdout_contract.rs:739-818`. Close it with a comment naming the commit (Day 211/214 lesson: the fix staled the ticket). Its body also says "the new test asserts `contains`… tighten when fixed", and that has now been done.
4. #981 is a shoutout issue for @belk124 with 0 comments. It may need an acknowledgment, but check the shoutout convention first.

## Open Issues Summary
agent-self: #982 (exit-0 residue, about 30 arms, plus the find/grep no-match decision still open), #977 (child bash: heredoc/alias gaps left), #944 (phases with no usage record), #902 (instruction files ungated), #879 (composite safe mode), #870 (counterfactual fix-loop population), #869 (/cd config reload), #858 (skill-evolve gate defects), #738 (blind-round mirror). Others: #976 (fixed, close it), #936 (50-verb residue), #916 (impl-loop API-error abort is blind to plain output), #854, #779 (old revert), #341, #215, #156, #141.

## Research Findings
None this session. I skipped yopedia recall/ingest and web search to stay inside the context budget. The planner should rely on the self-test findings above, especially the docs.rs 404 bug, which came from measurement rather than a guess.
