# Assessment — Day 211 (22:05)

## Build Status
Pass: the harness verified it at session start, and `target/debug/yoyo` was built at 22:06. I did not re-run the full suite. My binary probe below reproduced #965 exactly.

## Recent Changes (last 3 sessions)
- **21:13**: #963 fixed the compact-thrash test race with a `_with` seam, so each test uses its own counter, and paid-off register rows were retired. #964 fixed the max_tokens-ceiling warning: it had been silent on the Anthropic path because it was quiet-gated under captured output, and it compared against the default output limit instead of the maximum. It now warns at 131072 and stays silent at 128000.
- **19:30**: #742 fixed `/retry`, which now uses `PromptOutcome.last_tool_name` instead of scanning the error text. **#742 is still OPEN on GitHub and should be closed.** For #944 slice (e), `daily_diary.sh` now reports per-run spend by reading the audit log. Receipt #959 covers that slice.
- **16:28**: The dispatch tests no longer run the real setup wizard or init. They had been clobbering `.yoyo.toml`. Per-edit auto-check now skips cargo check for edits that cannot affect Rust.
- Creator commits since then: `70d70037` scan_commitments now calls through yoyo. `e08025e0`/`f31ce545` made `.yoyo.toml` the single model source, and the harness test now fails if a model id is hardcoded anywhere else.
- Earlier on Day 211, three sessions (00:12, 09:48, 14:59) made no commits at all. The 16:28 session identified the cause: the wizard test clobbered `.yoyo.toml`, which sent every call to the wrong model.

## Source Architecture
There are about 200 files under src/, roughly 116k lines. The big modules are tool_wrappers.rs (5.6k), agent_builder.rs (4.6k), prompt.rs (3.8k), commands_project.rs (3.6k), repl.rs (3.4k), dispatch.rs (2.4k) and dispatch_sub.rs (2.2k). The entry is `main.rs`, which goes to either `repl.rs` or `prompt.rs` (`handle_prompt_events` for the display path and a stream-json loop at around line 1848). Model and provider setup is in agent_builder.rs (`build_agent`, `try_fallback_prompt`). The scripts that matter are `scripts/evolve.sh` (protected), `extract_trajectory.py` and `scan_commitments.py`.

## Self-Test Results
I ran a local stub HTTP server that returns 404 and pointed the binary at it with `--base-url`, a dummy key and `--no-tools`:
- `yoyo -p "hi"` → **exit 0**, stdout is 1 byte, and stderr shows the red error line and the model list.
- Adding `--output-format json` → **exit 0**, stdout is 1 byte, and **no JSON is written**.
So #965 reproduces at HEAD. Cause, confirmed by reading `src/prompt.rs` lines 940–950: the final `else` in the `StopHandling::InspectError` arm prints the error and the diagnostic but sets no field. Its sibling branches set `fatal_error`, `overflow_error` or `retriable_error`. As a result `into_result()` returns `Done`, `last_api_error` stays None, `try_fallback_prompt` never tries the fallback, and main's exit-1 path is never reached. The stream-json loop (lines 1953–1962) already records this error, so only the display path has the gap.

## Evolution History (last 5 runs)
- 21:11, 19:29, 16:27 and 14:58 all succeeded. The 15:10 run was cancelled. The 22:04 run is this one, in progress.
- The trajectory shows the 21:42, 20:01 and 17:16 sessions each completed 2/2 tasks. Before that, three sessions made 0/1 with no verdict. They showed green but produced nothing: the empty diffs are filed as receipts #960 and #959, whose logs end in the "Switch with: /model" unknown-model text. That is the model and config breakage that has since been fixed.
- CI has been green for more than a day. The recurring CI errors in the trajectory are all more than 11 days old.
- A cross-cutting pattern: **green runs that did nothing**. #965 is the product-side root of it. The loop's harness greps stderr for errors precisely because the exit code cannot be trusted.

## Capability Gaps
- **Scripting and CI use (`-p`, `--output-format json`) fails open on auth, not-found and bad-request errors.** Claude Code and Codex exit non-zero with `is_error` set in their JSON output. This is the most important product gap right now, because anyone running yoyo in CI reads a failure as success.
- Carried over from before: there is no composite safe mode (#879); `/cd` does not reload project config (#869); there is no TUI (#215); and yoyo has no benchmark submissions (#156).

## Bugs / Friction Found
1. **#965** (creator-filed, `agent-input`, Kind: product): the exit status and JSON output are wrong for non-retriable errors. The fix is most likely one line in that `else` branch, setting `self.fatal_error`. Before choosing that, the planner should check that `FatalError` handling in both prompt loops does not print the error a second time, and that fallback now firing on 401/404 is intended. The issue says it is. Tests should sit at the emission point: 400, 401 and 404 should give exit 1 and `is_error: true` in JSON. Near-miss guards: a successful run still exits 0, a benign stream end is not an error, and overflow still goes to compaction. Positive control: neuter the new assignment, then restore it.
2. **#742 is fixed but still open.** Close it with the commit reference `8d1f0ff3`. #773 (the revert receipt for #742) can be closed too.
3. **#959 and #960 are history.** #959's work landed at 19:30 in `ecf15eee`. #960 was a fallback task with an empty diff, caused by the model/config breakage that has since been fixed. Both can be closed.
4. **#958** (accepted UNVERIFIED): the Day 210 unhittable-denominator diff has never had a review. It needs a small read of `git diff 9c222c88..` for that task. This is a cheap backlog item.
5. The 19:30 journal entry suspects that `social.sh` has the same quiet-mode spend blind spot that `daily_diary.sh` had. That was inferred from the code and never observed; it is part of #944.

## Open Issues Summary
The top priority is **#965**: filed by the creator 3 minutes before this session, precisely specified and reproduced by my probe above. Other open items:
- #944: remaining spend phases, with social the largest.
- #937: price drift.
- #902 and #879: trust and safe-mode.
- #869: `/cd` config reload.
- #858: skill-evolve gate defects.
- #916: the impl-loop API-error abort cannot see plain-output errors. It is related to #965, since the loop relies on grepping stderr.
- #951: the wrap-up sweep is ungated. Help wanted; evolve.sh is protected.
- #936: the 50-verb residue.
- Housekeeping closes: #742, #773, #959, #960, and possibly #958 after review.

## Research Findings
I skipped the competitor and yopedia research this session because the context budget ran out during probing. The relevant prior finding still stands: Claude Code's `-p` / `--output-format json` contract returns a non-zero exit and `is_error: true` on API failure, which is exactly what #965 restores. Suggested plan: Task 1 is #965 (product). Task 2 is small, such as reviewing #958 or the #944 social slice, together with the housekeeping closes.
