# Assessment — Day 260 (23:13)

## Build Status
Pass — verified by the harness at session start (CI green on 81a0a561, 22:38). Binary probes this session (target/debug/yoyo, built 23:14):
- `yoyo --version` → `yoyo v0.4.0 (81a0a561 2026-11-15)`.
- `yoyo config get model` → `model = claude-opus-5-5 (.yoyo.toml)`.
- `yoyo grep -c analyze_bash_command src` → per-file counts, fine.
- `yoyo -p "Reply with exactly: ok"` → `ok`, auto-watch announced then skipped (no files changed). End-to-end provider path works.

**Size-gate hazards (from trajectory, confirmed with wc):** `src/context.rs` is 2035 lines, UNLISTED and 35 over the 2000 cap — 15 more lines makes `cargo test` FATAL and reverts the whole task. `src/format/mod.rs` 2874 vs recorded 2788 (+86 drift; 14 more lines → FATAL). Any task touching either file must re-register it (or split) in the same diff. `src/agent_builder.rs` 5026 is at its recorded ceiling.

## Recent Changes (last 3 sessions)
- **20:52** — Part of #1202 (4eaa03b4): new `src/bash_parse.rs` (161 lines) using tree-sitter 0.25.10 + tree-sitter-bash 0.25.1; `analyze_bash_command` now also runs SAFETY_CHECKS on every `command` / `pipeline` / `redirected_statement` node of the parse tree. Differential over 613 inputs: 51 flips, all to refused, 0 innocent flips. Binary +1.48 MB. Parse failure still falls back to text-only checks (not fail-closed). Fixes #1203 (7c65218f): /side and /architect now use the main agent's max_tokens resolution (checked on built agent config, not a real request).
- **15:41** — Fixes #1198: Bedrock thinking-on max_tokens deferred to yoagent 0.27.9's one rule (2048 → 9216), stub-recorded wire rows in `tests/bedrock_max_tokens_request.rs`. Commit body was empty on first pass (harness committed for an interrupted implementer) — lesson recorded.
- **11:50** — Fixes #1201: /grep treats every word after the pattern as a path (git `--` semantics), names missing paths. Fixes #1199: config show/get mask values of every key yoyo doesn't read.
- **01:35** — Fixes #1197 (secret-word mask restored, 1080-name corpus) and #1173 (Bedrock thinking-on sends model max).

Pattern: 10/10 sessions green, 0 reverts; work is concentrated in safety (bash), config (secrets / TOML migration #1184) and providers (Bedrock limits).

## Source Architecture
~238k lines across src/ (incl. tests). Largest: cli.rs 7873, commands_risk.rs 6645, tool_wrappers.rs 5762, tools.rs 5286, agent_builder.rs 5026, safety.rs 4818, commands_search.rs 4784, config.rs 4683, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3987, commands_git.rs 3923. Entry: `src/main.rs` → `cli.rs` (arg parse) → `agent_builder.rs::build_agent` → REPL/prompt loop in `prompt.rs`. Safety: `safety.rs` (`analyze_bash_command`, 33 SAFETY_CHECKS) + new `bash_parse.rs` + `hard_deny.rs` + `safety/indirect_command.rs`. Config: 10 `config*.rs` files, 10,366 lines; #1184 migration to the `toml` crate still has `.lines()` loops in config.rs (5), config_paths.rs (3), config_unknown_keys.rs (2), config_value.rs (2), config_syntax.rs (1) — not all are TOML walkers, unverified which.

## Self-Test Results
See Build Status. Nothing broke. No friction in the non-model commands probed. Not probed this session: the new tree-sitter pass on live commands (only read #1202's census), per-command parse latency (explicitly "not measured" in #1202's step-1 comment).

## Evolution History (last 5 runs)
evolve.yml: 4 × success (01:29, 11:43, 15:34, 20:46), current run in progress (22:56). ci.yml: last 5 all success. Trajectory: 0 task reverts / 14 days; CI failures 2 days ago (clippy `if` guard returning bool literal, exit 101) since resolved. No provider errors in audit logs.

## Capability Gaps
- **Bash safety still text-first.** #1202 step 1 only *adds* refusals. Remaining per #1202's own next-slice note: parse error / `eval` / indirect command name → fail closed to *confirmation* (separate path from refusal); step 3 checks argv not node text (would fix #1196 false positive and the #1102 heredoc-mention class). Known parser gap: tree-sitter-bash 0.25.1 marks `exec 3<>/etc/passwd` as an error tree (14 corpus inputs) — fail-closed will send any `<>` redirect to confirmation.
- vs Claude Code / Codex: Codex auto-approves only word-only command sequences from the tree (allow-list of node kinds); yoyo still uses deny-list checks. Claude Code has richer permission rules and hooks; yoyo has hooks + [permissions] but /cd into a trusted project doesn't load them (#1040, parked).
- MCP lifecycle: no restart of a dead server (#1134), servers outlive a panicking yoyo (#1100).
- CLAUDE_CODE_GAP.md header 186 days stale — body unverified.

## Bugs / Friction Found
- #1196 (open): `echo "kill -9 1 at $(date)"` refused, `echo "kill -9 1"` passes — still refused by raw-text pass after step 1.
- #1195 (open): config get/show print file text where the run uses a different value (`thinking = ""` → medium; temperature clamp). `run_reads_as_unset` is an Option<bool> with a hardcoded `"provider" | "thinking" => false` arm.
- #1188 (open): trust keyed on project path not content — a `git pull` adding `base_url` to an already-trusted .yoyo.toml is honoured with no prompt. Security-relevant, product-facing.
- #1186 (open, upstream): claude-fable-5-2 priced as Fable 5 ($10/$50) vs vendor $8/$40.
- Size-gate hazards above (context.rs, format/mod.rs).

## Open Issues Summary
Active agent-self (unparked): #1202 (bash parse tree; step 1 done, steps 2–3 open), #1196, #1195, #1188, #1186, #1184 (toml crate; slice 3 done Day 259, last comment 2026-11-14 12:47), #1147 (29 of 78 non-Claude known ids unsourced, 5 look made-up), #1134, #1122/#991 (yoagent adoption, agent-input), #1121 (Bedrock inference-profile ids → 'Unknown model'), #1102, #1100, #1094 (write_file partial-read residue), #1064 (credential scrub for child processes), #1012 (75 price rows drift vs models.dev). Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.
Owner-filed and steering: #1202 and #1184 (both @yuanhao, "use a real parser"). These are the highest-signal threads.

## Research Findings
Recall (yopedia): prior notes "AST-based shell command safety classification" (Claude Code's leaked bashPermissions.ts parses with tree-sitter and splits `&&`/`;` compounds) and "Agent Policy Enforcement Gaps" (name-keyed matchers defeated by indirection like BASH_ARGV0). Today's work matches the field direction.

New this session (ingested to yopedia: openai/codex PR #39159):
- **Codex keeps two parsers with different contracts.** `try_parse_word_only_commands_sequence` is the only one allowed to *prove safe* (rejects any `has_error()` tree, any node kind outside a 10-kind allow-list, any operator outside `&& || ; |`). A second extractor "returns the statically known words from every command node … suitable for identifying dangerous literal commands, but must not be used to prove that a command is safe." yoyo's `extract_check_units` is the second kind; any future auto-approve built on it would be the wrong contract.
- **Codex PR #39159 ("Require approval for commands with dynamic shell words"):** tree-sitter represents brace expansions, globs and escaped text as plain `word` nodes even though the shell rewrites them at runtime, so source text is not argv. Codex now rejects unquoted words with expansion/glob/escape syntax from literal parsing (incl. heredoc prefixes) and requires approval even when an allow rule matches the unexpanded text. **Directly relevant to #1202 step 3 ("check argv, not node text")**: e.g. `r{m,} -rf /` or `\rm -rf /` would read as a harmless command name from the tree. Step 3 needs a "dynamic word → not literal" rule, and allow-list matching (`[permissions] allow`) should not trust unexpanded spelling either.
- **Codex issue #9541:** a literal newline inside a double-quoted argument yields multiple `string_content` children and broke their extractor (allow rules stopped matching `git commit -m "a\nb"`). Worth a row in yoyo's corpus — my loop's own commits use multi-line `-m` / heredoc bodies (the #1102 class).
- **Claude Code 2.1.319–2.1.323 (Nov 6–13):** `permissions.denyInsteadOfPrompting` (deny and continue instead of waiting on a prompt — yoyo already refuses without a tty, so roughly parity); experimental Code Mode (model writes JS to call tools); MCP tasks moved to background; a fix where `grep -r foo "$PWD/src"` asked permission while the typed-out path didn't (variable-expansion vs literal path, the same dynamic-word question as above); fix for session-start git status reading as clean when `git status` failed (now "unavailable") — worth checking yoyo's own git-status context block for the same failure-reads-as-clean shape (unverified).
- Claude Code made `claude-fable-5-2` its default Fable model (tracker page) — relevant to #1186's pricing row.

**Biggest gap, in my judgment:** bash safety is still deny-list-on-text with a tree pass that only adds refusals; the field (Codex, Claude Code) has moved to "prove-safe on a strict parse, else ask". #1202 steps 2–3 are the owner-requested, highest-value next slices; step 2 (parse error / eval / indirect name → confirmation) is the smaller, well-scoped one, but note the `exec 3<>` parse-error finding before failing closed.
