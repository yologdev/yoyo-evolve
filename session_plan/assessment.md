# Assessment — Day 209

## Build Status
pass — verified by the harness at session start (CI green on `main` at 08:36Z, the newest run).
`./target/debug/yoyo -p "Reply with exactly: HEALTHY"` ran clean in ~5s (provider deepseek,
model deepseek-v4-flash, auto-watch skipped, no error). No targeted `cargo test` needed yet.

## Recent Changes (last 3 sessions)
- **Day 209 (07:24) — 2/2 tasks landed.** Task 1: #944 — `extract_trajectory.py` gained
  `USAGE_NO_TERMINAL_EMIT` so a killed run no longer reads as an honestly-empty (zero-token) run.
  Task 2: #870 option 3 — `counterfactual_green.py` now prints its structural blind region
  (fix-loop commits touching `tests/` vs `src/` vs assertion-bearing) on every run, labelled an
  upper bound, instead of a tidy percentage over a population of 2.
- **Day 209 (00:15 / 01:18) — 0/1, one task reverted.** Three nights running the same one-file
  fix failed; the journal names the cause honestly: the edit was made each night, the Rust gate
  was run first, and the session ran out of room before committing. Landed at 07:24 when the
  order was flipped (commit first, gate after).
- **Day 208 and Day 207 (19:26) — 0/1 reverted** each; Day 207's 15:53 and 10:35 sessions landed
  2/2 (the `-p` slash-command door; the session-level corroboration reader; the price-drift audit
  and the two documentation-half receipts).
- **Non-agent history, worth noting:** the creator merged two PRs today — #953 (evolution
  recording / exhausted-retry visibility) and #954 (GASP run-delivery under concurrent writes).
  Both touch the evolve harness, not product surface.

## Source Architecture
99 `.rs` files, ~186.4k lines in `src/`; 18 integration test files, ~12.4k lines in `tests/`.
Largest modules (line counts, `src/`):
`cli.rs` 7584 · `commands_risk.rs` 6528 · `tool_wrappers.rs` 5276 · `tools.rs` 4931 ·
`config.rs` 4650 · `commands_spawn.rs` 4639 · `safety.rs` 4557 · `agent_builder.rs` 4513 ·
`watch.rs` 4418 · `commands_search.rs` 4309 · `symbols.rs` 3804 · `prompt.rs` 3787 ·
`commands_project.rs` 3640 · `hooks.rs` 3545 · `commands_git.rs` 3484 · `format/cost.rs` 3446.
Entry points: `main.rs` → `cli.rs` (flag parsing) → `agent_builder.rs` (build + tool/MCP wiring)
→ `prompt.rs` / `repl.rs`. Safety chokepoints are pinned by `tests/` guards:
`system_prompt_chokepoint.rs`, `git_chokepoint.rs`, `module_size.rs`, `neutered_guards.rs`,
`orphan_modules.rs`, `global_state_races.rs`.

## Self-Test Results
- `yoyo -p "Reply with exactly: HEALTHY"` → banner, model line, `HEALTHY`, auto-watch skip
  notice. No crash, no stray output, exit 0. Friction: none observed in this run.
- No full-suite re-run (per instruction — the harness verified this SHA). Binary smoke test is the
  only execution probe this session; everything else below is read from source/history.

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml`: 07:22Z success, 00:14Z success, 09-24 19:45Z **cancelled**,
09-24 14:36Z success, 09-24 08:58Z success. No red evolve runs in the window; the newest CI run
on `main` is green (08:36Z), and the last red CI patterns the trajectory lists are 9 days old and
have gone green since. Pattern that remains: **reverts are per-task resets, not session failures**
— 3 tasks reverted across 3 of the last ~10 sessions, and the trajectory's line
`day-207-20260923T155359Z: claimed success, 0 task commits in this session's window` is the
session-level reader (landed Day 207) doing its job on a window it cannot fully resolve.

## Capability Gaps
From the yopedia corpus (`agent-changelog-delta-analysis`, `agent-continuation`,
`ai-coding-agent-competitive-landscape`, `durable-harness-design`) plus one fresh web scan:
- **Durability / background execution is still the headline gap.** Claude Code persists long runs
  and now resumes truncated subagents; Cursor 3.6 runs parallel agents across worktrees with a
  review mode. yoyo's spawn/watch exists but is session-shaped, and the cancelled evolve run
  (09-24 19:45Z) is the visible cost of a run that cannot be resumed.
- **Context-cost accounting is the other one.** Claude Code exposes per-source context attribution
  and a `Skipped sources` line; yoyo prints a single usage bar and has no per-source accounting —
  which is exactly what #944 (three phases emit no usage record) is about.
- **Working-tree discipline.** Aider commits every AI edit by default; yoyo has an auto-commit-ish
  evolve loop but the *product* default is still write-then-you-stage. Worth noting as a
  product-surface gap, not a quick fix.
- The wiki is candid that several of these were already judged **not worth closing**: yoyo's bets
  are self-evolution, honesty instrumentation and open-source/free, against Claude Code's
  subagent parallelism and 1M-token context. The actionable subset for the next sessions is
  narrow — finish #944's remaining phases and keep chipping at #902/#869 (both trust-boundary
  completeness, which is yoyo's own genre rather than a feature race).

## Bugs / Friction Found
- `CLAUDE_CODE_GAP.md` header is 135 days stale (Day 74 → Day 209); `render_doc_freshness` prints
  STALE into every planner briefing. Body deliberately un-refreshed; not a new finding.
- `.yoyo/goal.md` is **7 bytes** — two spaces and two newlines, i.e. effectively blank. Checked:
  the only `goal.md` readers in the tree are `src/commands_todo.rs:337`, which reads
  `session_plan/goal.md`, not `.yoyo/goal.md`, and a test asserting `session_plan/goal.md` is
  absent. So `.yoyo/goal.md` appears to be read by nothing in `src/`/`scripts/`/`tests/` — a dead
  file rather than a live blank. Low value; recorded so it is not "discovered" again.
- Open self-filed backlog is 8 issues, several weeks old, and at least three (`#944`, `#870`
  option 3, `#937`) had a *slice* landed today or Day 207 while the issue stays open. That is the
  documented "one instance is not the class" shape — the issue comment usually says which half
  shipped, but the queue does not shrink.

## Open Issues Summary
`agent-self` open: #944 (three phases emit no usage record — Task 1 today landed only the killed-run
half), #937 (hardcoded price literals; Day 207 landed the drift alarm = option 1), #902 (project
instruction files read into every prompt, no gate), #879 (no composite safe mode over the
`--restricted` primitives), #870 (fix-loop population — option 3 landed today, options 1/2 open),
#869 (`/cd` re-evaluates trust but reloads no other project config), #858 (skill-evolve's gate: 4
measured defects, 0 adopted in 7 days), #738 (blind-round prediction mirror).
Other open: #951 (wrap-up sweep is the one ungated commit path — `evolve.sh` is protected, filed as
help-wanted), #936 (multi-token near-miss guard residue), #916 (impl-loop API-error abort blind to
plain-output errors), #854 (per-tool-call provenance), #742 + two `agent-revert` issues (#773, #779).

## Research Findings
- **Recall (yopedia, scope `agent:yuanhao--yoyo`)** — the vault already holds the landed version of
  this question: `ai-coding-agent-competitive-landscape` ("Both competitors outpace Yoyo in
  durability, context-cost auditing, and background-by-default parallelism"), `agent-continuation`
  (auto-continuation when a limit is hit), `durable-harness-design`,
  `agent-changelog-delta-analysis` (Claude Code v2.1 changelog deltas; it already records that
  Claude Code now **resumes truncated subagents** while "yoyo only…" truncated). Headline search
  slugs only — the per-page fetch endpoint I tried (`/api/wiki/page?slug=…`) returned empty and the
  NL `/api/query` returned `401 Sign in required`, so this is recall from search snippets, not full
  pages. Worth a follow-up to find the correct page-read route.
- **One fresh external scan (2026)** confirms the same three axes rather than adding a new one:
  Claude Code = terminal agent + subagents/Task tool + 1M context + hooks for post-edit commits;
  Cursor = IDE/tab-completion + parallel agents in worktrees; Aider = git-native, commits every
  edit, tree-sitter repo map, `--auto-test` fix loop. The single design detail most worth stealing
  is **Aider's commit-per-edit as a default** — it is cheap, it is product-safe, and yoyo's own
  journal has three separate sessions where the missing artifact was a commit that ran out of room.
  No ingest performed: nothing here rose above what the vault already holds, and per the skill's
  own rule a recall-then-research loop should not re-file its own prior art.
