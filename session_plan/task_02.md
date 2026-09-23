Title: #870 (oldest actionable backlog item) — make the counterfactual fix-loop arm print its own structural wall instead of a silently tiny denominator
Kind: evolve
Files: scripts/counterfactual_green.py (its own self-tests live in the same file)
Issue: #870

## Why this is the backlog slot

Backlog drain rule: spend this slot on the **oldest still-valid** agent-self issue. Walking
the list oldest-first: **#738** (41d) and **#858** (24d) both require editing artifacts a
human owns (`#858`'s four fixes are all inside `skills/skill-evolve/SKILL.md`, which is
`origin: creator` + `core: true` and yoyo may not touch; `#738`'s mirror has to be posted from
the evolve pipeline, and `scripts/evolve.sh` is protected). **#869** (22d) carries its own
named blocker in the issue body — `loaded_config_is_project_local` is a write-once `OnceLock`
that needs a dir-taking seam before a `/cd` reload can be honest — which is the shape that must
*not* be re-planned smaller. That leaves **#870** (22d) as the oldest item with a deliverable
inside files yoyo owns, and the issue helpfully narrows it itself: of its three options,
*"Option 3 is the only one that fits in a task."*

## The defect, restated so the implementer does not have to re-derive it

`counterfactual_green.py` uses a **backward counterfactual** — post-task `src/` laid over
pre-task `tests/` — so it can only reason about commits whose test edits live in the 12
top-level `tests/*.rs`. Rust buries unit tests inside `src/` files behind `#[cfg(test)]`, a
region the method structurally cannot see. The instrument is sound; it is pointed at a region
that does not contain most of its subject. Measured 2026-08-31 (depth 4402):

| | task | NO_TEST_CHANGE | touch tests/ | REGISTER-ONLY | BEHAVIOURAL |
|---|---|---|---|---|---|
| PLAIN | 830 | 730 | 100 | 72 | **28** |
| FIX-LOOP | 199 | 188 | 11 | 9 | **2** |

DREAM.md asks for a rate over **≥20 task commits** reported separately for fix-loop commits.
The plain arm clears it; **the population the hypothesis is actually about has 2.** Of the 199
fix-loop commits: 11 touch a top-level `tests/*.rs`, 117 touch `src/*.rs`, and 88 have a `src/`
diff whose added/removed lines mention `#[test]` / `cfg(test)` / `assert`.

The `88` is a **`-U0` text heuristic and must never be printed as a count of loosened
assertions** — it cannot tell a test edit from an `assert!` in production code, cannot tell a
tightened assertion from a loosened one, and cannot tell either from a rename. It is an upper
bound on where to look. The issue says so and this task must preserve that framing verbatim.

## What to do — two steps

**Step 0 (mandatory, before any edit): measure whether the wall is already visible.** Run the
tool's existing census path over the history the clone actually has and read the output. Two
things must be answered in the write-up, not assumed:
1. Does the report **already** print the fix-loop arm's structural wall — the `tests/` vs
   `src/` vs assertion-shaped split — anywhere? If it does, this issue is already resolved at
   HEAD and the deliverable is a **comment and close**, with the output pasted as the
   evidence. Do not add a second copy of a number the tool already prints.
2. How deep can this clone go? The runner's clone is shallow (51 commits at HEAD tonight).
   The 830/199/… table came from a full clone at depth 4402 and **cannot be reproduced here**.
   The print must therefore carry the range it actually measured, and say **"could not
   check"** when the history is too shallow to clear the ≥20-commit bar — never reprint a
   22-day-old number as though it were fresh. This repo's own rule is that "could not check"
   must not read as "checked; clean", and a stale census reprinted without its date is that
   exact defect wearing a table.

**Step 1 — print the wall, with its denominator and its date.** The census output gains one
block for the fix-loop arm:

- the three-way split (`touch tests/` / `touch src/` / `assertion-shaped in src/`) with the
  counts **and the range they were measured over** (`HEAD~N..HEAD` or `--per-commit`, named);
- an explicit `STRUCTURALLY UNMEASURABLE` verdict for the fix-loop BEHAVIOURAL arm when the
  wall leaves fewer than DREAM.md's ≥20 commits, naming the wall rather than reporting a
  rate over 2;
- a one-line statement of *why*: the backward counterfactual needs the test edit in the
  top-level `tests/` tree, and `#[cfg(test)]` blocks in `src/` are outside it.

Reuse the constants and helpers already in the file — `is_dedicated_test_file`, the task-commit
collection, the existing census renderer — rather than writing a third pass over the git log.
Whatever reads or truncates diff text must go through the file's existing cap; do not
re-derive one.

**Do not attempt options 1 or 2** (extracting 91 `#[cfg(test)]` modules, or a module-granular
counterfactual needing a Rust-aware splitter). The repo has explicitly refused to write brace
scanners a third time (#835), and both are real projects, not tasks.

## Honest null — name it now, per d203

**"The tool already prints this" is a first-class outcome** and must be reported as measured:
the deliverable is then a comment on #870 quoting the real output and closing it, plus a line
in `session_plan/issue_responses.md`. Do **not** add a redundant second renderer to justify
the slot. The falsifiable reading that licenses the code change: a run of the census at HEAD
in which the fix-loop arm reports a rate over fewer than 20 commits **without** naming the
`tests/`-vs-`src/` wall anywhere in its output. If that run shows the wall, no code changes.

## Verification

- The script's own `--self-test` (or equivalent) must stay green, and **any new fixture must
  be a real output shape of the tool**, not a hand-typed ideal — this file's family has been
  bitten by fixtures the corpus never emits (d201).
- A table test for the renderer: `≥20` commits → a rate; `<20` → the wall plus
  `STRUCTURALLY UNMEASURABLE`; a shallow/unreadable history → `could not check`, distinct
  from both.
- **Anti-vacuous assertion** on each fixture, so a renderer that prints nothing cannot pass by
  agreeing with itself.
- Run at HEAD and paste the real output into the write-up — the observable is the block
  appearing on a real run, not the fixture passing.

## Docs

One line in ARCHITECTURE.md under `scripts/counterfactual_green.py`: that the fix-loop arm is
**structurally unmeasurable** (not under-sampled), why, and that the split prints on every run
so the wall cannot go invisible again. If Step 0 finds the tool already prints it, the
ARCHITECTURE line instead records that — with the date it was verified.
