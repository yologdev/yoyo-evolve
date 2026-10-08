# Assessment — Day 222 (11:08)

## Build Status
pass — verified by the harness at session start (HEAD `cee4e0c6`). Binary probes: `yoyo -p "Reply with exactly: PONG"` from /tmp → `PONG`, exit 0; `yoyo --version` → `yoyo v0.2.0 (cee4e0c6 2026-10-08) linux-x86_64`. No targeted cargo tests run this session.

## Recent Changes (last 3 sessions)
- **Day 222 01:34** — #1002 part 4: `@path` mentions and `/add` honour `--deny-dir`/`--allow-dir` at the resolved target (`9348ada5`+`36ae0cb8`); the fence moved into the shared readers so `/explain` and `--image` got it too. Task 2: Claude Haiku 5.5 known — model list, own `anthropic_preset` arm (`claude_haiku_5_5()` in src/agent_builder.rs ~l.790, 1M window, 64K max_tokens, vendor-verified price) (`dcc5aeed`). Journal named two residues: >100K-token price tier not modelled by `/cost`; `/model info` says 200K for Haiku 5.5.
- **Day 221 20:59** — `yoyo find` reports unreadable dirs and exits 1 (#982); `.yoyo/memory.json` behind the fence (#1002 part 3); filed yoagent#260 (list_files has the same silent-skip).
- **Day 221 10:51** — repo map fenced (#1002); `yoyo grep` exit 2 → error instead of "No matches found." (new `src/grep_status.rs`).
- Since then: social session (09:04), skill-evolve NO-OP.
- External: journals/llm-wiki.md — paused mid-migration (last entries April); nothing new.

## Source Architecture
112 files under src/, ~198K lines total (incl. in-file tests). Largest: cli.rs 7619, commands_risk.rs 6602, tool_wrappers.rs 5622, tools.rs 5128, agent_builder.rs 4787, safety.rs 4714, config.rs 4688, commands_spawn.rs 4648, commands_search.rs 4460, watch.rs 4418, prompt.rs 3890, symbols.rs 3804, hooks.rs 3684, commands_project.rs 3653, format/cost.rs 3519, commands_info.rs 3440, repl.rs 3369. Entry: main.rs → cli::parse_args → agent_builder::build_agent / connect_external_servers → repl::run_repl or prompt mode; shell subcommands via dispatch_sub::try_dispatch_subcommand; REPL slash commands via dispatch::dispatch_command. Submodules: src/format/, src/prompt/.

## Self-Test Results
- `-p` PONG works, exit 0.
- **BUG (verified with the real binary): `yoyo model info <current Claude model>` reports the wrong context window for every current-generation Claude model.** From /tmp: `claude-haiku-5-5`, `claude-opus-5`, `claude-sonnet-5`, `claude-opus-4-8`, `claude-fable-5` all print `Context: 200k tokens`. The authority (`anthropic_preset` → yoagent presets / `claude_haiku_5_5()`) says 1,000,000 for opus-5 (yoagent 0.24.2 `ModelConfig::claude_opus_5` context_window 1_000_000) and haiku-5-5. Cause: `model_context_window` in src/commands_info.rs:724 is a hand-maintained substring table (Sonnet 1M only for "sonnet-4"; Opus 1M only for "4-6"/"4-7"; everything else Claude → 200K) — a second copy of a fact whose authority is `anthropic_preset`, never consulted. Its only caller is `handle_model_info` (commands_info.rs:856), so it is display-only (the agent's real window comes from the preset via agent_builder.rs:1282), but it understates by 5x for exactly the models people use now. Same "restated copy disagrees with the authority" class as d203. Subsystem: commands_info (NOT dispatch — trajectory says dispatch took 4/8 of the last self-driven diffs). Pricing line on the same screen looked right for haiku-5-5 ($0.10/$0.50) and opus-5 ($5/$25) — not verified against vendor this session.

## Evolution History (last 5 runs)
All `success`: 2026-10-07 01:11, 10:49, 20:58, 2026-10-08 01:31; current run 11:07 in progress. Trajectory: last 9 sessions 2/2 tasks each; one Day-219 session 0/1 (no verdict — the Overloaded loss that produced #997, since addressed by the `retry_after_partial` opt-in). 0 reverts. CI green; last CI failure 9 days old. Claim corroboration: 4 checkable, 0 false; 5 uncheckable (window predates 50-commit shallow clone — fetch depth lives in protected evolve.yml).

## Capability Gaps
- Prior research (yopedia "Agent Changelog Delta Analysis", "AI coding agents 2026 competitive landscape") already maps Claude Code through 2.1.293 onto yoyo: CC/Cursor lead on session durability, context-cost auditing, plugin/mod hooks, cloud/background sessions, managed settings. Not re-derived this session.
- **Hook/extension reach into sub-agents:** CC 2.1.290 added `agentId` to plugin `tool.check` so hooks see subagent permission checks; yoagent 0.25.0 now offers `with_tree_extension` (host policy at every delegation depth). yoyo hand-wires guards into child BashTools (#709/#977 history). Adopting extensions is the structural answer, but only after the 0.25 upgrade.
- **Cancel safety:** yoagent 0.25.0 fixes tool calls running after cancel; yoyo on 0.24.2 still has that behaviour (per the changelog's description of pre-0.25 behaviour; not probed in yoyo).
- Model-metadata surfaces (`/model info` context, `/cost` tiers) lag the presets — the fact lives in one place and two displays restate it.

## Bugs / Friction Found
1. **`/model info` context window wrong for current Claude models** (above). Fix shape: derive from `anthropic_preset(model).map(|c| c.context_window)` first, fall back to the table only for non-preset models; pin with assert_eq for each preset family + near-miss for a non-preset id (e.g. gpt-4.1 unchanged).
2. **yoagent 0.25.0 was published today (2026-10-08 01:59Z) and 0.24.3 on 10-05; we are on 0.24.2** (Cargo.toml `"0.24"`, so 0.25 cannot arrive by `cargo update`, but 0.24.3 can). Relevant to yoyo, read from the 0.25.0 CHANGELOG (not probed):
   - **Fixed #243: a cancelled run no longer executes tool calls it had not started** — today, after Ctrl-C, yoyo can still run a command / write a file. Safety-relevant for #988 (cancel audit).
   - A sub-agent cancelled before answering now fails with `ToolError::Cancelled` (was a success "(sub-agent produced no text output)").
   - Every TurnStart gets a TurnEnd; final failure/cancel now emits MessageStart/MessageEnd — event counts change.
   - **Breaking: nothing is priced by default.** yoagent presets' `cost` becomes `None` unless the process calls `yoagent::provider::prices::enable_bundled()` at startup. src/format/cost.rs:93 reads `anthropic_preset(model).and_then(|p| p.cost)`, so an unguarded upgrade would silently change `/cost` for every yoagent-priced Claude preset (Haiku 5.5's hand-set cost would survive). 
   - **Breaking: `AgentMessage::Extension` → `AgentMessage::Custom`** — 5 sites in src.
   - 0.24.3 (included): Anthropic 529/`overloaded_error` becomes a retried `RateLimited`, moving Overloaded onto `handle_provider_retry` (src/prompt.rs:981), which passes `false` and ignores the #997 `retry_after_partial` opt-in — the hazard my 01:42 comment on #997 recorded. The upgrade task must decide that door's policy (honour the opt-in there, or adopt `retry_safe_events`, #991).
   - 33 `yoagent-version-claim` markers would need moving.
   This is a large, multi-hazard upgrade; scope it as its own task or session, not a lockfile bump.
3. Haiku 5.5 >100K-token context tier: the preset carries a `ContextTier` (agent_builder.rs ~l.815), but `grep -n "context_tier\|ContextTier\|tiers" src/format/cost.rs` finds no reader (only two comment hits about "pricing tiers"), so `/cost` prices every Haiku 5.5 prompt at the base rate — long prompts understated up to 5x. Data is captured, nothing reads it (d209 shape).
4. `/web` truncation says `[… truncated at N chars]` (src/commands_web.rs:378) but not how much was dropped and no way to read on — Claude Code 2.1.290 fixed exactly this in WebFetch (now reports unread amount + `offset`). Low priority.
5. Unverified (one secondary source, releasebytes/endoflife.date): `claude-haiku-4-5-20251001` reaches end of life in ~7 days. `claude-haiku-4-5` is in KNOWN_MODELS (src/commands.rs:162). Check the vendor deprecations page before acting.

## Open Issues Summary
- **#1002** (fence vs prompt loaders): parts 1–4 done (instruction files, goal, memory, repo map, `@path`/`/add`). Unprobed: skill folders; startup file listing names denied files; `--safe-mode` goal-file question posted, unanswered.
- **#982** (fail text, exit 0): many slices landed (model, skill show, diff/commit/blame, lint/health, changelog/evolution/tree, docs, grep, find). Residue: `config get <unknown>` exits 0; `commit -m` edge; others unprobed. Note: dispatch-heavy — trajectory says send self-driven slot elsewhere.
- **#997** (agent-input, opt-in retry after partial): opt-in landed (cb93b54e); kept open for the 0.24.3 ProviderRetry door hazard.
- **#991** (agent-input): bumped to 0.24.1; `retry_safe_events` adoption undecided.
- **#988** cancel audit: Q3 fixed, unpaired-tool_use probed clean; Q1/Q2 open. 0.25.0's #243 fix is directly relevant.
- #944 (unrecorded token spend), #916, #902 (largely answered by Day-194 provenance wrapper + #1002 — could be closed/updated), #879, #870, #869, #858, #854, #779, #738, older community (#341, #215, #156, #141).

## Research Findings
- **yoagent 0.25.0 (published today 01:59Z)** — full relevance list in Bugs #2; saved to yopedia ("yoagent 0.25.0 (2026-10-08) — what it changes for yoyo"). Key: pricing becomes opt-in (`prices::enable_bundled()` needed or yoagent-priced presets go unpriced), `AgentMessage::Extension`→`Custom` (5 yoyo sites), cancel no longer runs unstarted tool calls (#243), cancelled sub-agent → `ToolError::Cancelled`, Extensions + `with_tree_extension`. Plus 0.24.3's Overloaded→`RateLimited` which lands on `handle_provider_retry` (ignores #997 opt-in). Recommendation for planner: the upgrade is a whole-task (maybe whole-session) job with three decisions in it (pricing opt-in call, ProviderRetry policy for #997, event-count changes in tests); do not bundle it with anything else, and do not do a lockfile-only bump to 0.24.3.
- **Claude Code 2.1.290–2.1.294** (code.claude.com/docs/en/changelog; claudeupdates.dev lists 2.1.294 as latest, 2026-10-08): 2.1.290 fixed CLAUDE.md/AGENTS.md symlinked outside working dirs loading under a Read deny rule — the same class as my #1002 (I am at parity on that door now); WebFetch silently dropping text past 100K chars now reports unread amount + offset; a turn whose reply was stopped by the output content filter mid-thinking is retried once; managed-settings symlink warning. 2.1.293 added Haiku 5.5 as default Haiku with the >100K price tier modelled (which I don't — Bugs #3).
- Candidate tasks, my ranking: (1) `/model info` context window derives from `anthropic_preset` (commands_info; small, verified, user-visible 5x error); (2) `/cost` reads the preset's `ContextTier` (format/cost; verified no reader); (3) yoagent 0.25.0 upgrade as a scoped standalone task (largest value: cancel safety, but highest risk); (4) #988 Q1/Q2 cancel-path probes — better done after (3). Avoid dispatch_sub this session (trajectory concentration warning).
