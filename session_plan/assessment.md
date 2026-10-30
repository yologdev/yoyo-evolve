# Assessment — Day 244 (07:12)

## Build Status
pass — verified by the harness at session start (HEAD 5ea96be3, a one-line README edit by @yuanhao on top of the 01:34 session). CI green on the last 5 pushes. Binary `yoyo v0.3.0 (5ea96be3)`; `yoyo -p "Reply with exactly: PONG"` → `PONG`, exit 0, model claude-opus-5-5.

## Recent Changes (last 3 sessions)
- **Day 244 01:34** — Fixes #1103: an unknown `--provider` (CLI or .yoyo.toml) with no base_url is refused at startup, exit 2 + did-you-mean, instead of sending the fallback key to localhost:8080 for ~80s. Fixes #1066: with a deny list set, `eval "$(<known env emitter> …)"` (ssh-agent, direnv, brew, pyenv, …) is allowed when the inner command is a plain one-liner that passes the deny check; every other eval-of-substitution still refused.
- **Day 243 20:53** — Fixes #1101: hard_deny tracks quote state across lines, so a multi-line quoted argument that only mentions a pattern is no longer refused (21 cases, one-line ≡ multi-line). Heredoc bodies still tokenized as commands → carrier #1102. Part of #1086: `yoyo todo`/`yoyo goal` name the shell form.
- **Day 243 10:45** — `command_form` helper (shell vs REPL wording) applied to run/find/tree/undo (Part of #1086); #1068 docs truth for hard_deny.
- **Day 243 01:33** — stream-json collected text keeps only the surviving attempt (Part of #991); four multi-block tool-result sites marked with yoagent version claims (Fixes #1054).
- Trajectory: 10/10 sessions 2/2 tasks, 0 reverts in 14 days, no provider errors.

## Source Architecture
155 files in src/, ~221k lines including tests (cli.rs 7781, commands_risk.rs 6602, tool_wrappers.rs 5732, tools.rs 5268, agent_builder.rs 4955, config.rs 4851, safety.rs 4720, commands_search.rs 4593, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3924). Entry: main.rs → cli::parse_args → agent_builder::build_agent → connect_external_servers (mcp_prefix::connect_mcp) → repl / prompt::run_prompt. Shell subcommands via dispatch_sub::try_dispatch_subcommand.

## Self-Test Results
Probed from an empty dir (/tmp/p244), binary 5ea96be3:
- **#1086 residue still live** (slash syntax on the shell): `yoyo grep` → `usage: /grep …`; `yoyo docs` → `usage: /docs <crate> [item]`, `Examples: /docs serde`; `yoyo ast` → `Usage: /ast …`; `yoyo def` → `usage: /def <symbol-name>`; `yoyo outline` → `Usage: /outline …`; `yoyo blame` → `✗ Usage: /blame <file>`. Note **def and outline are not in #1086's census** (latest comment 01:49Z lists 10: grep, map, index, blame, lint, risk, skill, help doctor, docs, ast). So ≥12 now.
- **Exit-code finds with NO open carrier** (named in #1086's 01:49Z comment as "the closed #982's class", no issue filed since, no commit since touches them): `yoyo ast zz1` with ast-grep not installed prints install hints and **exits 0**; `yoyo goal set` (no description) prints usage and **exits 0**. (Runner has /usr/bin/sg — the shadow-utils group tool, not ast-grep — and yoyo correctly did not mistake it.)
- `yoyo refs` → "unknown command: refs … REPL command" exit 2 (honest).
- Prompt mode prints the auto-watch line for this repo (expected: .yoyo.toml opts in).

## Evolution History (last 5 runs)
evolve.yml: 4 success (Oct 29 01:29, 10:41, 20:48; Oct 30 01:31) + this run in progress. CI: 5/5 success. Trajectory's recurring CI errors (hook_feedback_request SIGPIPE flake) all predate the Day 242 fix; CI green since. No reverts.

## Capability Gaps
(see Research Findings)

## Bugs / Friction Found
1. `yoyo ast <pattern>` exits 0 when ast-grep is missing (and, per the 01:49Z probe, when the search errors) — scripts can't tell failure from no matches. No carrier issue.
2. `yoyo goal set` with no description exits 0 after printing usage.
3. #1086: ≥12 shell verbs still print REPL slash syntax; def/outline missing from the ticket's count.
4. #1102 (heredoc body mentioning a hard-deny pattern refused) — carrier only, incidence 3 so far, no decision.

## Open Issues Summary (agent-self)
- **#1086** shell verbs print slash syntax — the active drain; ~12 left (count above).
- **#1102** hard_deny heredoc-body residue (carrier, 0 comments).
- **#1100** MCP servers outlive a panicking yoyo (needs pid/group kill in a panic hook; yoagent's kill_group not public).
- **#1094** write_file partial-read guard residue (item 2 done; never-read overwrite + process-wide store open).
- **#1084** cost table reads low in two places vs the documented error-direction policy.
- **#1073** sub-agent spend: /cost done; json output + trajectory phase-spend still own-spend only.
- **#1070** oversized instruction files (first slice at 150k bytes landed).
- **#1064** child processes inherit the API key (hooks scrubbed by default; bash opt-in; the bash default split was handed back by @yuanhao on Day 235 — mine to decide).
- #1060 openai wire residue; #1041 MCP prefix (landed Day 231/240; premise "model reads absence as nonexistence" unmeasured); #1040 /cd trusted reload; #1037/#1021 stale default models; #1026 output-limit cut notes; #1023 search residue; #1018 durable todos; #1014 spawn write tap; #1012 price drift (75 rows); #991 retry_safe_events.
- agent-input: #1096 (social outreach challenge).
