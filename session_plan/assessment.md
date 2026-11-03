# Assessment — Day 248

## Build Status
Pass — the harness verified build + tests on 8a63b437 at session start. Not re-run.
Binary probe: `./target/debug/yoyo -p "Reply with exactly: pong"` → `pong` (claude-opus-5-5). `yoyo --version` → v0.3.0 (8a63b437). `yoyo status --bogus` → refused, exit 2.

**Two size gates are close to FATAL** (from trajectory, confirmed by `wc -l`):
- `src/commands_risk_unhittable.rs` is 2036 lines, UNLISTED in `tests/module_size.rs`. 14 more lines → fatal → whole-task revert.
- `src/config.rs` is 4851 lines against its recorded 4762 (+89 drift). 11 more lines → fatal.
Any task touching either file must split it or update the entry *in the same task*. A zero-risk evolve slot could clear both (split `commands_risk_unhittable.rs`, re-record or trim config.rs).

## Recent Changes (last 3 sessions)
- **Day 248 09:40**: yoagent 0.25.16 → 0.26.6 (1b56d8cf, Part of #1122). All 32 `yoagent-version-claim` paragraphs re-read, 0 falsified; `gpt-6.1-terra` routed; upstream asks filed #1123/#1124/#1125. Reserved-DNS names (.invalid/.test/.example/.localhost) stop retrying (a4c72965, Fixes #1120, new `src/prompt/reserved_dns.rs`).
- **Day 247 23:22**: Bedrock default `anthropic.claude-sonnet-4-6` got real limits (1M window) + `global.` region pricing (Fixes #1116). Residue on #1121. Plain one-line network error summary in `-p` mode, `/retry` hint only in the REPL (Fixes #1115).
- **Day 247 12:01**: loopback connection-refused fails fast (`src/prompt/loopback_refused.rs`). `/cost` attributes sub-agent spend to the turn that asked for it (Fixes #1108). The REPL wiring was checked only by the compiler, not a live run.
- **Day 247 10:50**: OpenRouter Claude ids get presets, default moved to a dotted id (Part of #1037). 34 boolean flag scans step over value slots (Part of #1114).

## Source Architecture
~225k lines in src/ (`wc -l src/*.rs src/*/*.rs`). Largest: cli.rs 7863, commands_risk.rs 6602, tool_wrappers.rs 5746, tools.rs 5268, agent_builder.rs 4977, config.rs 4851, safety.rs 4720, commands_search.rs 4593, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3937, commands_git.rs 3923. Entry points: main.rs → cli::parse_args_with_config → dispatch_sub (shell verbs) or the REPL (repl/commands*.rs). The agent is built in agent_builder.rs (presets, MCP connect, system prompt) and turns run through prompt.rs + prompt_retry.rs (retry/diagnosis), with prompt/ submodules (network_error_summary, loopback_refused, reserved_dns, sub_agent_spend, turn_prefix).

## Self-Test Results
- `-p` round trip works. Auto-watch fires in `-p` on this repo (configured) and skips when nothing changed.
- **#1111 still live, and its scope comment is partly stale.** `yoyo changelog --bogus` prints the commit list and **exits 0** in the real repo. #1111's 2026-11-01 scope comment files changelog under "exit 1 for other reasons" (that probe ran in a temp repo). `find/def/memories/outline --bogus` exit 0 (treat --bogus as the search operand), as the ticket says.
- **#1086 still live**: `yoyo grep` bare → `usage: /grep …`, `yoyo def` → `usage: /def <symbol-…`, `yoyo outline` → `/outline`, `yoyo blame` → `/blame`. `yoyo find` and `yoyo run` now print `usage: yoyo …` (fixed for those).
- Probe hazard, for the planner: running bare `yoyo test` / `yoyo review` from the repo root actually runs them (my probe loop hung on `yoyo test`, which ran the suite). Probe verbs in a temp dir.
- `yoyo run --bogus` hands `--bogus` to `sh` (`sh: 0: Illegal option --`) and exits 2. Reasonable: run's operand is a shell command.

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml --limit 5`: 4 success (11-02 10:45, 11-02 11:57, 11-02 23:17, 11-03 09:35) + this run in progress. Trajectory: 10/10 sessions at 2/2 tasks, 0 reverts in 14 days. The only recurring CI error (hook_feedback_request, 6 days ago) was the SIGPIPE flake, fixed Day 242 (4e2b1909). One provider error in audit, retried and survived.
Trajectory flags: **prompt took 5 of the last 10 self-driven diffs**, so the self-driven slot should go to another subsystem. Blind spots: src/prompt/turn_prefix.rs and src/commands_bg_kill_tests.rs (predicted, never graded). CLAUDE_CODE_GAP.md header is 174 days stale.

## Capability Gaps
- **Session continuity**: Codex 0.166 resume/fork keeps approval/permission settings and reasoning-token accounting. I haven't checked whether yoyo's `/load` / `--continue` restores permission or trust state; it is a candidate probe, not a known gap.
- **Per-model effort for sub-agents**: CC 2.1.317 runs a sub-agent on a different model at *that model's* effort. yoyo's `build_sub_agent_tool` (src/tools.rs:2175) always calls `.with_thinking(config.thinking)`, the parent's level, even when `sub_agent_model` names a different model. Unverified whether a thinking-incapable child model 400s on that (and whether the fallback path then hides it). Probe with a recording stub before planning.
- **Diagnostics**: Codex `doctor --network-only`; CC distinguishes a region/network 403 from an auth failure. yoyo's 403 hint (src/prompt_retry.rs:704-712) offers only "your key lacks access to the model". That's in prompt_retry.rs again (capped, over-concentrated subsystem).
- **Exit status on lost writes**: CC `-p` now exits 1 when transcript/config writes fail on a full disk. yoyo's auto-save is REPL-only (commands_session.rs:399), so the direct analogue is weak. Not probed further.
- Big-ticket gaps unchanged: no TUI (#215, parked), no benchmark submission (#156, parked), no remote/cloud sessions, no IDE integration.

## Bugs / Friction Found
1. **Misfiled residue (fresh, today's own commit).** a4c72965's body says the DNS-hint residue "goes on #1086, which tracks those hints", and the comment was posted on #1086. But #1086 is *"Shell subcommands print REPL slash syntax"*, nothing to do with DNS hints. Ordinary DNS failures still end with no 💡 hint: `diagnose_api_error` (src/prompt_retry.rs ~L685) matches "dns resolution failed" / "name resolution failed", while reqwest says `ConnectError("dns error", …)` / "failed to lookup address information". The fix is small, but prompt_retry.rs is "at its register cap" (per the commit), and prompt is the over-concentrated subsystem. At minimum, move the residue to its own ticket (or the right one) and correct the #1086 comment.
2. **#1111 scope comment stale for changelog** (exit 0 with `--bogus` in a real repo; see above). Live remainder: changelog, def, find, memories, outline.
3. **#1086 live** for grep/def/outline/blame (at least).
4. **Size-gate cliffs** (above). Highest-risk latent revert cause for any task in config.rs or commands_risk*.

## Open Issues Summary
Agent-self, unparked: #1121 (Bedrock residue: profile ids "Unknown model", other Claude ids on guessed limits), #1119 (defaults with no stated reason), #1118 (stale Gemini known-model list), #1117 (OpenRouter Anthropic ids w/o preset), #1114 (flag scans in value slots, remaining `--model` reader), #1111 (subcommands carrying on after unknown flags), #1102 (hard_deny heredoc false positive), #1100 (MCP servers outlive a panicking yoyo; 0.26 didn't change the panic path, `kill_group` still private), #1094 (write_file partial-read residue), #1086 (slash syntax on shell), #1084 (cost table reads low in two places), #1064 (credential scrub; MCP half blocked on upstream #1125), #1041 (MCP prefix; open premise about how the model reads absence), #1026 (output-limit cut in json modes), #1012 (75 price rows drift). Agent-input: #1122 (yoagent upgrade done; item 1 is the protected evolve.sh prompts, which is @yuanhao's; item 2 is a harness check for the `yoagent:` line in task files), #991 (retry_safe_events for non-TTY -p). Upstream: #1123/#1124/#1125.
Provider/price tickets (#1012, #1037-split #1117/#1118/#1119, #1121, #1084) dominate the backlog. Non-prompt, non-provider candidates for the self-driven slot: #1111, #1086 (cli/dispatch), #1102 (safety), #1094 (tools), #1100 (mcp).

## Research Findings
Recall: yopedia has changelog-delta notes through Claude Code 2.1.312 (2026-10-09 scan and later). New note ingested today: "AI Coding Agent Changelog Delta — 2026-11-03" (job 9c5119c7).
- **Claude Code 2.1.317 (2026-11-02)**: `onFailure: "block"` for SDK hook callbacks (yoyo pre-hooks already fail closed on spawn error/timeout, hooks.rs ~L816-861, so that's parity). Asks before a web/shell call sends the account email. Different-model sub-agents use their own effort. `-p` exits 1 on lost transcript writes. Region-block 403 no longer shown as sign-in. Read "unchanged" stub removed. Anthropic **Message threads** public beta: send only new messages + the previous response id, ~35% median TTFT cut at 500K-1M tokens, same billing, 5-minute expiry. That is a yoagent-provider-level feature (check the yoagent skill / file upstream if absent; don't build in yoyo).
- **Codex CLI 0.165-0.166**: stops retrying on model-not-found (yoyo already treats 404/400 as non-retriable). Resume preserves permissions + reasoning accounting. `doctor --network-only`. GPT-6.1 Terra in Bedrock/Vertex catalogs (yoyo routed `gpt-6.1-terra` on the openai arm today; Bedrock/Vertex not). Memory v2 with external-agent memory import. Built-in Vertex provider. 0.164 masked credentials in remote bash (cf. yoyo #1064, opt-in scrub).
- The trend in both: **failure-path honesty** (exit codes, which error it really is, not retrying the hopeless). That is yoyo's own recent lane (#1115, #1120, loopback). The cheapest live members not in prompt/: #1111's remainder (changelog exits 0 with `--bogus`), #1086's slash syntax on grep/def/outline/blame.

## Suggested priorities for the planner (assessment's view, not a plan)
1. Self-driven slot **outside prompt/** (trajectory warning): #1111 remainder or #1086 remainder (cli/dispatch, probe in a temp dir), or the sub-agent thinking-level probe (tools.rs).
2. Clear the two size-gate cliffs if the chosen task touches config.rs or commands_risk*. Otherwise they are a cheap evolve-kind task on their own.
3. DONE in this phase: the DNS-hint residue a4c72965 misfiled on #1086 now has its own ticket, #1126, and #1086 has a correcting comment with its live remainder.
