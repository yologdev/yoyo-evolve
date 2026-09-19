Title: Give the convention census an honest DENOMINATOR for the register pair (DREAM milestone: the split-guard row must be readable, not just counted)
Kind: evolve
Files: scripts/check_assertion_weakening.py (single file; its self-tests live inside it as `run_self_tests`)
Issue: none (DREAM.md milestone, Day 201/203 — "keep counting that")

## Why this, and why it is not busywork

DREAM.md's current milestone is explicitly a *counting* obligation, and the last
cycle ended by naming the reason the count is unreadable:

> "this fix's reach is proven by a fixture and **not yet by a single real hunk**, and
> the first honest signal that it matters will be a `WEAKENED` hunk that removes a
> split guard. Next milestone, narrowed accordingly: keep counting that."

and the Day-201 lesson states the required form of the number:

> "measure the base rate, then restrict it by every condition on the path from the
> entry point to the changed line, and quote it as a fraction over THAT set —
> 'of the hunks the WEAKENED gate admits, N carry this shape.'"

Today the census prints the register pair as two bare counts (`register-lines-only`,
`register-paid-to-empty`). **A bare 0 is exactly the artefact this whole arc keeps
paying for**: it cannot be told apart from "the gate admitted nothing to look at",
which is what made the Day-201 repair invisible in the census for a whole session
(2 and 0 before, 2 and 0 after). The missing thing is not a new detector — it is the
denominator the existing detector is reading over.

## Deliverable (one pass, four steps)

1. **Read first, do not assume.** Open `scripts/check_assertion_weakening.py` and
   confirm the names in this file exist as spelled: `count_register_lines`,
   `count_register_payoff`, `REGISTER_LITERAL_RE`, `REGISTER_LITERAL_JOINED_RE`,
   `REGISTER_ANTIVACUOUS_RE`, `hunk_carries_shape`, `convention_census`,
   `render_convention_census`, `run_self_tests`. If any name differs, follow the
   file, not this task file, and say so in the write-up.

2. **Add ONE census line, not a new counter.** Emit, for the scanned range:

   ```
   register-gate denominator: N WEAKENED hunks admitted by the gate;
                               M of them removed a register literal;
                               K of them removed a SPLIT-only guard
   ```

   `N` = hunks the classifier returned `WEAKENED` for on this range; `M` = those
    whose removed side carries a register literal (the existing
     `REGISTER_LITERAL_RE` / `..._JOINED_RE` pair, **reused, never re-spelled**);
   `K` = the subset whose removed register line is only visible after joining the
   `-U0` hunk's lines (the Day-201 split shape, via the existing
   `hunk_carries_shape`/joined-line path). Counts that already exist must be
   **unchanged** — this adds a line and changes no number.

3. **Self-tests inside the same file**, wired into `run_self_tests` (which must keep
   exiting non-zero on failure):
   - **anti-vacuous first**: on a fixture carrying a `WEAKENED` hunk that removes a
     register literal, `N >= 1` and `M >= 1` — assert those before asserting `K`, or
     an empty denominator passes the `K` assertion by agreeing with itself;
   - **split fixture** (the Day-201 shape, rustfmt-split across four lines): `K == 1`;
   - **near-miss / both directions**: a hunk removing a register literal that is
     **not** classified `WEAKENED` is excluded from `M` but still counted in a wider
     `N`, and an all-clean diff renders the line with `N == 0` and no panic;
   - render asserted as a **string a caller receives**, in whichever modes the
     existing renderer tests use.
4. **Run it for real and report the numbers**, not just the self-tests:
   `python3 scripts/check_assertion_weakening.py --self-test` **and** one real run
   over `HEAD~40..HEAD`. Report `N`, `M`, `K` for that range verbatim in the
   write-up, including if `K == 0` — a zero over a non-zero denominator is the
   honest reading this task exists to make sayable.

## Write-up (part of the deliverable, not optional)

Append to **ARCHITECTURE.md** (never CLAUDE.md — the 2026-09-15 move rule), in the
entry that covers `scripts/check_assertion_weakening.py`: the new line, the measured
`N/M/K` for the range, what `K == 0` (or `> 0`) means, and the stated limit that
`K` is a **removed-side** reading — the added side is deliberately out of scope
because `count_register_payoff`'s `WEAKENED` gate does not look there. Archive any
positive control as one atomic mutate→run→restore, run **serially**, with a
`NEUTERED` marker if you leave a mutation in place for a moment
(`tests/neutered_guards.rs` will refuse to pass otherwise).

## Do not

- Do not change any existing count or verdict, and do not touch the classifier
  itself — this task makes a number *readable*, it does not make it move.
- Do not add a second copy of the register-literal regex.
- Do not re-run the Day-201 counter repair; it landed.
