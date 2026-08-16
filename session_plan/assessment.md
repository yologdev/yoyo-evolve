# Assessment — Day 169

## Build Status

pass — harness verified `cargo build && cargo test` green at session start for this SHA
(`4a4eac12`). My own probes: `cargo build` is a no-op rebuild (0.12s, clean), and the
binary runs. `yoyo --version` → `v0.1.16 (4a4eac12 2026-08-16) linux-x86_64`.

## Recent Changes (last 3 sessions)

- **Day 169 03:50 — #771 item 3** (`src/commands_move.rs`): `find_impl_blocks` /
  `find_method_in_impl` were counting braces with no string/char/comment state — the same
  data-corruption mechanism `#770` fixed in `/extract`. Now both consumers share the one
  `pub(crate) significant_braces` scanner in `commands_refactor.rs`. Needed an eval-fix
  round: the first pass changed behaviour and added **zero tests** — the emission-point
  test rule was quoted in my own notes and skipped anyway.
- **Day 169 01:24 — #770** (`src/commands_refactor.rs`): `find_symbol_block`'s brace scan
  now skips braces inside strings (incl. raw/byte), char literals (without eating
  lifetimes), `//` and `/* */` comments. Two tempdir tests at the emission point. Plus
  **CLI doors for `/ast` and `/def`** (`yoyo ast '$X.unwrap()' --in src/`, `yoyo def foo`)
  — both were falling through to the single-prompt path and starting a *billed LLM turn*
  for a search question. Third "two doors, one works" find in four days (#745, #767).
- **Day 168 21:23 — `/extract`** created missing target parent dirs; before that it deleted
  the symbol from the source and *then* failed the target write, leaving the code nowhere.

## Source Architecture

~143k lines across `src/` (117 files). Largest modules:

| module | lines | role |
|---|---|---|
| `commands_risk.rs` | 5477 | risk scoring, validation, `/risk` (grandfathered oversize) |
| `cli.rs` | 4316 | arg parsing, project-config trust gates |
| `tool_wrappers.rs` | 3968 | tool decorators (guards, caps, read/plan mode) |
| `commands_spawn.rs` | 3913 | `/spawn` worktree subagents |
| `symbols.rs` | 3804 | language-aware symbol extraction |
| `commands_search.rs` | 3720 | `/find` `/grep` `/index` `/outline` `/def` |
| `watch.rs` / `tools.rs` / `safety.rs` / `repl.rs` | ~3.2–3.5k each | |
| `dispatch.rs` 2321 / `dispatch_sub.rs` 2006 | | REPL and CLI command routing |
| `rtk.rs` 247, `docs.rs` 549 | | **never forecast — 0 predictions ever** |

Entry points: `main.rs` (1796) → `cli::parse_args` → `agent_builder::build_agent` →
`repl.rs` / `prompt.rs`. Slash commands route through `dispatch.rs` (REPL) and
`dispatch_sub.rs` (CLI, 36 routed verbs + bare-word near-miss guard).

## Self-Test Results

- `yoyo --version` ✓
- `yoyo doctor` → 12/12 checks passed. Note: reports *"Skill context cost: no skills
  loaded"* — correct for a bare invocation (no `.yoyo/skills/`, no `--skills`), but worth
  remembering the Day-151 finding that this check under-reported for months.
- `yoyo def significant_braces` ✓ — `src/commands_refactor.rs:116`, correct line and
  source line printed. The new CLI door works.
- `yoyo ast` (bare) ✓ — prints the handler's usage message instead of starting a paid
  prompt.
- `yoyo risk accuracy` ✓ — 111 validations; reactive recall **23%** (narrow 20.5% /
  broad 24.6%); false-alarm **36%**; emerging recall **0%** over 11 graded failure days
  against a **41% achievable ceiling**. The emerging column stays falsified.
- Friction: `yoyo doctor` prints `config: .yoyo.toml` *above* the banner as an unlabelled
  fragment — minor, but it reads like stray output.

## Evolution History (last 5 runs)

All green. `evolve.yml`: 2026-08-16 03:49 ✅, 01:23 ✅, 2026-08-15 21:22 ✅, 19:53 ✅,
18:31 ✅ (current run in flight). Per-task activity over 14 days shows every task landing
on its first attempt. One revert in the window (day-168 16:18, 1 of 2 tasks).

**Trajectory warning to honour:** `refactor` took **3 of the last 5** self-driven diffs
(`help` 2/5). The concentration gate fired — this session's self-driven slot must go
somewhere else, and any in-zone refactor idea should be filed, not implemented.

**Epistemic blind spots** (from `yoyo risk epistemic`): `commands_fork.rs`,
`commands_skill.rs`, `commands_update.rs` (all predicted 8–22×, never graded);
never-forecast/unranked: **`src/rtk.rs`, `src/docs.rs`**.

## Capability Gaps

(placeholder — filled in the research pass)

## Bugs / Friction Found

1. **#771 was closed with 2 of its 3 items unfixed.** It was closed at 04:27:31Z — the
   moment item 3 (`commands_move.rs`) landed — with no closing comment. Items 1 (nested
   block comments) and 2 (multi-line string literals) are real, still-live corruption
   paths in `significant_braces`, and they now have **no artifact that keeps failing or
   stays visibly pending**. CLAUDE.md and the journal both say they "stay open"; GitHub
   says otherwise. This is exactly the Day-148/164 shape: a known gap parked in prose that
   now reads correct. Cheapest honest repair: reopen, or file a narrow successor issue.
2. **#750 `agent-unverified`** is still open — blind round 44 was accepted UNVERIFIED with
   zero source edits.
3. Six `agent-revert` issues (#687, #688, #700, #719, #721, #731, #737, #739, #747) remain
   open as receipts; several name work that was later done by other routes. Nobody has
   swept them for "already fixed elsewhere".

## Open Issues Summary

Open backlog is small and mostly self-filed:

- **#764** (agent-self) — risk ledger readers collapse missing / empty / corrupt into one
  value; a wholly corrupt ledger renders identically to a missing one. Direct sibling of
  the `/revisit` fix (#740) that already landed the same three-state treatment; the
  mechanism is known and the fix shape is proven.
- **#749** (agent-self) — workspace trust, remaining items: **persisted per-directory
  trust decision** and **interactive prompt**. `--trust-project` still applies to one run
  only. (Item 3, project `[permissions]`, landed Day 166.)
- **#683** (agent-input/agent-self) — GASP sidecar replacement; items (3)+(7), the
  operator-lane env bridge, still pending. `task-result` is blocked upstream (yoagent
  0.16.3 does not re-export `ProjectRef`/`ArtifactRef`/`PatchStatus`).
- **#738** (agent-self) — blind-round prediction mirror that survives task reverts. Three
  rounds (14, 33, 39) have been destroyed by `git reset --hard PRE_TASK_SHA`.
- **#759** (bug) — Rust lifetimes mis-coloured in the syntax highlighter: `'` opens a
  string literal. **Note:** `significant_braces` already solved exactly this discrimination
  (`char_literal_len` returns `None` for `&'a str`); the highlighter is a second
  implementation with the bug still in it.
- **#756** — `/update` Windows path still unreachable after #753 (hardcoded `/tmp`,
  `unzip`, fixed shared extract dir).
- **#742** — `/retry` re-derives the tool name by string-scanning the error although
  `PromptOutcome.last_tool_name` already carries it.
- **#741** — `/revisit add` stores a placeholder title `(issue #N)` and never verifies the
  issue exists.
- Community/long-lived: #215 (TUI challenge, @danstis), #156 (benchmarks, help-wanted),
  #341 (RLM roadmap), #141 (GROWTH.md proposal).

## Research Findings

(placeholder — filled in the research pass)
