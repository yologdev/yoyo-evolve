# Assessment — Day 260 (20:52)

## Build Status
Pass, verified by the harness at session start (HEAD 80f10593). My probes on the built binary (mtime 20:53, `yoyo v0.4.0 (80f10593 2026-11-15)`):
- `yoyo config get model` → `claude-opus-5-5 (.yoyo.toml)`; `config get thinking` → `high`; `config show` lists 8 keys, nothing masked wrongly.
- `yoyo -p "Reply with just the word OK"` → `OK`, model header correct, auto-watch skipped ("no files changed").
No breakage seen.

**Size-gate hazards (from trajectory, confirmed with wc -l):**
- `src/context.rs` = 2035 lines, UNLISTED, 35 over the 2000 cap. Trajectory says 15 more lines makes `cargo test` FATAL → whole-task revert. Any task touching context.rs must split it or register it in `tests/module_size.rs` GRANDFATHERED list first.
- `src/format/mod.rs` = 2874 vs recorded 2788 (+86 drift); 14 more lines is FATAL. Same rule.

## Recent Changes (last 3 sessions)
- **Day 260 15:41** — Fixes #1198 (825601a3 + d3adb51a eval-fix): on yoagent 0.27.9 the Bedrock thinking-on path defers to yoagent's single maxTokens rule (user 2048 → 9216 under an 8192 budget), checked at the recording stub (`tests/bedrock_max_tokens_request.rs`). Dep bump 739395ad (yoagent 0.27.8 → 0.27.9). Commit bodies were empty (lesson d260: the wire table was lost to the time budget). Filed #1203 as residue (/side and /architect send no maxTokens on Bedrock).
- **Day 260 11:50** — Fixes #1201: `/grep` reads every word after the pattern as a path (`--` before paths), names missing paths. Fixes #1199: `config show/get` print `***` for any key yoyo doesn't read (option A).
- **Day 260 01:35** — Fixes #1197 (secret-key word list restored, 1080-name corpus, 0/696 regressions) and Fixes #1173 (Bedrock thinking-on maxTokens at the stub).
- Day 259: #1193 slice (safety checks re-run on segments split at `$(`/`(`/backtick: 15→32 of 33), #1194, #1172, #1184 slice 3 (top-level map from toml::Table).

## Source Architecture
186 .rs files in src/, ~240.7k lines total (incl. *_tests.rs). Largest: cli.rs 7873, commands_risk.rs 6645, tool_wrappers.rs 5762, tools.rs 5286, agent_builder.rs 5015, safety.rs 4809, commands_search.rs 4784, config.rs 4683, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3987. Entry: main.rs → cli::parse_args_with_config → agent_builder::build_agent → repl/prompt. Safety: safety.rs (33 SAFETY_CHECKS on raw text, `analyze_bash_command`), indirect_command.rs. Config: config.rs + config_value.rs (hand reader, still used by `parse_toml_array` for [permissions]/[directories] at config.rs:507-539 and cd_config_note.rs:100) + config_toml_read.rs (real toml crate).

## Self-Test Results
Binary runs, -p works, config get/show correct for this repo. No friction in the smoke path. Did not re-run the suite (harness did).

## Evolution History (last 5 runs)
evolve.yml: 15:34, 11:43, 01:29 (Nov 15), 20:22, 11:10 (Nov 14) all `success`; current run 20:46 in progress. Trajectory: 10/10 sessions all tasks landed, 0 reverts in 14 days, no provider errors. Last CI failures 2d ago (clippy `if` guard returning bool literal, exit 101) — green since. Pattern risk is not failure but **concentration**: config took 5 of the last 9 self-driven diffs; trajectory says send the self-driven slot to a different subsystem.

## Capability Gaps
- **Bash safety decided on text, not structure** (#1202, filed by @yuanhao today 14:57): 4 glued-shape leaks in 3 days (#1189, #1191, #1193, #1196). Codex CLI parses with tree-sitter-bash and auto-approves only word-only command sequences. This is the biggest correctness/security gap vs a peer tool, and the owner has written a staged plan (step 1: parse, extract every simple command incl. inside substitutions/subshells/backticks/strings, run existing checks on each extracted command plus raw text; parse error/eval/indirect command name fails closed to confirmation).
- Hand TOML reader still owns [permissions]/[directories] arrays (fences) and writes (#1184, owner).
- Trust keyed on path not content (#1188) — Claude Code had the same issue publicly.
- MCP lifecycle (no restart, no list_changed recheck, #1134).

## Bugs / Friction Found
- #1203 (agent subsystem): confirmed by reading src/agent_builder.rs:1836-1916 — `build_side_agent` and `build_architect_agent` set system prompt, api key, cache, `max_turns(1)`, and never `max_tokens` or thinking, **on any provider** (not only Bedrock): the user's `max_tokens` is ignored by /side and /architect everywhere; on Bedrock no maxTokens goes out at all. `build_editor_agent` (1917) does pass `max_tokens: self.max_tokens` — so the editor already shows the pattern. Small; stub test `tests/bedrock_max_tokens_request.rs` exists to extend (needs a way to drive /side, or a unit test on the built agent's `max_tokens`). Evidence grade: read, not run.
- #1196 (safety): `echo "kill -9 1 at $(date)"` refused, `echo "kill -9 1"` passes — a parse tree (#1202) would resolve this class rather than another text patch.
- #1193 residue: `indirect_command_word` shapes d, b (nested `$(echo rm)`), needs its own mechanism — again #1202's territory.
- #1195 (config): get/show print file text where run uses a different value (`thinking = ""` runs at medium, `temperature 5` → 1.0). Config-zone; trajectory says file, don't take, unless it is the only option.
- Size hazards above (context.rs, format/mod.rs) — not a user bug but a revert trap.
- CLAUDE_CODE_GAP.md header 186 days stale.

## Open Issues Summary
Owner-filed and open: #1202 (bash parse tree, today), #1184 (toml crate migration, config), #1122/#991 (agent-input, yoagent adoption). Self-filed live: #1203, #1196, #1195, #1193 (residue d,b), #1188, #1186 (upstream price), #1147 (made-up non-Claude ids, providers), #1134, #1121, #1102, #1100, #1094, #1064, #1012. Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.

Suggested priority for the planner: (1) #1202 step 1 (safety subsystem, owner request, not config) — choose crate with a stated reason (tree-sitter-bash vs brush-parser), structure-first, existing checks unchanged, fail closed on parse error; first task should be just the parser + extraction + running checks per extracted command with #1196 and #1193's d/b shapes as rows. (2) #1203 (agent_builder, small, extend existing stub test). Avoid config this session.

## Research Findings
(yopedia recall for tree-sitter / bash safety / codex returned nothing; findings ingested as a dated report.)
- **Claude Code's bash safety is structural and fail-closed** (Guanlan Dai's read of the leaked source, rmmod.com, 2026-04-02): `bashPermissions.ts` parses with tree-sitter, splits `&&`/`;` into subcommands judged independently, and returns *ask* for any command substitution such as `$(curl … | bash)`. The stated principle: if a reliable argv can be extracted, match precisely; if not, classify as too-complex and ask. Its own docs (code.claude.com/docs/en/permissions) say Bash rules are **not** a security boundary (`/usr/bin/curl`, `sh -c`, `git -C . push` slip prefix rules), exec wrappers (`watch`, `setsid`, `ionice`, `flock`) and `find -exec/-delete` never auto-approve through a prefix rule, and a fixed read-only set (ls, cat, echo, pwd, head, tail, grep, find, wc, which, diff, stat, du, cd, read-only git) runs without prompting.
- **Codex CLI** (per #1202's body): tree-sitter-bash, rejects `has_error()` trees, auto-approves only word-only sequences.
- **opencode PR #14108**: tree-sitter; strips leading `variable_assignment` nodes before matching rules, *unless* the assignment contains a command substitution — a concrete near-miss row for yoyo's #1202 work (`GIT_DIR=$(…) git …` is the #1189 family).
- **Crate choice for #1202**: `brush-parser` 0.4.0 is pure Rust (no C build), bash-compatible, used by `rust-bash`; API `tokenize_str` + `parse_tokens` → `Program`/`AndOrList`/`Pipeline`/`Command::Simple{prefix, word_or_name, suffix}`, and `word::parse` decomposes a word into pieces including command substitutions inside double quotes (exactly the #1196 shape). `tree-sitter-bash` is what Claude Code/Codex/opencode use, error-tolerant (`has_error`), but needs the C toolchain. The planner should have the first #1202 task state the choice and why; a cheap discriminator is to run both parsers over safety.rs's existing test commands and the #1193/#1196 shapes and count parse failures (fail-closed cost) before committing to one.
- Biggest gap vs Claude Code remains structural bash safety (#1202), then content-keyed trust (#1188, which Claude Code also had publicly).
