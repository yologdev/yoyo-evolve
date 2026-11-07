# Assessment — Day 252 (19:55)

## Build Status
Pass — verified by the harness at session start (d4103fbd). Binary `yoyo v0.4.0 (d4103fbd 2026-11-07)` built and ran; a live `-p "say PONG"` against claude-opus-5-5 returned `PONG`, exit 0. I did not re-run the full suite.

**Size gate, close to FATAL (from the trajectory, not re-measured):** `src/commands_risk_unhittable.rs` is 2036 lines, UNLISTED in `GRANDFATHERED_OVERSIZED_MODULES`, 14 lines short of FATAL. `src/help.rs` is 2946 vs a recorded 2856 (+90 drift), 10 lines short of FATAL. Any task that adds ~10+ lines to either file reverts the whole task. The planner should either budget for that or pay the entry first (paste the gate's printed values, as earlier sessions did).

## Recent Changes (last 3 sessions, all Day 252, all 2/2 green)
- **15:51**: Fixes #1114. `collect_repeatable_flag` and `resolve_numeric_setting` now skip value slots, so all argv readers walk the same way (6b9f6e52). Part of #1122: corrected my own overclaim. The docs and the stderr note had said OpenRouter returns 400 for temperature on Claude 4.7+. OpenRouter's docs say it *ignores* the field, so the docs now say "most likely ignored" and the note says "Anthropic's API rejects" (e12cab41).
- **13:14**: Part of #1122. `temperature_support.rs` now reads yoagent 0.27.3's `accepts_sampling_params` off the built ModelConfig, replacing yoyo's own version parsing. Fixes #1150: `yoyo -p hi extra` refuses the stray word with exit 2 and sends nothing (0a3305e0/92397abb, two commits under one title).
- **10:57**: Part of #1149. Docs now say plainly that `-p`/piped mode auto-approves tools (the decision is waiting on @yuanhao). Part of #1114: `flag_value` skips value slots, with a 39-caller census.
- **01:37**: Part of #1094. yoagent `ReadLog` is wired per agent stack, and yoyo's process-wide `partial_read_guard` stays as a second layer. Part of #1127: `blame`/`ast` usage is per surface.
- External: llm-wiki is still paused mid-migration. Its last journal entries are from April.

## Source Architecture
There are 168 `.rs` entries in src/ plus `format/`, `prompt/`, `safety/` subdirs, about 230k lines including tests (`wc -l src/*.rs src/*/*.rs`). Largest: cli.rs 7861, commands_risk.rs 6615, tool_wrappers.rs 5762, tools.rs 5295, agent_builder.rs 4996, config.rs 4851, commands_search.rs 4780, safety.rs 4720, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3987, commands_git.rs 3923. Entry points: `main.rs` → `cli::parse_args_with_config` → `dispatch_sub::try_dispatch_subcommand` (shell verbs) or `repl::run_repl` / the prompt path (`prompt.rs`, `prompt_retry.rs`). Agent construction is in `agent_builder.rs` (`build_agent`, presets, MCP via `mcp_prefix::connect_mcp`). Tools are in `tools.rs` and `tool_wrappers.rs`.

## Self-Test Results
- `-p` live call works. `--temperature 0.5 --model claude-opus-5-5 -p hi` prints the new note ("Anthropic's API rejects…") and the run proceeds.
- **#1127 re-probe (shell, empty tmp dir, `</dev/null`)**: `grep`, `def` and `outline` now print `yoyo <verb>` forms. Still printing REPL slash syntax in the shell:
  - `yoyo docs` → `usage: /docs <crate> [item]`, `Examples: /docs serde`
  - `yoyo risk zz1` → `Unknown /risk subcommand: zz1`
  - `yoyo memories` → `Use /remember <note> to add one.`

  `skill`, `map` and `lint` printed no slash forms on their bare/empty path, but I only probed one path each.
- **New friction, misleading refusal (#1150 fix + #1114 interaction):** `yoyo --system --model claude-zzz -p hi` now prints `error: word 'claude-zzz' is not part of -p's value and would be dropped. Quote the whole prompt (-p "hi claude-zzz") or put the extra words after --`. The word comes *before* `-p`, so it isn't "-p's value" leftover. What actually went wrong is that `--system` swallowed `--model` as its value. The suggested fix (`-p "hi claude-zzz"`) would send the user's intended model name to the model as prompt text. The refusal is safe (nothing sent, exit non-zero), but its explanation and its remedy are wrong for this input. A better message would name the flag that took a dash-led value ("--system took '--model' as its value"). There's already a "looks like a flag" warning machinery for value slots (`flag_like_value_warning` in dispatch_sub.rs). It isn't checked whether it fires first on this path.

## Evolution History (last 5 runs)
The four completed evolve runs today (01:34, 10:51, 13:08, 15:44Z) are all `success`, and the current one (19:50Z) is in progress. The trajectory shows 10/10 sessions green, 0 reverts in 14 days, and no provider errors. The last CI failure was the hook_feedback_request SIGPIPE flake 10 days ago, fixed Day 242. Concentration: agent/cli/temperature each 3 of 11. Today was mostly argv parsing and temperature.

## Capability Gaps
Compared against Claude Code 2.1.319 (2026-11-06; raw CHANGELOG.md fetched this session). Ranked by what a user would notice:
1. **Non-interactive permission default (#1149).** CC's `claude -p` with no permission mode starts in `default` mode on third-party providers (2.1.302). Its permission requests are auto-denied unless a `--permission-prompt-tool` is set (2.1.285 fix text), and 2.1.319 adds `permissions.denyInsteadOfPrompting`. yoyo `-p`/piped approves every tool call. This is already on #1149, waiting on @yuanhao. Do not change the default.
2. **Read-before-overwrite (#1094 item 1).** CC's Write requires a prior read; 2.1.319 even fixed "Write ending the turn as a refusal when it couldn't read the file to overwrite". yoagent 0.27.3's ReadLog deliberately *allows* a never-read write (`tools/file.rs` ~360: "allowed for a file it never read (this guard is about partial reads only)"). So the comparison table for the owed decision is CC = refuse, yoagent = allow, yoyo = allow. The Day-252 lesson says this second "undecided" must become a decision with a user-terms reason, or a bounded either/or with my default posted to @yuanhao on #1094. Either way the test name `never_read_overwrite_is_still_allowed_item_1_undecided` should be renamed to the behaviour.
3. **Exit code after refusals.** CC 2.1.319: "`claude -p` exiting 0 in auto mode when its actions were refused because no safety check was available: now exit 1". Probe on yoyo: `yoyo --deny '*' -p "use bash: echo hi"` refused the command, the model said REFUSED, and the process exited **0**. The analogy is weak: CC's case is a missing safety check (a system failure), and mine is the user's own deny rule (working as configured). So this is an observation, not a filed defect. It only matters to scripts that want to know "did the agent do anything".
4. Not new: TUI (#215), benchmarks (#156) and the RLM roadmap (#341) are parked.

## Bugs / Friction Found
1. The misleading `-p` stray-word refusal for a valueless flag before `-p` (above). It's a product surface, and #1150 is closed, so it has no carrier.
2. #1127 residue: `docs`, `risk <bad>` and `memories` still print slash syntax in the shell. The ticket lists 11 verbs; 5 are done (grep, def, outline, blame, ast).
3. Size gate: help.rs and commands_risk_unhittable.rs are each within ~10–14 lines of FATAL. help.rs is the likely file for any help-text fix, so this is a trap for item 2.
4. #1094 item 1: never-read overwrite is still "undecided", pinned by a test named `..._item_1_undecided`. The Day-252 lesson says the second "undecided" must become a decision or a bounded ask to the owner. Claude Code requires a read before any overwrite. yoagent's own ReadLog choice should be checked.
5. #1122 open remainder: the upstream ask for a public `takes_sampling_params(id)` for custom/ollama/gateway arms. The OpenRouter 'ignored' claim is still unverified by a real request; it would need a key.
6. CLAUDE_CODE_GAP.md header is 178 days stale.

## Open Issues Summary
Community/owner: #1149 (decision waiting on @yuanhao, do not change the default), #1148 (upstream yoagent ask), #1122 (yoagent reuse, active), #991 (retry_safe_events; last activity Oct 29, my correction to @yuanhao pending reply), plus parked #854, #341, #215, #156.

agent-self backlog (15): #1147 (~24 unverified non-Claude ids), #1134 (MCP live-server lifecycle), #1127 (slash syntax, 6 left), #1121 (Bedrock inference-profile ids), #1118 (Gemini stale ids), #1102 (hard_deny heredoc false positive), #1100 (MCP servers outlive a panic), #1094 (write guard item 1), #1070 (multi-MB CLAUDE.md, silent), #1064 (child processes inherit the API key), #1060 (openai wire residue), #1040 (/cd trusted project reload), #1018 (durable todos), #1014 (/spawn write tap), #1012 (price drift; live models.dev layer pending).

## Research Findings
- yopedia recall: 2.1.319 and Codex CLI 0.167.0 were already scanned and ingested earlier today (`claude-code-and-codex-cli-release-changelog-2026-11-06`, `non-interactive-permission-handling`). I re-read the raw changelog only for the items above and ingested nothing new, because nothing rose above what is already filed.
- Other 2.1.319 items with a possible yoyo analogue, not probed:
  - "Changed what Claude is told after safeguards stop a subagent: retry with a narrower prompt or tell you, instead of doing the work itself". Open question: what does yoyo's parent get told when a sub_agent child is refused or fails?
  - "Fixed `/context` sending its full visual grid to Claude on every later turn". Worth checking whether any yoyo slash-command output is fed back to the model at full size.
- Suggested picks for the planner, my ranking:
  - (a) #1094 item 1: decide it, or post a bounded ask with the CC/yoagent table above, plus the test rename. Mostly a non-code action; the ask plus rename is small.
  - (b) #1127 remaining slash-syntax verbs (docs, risk, memories, and others on the ticket). Watch the help.rs size gate: 10 lines of headroom. Pay the gate entry first if the fix touches help.rs.
  - (c) The misleading stray-word refusal for `--system --model X -p hi`: no ticket. File it or fix it; the predicate is "a value slot took a dash-led word, and the leftover operand is blamed on -p".
  - (d) Pay commands_risk_unhittable.rs's unlisted size (36 over, unlisted) before any risk-module task.
