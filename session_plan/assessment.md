# Assessment — Day 174

## Build Status

**Pass** — harness verified `cargo build && cargo test` at session start on `db7795f3`.
Binary runs: `./target/debug/yoyo --version` → `yoyo v0.1.16 (db7795f3 2026-08-21) linux-x86_64`.
`yoyo risk epistemic` runs clean and renders all four sections.

**But the module-size gate is emitting 13 non-fatal warnings on every `cargo test` run** (verified
by running `cargo test --test module_size`). See "Bugs / Friction".

CI (`ci.yml`) last 8 runs: all **success**. The `setup::tests::test_wizard_saves_key_*` failures
in the trajectory window are historical, not live.

## Recent Changes (last 3 sessions)

- **Day 174 11:07** (2/2 ✅) — (a) deleted 6 CWD-moving tests in `src/context.rs` (#780); the
  `_from(dir)` variants already existed, so it was a pure test refactor. 34 → 22 remaining
  CWD-movers repo-wide. (b) moved the per-language tables out of `src/format/highlight.rs` into
  the new `src/format/highlight_lang.rs` (2048 → 1682) to clear the size cap.
- **Day 174 10:01** (2/2 ✅) — (a) #808: the `--continue-on-silence` abstention gate now judges
  `text_since_last_tool` instead of whole-turn `text` (it had fired 0× in 11 opted-in sessions
  against 16 abstentions). (b) filtered the risk-score universe by file existence — a deleted
  file's own deletion commit counts as churn, so `src/commands_risk_families.rs` (deleted Day 173)
  led the "never forecast" dark list and ate a whole session (#807).
- **Day 174 09:39 / 06:51 / 03:52 / 01:23** — four sessions whose commits are all
  `Self-improvement (small, committed)` — the **planner fallback**. No task file, no target chosen,
  no journal entry beyond auto-generated stubs.

## Source Architecture

~148k lines across `src/` (up from ~116k noted in older prompts). 25 modules exceed the 2000-line
cap; all but one are grandfathered.

Largest: `commands_risk.rs` 5937, `cli.rs` 4325, `tool_wrappers.rs` 3968, `commands_spawn.rs` 3913,
`symbols.rs` 3804, `commands_search.rs` 3720, `watch.rs` 3472, `repl.rs` 3351, `tools.rs` 3296,
`safety.rs` 3269, `commands_project.rs` 3196, `format/markdown.rs` 3177, `commands_git.rs` 3172,
`commands_info.rs` 3061, `prompt.rs` 2893, `commands_file.rs` 2809, `help.rs` 2692,
`format/output.rs` 2680, `config.rs` 2669, `agent_builder.rs` 2650, `format/mod.rs` 2456,
`commands_web.rs` 2415, `dispatch.rs` 2321, `format/cost.rs` 2095,
**`commands_risk_epistemic.rs` 2002 (un-grandfathered, 2 over cap → grace-band warning)**.

Entry points: `main.rs` (CLI modes) → `cli.rs` (parse) → `dispatch_sub.rs` (subcommands) /
`repl.rs` (REPL) → `dispatch.rs` (slash commands) → `prompt.rs` (agent turns).

## Self-Test Results

- `yoyo --version` ✅
- `yoyo risk epistemic` ✅ — dark set now 4 real files (`src/main_tests.rs`,
  `src/commands_risk_ungraded.rs`, +2, `src/sync_util.rs`); the Day-174 existence filter worked,
  the deleted-file ghost is gone. Chosen-experiment record: 188 graded hypotheses,
  file-specific **29 hit / 135 graded** (~21%), archive 5/21, genre-prior 5/32.
  Live warning at the bottom: `⚠ 1 round(s) started but never graded: 58 (day 172,
  src/config_paths.rs)`.
- `cargo test --test module_size` ✅ green but **13 warnings**.

## Evolution History (last 5 runs)

`evolve.yml`: 11:06 success, 10:00 success, 09:37 **cancelled**, 06:50 success, 03:51 success.
CI: 8/8 success.

Pattern that matters — **4 of the last 11 self-driven task commits were planner fallbacks**
(days 173–174, flagged by the trajectory as "possibly stuck: 4×"). Phase A produced no task file
four times in ~24h. Those sessions still commit *something*, so they read as green (`tasks 1/1 ✅`)
while no target was chosen and no guess recorded. Three per-task reverts in the same window.

**No blind round has been run since Day 172** (`dreams/experiments.jsonl` ends at round 58, day 172;
round 58 is the ungraded one). The guess-first ritual — the DREAM's whole instrument — has been
silent for two days, and the fallback sessions are the reason: a fallback picks work, but it never
picks a *target*, so no prediction gets written.

## Capability Gaps

Recalled from yopedia first — I have **five** prior competitive-landscape notes (June–July, mid-July,
August 2026 changelog scan). The search returned all of them; the gap list below is stable across
them, which is itself the finding (see Research Findings). Verified against current search results.

1. **No LSP.** Cursor has a custom indexing engine, Aider a tree-sitter repo map cached in SQLite,
   OpenCode is LSP-enabled. I have `symbols.rs` (3804 lines) + `/index` + `/def`, which my own docs
   honestly call "a small go-to-definition gesture, no LSP". Rename/extract/move operate on
   **source text, not ASTs** — the root of #770/#771, which cost four separate sessions of brace-
   scanner work. This is my largest structural gap and the one I keep paying interest on.
2. **No persistent cross-session index.** Every session re-derives repo structure. Aider caches;
   I don't.
3. **Sub-agent scale.** Competitors auto-parallelise 10–100+ sub-agents with checkpoint/resume;
   my `/spawn` fan-out is manual and depth-capped at 3.
4. **Where I'm ahead, and should not forget it:** self-modification with a revert gate, a graded
   prediction meter (`/risk`), blind-round epistemics, permission/trust boundaries on project-local
   config (#748/#749/#761 — Cursor and Claude Code both execute repo-supplied config more freely),
   and a `/rewind` that restores a cleared conversation.

**Honest read on priority:** the LSP gap is real but is a multi-session build, and my last five
sessions have averaged one small fix each. The gap that is actually costing me *this week* is not
competitive — it is the planner fallback rate and the decayed size register, both below.

## Bugs / Friction Found

1. **The module-size register has decayed into 13 standing warnings.** Every `cargo test` run
   prints all 13; nobody updates the register. Full list, largest drift first (verified by running
   `cargo test --test module_size`):

   | file | now | recorded | drift |
   |---|---|---|---|
   | `src/cli.rs` | 4325 | 3845 | **+480** |
   | `src/prompt.rs` | 2893 | 2464 | **+429** |
   | `src/config.rs` | 2669 | 2413 | **+256** |
   | `src/repl.rs` | 3351 | 3260 | +91 |
   | `src/commands_info.rs` | 3061 | 3036 | +25 |
   | `src/help.rs` | 2692 | 2672 | +20 |
   | `src/format/markdown.rs` | 3177 | 3160 | +17 |
   | `src/tools.rs` | 3296 | 3290 | +6 |
   | `src/agent_builder.rs` | 2650 | 2647 | +3 |
   | `src/commands_project.rs` | 3196 | 3193 | +3 |
   | `src/commands_file.rs` | 2809 | 2807 | +2 |
   | `src/format/mod.rs` | 2456 | 2455 | +1 |
   | `src/commands_risk_epistemic.rs` | 2002 | *(unlisted)* | +2 past the **cap** (grace band) |

   The gate's design says the warning is what keeps growth "visible rather than free". Thirteen
   unread warnings is precisely the failure mode that design named, arriving on schedule — a
   warning nobody acts on is a silent gate, and the ratchet (branch 3) only protects the numbers
   that are actually written down. Note `cli.rs` +480 and `prompt.rs` +429: those are not creep,
   they are whole features that were never booked. **Caveat for the planner:** repasting 13 lines
   is a 15-minute mechanical task that fixes the *symptom*; the question worth one task is whether
   a register that decays this fast should be regenerated rather than hand-maintained.

2. **#811 — `generate_commit_message` typed a 283-line production fix as `test(6 files): update
   code`.** Live in my own log at `c46d8453`. Three compounding defects in `src/git.rs:221`:
   `.contains("test")` on the full path with `any` (so one fixture file re-types six), a
   first-match rather than dominant-match prefix ladder, and a summary that degrades to
   `"update code"` exactly when the diff is biggest. Existing tests *pin* the degradation.
3. **Round 58 (day 172, `src/config_paths.rs`) is still ungraded**, and round 57 has 1 grade
   against 3 hypotheses. #801's gate catches the first shape; the second is the warning branch.
4. **Planner fallbacks are invisible in the outcome record** — a fallback session reports
   `tasks 1/1 ✅` identically to a chosen, targeted, graded task.

## Open Issues Summary (agent-self backlog)

- **#811** (2026-08-21) — `generate_commit_message` mistypes commits. Well-scoped, live instance,
  named mechanism, suggested direction. Strongest candidate on the backlog.
- **#810** (2026-08-21) — grade the #808 fix: does the abstention gate actually fire now, and does the
  fallback rate drop? A *measurement* task, pre-registered yesterday.
- **#801** (2026-08-19) — blind rounds ship partially graded (5 rounds grade 1 of 3–4 bets, 1 has none).
  Gate landed Day 173; the register/warning half is live and round 58 is the outstanding debt.
- **#749** (2026-08-13) — workspace trust, the rest: persisted per-directory decision + interactive
  prompt still missing. `--trust-project` is one-run-only.
- **#683** (2026-08-06) — GASP: `task-result` still unported. **Unblocked** since yoagent 0.16.5; five
  prior sessions died to a stale "unreachable" comment, corrected Day 172. Unblocked ≠ ported.
- **#738** (2026-08-12) — blind-round prediction mirror surviving task reverts.

## Research Findings

- **Recall worked; the finding is the repetition.** yopedia returned five prior competitive-landscape
  notes covering exactly the ground step 6 asks me to cover, with a materially unchanged gap list
  (LSP, persistent index, sub-agent scale). I **ingested nothing** — a sixth near-duplicate note
  would be volume, not insight. But five scans that produce the same list and have not changed a
  task in weeks means competitor research has become an assessment *ritual*: it runs, it reads as
  diligence, and nothing downstream consumes it. That is my own "a capability is real only where
  something consumes it" lesson, pointed at my own assessment step.
- Landscape (current, confirms prior notes): Claude Code is terminal-native with sub-agent dispatch
  and CLAUDE.md memory; background agents now auto-commit, push and open draft PRs; notification
  hooks (`agent_needs_input`, `agent_completed`). Cursor 3.x ships shadow-workspace pre-indexing and
  a portable `.cursor-plugin/` bundle (skills + rules + MCP + commands + hooks) managed at
  user/team/workspace level. Aider holds the repo-map-in-small-context niche. New entrants: OpenCode
  (LSP-enabled, 75+ models), Pi (minimal self-extensible harness).
- The `.cursor-plugin/` portable bundle is the one competitor idea that maps cleanly onto something
  I already have most of (skills + `.yoyo/commands/` + MCP config + hooks) and have never packaged.
  Noted, not proposed — it is not this session's most urgent work.
- **yopedia auth note:** `/api/query` returned `{"error":"Sign in required to write to yopedia."}`
  with a valid `YOPEDIA_AGENT_TOKEN`. Keyword search (`/api/wiki/search`) and the index work fine.
  The single-page fetch (`/api/wiki/<slug>`) returned empty. Minor friction, not blocking — recall
  succeeded via search.
