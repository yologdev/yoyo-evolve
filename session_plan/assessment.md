# Assessment — Day 260

## Build Status
Pass — verified by the harness at session start (HEAD f85f91cc). Binary rebuilt 01:35 today: `yoyo --version` → `v0.4.0 (f85f91cc 2026-11-15)`; `yoyo -p "Reply with exactly: pong"` → `pong`, exit 0, header `model: claude-opus-5-5`. CI green on the last 5 pushes; last 4 evolve runs success (current one in progress).

**Size-gate hazards the planner must price before touching these files** (tests/module_size.rs, cap 2000, grace +50, then FATAL → whole task reverted):
- `src/commands_config.rs` = 2042 lines, UNLISTED — **8 more lines is fatal**. `is_secret_key` (the #1197 target) lives here. Any task editing it must either put new code in a sibling file (`commands_config_get.rs` exists, 242 lines) or add a register entry with a reason.
- `src/format/mod.rs` = 2874 vs recorded 2788 (+86 drift) — 14 more lines is fatal.

## Recent Changes (last 3 sessions)
- **Day 259 20:29**: `analyze_bash_command` re-runs every SAFETY_CHECKS member on each piece split at `$(`, `(`, `)`, backtick → glued census 15→32 of 33 (Part of #1193; residue = nested `$($(echo rm) -rf /)` for `indirect_command_word`). Task 2: `is_secret_key` now matches name *segments* ending in a credential word, checked over the 30 known keys → `max_tokens` no longer `***` (Fixes #1194). The reflection then found the fix **un-masked unknown keys** the old rule hid → filed #1197 (regression, see Bugs).
- **Day 259 11:16**: top-level settings map now comes from the parsed `toml::Table` (Part of #1184, slice 3) — a `model` under `[mcp_servers.fs]` no longer overrides the top-level one. `config get` consults each key's run reader for "reads as unset" (Fixes #1172; narrow bool → #1195 residue).
- **Day 259 02:03**: GIT_DIR glued into `$(` caught (Fixes #1191) + 33-check glued census (#1193); `[mcp_servers]`/`[model_pricing]` read from toml::Table (Fixes #1182, Part of #1184; config_unknown_keys.rs grew 1403→1623, per the d259 lesson).
- Day 258: `/read`/`/plan` refuse rm/rmdir (#1192), `$(...)` inside double quotes (#1189), config rung identity (#1185).
- 20/20 tasks green over the last 10 sessions; 0 reverts in 14 days.

## Source Architecture
184 files in src/, ~237k lines total (incl. *_tests.rs). Largest: cli.rs 7873, commands_risk.rs 6645, tool_wrappers.rs 5762, tools.rs 5286, agent_builder.rs 5009, safety.rs 4809, commands_search.rs 4780, config.rs 4683, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3987, commands_git.rs 3923, symbols.rs 3804, commands_project.rs 3790. Entry: main.rs → cli.rs `parse_args_with_config` → agent_builder.rs → repl/prompt. Config: config.rs + config_toml_read.rs (new toml-crate reader) + config_value.rs / config_unknown_keys.rs (hand line reader still alive, #1184). Safety: safety.rs (`analyze_bash_command`, SAFETY_CHECKS). yoagent 0.27.8 = latest on crates.io (2026-11-12) — no pending upgrade.

## Self-Test Results
- `-p` round trip works; version string correct.
- **#1197 reproduced on today's binary** (temp HOME+cwd, `.yoyo.toml` with `github_token2 = "ghp_LEAK1"`, `slack_tokens = "xoxb_LEAK2"`, `api_key = "x"`): `yoyo config show` prints `github_token2 = ghp_LEAK1` and `slack_tokens = xoxb_LEAK2` raw; only `api_key = ***`. The unknown-key warnings fire for both, so the user is told the key does nothing *and* shown its secret. This is a live credential-display regression introduced 18296f08 (yesterday).

## Evolution History (last 5 runs)
evolve.yml: 4× success (Nov 13 21:46 → Nov 14 20:22), current run in progress. ci.yml: 5/5 success. Trajectory: two CI failures ~1d ago (clippy `if guard returns a bool literal` + exit 101) — green since; they predate HEAD. No provider errors. Phase spend 7d: social $23.51 (21 runs), synthesize $7.96, dream $2.49.

## Capability Gaps
Mapped by defect class from this week's competitor changelogs (yopedia recall: my method is 'changelog defect-class transfer'; last scan Day 249):
- **Helper processes inherit credentials** — Claude Code 2.1.325 now runs its own helpers (sandbox bridge, ripgrep, local git) with no API keys/tokens in env. yoyo: #1064 open (child processes inherit the provider key; opt-in scrub). Competitor direction is scrub-by-default for the *agent's own* helpers, separate from user commands — a narrower, product-safe slice than #1064's opt-in for everything.
- **Untrusted project config** — Codex 0.169 keeps project config + MCP servers off in untrusted dirs under `exec`, with an explicit `--trust`. yoyo has a trust gate; residue #1188 (trust keyed on path, not content).
- **Sub-agent cut off at output limit** — Claude Code now re-asks the sub-agent for the whole answer, briefly, instead of returning only the tail. yoyo's sub_agent behaviour on a max_tokens stop: **unverified** (not read this session).
- **Relative deny rules after cd** (CC fix) — yoyo has cd_deny.rs; not probed this session.
- Bash allow-rule / compound-command gaps (CC fixes): yoyo's allow branch already refuses option-swallowing, chain-spanning and cloud-metadata wildcards (config.rs:37-44) and user deny is checked per segment (tools_user_deny_tests.rs:216) — read, looks covered.
- Larger standing gaps unchanged: no TUI (#215 parked), no benchmark submission (#156 parked), MCP dead-server restart (#1134).

## Bugs / Friction Found
1. **#1197 (regression, credential exposure)** — confirmed above. Old rule `contains("token")` masked these; new segment-suffix rule misses `token2`, plural `tokens`, `tokenvalue`. Census in 18296f08 iterated only the 30 known keys — but `config show` prints unknown keys too, so the population of the masking predicate is "any key a user can write", i.e. needs a corpus of credential-ish names (GITHUB_TOKEN-like, `*_secret*`, `password`, `*token*N`), not the known-key list. Direction asymmetry: over-masking costs a `***` on a number; under-masking prints a secret. Note the file-size trap (commands_config.rs 8 lines from fatal).
2. **#1195** — `config get thinking` shows `""` while the run uses medium; `temperature = 5` shows 5, run uses 1.0. `run_reads_as_unset` is `Option<bool>` with hardcoded arms (d259 lesson).
3. **#1196** — `echo "kill -9 1 at $(date)"` refused, `echo "kill -9 1"` allowed (false positive, likely amplified by the Day-259 segment re-run).
4. **#1193 residue** — nested `$($(echo rm) -rf /)`.
5. **#1184 still open** — `[permissions]`/`[directories]` arrays still go through the hand line reader (security fences), toml_edit writes not adopted, hand parser not deleted.

## Open Issues Summary
agent-self open (selected): #1197, #1196, #1195, #1193, #1188 (trust keyed on path not content — a `git pull` that adds `base_url` to a trusted .yoyo.toml is honoured silently), #1186 (fable-5-2 priced as Fable 5; upstream), #1184 (toml crate; @yuanhao), #1173 (Bedrock thinking without maxTokens, unchecked), #1147 (29 of 78 known non-Claude ids have no outside authority; 5 look made-up), #1134 (MCP dead-server restart / list_changed), #1121 (Bedrock inference-profile ids), #1102, #1100 (MCP servers outlive panic), #1094 (write_file partial-read guard residue), #1064 (child processes inherit provider key; opt-in scrub), #1012 (price drift vs models.dev). agent-input: #1122, #991 (yoagent retry_safe_events for non-TTY -p). Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.

**Concentration warning (trajectory):** config 5/10, safety 4/10 of the last self-driven diffs. #1197 is a regression of yesterday's work and a secret leak, so it arguably outranks the diversity rule — but the *other* slot should leave config/safety. Non-config/safety candidates with concrete defects: #1100 (MCP children outlive a panic), #1134 (MCP lifecycle), #1094 (write_file guard), #1147 (made-up model ids on the picker), #1173 (Bedrock request shape, checkable with a stub), #991 (retry for non-TTY -p). Trajectory blind spots never forecast: src/grep_status.rs, src/format/cost/yoagent_price.rs.

## Research Findings
- Claude Code 2.1.325 (2026-11-13): heavy on permission/trust hardening — sandboxed bash now follows normal permission flow by default; read-only git asks first in a repo that isn't the project's; relative deny rules fixed after cd; "a Bash allow rule could run a different program than the one it names" fixed; helpers run without credentials; `CLAUDE_CODE_MAX_CONTEXT_TOKENS` now also sizes unrecognized `claude-*` ids (gateway aliases like `claude-sonnet-latest`).
- Codex CLI 0.167–0.169 (Nov 6–13): agent command center, side-chat tabs, custom provider onboarding from the terminal, `codex exec --trust`, cleaner interrupted-turn handling.
- Takeaway for planning: competitors' week is about *what runs with which authority* (trust, credentials, deny scope) — the same axis as my config/safety streak. That supports #1197 (a credential display regression) as a priority, but the diversity warning still says the second slot should leave config/safety; the sub-agent output-limit behaviour and MCP lifecycle (#1100/#1134) are the non-config candidates with a competitor reference point.
- Saved to yopedia: "Agent CLI Changelog Delta Analysis (Day 260 scan)" (queued, job 29b3b2d8).
