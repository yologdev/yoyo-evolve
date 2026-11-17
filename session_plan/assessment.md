# Assessment — Day 262 (23:24)

## Build Status
Pass — verified by the harness at session start (HEAD e09df643). Binary `target/debug/yoyo` built 23:25,
`yoyo --version` = v0.4.0 (e09df643 2026-11-17). `-p "Reply with exactly: pong"` → `pong` on
anthropic/claude-opus-5-5. `config get model` reads `.yoyo.toml`. No full suite re-run (per instructions).

**Size-gate traps for the planner (from trajectory, confirmed with wc -l):**
- `src/context.rs` = 2035 lines, UNLISTED in tests/module_size.rs; +15 more lines makes `cargo test` FATAL. Any task touching it must add the register entry in the same diff (or split).
- `src/format/mod.rs` = 2874 vs registered 2788 (+86 drift); +14 more is FATAL. Same rule.

## Recent Changes (last 3 sessions)
- **Day 262 12:50** — Part of #1202 (fail-closed clause): `bash_parse::unanalyzable_reason` (parse error / `eval` / indirect command name) now asks even after "always" or under an exact allow rule; 0/30 ordinary commands ask; known cost `eval "$(ssh-agent -s)"` asks every time. Commits 62df6b10 + d4789f4f share a title (check `git show --stat` before calling either "empty" — Day 262 lesson). Still open: under `--yes`, `-p`, non-tty nobody is asked → unreadable commands still auto-run; A/B posted to @yuanhao on #1202 at 13:22Z, **no reply yet**. Also Fixes #1121: Bedrock picker dropped `anthropic.claude-opus-4-7`, test that every listed Bedrock Claude id resolves a yoagent preset.
- **Day 262 00:01** — Fixes #1205 (Part of #1202): commands checked twice, as typed and as bash expands them (brace lists, backslash-escaped names); a word bash rewrites never gets a wildcard-allow yes (`ls *` allow no longer covers `ls *.rs`). Fixes #1207: per-provider temperature range table (0–2 for OpenAI/Google/DeepSeek/Groq/Cerebras, 1.0 else), one authority for run, `/config set`, `config get`. Residue #1208 (keyed on provider name; Gemini 3.6+ ignores temperature).
- **Day 261** — Part of #1206: `prompt/refusal_history.rs` scrubs thinking off a refused turn so the next request isn't a 400 (upstream yoagent#455 still OPEN). Fixes #1204: `/config set temperature` refuses values the run would clamp. One session (01:38) produced nothing: model refusal during a census of disk-wiping shell shapes.

Subsystem concentration last 10 task commits: bash/bash_parse/safety 4/10 (+3), config 4/10. The loop has been in the shell-safety + config lane for ~5 days.

External: journals/llm-wiki.md — last entry 2026-04-06; journal says "paused mid-migration" every entry. Nothing new.

## Source Architecture
~240k lines Rust across 188 files in src/ (incl. subdirs). Largest: cli.rs 7890, commands_risk.rs 6645,
tool_wrappers.rs 5762, tools.rs 5301, agent_builder.rs 5039, safety.rs 4814, commands_search.rs 4784,
config.rs 4739, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3988, commands_git.rs 3923, symbols.rs 3804.
Key entry points: main.rs → cli.rs (`parse_args_with_config`) → agent_builder.rs (`build_agent`,
`create_model_config`) → prompt.rs (`run_prompt*`) / repl. Safety: tools.rs `build_bash_tool` confirm closure →
safety.rs `analyze_bash_command` + bash_parse.rs (tree-sitter-bash: `extract_simple_commands`,
`unanalyzable_reason`, `pre_prompt_route`). Config: config.rs (+ config_value.rs, config_unknown_keys.rs), toml crate
1.x already a dependency. yoagent 0.27.9 (latest release, 2026-11-15).

## Self-Test Results
- Binary runs, `-p` works, `config get`, `config show`, `doctor` work.
- **Probe (built binary, temp repo):** a standard multi-line TOML array
  ```
  [permissions]
  deny = [
    "rm *",
  ]
  ```
  `yoyo doctor` → `warning: .yoyo.toml:2: could not read the value of '[permissions] deny' (an array over several lines); it was ignored. This rule is NOT in force.` python3 tomllib reads it as `{'permissions': {'deny': ['rm *']}}`. Same for `[directories] deny`. So the most common way people format a long list builds **no fence** (warned, not silent). This is #1184's remaining slice: `parse_permissions_from_config` / `parse_directories_from_config` (config.rs:488-550) are still line walkers over `content.lines()`; the top-level map already comes from `toml::Table` (Day 259, de85ac91). `config show` prints "(no keys parsed from this file)" for a file that holds only sections — accurate but it doesn't show the permission/directory lists at all.
- Also still hand-written: `set_toml_key` (config.rs:1598) line-rewrites for `/config set` (#1184 lists toml_edit writes as open).

## Evolution History (last 5 runs)
evolve.yml: current run in progress (23:18Z); previous four (2026-11-16 01:32, 12:23, 23:57; 2026-11-17 12:43) all
`success`. Trajectory: last 10 sessions 9 fully green, 1 (Day 261 02:08) no verdict (refusal → #1206). 0 reverts in
window. CI failures: 2 older (4d ago, clippy `if` guard returning bool literal) — green since. Provider health:
1 retried error, 0 terminal give-ups.

## Capability Gaps
- **Non-interactive shell safety has no boundary.** Under `--yes`/`-p`/non-tty yoyo has no confirm callback and no sandbox, so an unreadable command (parse error, `eval`, `$cmd`) runs. Codex `exec` forces approval=never but pairs it with a read-only/workspace-write sandbox; Claude Code 2.1.323 now *refuses* a permission-check failure "where nobody can answer". yoyo has neither. Decision is with @yuanhao on #1202 (A default = unchanged); do not ship B before he answers.
- **Child-process credential scrub (#1064):** Claude Code 2.1.323 runs its own ripgrep/git "without most of its sign-in tokens and API keys". yoyo's children inherit the provider key; #1064 proposes opt-in.
- **Rule-shape warnings:** Claude Code 2.1.326 warns at startup when an allow/deny entry "doesn't look like a complete rule", and 2.1.323 warns on `*` inside a variable's value (`Bash(PORT=* npm run dev)`). yoyo already warns on wildcard directory entries; not checked for bash allow-rule shapes this session.
- **Sandboxing** (Codex workspace-write, CC Bash sandbox) — yoyo has none; large, not a one-session task.
- Shell safety: #1202 step 1 nearly done; steps 2–3 (owner's plan) not started. Non-interactive fail-closed decision pending owner.
- Config: multi-line arrays in [permissions]/[directories] dropped (above); toml_edit writes.
- CLAUDE_CODE_GAP.md header 188 days stale — rows unverified.

## Bugs / Friction Found
1. Multi-line `[permissions]`/`[directories]` arrays ignored (fence not built, warned). #1184 open slice. Product-facing, security-relevant.
2. `src/context.rs` unlisted past cap (2035/2000, FATAL at 2050) — latent revert trap.
3. `src/format/mod.rs` +86 register drift — latent revert trap.
4. #1202 fail-closed under `-p`/`--yes`/non-tty: unreadable commands still auto-run (awaiting owner A/B).

## Open Issues Summary
agent-self open: #1208 temperature residue (provider-name keyed; Gemini 3.6+ upstream), #1206 refusal→400 (yoyo half
done, upstream yoagent#455 OPEN), #1202 tree-sitter bash safety (step 1 near-done; A/B to owner pending since 13:22Z),
#1196 echo "kill -9 1 at $(date)" false refusal, #1188 trust keyed on path not content, #1186 claude-fable-5-2 pricing
(waits on yoagent#456 OPEN), #1184 toml crate (remaining: permissions/directories arrays, toml_edit writes, delete
hand parser), #1147 non-Claude ids with no outside authority (29 of 78), #1134 MCP lifecycle, #1102 hard_deny heredoc
mentions, #1100 MCP servers outlive panic, #1094 write_file partial-read residue, #1064 opt-in credential scrub,
#1012 price drift vs models.dev. Owner-filed/agent-input: #1122 yoagent upgrade + skill reuse, #991 retry_safe_events.
Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.
Upstream (yologdev/yoagent): #455, #456 both OPEN.

## Research Findings
- yopedia recall: last rival scan was Claude Code 2.1.318–2.1.319 + Codex 0.167 (Day ~249–255 pages); 199 learning pages, several on bash parse-tree safety, wildcard allow injection, TOML parsing, project trust.
- **Claude Code 2.1.323 (Nov 12) – 2.1.326 (Nov 16)** (code.claude.com/docs/en/changelog): incomplete-rule startup warning; `*`-in-variable allow-rule warning; refuse-where-nobody-can-answer for hook-rewritten input; internal git/rg run with credentials stripped; secret-looking "don't ask again" rules are session-only; unknown-model window assumption 200K→1M; VSCode trust prompt before first session in an untrusted folder (cf. yoyo #1188, trust keyed on path); sandbox ignores write grants through symlinks. Also 2.1.321 added `claude-fable-5-2` at $8/$40, $0.20 cache read — matches #1186's vendor figures (still waiting on yoagent#456).
- **Codex CLI 0.167**: `codex exec` cannot pause for approval (policy forced to `never`); blocked actions fail back to the model; exec default sandbox is read-only. "No prompts" is always paired with a sandbox.
- Saved to yopedia: "Claude Code 2.1.323-2.1.326 and Codex exec approval semantics (Day 262)".

## Candidate work for the planner (my read, not a plan)
1. **#1184 next slice — `[permissions]`/`[directories]` from the parsed `toml::Table`** (config.rs:488-550). Live, product-facing, security-relevant (a standard multi-line deny list builds no fence; probe above). Owner-filed ticket. Done-when per #1184's own text: no line-by-line TOML reading left; report the walker count before/after (Day 259 lesson). Watch: other readers of these sections (`config_unknown_keys`, `config_value::warn_unreadable_values_once`, `/permissions`, doctor) must agree — census the readers of `[permissions]`/`[directories]`, not just the two functions.
2. #1196 false refusal on `echo "kill -9 1 at $(date)"` — bash safety, has a carrier ticket; could now be decided on the parse tree (#1202 substrate exists).
3. #1188 trust keyed on path not content — CC just added a trust prompt; real security gap for product users.
4. Do NOT pick #1186/#1206-upstream/#1148 (all waiting on yoagent #455/#456, both OPEN), nor #1202 option B (owner hasn't answered).
5. Any task touching `src/context.rs` or `src/format/mod.rs` must include the module_size.rs register update.
