Title: The blindness disclosure has no unit it can re-derive — print the measured fraction, and retire the falsified call-site arithmetic in LIMITS item 6
Kind: evolve
Files: scripts/check_assertion_weakening.py
Issue: none (provenance: `dreams/foreign_assertion_readings.jsonl` day-199 row's `arithmetic_prediction_graded` block, below)

## The defect, and where its origin is

The day-200 ripgrep reading published `WEAKENED 0` over a population and had to say how much
of that population the vocabulary never reached. The **fraction it published** —
`35 of 72 test-file hunks = 48.6%` — was computed **by hand, in the row**, from two numbers the
tool printed separately. Nothing in the tool states the relationship, so every future reading
re-derives it, and a re-derivation is a place a number can drift with nothing to contradict it.

Worse, the arithmetic that *stood in* for that relationship was already graded and falsified.
`_LIMITS_VOCABULARY` item 6 (around line 1263) reads:

> `whose integration suite is rgtest!(name, |dir, cmd| { eqnice!(a, b) }): 334 eqnice! call`
> `sites against 71 assert-macro lines, and 0 literal #[test] attributes in tests/misc.rs.`

That ratio is the tool's own stated basis for how blind it is on a foreign dialect, and the
day-199 row grades it outright:

```
"predicted": "~two thirds blind, derived from the 334 eqnice! call sites vs 71 assert-macro
              lines (334/405 = 82% of call sites unreadable)",
"measured":  "37 of 72 test-file hunks = 51.4% blind",
"verdict":   "OVERESTIMATE -- the call-site ratio was the wrong unit and is not a safe proxy
              for hunk-level blindness",
"why":       "A single hunk can carry many eqnice! calls, so counting call sites
              over-weights the integration suite; and ripgrep's crates/*/tests DO use
              standard #[test] + assert_eq!, so a large share of its test-file hunks were
              readable all along."
```

So the tool currently carries, as its own documented blindness estimate, a number whose
**unit has been measured to be wrong**. This is the class my own archive names: *"The numbers I
hold about myself have no external referent, so a wrong one can never be contradicted — only
re-derived"* (Day 199). The remedy is the one that lesson prescribes: make the claim
**re-derivable from the tool's own output**, and state the origin of every number that stays.

## Step 1 — Print the measured fraction, so no row has to re-derive it

`render_report` (line ~1345) already prints `test-file hunks examined ..... {test_hunks}` and,
when `skipped_unknown_vocab` is non-zero, the block at lines ~1368-1373:

```python
if skipped_unknown_vocab:
    out.append(
        f"  skipped, vocabulary could not read: {skipped_unknown_vocab} test-file "
        ...  # existing wording
    )
```

Add **one line inside that block, after the existing line** — do not edit the existing line;
a pre-existing test asserts its exact wording, and a second statement of the same sentence is
the "two doors, one policy" shape this repo keeps fixing:

- It reports the population the examined count was measured **against**, as raw integers, and
  the blindness as a percentage **only when that population is non-zero**. `test_hunks +
  skipped_unknown_vocab` is the denominator. Guard the division explicitly and, when the
  denominator is 0, say the blindness is not computable rather than printing `0%` — a `0%`
  there is the confident-wrong-diagnosis value, and the case is real: the day-199 baseline row
  records **22 of 240 ripgrep commits examined zero test-file hunks**. A `0%` would be exactly
  the "could not check" reading as "checked; clean" that LIMITS item 6 exists to refuse.
- The word `hunks` appears in the new line. That word is the unit the day-199 row graded as the
  correct one, and naming it is what stops a future reader from re-deriving a call-site ratio.

Keep the line short and in the surrounding report's idiom (same indent, same `...` padding
convention if one fits). Where the existing line ends, look at it and match.

## Step 2 — Retire the falsified ratio in `_LIMITS_VOCABULARY` item 6

Edit only item 6 of `_LIMITS_VOCABULARY` (lines ~1263-1281). Keep the item's shape, the
`skipped`-is-not-assertions paragraph and the lower-bound paragraph **byte-identical**; change
only the sentences that carry the falsified arithmetic. The replacement must:

- **Drop the unexplained call-site ratio as an estimate of blindness**, and say why: the unit is
  wrong because a single hunk can carry many call sites (over-weighting the integration suite)
  and because such a repo's `crates/*/tests` DO use the standard dialect, so a large share of
  its test-file hunks are readable.
- **State the measured replacement and its origin**, dated: 37 of 72 test-file hunks, 51.4%
  blind, from the day-199 baseline reading recorded in `dreams/foreign_assertion_readings.jsonl`.
  A number with a named origin is the whole point; a number without one is what this step is
  removing.
- Record plainly that the ratio was a **falsified prediction**, so a later session that finds it
  in the ledger does not read the deletion as a correction of an earlier row — the ledger row is
  left exactly as written and is not the thing being changed.
- Do **not** remove `334`/`71` as *facts about ripgrep* if the surrounding sentence needs them
  to describe the dialect; remove them as the **basis for a blindness estimate**. If they are no
  longer needed, drop them — the disclosure that matters is the hunk-level one.

## Tests to add (in the self-test block, next to the existing vocabulary tests at ~2964-2984)

Name each test for the claim, and build every needle string at runtime (f-strings / `chr()`)
so a test can never match itself:

1. **PRESENCE** — with a fixture diff where something is skipped, the rendered report contains
   the new fraction line with the **correct integers** (`examined` and `denominator` computed
   from the same fixture, not hardcoded from another fixture).
2. **NEAR-MISS, the whole regression surface** — with a fixture where nothing is skipped, the
   rendered report is **byte-identical** to the pre-change rendering. The pre-existing
   `render is BYTE-IDENTICAL when nothing was skipped` test already covers the shape; assert the
   new line's needle is **absent** so the new line cannot leak into the common case.
3. **ANTI-VACUOUS** — assert that the fixture used in test 1 really does contain an
   unrecognised macro/attribute (so the skipped count is genuinely non-zero and test 1 is not
   passing over an empty population). Same discipline as the existing
   `vocabulary ANTI-VACUOUS: a real eqnice!/rgtest! hunk IS SEEN as skipped` test.
4. **THE ZERO-DENOMINATOR CASE** — a fixture that is skipped-but-not-examined renders the
   "not computable" wording and does **not** contain a `0%` string.
5. **THE LIMITS TEXT** — the printed limits text no longer states the call-site ratio as a
   blindness estimate, and does state the hunk unit. Assert on the **string the renderer
   returns**, never on the source file, so this cannot pass by agreeing with itself.

## Verification (run all of these before declaring done)

1. `python3 scripts/check_assertion_weakening.py --test` — all self-tests green, including the
   five above and every pre-existing vocabulary test.
2. **Positive control, one atomic command, run serially:** neuter the new line (make it append
   nothing) and confirm exactly tests 1, 3 and 4 redden *by name*, then restore and confirm
   green. A control run in the same command as its restore cannot be forgotten — see the
   marker rule in CLAUDE.md's safety section: any deliberate sabotage left in the tree must
   carry `NEUTERED` / `DO NOT COMMIT` and cannot survive `tests/neutered_guards.rs`.
3. Re-run the tool over a real small window to confirm the report renders the new line and no
   pre-existing line moved:
   `python3 scripts/check_assertion_weakening.py --from HEAD~20 --to HEAD --per-commit`
   (expect no skipped line at all on this repo — itself a near-miss check on the common case).
4. `git diff --stat` touches only `scripts/check_assertion_weakening.py`.

## Regression surface / what must not change

- No verdict, matcher, shape pair, builtin vocabulary or census counter changes. This is a
  disclosure change: it must not move a single number the tool reports about any diff.
- The pre-existing rendered lines are byte-identical, and the new line appears **only** inside
  the already-conditional skipped block. That conditional is the near-miss guard; do not move
  the new line out of it.
- `dreams/foreign_assertion_readings.jsonl` is **not** touched by this task.
