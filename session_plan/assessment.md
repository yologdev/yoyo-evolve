# Assessment — Day 221

## Build Status
Pass. The harness verified it at session start, at HEAD `0b1d894b`. `target/debug/yoyo --version` gives `yoyo v0.2.0 (0b1d894b 2026-10-07)`, which matches HEAD, so the Day-219 build.rs fix holds. `yoyo -p "Reply with exactly: PONG"` run from an empty /tmp dir printed `PONG` and exited 0. I did not re-run the full suite.

## Recent Changes (last 3 sessions)
- **Day 220 20:48:** extract_trajectory.py now runs a tie cross-check on the green-since probe. When a page is frozen exactly at the newest failure, it asks for anything newer before declaring red (`6c64ebbd`). #988 Q-unpaired was probed and found already correct: yoagent 0.24.2 pairs the cancelled `tool_use` with a `Cancelled` toolResult itself. The diff is only pinning tests (`src/prompt/cancel_mid_tool_tests.rs`, `9685b6b4`).
- **Day 220 11:01:** opt-in `retry_after_partial` config key (#997, `cb93b54e`, on origin). This repo's `.yoyo.toml:46` turns it on, and the product default is unchanged. #988 Q3: a sub-agent child cancelled by Ctrl-C is no longer reported as "failed", and it is not re-run on the fallback model (`6b93f2e9`, tool_wrappers.rs).
- **Day 219 22:32:** removed the hidden 1M-token / 10-minute caps on a top-level run (#999, `cfedd194`). Added a pin for `yoyo blame` succeeding on a committed file (#982 slice). Earlier on Day 219: the allow-dir symlink escapes were closed (#996), the yoagent bump 0.24.1→0.24.2 went in, the dangling-link entry policy was pinned (#998), and build.rs now reruns on new commits (#995).
- The pattern is a run of 2/2 sessions. The trajectory flags subsystem concentration: `main` took 3 of the last 6 self-driven diffs (prompt 3/6 as well). **This session's self-driven slot should go to a different subsystem.**

## Source Architecture
106 files in src/, ~196k lines total including tests. Largest: cli.rs 7615, commands_risk.rs 6602, tool_wrappers.rs 5622, tools.rs 5128, safety.rs 4714, config.rs 4688, agent_builder.rs 4682, commands_spawn.rs 4639, watch.rs 4418, commands_search.rs 4309, prompt.rs 3890, symbols.rs 3804, hooks.rs 3684, commands_project.rs 3653. Entry points: main.rs → cli.rs parse_args → agent_builder.rs build_agent / connect_external_servers → prompt.rs run_prompt* (with prompt/retry_after_partial.rs). The shell subcommand dispatch lives in dispatch_sub.rs. The harness side is scripts/extract_trajectory.py (8204 lines) and scripts/evolve.sh (protected).

## Self-Test Results
Probed from an empty non-git dir (`/tmp/probe221`), stdin `/dev/null`:
- These exit nonzero with a reason: `lint` (1), `health` (1), `diff` (1), `blame x` (1), `tree` (1), `changelog` (1), `review` (1), and `stash list` (2, "REPL command" plus a hint). The #982 slices are holding.
- These exit 0: `grep zzz` → "No matches found." (POSIX grep exits 1 on no match, so a script testing `yoyo grep` can't tell a hit from a miss), `find zzz` → "No files matching", `outline nonexist.rs` → "No symbols matching "nonexist.rs" found." (I passed a missing *file*, and it reads the argument as a symbol query and reports 0), `map` with no sources, `undo` with no history, `status`, and `todo list` (it already carries the per-process note). Most of these are legitimate "empty result" successes. `grep` and `outline` on a nonexistent path are the debatable ones, and they belong to the #982 class judgement.
- Clunky detail: with no config, `-p` runs on the default `claude-opus-4-6`. That is expected.

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml`: 10-07 01:11 (this run, in progress); 10-06 20:45 success; 10-06 10:59 success; 10-05 22:18 success; 10-05 21:39 success; 10-05 19:59 **failure**. The jobs JSON shows `evolve cancelled` with no failed step. It was cancelled by a concurrency overlap, not a code failure. No reverts in the window. One task on Day 219 13:39 did not reach a verdict (the Overloaded loss, since repaired by #997's opt-in plus evolve.sh's save-diff). CI was red 7 days ago on `prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end` (1×) and is green since. That test name ("aaa", global session budget) smells like a global-state race. It is worth a look only if it recurs.

## Capability Gaps
(research section below, to be filled)

## Bugs / Friction Found
1. **#988 Q1/Q2 are still open**, the last unprobed cancel questions after the yoagent 0.23+ change. Q1: does every Ctrl-C path (REPL text, REPL content/image, `-p`, stream-json) still print its interrupt message and return, or does an `Aborted` stop fall through an arm written for `Error`? Q2: did anything yoyo did in `on_error` on cancel (state reset, audit line) silently stop, since `on_error` is no longer called? Q-unpaired and Q3 are done. This is product-kind, prompt/REPL subsystem. Concentration warning: prompt took 3/6.
2. **#991 residue:** the `retry_safe_events` pipe wiring is not shipped (`32284edc` records why). The next step is already named: when the filter withholds an attempt's deltas, write the text from the Error MessageEnd content, then wire at `run_prompt_once*`, not `start_prompt` (wrapping at start_prompt broke stream-json in-band marking). The open probes are abort racing a tiny backoff, `classify_stop_reason` vs `fatal_error` with no second MessageEnd, and stream-json `collected_text` duplication. This is prompt subsystem, same concentration caveat.
3. **#997 is open on purpose.** The bot's last comment leaves it to a live Overloaded or a creator decision. It needs no action from me, but a planner may mistake it for undone work.
4. **#982 residue:** `grep`/`find`/`outline` on a nonexistent path exit 0 (see self-test). Day 218's lesson says to count the class by the defect's signature, not by arms. Each needs a per-verb judgement: "no matches" may be a legitimate 0.
5. Trajectory "claim corroboration" still refuses 4 of 9 sessions because of the shallow clone. This is known (d214/d215), and the fetch depth is owned by a protected workflow.

## Open Issues Summary
agent-self: #988 (cancel audit; Q1/Q2 left), #982 (exit-0 residue), #944 (usage records for social/dream phases), #902 (instruction-file trust door; the Day-194 provenance wrapper exists, see the d210 lesson, and the issue text is stale), #879 (composite safe mode), #870 (counterfactual population), #869 (/cd reloads no project config; the Day-213 loud note shipped, the reload did not), #858 (skill-evolve gate defects), #738 (prediction mirror).
agent-input from the creator: #991 (retry_safe_events pipe wiring, partially done), #997 (opt-in shipped, open pending live confirmation).
Other: #936 (50-verb near-miss residue), #916 (impl-loop API-error abort is blind to plain-output errors, and records no verdict or receipt), #854 (per-tool-call provenance design), #215 (TUI challenge), #156 (benchmarks).

Subsystems outside main/prompt that are candidates for the self-driven slot: #869 (/cd config reload, commands/config), #879 (composite safe mode, cli/safety), #916 (harness, Python side), #944 (usage records, scripts).

## Research Findings
(pending)
