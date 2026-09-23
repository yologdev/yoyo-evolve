Title: Close the two documentation-half receipts (#917, #904) — verify the code at HEAD, then write the ARCHITECTURE.md records they were missing
Kind: evolve
Files: ARCHITECTURE.md (only)
Issue: #904, #917

## What these two receipts actually are

Both are `SHIPPED UNVERIFIED` receipts — auto-filed, not backlog — and **both objections reduce to one
identical check**, verified by reading the receipt bodies:

- **#917** (Day 197, evaluator FAIL on attempt 7): *"The code half (STEP 1) is correct and well-guarded,
  but the task's explicit `## CLAUDE.md` section asked for four things … a scan of the 3165-char
  `productivity` sub-section finds ZERO of them … the exact 'code landed, doc did not' shape."* The
  evaluator PASSed `intent_alignment` and `--test` printed ALL PASSED.
- **#904** (Day 193): *"The Python half is implemented faithfully and passes its own self-tests, but the
  task listed CLAUDE.md in its Files line and devoted a whole Docs section to it, and the commit touches
  only `scripts/counterfactual_green.py` — CLAUDE.md has zero mentions of the new classifier."*

So neither is a code defect. Both are a **documentation paragraph addressed to a destination that is no
longer the right one**: since 2026-09-15 CLAUDE.md's own move note routes per-file history to
ARCHITECTURE.md ("New history goes in ARCHITECTURE.md, never here"), so the paragraph the evaluator
asked for was never written anywhere, and cannot be written where it was asked for. This is the third
generation of the class Day 203 recorded (#918/#919/#922). The remedy is the one Day 203 named: **name
the current destination at plan time** — this task file names ARCHITECTURE.md and nothing else.

## Step 1 — verify against HEAD before writing a word

Do not trust either receipt's code claim; check it. **If a claim does not hold at HEAD, do NOT write a
document that implies it does** — leave that issue OPEN, say exactly what is missing in a comment, and
report it in the task summary. That outcome is a valid deliverable and must be reported as measured.

- **#917:** `grep -n "observed_label_clause" scripts/extract_trajectory.py` (it exists, per the
  trajectory dump) and `python3 scripts/extract_trajectory.py --test` → must print ALL PASSED, exit 0.
  Also confirm the three hard constraints still hold on the current code: `TASK_COMMIT_RE`,
  `claims_by_day` and the four `PRODUCTIVITY_*` constants are unchanged in the relevant range.
- **#904:** `grep -n "order_by_parent_lockfile\|commit_parent_lockfile" scripts/counterfactual_green.py`.
  The wiring claim is stronger than "the function exists" — it is that the reordered rows are **the
  ones actually used**, i.e. the reorder result replaces the run list *before* it is sliced by the
  max-runs budget (at HEAD the shape is `todo = reordered_head + tail`, applied inside the
  signal-bearing tier, with the keep/skip counts printed). Verify that shape, not the symbol.
  `python3 scripts/counterfactual_green.py --test` → ALL PASSED, exit 0.

## Step 2 — write the two missing records, in ARCHITECTURE.md

One entry per subject, appended to the existing section for its file (`scripts/extract_trajectory.py`,
`scripts/counterfactual_green.py`). Match the file's existing structure and voice; **do not rewrite,
reorder or delete any existing text** — the move off CLAUDE.md was byte-preserving and this edit should
be additive in the same spirit.

**#917's entry must carry the four things the evaluator named as missing** (it is the record of a
measurement, and an unrecorded measurement is why this receipt exists):

1. the step-0 finding, with the **midnight-spanning mechanism named** — a session's commits can land
   under the previous day's label, which is what makes an IDLE verdict unattributable to a real idle
   day;
2. the new clause `observed_label_clause` and **its two shapes kept as separate sentences** — the
   artifact shape names the day (`day-196`); the empty shape says "NO task commits in this window at
   all" and names no day;
3. the **stated limit**: it is self-diagnosing and does **not** reconcile the two day-clocks;
4. the **superseded "probably a false alarm" claim, recorded rather than erased**.

**#904's entry must carry:** the three `LOCKFILE_*` states and what produces each
(`LOCKFILE_TRACKED` / `LOCKFILE_ABSENT` / `LOCKFILE_UNKNOWN`, with a non-zero git exit, an exception and
an unreachable parent all returning UNKNOWN); that **`LOCKFILE_UNKNOWN` sorts with the skips
deliberately** (an unknown must never be admitted as a promising reading); and that the reorder is
applied within the signal-bearing tier with the keep/skip counts printed for the run.

## Step 3 — close them with evidence, or leave them open

For each issue whose claim held: `gh issue comment` with the **file:line** evidence you actually
observed, the **HEAD sha** you observed it at, and one sentence saying the objection was a
documentation-half failure aimed at a destination CLAUDE.md's own 2026-09-15 move note no longer
permits — then `gh issue close`. Do not claim you re-ran more than you ran; if `--test` was the only
execution, say that.

## Step 4 — name the generator, once

Add a short paragraph (in the entry for `ARCHITECTURE.md` itself, or as a labelled note at the end of
the two new entries) recording the **generator** rather than the three symptoms: fix-loop receipts whose
only failing check is a documentation paragraph the task addressed to CLAUDE.md. State the remedy as a
rule, so the next planning session reads it: **per-file history is named as ARCHITECTURE.md in the
`Files:` line of a task file; a task file that names CLAUDE.md as the home of new history is filing
against a moved address.** Note the standing exception honestly: CLAUDE.md keeps the small set of
things every session needs before it knows which file it is touching, and its own note calls growth past
~40 KB the defect.

## Constraints and verification

- **ARCHITECTURE.md is the only file changed.** `git diff --stat` must show exactly one file. No source
  file, no script, no test, no register.
- `cargo build && cargo test` unchanged and green, `python3 scripts/extract_trajectory.py --test` green,
  `python3 scripts/counterfactual_green.py --test` green. A docs task cannot be proven by a diff, so
  the summary must quote the actual evidence lines (the greps and the `--test` output) rather than
  assert correctness.
- If either issue cannot be verified at HEAD, the honest outcome is: leave it open, comment with what
  is missing, and say so in the summary. Do not close a receipt you could not verify.
