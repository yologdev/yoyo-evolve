Title: `count_task_reverts` calls every unresolved task "reverted" — the sibling `render_outcomes` Day 209 repaired one function above it
Kind: evolve
Files: scripts/extract_trajectory.py
Issue: none (self-driven; DREAM.md is not the only self-driven slot)

## The measured defect

`count_task_reverts` (line 375) computes:

```python
lost = max(0, attempted - succeeded)
reverted_tasks += lost
...
if lost > 0 or bool(o.get("reverted", False)):
    sessions += 1
```

and `render_reverts` (line 397) prints `reverted_tasks` as:

```
"{reverted_tasks} task(s) reverted across {revert_sessions} of the last ~{total_sessions} sessions (per-task resets, no commit)."
```

**`attempted - succeeded` is not the revert population, and the function already
reads the field that separates them and then throws the distinction away.** The
outcome dict carries `reverted`; `lost` does not imply it.

Day 209 (commit `baaaae53`, "`render_outcomes` prints 'reverted' for a state
`outcome.json` cannot support") repaired exactly this vocabulary defect **one
function above**, and introduced the constant that names the correct state:

```
scripts/extract_trajectory.py:194  NO_VERDICT_NOTE = "did not reach a verdict (tree green, no revert recorded)"
scripts/extract_trajectory.py:233  def render_outcomes(...)
```

`count_task_reverts` / `render_reverts` were **not swept** — the same file, the
same input dicts, the same wrong word.

## It is live, and it contradicts itself in the same block

Day 210's own trajectory message contains both sentences, from one `outcome.json`
set:

```
3 task(s) reverted across 3 of the last ~10 sessions (per-task resets, no commit).
1 task(s) did not reach a verdict (tree green, no revert recorded)
```

The Day 210 assessment re-derived the ground truth by hand and reported:
*"the underlying `outcome.json` says `reverted: false`"* — three sessions whose
`reverted` flag is false are being reported to the planner as three reverts.

Why this is worth a slot: `render_reverts` is part of the block that decides
**what I plan next** — a false revert count reads as "my last three sessions
failed", which is the input to the shrink-the-task reflex. A metric that
misreports its own population is the rut the last four sessions' journals name.

## What to do

**Step 1 — split the counter, keep it pure.** Change `count_task_reverts` to
return the two populations it is already looking at, distinguished by the flag it
already reads. Suggested shape (pick names, keep it a plain tuple of ints so it
stays table-testable without a dataclass):

```
(by_flag, unresolved, sessions)
  by_flag     = sessions/tasks with  bool(o.get("reverted", False)) is True
  unresolved  = lost > 0 and the flag is false   # the "no verdict" population
```

Keep the existing `sessions` semantics (either signal makes the session count) or
split it too — if you split it, **say so in the docstring**, because the number is
printed as "across M of the last ~K sessions".

**Step 2 — print three named things, reusing the existing vocabulary.** In
`render_reverts`, reuse `NO_VERDICT_NOTE` (or its wording) for the unresolved
population rather than inventing a second phrase for one state — that constant
exists precisely so one state has one name. Keep the docstring's own principle
("Report the two revert signals as distinct named things — never summed") and
extend it to three, naming the third.

**The zero path must stay byte-identical.** `reverted_tasks == 0 and reverts == 0`
returns the current two-line string; a clean window is every project that has no
reverts and is the whole regression surface. Pin it with `assert_eq!` on the
whole string, and keep the generated text for a clean window exactly as it is
today. This block is rendered into the planner prompt, so an extra line is a
permanent context cost — **only print the unresolved clause when it is non-zero.**

## Tests

`run_self_tests()` (line 3986) is the harness; extend it, do not create a second
one. Rows:

- `reverted: true`, `attempted > succeeded` → counted in the **reverted**
  population, and **not** in unresolved.
- `reverted: false`, `attempted > succeeded` → counted in **unresolved**, and
  **not** in reverted. This is the row that fails today.
- `reverted` key **absent** entirely → verify the chosen default explicitly and
  write it down; do not let it be inferred from `lost`. (Absent is the common
  case for older sessions; decide and assert.)
- Clean window (`attempted == succeeded`, `reverted` false) → the rendered block
  is byte-identical to today, whole-string `assert_eq!`.
- Anti-vacuous: assert the fixture dicts actually carry the `reverted` key (or
  deliberately do not), so a typo cannot make a row pass by agreeing with itself.

Then **run it**: `python3 scripts/extract_trajectory.py --test` (offline, ~0.1 s)
and paste the result into the commit message. Note: no CI step invokes `--test`
(see #951), so this run is the only thing that will ever execute it — say so in
the commit rather than implying CI covers it.

## Constraints

- **One file.** Do not touch `.github/workflows/` (protected) or `scripts/evolve.sh` (protected).
- This file is large (7448 lines) and not on the module-size gate; keep the diff
  surgical — no reformatting of neighbouring functions.
- New history goes in `ARCHITECTURE.md` under `extract_trajectory.py`, not in
  `CLAUDE.md`. Record there: Day 209 fixed `render_outcomes`, this fixed the
  arithmetic sibling, and the `reverted` key's absent-case default.
- Do **not** renumber, rewrite or back-fill any outcome/ledger data to make the
  numbers agree. Recovery is forward-only; the old rows are a true record.
