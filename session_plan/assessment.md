# Assessment — Day 253

## Build Status
Pass. The harness verified it at session start; HEAD is f822db95 (transitive dep bumps). `yoyo --version` prints `v0.4.0 (f822db95 2026-11-08)`. `yoyo -p "Reply with just the word: pong"` in an empty temp dir returned `pong`, exit 0, model claude-opus-5-5. I did not re-run the full suite.

## Recent Changes (last 3 sessions)
All of Day 252 went 2/2 green across six sessions. The theme was shell/REPL wording and argv parsing.
- 23:00: Fixes #1152, which locked yoagent 0.27.4, re-ran the sampling-flag table, and made the dropped-temperature note name the vendor that really rejects it (a GPT-6 request through a recording stub). Also Part of #1127: `yoyo map` and `yoyo skill` now print shell forms. Filed #1153 (skill show/install/init with no argument exit 0).
- 19:55: Fixes #1151 (the stray-word refusal names --system as the flag that swallowed --model). Part of #1127 (docs, risk, memories wording).
- 15:51: Fixes #1114 (the last value-slot scans, plus the numeric-flag refusal), and corrected the OpenRouter temperature claim (OpenRouter drops unsupported params; it doesn't 400). Part of #1122.
- Earlier on Day 252: #1150 (`-p hi extra` refuses), #1146/#1122 (temperature_support delegates to yoagent's accepts_sampling_params), the ReadLog wiring for #1094, and the #1149 disclosure of -p auto-approve.

## Source Architecture
169 files in src/, about 208k lines (.rs files in src/ only). Largest: cli.rs 7861, commands_risk.rs 6624, tool_wrappers.rs 5762, tools.rs 5295, agent_builder.rs 4997, config.rs 4851, commands_search.rs 4780. Entry: main.rs → cli::parse_args_with_config → repl::run_repl or the -p path. Dispatch goes through dispatch.rs / dispatch_sub.rs (shell subcommands). Agent construction is in agent_builder.rs (presets, MCP via mcp_prefix.rs, fallback).

**Size-gate hazards (from the trajectory, confirmed with wc):**
- `src/commands_risk_unhittable.rs` is 2036 lines, past the 2000 cap and NOT in GRANDFATHERED_OVERSIZED_MODULES. 14 more lines (2050 = cap + OVERSHOOT_GRACE 50) makes `cargo test` FATAL and reverts the whole task. Any task touching it must split it or register it first.
- `src/help.rs` is 2946 against its recorded 2856 (+90; REGISTER_DRIFT_GRACE_LINES=100). 10 more lines is FATAL. A task that adds help text must paste the new count into tests/module_size.rs:571 in the same diff.

## Self-Test Results
Built binary, empty temp dir, fresh HOME, `</dev/null`:
- `yoyo lint --bogus` → `Unknown /lint subcommand: --bogus` (exit 2). This is the last open #1127 wording row.
- `yoyo skill show` / `skill install` / `skill init` → usage line, **exit 0** (#1153, confirmed live).
- `yoyo grep`, `def`, `docs` → `usage: yoyo <verb> …`, exit 2 (fixed, good).
- `yoyo risk zz` → shell-form error, exit 2 (good).
- `yoyo memories` → "Add one in the REPL: run `yoyo`, then /remember <note>." (honest: there is no shell form).
- `yoyo lint` in a dir with no project → "No recognized project found", exit 1 (reasonable).
- `yoyo skill` with no skills → "use --skills <dir>" (fine).

## Evolution History (last 5 runs)
The 4 completed evolve runs (2026-11-07 13:08 → 22:54) all succeeded; the 5th is this one, in progress. No reverts in 14 days. The only CI failure in the window was a 10-day-old flake in hook_feedback_request, fixed Day 242 (SIGPIPE). Provider health is clean. Phase spend over 7 days: social 22 runs / $24.09, synthesize 7 / $8.17, dream 1 / $2.55.

## Capability Gaps
(See Research Findings.) Standing gaps against Claude Code: -p permission default (#1149, waiting on @yuanhao), MCP over HTTP (stdio only), no live restart of a dead MCP server (#1134), credential scrub for child processes (#1064), and CLAUDE_CODE_GAP.md is 178 days stale (its header was last verified on Day 74).

## Bugs / Friction Found
1. #1153: `yoyo skill show|install|init` with no argument exits 0. Small and concrete; the done-when is on the ticket (exit 2 + a shell-door test).
2. #1127: the last row is `lint` (`Unknown /lint subcommand` on the shell), plus two `/skill` strings outside the usage paths per the ticket comment. Finishing these closes the ticket. Before writing Fixes, check the 00:00Z comment's two `/skill` strings.
3. The module-size hazards above. Registering commands_risk_unhittable.rs is a cheap safety task that protects any future risk task.
4. CLAUDE_CODE_GAP.md is stale (178 days). The trajectory flags its header; the body is unverified.
5. Trajectory "epistemic blind spots": src/prompt/turn_prefix.rs, commands_bg_kill_tests.rs, and the never-forecast commands_ast_grep_tests.rs / grep_status.rs. These are planner hints only.

## Open Issues Summary
agent-self, open: #1153 (skill exit 0), #1147 (29 of 78 non-Claude known ids have no outside authority, 5 look made-up; per the d251 lesson, iterate the authority, not the suspicious shortlist), #1134 (MCP lifecycle), #1127 (lint + 2 strings), #1121 (bedrock inference-profile ids), #1118 (gemini list stale ids), #1102 (hard_deny heredoc), #1100 (MCP servers outlive a panic), #1094 (write_file partial-read residue: the never-read overwrite decision), #1064 (credential scrub), #1012 (price drift; @yuanhao decided to reuse yoagent prices, live layer pending). Upstream: #1148. Waiting on the owner: #1149 (do not change the -p default). agent-input: #1122 (yoagent upgrade/skill reuse, ongoing), #991 (retry_safe_events). Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.

## Research Findings
Recall: yopedia already has changelog-delta scans through CC 2.1.317 and Codex 0.164-0.166 (the Day 249 scan). New since then, and ingested into yopedia this session: CC **2.1.319** (2026-11-06) and Codex **0.167.0** (2026-11-06).
- **CC 2.1.319 adds `permissions.denyInsteadOfPrompting`**: "a tool call that would wait on a permission prompt is denied and the turn continues", in terminal sessions, subagents and teammates. This is direct input for #1149 (should -p keep auto-approving?). It is a third option beside "approve all" and "prompt": deny, and let the model carry on. Add it to the #1149 ask as data for @yuanhao. **Do NOT change the default** (the ticket says to hold).
- CC 2.1.314 `permissions.blockWritesOutsideWorkingDirectories` (Write/Edit refuse paths outside the working dirs in every mode). yoyo has fences (cd_deny, the restricted mode). I haven't checked whether a write-outside-cwd block exists as a config key; that is a candidate gap.
- CC 2.1.319 changed "an exact Bash ask rule is refused, not asked about, when a broader deny rule covers the command". That is a precedence rule; whether yoyo's allow/deny precedence matches is unchecked.
- CC 2.1.319: PostToolUse hooks now run, with up to 10 seconds to finish, on tool calls that finish after an interrupt. yoyo's hook behaviour on interrupt is unchecked.
- Codex 0.167: "Automatic context compaction now accounts for retained output from failed responses and responses with missing token usage data." Defect class to transfer: does yoyo's context bar or compaction count a turn whose response errored or carried no usage? Unchecked; it would need a read of yoagent's ContextConfig token accounting first (don't reinvent).
- Codex 0.167: "Return standalone web search failures to the model". Possible check of yoyo's web_search tool failure path (does the model see the error text or nothing?). Unchecked.

## Suggested priorities (for the planner; not task files)
1. #1153 (skill show/install/init exit 0 → 2, plus a shell-door test). Small, with a clear done-when. Mind help.rs and the module-size headroom.
2. Close #1127: the `lint` wording row plus the two `/skill` strings named in the 2026-11-08T00:00Z comment. Read that comment's rows before titling it Fixes.
3. Safety chore: register or split `src/commands_risk_unhittable.rs` (2036, unlisted, 14 lines from FATAL) before any risk task lands.
4. Optional product: the Codex compaction defect-class transfer (context accounting on failed or usage-less responses). Probe first; it may already be correct.
