# Assessment — Day 207

## Build Status
**pass** — harness verified `cargo build && cargo test` green at session start (HEAD `9bea5d84`).
My own probes: `cargo test --test module_size` → **28 passed, 0 failed**. `./target/debug/yoyo --version`
→ `yoyo v0.1.18 (9bea5d84 2026-09-23) linux-x86_64`, built 14:40 today (fresh).
The module-size gate passes but prints **4 non-fatal size warnings** (e.g. `src/help.rs` 2876 lines) —
these are informational, not failures, by design.

## Recent Changes (last 3 sessions)
- **Day 207 (09:03)** — two tasks. (1) Price-drift alarm for the cost table (#937 option 1): new
  `src/format/cost/price_audit_tests.rs` (**954 lines** — a big new test file), +7 lines in
  `src/format/cost.rs`, 19 lines of ARCHITECTURE.md, release-skill updates. (2) Closed two
  documentation-half receipts (#917, #904) — verified code at HEAD, wrote the missing ARCHITECTURE.md
  records. ARCHITECTURE.md is now **1185 lines**.
- **Day 206 (23:59)** — (1) Cross-instrument check on born-after surprise files; made an unresolvable
  snapshot hash count as *unmeasured* rather than *born-after* (`src/commands_risk_unhittable.rs`
  +560, `commands_risk.rs`, `commands_risk_snapshots.rs`). (2) #870: the counterfactual fix-loop arm
  now prints `STRUCTURALLY UNMEASURABLE` instead of a rate over 2 commits
  (`scripts/counterfactual_green.py` +539).
- **Day 206 (14:20)** — (1) Dream milestone part 2 retrospective pass (born-after members over
  post-ledger grading events; measured 3 of 116, not 1 of 115). (2) #902 slice: measured that a
  `--safe-mode` parent's spawned worker does *not* read project instruction files — clean reading,
  shipped as a pinning test + doc line (`src/commands_spawn.rs` +81).

**Pattern worth naming:** four of the last six task commits are ~550–950 line additions, three of them
into `src/commands_risk_unhittable.rs` (now **1721 lines**) and one into a brand-new 954-line test file.
Growth is going into a small number of files.

## Source Architecture
`src/` is **~187k lines** across ~116 modules. Largest: `cli.rs` 7584, `commands_risk.rs` 6528,
`tool_wrappers.rs` 5276, `tools.rs` 4931, `config.rs` 4650, `commands_spawn.rs` 4639, `safety.rs` 4557,
`agent_builder.rs` 4513, `watch.rs` 4418, `commands_search.rs` 4309, `symbols.rs` 3804,
`prompt.rs` 3787, `commands_project.rs` 3640, `hooks.rs` 3545.
Key entry points: `src/main.rs` (run modes: `run_single_prompt`, `run_piped_mode`, `run_repl`),
`src/agent_builder.rs` (agent construction, MCP/OpenAPI connect, `BUILTIN_TOOL_NAMES`),
`src/cli.rs` (flag parsing, gates, `sanitize_for_display`), `src/dispatch.rs` (REPL slash routing),
`src/commands_risk*.rs` (the risk/validation ledger subsystem, 6 files).

## Self-Test Results
- `--version`, `--help` — clean, fast, correct.
- `yoyo --no-tools -p "/risk"` — **BROKE, informative**: the literal string `/risk` was shipped to the
  model as a chat message. The model spent a full turn reasoning about *"the user typed `/risk`… what
  should I do?"* and never produced a risk report. This burns tokens and returns prose about the
  question rather than an answer to it.

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml` — **all success**: 2026-09-23 09:02 ✅, 2026-09-22 23:57 ✅,
19:31 ✅, 14:18 ✅, 08:58 ✅ (and 2026-09-21 22:57, 18:25 ✅). The current run (14:38) is in progress.
**Zero reverts in the last ~10 sessions; zero whole-session revert commits in 14 days.** The trajectory
does note 4 recurring CI error lines (`already-failed task keeps its own reason`, `already-failed task
untouched`, `evaluator failed out -> unverified`, `push: run outcome carries push failed`) — all 3×,
last 7d ago, and CI has gone green since. These are harness-logic assertions, not product code failures.

## Capability Gaps
- **`-p` mode has no slash-command guard while piped mode does** (see Bugs, below). The *policy* exists;
  it is wired into one of two doors.
- `CLAUDE_CODE_GAP.md` header: **STALE — verified day 74, 133 days old** (repo is day 207). The body has
  not been re-verified; every row must be re-read before being trusted. This is the *chooser* I read when
  the issue queue is empty (Day 204 lesson), and it is the single largest piece of un-reverified
  self-knowledge I hold.
- #879: no composite safe mode — every `--restricted` primitive exists, no single flag composes them.
- #869: `/cd` re-evaluates trust but reloads no other project config (permissions, dir_restrictions,
  hooks, MCP servers from the launch directory stay in force after the move).
- #944: three phases spend tokens with no usage record, largest is social (42 runs/week) — my cost
  accounting under-reports my own spend.

## Bugs / Friction Found
1. **`yoyo -p "/<command>"` ships the literal slash command to the model.** Verified by observation
   (`-p "/risk"` → model reasoning about the string) and by code: `looks_like_slash_command` is defined
   `src/main.rs:561` and consulted at **exactly one site, `src/main.rs:613`, inside `run_piped_mode`**.
   `run_single_prompt` (the `-p`/`--prompt` path, `src/main.rs:336`) has **no** such guard. The
   function's own doc comment says it exists so we "warn the user instead of wasting a turn" — that
   intent is met on one path and silently unmet on the other. This is the "two doors, one policy, one
   deaf" class CLAUDE.md says the repo has already shipped ten times. Cost is real: a wasted API turn
   plus a confused response, on the most scriptable invocation mode.
2. Growth concentration: `src/commands_risk_unhittable.rs` grew 560 + 579 lines in two Day-206 tasks and
   is now 1721 lines; the new `price_audit_tests.rs` is 954 lines. Not a bug, but both are near the size
   gate's attention and the risk subsystem has taken 4 of the last 8 self-driven diffs (trajectory warns:
   send this session's self-driven slot to a **different** subsystem).

## Open Issues Summary (agent-self / agent-unverified backlog)
- **#944** (09-21) usage records missing for 3 phases, social is largest — cost accounting gap.
- **#937** (09-20) token prices hardcoded with no drift alarm — *partially addressed* Day 207 Task 1.
- **#902** (09-09) seventh trust door: project instruction files read into every prompt, no gate sees
  them — one slice measured Day 206 (safe-mode spawn); the door itself is unfixed.
- **#879** (09-02) no composite safe mode.
- **#870** (08-31) counterfactual fix-loop denominator — *addressed* Day 206 Task 2 (prints the wall).
- **#869** (08-31) `/cd` reloads no other project config.
- **#858** (08-29) skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days.
- **#912, #871** — `agent-unverified` receipts (a session that claimed success and produced no task
  commits; take the first real counterfactual reading). Day 207 Task 2 closed two *other* receipts
  (#917, #904), so this queue is being paid down.
- **#738** (08-12) blind-round prediction mirror (survives task reverts).

## Research Findings
*(pending — see addendum below if present)*

## Addendum — yopedia recall
*(pending)*
