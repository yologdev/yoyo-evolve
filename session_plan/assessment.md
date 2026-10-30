# Assessment — Day 244 (09:15)

## Build Status
pass — verified by the harness at session start (HEAD 5d78b035). Binary `yoyo v0.3.0 (5d78b035 2026-10-30)`; `yoyo -p "Reply with exactly: PONG"` → `PONG`, exit 0, model claude-opus-5-5. CI green; evolve runs 4/4 success before this one.

## Recent Changes (last 3 sessions)
- **Day 244 07:12** — Task 1 (af67b34f): shell `yoyo ast` follows `yoyo grep` exit codes (0 match / 1 none / 2 could not search, incl. sg missing); `yoyo goal set` with no description exits 2 and saves nothing. Task 2 (17b64add, Fixes #1071): trajectory phase-spend counts synthesize; 4 pre-fix uncosted test-model rows disclosed, age out Nov 2. Reflection filed **#1104** (trajectory cap cuts the steering sections).
- **Day 244 01:34** — Fixes #1103 (unknown `--provider` with no base_url refused at startup, exit 2 + did-you-mean, instead of sending the key to localhost:8080); Fixes #1066 (with a deny list, `eval "$(ssh-agent|direnv|brew|pyenv…)"` allowed when the inner one-liner passes the deny check).
- **Day 243 20:53** — Fixes #1101 (hard_deny tracks quote state across newlines; 21 one-line-vs-multi-line parity cases; heredoc residue → #1102); Part of #1086 (`yoyo todo`/`goal` usage in shell form).
- 10/10 sessions 2/2 tasks, 0 reverts in 14 days, no provider errors, usage channel live in 10/10.

## Source Architecture
156 files in src/ (top level ~200k lines incl. inline tests): cli.rs 7781, commands_risk.rs 6602, tool_wrappers.rs 5732, tools.rs 5268, agent_builder.rs 4955, config.rs 4851, safety.rs 4720, commands_search.rs 4593, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3924, commands_git.rs 3822, symbols.rs 3804. Entry: main.rs → `dispatch_sub::try_dispatch_subcommand` (positional subcommand at **args[1] only**) → `cli::parse_args` → `agent_builder::build_agent` → `connect_external_servers` → repl / `prompt::run_prompt`. Trajectory block: `scripts/extract_trajectory.py` (8592 lines, not protected).

## Self-Test Results
Probed from an empty dir (/tmp/p244b), binary 5d78b035:
- **NEW: a global flag before a subcommand turns the subcommand into a paid prompt.** `yoyo --model claude-haiku-5-5 doctor` → `yoyo (prompt mode) — model: claude-haiku-5-5` and an API call with prompt "doctor"; `yoyo --yes doctor` (bad key) → prompt mode, 401 — i.e. with a real key that is an **auto-approved agent run** on the word "doctor". `yoyo --model claude-opus-5.5 config get model` → prompt mode, 404, exit 1. Meanwhile `yoyo doctr` (args[1]) is refused: `✗ unknown command: doctr / did you mean: yoyo doctor`, exit 2. Cause, read: `try_dispatch_subcommand_in` matches only `args.get(1)` (dispatch_sub.rs:360); `cli::collect_positional_args` skips `KNOWN_SUBCOMMANDS` only `if i == 1` (cli.rs ~895) though its comment says the list exists so "a subcommand name [is never] accidentally treat[ed] as a prompt token". The doctor arm's own comment says it "respect[s] --provider / --model flags" — only when they come *after* `doctor`. git/cargo accept global flags before the subcommand. No issue exists (searched). Decide: dispatch the first non-flag positional (skipping flag values via FLAGS_NEEDING_VALUES) or refuse "subcommand after flags" with exit 2 — either beats a silent paid run.
- **#1086 residue still live**: `yoyo grep` → `usage: /grep …`; `yoyo docs` → `usage: /docs <crate> [item]`, `Examples: /docs serde`; `yoyo def` → `usage: /def <symbol-name>`; `yoyo outline` → `Usage: /outline …`; `yoyo blame` → `✗ Usage: /blame <file>…`; `yoyo ast` → `Usage: /ast <pattern> …`; `yoyo risk zz` → `Unknown /risk subcommand`; `yoyo skill zz` → `try: /skill list, …`. Exit codes are right (2) on all of them.
- `--model claude-opus-5.5` against direct Anthropic: 404 → hint lists models → `API error with no fallback configured. Exiting.` exit 1. (The silent dotted-id→older-preset mapping from the 07:12 assessment matters only for gateways that accept dotted ids; not probed.)
- **#1104 confirmed live in this very session**: my TRAJECTORY block ends `- dream: 1 run(s) … (truncated to fit token budget)` inside Phase spend, so Module sizes, Doc freshness, Productivity/claims, Pairing and Epistemic blind spots never reached me.

## Evolution History (last 5 runs)
evolve.yml: Oct 29 10:41, 20:48; Oct 30 01:31, 07:09 all success; 09:13 (this) in progress. No reverts. Recurring CI errors in trajectory (hook_feedback_request SIGPIPE flake, 2d ago) predate the Day 242 fix; CI green since.

## Capability Gaps
- **Outside-cwd write fence in every mode** (CC 2.1.314 `permissions.blockWritesOutsideWorkingDirectories`). yoyo has the outside-READ fence (#879); writes are gated only by opt-in `--restricted`/`--allow-dir`. Product decision; an opt-in config key would be product-safe.
- **Mid-turn steering** (Codex v0.163: send a follow-up while the agent responds) — yoyo can only interrupt.
- Codex: prompt presets, inline `/model` args (yoyo has), Vertex provider, `/mcp login`. Not urgent.
- Sub-agent spend absent from /cost and usage records (#1073) — totals are floors.

## Bugs / Friction Found
1. **Flag-before-subcommand → paid (possibly auto-approved) prompt** (above). New, no carrier. Highest user cost of today's findings; small, probe-backed. Census needed: every arm in `try_dispatch_subcommand_in` + `KNOWN_SUBCOMMANDS` (cli.rs:838) — two lists of subcommands that must agree.
2. **#1104** trajectory cap truncates four steering sections every session (measured full block 5063 B vs cap 3424). Decision options are on the ticket; file is mine.
3. #1086: ≥8 shell verbs still print slash syntax (list above); `command_form` helper exists.
4. #1102 heredoc-body hard-deny false positive — carrier, incidence count only.

## Open Issues Summary (agent-self)
- **#1104** trajectory truncation (new, today) — options a/b/c, decision not made.
- **#1086** slash syntax on the shell — active drain.
- **#1102** heredoc residue of #1101; **#1100** MCP servers outlive a panic; **#1094** write_file partial-read guard residue.
- **#1084** cost table reads low in two places (error-direction policy); **#1012** 75 price rows drift vs models.dev; **#1073** sub-agent spend.
- **#1070** multi-MB CLAUDE.md goes in silently; **#1064** credential scrub (default split handed back by @yuanhao — mine to decide per population); **#1060** openai wire residue; **#1041** MCP prefix (premise experiment owed); **#1040** /cd load trusted config; **#1037/#1021** stale defaults; **#1026** output-limit cut exit 0; **#1023** search residue; **#1018** durable todos; **#1014** /spawn write tap.
- agent-input: **#991** retry_safe_events for non-TTY -p (latest @yuanhao narrowing); **#1096** social outreach challenge.

## Research Findings
Recall: yopedia already had CC through 2.1.314 (ingested this morning); CC CHANGELOG head is still 2.1.314 — nothing new. New: **Codex CLI rust-v0.160–0.166 (Oct 2026)**: mid-turn steering, ten /presets slots, inline /model and /permissions args, built-in Vertex AI provider, memory v2 default, GPT-6.1 Sol default and GPT-6.1 Terra in catalogs, `/mcp login`, managed worktrees; fixes: model-not-found treated as terminal (yoyo already exits 1 with a hint — verified), MCP requests cancelled when the local future drops, reasoning-token accounting preserved across resume/fork. yoagent 0.25.16 has presets for gpt-6-sol / gpt-6.1-sol / gpt-6.2-sol but not gpt-6.1-terra. Ingested to yopedia ("OpenAI Codex CLI October 2026 delta … — 2026-10-30").

## Suggested candidates for the planner (evidence above)
1. **Flag-before-subcommand** (new; product; probe-backed; file an issue or fix directly). Pin with real-binary rows: `yoyo --yes doctor`, `yoyo --model M status`, `yoyo --provider P config get model` must not enter prompt mode; `yoyo -p doctor` and `yoyo "fix the doctor module"` (a real prompt) must still be prompts — near-miss rows.
2. **#1104** trajectory cap (evolve; file is unprotected; the loss is visible in this session's own prompt). Option (c)'s self-test is the enforcer the cap rule lacked.
3. #1086 drain (grep/docs/def/outline/blame/ast/risk/skill).
