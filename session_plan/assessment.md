# Assessment — Day 236

## Build Status
Pass — verified by the harness at session start (HEAD af749bed). Binary `target/debug/yoyo` reports `v0.3.0 (af749bed 2026-10-22)`. CI green on the last 4 runs; evolve runs green 5/5 (the 6th is this one, in progress).

## Recent Changes (last 3 sessions)
- **Day 235 21:41**: `Fixes #1055`: the outside-read fence default is now *decided* (on at a terminal) per @yuanhao's answer; "provisional" dropped from CHANGELOG/v0.3.0 notes (no code). `Part of #1064`: key presence reads yoagent's resolver, so `GEMINI_API_KEY` alone starts yoyo on google. Residue the journal names: 23 one-name key checks across 9 files, including the auth hint (still says `GOOGLE_API_KEY` only).
- **Day 235 19:56**: `yoyo undo --all` checks the disk afterwards and names survivors on ⚠ instead of ✓ (#1065). `Fixes #1011`: with a deny list set, `eval "$(...)"` is refused (cost written into the refusal; carve-out question filed as #1066).
- **Day 235 14:52**: opt-in `YOYO_SUBPROCESS_ENV_SCRUB=1` strips provider keys from model bash (`Part of #1064`); `yoyo undo` outside a repo exits 1 (#1059).
- Last 10 sessions: 20/20 tasks green, 0 reverts. Trajectory warning: **dispatch took 5 of the last 10 self-driven diffs — send the self-driven slot elsewhere.**
- llm-wiki (external): still paused mid-migration (last entry 2026-05-04, StorageProvider migration).

## Source Architecture
134 files in `src/`, ~213K lines total (incl. tests). Largest: cli.rs 7752, commands_risk.rs 6602, tool_wrappers.rs 5696, tools.rs 5247, agent_builder.rs 4962, config.rs 4851, safety.rs 4720, commands_search.rs 4582, commands_spawn.rs 4576, watch.rs 4486, prompt.rs 3905. Entry: `main.rs` → `cli::parse_args` → `dispatch_sub::try_dispatch_subcommand` (shell verbs) or `agent_builder` (build agent + `connect_external_servers`) → `prompt.rs` turn loop. yoagent = 0.25 (lock 0.25.12).

## Self-Test Results
Probed with dummy keys in a fresh `HOME`:
- `--provider zai --model glm-5.3`: warns "Unknown model … Known models: glm-4-plus, glm-4-air, glm-4-flash" (stale list) **and** "no preset is known … guessed limits: 128000 window, max_tokens = 4096". But yoagent 0.25.12 ships `ModelConfig::glm_5_3()` (and `glm_5_3_flash`). Same for `--provider openai --model gpt-5.5` → guessed 4096 cap, while yoagent has `gpt_5_5`. `gpt-6.1-sol` correctly gets no warning.
- `--provider qwen`: "Unknown provider 'qwen'", treated as OpenAI-compatible on localhost:8080 and retries. yoagent has `ModelConfig::qwen` generic + `qwen3.8-max`/`qwen3.8-flash` presets. (Adding a provider is bigger; note only.)
- `--provider openai --api-key dummy -p hi` → real 401, then hint "OPENAI_API_KEY is not set … or use --api-key <key>" — false, the user did pass `--api-key`. **#1062's second door still live** (`src/prompt_retry.rs:524` decides `key_set` from `std::env::var` alone).
- Piped with no stdin: "No input on stdin." fine.

## Evolution History (last 5 runs)
All 5 completed evolve runs succeeded (2026-10-21 00:21 → 21:38); no failures, no reverts. CI recurring errors (config_fifo tests) predate a green; the env-scrub fix landed Day 233.

## Capability Gaps
- **Model tables drift vs yoagent** (the most concrete, creator-specified): yoagent 0.25.10+ exposes `ModelConfig::preset(provider,id)`, `generic(provider,id)`, `presets()` (22 presets). yoyo keeps three hand lists in `src/agent_builder.rs` — `anthropic_preset` (:824, prefix match), `openai_preset` (:864, 4 ids), `vendor_preset` (:880, 4 ids) + `vendor_preset_by_id` (:893). Missing vs yoagent: `gpt-5.5` (+ other openai rows), `glm-5.3`, `glm-5.3-flash`, `qwen3.8-*`. Users on those ids silently get a 4096-token output cap (measured above). This is CLAUDE.md's "don't reinvent the wheel" rule. 53 references to these fns outside their definitions.
- Default models per provider still stale (#1037 rest, #1021 google).
- Product parity items still open: reconnecting MCP survivors after a failed connect; /cd into trusted project not loading hooks/MCP/skills (#1040).

## Bugs / Friction Found
1. glm-5.3 / gpt-5.5 (and any yoagent preset id not in yoyo's tables) run on guessed 4096 cap — #1037 (creator's 21:30Z comment gives the exact plan and caveats: keep a `-YYYYMMDD` normalization for dated Anthropic ids, pin both ways; keep per-provider overrides; keep vendor-number tests as independent check; `presets()` can drive a test that every default resolves to a preset).
2. zai "Known models" list stale (glm-4-*) — `src/providers.rs` `known_models_for_provider`; same class as #1037/#1021.
3. Auth-failure hint says "not set / use --api-key" when the key came from `--api-key` or config (#1062, second door: `prompt_retry.rs:524`). First door: wizard not skipped with `--api-key`.
4. #1064: decision released back to the loop by @yuanhao (21:35Z, via my 22:45Z correction): either split the default (scrub MCP stdio + repo hooks by default; bash/`/run`/`/bg`/`/watch` stay opt-in) or keep all opt-in with a present-tense reason answering his MCP/hooks point. His near-miss tests: `bash` byte-identical by default; non-credential var still reaches an MCP server and a hook.
5. Google key residue: 23 one-name key checks in 9 files (journal count, Day 235), incl. auth hint naming only `GOOGLE_API_KEY`.

## Open Issues Summary
agent-self open: #1066 (eval ssh-agent carve-out decision), #1064 (scrub default, decision now ours), #1063 (/bg,/watch wait on grandchild EOF), #1062 (--api-key invisible to wizard + hint), #1060 (openai wire residue), #1057 (/spawn sweep removes live worktree), #1054 (yoagent 0.25.9 extra tool-result blocks, no request-level test), #1045 (/doctor context row), #1041 (MCP prefix residue), #1040, #1037 (presets → yoagent; creator plan), #1026, #1023, #1021, #1018 (durable todos, @barneysspeedshop design), #1016, #1014 (spawn write tap, promised in #734), #1012 (price drift 75 rows), #944, #902. agent-input: #991 (retry_safe_events).

Selection notes for the planner:
- #1037 is creator-specified with exact file:line and caveats → a strong product task; touches agent_builder/providers (not dispatch — satisfies the concentration warning). Title should be `Part of #1037` (default-model choice remains). Before "Fixes", check `git grep -n '#1037'` per d231.
- #1062 hint door is small and measured (above); fix in prompt_retry.rs needs the key's source threaded or a "sent key" flag.
- #1064 split-default is a real decision now owed by the loop; plan must state placement + caller lines (evolve.sh runs `yoyo -p` through bash, which stays opt-in in the split, so the loop is unaffected).

## Research Findings
Recall (yopedia): prior notes exist on changelog deltas through 2026-10-09, cost/config observability, and subprocess credential scrubbing (already informs #1064). Fresh read of the Claude Code feed (v2.1.305–2.1.307, 2026-10-20/21):
- **HTTP MCP tool calls were sent twice when a connection closed mid-call; now only read-only/idempotent tools retry.** yoyo has no HTTP MCP path (stdio only), so not directly applicable; relevant to #991 retry-safety thinking only.
- **"Claude named the wrong model for a subagent's work after the subagent switched to a fallback model."** yoyo's `sub_agent_model_label` (`tool_wrappers.rs:1699`) honestly says "which of the two produced this error is not observable" on the *error* path; whether the *success* path names the model that actually answered after a fallback is unprobed. Possible small probe, not a selected bug.
- **An API error was replaced by "prompt is too long" + an unneeded compaction in sessions with many MCP tools** (classifier masking a real error) — same class as yoyo's error-classifier work; not probed here.
- `allowedMcpServers` no longer filters managed stdio servers using `${VAR}` only in env — env-only config as a filter blind spot.
- Biggest concrete gap found this session isn't a rival feature but dependency drift: yoagent already has the preset registry (`preset`/`generic`/`presets`) and yoyo's three hand tables lag it, so real current ids (gpt-5.5, glm-5.3, glm-5.3-flash) run on a 4096-token guessed cap. Saved to yopedia (job 6bbe52e0).

## Suggested priorities (for the planner; not task files)
1. `Part of #1037`: route `anthropic_preset`/`openai_preset`/`vendor_preset` through `ModelConfig::preset(p,id).or_else(generic)` per the creator's 21:30Z comment (keep dated-Anthropic-id normalization pinned both ways; keep per-provider overrides; keep vendor-number tests; add a `presets()`-driven test). Probe after: `--provider zai --model glm-5.3 -p hi` must print no guessed-limits warning. Watch `tests/module_size.rs` headroom on agent_builder.rs (4962 lines) — net change should be negative. Also stale zai "Known models" list in providers.rs.
2. `Part of #1062`: auth hint door — `prompt_retry.rs:524` says "not set / use --api-key" when the key came from argv/config. Small, measured on this binary.
3. `Part of #1064`: decide the split default (MCP stdio + hooks scrubbed by default; bash family stays opt-in) with @yuanhao's two near-miss tests, or keep opt-in with a present-tense reason. Loop caller line: evolve.sh runs `yoyo -p` through model bash, which stays opt-in under the split.
Avoid dispatch_sub.rs this session (concentration warning).
