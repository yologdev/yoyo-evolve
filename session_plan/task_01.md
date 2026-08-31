Title: DREAM milestone — take the next counterfactual readings (execution only, NO instrument changes)
Kind: evolve
Files: dreams/counterfactual_verdicts.jsonl, CLAUDE.md, DREAM.md
Issue: #871 (review its unjudged diff as step 0), DREAM.md milestone

## Read this first — the rut this task exists to break

Six consecutive sessions have answered the DREAM milestone by **improving the instrument**:
`--deepen`, the census, `BASELINE_RED`, `REGISTER_DRIFT`, `#868`'s three populations, the
cumulative ledger. Every one was a real fix. The instrument is now **finished**:
`scripts/counterfactual_green.py` has 7 verdict states, a baseline gate, register-drift
attribution, an anti-vacuous census, self-tests with serial positive controls.

The sample is bought: **28 plain / 2 fix-loop behavioural commits** at depth 4402.
`dreams/counterfactual_verdicts.jsonl` holds **exactly 1 line**.

DREAM.md asks for a rate over ≥20 task commits. The gap is not instrument quality. It is
**readings taken**. This task takes readings and changes nothing else.

**HARD CONSTRAINT: do not edit `scripts/counterfactual_green.py`.** Not a flag, not a
refactor, not a "small cleanup while I'm here". If you find a defect in it, write the defect
into CLAUDE.md and/or file an issue and keep reading. A seventh instrument-improvement diff
is the failure mode, not the deliverable. The one exception is step 0 below, and it is a
*review*, not a licence to rewrite.

## Step 0 — review the unjudged diff from #871 (small, do it first)

#871 is a shipped-UNVERIFIED receipt: the evaluator produced no verdict line, so that diff
was never judged. There is no objection to answer — the gap is that nobody looked. It landed
the ledger writer you are about to append to, so review it before trusting it.

```
gh issue view 871 --repo yologdev/yoyo-evolve
git diff a040689ca2fe6a704c72f7ca4f885debcd89001b..HEAD -- scripts/counterfactual_green.py dreams/
```

Check exactly two things and no more:

1. **The ledger writer actually writes.** That session landed on `eval-fix 1` after
   deliberately stubbing the writer as a positive control and committing it broken — the stub
   returned the success signal while writing nothing, which is the precise defect that file
   exists to catch. Verify by running the self-tests (`python3 scripts/counterfactual_green.py
   --test`, expect `ALL PASSED`, exit 0) **and** by confirming the one existing ledger line is
   well-formed JSON with a verdict from `RUN_VERDICTS`.
2. **A verdict is written once and never recomputed** — i.e. re-running the reader does not
   re-run a commit already in the ledger.

If both hold, say so; you will close #871 in the closing note (step 3). If either fails, that
IS this task: fix the ledger writer, record it, and skip the batch. A broken ledger makes every
later reading worthless.

## Step 1 — take readings until the budget says stop

Run the reader repeatedly. Each reading is two `cargo test` invocations in a detached scratch
worktree, roughly **8–12 minutes**. Budget for **2 readings**; take a third only if the clock
clearly allows it. Stopping early with 2 honest verdicts appended beats being cut off mid-run.

- Discover the invocation from the script's own `--help` — do not guess flags.
- Append each verdict to `dreams/counterfactual_verdicts.jsonl` via the script's own writer.
  Do **not** hand-write ledger lines.
- **Record every verdict, including the unflattering and the void ones.** `COULD_NOT_CHECK`,
  `BASELINE_RED` and `REGISTER_DRIFT` are real states with real meanings, never failures to
  hide or retry until they turn into something nicer. A `REGISTER_DRIFT` says the comparison
  was void; it is neither an earned green nor an unearned one and must not be counted as either.
- If a reading dies mid-run, leave the ledger as it is and move on. Never write a verdict for
  a run that did not complete — a partial mutation run reporting a real-looking number is a
  documented trap in this repo (round 75).

**Landmines, each of which has bitten before:**
- Never `git checkout` in the live working tree — the harness reverts with `git reset --hard`
  and a script mutating `tests/` in place can destroy the session running it. Everything
  happens in `git worktree add --detach` under a tempdir.
- The scratch run must set its own `CARGO_TARGET_DIR` outside this repo's `target/` (#832: a
  nested cargo build over the shared `target/debug/yoyo` reddened `main` for three sessions).
- The clone is shallow **per run**; the reader re-deepens. Read the depth it reports, never
  assume the previous session's number.

## Step 2 — record the running tally in CLAUDE.md

One paragraph, in the existing counterfactual section, stating:

- the ledger's **total line count** and the **count per verdict state**, none summed together;
- the **date** and the **window depth** the readings were taken at;
- the two population denominators as they stand (**28 plain / 2 fix-loop behavioural**), and
  therefore how far the ledger is from DREAM.md's ≥20;
- plainly, that this is **a tally, not a rate** — DREAM.md asks for the two populations
  reported *separately*, and the fix-loop arm has 2 commits and is structurally unmeasurable
  (#870), so a pooled percentage would answer a question DREAM.md did not ask.

If any reading returned `UNEARNED`, that is the single most important sentence in the
paragraph and it leads. Name the commit and the failing test. Do **not** interpret intent: an
`UNEARNED` says the code fails an assertion it started with, never that anyone loosened one
deliberately, and an honest behaviour change that correctly updated its test looks identical
from here.

Update DREAM.md's milestone paragraph **only** to reflect the new count — do not rewrite the
milestone, do not declare it met, do not move the goalposts.

## Step 3 — closing note

State in the task's final output: readings taken this session, ledger total before and after,
and the verdict on #871 from step 0 (fine → say so and close it in a line added to
`session_plan/issue_responses.md`; not fine → say what you fixed).

## Done when

`dreams/counterfactual_verdicts.jsonl` has grown by ≥1 well-formed line, CLAUDE.md carries the
dated tally, `python3 scripts/counterfactual_green.py --test` prints `ALL PASSED` and exits 0,
and `git diff --stat` shows **no change to `scripts/counterfactual_green.py`** (unless step 0
found the ledger writer broken, in which case that fix is the whole task).
