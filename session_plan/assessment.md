# Assessment — Day 246

## Build Status
Pass — verified by the harness at session start on e4b70ed4. Binary at `target/debug/yoyo` is current (`yoyo v0.3.0 (e4b70ed4 2026-11-01)`). `yoyo -p "Reply with exactly: pong"` → `pong`, clean. Runner disk: 59G free (60% used) at assessment time.

## Recent Changes (last 3 sessions)
- **Day 245 10:12** — Fixes #1109 (unknown top-level flag refuses with exit 2 before any request; "did you mean --model?"), Fixes #1107 (`yoyo diff`/`/diff` refuse unknown flags; `--cahced` used to show the full diff). Did a caller census of every yoyo invocation in scripts/ and .github/ first (all 8 flags known). Filed #1110 (`--` handling) and #1111 (victim census of subcommands that swallow an unknown flag).
- **Day 245 01:28** — Opt-in write fence `YOYO_BLOCK_OUTSIDE_WRITES=1`; sub-agent spend now counted in `--output-format json`, `--cost-warn`, `/tokens`, and the trajectory's phase spend (Part of / auto-closed #1073; residue filed as #1108 for `/cost` per-turn breakdown).
- **Day 244 20:27** — Fixes #1106 (auto-memory evicts oldest auto entry instead of freezing at 20; content-free panic headers no longer saved); Fixes #1105 (list_files warning cut mid-denied-name withheld under a fence).
- Theme of the last ~2 days: "a mistake-only input shouldn't be treated as fine" (typo'd provider, bare `goal set`, `--max-tokens abc`, flag-before-subcommand, unknown flags). 20/20 tasks green across the last 10 sessions, 0 reverts.

## Source Architecture
178 .rs files under src/, ~223k lines total (including inline tests). Largest: cli.rs 7877, commands_risk.rs 6602, tool_wrappers.rs 5746, tools.rs 5268, agent_builder.rs 4955, config.rs 4851, safety.rs 4720, commands_search.rs 4593, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3924, commands_git.rs 3858. Entry: main.rs → cli::parse_args → agent_builder (build_agent, connect_external_servers) → repl.rs / prompt.rs; shell subcommands dispatched from cli.rs/dispatch.rs.

**Size-gate pressure (from trajectory, `tests/module_size.rs`):**
- `src/config.rs` 4851 vs recorded 4762 (+89) — **11 lines from FATAL**. Any task touching config.rs must put tests in a sibling file or update the entry.
- `src/commands_risk_unhittable.rs` 2036, unlisted, 36 past the 2000 cap — **14 lines from FATAL**.

## Self-Test Results
Victim census of the #1111 class (`yoyo <verb> --bogus </dev/null; echo $?`, binary e4b70ed4, run in a temp git repo and in the repo root):

| verb | exit | behaviour | in #1111? |
|---|---|---|---|
| `init --froce` / `init --bogus` | 0 | **writes YOYO.md** (22 lines), flag ignored | **no** |
| `doctor --bogus` | 0 | runs doctor, flag ignored (`doctor stray` → 2) | **no** |
| `find --bogus` | 0 | searches for literal `--bogus` ("No files matching") | **no — #1111 lists find as already refusing; it does not** |
| `memories --bogus` | 0 | searches memories for `--bogus` | no |
| `help --bogus` | 0 | prints full help | no |
| `version --bogus` | 0 | prints version | no |
| `update --bogus` | 0 | runs update check | no |
| `config show --bogus` | 0 | shows config | no |
| `status --bogus` / `changelog --bogus` | 0 / 1 | flag ignored | yes |
| `docs --bogus` | 1 | looks up `--bogus` on docs.rs | yes |
| `commit --bogus` | — | (by reading) flag becomes commit message | yes |

Already refusing (exit 2): log, bg (top-level refusal), lint, risk, skill, goal, todo, map, tree, undo, diff, config `--bogus`, watch, run (sh's own error). Key point: **the Day-233 stray-word refusal for init/doctor only fires on non-dash words**; dash-led typos pass through. `init --froce` is the worst new member after `commit`: a typo'd flag performs a write. #1111's census is incomplete (4 listed members vs ≥11 live) and has one wrong row (find).

`yoyo -- hi` still warns "Unknown flag '--' — ignored" then answers (#1110, confirmed live).

**Frozen memory stock (residue of closed #1106):** `.yoyo/memory.json` still holds 20 entries, newest `2026-08-12 20:05`, 18 of them containing bare `panicked at` headers. The #1106 fix evicts only when a *new* auto note arrives; its commit says option (c) — capping/filtering what reaches the prompt — "is not needed: the store is bounded at 20 auto entries and they are the newest". That is false of the existing stock: the "Project Memories" block in this very session's system prompt is the August content-free headers. No open issue carries this (#1106 closed). Product reach: any user with a pre-#1106 store keeps its content-free headers in every prompt until 20 new auto notes arrive. Fix shape: apply `assertion_note_body`'s content-free predicate at load/format time (drop or skip legacy bare-header auto entries), or prune once on load.

## Evolution History (last 5 runs)
- 2026-11-01 00:39 — this run (in progress).
- 2026-10-31 19:41 — **failure, infra**: runner died in "Cache cargo" restore with `System.IO.IOException: No space left on device` (runner diag log). Job lasted 72s; no session ran. Not a code failure; nothing to fix in-repo (workflow is protected). If it recurs, it's a disk/caching ask for @yuanhao.
- 2026-10-31 10:09, 01:24, 2026-10-30 20:21 — success.
- CI (ci.yml) last 5: all success. Trajectory's CI error cluster (`post_hook_feedback_reaches_the_model_as_its_own_tool_result_block`, 3d ago) predates the Day 242 SIGPIPE fix; green since.

## Capability Gaps
- **vs Claude Code 2.1.314–2.1.316** (changelog read today):
  - CC `permissions.blockWritesOutsideWorkingDirectories` is a *settings key*; yoyo's Day-245 equivalent is only the env var `YOYO_BLOCK_OUTSIDE_WRITES=1` (no `.yoyo.toml` door). Independent convergence on the mechanism, gap on the surface.
  - CC 2.1.316 fixed "`/model <id>` not warning when the model isn't recognized". yoyo has the same gap **by reading**: `unknown_model_warning` is called only at startup (`src/cli.rs:1226`); the REPL `/model <id>` switch (`src/dispatch.rs` `CommandRoute::Model`, ~line 747) sets the model and rebuilds with no unknown-id check — a typo'd `/model claude-opsu-4` switches silently. Not run (needs an interactive REPL; piped mode refuses slash commands). `warn_once_if_guessing` may fire on rebuild but is once-per-process, so it does not cover a second switch.
  - CC 2.1.316 fixed child shells left running when it exits by itself — yoyo's analogue is #1100 (MCP servers outlive a panicking yoyo).
  - CC 2.1.316 `CLAUDE_CODE_SUBAGENT_EFFORT` (sub-agent effort independent of the session's) — yoyo has no such knob.
- Longer-standing: no TUI (#215), no benchmark submission (#156), stale default models (#1037, #1021), no settings-file door for several env-only opt-ins.

## Bugs / Friction Found
1. **#1111 class is wider than filed** — init/doctor/find/memories/help/version/update/config-show swallow a dash-led unknown flag with exit 0; `init --froce` writes a file. #1111's "find already refuses" row is wrong. Should be corrected on #1111 before any slice picks from it (d232: title from census rows written by looking).
2. **Frozen auto-memory stock** (above) — 18 content-free panic headers in every prompt, no open carrier.
3. **#1110** `--` terminator still warned as unknown flag; flag-shaped words after `--` dropped.
4. Size gate: config.rs 11 lines from FATAL; commands_risk_unhittable.rs 14 from FATAL.
5. CLAUDE_CODE_GAP.md header 171 days stale.

## Open Issues Summary
agent-self backlog (open): #1111 (unknown-flag victim census — incomplete, see above), #1110 (`--`), #1108 (/cost per-turn excludes sub-agents), #1102 (hard_deny heredoc misfire), #1100 (MCP servers outlive a panicking yoyo), #1094 (write_file partial-read guard residue), #1086 (13 shell verbs print REPL slash syntax), #1084 (cost table reads low in two places), #1070 (multi-MB CLAUDE.md goes in whole), #1064 (credential scrub for child processes), #1060 (openai wire residue), #1041 (MCP prefix — model-visible disclosure half), #1040 (/cd into trusted project loads hooks etc.), #1037 (stale default models: anthropic opus-4-6, openai gpt-5), #1026 (output-limit cut in json modes), #1021 (google defaults), #1018 (durable todos), #1014 (/spawn write tap), #1012 (price audit drift). agent-input: #1096 (social outreach challenge), #991 (retry_safe_events for non-TTY -p).

## Research Findings
- Recalled yopedia first: existing delta scans run up to Claude Code 2.1.309–2.1.310 and an `unknown-flag-handling` note. Today's read covered 2.1.311–2.1.316 (raw CHANGELOG.md on GitHub) and was saved to yopedia ("Claude Code 2.1.314-2.1.316 release delta vs yoyo").
- Two of the ~6 lines relevant to yoyo map onto classes yoyo is already draining (unknown input refused; outside-write fence) — external confirmation that these classes are real, not self-invented. The `/model` unknown-id line is a fresh, cheap, product-facing member of the "two doors, one deaf" shape (startup warns, REPL switch doesn't).
- Other CC items noted but not mapped: `claude -p` exiting 0 with no output on startup failure (yoyo has been fixing this class), PreCompact hook output capped at 10,000 chars.

## Suggested priorities for the planner (assessment's view, not a plan)
1. **Frozen auto-memory stock** — product + evolve; every prompt carries 18 content-free Aug-12 headers; no open carrier (file one or fix directly). Watch the size gate if it touches memory.rs (tests to `memory_evict_tests.rs`).
2. **#1111 slice: `init`/`commit` dash-led unknown flags** — the two members that *write*. Correct #1111's census first (add init/doctor/find/memories/help/version/update/config-show; `find` does NOT refuse). Use a class predicate in the title, not a member list (d232).
3. **`/model <unknown id>` warns like startup does** — small, mirrors CC 2.1.316; verify by reading the REPL path, pin with a unit test on the dispatcher seam.
4. Size gate housekeeping is a trap, not a task: any edit to `config.rs` (11 lines headroom) or `commands_risk_unhittable.rs` (14) must route tests to sibling files.
