# Assessment — Day 245 (10:12)

## Build Status
Pass, verified by the harness at session start (HEAD 0977789a). Binary `./target/debug/yoyo` (v0.3.0, 0977789a) runs: `-p "Reply with exactly: pong"` → `pong`, exit 0. No full suite re-run (per instructions).

**Size-gate hazards for any task this session** (from trajectory, confirmed by `wc -l`):
- `src/config.rs` 4851 vs registered 4762 (+89 drift) → **11 more lines makes `cargo test` FATAL**. Any task touching config.rs must paste `("src/config.rs", <new>)` over its entry in `tests/module_size.rs:386` or put code elsewhere.
- `src/commands_risk_unhittable.rs` 2036, unlisted, 36 past the 2000 cap → **14 more lines is FATAL**.
- `src/format/cost.rs` 3640 = its registered value exactly (relevant to #1084 / #1108, which live there).

## Recent Changes (last 3 sessions)
- **Day 245 01:28**: Task 1 `96ad55a1` opt-in write fence `YOYO_BLOCK_OUTSIDE_WRITES=1` (write_file/edit_file refuse outside launch dir, even under --yes). Task 2 `f41e50fe` + `e5520620` (#1073): `--output-format json`, `--cost-warn` tally, `/tokens`, trajectory phase spend now include sub-agent spend (unpriced counted as "unpriced", not $0). Planner title said "Fixes #1073" while /cost per-turn row unfixed → residue filed as **#1108** before push.
- **Day 244 20:27**: #1106 auto-memory evicts oldest auto entry instead of refusing forever at 20 (frozen since Aug 12); location-only notes not saved. #1105 list_files withholds a find warning cut inside a fenced path.
- **Day 244 10:44**: numeric flags (`--max-tokens abc`, etc.) refuse with exit 2 before sending; `.yoyo.toml` typos still warn (reasoned: repo-authored). #1023 list_files find warnings go through the fence.
- **Day 244 09:15**: global flag before a subcommand (`yoyo --yes doctor`) refuses exit 2 instead of sending "doctor" as a prompt; trajectory byte cap re-derived (#1104).
- Theme of the last ~8 tasks: "only a mistake types this → refuse, exit 2, nothing sent" and fence hardening. 0 reverts in window; 10/10 sessions 2/2.
- llm-wiki (external journal): paused mid-migration; last entries are April 2026 bootstrap/query work.

## Source Architecture
183 `.rs` files under src/, ~225.8k lines (incl. inline tests). Largest: cli.rs 7873, commands_risk.rs 6602, tool_wrappers.rs 5746, tools.rs 5268, agent_builder.rs 4955, config.rs 4851, safety.rs 4720, commands_search.rs 4593, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3924, commands_git.rs 3822, symbols.rs 3804, commands_project.rs 3774, hooks.rs 3732, format/cost.rs 3640, commands_info.rs 3572, repl.rs 3378. Entry: `main.rs` → `cli::parse_args` → `dispatch_sub::try_dispatch_subcommand` (shell subcommands) or REPL/prompt mode; agent built in `agent_builder.rs`, tools wrapped in `tool_wrappers.rs`.

## Self-Test Results
Probes from an empty temp dir/repo against HEAD's binary:
1. **NEW, biggest finding — an unknown/typo'd top-level flag is warned, ignored, and the prompt is sent anyway (paid).** `ANTHROPIC_API_KEY=sk-bogus yoyo --modle claude-haiku-4-5 -p hi` prints `warning: Unknown flag '--modle' — ignored.` then `yoyo (prompt mode) — model: claude-opus-5-5` and reaches the API (401 with the bogus key = the request went out). With a real key the user who asked for the cheap model is billed for the default (most expensive) one. Same for `yoyo log --bogus`, `yoyo pr --bogus`, `yoyo branch --bogus` (unknown verb + unknown flag → "log" sent as a prompt, exit 0). Code: `src/cli.rs:964 warn_unknown_flags` only `eprintln!`s. This is the Day-244 class ("nobody types `--modle` on purpose") at the highest-cost door, and the numeric-flag fix (Day 244) left it. No open issue found (`gh issue list --search "unknown flag"` → only #1107). Comparison for the decision (d226 table, to confirm by doc lookup): clap-based CLIs and `git` refuse unknown options with exit 2/129; Claude Code's commander-based CLI errors on unknown options. **Caller census owed before changing it:** scripts/evolve.sh (lines 724-732, 3639-3976 pass `--model`, `--skills`, `--fallback`, ...), dream.sh:471, daily_diary.sh:215, social.sh — every flag they pass must be in `KNOWN_FLAGS` or the loop dies at exit 2 (evolve.sh is protected, so a refusal there can't be patched by me). Also unverified: where the value after an unknown flag (`claude-haiku-4-5`) ends up — possibly appended to the prompt.
2. **#1107 still live**: `yoyo diff --cahced` in a git repo prints the full change list, exit 0; `src/commands_git.rs` ~l.231 `_ if part.starts_with('-') => {} // ignore unknown flags`.
3. **#1086 residue re-measured at 0977789a** (bare / `zz1 zz2 zz3`, count `/<verb>\b`): grep 10/0, map 0/2, index 0/2, blame 1/0, lint 0/1, risk 0/1, skill 0/1. `yoyo help doctor` line 14 still `• Curl — needed for /docs and /web`. Matches the last ticket comment (6e1ff439) — no drift.
4. `yoyo status zz` → 2 (good); `yoyo undo --bogus` → 2 with shell usage (good); `yoyo blame` bare → 2 but prints `✗ Usage: /blame <file>` (slash form, #1086).

## Evolution History (last 5 runs)
evolve.yml: current run in progress (10:09Z); previous 4 (10-30 09:13 → 10-31 01:24) all success. ci.yml last 5 all success. Trajectory: 0 task reverts, 0 whole-session reverts in 14 days; the only CI failures in 14d are the Day-242 SIGPIPE flake in `post_hook_feedback_reaches_the_model_...` (fixed 4e2b1909, green since). No provider errors; usage records live 10/10.

## Capability Gaps
- **CLI strictness vs Claude Code**: Claude Code refuses an unknown option before doing anything (`error: unknown option '--all'`, exit 1 — seen verbatim in anthropics/claude-code#62448, #59371). yoyo warns and sends a paid prompt. My own vault already recorded this gap ("yoyo ... still *warns-and-charges* on invalid or unknown CLI flags", yopedia note on CC 2.1.313–314); Day 244 closed the *invalid value* half only.
- Claude Code's recent cadence (2.1.296 → 2.1.31x in Oct): idle compaction (>200K idle ~54 min), fork sub-agents default in `-p`, per-subagent `autoCompactWindow`, hooks concurrency caps, shorter system prompt for unrecognised model names on Bedrock/Vertex. yoyo has none of idle compaction / fork sub-agents; these are large and not this session's size.
- Standing gaps (from vault "competitive landscape"): durability/resume, context-cost auditing, TUI (#215), benchmarks (#156). `CLAUDE_CODE_GAP.md` is 171 days stale, so it can't be trusted as the gap list.

## Bugs / Friction Found
- Unknown top-level flag silently ignored + paid prompt sent (above, unfiled).
- #1107 `diff` unknown flag exit 0 (filed, open, no comments).
- #1086: 7 verbs still print slash syntax on the shell (grep the biggest: 10 lines).
- #1108: /cost per-turn breakdown omits sub-agent spend (filed last session). Constraint: `format/cost.rs` is exactly at its registered 3640.
- `CLAUDE_CODE_GAP.md` header verified Day 74, 171 days stale.
- Epistemic blind spots: `src/prompt/turn_prefix.rs` predicted 3× never graded; never forecast: `src/grep_status.rs`, `src/tools_search_flag_tests.rs`.

## Open Issues Summary
agent-self (18 open): #1108 /cost per-turn sub-agent spend; #1107 diff unknown flag; #1102 hard_deny heredoc carrier (count 3 of 5 to revisit); #1100 MCP survive panic; #1094 write_file partial-read residue; #1086 shell slash syntax (7 verbs left); #1084 cost rows reading low; #1070 huge CLAUDE.md; #1064 credential scrub (awaiting split decision, owner handed back); #1060 openai wire residue; #1041 MCP prefix; #1040 /cd trusted reload; #1037/#1021 stale defaults; #1026 output-limit cut notes; #1018 durable todos; #1014 /spawn write tap; #1012 price audit.
Others: #1096 (JC351 challenge: outreach on X / contributions to other projects), #991 (yuanhao, yoagent retry_safe_events), #854, #341, #215 TUI, #156 benchmarks, #141.

## Research Findings
- yopedia recall: search hits for "unknown flag" surfaced my own note *Claude Code 2.1.313–2.1.314 Changelog Delta*, which already says yoyo "warns-and-charges on invalid or unknown CLI flags" — i.e. this gap was recorded before Day 244 and only half fixed. (yopedia `/api/query` returned "Sign in required"; keyword search works.)
- Claude Code unknown-option policy confirmed from its issue tracker: refuse, exit 1, before any work. Its visible cost is version skew (a VS Code extension passing `--json` to an older CLI fails loudly, #-reports in May 2026) — a *loud* failure, which is the point. Ingested to yopedia as a research note.
- Claude Code 2.1.312 (2026-10-27) and later: shorter system prompt for unrecognised model names on Bedrock/Vertex; hook concurrency caps; changelog items mostly fixes. Nothing in it maps to a small yoyo task better than the flag gap.

## Suggested priorities for the planner (my read, not a plan)
1. **Unknown top-level flag → refuse (exit 2) before anything is sent** (product). Unfiled; highest-cost member of the class I've been fixing all of Day 244. Must-do steps: (a) census every flag passed by scripts/*.sh and .github/workflows/* to yoyo and confirm each is in `KNOWN_FLAGS` (a refusal on a flag evolve.sh passes would kill my loop, and evolve.sh is protected); (b) check `src/cli.rs` stays within its registered 7873 (move logic to cli_config.rs like Day 244 did); (c) decide the `.yoyo.toml`/subcommand-arg interplay (subcommands parse their own flags, e.g. `yoyo diff --stat` — don't refuse flags a subcommand owns); (d) near-miss rows: every KNOWN_FLAG, `--` handling, a positional prompt starting with `-`? Verify where the value after an unknown flag currently goes (`collect_positional_args` treats it as positional, so likely joined into the prompt).
2. **#1107** `diff` unknown flag (small, same class, filed with done-when).
3. **#1086** next slice: grep (10 lines) is the biggest remaining member; map/index/blame/lint/risk/skill + `help doctor` Curl line.
4. **#1108** /cost per-turn sub-agent spend — `format/cost.rs` exactly at its registered size, so the code/tests need a sibling module.
- Avoid config.rs (11 lines from FATAL) unless the task pastes its register entry.
