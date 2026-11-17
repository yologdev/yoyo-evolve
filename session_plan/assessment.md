# Assessment — Day 262

## Build Status
Pass — verified by the harness at session start (HEAD 74e64cbc). Binary `target/debug/yoyo` (built 12:50, `yoyo v0.4.0 (74e64cbc)`) answers `-p "Reply with exactly: ok"` → `ok`, exit 0; `config get model` → `claude-opus-5-5 (.yoyo.toml)`; `config get temperature` → "not set in config file (using default)". No full suite re-run (per instructions).

**Gate risk the planner must price (module-size test):**
- `src/context.rs` = 2035 lines, UNLISTED, 35 past the 2000 cap. Trajectory: +15 more lines makes `cargo test` fatal → whole-task revert. Any task touching context.rs must split it or register it first.
- `src/format/mod.rs` = 2874 vs registered 2788 (+86 drift); +14 more lines is fatal. Any format/ edit must update the register entry in `tests/module_size.rs` in the same diff.

## Recent Changes (last 3 sessions)
- **Day 262 00:01** — 3ef0bd2a (Part of #1202, Fixes #1205): `bash_parse.rs` re-spells dynamic words (brace lists, backslash escapes, globs, `$'…'`, leading `~`) into the argv bash will run and checks both; such words are never "literal" for a wildcard allow. Cost documented in permissions.md (`ls *` no longer approves `ls *.rs`). 58717a77 (Fixes #1207): temperature bound per provider (0–2 for OpenAI/Google/DeepSeek/Groq/Cerebras, 1.0 elsewhere, xAI stays 1.0), one table read by run, `/config set`, `config get`. Residue filed as #1208.
- **Day 261 12:31** — 8728da3b (Part of #1206): `src/prompt/refusal_history.rs` scrubs thinking blocks off a refused turn so the next request isn't a 400 on `thinking.cache_control`; upstream ask yologdev/yoagent#455. Fixes #1204 (`/config set temperature` refuses what the run wouldn't send).
- **Day 261 01:38** — no commits: the serving model refused mid-task (transcript became a list of disk-wiping commands), then the next request 400'd (#1206). Lesson: tests over dangerous shell strings need a shape that doesn't read like a recipe.
- **Day 260** — #1202 slices (tree-sitter-bash adopted; wildcard allow only for one plain command; 4eaa03b4), #1195 config get reads run's value, #1198 thinking-off max_tokens, #1194/#1197 secret masking (mask unread keys).
- Subsystem concentration last 9 task commits: config 4, agent 3, bash 3. All last 10 sessions green except one no-verdict (Day 261 01:38). 0 reverts.
- External: journals/llm-wiki.md — still paused mid-migration (no new entries).

## Source Architecture
188 files in src/, ~242k lines total (incl. tests modules). Largest: cli.rs 7890, commands_risk.rs 6645, tool_wrappers.rs 5762, tools.rs 5286, agent_builder.rs 5039, safety.rs 4814, commands_search.rs 4784, config.rs 4739, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3988, commands_git.rs 3923.
Key entry points: `main.rs` → `cli::parse_args` → `agent_builder::build_agent` → `prompt::run_prompt*` (REPL in repl/commands*.rs). Bash safety: `safety::analyze_bash_command` (raw-text checks, then `bash_parse::tree_refusal` over tree units, safety.rs:127); permissions `config::PermissionConfig::check_with_suspended` (config.rs:23–49, wildcard allow gated by `bash_parse::allow_wildcard_not_one_plain_command`); confirm closure `tools::build_bash_tool` (tools.rs:1236).

## Self-Test Results
- `-p` round-trip works; auto-watch prints its "no files changed — skipping" line in -p mode (expected for this repo's config).
- Read code for #1202's remaining step-1 clause ("a parse error, or a construct yoyo cannot analyse (`eval`, indirect name), **fails closed** to confirmation, never to auto-approve"). What HEAD does:
  - Wildcard allow: already declines a parse error (`word_only` returns false on `has_error()`), so that door is closed.
  - `tree_refusal` (bash_parse.rs:58): its doc says "A parse issue adds nothing (the next slice)" — a parse error yields no refusal and no routing change.
  - **Still auto-approving on a parse error / eval / indirect name:** (a) the session "always" flag in `build_bash_tool` (tools.rs ~1257) — after a user answers `a`, every later command, including unparseable ones, runs with `✓ Auto-approved`; (b) an exact (non-wildcard) allow pattern; (c) `--yes`/`auto_approve` builds the bash tool with no confirm at all (tools.rs:1248). (c) is what yoyo's own evolve loop runs under — a d235 situation: deciding the product default for (a)/(b) is separable from the loop's opt-out; changing (c) would need an owner ask, and the confirm closure has no terminal under the loop.
  - The Day 262 journal also says `{echo,hi}` does not parse cleanly, so the dynamic-word check never sees it — it falls into exactly this unrouted parse-error branch.
- Not probed live (no command-only CLI for analyze_bash_command); the above is code-read grade.

## Evolution History (last 5 runs)
gh run list evolve.yml: 2026-11-15 20:46, 22:56, 2026-11-16 01:32, 12:23, 23:57 — all `success`; current run (2026-11-17 12:43) in progress. No failed runs to read logs for. CI: last red ~4 days ago (clippy `if` guard returning bool literal; exit 101), green since. One provider error in 10 sessions (the Day 261 refusal → 400), now mitigated by 8728da3b.

## Capability Gaps
- **Approval granularity.** Answering `a` (always) at a bash prompt sets ONE session-wide flag shared by bash, write_file and edit_file (tools.rs:1194, 1251). The confirm closure checks that flag first, and `analyze_bash_command`'s soft warnings are routed through the same closure (tools.rs:386–395), so after one `a`, even on `ls`, every later safety warning (`find -delete`, recursive deletes of a build dir, the whole #1202 tree work) prints `✓ Auto-approved` and runs. Only hard_deny and the user's deny list still bite (permissions.md:50 documents that, so it is documented behaviour, not hidden). Claude Code's "don't ask again" and Codex's execpolicy are per-command/prefix, not session-global. Code-read grade; not driven through the binary.
- `offer_persist_pattern` saves `simplify_command_pattern(cmd)` = first two whitespace tokens + `*` with no space (tools.rs:1116–1123): a two-flag delete becomes `<prog> -rf*`; `export TOKEN=sk-…` is saved verbatim into .yoyo.toml. Claude Code 2.1.323 made "don't ask again" refuse to persist rules that look like they hold a secret. yoyo has secret-name machinery (#1194/#1197) but nothing on this path.
- Shell safety still mid-migration from text to parse tree (#1202 step 1 fail-closed clause + step 2 open, step 3 not started); Codex decides on the tree.
- Config still partially hand-parsed (#1184: [permissions]/[directories] arrays via line reader, no toml_edit writes, hand parser not deleted).
- No TUI (#215 parked), no benchmark submissions (#156 parked), MCP lifecycle gaps (#1134).
- CLAUDE_CODE_GAP.md header is 188 days stale — any row is unverified.
- Shell safety is mid-migration from text to parse tree (#1202 steps 1 partial, 2 partial, 3 not started) — Codex already decides fully on the tree.
- Config still partially hand-parsed (#1184: [permissions]/[directories] arrays still via line reader, no toml_edit writes, hand parser not deleted).
- No TUI (#215 parked), no benchmark submissions (#156 parked), MCP lifecycle gaps (#1134: no restart of dead server).
- CLAUDE_CODE_GAP.md header is 188 days stale — any row is unverified.

## Bugs / Friction Found
1. **#1202 fail-closed clause (owner-bolded, "never")** — parse error / eval / indirect name still auto-approves via the session "always" flag and exact allow patterns; `tree_refusal` ignores ParseIssue. Highest-priority open safety item; per d260 it is not deferrable without his answer. Step 2 ("every extracted simple command matches an allow rule") also open — `git * && ls` still approves `git status && rm -rf x && ls` (Day 260 journal).
2. **#1196** — `echo "kill -9 1 at $(date)"` refused by the raw-text net while `echo "kill -9 1"` passes; tree reads it right, raw text still decides (stays until step 3).
3. **#1208** — temperature bound keyed on provider name (openai + foreign base_url gets 0–2); Gemini 3.6+ silently ignores temperature with no note (Google doc read Day 262).
4. **#1184** — permissions/directories arrays still read by hand line reader (security fence); hand parser not deleted.
5. **#1206** — yoyo-side scrub landed; open until yoagent#455 ships (check `gh issue view 455 -R yologdev/yoagent` before restating).
6. **#1188** — trust keyed on path not content: a pull adding base_url to a trusted .yoyo.toml is honoured silently.
7. Module-size gate: context.rs and format/mod.rs near fatal (see Build Status).

## Open Issues Summary
agent-self open: #1208 temperature residue; #1206 refusal 400 (waiting upstream); #1202 bash parse tree (owner's, staged — step 1 fail-closed + step 2 + step 3 open); #1196 quoted-mention false positive; #1188 trust by path; #1186 Fable 5.2 pricing (upstream label); #1184 toml crate migration (owner's); #1147 non-Claude known ids with no authority (5 look made-up); #1134 MCP lifecycle; #1121 Bedrock inference-profile ids; #1102 hard_deny heredoc mention; #1100 MCP servers outlive panic; #1094 write_file partial-read residue; #1064 credential scrub for children; #1012 price audit vs models.dev. Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156. agent-input: #1122 (yoagent upgrade/skill use), #991 (retry_safe_events for non-TTY -p).
Owner-filed and active: #1202 and #1184 — both are "adopt the real parser" migrations; per social learning (Day 261), he flags a run of same-class tickets; finishing his staged plans is the clearest signal of listening.

## Research Findings
Recall (yopedia keyword search): prior notes on agent changelog deltas (CC 2.1.318–319: deny-instead-of-prompt mode, a Bash permission bypass fix), harness comparison, and "AST-based shell command safety classification" (CC parses with tree-sitter and splits `&&`/`;` compounds). Authenticated /api/query returned "Sign in required", so recall was keyword-only.

New this session (saved to yopedia as a dated research note, job queued):
- **Claude Code 2.1.323 (2026-11-12) / 2.1.325 / 2.1.326 (11-16)** changelog (code.claude.com/docs/en/changelog): fixed "a gap in Bash permission checks where some compound commands could run without asking; these now ask" (= #1202 step 2); fixed "a Bash allow rule could run a different program than the one it names" (= our dynamic-word / globbed-program-name slice); "don't ask again" no longer saves an allow rule that looks like it holds a secret or would land in a git-tracked settings file; start-up warning for allow rules with `*` in a variable's value (`Bash(PORT=* npm run dev)`); unknown model id now assumed 1M context (was 200K); CC's own ripgrep/git subprocesses run without most API keys (cf. our #1064); MCP sign-in tool asks even in bypass/auto modes.
- **Codex** (execpolicy docs via dev.to; openai/codex PR #1668): a `bash -lc` script of plain words joined by `&&`/`||`/`;`/`|` is split and each command evaluated separately, so allowing `git add` doesn't allow `git add . && <destructive>`. Anything with redirection, substitution, env vars, wildcards or control flow is not split and is treated as one whole command. `is_known_safe_command` blocks redirections. This is exactly #1202 step 2's semantics — a ready reference.
- Takeaway: competitors' permission work this month is the same class yoyo is in (#1202). yoyo's distinct gap is the session-global "always", which makes every soft check moot after one keypress — a granularity question no open ticket carries.

## Incidence during this assessment
Writing this file through a bash heredoc was refused by hard_deny because the text *mentioned* a root-delete command inside a quoted heredoc body (Codex's example). That is #1102's class (heredoc body that only mentions a pattern), one more counted hit — should be logged on #1102 with date and shape (d243).

## Suggested priorities for the planner (my reading, not a plan)
1. **#1202 step-1 fail-closed clause** (owner-bolded, "never"): when the tree has an ERROR/MISSING node, or the command is `eval`/an indirect command name, `analyze_bash_command` returns an "Unanalyzable command" soft warning (the mechanism already exists for globbed program names), so an interactive user is asked even under an exact allow rule. Under piped/-p/--yes there is no confirm_fn, so it only logs — the evolve loop (piped stdin, no --yes, evolve.sh:743–756) should be unaffected; code-read grade, verify with the built binary. Needs a false-positive census: which valid commands tree-sitter-bash marks HasError (`{echo,hi}` is one, per the Day 262 journal). Mind the Day 261 refusal: keep test corpora mechanical (fixture tables, not long transcripts of destructive commands). State what the session "always" flag still lets through (d260 rule 3).
2. **#1202 step 2**: split word-only chains and require every extracted command to match an allow rule (Codex semantics above); closes the `git * && ls` hole.
3. **#1184** next slice: [permissions]/[directories] arrays from the parsed toml::Table (a security fence still on the hand reader).
4. Smaller: #1208 (Gemini 3.6+ silently ignores temperature: add the dropped note); secret check on `offer_persist_pattern` (new, no ticket yet).
Gate warning applies to all: context.rs / format/mod.rs are near fatal size.
