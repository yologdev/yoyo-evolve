# Assessment — Day 252 (13:14)

## Build Status
Pass, verified by the harness at session start (HEAD e571c191, yoagent 0.27.2 → 0.27.3 semver bump). I didn't re-run the suite.
Binary `yoyo v0.4.0 (e571c191)`: `-p "Reply with just the word PONG"` in an empty temp dir printed `PONG`, exit 0, model claude-opus-5-5.

**Size-gate hazards (from the trajectory, confirmed with wc + tests/module_size.rs):**
- `src/help.rs` is 2946 lines; its register entry says 2856 (+90 drift, the grace band is 100). **10 more lines are FATAL.** Any task touching help.rs or help text has to paste ("src/help.rs", <new count>) over the entry in the same diff.
- `src/commands_risk_unhittable.rs` is 2036 lines, unlisted, 36 past the 2000 cap (grace is 50). **14 more lines are FATAL.**
- `src/cli.rs` 7863 matches its entry exactly, so it has no drift headroom beyond its band.

## Recent Changes (last 3 sessions)
- **Day 252 10:57:** f9378cb3, Part of #1149. Docs (permissions.md, piped-mode.md, single-prompt.md) now say that `-p`/piped runs auto-approve every tool call. The behaviour stays as it is while @yuanhao decides on #1149. fb0c95bf, Part of #1114: `flag_value` / `flag_value_warning_for` now find a flag with `cli_config::option_position`, so a flag sitting in a value slot is skipped. 39 of 39 callers were censused, and tests/flag_value_slot.rs got a positive control. That commit **filed #1150**: with the fix in, `yoyo --system --model claude-zzz -p hi` now silently drops `claude-zzz`, because bare positional words in a `-p` run are discarded.
- **Day 252 01:37:** 421f3b86, Part of #1094. yoagent's per-agent `ReadLog` is wired into read_file/write_file (parent, sub-agent and explore child). yoyo's own partial_read_guard is kept, because ReadLog records before TruncatingTool cuts the page. The retry classifier also recognises the ReadLog refusal wording. 60b407d7, Part of #1127: shell `yoyo blame` / `yoyo ast` print `yoyo <verb>` usage.
- **Day 251 (four sessions):** #1147 removed 5 made-up non-Claude ids, with a test that lists the vendor URLs. #1144/#1121: `/model info` context windows and `anthropic_output_maximum` now read yoagent. #1145 stops sending temperature to Claude models that reject it. #1012 slice 1: yoagent prices are reused and the promo module deleted.
- e571c191 (deps): yoagent 0.27.3. **Its changelog hasn't been read against the tree yet.** Per the d230 upgrade rule, check what 0.27.3 changed (ReadLog wording? presets? #1148's asks for Claude max_output data and the Bedrock opus-4-7 mapping?).

## Source Architecture
168 files in src/, ~229.6k lines including the *_tests.rs siblings. Largest: cli.rs 7863, commands_risk.rs 6615, tool_wrappers.rs 5762, tools.rs 5295, agent_builder.rs 4977, config.rs 4851, commands_search.rs 4780, safety.rs 4720, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3987.
Entry points: main.rs → cli::parse_args_with_config → dispatch_sub::try_dispatch_subcommand (shell verbs) or repl::run_repl / prompt::run_prompt (-p). The agent is built in agent_builder.rs (build_agent, create_model_config, presets), tools in tools.rs and tool_wrappers.rs, and the MCP connect path in mcp_prefix.rs.

## Self-Test Results
Built binary, empty temp dir, `</dev/null`:
- `-p` round trip: works.
- #1127 residue, confirmed live, all six still print REPL slash syntax on the shell: `yoyo docs` → `usage: /docs <crate> [item]` / `Examples: /docs serde` (exit 2). `yoyo lint zzz` → `Unknown /lint subcommand`. `yoyo risk zzz` → `Unknown /risk subcommand`. `yoyo skill zzz` → `try: /skill list, /skill show …`. `yoyo map --zzz` → `usage: /map [--all] …`. `yoyo memories zzz` (exit 0) → `Use /remember <note> to add one.` (that one is a *hint* naming a REPL command that has no shell equivalent; check whether `yoyo remember` exists before rewording). The ticket's stated remainder (docs, lint, risk, skill, map, memories) matches what I probed: 6 of 6 still open.
- #1150 is read from code by the filing session and not re-probed here. The fix site is named on the ticket: cli.rs:2717 / cli_config.rs:423 `prompt_text`.

## Evolution History (last 5 runs)
evolve.yml: 4 successes (11-06 14:38, 21:14, 11-07 01:34, 10:51) plus the in-progress run (this one). The trajectory's last 10 sessions were all 2/2 or 1/1 tasks, with 0 reverts in 14 days. The only recurring CI error is a 1× hook_feedback_request flake 10 days ago (the SIGPIPE fix, d242), and CI has been green since. Provider/API health is clean. Subsystem concentration is providers/agent/anthropic_preset (5/4/4 of 12): the last ~2 days were mostly price/model-id work.

## Capability Gaps
- **Non-interactive permission policy.** yoyo's `-p`/piped mode auto-approves every tool call (now documented; the default is #1149, owner's call). Claude Code 2.1.319 added `permissions.denyInsteadOfPrompting`: a call that would wait on a prompt is *denied* and the turn continues, in terminal, subagents and teammates. That's a third option between "prompt" and "approve all", and it's useful data for @yuanhao on #1149 (offer it as data, don't change the default).
- **MCP long-running tasks:** Claude Code now moves an MCP task to the background and reports it when done. yoyo has no MCP task support, and its MCP lifecycle residue (#1134 no restart, #1100 orphaned servers) is still open.
- Not checked this session: Cursor, Codex, Aider. Prior yopedia notes cover Claude Code up to the Nov 3 changelog. CLAUDE_CODE_GAP.md's header is 178 days stale, so don't trust its rows.

## Bugs / Friction Found
1. **#1150** (agent-self, filed today): `yoyo -p hi extra` sends only `hi`, and the extra word vanishes without a word. Since fb0c95bf this also swallows the #1114 repro's `claude-zzz`. It's a product input-loss bug and the fix site is known. Done-when per the ticket: refuse (exit 2, like #1109) or warn/append, tested at the real binary. Nearby lessons: d245/d246 (probe the class from the authority, here every positional-collection path in a -p run), d235 (list the inputs a normaliser merged).
2. **#1114 residue:** `collect_repeatable_flag` (cli.rs) and `resolve_numeric_setting` (cli_config.rs) still read value slots. cli.rs has no headroom left, so the fix belongs in cli_config.rs's `option_position` with only call-site changes in cli.rs (and d248 applies: if the diff bumps cli.rs's cap, a residue can't be declined "because cli.rs is at its cap").
3. **#1127 residue:** 6 verbs, confirmed live above. The `command_form(surface, verb)` helper already exists (60b407d7), so each one is mechanical. Watch help.rs's 10-line margin if any text lives there.
4. **Size gates:** help.rs and commands_risk_unhittable.rs are close to FATAL (see Build Status). A planner picking help-text work must budget the paste.
5. **yoagent 0.27.3 (now locked) adds `ModelConfig::accepts_sampling_params`, which was asked for by yoyo's #1146.** @yuanhao's closing comment on #1146 (2026-11-07T13:01Z) prescribes the yoyo follow-up: *"Replace #1145's own version check with `config.accepts_sampling_params`. Keep the stderr note when the user set a temperature and the flag is `false`."* He lists the limits to handle. Gateway and custom configs (`openai_compat`, `local`, `ollama`, `custom`) are never inferred, so yoyo has to set the flag where it routes Claude that way. The flag is computed when the config is built. A config persisted before 0.27.3 loads with the flag on.
   Today yoyo carries its own copy of the predicate: `src/temperature_support.rs` `rejects_temperature` parses the Claude version itself and covers only `anthropic` and `bedrock`. Callers: agent_builder.rs:27 (`effective_temperature`) and repl.rs:875. **An openrouter user (`anthropic/claude-opus-5.5` with `--temperature`) gets no stderr note.** yoagent now drops the value there silently, so the note is missing exactly where yoagent and yoyo disagree. This is the d251 shape (a hand copy of a dependency's rule).
   **No open carrier:** #1145 and #1146 are both CLOSED, so a task needs `Part of #1122` (his reuse umbrella) or a new issue. Product kind. Fix site: temperature_support.rs plus its tests. Small, and the owner has already specified it. **Recommended as a top pick.**
   The rest of 0.27.3 is only that change. 0.27.2's limits-as-data and Opus 4.5–4.7 rows were already consumed on Day 251.

## Open Issues Summary
agent-self open: #1150 (-p drops positionals, new), #1147 (29 of 78 non-Claude ids lack an outside authority; 5 fakes removed, ~24 unchecked plus grok-3, retired in May and still offered), #1134 (MCP live-server lifecycle), #1127 (6 verbs left), #1121 (Bedrock inference-profile ids), #1118 (stale Gemini ids), #1114 (2 helpers left), #1102 (hard_deny heredoc false positive), #1100 (MCP servers outlive a panic), #1094 (partial-read residue: never-read overwrite, truncated pages, process-wide store), #1064 (opt-in credential scrub for children), #1012 (75 price rows drift; slice 2 = live models.dev layer). Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.
Waiting on the owner, **do not change**: #1149 (-p auto-approve default). Upstream: #1148.
agent-input: #1122 (@yuanhao's yoagent-reuse umbrella; the yoyo-side comments keep landing on it. Most recent: ReadLog). #991 (adopt retry_safe_events for non-TTY -p, from yoagent 0.24.1; untouched since 10-29, so worth checking whether it's still open work).

## Research Findings
- yopedia recall: I already have notes "Agent Changelog Defect-Class Transfer" (Claude Code Nov 3) and "Agent Policy Enforcement Gaps" (2.1.295–299). New: ingested a note on **Claude Code 2.1.319 (2026-11-06)** with its transfer candidates.
- Transfer candidates from 2.1.319, **none probed against yoyo yet** (each is a cheap probe for a planner who wants one):
  (a) "Fixed `claude -p` exiting 0 in auto mode when its actions were refused". Question: does `yoyo -p` exit 0 when hard_deny / a deny rule refuses the model's tool call and the model gives up?
  (b) "Fixed slash commands typed after a leading space/tab/newline being sent as plain text". Question: does the yoyo REPL run `  /help`?
  (c) "Fixed /rename reporting plain success when the save failed". Same fail-flattering class as yoyo's #1137 write-failure census.
  (d) Deny precedence: an exact ask rule is refused when a broader deny covers the command.
- yoagent cadence: 0.27.1, 0.27.2 and 0.27.3 each shipped a yoyo ask within hours (d248 social note holds: narrow upstream asks get answered fast). #1148 (Claude max_output as data, Bedrock opus-4-7) is still open upstream.

## Planner notes (priorities, by evidence)
1. **0.27.3 `accepts_sampling_params` adoption.** Owner-prescribed, small, product, closes a real openrouter note gap. Needs a carrier (Part of #1122 or a new issue).
2. **#1150:** silent input loss in `-p`, filed today, fix site named. Probe at the binary first (the ticket says it was read from code, not probed). Decide refuse vs warn using #1109's precedent (unknown flag → exit 2, nothing sent).
3. **#1127:** 6 verbs confirmed live; mechanical with `command_form`. Watch help.rs's 10-line FATAL margin.
4. **#1114:** 2 helpers left (`collect_repeatable_flag`, `resolve_numeric_setting`). cli.rs has no headroom, so put the logic in cli_config.rs.
Avoid: #1149 (waiting on owner). The concentration warning says providers/model-id work has dominated for 2 days, so items 2–4 diversify.
