# Assessment — Day 216

## Build Status
Pass. The harness verified it at session start, and CI is green on HEAD `7e192b17`. Binary probes (debug build, from an empty `/tmp` repo with no config, default model claude-opus-4-6):
- `-p "Reply with exactly: PONG"`: stdout is exactly `PONG\n` (5 bytes), exit 0. The Day-215 leading/trailing blank-line fixes hold live.
- Piped stdin, same prompt: `PONG\n`, exit 0.
- `--output-format stream-json`: starts `{"type":"agentStart"}` then `{"type":"turnStart"}`. No session acknowledgement exists.
- **#979 reproduced exactly as filed.** I wrote `{ not json` into `.yoyo/last-session.json` and ran `-p ... --continue`. stderr: `warning: Failed to restore session: invalid type: map, expected a sequence at line 1 column 0`. A model call was made, stdout was `PONG`, **exit 0**.
- **#978 confirmed.** After a `-p` run, `.yoyo/` contains only `audit.jsonl`. No session is written.

## Recent Changes (last 3 sessions)
- **Day 216 01:17**: Sub-agent and explore-agent child bash now honour the user's `permissions.deny` (`UserDenyBashTool` and the shared `user_deny_refusal` in src/tools.rs). The gap that remains is filed as #977. Separately, BEL bytes no longer land on stdout in `-p`/piped mode (#976 half 1). The other half of #976 is open: each retry re-streams the partial answer, giving 6 copies on a pipe.
- **Day 215 20:47**: A user `deny` pattern now blocks bash under `--yes` and after "always" (`StreamingBashTool::user_deny`, tests in src/tools_user_deny_tests.rs). The `-p` partial-answer-survives check came back clean, and that probe is how #976 was found.
- **Day 215 10:43**: `-p` stdout no longer ends with a trailing blank line (`stream_leading_blank.rs`). #936 slice 2: the 39-verb census, with `mcp` gated.
- Every recent session went 2/2 with no reverts. Themes: stdout contract for scripted callers, and permission/deny propagation. The journal's open question is "how many protections assume a person is watching". Two new creator issues (#978, #979) sit on the same axis: scripted callers of `-p`.
- llm-wiki (external project) is still paused mid-migration. No new entries.

## Source Architecture
About 116k lines across ~190 files in src/. Entry: `src/main.rs` (1311). Key modules: `cli.rs` 7584 (grandfathered at exactly its current length, so **any growth needs a register bump in tests/module_size.rs**), `tools.rs` 5033 (grandfathered, at cap), `prompt.rs` 3792 (grandfathered), `format/mod.rs` 2874, `commands_session.rs` 1754 (under the 2000 cap, ~246 lines of headroom), `commands_config.rs` ~2015. The module cap is 2000 lines, with a 50-line overshoot grace and a 100-line register-drift grace.

Seams relevant to #978/#979:
- `restore_session(agent)` at src/main.rs:1010 prints and swallows both errors: a parse failure gives a YELLOW warning, and a missing file gives a DIM "no previous session found". It is called at main.rs:1224 **before** the `-p` branch, so a strict check can exit before any model call.
- `continue_session` is parsed at cli.rs:2706 (`--continue`/`-c`), and the flag list is at cli.rs:693 (the known-flags list, so a new flag must be added there too).
- `commands_session::auto_save_on_exit` (l.399) wraps `auto_save_on_exit_in(agent, root)` and `auto_save_on_exit_result -> AutoSaveOutcome {NothingToSave, Saved, Failed}`. It already has a path-parameterised, testable seam. Its only caller is repl.rs:1492. `AUTO_SAVE_SESSION_PATH` lives in cli_config.rs:115, and `continue_session_path()` in commands_session.
- stream-json output comes from `run_prompt_stream_json` / `handle_stream_json_events` in prompt.rs:1785, which serialises yoagent `AgentEvent`s. A `sessionRestored` line has to be emitted by yoyo itself after `agentStart`. yoagent has no such event.

## Self-Test Results
See Build Status. Nothing broke. Friction found: the #979 silent-restart path (exit 0 after a failed restore) and #978 (no session from scripted runs). I did not re-probe the #976 retry re-stream live (it needs a stub server). It is pinned by `tests/print_stdout_contract.rs::armed_doors_keep_streamed_partial_answer_when_turn_dies_mid_stream`, which asserts `contains`, and the issue says to tighten it to an exact assert when fixed.

## Evolution History (last 5 runs)
Evolve: 4 success (10-01 01:02, 10:41, 20:45; 10-02 01:15), and the current run is in progress. CI: the last 5 are all success. The trajectory shows 0 reverts in 14 days and 10/10 sessions at 2/2. The only CI failure in the window is a one-off, 3 days old (`prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end`), and CI has been green since. Claim corroboration: 4 checkable, 0 flagged. 6 were uncheckable because of the shallow clone (a standing, known shape).

## Capability Gaps
- **Scripted-caller session contract** (#978, #979). Claude Code exposes `--resume`/`--continue` and session ids in its print/stream-json modes, and wrappers rely on a run being resumable and on a resume being verifiable. yoyo has no save from `-p`, and `--continue` fails silently. These are the most concrete product gaps on the board, they were filed today by the creator, and both are opt-in.
- Sub-agent child bash still skips the safety.rs destructive checks and confirm (#977). This is design-blocked: a child has no user, so "confirm" means refuse, or inherit the parent's state.
- Longer-standing: TUI (#215), benchmarks (#156), composite safe mode (#879), /cd config reload (#869).

## Bugs / Friction Found
1. **#979, reproduced.** A corrupted session plus `--continue` gives a warning, a fresh conversation, exit 0. The issue asks for an opt-in `--continue-strict` (exit non-zero before any model call when the file is missing or unparsable) and a stream-json `{"type":"sessionRestored","messages":N}` line after `agentStart`. Plain `--continue` stays lenient. Natural shape: a pure `restore_session_result(path) -> Result<usize, String>` behind both modes, with the current `restore_session` keeping its exact stderr text (near-miss: lenient mode must stay byte-identical).
2. **#978, confirmed.** `-p`/piped never write a session. The ask is opt-in `--save-session <path>` for both non-interactive modes. It must also write after a failed turn, and a failed write must exit non-zero. The output must equal what `/save <path>` writes (`agent.save_messages()`). Remember the yoagent lifecycle gotcha (CLAUDE.md, #258): call `agent.finish().await` before `save_messages()`, or the history is stale. Both -p and piped paths need wiring ("two doors" risk), plus the stream-json and json paths.
3. **#976 half 2**: the retry re-stream of a partial answer on the plain `-p`/piped doors (6 copies on a pipe). The test fixture already exists.
4. Both #978 and #979 touch cli.rs, which is at its grandfathered ceiling. Any flag-parsing growth must re-paste `("src/cli.rs", N)` in tests/module_size.rs, or the logic should live in commands_session.rs (which has headroom), leaving only the parse in cli.rs. A new flag also has to be added to the known-flags list (cli.rs:693) and to help.rs.

## Open Issues Summary
- **agent-input (new today, creator):** #979 strict `--continue` + `sessionRestored`; #978 `--save-session <path>`. Both `Kind: product`, opt-in.
- **agent-self:** #977 (child bash safety and confirm, needs a design decision first), #944 (phases spending tokens with no usage record; social is the largest), #902 (instruction-file trust door, partly shipped Day 194/210), #879 composite safe mode, #870 counterfactual fix-loop population, #869 /cd config reload, #858 skill-evolve gate defects, #738 blind-round mirror.
- **Other:** #976 (half 2 open), #936 (verb residue, per-verb judgement), #916 (impl-loop API-error abort blind to plain-output errors), #854, #779 (revert record), plus the long-running #341/#215/#156/#141.

## Research Findings
I skipped the web research and yopedia recall this session: the context budget ran out during the survey. From memory, which I did not re-verify today: Claude Code's headless mode documents `--resume <id>`/`--continue` and emits a session id in its stream-json init and result records. The planner should treat that as unverified. The concrete, verified gaps are #978/#979 above, so research would not change the priority.

**Suggested priority for the planner:** Task 1 is #979 (small, reproduced, and it is a silent-success defect: strict mode plus the stream-json ack, with lenient mode byte-identical as the near-miss). Task 2 is #978 (`--save-session`, both doors, write after a failed turn, non-zero exit on write failure, `finish()` before save). #976 half 2 is the fallback if either turns out too large.
