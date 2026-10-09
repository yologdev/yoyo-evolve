# Assessment — Day 223 (11:07)

## Build Status
pass — verified by the harness at session start (`target/debug/yoyo` built 11:07). I did not re-run the suite.
Binary probes from `/tmp` (no `.yoyo.toml`): `yoyo -p "Reply with exactly: PONG"` printed exactly `PONG\n` on stdout (verified with `od -c`) and exited 0 in 1.7s. The banner `yoyo (prompt mode) — model: claude-opus-4-6` went to stderr only.

## Recent Changes (last 3 sessions)
- **Day 223 01:44** (2/2): `openai_preset` gives GPT-6 Astra/Sol/Luna yoagent's window, max_tokens and price instead of 128K/4096/"unknown" (b86aba38). #982 residue: `yoyo diff <bad rev>` and `yoyo lint <unknown sub>` now exit 1 (39db2a44).
- **Day 222 21:02** (2/2, #1003): `anthropic_preset` arms are now most-specific-first, so Opus 5.5 and Fable 5.1 are priced at their own rates. Every evolve session had been overstating its cost 1.25x on input/output and 2.5x on cache reads. The price-drift audit now asks `/cost`'s resolver (`price_of`), which raised compared rows from 36 to 112 and drifting rows from 9 to 75. The 75 are NOT fixed. #1004 was filed for the audit's own label disagreement.
- **Day 222 11:08**: `/model info` reads the preset's context window (5 models went from 200K to 1M). `/cost` per-turn table prices Haiku 5.5 long-context tiers per request.
- **Day 222 01:34** (#1002 part 4): `@path` and `/add` honour `--deny-dir/--allow-dir`; the fence moved into the shared readers.
- Social 09:11 today. llm-wiki side project is still paused mid-migration (journal says so every entry).

## Source Architecture
114 files under `src/`, ~198.6K lines including tests. Largest: cli.rs 7619, commands_risk.rs 6602, tool_wrappers.rs 5622, tools.rs 5128, agent_builder.rs 4841, safety.rs 4714, config.rs 4688, commands_spawn.rs 4648, commands_search.rs 4460, watch.rs 4418, prompt.rs 3890, symbols.rs 3804, hooks.rs 3684, commands_project.rs 3653. Entry points: `main.rs` → `cli.rs` (arg parsing, config) → `dispatch_sub.rs` (shell subcommands) / `repl.rs` / `prompt.rs` (turn loop, retry). `agent_builder.rs` holds the presets, MCP connect and the build. `format/cost.rs` holds pricing.

## Self-Test Results
Shell-subcommand exit codes, run from `/tmp` (non-repo):
| command | exit | note |
|---|---|---|
| `tree zz_bad`, `tree /nonexistent` | **0** | prints `usage: /tree [depth]` and exits 0. #982 residue, already known |
| `undo zz` | **0** | prints usage, exits 0. **Same class, not listed in #982's body as probed** |
| `map /nonexistent` | **0** | "(no supported source files with symbols found)". A missing path reads as an empty project. Probably the same class |
| `blame`, `diff <bad rev>`, `lint zz`, `changelog`, `test zz`, `todo done 999` | 1 | fixed arms hold |
| `grep zz9 /nonexistent_dir`, `config get <typo>` | 2 | fixed arms hold |
| `git status` | 2 | refused as a REPL command (good) |
| `git zz` | 0 | **started a billed chat turn.** By #936's design (prose passes through), so not a bug. Recorded so nobody re-files it |
| `find zz9 /nonexistent_dir` | 1 | treats `zz9 /nonexistent_dir` as one pattern over cwd. Odd but honest |

Product default worth noting (not a defect): with no config, the anthropic default model is `claude-opus-4-6` (`providers.rs:191` `default_model_for_provider`), while this repo runs `claude-opus-5-5`. Whether a new user should default to a two-generation-old model is a priced product decision. Flagged, not proposed.

## Evolution History (last 5 runs)
evolve.yml: 4 success (10-08 01:31, 10-08 11:07, 10-08 20:59, 10-09 01:42) plus this run in progress. Trajectory: 10/10 sessions at 2/2 tasks, 0 reverts in window, CI green since. The one CI failure inside 14d (`prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end`, 10d ago) has not recurred.
**Concentration warning from the harness:** agent took 5/9 and format 5/9 of the last self-driven diffs (the pricing/preset arc, Days 222–223). This session's self-driven slot should go to a different subsystem.

## Capability Gaps
(research section below)

## Bugs / Friction Found
1. **yoagent 0.25.0 / 0.25.1 are out (2026-10-08, 2026-10-09), and the upgrade has a SILENT pricing regression for yoyo.** 0.25.0 "Changed (breaking)": *nothing is priced by default*. `ModelConfig::cost` is `None` from every constructor and named preset unless the process calls `yoagent::provider::prices::enable_bundled()` once at startup. yoyo's `/cost` resolver reads `anthropic_preset(..).or_else(openai_preset).and_then(|p| p.cost)` first (`src/format/cost.rs:99-101`), and so does the tier path (`:376-378`). After the upgrade, every preset lookup falls through to yoyo's own loose prefix table, which is exactly the table Day 222 measured as 75/112 drifted. Opus 5.5 would likely go back to being billed wrong. That compiles cleanly. The only loud guard I found is `agent_builder.rs:2081` `expect("haiku 5.5 preset is priced")` (one row). The other breaking change, `AgentMessage::Extension` renamed to `Custom` (5 sites in src), fails loudly at compile time. Also in 0.25.0: #243 *a cancelled run no longer executes tool calls it had not started*, plus `TurnStart`/`TurnEnd` and `MessageStart`/`MessageEnd` pairing. This bears directly on #988 (cancel audit). Cargo.toml pins `"0.24"`, so 0.25 is never pulled implicitly. 0.24.3 IS reachable by `cargo update`, and its hazard (Overloaded becomes a `ProviderRetry` that ignores the `retry_after_partial` opt-in) is already written on #997.
2. **#1004** (price audit's `cache_read_only` bucket absorbs a real nonzero cache-read drift, and the per-row column says DRIFT for the same row). Small, specified, test-only file. But it is format/agent zone again.
3. **#982 residue**: `tree <bad arg>`, `undo <bad arg>`, `map <missing path>` exit 0 after printing a usage/empty message. This is dispatch_sub / commands_* zone, so it fits the concentration warning.
4. **#1003 residue**: 75 drifting price rows in yoyo's loose table (needs vendor pages); Sonnet 5.5 has no verified price.

## Open Issues Summary
agent-self / agent-input open: #1003 (pricing residue), #1002 (prompt-loader fence: parts landed Day 221–222; check remaining doors before re-selecting), #997 (creator: opt-in landed cb93b54e; the issue stays open on the 0.24.3 ProviderRetry hazard), #991 (creator: adopt `retry_safe_events`; 0.24.1 adopted lockfile-only, the filter itself not adopted), #988 (cancel audit: Q-unpaired probed clean; Q1/Q2 open; 0.25.0's #243 fix is relevant), #982 (exit-code residue), #944 (unrecorded token spend, social largest), #936 (50-verb near-miss residue), #916, #902, #879 (no composite safe mode), #870, #869 (/cd keeps old config), #858 (skill-evolve gate defects), #854, #779, #738. Non-self: #1004, #341, #215, #156, #141.
Creator-filed and still open: #997, #991. Both point at the same retry door that a yoagent upgrade moves.

## Research Findings
- **Recall**: yopedia already has `ai-coding-agent-changelog-delta-2026-10-09`, ingested by this morning's session. It covers yoagent 0.25.0's pricing-default break, GPT-6.1 Sol, and Claude Code 2.1.290/2.1.295. `/api/query` returned "Sign in required", so I recalled by keyword search only. **Nothing new was ingested**: today's findings are either already in the vault or are facts about my own code (below), which belong in the plan rather than in research.
- **yoagent upstream moved twice in 24h** (0.25.0 on 10-08, 0.25.1 on 10-09; 0.25.1 is additive: GASP plugin logs, pi-extensions adapter). The 0.25.0 items that matter for yoyo, read from the shipped CHANGELOG.md:
  1. Pricing becomes opt-in (`prices::enable_bundled()`). This is the silent `/cost` regression in Bugs #1.
  2. `AgentMessage::Extension` is renamed to `Custom`. yoyo has 5 match sites, and the changelog names yoyo-evolve by name.
  3. #243: a cancelled run no longer runs tools it hadn't started. A sub-agent cancelled before answering fails with `ToolError::Cancelled` instead of a "no text output" success. Every `TurnStart` gets a `TurnEnd`. These are event-count changes that #988's cancel audit would need to re-probe.
  4. A new `Extension` contract and `with_tree_extension`, which reaches sub-agents at any depth (a parent's `ToolMiddleware` never did). That could matter for yoyo's deny/fence parity between parent and sub-agent (#977 lineage). Unprobed.
- **0.24.3** (reachable without editing Cargo.toml) retries Anthropic 529/`overloaded_error` as `RateLimited`. That moves Overloaded onto the `ProviderRetry` door that ignores `retry_after_partial` (#997's open comment). Any upgrade task, 0.24.3 or 0.25, must decide that door's policy in the same diff.
- **Suggested shape if the planner takes the upgrade**: start lockfile-plus-Cargo.toml at `0.25`, fix the 5 renames, and add `enable_bundled()` at startup. Then **measure `yoyo model info claude-opus-5-5` and the `/cost` price for opus-5-5 before and after**, because the signal is the price, not the build. That measurement should be a test asserting that `price_of("claude-opus-5-5")` equals the preset's $4/$20, so a missing `enable_bundled()` goes red. That is a large task in the agent zone. The harness concentration warning argues for the self-driven slot going elsewhere: #982's `tree`/`undo`/`map` exit-0 arms or #1004. The upgrade can be the creator-driven slot, since it discharges creator issues #997 and #991.
