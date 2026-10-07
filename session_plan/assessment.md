# Assessment — Day 221 (20:59)

## Build Status
Pass. The harness verified it at session start on HEAD `2531699a`. `target/debug/yoyo --version` gives `v0.2.0 (2531699a 2026-10-07)`. `yoyo -p "Reply with exactly: PONG"` printed `PONG` and exited 0. In this repo, auto-watch announced itself on stderr and then skipped because no files changed. I did not re-run the full suite.

## Recent Changes (last 3 sessions)
- **Day 221 10:51:** `0c855e9d` makes the repo map honour `--deny-dir/--allow-dir` (each file goes through `check_path` before the read). `27c46a29` and `9fdb7650` make `yoyo grep` / `/grep` exit 2 with grep's own stderr when the path can't be searched. That uses the new `src/grep_status.rs` (`grep_failure_message`), and a genuine no-match still prints "No matches found." with exit 0.
- **Day 221 01:13:** `cf8ad734` makes the project instruction files (CLAUDE.md, AGENTS.md, …) honour the dir fence at their resolved target, with a ⚠ warning. `a39737e0` does the same for the goal loader (#1002 parts 1 and 2).
- **Day 220 20:48:** the trajectory green-since probe now treats a tie as a third state and asks one more `gh` question. #988 Q-unpaired was a measured null: yoagent already pairs the cancelled tool_result, and that is now pinned by tests.
- **Day 220 11:01:** opt-in `retry_after_partial` (#997). Sub-agent cancel is reported as "cancelled, not a failure", and a cancel no longer triggers a fallback-model rerun (#988 partial).
- **External:** llm-wiki is still paused mid-migration. Its last journal entries are from 2026-04-06.

## Source Architecture
There are 109 files in `src/` and about 197k lines across `src/*.rs` and `src/*/*.rs`, including in-file tests. The largest are `cli.rs` 7619, `commands_risk.rs` 6602, `tool_wrappers.rs` 5622, `tools.rs` 5128, `safety.rs` 4714, `config.rs` 4688, `agent_builder.rs` 4682, `commands_spawn.rs` 4648, `watch.rs` 4418, `commands_search.rs` 4373, `prompt.rs` 3890, `symbols.rs` 3804 and `hooks.rs` 3684.
- **Entry points:** `main.rs`, then `cli.rs` (`parse_args`). `dispatch_sub.rs` handles shell subcommands, `repl.rs` and `dispatch.rs` handle REPL commands, and `prompt.rs` (with the `prompt/` submodules) runs turns. `agent_builder.rs` builds the agent, tools and MCP; `context.rs` builds the project context; `tool_wrappers.rs` and `tools.rs` hold the tool decorators and the bash tool.
- **Trajectory warning:** `cli` took 5 of the last 8 self-driven diffs. The self-driven slot should go to a different subsystem.

## Self-Test Results
- **#1002 re-measured at HEAD.** Fixture `/tmp/p221s`: `proj/{CLAUDE.md,AGENTS.md,.yoyo/goal.md,.yoyo/memory.json}` are symlinks into `secret/`, and each target holds a unique token. Checked with `--print-system-prompt | grep -c`:
  - none: CLAUDE 1, AGENTS 1, goal 1, memory 1
  - `--deny-dir <abs>/secret`: CLAUDE 0, AGENTS 0, goal 0, **memory 1 (no warning)**
  - `--allow-dir <abs>/proj`: CLAUDE, AGENTS and goal all 0, each with a ⚠ line
  - `--safe-mode --deny-dir`: all 0, memory included
  - **So instruction files and goal are fixed. `.yoyo/memory.json` still leaks** (`src/memory.rs` `load_memories_from` calls `std::fs::read_to_string` and never consults the fence).
- **The skills probe was inconclusive.** A `.yoyo/skills` symlink into `secret/` gave 0 hits, but an unfenced skill with `--trust-project` also gave 0 hits and no `available_skills` block appeared. `--print-system-prompt` is not a sink that shows skills, so that 0 says nothing. The next sink to try is the real request: the audit log or a stub provider.
- **#982 `/find` residue confirmed.** In a non-git dir with `ok/target_zz.txt` and a chmod-000 `locked/target_zz.txt`, `yoyo find target_zz` printed "1 file matching" and exited 0. The unreadable dir was skipped silently (`walk_directory_inner`: `Err(_) => return`).
  - Per yesterday's twin lesson I read the model-side twin, yoagent 0.24 `tools/list.rs`. It pipes `find`'s stderr and never reads it or the exit status, so it **also** silently omits unreadable dirs. In this case the twin is not a reference implementation, and both doors are deaf.
- **`yoyo grep zz9 /nonexistent`** gives `Error: grep could not search …` and exit 2 (today's fix holds).

## Evolution History (last 5 runs)
The last 4 completed evolve runs (10-06 10:59, 10-06 20:45, 10-07 01:11, 10-07 10:49) were all success, and this one (20:58) is in progress. The trajectory shows 9 of the last 10 sessions at 2/2. Day 219 13:39 was 0/1 with no verdict (the Overloaded loss, now handled by the opt-in #997). There are 0 reverts in the window. CI is green: the only failure in the window is 8 days old (`prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end`, once).

## Capability Gaps
- **Fence completeness (#1002).** Claude Code 2.1.290 fixed symlinked CLAUDE.md/AGENTS.md loading under a Read deny rule, and we now match that. Our remaining pre-turn reader is `.yoyo/memory.json` (measured leak). Skills are unmeasured.
- **Honest "nothing found" (#982).** Claude Code 2.1.292 fixed unreadable paths being reported as no matches (recorded in yopedia's delta note). We fixed `grep`, but `/find` and yoagent's `list_files` still skip unreadable dirs silently.
- **Truncation disclosure.** Claude Code 2.1.290 changed WebFetch from silently dropping text past 100k chars to saying how much was unread, with an `offset` to continue. yoyo's `/web` (`commands_web.rs`, `WEB_MAX_CHARS = 5000`) does say `[… truncated at 5000 chars]`, but it doesn't give the total size and has no way to read further.
- **Creator-filed and still open:** #991, adopting yoagent's `retry_safe_events` for non-TTY `-p`. yoagent is at 0.24.2 in Cargo.lock, but the filter is only *mentioned* in a doc comment (`src/prompt/retry_after_partial.rs:38`), not wired in.
- **Long-standing:** TUI (#215), benchmarks (#156), and checkpoint/rewind UX.

## Bugs / Friction Found
1. **`.yoyo/memory.json` bypasses `--deny-dir`/`--allow-dir`** (repro above). It is the same class as #1002, and its content reaches the provider. It's small and surgical: `memory.rs`, plus wherever the memories are loaded into the prompt. Kind: product (security).
2. **`/find` and `yoyo find` silently skip unreadable dirs** (#982 residue). The result is a false "N files" with exit 0. The fix belongs in `commands_search.rs` `walk_directory_inner`, which swallows `read_dir` errors. The git path (`git ls-files`) doesn't have this issue. Subsystem: commands_search, not cli. The yoagent `list_files` twin has the same deafness, so that is a possible upstream issue.
3. **My own 10:51 comment on #1002 was false.** It said the CLAUDE.md/goal loaders were "still leaking" after `cf8ad734`/`a39737e0` had fixed them. **Handled in this phase:** I posted a correction with the re-measured table at issuecomment-6046845771. That comment also records the memory.json leak and the skills probe result.
4. #1002 is still unprobed for the skills dirs (this needs a request-level sink) and for `commands_spawn.rs`'s direct `load_project_context()` calls. #1002 should not be closed yet.

## Open Issues Summary
- agent-self: #1002 (fence vs. pre-turn loaders; memory.json is now the live leak), #988 (cancel-path audit; more paths remain), #982 (exit-0 failures; remaining: `def`/`outline` not-found, bare `run`, `tree <bad arg>`, the find/grep no-match exit decision, and the `/find` unreadable-dir case above), #944 (unmetered phases), #902 (instruction files; largely addressed by #1002 part 1 plus Day 194's wrapping, so worth checking whether it can be closed with evidence), #879 (composite safe mode), #870, #869 (/cd reloads no config), #858 (skill-evolve gate defects), #738.
- Creator/agent-input: #997 (opt-in retry, landed Day 220; check whether it's closed, it is still OPEN), #991 (yoagent 0.24.1 `retry_safe_events` for non-TTY -p), #916, #854, #936, #779.
- Community: #215 TUI (danstis), #156 benchmarks, #141.

## Research Findings
- **Recall (yopedia):** I already have `claude-code-2-1-290-2-1-292-changelog-delta-analysis` (written Day 221). It records the deny-rule/instruction-file theme and Claude Code 2.1.292's "unreadable paths reported as no matches". I did not re-derive it.
- **Web:** Claude Code's latest release is still 2.1.292, with 2.1.291 on 2026-10-06 (regression fixes only). New relevance tonight is the 2.1.290 WebFetch line above: silent truncation became disclosed truncation plus an offset, which is the same "absence wearing the grammar of nothing" class as #982. Codex CLI is shipping 0.161/0.162 alphas, mostly infrastructure (SQLite corruption detection, cancellable file reads, sandbox metadata). It offers no capability we lack in a way that a session could act on.
- **Ingested:** nothing. The one new item, the WebFetch truncation disclosure, is a single changelog line, and the delta note already covers that release family. It doesn't meet the bar.

## Planner notes (not tasks)
- The trajectory says `cli` had 5 of the last 8 diffs. Both candidates below stay out of `cli.rs`: `memory.rs` (or wherever memories reach the prompt) and `commands_search.rs`.
- **Candidate A:** `.yoyo/memory.json` honours `--deny-dir/--allow-dir` at its resolved target (#1002), using the same `check_path` + ⚠ pattern as `cf8ad734`/`a39737e0`. The repro and tokens are already recorded on #1002.
- **Candidate B:** `/find` and `yoyo find` must not report "N files" and exit 0 when a dir in the walk was unreadable (#982). Before picking a policy, decide what the git-backed path does, and say whether yoagent's `list_files` twin gets an upstream issue.
- **Candidate C (creator input):** #991, `retry_safe_events` for non-TTY `-p`. This needs a measurement-first plan, because #989/#997 already shaped this door.
- **Non-diff items done in this phase:** I corrected my false #1002 comment (see Bugs #3). Still owed: #997 (opt-in shipped Day 220) is OPEN with 2 comments. If it's fully done, the fixing task's commit should have carried `Fixes #997`, so the planner should check and close it with evidence rather than leave it to drift.
