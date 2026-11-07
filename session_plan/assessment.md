# Assessment — Day 252 (23:00)

## Build Status
Pass — verified by the harness at session start (HEAD 0b39ee30). Binary `target/debug/yoyo` built 23:03, `yoyo --version` → `v0.4.0 (0b39ee30 2026-11-07)`. CI on the last 5 pushes: all success.

## Recent Changes (last 3 sessions)
- **19:55** — Fixes #1151 (`-p` stray-word refusal names the flag that swallowed a flag as its value, e.g. `--system` ate `--model`; per-word before/after table in the commit). Part of #1127: `yoyo docs`, `yoyo risk <bad>`, `yoyo memories` print shell forms via `DoctorSurface` + `commands_dev::command_form`; `/remember` has no shell verb so the hint sends you to the REPL.
- **15:51** — Fixes #1114 (collect_repeatable_flag and resolve_numeric_setting walk argv skipping value slots; numeric-flag refusal no longer refuses a value belonging to another flag). Part of #1122: corrected my own overclaim that OpenRouter 400s on temperature (OpenRouter docs say it ignores unsupported params).
- **13:14** — Part of #1122: `temperature_support.rs` now reads yoagent 0.27.3's `accepts_sampling_params` instead of yoyo's own Claude-version parser. Fixes #1150 (`yoyo -p hi extra` refuses the stray word, exit 2).
- Earlier Day 252: wired yoagent ReadLog for every agent/sub-agent (Part of #1094); permissions docs now say `-p`/piped mode auto-approves (Part of #1149, decision waiting on @yuanhao); `flag_value` skips value slots (Part of #1114).
- llm-wiki (external journal): paused mid-migration; last entries are April 2026.

## Source Architecture
169 files in src/, ~208k lines total (incl. tests). Largest: cli.rs 7861, commands_risk.rs 6624, tool_wrappers.rs 5762, tools.rs 5295, agent_builder.rs 4996, repl.rs 3391, commands_project.rs 3790, help.rs 2946. Entry points: `main.rs` → `cli::parse_args` → `dispatch_sub::try_dispatch_subcommand` (shell verbs) or `repl::run_repl` / `-p` prompt path; `agent_builder::build_agent` builds the yoagent Agent; `dispatch.rs` routes REPL slash commands.

**Module-size gate is close to FATAL on two files (from the trajectory, confirmed with wc):**
- `src/commands_risk_unhittable.rs` 2036 lines, UNLISTED, 36 past cap — 14 more lines = FATAL (whole-task revert).
- `src/help.rs` 2946 vs recorded 2856 (+90 drift) — 10 more lines = FATAL. Any task adding help text must paste a new entry.
- #1127's three remaining verbs live in files near the 2000 cap: `commands_lint.rs` 1939, `commands_map.rs` 1920, `commands_skill.rs` 1951.

## Self-Test Results
- `yoyo --version`, `yoyo help`, `yoyo grep` (usage), `yoyo map` (2796 symbols / 172 files, regex), `yoyo skill` (no skills loaded) all work.
- `yoyo lint --bogus` → `Unknown /lint subcommand: --bogus`, exit 2 — slash syntax on the shell (#1127 residue).
- `yoyo skill bogus` → `try: /skill list, /skill show <name>, …` (#1127 residue).
- `yoyo map --bogus` → `usage: /map [--all] [--regex] [path]` (#1127 residue). These match my own 20:xx comment on #1127 (3 of 11 still slash).
- Did not run a live model prompt (no need; harness verified).

## Evolution History (last 5 runs)
evolve.yml: 4 success (10:51, 13:08, 15:44, 19:50 on 2026-11-07), 1 in progress (22:54, this one). Trajectory: last 10 sessions all 2/2 (or 1/1) tasks, 0 reverts, no provider errors. Only CI failure pattern in 14 days is the hook_feedback_request flake from 10 days ago (fixed Day 242, SIGPIPE).

**Silent recurring failure (#1152, self-filed):** every session since 19:50 Step 1a tries `yoagent 0.27.3 → 0.27.4`, `temperature_support::tests::built_config_sampling_flag_table` panics on `openai/claude-opus-5-5 should keep temperature`, and Cargo.lock is restored. So yoyo is stuck on 0.27.3 and every session silently pays a failed upgrade attempt.

## Capability Gaps
- **Non-interactive permissions (biggest, and owner-gated):** Claude Code `-p` (v2.1.302+) starts in Manual mode — any call needing approval is *denied* (deny-and-continue) unless an allow rule / hook / `--permission-prompt-tool` approves it; `--permission-mode dontAsk` and `--permission-prompts none` (tells the model nobody can approve, don't retry) exist for CI; stream-json reports `permission_denials`. yoyo `-p`/piped approves everything. This is #1149, waiting on @yuanhao — do NOT plan a default change; the reference is already cited in the issue body.
- Deny-and-continue as a config knob (CC 2.1.319 `permissions.denyInsteadOfPrompting`, covers subagents) — yoyo has no equivalent for the REPL either.
- MCP tasks run in background with result delivered on finish (CC 2.1.319); yoyo's MCP is stdio-only, no restart of a dead server (#1134).
- `blockWritesOutsideWorkingDirectories` (CC 2.1.314) — yoyo has dir_restrictions; not re-compared this session.
- CLAUDE_CODE_GAP.md is 178 days stale, so it is not a reliable gap list.

## Bugs / Friction Found
1. **#1152 is NOT test-only once applied — the user-facing note goes wrong.** yoagent 0.27.4 (CHANGELOG, read in ~/.cargo/registry/.../yoagent-0.27.4) turns `accepts_sampling_params` off for GPT-6 ids (`gpt-6…`, after any `vendor/` prefix) under `openai`, `openai_responses`, OpenCode and OpenRouter. yoyo's `temperature_dropped_note` (src/temperature_support.rs) is hardcoded: "Anthropic removed temperature for Claude 4.7 and later, and Anthropic's API rejects every request that sets it". After the upgrade, `--provider openai --model gpt-6… --temperature 0.5` will print a note naming Anthropic/Claude for an OpenAI model. The note needs a per-vendor reason (OpenAI's: reasoning effort not `none` → temperature removed). Also the issue body says a Claude id under `openai` now turns the flag off — the changelog only names GPT-6; the failing row proves it empirically for `openai/claude-opus-5-5`, so the table move needs each row re-run (the test stops at the first failing row; rows after it are unchecked). Done-when per #1152: Cargo.lock at 0.27.4, suite green, next Step 1a has nothing to update. Also check #1060 ("--temperature with effort" on openai wire) — 0.27.4 may resolve part of it.
2. **#1127 residue: 3 verbs** (`lint`, `skill`, `map`) still print slash syntax on the shell. The seam (`DoctorSurface` + `command_form`) exists; files are 49–80 lines under the cap.
3. **Module-size near-misses** (above) — a planner task touching help.rs or commands_risk_unhittable.rs must budget it.
4. CLAUDE_CODE_GAP.md header 178 days stale (doc-freshness warning).

## Open Issues Summary
- **#1152** (agent-self, filed today): yoagent 0.27.4 rolled back every session — highest leverage, small, plus the note-wording finding above.
- **#1127**: 3 of 11 verbs left (lint, skill, map).
- **#1094**: item 1 (never-read overwrite) — waiting on @yuanhao's A/B pick (posted 20:12Z, default A = keep allowing). Hold; don't ship.
- **#1149**: `-p` auto-approve decision — waiting on @yuanhao, do not change the default.
- **#1147**: ~24 offered non-Claude ids with no outside authority still unchecked; per d251, next slice must iterate the authority over the whole population, not a suspicion-ranked shortlist.
- **#1122**: OpenRouter/custom gateway temperature; asked upstream for an id predicate for gateways.
- **#1121** (Bedrock inference-profile ids → 'Unknown model'), **#1118** (stale Gemini ids), **#1148** (upstream: Claude max output as data, opus-4-7 Bedrock map), **#1134/#1100** (MCP lifecycle), **#1102** (hard_deny heredoc), **#1064** (credential scrub opt-in), **#1012** (price drift vs models.dev).
- Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.

## Research Findings
- Recall (yopedia, agent-scoped search): prior pages cover changelog-delta scans through CC 2.1.313–2.1.314 and the Nov 3 entry, a GPT-6 API page ("when effort is not `none`, remove temperature/top_p/top_logprobs") — that page independently confirms yoagent 0.27.4's GPT-6 rule, so the #1152 note-wording fix has a vendor source on file.
- New this session (ingested to yopedia, job 4a67ab2a): CC 2.1.319 (2026-11-06) — `denyInsteadOfPrompting`; MCP tasks backgrounded; PostToolUse hooks run (10s budget) on tool calls finishing after an interrupt; after safeguards stop a subagent the parent is told to retry narrower or tell the user rather than do the work itself; exact ask rule refused when a broader deny covers it. Headless docs: Manual-mode default for `-p`, `dontAsk`, `--permission-prompts none`.
- Defect-class transfer candidates for yoyo (unverified against yoyo code — planner should probe before selecting): (a) does a yoyo PostToolUse/shell hook run when a tool call is interrupted (Ctrl-C)? (b) when a sub-agent is refused/stopped, what does the parent model get told?

## Suggested priorities for the planner (my read, not a plan)
1. **#1152 upgrade to yoagent 0.27.4** — move the `openai/claude-opus-5-5` row (and re-run every row after it, since the test stops at the first panic), lock 0.27.4, AND make `temperature_dropped_note` give the reason for the vendor that actually rejected it (GPT-6/OpenAI vs Claude/Anthropic), with a test for a `gpt-6…` id. Otherwise the upgrade ships a note blaming Anthropic for an OpenAI model. Check `grep -rn gpt-6 src docs` for other text that 0.27.4 makes stale (#1060's "--temperature with effort" row may now be partly resolved by yoagent).
2. **#1127 last 3 verbs** (lint, skill, map) via the existing `DoctorSurface`/`command_form` seam — mind the files' headroom (49–80 lines) and help.rs (10 lines to FATAL).
3. **#1147** next slice: iterate an outside authority over all ~24 remaining ids (d251 rule), not a shortlist.
- Holds: #1094 item 1 and #1149 are waiting on @yuanhao — do not ship either default.
- Size gate: any task touching `commands_risk_unhittable.rs` (14 lines to FATAL, unlisted) or `help.rs` (10 lines) must register/split in the same task.
