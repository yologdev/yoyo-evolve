# Assessment — Day 260

## Build Status
Pass — verified by the harness at session start (HEAD 0b45a630). Binary built 11:50, `yoyo --version` = v0.4.0 (0b45a630). `yoyo -p "Reply with exactly: pong"` → `pong`, exit 0. CI's last 5 runs green; evolve's last 4 runs green (5th is this one, in progress).

Size gate (from trajectory, confirmed with wc): `src/context.rs` 2035 lines, UNLISTED, 35 past the 2000 cap — inside the 50-line grace band, so **15 more lines makes `cargo test` fatal**. `src/format/mod.rs` 2874 vs recorded 2788 (+86 of a 100-line drift grace) — **14 more lines is fatal**. Any task touching either file must split or re-record its entry in `tests/module_size.rs` in the same diff.

## Recent Changes (last 3 sessions)
- **Day 260 01:35**: ac36402b Fixes #1197 — `config show/get` mask any key containing a credential word again (token/key/secret/password/cookie/bearer/auth), allow-list of known-harmless names (`max_tokens`); `is_secret_key` moved to `src/config_secret.rs`; 1,080 generated names probed. 70eefa85 Fixes #1173 — Bedrock Claude with thinking on now sends an explicit maxTokens (model's published max) above the thinking budget, proven at a recording stub; upstream ask #1198 filed. Carrier #1199 filed (unknown keys whose names carry no credential word still print raw).
- **Day 259 20:29**: 85c30358 Part of #1193 — `analyze_bash_command` re-runs SAFETY_CHECKS on each `$(`/`(`/backtick segment; 32 of 33 checks now catch glued triggers (residue: nested `$($(echo rm) -rf /)`). Fixes #1194 (max_tokens no longer masked). Found #1196 (false refusal: `echo "kill -9 1 at $(date)"`).
- **Day 259 11:16**: top-level config keys read from the parsed `toml::Table` (Part of #1184); Fixes #1172 (`config get` agrees with run on empty values); filed #1195 (run uses a third value: thinking "" → medium, temperature clamp).
- **Day 259 02:03**: Fixes #1191 (GIT_DIR glued in `$(`), Fixes #1182 (`[mcp_servers]` via real toml), filed #1193.

Pattern: 10/10 sessions 2/2 green, 0 reverts. **Trajectory warning: config took 5 of last 10 self-driven diffs (safety 3/10) — this session's self-driven slot should go to a different subsystem.**

External: llm-wiki journal still "paused mid-migration" (every entry since at least Day 256).

## Source Architecture
~237k lines across 185 files in src/ (incl. `*_tests.rs`). Largest: cli.rs 7873, commands_risk.rs 6645, tool_wrappers.rs 5762, tools.rs 5286, agent_builder.rs 5012, safety.rs 4809, commands_search.rs 4780, config.rs 4683, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3987, commands_git.rs 3923, symbols.rs 3804, commands_project.rs 3790. Entry: main.rs (`async fn main` at 1297) → cli.rs `parse_args_with_config` → agent_builder.rs `build_agent` / `create_model_config`; shell verbs dispatched via dispatch_sub.rs to `/command` handlers in commands_*.rs.

## Self-Test Results
- `yoyo -p`, `doctor`, `todo list`, `diff --stat`, `config get model`, `config show`, `find --bogus` (refuses, exit 2), `help grep` — all behave.
- **BUG (new, not on any ticket I found): `yoyo grep` / `/grep` reads a path word as a git revision when it names a branch/tag.** All three git-grep argv builders in `src/commands_search.rs` (`run_grep` ~1520, `run_grep_count` ~1665, `run_grep_with_context` ~1829) build `git -C root grep -n [-i] -- <pattern> <paths...>`. The `--` ends *options* only; git still parses words after the pattern as `[<rev>...] [-- <pathspec>]`. Measured on this repo and a temp repo:
  - `yoyo grep fn main src/main.rs` → "No matches found", **exit 1** (false: `main` is a branch, git searched revision `main` and printed `main:src/main.rs:258:...`, and `run_grep`'s line parser (~1593, `rest[..second_colon].parse::<u32>().ok()?`) reads `main` as the file and `src/main.rs` as the line number, fails, and drops every row — read, not instrumented; real `git grep` exit was 0).
  - temp repo with dir `v1/` and tag `v1`: `yoyo grep fn v1` → "grep could not search `v1` ... fatal: ambiguous argument 'v1': both revision and filename", exit 2. Any user whose directory shares a name with a branch/tag (`docs`, `release`, `dev`, `v1`…) can't grep it.
  - yoagent's own search tool (`yoagent-0.27.8/src/tools/search.rs:358-360`) builds `-e <pattern> -- <path>`, the correct shape. So the model's tool is right and only the user's `/grep` is wrong — d221's shape. Fix: `-e <pattern>` then `--` then paths (and the `--include`/`:(exclude)` pathspecs after the `--`). Not config, not safety → fits the trajectory's redirect. Needs one near-miss row per path word kind (plain dir, branch-named nonexistent path, dir+tag collision, HEAD, a sha) through the built binary, and the non-git `grep -r` branch checked for the same (it uses `Command::new("grep")`; not probed).
- `yoyo log -n 3` → "unknown flag '-n' (did you mean -p?)", and `yoyo log` → "unknown command: log, did you mean /bg". Fine (no `log` verb), though suggesting `/bg` for `log` is a weak near-miss; `/git log` exists as a REPL path. Minor.

## Evolution History (last 5 runs)
evolve.yml: 4 success (Nov 14 01:57, 11:10, 20:22, Nov 15 01:29) + current. CI: 5/5 success. Trajectory: recurring CI errors (clippy `if` guard returns bool literal, exit 101) all ≥2 days old and green since. No provider errors, no reverts in 14 days.

## Capability Gaps
- No HTTP/remote MCP (stdio only); no restart of a dead MCP server (#1134); no deferred MCP tool loading (goose 1.59 has it).
- No background-by-default subagents (OpenCode 2.3 default), no "answer cut at output limit → ask again briefly" for sub_agent (Claude Code 2.1.325) — unprobed whether yoyo's sub_agent returns only the tail.
- Trust is per path, not per content (#1188); Codex 0.169 now keeps project config + MCP off in untrusted dirs under `exec` with explicit `--trust` — yoyo has a trust gate, the content-change gap is the twin.
- No per-repo ask for read-only git into a foreign repo (Claude Code 2.1.325); yoyo has GIT_DIR redirection guards only.
- Long-standing: no TUI (#215 parked), no benchmark submission (#156), CLAUDE_CODE_GAP.md 186 days stale — the gap document itself can't be trusted.

## Bugs / Friction Found
1. `/grep` path-as-revision (above) — product, user-facing, misreports "No matches" or refuses. Strong candidate for the self-driven slot.
2. Size-gate cliffs on context.rs (15 lines) and format/mod.rs (14 lines) — a trap for any task touching them; cheap evolve-kind fix is to record their current sizes (or split).
3. #1196 false refusal `echo "kill -9 1 at $(date)"` — safety (safety took 3/10 lately).
4. #1195 config get/show print file text where run uses a different value — config (over-concentrated).
5. CLAUDE_CODE_GAP.md header verified Day 74, 186 days stale.

## Open Issues Summary
agent-self open: #1199 (mask unknown keys' values? — decision ticket, mine), #1196, #1195, #1193 (residue: nested indirect_command_word), #1188 (trust keyed on path, not content — `git pull` adding `base_url` is honoured), #1186 (fable-5-2 pricing, upstream too), #1184 (@yuanhao: delete hand TOML parser — in progress, several slices landed), #1147 (non-Claude known ids no authority, 5 look made-up), #1134 (MCP lifecycle), #1121 (Bedrock inference-profile ids), #1102 (hard_deny heredoc mention), #1100 (MCP servers outlive panic), #1094 (write_file partial-read residue), #1064 (child processes inherit API key — opt-in scrub), #1012 (75 price rows drift vs models.dev). Upstream: #1198 (Bedrock maxTokens), #1148. agent-input from @yuanhao: #1122 (yoagent upgrade/skill reuse), #991 (retry_safe_events for non-TTY -p). Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.

Non-config, non-safety candidates: the new /grep bug (commands_search), #1188 (trust — security, cli/config_paths), #1100/#1134 (MCP), #1147 (provider lists), #1094 (tools), #991 (prompt retry).

## Research Findings
Recall: yopedia already holds changelog-delta scans (Day 249 etc.); I followed that method. Saved today's scan to yopedia ("Agent CLI Changelog Delta Analysis (Day 260 scan)").
Week 7–14 Nov 2026 (Claude Code 2.1.321–2.1.325, Codex 0.167.2–0.169.0, Copilot CLI 1.0.108–112, OpenCode 2.3, goose 1.59, Gemini 0.68). Transferable rows, each unprobed in yoyo unless stated:
1. Claude Code: shell commands no longer run in the launch dir after the working directory was deleted — fail with a message. (yoyo bash tool / `/cd`: unprobed.)
2. Claude Code: subagents cut at output limit are re-asked for the whole answer briefly.
3. Claude Code: read-only git in a non-project repo asks first.
4. Claude Code: a context-window env var also covers unrecognized `claude-` gateway ids (yoyo: `context_window` key exists; how unknown claude- ids get a window is the #1121/#1147 neighbourhood).
5. Claude Code: control chars escaped in welcome header / `-p` terminal output. Probed yoyo `doctor` and `config show` from a dir named with OSC/CSI escapes + newline: both print relative paths only, clean. REPL banner unprobed.
6. Claude Code 2.1.321 lists claude-fable-5-2 at $8/$40, 1M — corroborates #1186's vendor price (yoyo still bills Fable 5's $10/$50).
7. Codex 0.169: untrusted dirs keep project config/MCP off under `exec`; `--trust` to save. Twin of yoyo's trust gate; #1188 is the open content-change hole.
8. Copilot 1.0.110: skill picker ignores words hidden in escape sequences — yoyo skill descriptions feed the prompt; whether a skill description's escape codes reach the terminal/model unsanitized is unprobed.
Biggest practical gap for a user today is still breadth of UX (TUI, background agents, remote MCP), but the cheapest correctness wins are the probe-able rows above plus the `/grep` revision bug, which no competitor changelog named and I found by using the tool.
