# Assessment — Day 244

## Build Status
Pass — verified by the harness at session start (HEAD e525f828). Binary `target/debug/yoyo` present and working: `yoyo -p "Reply with exactly: PONG"` → `PONG`, exit 0; a `read_file` tool round-trip in `-p` mode worked (`▶ read a.txt ✓`). Model `claude-opus-5-5` (anthropic). No full suite re-run (per instructions).

## Recent Changes (last 3 sessions)
- **Day 243 20:53** — Fixes #1101: `hard_deny` tracks quote state across lines, so a quoted newline no longer splits a mention of a deny pattern into a "command" (21 table rows: multi-line ≡ one-line). Residue carried on **#1102** (heredoc bodies still tokenized as commands; incidence rows go there). Part of #1086: `yoyo goal` / `yoyo todo` usage text names the shell form via `commands_dev::command_form` + a `DoctorSurface` argument.
- **Day 243 10:45** — Part of #1086: `command_form` helper introduced; `run`, `find`, `tree`, `undo` now speak the door they were called from. Fixes #1068: hard_deny doc stops overclaiming quote handling.
- **Day 243 01:33** — Part of #991 item 3: stream-json `collected_text` keeps only the surviving retry attempt (`drop_dying_attempt` shared). Then a correction comment to @yuanhao on #991 offering A (drop it, his original instruction — my default) / B (keep) — **held; no reply yet** (my comment 2026-10-29T02:44Z is last). Fixes #1054: four multi-block tool-result sites marked with yoagent version-claim markers.
- Day 242: #1099 (close live stdio MCP servers on every exit door; panic residue → #1100), SIGPIPE fix for hook-feedback flake, doctor's Curl row (#1083).
- Trajectory: 10/10 recent sessions 2/2 tasks, 0 reverts. CI green now; the only recent CI red was the hook-feedback SIGPIPE flake fixed Day 242.

## Source Architecture
175 Rust files under `src/`, ~220.8k lines total (incl. inline tests). Largest: `cli.rs` 7780, `commands_risk.rs` 6602, `tool_wrappers.rs` 5732, `tools.rs` 5268, `agent_builder.rs` 4954, `config.rs` 4851, `safety.rs` 4720, `commands_search.rs` 4593, `commands_spawn.rs` 4566, `watch.rs` 4444, `prompt.rs` 3924, `commands_git.rs` 3822. Entry: `main.rs` → `cli::parse_args` → `agent_builder::build_agent` → REPL (`repl.rs`) or `-p` (`prompt.rs`); shell subcommands go through `dispatch_sub::try_dispatch_subcommand`. Module size gate in `tests/module_size.rs` (prompt.rs already over its cap per #991 comment).

## Self-Test Results
Probed binary e525f828 from an empty dir `/tmp/probe244` (no git, no config). Re-ran #1086's probe (`yoyo <verb>` bare and with `zz1 zz2 zz3`, grep for `/<verb>`):
- **#1086 residue confirmed at HEAD:** grep bare (10 lines, `usage: /grep ...`), map 3-arg (2, `/map takes at most one path`), index 3-arg (2), blame bare (1), lint 3-arg (`Unknown /lint subcommand`), risk 3-arg (`Unknown /risk subcommand`), skill 3-arg (`try: /skill list, ...`), `yoyo help doctor` line 14 `Curl — needed for /docs and /web`. Matches the Day-243 comment's list exactly.
- **NEW, not on #1086's list:** `yoyo docs` bare prints `usage: /docs <crate> [item]` + `Examples: /docs serde, /docs tokio task` (exit 2); `yoyo ast` bare prints `Usage: /ast <pattern> [--lang <lang>] [--in <path>]` (exit 2). The #1086 census undercounted by two verbs — worth adding to the ticket.
- `yoyo todo` (bare) still says `use: /todo` — this is one of the two deliberate ones (todo list only exists in-session). Correct per Day 243.
- **Exit-0-on-failure (the #982 class, #982 is CLOSED):**
  - `yoyo ast zz1 zz2 zz3` with ast-grep not installed prints `ast-grep (sg) is not installed` and **exits 0**. `dispatch_sub.rs:308-314` `ast_shell_status` doc says `0 otherwise, including "sg not installed", which is not a usage error` — that's a reason not to be 2, not a reason to be 0 (d229 tell). Also `run_ast_grep_search` returning `Err(e)` (a real search failure) prints in YELLOW and exits 0 (`commands_ast_grep.rs` `handle_ast_grep` → `Err(e) => println!`).
  - `yoyo goal set` with no description prints `Usage: yoyo goal set <description>` and **exits 0** (noted, not touched, on #1086 Day 243).
- `-p` mode, tool calls, model id reporting all fine.

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml --limit 5`: 4 × success (2026-10-28 20:44, 10-29 01:29, 10-29 10:41, 10-29 20:48) + the current run (in progress). No failed runs to diagnose. Trajectory: 0 task reverts, 0 whole-session reverts in 14 days; no provider-error lines.

## Capability Gaps
- **Write fence outside cwd in every mode:** Claude Code 2.1.314 added `permissions.blockWritesOutsideWorkingDirectories` (Write/Edit refuse outside working dirs in *every* permission mode). yoyo's outside fence (`src/outside_read.rs`) covers only read_file/search/list_files; its doc (lines 85-89) says writes "are a different fence" — i.e. `confirm_file_operation`, which `--yes` bypasses; only `--restricted`/`--allow-dir` hard-fence writes.
- **Mid-turn steering** (Codex rust-v0.163: send a follow-up while the model is responding) and **/presets** (saved prompts in slots) — yoyo has neither.
- **Startup-failure exit codes:** CC fixed `claude -p` exiting 0 with no output on startup failure. yoyo -p with a bad key / bad model exits 1 correctly (probed) — parity there.
- Standing: no TUI (#215), no benchmark submission (#156), sub-agent spend absent from `/cost` (#1073), `/cd` into a trusted project loads nothing without restart (#1040), openai default still `gpt-6-sol` while Codex defaults to `gpt-6.1-sol` (the remaining slice of #1037; `providers.rs:243-245` says so; #1037's title "openai defaults to gpt-5" is stale).
- Standing gaps known from prior assessments: no TUI (#215), no official benchmark submission (#156), sub-agent spend not in `/cost` (#1073), MCP reconnect after failure, `/cd` into trusted project doesn't load hooks/MCP/skills without restart (#1040).

## Bugs / Friction Found
1. **#1086 residue (shell doors print slash syntax):** grep, map, index, blame, lint, risk, skill, help doctor — plus newly found **docs** and **ast** bare usage. Mechanical now that `command_form` + `DoctorSurface` is the established shape. grep alone is 10 lines (one usage block) — good single-verb slice.
2. **`yoyo ast` exits 0 when sg is missing or the search errors.** Population that reaches it: anyone running `yoyo ast` without ast-grep installed (default for most users) — none chose it. Fix shape: 1 for "not installed"/search error, keep 2 for usage, 0 for a real (even empty) result. Needs `handle_ast_grep` to return a status instead of `()` (or a `_status` sibling like `handle_def_status`).
3. **`yoyo goal set` (no description) exits 0** while printing usage. Same class; `goal_subcommand_is_unknown` covers unknown verbs only.
4. **Unknown `--provider` (a typo) runs for ~80s against localhost:8080 and then blames the network.** `yoyo --provider anthropc -p hi` (probed): one `Unknown provider` warning (`cli.rs:1078`), then `treating unknown provider 'anthropc' as OpenAI-compatible (localhost:8080)` printed **5 times** (`agent_builder.rs:1104`, once per `create_model_config` call), 6 retry attempts on `Connection refused`, then `💡 Network error — Check your internet connection and that anthropc's API is reachable. You can retry with /retry.` and exit 1 after **80 s**. Problems: (a) no did-you-mean (`anthropc` → `anthropic` is one edit); (b) wrong diagnosis (it's a typo, not the internet); (c) `/retry` is REPL-only advice printed in `-p` mode (`prompt_retry.rs:699`, #1086's class in a hint); (d) the key sent to localhost:8080 comes from the fallback chain (`ANTHROPIC_API_KEY` → `API_KEY`) — a credential handed to whatever listens locally; (e) the warning hardcodes "(localhost:8080)" even when `--base-url` was given. Population: anyone who mistypes a provider, none chose this. Candidate fix: unknown provider **without** `--base-url` → refuse at startup, exit 2, with a nearest-known suggestion; with `--base-url` keep today's OpenAI-compatible path (that's a choice, `custom` is the documented spelling). Check `/provider` in the REPL (same `KNOWN_PROVIDERS` check per the comment at `agent_builder.rs:1101`) for the second door.
5. Minor: `--model claude-opus-4.8` warns unknown but doesn't suggest the listed `claude-opus-4-8` (CC 2.1.314 fixed exactly this dot-spelling behind gateways).
6. #991 item 3: held on @yuanhao's A/B (my default A = drop `collected_text`). Do not act until he answers.

## Open Issues Summary (agent-self)
- **#1086** shell subcommands print slash syntax — 7 verbs + help topic left (+ docs, ast found today).
- **#1102** hard_deny heredoc body residue — carrier, incidence counting (Day 217, Day 243 ×2), default A = keep.
- **#1100** MCP servers outlive a panicking yoyo — needs pid record at connect + sync group-kill in panic hook.
- **#1094** write_file partial-read guard residue (item 1 never-read overwrite undecided; item 2 done Day 242).
- **#1084** cost table reads low in two places vs. error-direction policy.
- **#1073** `/cost` omits sub-agent spend (yoagent reports `SessionStats.sub_agents` since 0.19).
- **#1071** synthesize.yml spend row (protected file — help wanted).
- **#1070** multi-MB CLAUDE.md goes into the system prompt whole (warning past 150KB shipped Day 238).
- **#1066** eval "$(ssh-agent)" carve-out decision.
- **#1064** child processes inherit API key (opt-in scrub shipped; default split handed back to me by @yuanhao Day 235).
- **#1060** openai wire residue after GPT-6 → Responses.
- **#1041** MCP collision prefixing (shipped Day 231/240; model-visibility premise open).
- **#1040** /cd into trusted project loads nothing without restart.
- **#1037 / #1021** stale default models (openai gpt-5; google gemini).
- **#1026** output-limit cut: json/stream-json no note, exit 0.
- **#1023** search residue (rg config, stale footer).
- **#1018** durable todos with due dates. **#1014** /spawn write tap. **#1012** price audit drift (75 rows).
- agent-input: **#1096** (challenge: X outreach — social-session scope, not an evolve task), **#991** (held, see above).

## Research Findings
- **Recall:** yopedia already has CC delta notes through 2.1.312 (`agent-changelog-delta-analysis`, `agent-safety-guard-parity-and-stop-attribution`, `agent-tool-i-o-hazards`). This scan covered 2.1.313–2.1.314 and Codex Oct 2026; ingested as "Agent Changelog Delta — Claude Code 2.1.313-2.1.314 and Codex rust-v0.166 (2026-10-30)".
- **Claude Code 2.1.314** (relevant items): write fence outside working dirs in every mode; `-p` startup failure now exit 1 (was 0 + no output); PreCompact hook output capped at 10,000 chars (huge hook output broke compaction — yoyo hook feedback cap worth checking); dot-spelled model ids behind gateways normalized; refused shell/MCP calls no longer displayed as if they ran; subagent tool results freed before turn end.
- **Codex CLI rust-v0.160–0.166 (Oct 2026):** mid-turn steering, `/presets`, inline `/model`/`/permissions` args, searchable `/config` that flags which changes need a restart, Vertex AI provider, GPT-6.1 Sol default, `/mcp login`, managed git worktrees, memory v2 default.
- Biggest practical gaps for yoyo users remain interactive UX (steering, TUI) rather than correctness; the correctness items above (provider typo, ast exit 0, #1086 residue) are cheap, product-kind, and each probed at HEAD e525f828.
