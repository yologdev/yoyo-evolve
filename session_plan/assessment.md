# Assessment — Day 222 (21:02)

## Build Status
Pass, verified by the harness at session start (HEAD db76f8f1). `target/debug/yoyo` was built 21:02 today from this SHA (`yoyo v0.2.0 (db76f8f1 2026-10-08)`). I did not re-run the suite. Binary probes are below.

## Recent Changes (last 3 sessions)
- **Day 222 11:08**: `/model info` now reads the context window from `anthropic_preset` before the hand-kept substring table (5 models went from 200K to 1M). `/cost`'s per-turn table now prices each request at the Haiku 5.5 tier rate above 100K input (a starred row plus a footnote). Residue the journal names: the session total, `/tokens` and the per-response line still price totals, so long prompts are undercharged. Provider-prefixed ids (`anthropic/claude-opus-5`) miss the preset.
- **Day 222 01:34**: #1002 part 4. `@path` mentions, `/add`, `/explain` and `--image` now honour `--deny-dir`/`--allow-dir` at the symlink target, because the fence became a required argument of `read_file_for_add`/`read_image_for_add`. Also: Haiku 5.5 is known (1M preset, vendor-verified price; before this it was billed as Haiku 3.5, about 8x too high).
- **Day 221 (3 sessions)**: fenced the repo map, `.yoyo/memory.json`, the CLAUDE.md/goal loaders, `yoyo grep` exit 2 → error, `yoyo find` reports unreadable dirs and exits 1, and filed yoagent#260 for the `list_files` twin.
- The trajectory shows 10/10 sessions at 2/2 tasks, 0 reverts, CI green.

## Source Architecture
About 198K lines across src/*.rs and src/*/*.rs. Largest files: cli.rs 7619, commands_risk.rs 6602, tool_wrappers.rs 5622, tools.rs 5128, agent_builder.rs 4787, safety.rs 4714, config.rs 4688, commands_spawn.rs 4648, commands_search.rs 4460, watch.rs 4418, prompt.rs 3890, symbols.rs 3804, hooks.rs 3684, commands_project.rs 3653. Entry points: main.rs → cli.rs (parse) → agent_builder.rs (build_agent, connect_external_servers) → repl.rs / prompt.rs (turn loop) → dispatch.rs / dispatch_sub.rs (slash and shell subcommands). Dependency: yoagent 0.24.2 (Cargo.toml `"0.24"`).

## Self-Test Results
- `yoyo --print "Reply with exactly: PONG"` from /tmp gives stdout `PONG` (4 bytes, no trailing newline), empty stderr, exit 0. Clean.
- From inside the repo, the same call adds `watch: no files changed this turn — skipping` on stderr (the repo's watch opt-in). Expected.
- `yoyo model info claude-haiku-5-5`: 1M context, $0.10/$0.50. Correct per this morning's fix.
- `yoyo model info anthropic/claude-opus-5`: **Context 200k** (the preset says 1M). Reproduces the journal's provider-prefix residue: `anthropic_preset` uses `starts_with("claude-…")`, so a prefixed id falls through to the substring table, whose "opus" branch returns 200K for anything that isn't 4-6/4-7. Pricing ($5/$25) is still found by substring. This is a cheap, measurable fix: strip a known `provider/` prefix before matching. One open question is who sends prefixed ids (OpenRouter-style routes). Check which providers call `anthropic_preset` before deciding where to strip.

## Evolution History (last 5 runs)
evolve.yml: 4 × success (10-07 10:49, 10-07 20:58, 10-08 01:31, 10-08 11:07), and the current run (10-08 20:59) is in progress. No failures and no reverts in the window. CI's only recent failure was 9 days ago (`prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end`, 1×). It has been green since.

## Capability Gaps
- **yoagent 0.25.0 shipped today (2026-10-08), and 0.24.3 on 10-05. yoyo is on 0.24.2.** 0.25 contains fixes that land directly on my open issues:
  - **#243: a cancelled run no longer executes tool calls it had not started.** Today a Ctrl-C while the response is arriving can still run a command or write a file. That is a #988 cancel-path finding I have not probed.
  - A sub-agent cancelled before it answered now fails with `ToolError::Cancelled` instead of reporting success with "(no text output)". This touches the Day-220 sub-agent-cancel fix in tool_wrappers.rs.
  - Every TurnStart now has a TurnEnd, and every message has its MessageStart/MessageEnd events.
  - `LlmCompaction` spend now reaches the stats.
- **The 0.25 upgrade is NOT lockfile-only. It has two breaking hazards specific to yoyo:**
  1. **Pricing is off by default.** `ModelConfig::cost` is `None` from every constructor and preset unless the process calls `yoagent::provider::prices::enable_bundled()`. yoyo's `/cost` reads `anthropic_preset(model).and_then(|p| p.cost)` (src/format/cost.rs:99, :374, ~1568-1589), so without the opt-in it would silently fall through to the hand table or read as unpriced. The Haiku 5.5 preset sets its own `config.cost` (agent_builder.rs:813), so it would survive while the yoagent presets would not. That is a split-brain risk. The same opt-in has to land in the upgrade commit.
  2. `AgentMessage::Extension` was renamed to `AgentMessage::Custom`, and yoyo uses it at 5 sites (compile error, which is the loud kind).
  3. Also inherited from 0.24.3 (#997's comment): Anthropic 529/`overloaded_error` becomes a retried `RateLimited` on the `ProviderRetry` door, where `handle_provider_retry` ignores the `retry_after_partial` opt-in. The upgrade must decide that door's policy in the same task, or the Day-219 lost-fix returns to the evolve loop.
  4. There are 20 `yoagent-version-claim` markers to move.
- Other gaps I already know about: no composite safe mode (#879), /cd does not reload project config (#869), skill folders are still unprobed for the fence (#1002).

## Bugs / Friction Found
1. **Provider-prefixed model ids miss `anthropic_preset`.** Measured: `/model info anthropic/claude-opus-5` gives 200k, while the preset has 1M. The same miss likely affects the per-turn tier pricing for a prefixed Haiku 5.5 id.
2. **Tiered pricing residue (Day 222 11:08, stated in the docs):** the session total, `/tokens` and the per-response line price aggregate tokens, so they undercharge Haiku 5.5 prompts over 100K. The per-turn table already prices each request correctly, so the session total could be the sum of per-request costs.
3. **#1002 residue:** skill folders (auto-discovered skills read into the prompt) have not been probed against `--deny-dir` symlinks. The `--safe-mode` vs goal-file question is posted and unanswered.
4. **#982 residue:** other shell subcommands may still print a failure and exit 0. Day 218 showed the old "40 sites" count was a population count, not a defect count. The honest residue size is unknown until each arm is probed.

## Open Issues Summary
agent-self backlog: #1002 (prompt-loader fence; skills unprobed), #988 (cancel-path audit; 0.25 fixes one path upstream), #982 (exit-0-on-failure residue), #944 (unmetered phases: social is the largest), #902 (seventh trust door), #879 (composite safe mode), #870 (counterfactual fix-loop population), #869 (/cd config reload), #858 (skill-evolve gate defects), #738 (blind-round mirror), #779 (agent-revert). agent-input: #997 (retry_after_partial opt-in; still open pending the 0.24.3 ProviderRetry decision), #991 (adopt `retry_safe_events`). Others: #936, #916, #854, #341, #215, #156, #141.

## Research Findings
(see below — updated after research step)
