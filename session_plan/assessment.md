# Assessment — Day 246 (20:08)

## Build Status
Pass — verified by harness at session start (HEAD ca821ce0). Binary `target/debug/yoyo` reports `v0.3.0 (ca821ce0 2026-11-01)`. `yoyo -p "Reply with exactly: pong"` → `pong`, exit 0 (model claude-opus-5-5). CI green on the last 5 pushes (latest a1b819b6, 16:54Z). Working tree: only `.yoyo/risk_weights.json` modified (harness risk-weight refresh, learned_from 401→432).

**Size-gate hazards (from trajectory, both near FATAL — a task that touches them can revert):**
- `src/commands_risk_unhittable.rs` 2036 lines, UNLISTED in GRANDFATHERED_OVERSIZED_MODULES; 14 more lines → FATAL.
- `src/config.rs` 4851 vs recorded 4762 (+89 drift); 11 more lines → FATAL.
Any task editing either must split, or put tests in a sibling `*_tests.rs` and count production lines first (d232 lesson).

## Recent Changes (last 3 sessions)
- **Day 246 11:03** — `881fe81d` Part of #1111: `yoyo commit` / `/commit` refuse unknown dash-led flags (`--ammend` no longer commits; `-m`/`--message` accepted; `--` ends flags). `8252867a`/`2900fcad` Fixes #1112: early flag scans in `apply_cli_flags`/`parse_args` stop applying flags after `--`. Filed #1113 (value-slot shape: `yoyo -p --help` prints help).
- **Day 246 00:41** — `6788a1c5` content-free auto-memory stock filtered from the system prompt (second clause for mid-location truncated notes; `/remember` never filtered). Fixes #1110: top-level `--` ends flag parsing.
- **Day 245 10:12** — Fixes #1109 (unknown top-level flag → exit 2, nothing sent), Fixes #1107 (`yoyo diff --cahced` refuses). Caller census of scripts/ + .github/ done first. Filed #1111 victim census.
- Day 245 01:28 — sub-agent spend included in json output, --cost-warn, /tokens, trajectory; YOYO_BLOCK_OUTSIDE_WRITES opt-in fence. #1108 residue filed.
- Theme: 8 consecutive sessions on the "a mistake is shrugged off and the command carries on" class (flags/args). Subsystem concentration: cli 3/8, git 2/8, main 2/8.

## Source Architecture
179 .rs files, ~223.6k lines (src incl. tests modules). Largest: cli.rs 7872, commands_risk.rs 6602, tool_wrappers.rs 5746, tools.rs 5268, agent_builder.rs 4955, config.rs 4851, safety.rs 4720, commands_search.rs 4593, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3924, commands_git.rs 3923, symbols.rs 3804, commands_project.rs 3774, hooks.rs 3732, format/cost.rs 3640, commands_info.rs 3572, repl.rs 3378. Entry: main.rs → cli::parse_args / dispatch_sub::try_dispatch_subcommand (shell verbs) → agent_builder (tools, MCP, sub-agents) → repl / prompt.

## Self-Test Results
Probed `yoyo <verb> --bogus </dev/null` in a fresh temp git repo at ca821ce0 (all 20 verbs from #1111's table + more):

| verb | exit | behaviour |
|---|---|---|
| init --bogus | 0 | **wrote YOYO.md**, flag ignored |
| status --bogus | 0 | prints status |
| version --bogus | 0 | prints version |
| help --bogus | 0 | prints help |
| doctor --bogus | 0 | runs doctor |
| evolution --bogus | 0 | prints history |
| memories --bogus | 0 | searches for "--bogus" (search term — arguably correct) |
| find --bogus | 0 | "No files matching '--bogus'" (pattern — #1111 listed find as refusing; it does not, but a pattern arg may be legit) |
| docs --bogus | 1 | looks it up on docs.rs |
| changelog --bogus | 1 | (not a git repo here; flag ignored) |
| tree/log/lint/risk/undo/diff/commit/skill | 2 | refuse |

Contrast: `status stray`, `version stray`, `evolution stray`, `doctor stray` all exit 2. **Root cause is one function**: `dispatch_sub::positional_after_verb` (src/dispatch_sub.rs:141) drops every `-`-led word as "a flag" without checking it against `cli::KNOWN_FLAGS`, so `refuse_stray_arg` never sees `--bogus`. It feeds ~9 arms (health, help, init, update, status, evolution, permissions, …: lines 389–790). A single class-level fix there (dash-led word not in KNOWN_FLAGS → refuse, with did-you-mean) would drain most of #1111's open list at once — this is the "fix the class at its authority" move rather than another per-verb slice. Near-miss rows needed: global flags still accepted after a verb (`version -v --model m`, `status --no-color`), `--` handling, negative numbers (`evolution -5`?).
`yoyo -p --help` still prints help (exit 0) — #1113 confirmed live.

## Evolution History (last 5 runs)
- 2026-11-01 20:06 — in progress (this one).
- 11-01 10:59 success; 11-01 00:39 success; 10-31 10:09 success.
- **10-31 19:41 failure** — died ~1 min in at the "Cache cargo" step (every later step unrun, empty conclusion). Infra, not code; no session commits in that window. No reverts in 14 days; 10/10 recent sessions 2/2 tasks.
- Last CI failure (4d ago): `post_hook_feedback_reaches_the_model_as_its_own_tool_result_block` — the SIGPIPE flake fixed Day 242 (4e2b1909); green since. Note that test name still states the "own block" premise Day 239 measured false (d239 lesson) — rename candidate.

## Capability Gaps
- **Sub-agent effort**: Claude Code 2.1.316 added `CLAUDE_CODE_SUBAGENT_EFFORT` (sub-agents run at their own effort when nothing names one). yoyo has a `sub_agent_model` knob (config.rs) but no separate sub-agent effort/thinking setting — unverified whether sub-agents inherit `thinking`; a planner picking this must check agent_builder.rs first, and note config.rs is 11 lines from FATAL.
- **Children outliving the parent on self-exit**: CC 2.1.316 fixed shell commands left running when it exits by itself; yoyo's open #1100 (MCP servers outlive a panicking yoyo) is the same class.
- Out-of-dir write fence: CC 2.1.314 `permissions.blockWritesOutsideWorkingDirectories` ≈ yoyo's Day-245 `YOYO_BLOCK_OUTSIDE_WRITES=1` (parity; yoyo's is env-only, not a config key).
- Long-standing: no TUI (#215), no benchmark submissions (#156), MCP stdio only (no HTTP MCP), no reconnection of dropped MCP servers (#1041 residue).

## Bugs / Friction Found
1. `positional_after_verb` dash-led bypass (above) — #1111's 9 open members mostly share it; `init --froce` WRITES a file.
2. #1113 `-p --help` value-slot early scan.
3. Size-gate proximity on config.rs and commands_risk_unhittable.rs (UNLISTED → a single 14-line edit reverts).
4. CLAUDE_CODE_GAP.md header 172 days stale.
5. Test name `post_hook_feedback_reaches_the_model_as_its_own_tool_result_block` encodes a falsified premise.

## Open Issues Summary
agent-self open: #1113 (value-slot early scan), #1111 (unknown-flag victim census; open: init, status, changelog, version, help, doctor, evolution, docs, memories), #1108 (/cost per-turn sub-agent spend), #1102 (hard_deny heredoc false-positive), #1100 (MCP servers outlive panicking yoyo), #1094 (write_file partial-read guard residue), #1086 (shell verbs print REPL slash syntax, 13 verbs), #1084 (cost table reads low in 2 places), #1070 (multi-MB CLAUDE.md silently injected), #1064 (child processes inherit API key, opt-in scrub), #1060 (openai wire residue), #1041 (MCP prefix — mostly landed Day 231/240), #1040 (/cd into trusted project reload), #1037 (stale default models anthropic/openai), #1026 (output-limit cut json note/exit), #1021 (google stale ids), #1018 (durable todos), #1014 (/spawn write tap), #1012 (price drift 75 rows). agent-input: #1096 (@JC351 outreach challenge — @yuanhao answered: X posting handled by separate harness/gasp, stay focused on evolving; no code action here), #991 (yoagent retry_safe_events).

## Research Findings
- yopedia recall: prior delta scans cover Claude Code through 2.1.312 (`agent-changelog-delta-analysis`, `ai-coding-agent-release-notes-october-2026`). New delta ingested (job b5f28466).
- Claude Code 2.1.314 (10-30) / 2.1.316 (10-31): blockWritesOutsideWorkingDirectories; CLAUDE_CODE_SUBAGENT_EFFORT; fixed shell commands left running on self-exit; fixed `/model <id>` not warning on unrecognized ids (yoyo already has warn_once_if_guessing); fixed `--restricted` sessions reading outside dirs via a Read allow rule; fixed sub-agents losing an inherited model on wake; Opus 4.8 fast mode removed 2026-11-30.
- Codex CLI (clap) unknown-argument convention, exit 2: `error: unexpected argument '--x' found` + `tip: to pass '--x' as a value, use '-- --x'`. Since 0.147 even removed hidden flags fail that way rather than warn. This is the reference row for the #1111 class fix: yoyo's refusal should give exit 2, did-you-mean, and the `--` hint (yoyo commit's refusal already does).
- Recommendation for the planner: the highest-leverage product slice visible is the **class fix in `positional_after_verb`/`refuse_stray_arg`** (one authority, ~9 arms, includes the `init --froce` writer), title by class predicate ("shell verbs that take no flags refuse an unknown dash-led flag") with membership listed in the body after probing (d232). Near-miss rows: global flags after a verb still accepted (`version -v --model m`, `status --no-color`), `-- --x` passes, `evolution 5` still works; memories/find/docs take a free-text operand, so decide per verb whether `--bogus` is a search term (Codex/git convention: `--` for that). Second candidate: #1113 (value-slot early scan, `-p --help`).
