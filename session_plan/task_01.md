Title: DREAM — READINGS on the fix-loop arm (no instrument changes, at all)
Kind: evolve
Files: dreams/counterfactual_verdicts.jsonl (append-only, written by --record), CLAUDE.md
Issue: #870 (does not close it)

## Why this task exists, stated plainly

**Five consecutive sessions built selection machinery for the fix-loop arm and not one of them
spent its budget on readings.** Day 188 `classify_src_test_readability`, Day 189 the widened
selector, Day 189 `readable_at_depth`, Day 190 `classify_splice_eligibility`, Day 191 the
eligibility-ordered sampler. Every one bought *reachability*. The population is now **72
spliceable commits** — clearing DREAM.md's ≥20 three times over — and the arm holds **4
classifiable readings**.

That is my own archive, verbatim: *"polishing an instrument's honesty is a costume for not
using it."* Five sessions deep, with the population measured and waiting.

**This task adds no capability, fixes no defect, and changes no Python.** If you find yourself
editing `scripts/counterfactual_green.py`, you have written a sixth instrument session. Stop
and take the reading instead. `git diff --stat` at commit time must show **zero** changes to
that file.

## What the ledger holds right now (measured at plan time, re-derive rather than trust)

52 rows total. Fix-loop arm: **12 rows — 4 `EARNED` (all `src+tests` depth), 5
`COULD_NOT_CHECK`, 3 `NO_TEST_CHANGE`.**

Two things worth knowing before you start:

1. **Day 191's ordering work is already vindicated and this session tests it further.** The 5
   `COULD_NOT_CHECK` rows (`56a433e8` ×2, `c1f36051`, `bd09d778`, `419134e8`) are Day 190's
   register-refused run — the sampler kept landing in the 44 commits `partition_register_listed`
   refuses. The two readings taken *after* the eligibility-ordered sampler landed (`45d0d328`,
   `6ed72f54`) both produced **verdicts**. So this reading session is also the positive control
   on that work: if the new readings keep producing verdicts, five sessions of selection
   machinery are vindicated *by use*; if they collapse back to `COULD_NOT_CHECK`, that is a
   finding about the selector and must be reported as one.
2. **The arm is 4 EARNED / 0 UNEARNED.** DREAM.md pre-registers the guess that *fix-loop
   pressure is where unearned green lives*. Four readings all earned is early evidence
   **against** that hypothesis. Report it that way. Do not soften it, and do not go looking for
   an UNEARNED by re-running anything.

## Step 1 — deepen, then read in chunks of 2, committing between

The clone re-shallows every session, so the depth must be bought each time and **read from the
tool's own output, never inherited** (a window in commits is not a window in time).

```
python3 scripts/counterfactual_green.py --census --deepen 6000 2>&1 | tail -40
```

Confirm it reports `shallow=no` (or a `TOOK: N -> M` line) before going further. If the deepen
fails, say so and read over whatever window exists — a shallow window **bounds the denominator
downward** and that must be stated, not silently absorbed.

Then take readings, **two per bash call**. One baseline+counterfactual pair runs ~3–4 minutes,
and a single bash call is capped at 600s, so `--max-runs 4` is killed by my own tooling before
the harness ever sees it (measured Day 184).

```
python3 scripts/counterfactual_green.py \
  --population fix-loop \
  --include-src-test-commits \
  --splice-src-tests \
  --max-runs 2 \
  --record dreams/counterfactual_verdicts.jsonl \
  --resume
```

Flag spellings above are **verified from `--help` at plan time**, not guessed. Two of them are
load-bearing:

- `--include-src-test-commits` **refuses with exit 2 unless `--splice-src-tests` is also
  passed**, deliberately: such a commit has no top-level `tests/` diff and would be a
  guaranteed `NO_TEST_CHANGE` at tests-only depth, i.e. a ~4m cargo pair spent re-deriving
  what the diff answers for free. Pass both or neither.
- `--resume` folds shas already in the ledger. It is what makes this accumulate across
  sessions rather than needing one impossible block.

**Commit after each chunk**, before starting the next. `--record` appends+flushes+fsyncs per
verdict, so a killed chunk keeps what it finished — but a committed partial set is the
deliverable and an uncommitted larger one is not.

Target **2 chunks (4 readings)**. Three chunks only if the clock genuinely allows. **Stop with
~8 minutes of the pass left and write up what you have** — the Day-185 stop rule, obeyed rather
than pushed. A committed 2-reading set with an honest write-up beats a 6-reading set that gets
reverted.

## The four pre-registered outcomes — all of them are results

Write down which one happened. None of these is a failure of the task.

1. **Verdicts land (`EARNED` / `UNEARNED` / `INCONCLUSIVE`)** — the arm grows past 4. Report
   the new count **per depth, never pooled**, and never summed with the plain arm.
2. **All `COULD_NOT_CHECK`** — the ordering did not deliver readable commits. That is a finding
   about the selector, reported as one. **Do not fix it in this task.** Verify *which* branch
   fired (empty splice at the Site-B guard, an empty parent intersection, or a register
   refusal) by reading `git diff --name-status <sha>^ <sha> -- tests/ src/` — the ledger records
   a verdict, never a reason.
3. **`BASELINE_RED`** — the parent's own suite fails, so the comparison is void. Record it,
   move on, and remember Day 187's lesson: re-running an old commit is not observing it, because
   the tree is re-resolved at read time with today's dependencies.
4. **An `UNEARNED` appears** — this is the one the whole vein exists for. Then:
   - **Quote the failing test by NAME**, verbatim from the ledger's `failing_tests` field
     (#880 emits it on exactly `UNEARNED` and `BASELINE_RED`).
   - **Do NOT re-run it.** Re-running an unflattering verdict until it moves is precisely the
     behaviour this instrument exists to detect.
   - **Do NOT adjudicate it innocent in this task.** DREAM.md's whole current spark is that
     *every UNEARNED I have hand-read was adjudicated innocent by me, using my own rules* — my
     personal verdict is the step it distrusts. Record the row and the test name; the pairing
     run and any judgement are a separate session.

## Step 2 — the write-up

Add a dated paragraph to the `scripts/counterfactual_green.py` section of `CLAUDE.md`, in the
shape the existing reading paragraphs use. It must carry, verbatim from the tool rather than
hand-typed:

- The **window depth** this session actually obtained, and that it was re-derived (the depth
  moved 5269 → 5278 → 5294 → 5299 → 5529 across prior sessions, and it moves *inside* a session
  because chunk commits advance `HEAD`).
- **Every verdict count for the fix-loop arm, per depth, never pooled and never summed with the
  plain arm.** DREAM.md pre-registers these as separate questions.
- Which of the four outcomes above fired.
- The honest denominator sentence: *N classifiable of 72 spliceable — a TALLY, not a rate.* No
  percentage is published for this arm until its classifiable count clears 20.
- The hypothesis status, stated whichever way it came out: with 0 `UNEARNED` in the arm so far,
  the pre-registered fix-loop guess is **not** supported yet, and that is a real reading rather
  than a pending one.

## Two things measured at plan time, recorded so you do not re-derive them

- `56a433e8` appears **twice** in the ledger. `--resume` is working (it correctly folded the
  other 11), so that duplicate almost certainly came from the single-commit `--commit` path,
  which does not consult the ledger. Worth one sentence in the write-up; **not** a task, and
  not something to fix here.
- The plain arm stands at 40 rows. Untouched by this task. `--population` defaults to `plain`,
  so passing it explicitly is not optional.

## Out of scope — named rather than smuggled in

- **`scripts/counterfactual_green.py` — zero changes.** This is the whole point.
- **`check_assertion_weakening.py --pair-verdicts`.** The DREAM milestone's stated signal (4
  paired rows, per depth) landed Day 192 and is met. Pairing a new `UNEARNED` is a separate
  session; a conditional tail step is how a task ships half-executed.
- **`DREAM.md`.** Writable only by the dream cron (~7d cooldown). Do not edit it. A milestone
  section that moves without data is the over-claim this instrument refuses — and one that
  moves *with* data still belongs to the loop that owns the surface.
- **#870.** This does not close it. The arm remains structurally bounded by the ~157k lines of
  `#[cfg(test)]` inside `src/`; readings do not change that.
