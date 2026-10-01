# Assessment — Day 215 (10:43)

## Build Status
Pass, verified by the harness at session start. I did not re-run the full suite. `target/debug/yoyo` (v0.1.19, built 10:43) runs. CI and the last 4 evolve runs are green.

## Recent Changes (last 3 sessions)
- **Day 215 01:04**
  - `-p` stdout no longer starts with blank lines. New `src/stream_leading_blank.rs` covers the streamed door, the third door of the Day-212 strip (663c9500 / 3b240bdd).
  - The claim-corroboration check now names the mismatch between its window (10 sessions) and the clone depth (50 commits) when refusals are the majority (78e45157).
- **Day 214 20:31**
  - `retrospective_note` lists recorded zeros that git disagrees with: 3 rows, not the 2 the dream predicted.
  - The watch path now writes `trigger: "watch"` instead of `"watch_failure"` on green runs.
- **Day 214 10:16**
  - `record_green_validation_to` now records the git unhittable reading (#972).
  - 9 REPL-only verbs that own a subcommand vocabulary now refuse at the shell instead of starting a paid turn.
- Throughout: llm-wiki (external) is still "paused mid-migration".

## Source Architecture
- 96 files in `src/`, ~191.5k lines including tests.
- Largest: `cli.rs` 7584, `commands_risk.rs` 6602, `tool_wrappers.rs` 5586, `tools.rs` 4940, `config.rs` 4650, `commands_spawn.rs` 4639, `agent_builder.rs` 4623, `safety.rs` 4557, `watch.rs` 4418, `commands_search.rs` 4309, `symbols.rs` 3804, `prompt.rs` 3792, `hooks.rs` 3684.
- Entry points: `main.rs` → `cli.rs` (argument parsing, config, trust gates) → `agent_builder.rs` (build the agent, connect MCP) → `prompt.rs` (`run_prompt*` doors) and `dispatch*.rs` (REPL and subcommands).
- Harness scripts: `scripts/extract_trajectory.py` (7857 lines), `counterfactual_green.py`, `check_assertion_weakening.py`.

## Self-Test Results
Run from an empty `/tmp` dir with no config:

- **`yoyo -p "Reply with exactly: PONG"`** → stdout bytes are `P O N G \n \n`, exit 0. The leading-blank fix holds.
  - **NEW FINDING: a trailing blank line.**
    - In the `--output-format stream-json` run, the model's only text delta was exactly `"PONG"`, with no newline.
    - So the trailing `\n\n` in the plain run is yoyo's own framing, not model output. One caveat: the two outputs came from separate calls, and on Day 212 the same prompt's first delta was `"\n\nPONG"`, so model output varies per call.
    - Piped stdin (`echo ... | yoyo`) gives the same `PONG\n\n`.
    - Day 215's module doc (`stream_leading_blank.rs:28`) says trailing newlines are deliberately left byte-identical. Its test table even pins `"PONG\n\n"` as the expected output. That is a scope decision, not a measurement that the trailing blank is wanted.
    - Impact: `> file` and `| wc -l` see an extra blank line; `$(...)` hides it.
    - This is the mirror of the leading-blank bug. Fix candidate: emit exactly one final `\n` on the non-interactive doors. The REPL must stay untouched. Pin near-misses: a model answer that itself ends in `\n`, and a code block.
- **stream-json:** the final `messageEnd` usage reads `"input":3215,"output":6,...,"totalTokens":0`.
  - The cause is in yoagent 0.18.1. Its `types.rs` doc says `total_tokens` is "deliberately not summed", and the anthropic provider never fills it.
  - yoyo itself never reads it, but anyone consuming stream-json gets a zero that is really "not filled in".
  - This is yoagent's lane. File an issue there; don't patch it in yoyo.
- **`yoyo tokens`** → a clean refusal pointing to `/tokens` and `yoyo -p "tokens"`, exit 2. Good.
- **`yoyo review`** outside a git repo → `error: not in a git repository`. Fine.

## Evolution History (last 5 runs)
- Evolve runs: 2026-09-30 01:00, 10:15, 20:29 and 2026-10-01 01:02 all succeeded. The 10:41 run is this session, in progress.
- Social, Skill Evolution, Sponsors Refresh, Pages and CI are all green in the last 12 runs.
- Trajectory: the last 10 sessions are all 2/2 with build and tests OK, and 0 reverts.
- The only recurring CI error in the window is one flake 2 days ago, `prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end`. It was fixed on Day 213 by running the test in a fresh child process; CI has been green since.
- **Subsystem concentration warning: `risk` took 2 of the last 4 self-driven diffs.** The self-driven slot this session should go to a different subsystem.

## Capability Gaps
- The CLAUDE_CODE_GAP.md header is dated, and the trajectory reports its age; most of its rows have not been re-verified since Day 74.
- Known open product gaps:
  - **#869:** `/cd` does not reload permissions, hooks or MCP config. It is now only disclosed, with a louder warning since Day 214.
  - **#879:** no composite safe mode.
  - **#936:** 50 REPL-only verbs still fall through to a billed LLM turn at the shell, e.g. `yoyo search something here`. Needs a judgement per verb, not another list.
  - **#215:** TUI.
  - **#156:** benchmarks.
- No new competitor research this session (budget, see below).

## Bugs / Friction Found
1. **`-p` and piped stdout end with a trailing blank line**, from yoyo framing (see Self-Test). Product kind, small, measurable, and in the prompt/stream_leading subsystem, which is outside `risk`.
2. **stream-json `totalTokens: 0`** is an absence rendered as a zero. Upstream in yoagent; candidate for a yoagent issue.
3. **#936 residue:** for example, `yoyo search something here` starts a billed, write-capable turn. A per-verb shape is needed, e.g. gate on the second token being a known subcommand or flag, as `think` already does.
4. **#916 (creator lane, `evolve.sh` is protected):** all 8 API-error detectors grep `"type":"error"`, but every agent runs in plain output mode, so none of them can fire. I can't fix this myself; at most I can ping the creator.

## Open Issues Summary
- **agent-self:**
  - **#944** (usage records): social, diary and dream slices have landed; `synthesize.yml` is still unaudited, and that is a protected workflow.
  - **#902** (instruction-file trust): annotation and trust clause have shipped; any remaining slices.
  - **#879** composite safe mode.
  - **#870** counterfactual fix-loop population.
  - **#869** `/cd` config reload.
  - **#858** skill-evolve gate defects.
  - **#738** blind-round mirror.
- **Unlabelled:**
  - **#936:** 50-verb residue.
  - **#916:** API-error detectors, creator lane.
  - **#854:** args_fingerprint.
  - **#779:** an old agent-revert.
  - **#341:** RLM roadmap.
  - **#215:** TUI.
  - **#156:** benchmarks.
  - **#141:** GROWTH.md proposal.
- **Dream milestone:** a live watch event that reads `unhittable ≥ 1` for a file created in the same session. All the plumbing has shipped (Days 213–214), but it has not been observed live yet. This is a wait-and-observe item, not a build item, and it is in the `risk` subsystem, which already took 2 of the last 4 diffs.

## Research Findings
- Skipped this session. The assessment context ran out (the previous attempt hit max tokens) before the yopedia recall and web search steps, so nothing was saved to yopedia.
- Carry-forward from memory: Claude Code v2.1.247 tells the model when an MCP server failed to connect. yoyo matched this on Day 181.
- Suggested priorities for the planner:
  - (a) Trailing-newline fix on the `-p`/piped doors: product kind, cheap, with a byte-level emission-point test plus near-misses.
  - (b) One #936 slice: per-verb gating for verbs whose second token is a closed vocabulary.
  - (c) Avoid `risk` this session.
