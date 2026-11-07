# Assessment — Day 252

## Build Status
pass — verified by the harness at session start (HEAD f981070e). Binary `yoyo v0.4.0 (f981070e 2026-11-07)` answers `-p "Reply with just the word ok"` with `ok`, exit 0, model claude-opus-5-5 (no temperature error — #1145 fix holds on the default).

**Two size-gate tripwires a task could hit (from trajectory, confirmed with wc -l):**
- `src/commands_risk_unhittable.rs` = 2036 lines, UNLISTED in tests/module_size.rs, 36 over the 2000 cap. 14 more lines → `cargo test` FATAL → whole task reverts. Any task touching it must first add `("src/commands_risk_unhittable.rs", 2036)` to GRANDFATHERED_OVERSIZED_MODULES (or split).
- `src/help.rs` = 2946 vs recorded 2856 (+90; drift grace is 100). 10 more lines → FATAL. Any help-text task must paste `("src/help.rs", 2946)` over its entry first. (help.rs/help_data.rs are where any #1127 or model-example edit lands, so this is likely to bite.)

## Recent Changes (last 3 sessions)
All Day 251, all providers/model-id work (5 sessions, 9 task commits, 0 reverts):
- 21:18 — 8cbfd42a Fixes #1144 item 2: `/model info` Claude context/output limits read yoagent 0.27.2's table (no hand copy); ea62b220/607d0dbc Part of #1147: dropped 5 made-up non-Claude ids (o4-mini-high, mixtral-8x7b-32768, grok-2, gpt-5.5-mini, MiniMax-M1-40k) with a sourced test list. Journal admits grok-3 (retired May) still offered and ~24 unverified ids remain.
- 14:53 — 0a995f61 Fixes #1145: don't send temperature to Claude 4.7+ (`src/temperature_support.rs`); a5970942: dropped dead Claude ids (sonnet-4-7 never existed, spread to 4 help examples + 4 docs pages).
- 11:49 — bedrock pricing via yoagent lookup, deleted `bedrock_anthropic_preset_id`; Fixes #1117 OpenRouter dash→dot id mapping.
- 06:43 — Fixes #1026 json `is_error` agrees with exit code + `incomplete` field; Fixes #1143 GIF refused everywhere, one SUPPORTED_IMAGE_FORMATS list.
- 02:02 — Part of #1012: `/cost` reads yoagent prices first, ~70 fallback rows kept; Gemini promo fix deleted (bills up to 2x until promo end, owner's decision).

**Trajectory warning: providers took 7 of the last 13 self-driven diffs. Send this session's self-driven slot to a different subsystem.**

## Source Architecture
167 files in src/, ~229k lines total incl. tests. Largest: cli.rs 7863, commands_risk.rs 6615, tool_wrappers.rs 5762, tools.rs 5268, agent_builder.rs 4977, config.rs 4851, commands_search.rs 4780, safety.rs 4720, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3987, commands_git.rs 3923. Entry: main.rs → cli::parse_args_with_config → repl::run_repl / prompt::run_prompt*; agent_builder::build_agent assembles tools (tools::build_tools) + MCP (mcp_prefix::connect_mcp).

## Self-Test Results
- `-p` round trip: ok.
- #1127 (shell verbs printing REPL slash syntax) re-probed on the built binary, `</dev/null`, no args: **still live** for `blame` (`Usage: /blame <file>…`), `docs` (`usage: /docs <crate>`, `Examples: /docs serde`), `ast` (`Usage: /ast <pattern>…`), `risk` (`Use /risk --all`), `memories` (`Use /forget <n>`). grep/def/outline fixed (3ea76ec6). lint/skill/map printed no slash line on the bare form (not probed with other args — the ticket's "still open" list names them; recheck with the ticket's arg forms).
- #1114 probe `yoyo -p --print-system-prompt`: now warns `-p value looks like another flag` and sends it as a prompt (boolean scans done Day 247); ticket's open scope is value lookups (`flag_value`, 54 sites).

## Evolution History (last 5 runs)
evolve.yml: 4 success (2026-11-06 06:37, 11:45, 14:38, 21:14), current run in progress. ci.yml last 5: all success. No failures, no reverts in 14 days. Only CI red in window: hook_feedback_request flake 9 days ago (SIGPIPE, fixed 4e2b1909).

## Capability Gaps
- **Non-interactive permission policy.** Claude Code 2.1.319 added `permissions.denyInsteadOfPrompting`: a call that would wait on a prompt is *denied* and the turn continues. yoyo's non-interactive story (seen in src/cli.rs:1371-1376): `--print` implies `--yes` (approve everything). I did NOT verify what `-p` without `--yes` does at a would-prompt call in a pipe — a planner picking this must probe it first. Design-sized, product.
- **Background/async tool results.** Claude Code backgrounds MCP task calls and reports results later; Codex 0.167 adds an "Agent Command Center" listing active/queued/finished background agents. yoyo has /bg for shell jobs and /spawn, no unified view of finished sub-agents.
- **Approval UX.** Codex 0.167 expands the full patch inline in the approval prompt; yoyo already has `format_edit_diff_preview` in tool_wrappers.rs (parity-ish, not re-checked).
- Long-standing: no TUI (#215 parked), no benchmark submission (#156 parked), no MCP restart (#1134).

Most useful gap that fits one session and is NOT providers: **adopt yoagent's ReadLog (#1094)** — pure "use the dependency instead of a hand copy", the same move the last 9 commits made for prices/limits, but in the tools subsystem.

## Bugs / Friction Found
1. **#1094 — yoagent now ships the partial-read guard yoyo invented, and yoyo doesn't use it.** yoagent 0.27.2 has `tools::ReadLog` shared via `ReadFileTool::with_read_log` / `WriteFileTool::with_read_log` (confirmed in `yoagent-0.27.2/src/tools/file.rs:377`, `tools/mod.rs:56` wires it in `default_tools`). `grep -rn "ReadLog\|read_log" src/` → 0 hits; yoyo still runs its own 275-line `src/partial_read_guard.rs`. Day 250 comment specifies the task: one ReadLog per agent (fresh per sub-agent), pass to both tools, decide whether partial_read_guard.rs retires or stays as second layer; acceptance = replay the residue shapes against the built binary. Note yoagent's doc says a never-read overwrite stays allowed (residue item 1 is still a yoyo decision). Subsystem: tools — NOT providers. Fits CLAUDE.md "don't reinvent the wheel".
2. **#1127 — 5+ shell verbs still print `/verb` usage** (above). Product, text-only, exit codes already right; seam `commands_dev::command_form(surface, verb)` per the ticket. Watch help.rs size gate.
3. **grep-3 still offered** although xAI retired it (journal 21:18, #1147). Providers zone — file/defer per the concentration warning.
4. Size gate tripwires above.

## Open Issues Summary
agent-self open: #1147 (~24 unverified non-Claude ids + 3 rows), #1134 (MCP lifecycle: no restart, no re-check on list_changed, unmeasured premise), #1127 (shell slash syntax, 8 verbs per scope line), #1121 (bedrock residue, waits on upstream #1148), #1118 (Gemini stale ids), #1114 (flag_value value-slot lookups), #1102 (hard_deny heredoc; owner decision A, keep counting, revisit at 5), #1100 (MCP servers outlive a panic; upstream accessor closed), #1094 (adopt yoagent ReadLog), #1064 (API key scrub: /run, !, /bg, /watch, sub-agent bash still open), #1012 (price drift). Upstream: #1148, #1146. agent-input: #1122 (yoagent upgrade/reuse umbrella), #991 (retry_safe_events). Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.

## Research Findings
- yopedia recall: prior changelog-delta notes exist through 2026-11-03 (`ai-coding-agent-changelog-delta-*`, "Agent Changelog Defect-Class Transfer"). New note ingested today: "AI Coding Agent Changelog Delta — 2026-11-07" (queued).
- **Claude Code v2.1.319 (2026-11-06)** items with yoyo analogues: denyInsteadOfPrompting (above); `/model` picker groups older models under "More models" (yoyo's picker just got pruned of dead ids — grouping is the alternative to deleting); `/context` keeps 10 largest rows per list, on-demand MCP tools one row per server; PostToolUse hooks still run (10s cap) for calls that finish after an interrupt; after safeguards stop a subagent the model is told to retry narrower or tell the user rather than do the work itself; a startup line naming which org is billed when auth falls back to another sign-in (cf. CLAUDE.md's warning that yoyo's key lookup silently falls back ANTHROPIC_API_KEY → API_KEY and 401s — yoyo has no such disclosure line; unverified whether one exists, worth one grep).
- **Codex CLI v0.167.0 (2026-11-06)**: Agent Command Center (`codex agents`), multi-backend switch from the resume picker keeping the draft, inline patch review in approvals, GCP Vertex setup wizard, server-side bounded output, per-chat MCP connection leases, PreToolUse hooks now run before policy checks.
- Sources: https://code.claude.com/docs/en/changelog ; https://codex.danielvaughan.com/2026/11/06/codex-v0167-stable-agent-command-center-multi-backend-image-attachments-gcp-vertex/

## Planner notes (facts, not tasks)
- Concentration warning: providers 7/13. Non-providers candidates with tickets and seams already written: #1094 (tools; ReadLog), #1127 (help/usage text; `command_form(surface, verb)`), #1064 remainder (/run, !, /bg, /watch env scrub opt-in), #1114 (flag_value value-slot lookups, 54 sites).
- Epistemic blind spots per trajectory: src/prompt/turn_prefix.rs, src/commands_bg_kill_tests.rs, never-forecast src/tls_preflight.rs, src/tool_wrappers_cancel_tests.rs.
- Size gate: pre-pay help.rs (2946) and commands_risk_unhittable.rs (2036, unlisted) entries in tests/module_size.rs before any edit to either.
- session_plan/ is gitignored, so `git add session_plan/assessment.md` fails without -f; the file is on disk.
