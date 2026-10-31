# Assessment — Day 245

## Build Status
Pass — verified by the harness at session start on HEAD 38514750. Binary `./target/debug/yoyo` reports `yoyo v0.3.0 (38514750 2026-10-31)`; `yoyo -p "Reply with exactly: pong"` → `pong`, exit 0 (model claude-opus-5-5, auto-watch skipped: no files changed).

## Recent Changes (last 3 sessions)
- **Day 244 20:27** — Fixes #1106: auto-memory evicts the oldest *auto* entry at MAX_AUTO_MEMORIES=20 instead of refusing forever (the repo's store had been frozen since 2026-08-12); `/remember` entries never evicted; legacy auto entries recognised by builder openings; content-free panic headers not saved (`src/memory.rs` +228, tests in `src/memory_evict_tests.rs`). Fixes #1105: with a fence active, `list_files` withholds an unterminated `find` warning line so a cut inside a denied root's name can't leak the prefix (`src/fence_result_filter.rs`). Note: commit 2c1c7faa repeats Task 1's title but only touches `.yoyo/risk_validations.jsonl` (harness bookkeeping, benign).
- **Day 244 10:44** — explicit numeric flags (`--max-tokens/--temperature/--max-turns/--context-window`) with unparseable values refuse with exit 2 before any request (config-file typos still only warn, reason recorded). Fixes #1023: `list_files`' find warnings go through the fence filter; also fixed "(1 files)" summary being mistaken for a path.
- **Day 244 09:15 / 07:12 / 01:34** — global flag before a subcommand (`yoyo --yes doctor`) refused exit 2; trajectory byte cap fixed (#1104); `yoyo ast` exit codes 0/1/2; bare `goal set` exit 2; synthesize phase spend counted (#1071); unknown `--provider` without base_url refused exit 2 (#1103, was sending the key to localhost:8080); `eval "$(ssh-agent)"`-style setup-tool carve-out (#1066).
- 10/10 recent sessions 2/2 tasks, 0 reverts. CI green.

## Source Architecture
157 files in `src/`, ~201k lines total (incl. tests). Largest: cli.rs 7873, commands_risk.rs 6602, tool_wrappers.rs 5746, tools.rs 5268, agent_builder.rs 4955, config.rs 4851, safety.rs 4720, commands_search.rs 4593, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3924. Entry: `main.rs` → `cli::parse_args` → `dispatch_sub::try_dispatch_subcommand` (shell verbs) or REPL/`-p`; agent construction in `agent_builder.rs`; tool wrapper stack in `tool_wrappers.rs`; fences in `fence_result_filter.rs`, `hard_deny.rs`, `safety/`.

**Size-gate traps for the planner (tests/module_size.rs):**
- `src/commands_risk_unhittable.rs` 2036 lines, unlisted, 36 past cap — **14 more lines = FATAL `cargo test`** (whole-task revert).
- `src/config.rs` 4851 vs recorded 4762 (+89) — **11 more lines = FATAL**. Any task touching config.rs must paste the new count or put tests in a sibling file.
- `src/memory.rs` 1975 — 25 lines below the 2000 cap.

## Self-Test Results
(probed binary 38514750 from the repo dir, stdin </dev/null)
- `yoyo find` → `usage: yoyo find <pattern>` exit 2 ✓ (fixed in #1086 slices).
- `yoyo tree --bad` → `usage: yoyo tree [depth]` exit 2 ✓.
- Still REPL syntax on the shell (#1086 residue, all exit 2 correctly): `yoyo grep` → `usage: /grep ...`; `yoyo map zzz --bad` → `usage: /map ...`; `yoyo lint zzz` → `Unknown /lint subcommand`; `yoyo blame` → `Usage: /blame <file>`; `yoyo outline` → `Usage: /outline ...`; `yoyo ast` → `Usage: /ast <pattern> ...`.
- **`yoyo diff --bad` → `(no uncommitted changes)`, exit 0.** Cause located: `src/commands_git.rs:231` in `diff_options_from_tokens` (shared by both `/diff` doors): `_ if part.starts_with('-') => {} // ignore unknown flags`. So a typo like `yoyo diff --cahced` silently shows the *unstaged+staged* diff instead of staged-only and exits 0 — the Day-244 "only a mistake types this" class (#1103, numeric flags, global-flag-before-subcommand). `git diff --bogus` itself refuses. Only site in src/ with that comment (grep). No open ticket found (#982, the stray-word exit-0 class, is CLOSED). `yoyo diff zzz` (non-path, non-ref) correctly exits 1 via git.
- **`write_file` outside the cwd with `--yes`: succeeds silently.** Probed from `/tmp/p245w`: `yoyo --yes -p "write_file 'probe' to /tmp/p245_outside_write.txt"` → `Wrote 5 bytes`, file created. By design today: `src/outside_read.rs` module doc says the ask-once fence covers the three *read* tools only, writes are "a different fence" (confirm prompt, which --yes skips; `--allow-dir`/`--restricted` are the hard fences). Claude Code shipped `permissions.blockWritesOutsideWorkingDirectories` on 2026-10-30 (opt-in, applies in every permission mode). Gap candidate, product-kind, must be opt-in.
- `yoyo index zzz` → `no such path` exit 1; `yoyo spawn` → clear REPL-only refusal exit 2 ✓.

## Evolution History (last 5 runs)
evolve.yml: 4 most recent completed runs all `success` (2026-10-30 07:09, 09:13, 10:41, 20:21); current run 41155211083 in progress. ci.yml: last 5 all success. Trajectory: CI failures in window all predate going green (2× exit 101 from the `post_hook_feedback_reaches_the_model_as_its_own_tool_result_block` SIGPIPE flake, fixed Day 242). No provider errors, no reverts.

## Capability Gaps
- **Write fence outside the working dir** (CC 2026-10-30 `blockWritesOutsideWorkingDirectories`, every mode): yoyo has an outside-READ fence only; with `--yes` writes anywhere succeed unless `--allow-dir`/`--restricted`. Opt-in config key would be the parity slice.
- **Subagent effort knob** (CC `CLAUDE_CODE_SUBAGENT_EFFORT`): yoyo has `sub_agent_model` but sub-agents inherit thinking/effort. Not verified in this session whether a knob exists — grep `effort` in agent_builder/tools before planning.
- **Children left running when the process exits by itself** (CC fix 2026-10-31): yoyo's open analog is #1100 (MCP servers outlive a panic).
- **Codex 0.164**: `/presets` prompt slots, inline `/model X <prompt>`, credential masking in transcripts (yoyo #1064 is the scrub half), non-destructive compaction failure, resume restoring thread settings.
- Known from backlog: no TUI (#215), no benchmark submission (#156), sub-agent spend absent from /cost and usage records (#1073), `/cd` into a trusted project doesn't load its hooks/allow/MCP/skills (#1040), stale default models on several providers (#1037, #1021), MCP collision prefixing already done (#1041 partly).

## Bugs / Friction Found
1. `yoyo diff --bad` exits 0 ignoring the unknown flag (above).
0. `yoyo diff` ignores unknown flags (exit 0, wrong diff on a typo) — see Self-Test. Small, single site, near-miss rows: `--staged`, `--cached`, `--name-only`, `--stat`, `--explain`, `--functions`, ranges, paths; check whether REPL `/diff` shares the parser (it does: "Shared token classification for both `/diff` doors") so the REPL door's behaviour must be decided too.
2. #1086 residue: 6+ shell verbs still print `/verb` usage (grep, map, lint, blame, outline, ast; plus skill per the 20:38Z comment).
3. Size gate near-misses (config.rs, commands_risk_unhittable.rs) — not bugs, but a task landing there will be reverted unless the count is pasted.
4. CLAUDE_CODE_GAP.md header verified day 74 — 170 days stale.

## Open Issues Summary
agent-self backlog (open): #1102 hard_deny heredoc body residue (incidence carrier, no decision yet); #1100 MCP servers outlive a panicking yoyo; #1094 write_file partial-read guard residue (never-read overwrite, process-wide store); #1086 shell verbs print REPL syntax (8 open per Day-244 probe); #1084 cost table reads low in two places vs error-direction policy; #1073 sub-agent spend missing from usage/cost; #1070 multi-MB CLAUDE.md goes in whole; #1064 child processes inherit API key (opt-in scrub shipped; default split by population is mine to decide per @yuanhao); #1060 openai wire residue; #1041 MCP prefixing (Day 240 slices); #1040 /cd trusted-project reload; #1037/#1021 stale defaults; #1026 output-limit cut in json modes; #1018 durable todos; #1014 /spawn write tap; #1012 price drift audit (75 rows).
agent-input: #1096 X outreach challenge (answered Day 244: replied, not started — X posting is read-only by design, decision is the owner's); #991 yoagent retry_safe_events — **my either/or to @yuanhao (2026-10-29) is unanswered; hold item 3 (do not drop/keep collected_text until he answers).**

## Research Findings
Recalled yopedia (latest prior delta note 2026-10-09); saved a new delta note (job 91be9146).
- **Claude Code, Oct 30–31 2026**: `permissions.blockWritesOutsideWorkingDirectories`; `CLAUDE_CODE_SUBAGENT_EFFORT`; fixed shell commands left running when CC exits by itself; fixed `/model <id>` not warning for unrecognised ids (assumes 200K) — yoyo already warns (`warn_once_if_guessing`, #1021); fixed `--restricted` uploading files outside working dirs via a Read allow rule (worth a probe of yoyo's `--restricted` + `[permissions] allow` interaction); fixed Grep/Glob hanging when the killed search process doesn't exit; idle compaction; Opus 4.8 fast mode removed Nov 30.
- **Codex CLI 0.164.0 (Oct 23)/0.163.0 (Oct 15)**: `/presets`, inline `/model`/`/permissions` in a prompt, image previews, MCP OAuth GA, credential masking, CLI `-c` overrides preserved across subcommand levels (yoyo instead refuses a global flag before a subcommand, Day 244), searchable `/config` showing restart-needed settings, steer mid-response, parallel read-only tools, "local commands finish promptly when a child keeps an output pipe open".
- Biggest structural gaps unchanged: TUI (#215), benchmarks (#156). Biggest *cheap* gap found today: the write fence (opt-in) and the diff flag typo.

## Notes for the planner
- **#991 item 3 is on hold**: my either/or to @yuanhao (2026-10-29T02:44Z) is unanswered (~47h; his spread is 2–92h). Do not act on collected_text either way.
- **#1064**: @yuanhao handed the default split back to me on Day 236 — it is mine to decide, not pending.
- Size gate: config.rs has 11 lines of headroom before FATAL, commands_risk_unhittable.rs 14. Put tests in sibling `*_tests.rs` files and paste counts if touched.
- Trajectory epistemic hint: never-forecast files include `src/grep_status.rs`, `src/tools_search_flag_tests.rs`; `src/prompt/turn_prefix.rs` predicted 3×, never graded.
