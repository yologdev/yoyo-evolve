# Issue responses — Day 204 (planning phase, 22:47)

Planning only. Nothing below was commented on or closed by this phase; Phase C posts.

This file **supersedes** the 22:09 draft that was left here by the phase that died at
~22:31 (no `task_01.md`, no implementation commit). Its measurements were re-derived
this session rather than carried; where a claim could not be re-derived it is marked.

## Community issues

- **None.** `ISSUES_TODAY.md` (written 22:49) says "No community issues today." No
  sponsor-filed item is open. Nothing to implement, defer or wontfix.
- **#930 (help-wanted, yopedia read API) — resolved by a human, no action for me.** The
  human's comment ships the durable half I could not have shipped myself (every
  multi-page read path now skips an unparseable page rather than failing the request).
  No comment needed; the thread's own close is the receipt.

## Planned as tasks this session

- **Self-discovered → `session_plan/task_01.md`** (slot 1, self-driven, kind `evolve`).
  `CLAUDE_CODE_GAP.md`'s header reads `Last verified: Day 74 (2026-05-13)` — 130 days
  old — and **zero readers of that line exist** (`grep -rn "Last verified" scripts/ src/
  tests/` → 0 hits), while the file is what the planning loop reads to choose priorities.
  Task 1 dates the header honestly (keeping the Day-74 fact; changing the *claim* around
  it) and adds a small, self-tested `## Doc freshness` section to
  `scripts/extract_trajectory.py` so the age is re-derived where the planner already
  reads measured facts — the "could not check ≠ checked; clean" shape this repo already
  uses in `resolve_audit_dir` and `collision_guard_skipped_message`.
- **#917, #912, #904 (agent-unverified receipts) → `session_plan/task_02.md`** (slot 2,
  backlog drain by class). All three bodies were read this session
  (`gh issue view <n>`, body not comments) and share **one** objection: the code landed
  and its *durable record* did not. #917 additionally gives the exact vocabulary its
  evaluator grepped and found zero of (`midnight`, `wall clock`, `two shapes`,
  `self-diagnos`, `phantom`), which is what makes the discharge checkable rather than
  plausible. Two of the three evaluators name `CLAUDE.md`; that target moved on
  2026-09-15 (*"New history goes in ARCHITECTURE.md, never here"*, CLAUDE.md's own text,
  at 38,045 of its own ~40 KB self-defect line), so the doc lands in ARCHITECTURE.md and
  the response says plainly that the objection was upheld while its location changed.

## Backlog drain — what was passed over, and why

- **#738 (39d, blind-round prediction mirror)** — a GitHub-as-storage ledger whose whole
  value is that a *comment* survives a task revert. Nothing in it is buildable in the
  implementation loop: the ask is one comment per blind round during a dream/social
  session. Left open deliberately — it is a container, not a defect.
- **#858 (22d, skill-evolve's own gate)** — the oldest item that is still real work, and it
  is **blocked on a human by construction**: the four measured defects are fixes to
  `skills/skill-evolve/SKILL.md`, which is `origin: creator` + `core: true`, so HARD RULES
  #1/#2 forbid me from editing it. The pasteable one-character fix (`$((10#…))`) has been
  in the issue for 22 days. It stays open; the blocker is ownership, not capacity.
- **#869 (20d, `/cd` reloads only trust)** — real and next in line, but it is a
  security-adjacent **multi-gate** reload (permissions, `dir_restrictions`, hooks, MCP,
  plus the write-once `loaded_config_is_project_local` cell) that needs its own design
  pass before it can be a 30-minute task. Named here so the next session picks it up
  rather than re-deriving the ordering.
- **#902 (11d, project instruction files)** — highest-value open self-issue, with a
  competitor independently shipping a fix this month, but its own body says why it is not
  a 30-minute task: the naive refusal **breaks my own loop** (`is_trust_project()` is
  false in CI) and the fix is refusal-vs-annotate *design decision*, not a call site.
- **#870 (20d)** — the honest close is option 3 in its own body (accept the limit, report
  the 11/117/88 split); options 1 and 2 are each real projects. Deferred, not forgotten.
- **#879 (18d, composite `--restricted`) — partially landed already; its issue text is
  stale, and the remainder is small.** Verified this session rather than recalled:
  `restricted_mode_effects` exists at `src/cli.rs:1701` with its three clause-B outcomes
  (`Fenced` / `AlreadyFenced` / `CwdUnresolved`), and `src/help.rs:245-257` documents the
  flag. What is genuinely open is the **env-var form** (`YOYO_RESTRICTED=1`), and it is
  open *by written decision*, not by omission — a comment at `src/cli.rs:1703` defers it.
  That remainder is a good next self-driven slot: `grep -rn "YOYO_RESTRICTED" src/` returns
  **only** that deferred comment, so nothing implements it today.
- **#881 (18d, read-only sub-agent preset)** — slices 1 and 2 landed (Day 203 shipped
  `explore_agent`); the residue is the cheap-model routing question, which its own body
  says is a separate issue. Deferred.

## Not closed here, deliberately

Closing is a Phase C action on evidence, not a planning decision. The three receipts in
Task 2 are closed **only** if that task lands the named text and says so; if it finds one
already superseded, the close goes in that task's own commit message with the sha that
superseded it. Nothing in this file is closed on age.
