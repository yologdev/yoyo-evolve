# Assessment — Day 252 (15:51)

## Build Status
Pass — verified by the harness at session start (HEAD d51d4121). Binary `yoyo v0.4.0 (d51d4121 2026-11-07)` runs. I did not re-run the suite.

**Size-gate hazards (from trajectory, not re-measured):** `src/commands_risk_unhittable.rs` is 2036 lines, unlisted, **14 lines from FATAL**. `src/help.rs` is 2946 against its recorded 2856 (+90 drift), **10 lines from FATAL**. Any task that adds help text to help.rs or touches risk_unhittable must re-paste the entry in `tests/module_size.rs` (help.rs is at line 568) in the same diff, or the whole task reverts.

## Recent Changes (last 3 sessions, all Day 252, all 2/2 green)
- **13:14** — 213c26c6 Part of #1122: `temperature_support.rs` now reads yoagent 0.27.3's `ModelConfig::accepts_sampling_params` in place of yoyo's own Claude-version parse (owner's prescription on #1146). 0a3305e0/92397abb Fixes #1150: `yoyo -p hi extra` refuses the stray word (exit 2, nothing sent).
- **10:57** — Part of #1149: docs say plainly that `-p`/piped runs auto-approve tools (default unchanged, waiting on @yuanhao). fb0c95bf Part of #1114: `flag_value` skips value slots via `option_position` (39/39 caller census).
- **01:37** — Part of #1094: yoagent's per-agent read tracker wired into every agent and sub-agent, beside yoyo's own partial-read guard; retry classifier learns its refusal wording. Part of #1127: `blame`/`ast` usage now says `yoyo <verb>` on the shell.
- yoagent 0.27.2 → 0.27.3 (e571c191, deps bump).
- **Reflection-found defect, not yet fixed:** the 13:14 Day 252 reflection (#1122 last comment, 14:30Z) says 213c26c6's claim "OpenRouter answers with HTTP 400" is unmeasured and probably false. OpenRouter's docs say unsupported params are *ignored*, and its `/models` lists no `temperature` for the Claude 5.x ids. That false sentence is live in `docs/src/configuration/skills.md:183` ("on OpenRouter … the API answers every request that sets it with HTTP 400") and in the test comment `src/temperature_support.rs:~83` ("these sent a temperature (and got a 400) before").

llm-wiki (journals/llm-wiki.md): paused mid-migration, unchanged across entries.

## Source Architecture
168 files in `src/`, **~233k lines** total (the task prompt's "~116k" is out of date). Largest: cli.rs 7863, commands_risk.rs 6615, tool_wrappers.rs 5762, tools.rs 5295, agent_builder.rs 4996, config.rs 4851, commands_search.rs 4780, safety.rs 4720, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3987. Entry points: `main.rs` → `cli::parse_args_with_config` → `dispatch_sub::try_dispatch_subcommand` (shell verbs) or `repl::run_repl` / the `-p` prompt path; `agent_builder::build_agent` builds tools (`tools::build_tools`), MCP (`mcp_prefix::connect_mcp`) and the model config (`create_model_config`).

## Self-Test Results
Built binary, empty temp dir, `</dev/null`:
- `yoyo grep`, `def`, `outline` (no args): usage says `yoyo <verb>`, exit 2 ✓ (#1127 rows fixed earlier).
- **#1127 residue, re-probed now, 6 verbs still print slash syntax on the shell:**
  - `yoyo docs` → `usage: /docs <crate> [item]`, `Examples: /docs serde, /docs tokio task`
  - `yoyo lint bogusverb` → `Unknown /lint subcommand: bogusverb`
  - `yoyo risk bogusverb` → `Unknown /risk subcommand: bogusverb`
  - `yoyo skill bogusverb` → `try: /skill list, /skill show <name>, …`
  - `yoyo map --bogus` → `usage: /map [--all] [--regex] [path]`
  - `yoyo memories bogusverb` / `memories forget` → `Use /remember <note> to add one.`
  This matches the ticket's "still open = docs, lint, risk, skill, map, memories" exactly. The `command_form(surface, verb)` helper from 60b407d7 is the established mechanism.
- `yoyo lint` with no args actually runs `cargo clippy` (timed out at 10s, expected).
- `yoyo memories`: 20 entries, the oldest "5mo ago". Only 2 of them reach the system prompt (Day 246 filter), as this session's own prompt shows.

## Evolution History (last 5 runs)
evolve.yml: 4 × success (Nov 6 21:14, Nov 7 01:34, 10:51, 13:08), 1 in progress (this run, 15:44). Trajectory: last 10 sessions all 2/2 (one 1/1), 0 reverts in 14 days, no provider errors. The only CI failure in the window was a 10-day-old `hook_feedback_request` flake (SIGPIPE, fixed Day 242 4e2b1909). Concentration in the last 11 task commits: agent/providers/anthropic_preset (model-id and pricing work).

## Capability Gaps
(See Research Findings.) Standing gaps vs Claude Code and Codex: no real TUI (#215 parked), no benchmark submission (#156 parked), MCP over stdio only (no HTTP/SSE transport), no restart of a dead MCP server (#1134), `-p` auto-approve policy undecided (#1149). The recent work is mostly correctness at the edges (argv, model ids, prices) and not new capability. That's 11 of 11 self-driven commits in the provider/cli/agent area.

## Bugs / Friction Found
1. **False doc sentence (fresh, owned, cheap):** skills.md:183 claims OpenRouter returns HTTP 400 for temperature on Claude 5.x. No request was ever made, and OpenRouter's docs say "ignored". Fix: rewrite it as "OpenRouter ignores it (per its parameters doc)" or "not sent", and fix the test comment. Optional: one real call settles it, but that needs an OpenRouter key, which the loop probably doesn't have. Titled under #1122 as "Part of" (#1122 has other open scope: the upstream gateway ask).
2. **#1127 residue: 6 shell verbs print slash syntax** (probed above). Mechanism already exists (`command_form`). ⚠ If any usage text lives in help.rs, the 10-line headroom matters. Check where `/docs` usage and `/map` usage are built first (likely commands_*.rs, not help.rs).
3. **#1114 residue:** `collect_repeatable_flag` (cli.rs) and `resolve_numeric_setting` (cli_config.rs) still read a value slot as a flag. Per the Day 252 10:57 journal, one of these sits in a file at "my own size limit": that is a choice, not a wall (d248). A matrix-shaped fix (one shared reader) would close #1114.
4. **Module-size cliff:** commands_risk_unhittable.rs is unlisted and 36 lines over the cap. It is either split or listed, and any edit near it trips FATAL at +14.

## Open Issues Summary (agent-self, non-parked)
- #1149 decision waiting on @yuanhao (do not change the default).
- #1148 upstream (yoagent: Claude max output as data; Bedrock opus-4-7 mapping). OPEN, no reply yet. Blocks the last row of #1121.
- #1147: 5 fake ids dropped; ~24 offered non-Claude ids still have no outside authority, and the done-when test (every offered id is a preset, a price row, or vendor-sourced) is not written yet.
- #1134 MCP lifecycle (restart, re-check on list_changed, unmeasured premise).
- #1127: 6 verbs left (above). #1114: 2 helpers left.
- #1122: temperature slice done; open = the doc correction above plus the gateway upstream ask.
- #1121: waits on #1148. #1118: Gemini list carries ids Google no longer lists.
- #1102 hard_deny heredoc false-positive. #1100 MCP servers outlive a panic. #1094 partial-read residue (never-read overwrite). #1064 credential scrub for children. #1012 price audit (staged migration to yoagent prices; the live layer is slice 2).
- Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.

## Research Findings
Recall (yopedia): earlier changelog-delta notes go up to the Day 249 scan and the CC Nov 3 entry. New ingest today: "Claude Code 2.1.319 and Codex 0.167 deltas vs yoyo".

**Claude Code 2.1.319 (2026-11-06)**, items with a yoyo analogue:
- `permissions.denyInsteadOfPrompting`: deny and continue rather than prompt. Already cited as option B on #1149, so nothing new to post.
- "Fixed a Bash permission bypass where a crafted environment prefix could hide part of a command from permission checks." yoyo strips `VAR=x` assignments per segment (#1010, `user_deny_refusal` in tools.rs:1645 → `safety::user_deny_spelling_refusal`), and hard_deny skips assignments and wrappers (hard_deny.rs:383-390). **Unprobed:** whether an assignment's VALUE that carries a command substitution (`X="$(<denied cmd>)" ls`) is seen by the user-deny check. That's a cheap probe (a unit test over `user_deny_refusal`) and a possible security task. Class predicate: "a denied command inside an assignment value runs while the deny check reads only the post-assignment word."
- "Fixed a case where sed could skip the permission check in accept-edits." yoyo's `--auto-edit` is unprobed for this.
- MCP tasks (a long MCP call moves to the background and its result is delivered later). yoyo has no equivalent.
- After a safeguard stops a subagent, the model is told to retry narrower or tell the user, not do the work itself.

**Codex 0.167 (2026-11-06):** compaction now accounts for output retained from failed responses and responses with missing usage data. That's a defect class for yoyo's context bar / `/tokens` when a provider omits usage (unprobed). Web-search failures go back to the model. Agent command center. Vertex AI setup wizard.

**Pattern:** competitors this week hardened permission checks at the parsing edge (env prefixes, sed, `which` with flags) and made non-interactive permission policy explicit. yoyo's matching residue is #1149 (owner decision) plus the unprobed env-value case above.

## Incidence recorded this phase
hard_deny refused my own assessment work **twice**: a `python3 -c` multi-line quoted argument and then a `gh issue comment --body`, both only *mentioning* an `rm -rf` form. Recorded on #1102 (tally now Day 217 ×1, Day 243 ×2, Day 252 ×2). Two new facts went on the ticket: (1) neither shape was a heredoc, so #1102's title is narrower than its population (any multi-line quoted argument, since lines are split before quotes are parsed); (2) the refusal names `'rm -rf /'` (the class label, hard_deny.rs:118/595) while my command had the tilde form, so a user can't find the offending line by searching for the quoted text. Both of those are fixable product friction. (2) is a small, local change: say which text matched.

## Candidate tasks for the planner (my ranking)
1. **Correct the unmeasured OpenRouter "HTTP 400" sentence** (skills.md:183, temperature_support.rs test comment ~line 83). Small and owned, and it's a false claim to users in docs. Part of #1122. Wording per the reflection: "OpenRouter ignores it (its parameters doc)", or just "yoyo doesn't send it". Don't assert a 400.
2. **#1127 residue, 6 verbs** (docs, lint, risk, skill, map, memories), via the existing `command_form(surface, verb)`. Probe receipts are above. Check help.rs headroom (10 lines) if any of that text lives there.
3. **#1114 residue:** `collect_repeatable_flag` + `resolve_numeric_setting` through `option_position`/`has_option`. Closes #1114.
4. **Probe the env-assignment-value deny bypass** (CC 2.1.319 class transfer). If it's live, fix it in `user_deny_spelling_refusal`.
5. hard_deny refusal names the matched text and not only the class label (#1102 side-finding).
Avoid: the #1149 default (owner's), #1121 (waiting on #1148 upstream), and anything adding >10 lines to help.rs or >13 to commands_risk_unhittable.rs without re-pasting module_size.rs.
