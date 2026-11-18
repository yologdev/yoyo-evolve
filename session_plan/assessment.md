# Assessment — Day 263 (22:56)

## Build Status
Pass — verified by the harness at session start for 54b931bd. Binary `target/debug/yoyo` rebuilt 22:56 (newer than HEAD's commit time 20:05): `yoyo v0.4.0 (54b931bd 2026-11-18)`. `yoyo -p "Reply with exactly: ok"` → `ok`, exit 0, model claude-opus-5-5. `yoyo config get model` → "not set in config file (using default)", exit 0 (run from /tmp, no .yoyo.toml there — correct).

**Size-gate hazard the planner must price (from trajectory, confirmed with wc -l):**
- `src/context.rs` = 2035 lines, UNLISTED in `tests/module_size.rs` GRANDFATHERED_OVERSIZED_MODULES; +15 more lines makes `cargo test` fatal → whole task reverts. Any task touching context.rs either registers it or splits it first.
- `src/format/mod.rs` = 2874 vs recorded 2788 (+86 drift); +14 more lines is fatal (>100 drift). Any task touching format/mod.rs pastes the new value over its entry.

## Recent Changes (last 3 sessions)
- **Day 263 12:40** — `aecaadb7` `/export` refuses to overwrite an existing file unless `--force` (remedy round-tripped through the parser). `c9ef28f5` Fixes #1209 / Part of #1202: `bash_parse::dash_c_body` reads the literal body of `bash/sh/zsh/dash/ksh -c` (through sudo/env/xargs wrappers, `-lc` clusters, nested up to 8) and re-runs `tree_refusal` + `unanalyzable_reason` on it; blind checks 15→0, 317 of 4,440 corpus commands allowed→refused, 0 the other way. Filed #1211 (/save, /spawn -o overwrite).
- **Day 262 23:24** — `638791d2` Fixes #1196: bare `echo`/`printf` quoted words count as mentions (1,084 refused→allowed, none other way). Part of #1184: `[permissions]`/`[directories]` now read from `toml::Table` (line-walker metric 21→14). #1210 filed for the non-print residue (`git commit -m "kill -9 1 at $(date)"`).
- **Day 262 12:50** — Part of #1202 fail-closed clause: `unanalyzable_reason` (ERROR tree, `eval`, command name in a variable) asks even under "always"/allow rules. Still auto-runs under `--yes`, `-p`, non-tty — A/B put to @yuanhao on #1202 (no human reply yet; all 11 #1202 comments are mine). Fixes #1121 (Bedrock picker offers opus 5.5).
- Concentration: bash/safety/bash_parse = 4/4/3 of last 10 task commits; config 3/10.

## Source Architecture
189 files under src/, ~240k lines total (incl. *_tests.rs). Largest: cli.rs 7890, commands_risk.rs 6645, tool_wrappers.rs 5762, tools.rs 5301, agent_builder.rs 5039, safety.rs 4816, commands_search.rs 4784, config.rs 4700, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3988, commands_git.rs 3923, symbols.rs 3804, commands_project.rs 3790, hooks.rs 3732, format/cost.rs 3695, commands_info.rs 3559, repl.rs 3411. Entry: main.rs → cli.rs (parse_args) → agent_builder.rs (build_agent) → repl.rs / prompt.rs. Safety: safety.rs (33 SAFETY_CHECKS on text) + bash_parse.rs (tree-sitter-bash pass, 873 lines) + safety/indirect_command.rs. Config: config.rs + config_value.rs + config_toml_read.rs (toml crate) + config_unknown_keys.rs.

## Self-Test Results
- `-p` round trip works; startup banner fine.
- **#1211 reproduced on the built binary (tty via `script`)**: in /tmp/yt with `a.md` containing `hi`, `/save a.md` printed `(session saved to a.md, 0 messages)` and a.md now contains `[]` — the user's file replaced with no warning. Two bare `/save`s both wrote `yoyo-session.json` silently. `/export` (fixed today) is the only user-named write that refuses.
- Piped stdin with slash commands is refused with a clear exit-2 message and three remedies — fine.
- REPL tab-hint redraws (`🐙 › /status — Show session dashboard🐙 › /save — ...`) appear repeatedly in the `script` capture; this is the inline completion hint rendering under a pseudo-tty with fast input, probably cosmetic, not investigated.

## Evolution History (last 5 runs)
gh run list evolve.yml: 4 × success (2026-11-16 23:57, 11-17 12:43, 11-17 23:18, 11-18 12:36), current run 22:49 in progress. Trajectory: last 10 sessions 0 reverts; one Day-261 session had a task with no verdict (tree green). CI failures in window are 5d old (clippy `if` guard returning bool literal), green since. 1 provider error hit, retried.

## Capability Gaps
- **No OS-level sandbox.** Codex runs commands inside an OS sandbox with permission profiles (default `:workspace`: writes only in the workspace, `.git`/`.codex` read-only, network off; per-command escalation the harness decides). Claude Code has a Bash sandbox too. yoyo's whole defence is command classification (text checks + tree-sitter pass) plus confirm prompts — a classifier, not a boundary. Large; not a one-session task, but it is the biggest structural gap.
- **Untrusted-project config.** Codex 0.169: `codex exec` keeps project config and MCP disabled in untrusted dirs even with workspace-write; `--trust` saves trust explicitly. yoyo #1188: trust keyed on path, so a pulled change to an already-trusted `.yoyo.toml` (e.g. base_url) applies silently.
- **Self-diagnostics in /status.** Claude Code 2.1.327 lists broken skill/agent/command files, hook matchers that never match, ignored permission settings in `/status`. yoyo has pieces (unknown-hook-tool warning, unknown config keys, cd_config_note) scattered at startup, not one place.
- Safety is still mostly text-pattern (33 checks) with a tree pass layered on; #1202 step 2/3 (decide on the tree, retire text checks) not started. Unparseable-command policy under `--yes`/`-p`/non-tty awaits owner.
- Config still has 14 line-walker hits (#1184): unknown-key detection, writes (`set_toml_key` hand edits vs toml_edit), deleting the hand parser.
- MCP: stdio only, no restart of dead servers (#1134), no HTTP transport.

## Bugs / Friction Found
1. **#1211 live**: `/save <path>` overwrites any existing user file silently (reproduced above; data loss). `/spawn -o <path>` same per census (not re-probed). Small, well-scoped, has today's `/export` as the template (create_new + `--force` + remedy round-trip test). Strong candidate.
2. **#1210**: quoted mention in a non-print command (`git commit -m "kill -9 1 at $(date)"`) refused as if it ran — false positive that hits the agent's own commits; graded "read from code, not re-probed".
3. **#1206** Part-fixed locally (refusal_history.rs); upstream yoagent#455 pending — re-query state before restating.
4. **#1208** temperature bound keyed on provider name (openai base_url to other servers gets 2.0).
5. Size-gate hazards (context.rs, format/mod.rs) above.

## Open Issues Summary
agent-self open: #1211 (/save,/spawn -o overwrite), #1210 (mention FP residue), #1208 (temperature by provider name), #1206 (refusal→400; upstream half), #1202 (owner: tree-sitter safety; step 1 partly done, A/B unanswered), #1188 (trust keyed on path not content), #1186 (fable-5-2 price, upstream), #1184 (owner: toml crate; 14 line-walker hits left), #1147 (29/78 non-Claude ids without outside authority), #1134 (MCP lifecycle), #1102 (heredoc mention FP), #1100 (MCP outlives panic), #1094 (write_file never-read overwrite etc.), #1064 (child processes inherit API key, opt-in scrub), #1012 (75 price rows drift vs models.dev). Owner agent-input: #1122 (yoagent upgrade/skill/plugins), #991 (retry_safe_events for non-TTY -p). Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.

## Research Findings
Recall: yopedia already has "AI coding agent competitive landscape", "Agent Changelog Defect-Class Transfer/Delta Analysis", "AI Coding Agent Security Hardening". New this session (ingested): OpenAI "Inside the Codex sandbox" (2026-11-18) and a Claude Code 2.1.327 / Codex 0.169-0.170 delta note.
- **Claude Code 2.1.327 (2026-11-17)** defect-class transfers to yoyo:
  - `claude mcp add --scope project` now *stops instead of overwriting* a `.mcp.json` it can't read → same class as #1211 (silent overwrite of a user file). Industry is closing this class; yoyo has `/save` and `/spawn -o` open.
  - "Improved the refusal for shell commands ... the dangerous rm check can't parse, so Claude can resend harmless ones in plain syntax instead of giving up" → yoyo's `unanalyzable_reason` path: the interactive prompt prints the reason, and `child_confirm_refusal` tells a sub-agent to hand the step back to the parent, but nothing tells the *model* "resend in plain syntax" when an unreadable command is refused (e.g. user says n, or non-tty after owner's A/B lands). Cheap message improvement once #1202's A/B is answered.
  - "Bash commands whose variables expand to very long text are denied" — no yoyo equivalent; low priority.
  - `/config` notes when other settings override a row → yoyo already has `shadowed_key_note` for `config get`.
  - ExitWorktree asks before removing a worktree that may hold unsaved work; refused when nobody can answer → compare `/spawn` worktree cleanup (not checked this session).
- **Codex 0.169/0.170**: untrusted-dir config/MCP disabled for `exec`; explicit `--trust`. Fixed "child threads losing existing sandbox restrictions" — yoyo's analogous child-door work (#977 child_confirm_refusal, UserDenyBashTool) is already in place.

## Suggested priorities for the planner (my read, not binding)
1. **#1211** — reproduced live today: `/save a.md` replaced the user's `a.md` with `[]`. Data loss, small, `/export` (aecaadb7) is the template (create_new + `--force` + remedy round-trip test). Census both members (`/save`, `/spawn -o`) with the built binary; done-when is the class ("every user-named write path"), not the two names. Kind: product.
2. **#1184** next slice (14 line-walker hits left; owner-filed). Report the metric before/after in the first 200 chars.
3. **#1210** (mention false positive in non-print commands) — but bash/safety is already 4/10 + bash_parse 3/10 of recent commits, and #1202's owner A/B (fail-closed under --yes/-p/non-tty) is unanswered since 11-17 13:22; don't ship the B branch of that question without him.
4. Size gate: any task touching `src/context.rs` (2035, unlisted) or `src/format/mod.rs` (+86 drift) must register/paste its entry in the same diff or it reverts.
