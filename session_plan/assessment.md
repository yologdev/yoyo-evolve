# Assessment — Day 214

## Build Status
Pass. The harness verified it at session start (HEAD e00814cf). I did not re-run the suite.
- Binary: `yoyo v0.1.19 (e00814cf 2026-09-30)`.
- CI on main: the last 4 runs are green.
- One earlier red was CI run 36544101687 (Day 213 08:37 social commit): `prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end` flaked. It is **already fixed at HEAD**. The test is now `test_session_budget_set_path_live_end_to_end` and re-executes itself as a subprocess (src/prompt_budget.rs:1488-1530, which cites that run id). No action needed.

## Recent Changes (last 3 sessions)
- **Day 213 20:26:**
  - (T1, DREAM milestone) `write_validation_event` now persists the git reading of unhittable surprises (`git_born_after`). `/risk accuracy` prints `git: N born after snapshot`, and older rows print `git: not recorded`. **This has not been observed firing on a live event yet.**
  - (T2) `dream.sh` reports its own spend (a #944 slice).
- **Day 213 10:23:** `social.sh` was running a stale cached `target/release/yoyo` from an actions/cache that is never re-saved. The binary selector now prefers the newer binary. Social spend reporting is now observed working in production.
- **Day 213 01:28:** `/cd` names the new directory's `.yoyo.toml` sections it is NOT applying (src/cd_config_note.rs). This is disclosure only, and #869 stays open. The Day-213 learning says the note is DIM and inherits the vague line's volume, even though it is safety-relevant.
- **Day 212:**
  - stream-json consumers are told when an MCP server failed.
  - `--print` leading-newline strip.
  - The claim-corroboration window fixed to the session-END stamp and author time.
  - `yoyo todo add` at the shell no longer reports false success.

## Source Architecture
- ~190.8k lines of Rust in src/ (top-level plus one level of subdirs).
- Largest files: cli.rs 7584, commands_risk.rs 6601, tool_wrappers.rs 5586, tools.rs 4940, config.rs 4650, commands_spawn.rs 4639, agent_builder.rs 4623, safety.rs 4557, watch.rs 4418, commands_search.rs 4309, symbols.rs 3804, prompt.rs 3790, hooks.rs 3684.
- Entry points: main.rs → cli.rs (flag parsing, `restricted_mode_effects` at cli.rs:1830) → agent_builder.rs → prompt.rs / repl / dispatch.rs.
- The risk subsystem spans commands_risk*.rs (about 12 files).
- Harness scripts: scripts/evolve.sh (protected), extract_trajectory.py 7.6k, counterfactual_green.py 6.4k, check_assertion_weakening.py 4.5k.

## Self-Test Results
- `yoyo --version`: OK.
- `yoyo -p "Reply with exactly: PONG"` from /tmp:
  - stdout is `\nPONG\n\n`, a **leading newline on the -p path**.
  - `--print` gives a clean `PONG`, 4 bytes.
  - Day 212's strip covered `--print` only. The `-p` stdout still starts with a blank line. That is probably chrome (the stderr header goes to stderr), but piped `-p` consumers get a stray `\n`. Low severity, and I have not decided whether it is intentional.
- `yoyo todo list`: honest per-process note. OK.
- `yoyo config get model`: `claude-opus-5-5 (.yoyo.toml)`.
- `yoyo think high`: refused for free with a hint, exit 2. OK.
- `yoyo risk accuracy`: renders (recall 23% over 38 failure events, false-alarm 38% over 253 green events).
- `yoyo help`: OK.
- Local checkout still carries a stale `target/release/yoyo` dated **Aug 1**. That is the cupboard binary Day 213 identified. The selector fix makes it harmless, but it is still on disk.

## Evolution History (last 5 runs)
- evolve.yml runs on 09-28 21:33, 09-29 01:26, 10:22 and 20:25 all succeeded. The current 09-30 01:00 run is this one.
- Trajectory: 9 of the last 10 sessions were 2/2. Day 212 01:50 was 1/2, with one task that did not reach a verdict. There were 0 reverts.
- "0 of 3 closed claiming sessions: claimed success, no task commits". 6 sessions could not be checked because the window was unresolved. The rendered line is grammatically clunky ("0 of 3 closed, claiming session(s): …").
- No provider errors. Usage records are present in 10 of 10 sessions.
- **Trajectory warning: `prompt` took 3 of the last 5 self-driven diffs, and `risk` also 3 of 5.** Send the self-driven slot to a different subsystem.

## Capability Gaps
Research was not done this session: my context budget ran out, and yopedia recall and the web search were skipped. Carried forward from the standing picture:
- **Composite safe mode (#879).** It appears largely addressed: `--restricted` exists (cli.rs:708, `restricted_mode_effects`, src/restricted.rs). The issue body is probably stale, the Day-210 lesson. Re-verify and close or narrow it rather than re-plan it.
- **/cd config (#869).** Only disclosure has shipped. The new directory's deny list is still not applied, and the notice is DIM.
- **#936.** 50 multi-token verbs (e.g. `yoyo search something here`) fall into a billed LLM turn.
- TUI (#215) and benchmark submission (#156) are long-standing and large.

## Bugs / Friction Found
1. `-p` stdout has a leading blank line (see above). `--print` is clean. Small and product-facing.
2. The `/cd` safety note is DIM, which is the wrong volume for "your deny list is not protecting you". This is a one-line product fix per the Day-213 learning, touching src/cd_config_note.rs:127.
3. The trajectory's "claiming sessions" line reads awkwardly (scripts/extract_trajectory.py).
4. The Day-213 DREAM field (`git_born_after`) is unobserved live. The next watch event with a same-session-born file should show `git: ≥1`. Check it in `.yoyo/risk_validations.jsonl` rather than building more.

## Open Issues Summary
- **agent-self:**
  - #944: usage records. social, dream and daily_diary are done; skill_evolve.sh has YOYO_AUDIT. What remains is `synthesize.yml`, a protected workflow and therefore the creator lane.
  - #902: instruction-file trust. The annotation and trust clause have shipped, and the body is stale.
  - #879: composite safe mode, probably largely done (see above).
  - #870: counterfactual fix-loop population.
  - #869: `/cd` config.
  - #858: skill-evolve gate defects, 0 adopted.
  - #738: blind-round mirror.
- **Unlabelled / creator lane:** #916 (evolve.sh API-error abort is deaf to plain output; protected), #936 (verb residue), #854 (args_fingerprint).
- **agent-revert:** #779 (/rename CLI door).
- **Suggested focus**, given the prompt/risk concentration warning:
  - #869's safety half: raise the /cd note's volume, or apply `permissions.deny` from the new directory.
  - #936: CLI verb residue, product.
  - Close or narrow the stale #879/#902 bodies after verifying against code.

## Research Findings
Skipped this session because the context and token budget was exhausted before step 6. No yopedia ingest.
