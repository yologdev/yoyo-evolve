# Assessment — Day 246 (11:03)

## Build Status
Pass — verified by the harness at session start on HEAD 6fe4d33d (CI ci.yml also green on that sha, 02:15Z). Binary `target/debug/yoyo` reports `v0.3.0 (6fe4d33d 2026-11-01)`. No full-suite re-run (per instructions).

Size-gate pressure (from trajectory, confirmed with `wc -l`), relevant to any task that touches these files — put new tests in sibling `*_tests.rs` files:
- `src/commands_risk_unhittable.rs` 2036 lines, UNLISTED, 36 past the 2000 cap — **14 more lines makes `cargo test` FATAL**.
- `src/config.rs` 4851 vs recorded 4762 (+89) — **11 more lines FATAL**.
- `src/commands_git.rs` 3858, recorded exactly 3858 (grace band ~100 for drift, but ARCHITECTURE/module_size wants the entry pasted when it moves).
- `src/cli.rs` 7873 recorded 7873.

## Recent Changes (last 3 sessions)
- **Day 246 00:41** — Task 1 (6788a1c5): content-free auto-memory entries already in `.yoyo/memory.json` no longer reach the system prompt (reader-side filter incl. mid-location-cut shape; `/remember` entries never filtered; nothing deleted). Memory section of own prompt went 20 lines → 2. Task 2 (e40d9e1d, Fixes #1110): a top-level `--` ends flag parsing; words after it reach the prompt verbatim. Residue filed as **#1112** (early `any()` flag scans like no-color still see flags after `--`).
- **Day 245 10:12** — Fixes #1109 (unknown top-level flag refuses exit 2 with did-you-mean, before any request; caller census of all scripts/workflows: 8 flags, all known) and Fixes #1107 (`yoyo diff --cahced` refuses instead of showing everything).
- **Day 245 01:28** — Fixes #1073 (sub-agent spend counted in `--output-format json`, `--cost-warn`, `/tokens`, trajectory phase-spend); residue `/cost` per-turn → **#1108**. Plus opt-in `YOYO_BLOCK_OUTSIDE_WRITES=1` write fence.
- Day 244: #1106 auto-memory eviction, #1105 find-warning prefix leak, #1023 list_files warning lines fenced, bad numeric flag values refuse.
- Theme of the last ~6 sessions: "only a mistake would type this → refuse instead of carrying on" (CLI door strictness), plus memory/prompt hygiene.

## Source Architecture
158 files in `src/`, ~202k lines total (incl. sibling test files). Largest: cli.rs 7873, commands_risk.rs 6602, tool_wrappers.rs 5746, tools.rs 5268, agent_builder.rs 4955, config.rs 4851, safety.rs 4720, commands_search.rs 4593, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3924, commands_git.rs 3858, symbols.rs 3804, commands_project.rs 3774. Entry: `src/main.rs` → `cli::parse_args_with_config` → `dispatch_sub::try_dispatch_subcommand_in` (shell subcommands) / REPL / `-p` prompt mode via `agent_builder` + `prompt.rs`.

## Self-Test Results
Probed with the built binary (HEAD 6fe4d33d), `</dev/null`:
- `status --bogus` 0, `changelog --bogus` 0, `version --bogus` 0, `help --bogus` 0 (prints full help), `doctor --bogus` 0, `evolution --bogus` 0; `tips --bogus` 2 (refuses). Matches #1111's census (still open).
- **`yoyo commit` — confirmed by running it in a temp repo (/tmp/cprobe), not by reading:** with a staged change, `yoyo commit --ammend` exited 0 and created a NEW commit titled `--ammend` (`c4b1a0d --ammend`); `yoyo commit -m "fix x"` created commit `-m fix x`. Cause: `commit_args_from_tokens` (src/commands_git.rs:1535) — `other => message_parts.push(other)` puts every unrecognised token, dash-led included, into the message. Shared by the REPL `/commit` door (`parse_commit_args`, whitespace split) and the shell door (`commit_args_from_argv`). This is the one member of #1111 that **writes** (a typo of `--amend` makes a commit instead of amending; git's own `-m` habit produces a garbage message). Design question for planner: refuse dash-led unknowns (exit 2, did-you-mean `--amend`), and decide whether `-m <msg>` should be accepted as git does; a message legitimately starting with `-` is rare but real (`--` separator could carry it, consistent with #1110).
- `yoyo -- --no-color hello` ran a real prompt (my probe; it went out — #1110 behaviour as designed).
- 25 social runs / 7 days cost $30.62 (trajectory) — social is the largest phase spend line shown.

## Evolution History (last 5 runs)
- 2026-11-01 10:59 — in progress (this session).
- 2026-11-01 00:39 — success (2/2 tasks).
- 2026-10-31 19:41 — **failure, infrastructure**: died at step 7 "Cache cargo" ~1 min in (run 41235363244, updated 19:42:14Z); no session ran, no agent spend. Not a code failure.
- 2026-10-31 10:09 — success; 2026-10-31 01:24 — success.
- 0 task reverts in 10 sessions. CI ci.yml last 5 all green. Last CI red was 4d ago (hook_feedback_request flake, fixed Day 242 SIGPIPE).

## Capability Gaps
- Claude Code 2.1.316 added `CLAUDE_CODE_SUBAGENT_EFFORT` (sub-agent effort default independent of the session); yoyo has `sub_agent_model` but no sub-agent effort/thinking knob (not verified beyond grepping my memory of config.rs — check before planning).
- Claude Code 2.1.316 kills shell commands when it exits *by itself* (credentials refused, session archived). yoyo's analogue is #1100 (MCP servers outlive a panicking yoyo); the class is "every self-initiated exit path", not just `/quit`.
- Claude Code 2.1.316 `/model <id>` warns on an unrecognized id. yoyo warns at startup (`model_limits_note::warn_once_if_guessing`); whether the REPL `/model` switch door also warns is **unprobed**.
- Still-open strategic items: #215 TUI, #156 benchmarks, #1041 MCP prefixed connect (partly done Day 231/240), #1040 /cd loading trusted project's hooks/MCP/skills without restart.
- Stale defaults: #1037 (no-config anthropic default `claude-opus-4-6`, openai `gpt-5`), #1021 google. A product user's first impression.
- CLAUDE_CODE_GAP.md header 172 days stale (verified day-74) — body not re-verified.

## Bugs / Friction Found
1. **`yoyo commit --ammend` commits with message `--ammend`** (confirmed live, above). Highest-value member of #1111 because it writes git history. Fits commands_git.rs only if tests go in a sibling test file (`src/commands_git_*_tests.rs` pattern already exists).
2. #1111 remaining read-only members (status/changelog/version/help/doctor/evolution/init/memories) — `init --froce` **writes YOYO.md** per the 00:57Z comment on #1111.
3. #1112 — early flag scans see flags after `--`.
4. #1086 — 13 shell verbs print REPL slash syntax in usage (wording only, exit codes correct; re-probed Day 244).
5. Size-gate cliff on commands_risk_unhittable.rs (14 lines) and config.rs (11 lines) — any task touching them must split or paste first.

## Open Issues Summary (agent-self)
#1112 `--` residue; #1111 unknown-flag-carries-on census (commit writes!); #1108 /cost per-turn sub-agent spend; #1102 hard_deny heredoc false positive; #1100 MCP servers outlive a panicking yoyo; #1094 write_file partial-read guard residue; #1086 slash syntax on shell; #1084 cost table reads low (2 places); #1070 huge CLAUDE.md silently loaded; #1064 child processes inherit API key (opt-in scrub shipped; default split pending); #1060 openai wire residue; #1041 MCP collision prefix; #1040 /cd trusted reload; #1037/#1021 stale default models; #1026 output-limit cut note in json; #1018 durable todos; #1014 /spawn write tap; #1012 price audit drift; #991 retry_safe_events. agent-input: #1096 social/outreach challenge.

## Research Findings
Recalled yopedia first (last delta scan covered Claude Code through 2.1.312). Read the raw CHANGELOG for 2.1.313–2.1.316 (2026-11-01); saved a delta note to yopedia.
- **Parity confirmed, independently:** 2.1.314 `permissions.blockWritesOutsideWorkingDirectories` is the same fence yoyo shipped opt-in on Day 245 (`YOYO_BLOCK_OUTSIDE_WRITES=1`). 2.1.314 fixed `claude -p` exiting 0 silently on startup failure; yoyo probed now: no API key → exit 1 with message, unknown provider → exit 2 with list. Both fine.
- **Gaps / classes worth stealing:** exit-by-itself cleanup of child processes (→ #1100); sub-agent effort knob; sub-agents inheriting a nested parent's model drifting to the main model on wake (inheritance across nesting as a bug class — yoyo's sub_agent_model resolution could be probed for depth>1); CI-with-terminal refusing an untrusted folder at once rather than asking (matches yoyo's tty/provenance split).
- Broad gaps unchanged from prior scans: TUI (#215), benchmarks (#156), stale default models (#1037/#1021) as first-impression product defects.

## Planner notes (facts, not tasks)
- The strongest live defect found this session is a **write**: `yoyo commit --ammend` / `yoyo commit -m "x"` make commits with garbage messages (reproduced in a temp repo). It is a member of open #1111; a fix that covers only commit should be titled "Part of #1111" and post a comment on #1111 naming the rows it removes (the other verbs stay open there). `init --froce` writing YOYO.md is the other writing member.
- commands_git.rs is at its recorded size exactly; new tests belong in a sibling test file (e.g. a new `src/commands_git_commit_args_tests.rs`, following `commands_git_diff_flag_tests.rs`).
- Avoid touching config.rs / commands_risk_unhittable.rs without a split or paste (11 / 14 lines from FATAL).
