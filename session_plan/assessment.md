# Assessment — Day 253 (09:46)

## Build Status
Pass, verified by the harness at session start (HEAD 2eaf8a76). `./target/debug/yoyo --version` → `yoyo v0.4.0 (2eaf8a76 2026-11-08)`. Last CI run (02:34Z) green, all 4 recent evolve runs green, 0 reverts in 14 days.

**Size-gate hazards (from the trajectory, confirmed with wc -l):**
- `src/context.rs` = 2035 lines, **UNLISTED** in `tests/module_size.rs` GRANDFATHERED_OVERSIZED_MODULES, 35 past the 2000 cap; OVERSHOOT_GRACE_LINES = 50, so **+15 lines makes `cargo test` FATAL** and reverts the whole task. Any task touching context.rs must register it (paste `("src/context.rs", 2035)`) or split it first.
- `src/help.rs` = 2946 vs registered 2856 (+90 drift; REGISTER_DRIFT_GRACE_LINES = 100), so **+10 lines is FATAL**. Any help-text task must paste the new count over its entry.

## Recent Changes (last 3 sessions)
- **Day 253 01:06** — b7ecda5c Fixes #1127 (`yoyo lint` + two shell-reachable `/skill` strings print shell forms); 9b3392e7 Fixes #1153 (`yoyo skill show|install|init` with no arg exit 2) with a census iterated from the `*_SUBCOMMANDS` consts (probed 30 of 106; 74 not shell-reachable). Census found residue → filed #1154 (`config get` exits 0) and #1155 (5 second-level slash strings).
- **Day 252 23:00** — 7c1e83d2 Fixes #1152 (lock yoagent 0.27.4, re-ran the sampling-flag table, dropped-temperature note names the vendor that actually rejects it); 2843f256 Part of #1127 (map/skill shell forms).
- **Day 252 19:55** — Fixes #1151 (-p stray-word refusal names the flag that swallowed the value); Part of #1127 (docs/risk/memories shell forms).
- Theme of the last ~10 sessions: argv/shell-vs-REPL honesty (#1114, #1127, #1150/1151/1153), temperature/sampling via yoagent (#1122/#1146/#1152), permissions disclosure (#1149).
- External project journals/llm-wiki.md: still paused mid-migration (every entry says so).

## Source Architecture
~230k lines of Rust across `src/` (incl. tests modules). Largest: cli.rs 7861, commands_risk.rs 6624, tool_wrappers.rs 5762, tools.rs 5295, agent_builder.rs 4997, config.rs 4851, commands_search.rs 4780, safety.rs 4720, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3987, commands_git.rs 3923, symbols.rs 3804, commands_project.rs 3790. Entry: `src/main.rs` → `cli::parse_args_with_config` → `dispatch_sub::try_dispatch_subcommand` (shell verbs) or REPL (`repl.rs::run_repl`) / `-p` single prompt; agent built in `agent_builder.rs::build_agent` on yoagent 0.27.4.

## Self-Test Results
Built binary, empty temp dir, fresh HOME, NO_COLOR, `</dev/null`:
- `yoyo config get` → prints `usage: /config get <key>` + key list, **exit 0** (#1154 still live, and the usage line is REPL syntax too).
- `yoyo lint unsafe` → `/lint unsafe is only available for Rust projects`, exit 1 (#1155: slash form in shell).
- `yoyo risk history` → `Run /risk snapshot periodically`, exit 0 (#1155).
- `yoyo risk validate` → `Run /risk snapshot first.`, exit 0 (#1155).
- `yoyo skill list` → `no skills loaded / use --skills <dir>`, exit 0 (fine).
- `yoyo help` → fine.
So #1154 and #1155 reproduce exactly as filed; they are the natural small pair (both one level below #1127/#1153, same seam `commands_dev::command_form` + DoctorSurface).

## Evolution History (last 5 runs)
evolve.yml: 4 success (Nov 7 15:44, 19:50, 22:54; Nov 8 01:01) + current run in progress. Other workflows (Social, Sponsors Refresh, Skill Evolution, Pages, CI) all success. No API errors in audit; no reverts. Only CI failure in 14 days is the 11-day-old `hook_feedback_request` SIGPIPE flake (fixed Day 242).

## Capability Gaps
- **Unattended-run lockdown (new, from Claude Code 2.1.319's `permissions.denyInsteadOfPrompting`).** Claude Code can deny any call that would have prompted and continue the turn; its `-p` also denies by default. yoyo `-p`/piped auto-approves everything (`src/main.rs:1398` `auto_approve = config.auto_approve || !is_interactive`), and @yuanhao's #1149 answer (A) keeps that **default**. There is no **opt-in** to make an unattended run deny anything not on the allow list (a grep for deny_unmatched or similar found nothing). A product-safe opt-in (config key + flag) would leave his decision alone. It is a new product feature and needs a ticket first; it isn't a fix.
- MCP: stdio only, no HTTP transport; no restart of a dead server; no MCP "tasks" (long calls run in the background; Claude Code 2.1.319 added these); the "missing tool reads as nonexistent" premise is unmeasured (#1134).
- Durable todos (#1018), /cd reload of project config (#1040), TUI (#215), benchmarks (#156) all parked.
- MCP: no restart of a dead server; HTTP MCP absent (stdio only); "missing tool reads as nonexistent" premise unmeasured (#1134).
- Model-id authority: 29 of 78 non-Claude known ids have no outside source (#1147); Gemini list stale (#1118); Bedrock inference-profile ids (#1121).

## Bugs / Friction Found
1. #1154 `yoyo config get` exits 0 with REPL-syntax usage (reproduced).
2. #1155 five second-level shell lines name slash commands (3 reproduced: lint unsafe, risk history, risk validate).
3. Size-gate traps on context.rs (+15 fatal, unlisted) and help.rs (+10 fatal).
4. **hard_deny's refusal text names the rule, not the input.** `src/hard_deny.rs:118` prints `contains '{pattern}'`, where pattern is the rule label `RULE_RM_ROOT = "rm -rf /"`. So an input using the home-directory (tilde) form is told it "contains 'rm -rf /'", which is false. I hit this **three times** this assessment: two heredoc bodies and one `gh issue comment --body` that only *quoted* test fixtures were refused (the #1102 mention class). The first two hits are recorded on #1102 (comment 6720041992). Candidate small fix: make the wording say the input matched the rm-root rule.
5. `CLAUDE_CODE_GAP.md` header verified day-74, 179 days stale — any gap claim from it must be re-read.

## Open Issues Summary
- **#1149 — CLOSED this assessment.** @yuanhao answered 09:42Z: "A — keep non-interactive auto-approve, keep it disclosed. No change to the default." The disclosure shipped Day 252; I commented and closed it (completed). No code task follows.
- Live, small, reproduced: #1154, #1155.
- #1122 (agent-input, @yuanhao's upgrade ticket): latest comment (Day 252 00:18) says yoagent 0.27.4 now infers `accepts_sampling_params` for openai/openrouter; **still not covered: custom, openai_compat, local, ollama** (narrow upstream ask open).
- #1147 model ids w/o authority (lesson d251: iterate the authority over the whole population, not the 5 suspicious ones); #1118 Gemini list; #1121 Bedrock residue; #1148 upstream.
- #1094 write_file partial-read guard residue (incl. "never-read overwrite", which the d252 lesson says must be decided, not left 'undecided').
- #1102 hard_deny heredoc misfire; #1100 MCP servers outlive a panicking yoyo; #1064 opt-in credential scrub for children; #1012 price drift; #991 retry_safe_events for non-TTY -p.
- Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.

## Research Findings
Recall: yopedia already has delta scans through Claude Code 2.1.317 / Codex 0.166 (note `claude-code-and-codex-cli-changelog-delta-2026-11`). New this session: Claude Code **2.1.318 (Nov 4) and 2.1.319 (Nov 6)**, mapped against yoyo and ingested to yopedia as "Claude Code 2.1.319 delta vs yoyo (Day 253)":
- `permissions.denyInsteadOfPrompting`: a yoyo gap (Capability Gaps, first bullet).
- "Bash permission bypass via a crafted environment prefix": **yoyo already covers this.** `user_deny_refusal` strips leading assignments and `export` chains (pinned in `src/tools_user_deny_tests.rs` ~225), and `safety.rs` has the `a_leading_assignment_never_changes_the_*_verdict` tests. I checked this by reading the tests, not with a new run.
- Deny wins over a narrower ask/allow rule: yoyo `PermissionConfig::check` (src/config.rs:19) checks deny first, so this is covered.
- `claude remote-control` used to exit 0 silently on an unanswered question and now exits 1; `/rename` used to report plain success when the save failed. Both are the exit-code / false-success class yoyo is draining (#1153 done, #1154 open).
- MCP tasks (long MCP calls run in the background, result delivered later): yoyo has nothing like this. Related to #1134, but a new capability.
- `-p` now warns on stderr before stored credentials expire, and `-p` JSON carries `credential_expires_at`. yoyo has no analogue; it uses API keys, so this matters little.

## Suggested priorities for the planner (my read, not a plan)
1. #1154 + #1155: reproduced, small, and on the same seam as #1127/#1153. Check the help.rs/context.rs size traps before touching either file. Take the census from the `*_SUBCOMMANDS` consts; the unit is verb × subcommand (d250/d253).
2. The hard_deny refusal label: one string, and it states something false to users. Or fold it into #1102.
3. A new ticket for the opt-in unattended-run lockdown, if the planner wants a product-feature slot.

Non-diff action already done this phase: #1149 closed after @yuanhao chose A (comment 6720020797).
