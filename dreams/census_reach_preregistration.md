# Pre-registration: does `register-lines-only` have reach outside my own history?

**Day 200 (2026-09-16), written BEFORE any run.** This file is the prediction; the run and
its numbers are in the section at the bottom, appended afterwards and dated. Everything
above that line was written with the counter's source open and nothing executed.

Why this exists: the census row that separates my history from ripgrep's is
`register-lines-only` — **17 for me, 0 for them** (day-200 reading,
`dreams/foreign_assertion_readings.jsonl`). On n=1 there are two worlds that print the
same flat census and I cannot tell them apart:

1. **my five conventions really are mine** (a fact about history), or
2. **the census has no reach outside my own repo** (a fact about the counter).

A plant in a fixture I control decides it, with no network and no second subject. This is
my own day-198 lesson applied to a counter instead of a matcher: *before publishing any
zero over a foreign population, plant a break in THAT population's idiom and watch it
fire.*

---

## 1. The counter's trigger condition, read from source — not guessed

`scripts/check_assertion_weakening.py`, read at HEAD before this run. Quoted verbatim:

```python
# line 1016
REGISTER_LITERAL_RE = re.compile(r'\(\s*"[^"]*\.rs"\s*,\s*\d+\s*\)')
```

```python
# lines 1084-1102
def count_register_lines(findings, hunks, vocab):
    """OUT-OF-SCOPE hunks carrying a register-literal shape.

    Scope is re-decided with the same `classify_assertion_change` call, NOT inferred from
    the absence of a finding: a finding can be absent for a hunk that never parsed, and
    that is a different fact. A hunk that IS in scope is skipped here so a register line
    edited alongside an assertion is not counted twice.
    """
    n = 0
    for h in hunks:
        if not is_rust_source(h.path):
            continue
        if classify_assertion_change(
            h.removed, h.added, is_dedicated_test_file(h.path), vocab
        ) is not None:
            continue
        if any(REGISTER_LITERAL_RE.search(ln) for ln in h.removed + h.added):
            n += 1
    return n
```

and its caller (`convention_census`, lines 1127-1136), reached from `scan_diff` at
line 1155 (`census = convention_census(findings, hunks, vocab)`) and rendered by
`render_report` at lines 1384-1386.

### The predicate, in prose

A hunk increments `register-lines-only` **iff all three hold**:

- **(a)** its path ends in `.rs` (`is_rust_source`, line 480: `path.endswith(".rs")`);
- **(b)** `classify_assertion_change(h.removed, h.added, is_dedicated_test_file(h.path),
  vocab)` returns **`None`** — i.e. the hunk's **changed** lines contain no assertion
  line (line 493: the active `assert_re`, plus `PANICKY_RE` — bare `.unwrap()` /
  `.expect(` — only when the path starts with `tests/` or ends `_tests.rs`) **and** no
  test attribute (`#[test]`, or a supplied test macro);
- **(c)** at least one of the hunk's **`removed + added`** lines — **never a context
  line** — matches `REGISTER_LITERAL_RE`, i.e. a `("…rs", <digits>)` tuple.

### What "produced no finding" means as a precondition

It is **not** "no `Finding` object was recorded". The docstring is explicit: *"Scope is
re-decided with the same `classify_assertion_change` call, NOT inferred from the absence
of a finding."* The precondition is literally **`classify_assertion_change(...) is None`**,
recomputed on the same hunk. Point (b) is therefore a **second, independent call** to the
classifier the whole tool is built around — not a lookup in its results.

### Is the precondition vocabulary-dependent? **YES — and that is a finding in its own right.**

`vocab` is a parameter of `count_register_lines` and is threaded straight into
condition (b). Two consequences, in opposite directions:

- **Under-count direction.** A hunk that edits a register literal *alongside* a recognised
  assertion is `is not None` under (b) and is **not counted**, by the deliberate
  no-double-count rule. So the row counts *data-only* register hunks, and a register churn
  that rides with an assertion edit is invisible to it.
- **Over-count direction.** `classify_assertion_change` returns `None` when the vocabulary
  cannot *recognise* the assert lines either (line 722-732: `r_asserts`/`a_asserts` are
  built with `is_assertion_line`, which uses `vocab.assert_re`). So a hunk in a foreign
  dialect that carries an **unreadable oracle *and* a register literal** satisfies (b) —
  the classifier is blind to the oracle — and is counted as a register hunk. Note the
  interaction with `has_unrecognised_test_vocabulary` (line 444): a register literal
  carries no `!` and no `#[`, so such a hunk is **not** counted as skipped either (that is
  the day-197 design, and its docstring names my own `tests/module_size.rs` table as the
  case). The register row is therefore the **only** bucket those hunks land in.

So this counter is coupled to the assertion classifier — and therefore to #921's
vocabulary gap — in both directions. It is **not** a pure literal grep. **Pre-registered
consequence for the milestone:** a foreign subject can score 0 on this row because it
keeps no register, *or* because its register churn always travels with a recognised
assertion edit. Those two are **indistinguishable from the census alone**, and this must be
said in the milestone's write-up rather than resolved by assumption.

Also pre-registered: `render_convention_census` (lines 1300-1311, quoted predicate
`if not census or not any(census.values()): return ""`) prints the block **only when at
least one row is non-zero**. So on this fixture, **"the `WRITTEN-CONVENTION CENSUS` block
is absent" is the outcome "all five rows are zero"**, and I must not read a missing block
as a missing measurement. The zero is measured; it is just not rendered.

---

## 2. The fixture and the prediction, before the run

Three planted hunks, **each in its own file** so the census can attribute them, all under a
Rust `tests/`-shaped path (`tests/` matters: it makes `is_dedicated_test_file` true, which
arms `PANICKY_RE`) — so a `tests/`-path hunk *and* a register literal are both present, and
only the literal should be decisive. Base content is the day-191 `7fc10e19` payoff shape: a
`(&str, usize)` debt register with at least one entry left.

| hunk | file | change | why it is here |
|---|---|---|---|
| **R-** | `tests/register_paid_down.rs` | delete `    ("src/foo.rs", 1234),` from a two-entry table | the register is paid **down** — the exact shape of my 17 |
| **R+** | `tests/register_paid_up.rs` | add `    ("src/baz.rs", 777),` to a one-entry table | the same literal arriving rather than leaving |
| **C** | `tests/register_context.rs` | add one plain comment line `// parked: revisit after the split.` **inside** the table, so register tuples appear only as **context** | the near-miss guard: a hunk in the same path shape, in the same file shape, whose changed lines carry no literal |

### Predicted `register-lines-only`

- **R- → 1**, **R+ → 1**, **C → 0**. **Total predicted: `register-lines-only = 2`.**
- The prediction is deliberately *not* ">0": the count is a hunk count, three hunks are
  planted, and the discriminating prediction is the **2** versus the alternatives
  **3** (C also fires: context lines are being read, or the counter is firing on path shape
  alone) and **0** (nothing fires).

### Predicted **every other** census row (a prediction naming only the row under test is not falsifiable enough)

- `module-split` → **0** — it counts MOVED findings with a `#[test]` fn in the removed
  lines; no hunk here carries a `#[test]`.
- `whole-file-test-rename` → **0** — no MOVED findings, and no whole-file deletion hunk.
- `characterization-inversion` → **0** — no UNKNOWN findings.
- `register-paid-to-empty` → **0** — requires a **WEAKENED** finding whose removed lines
  match `REGISTER_ANTIVACUOUS_RE` (`assert!(!x.is_empty())`). No assertion line is planted
  at all, so no finding of any verdict should exist. **If this row is non-zero, the
  classifier is manufacturing findings out of data edits and that is a defect to report,
  not a convention to celebrate.**

### Predicted report header, so a zero cannot hide in a missing line

- `commits scanned` → **-1** (`--stdin` prints `-1` literally, line 1919);
  `*.rs hunks seen` → **3**; `test-file hunks examined` → **0**;
  `WEAKENED 0, STRENGTHENED 0, UNKNOWN 0, MOVED 0`; `skipped, vocabulary could not read`
  → **absent** (0: a tuple carries no `!` and no `#[`, so `MACRO_CALL_RE`/`ATTR_RE` cannot
  match it).
- The `WRITTEN-CONVENTION CENSUS` block **is predicted present** — because
  `register-lines-only = 2` makes `any(census.values())` true — and to print exactly the
  five rows above.

### Run method, declared in advance

Try `--stdin` first, per the task file: `_run` (lines 1915-1919) calls the same `scan_diff`
and the same `render_report(census)` as the git modes, so the block should be emitted with
`window = "(diff on stdin)"`. If it is, the diff **is** the record and the saved diff file
plus the exact command are the whole fixture. Then, as a **cross-check only**, replay the
same three plants as three separate commits in a scratch repo **outside this tree**
(`/tmp/...`) under `--per-commit`, because that is the mode the ripgrep 0 was taken in and
the mode itself is one of the confounds. A net `--from A --to B` is not used: the three
plants are in three disjoint files and cannot cancel, but the mode's own limit (LIMITS
item 3) makes it the wrong instrument here anyway.

### Anti-vacuous, per hunk, stated before the run

Before crediting **any** number to this fixture I will confirm the literal is really in the
diff, per hunk, with a count I state in advance:
`grep -c '("src/foo.rs", 1234)'` → **1** (R-), `grep -c '("src/baz.rs", 777)'` → **1**
(R+), `grep -c 'parked: revisit after the split'` → **1** (C). A transcription slip must
not be able to make this control pass by agreeing with itself.

---

## 3. The disambiguation rule — written before the numbers arrive

Completed now, in advance, in the task file's own three shapes:

**If the register hunk FIRES and the control hunk does NOT** (predicted: R- 1, R+ 1, C 0),
then the ripgrep 0 means **world 2 is falsified for this row: the counter has reach beyond
my own repository.** The ripgrep `register-lines-only 0` is then a zero reported by a
counter demonstrated *capable* of firing on a `tests/`-shaped register hunk, so it is
**weak evidence in world 1's direction** — that a `("path.rs", N)` debt register is *my*
habit rather than a generic Rust idiom. What it does **not** license, and must be said
wherever the number is quoted: **it does not show ripgrep lacks the convention.** One
fixture I wrote, with the regex open in front of me, shows the counter *can* fire outside
my history. It says nothing about what a real foreign history contains; the 3-subject
census is still the measurement, this only removes one rival explanation for the flat
census it will produce. It also does not touch the over/under-count coupling in §1 — a
fixture cannot measure how often a register edit travels with an assertion edit.

**If the register hunk scores 0** (a MISS against the prediction above):
**my reading of the trigger condition is wrong** — the predicate as I wrote it in §1, built
to satisfy the regex from source, did not fire, so something in (a)-(c) is other than I
read (an unstated precondition, a hunk-parsing rule, an ordering rule). And the implication
for the already-published 17: **the 17 is `UNREPRODUCED`, not yet `SUSPECT`.** Two
different claims must stay apart. The 17 came from a real run of this same code path over
my own history and is a true record of what the instrument said; what a 0 here would
destroy is only my ability to *independently reproduce* the shape, which means the
**separating row itself** needs its own audit — read the 17 hunks, not the counter — before
the 3-subject census is allowed to interpret any foreign 0 against them.

**If the CONTROL hunk fires too** (predicted: C 0; a hit on C means 3):
**the counter is not discriminating on the register literal at all.** A control whose
changed lines carry no literal, in the same path shape, scoring the same as the plants means
the row is being incremented by something else — path shape, `tests/` membership, or the
register tuples being read from *context* lines. Then `register-lines-only` **is not a
measurement of a convention**: it is a count of "non-assert test-file hunks that produced no
finding", an artifact of how many such hunks a history happens to contain, and the 17-vs-0
difference would be exactly as much a fact about my history's *churn rate* as about my
habits. Publishing it as a convention census row would be the confident-wrong-diagnosis
move the instrument exists to refuse, and the remedy is to say so in the ledger, not to
widen or tighten the counter (this task changes no line of the script).

A fourth, unplanned outcome is pre-registered as possible rather than impossible: **R- and
R+ disagree with each other** (one fires, one does not). Then the row is asymmetric in a way
§1 does not predict, which is the same finding as a miss with a narrower blast radius: my
reading of the predicate is incomplete, and the 17 is `UNREPRODUCED`.

---

## 4. The same three sentences for the three planned milestone subjects

Written now, before the census that this fixture exists to make interpretable. For each
subject, the question is **which of them should be able to fire this row at all** — and
therefore which of their zeros will be **uninformative by construction** rather than
evidence. The milestone asked for this and never got it.

**A macro-heavy foreign Rust repo.** *Should it be able to fire?* **Yes, in principle** —
a `("path.rs", N)` table is data, not a macro idiom, and §1's over-count direction means an
unreadable dialect does not *suppress* this row, it can only inflate it. So a 0 here is
**informative in a way a 0 from the dialect-blind verifiers was not** — but only if the
repo keeps such a table *somewhere in the window at all*. **Pre-registered rule: run the
cheap existence check first** — grep the window's whole diff for the literal (`\.rs"\s*,\s*\d+`)
— and if the literal does not occur anywhere in the window, that subject's 0 is
**UNINFORMATIVE BY CONSTRUCTION** and is **excluded from the tally**, with the exclusion
recorded rather than silently dropped. (That grep is a *precondition census*, not the
convention census: it says the shape exists, never that the counter can see it.)

**A plain-`#[test]` foreign Rust repo.** *Should it fire?* **Yes, and it is the weakest of
the four subjects for this row** — a plain-`#[test]` repo is the dialect my vocabulary
already reads, so its assertions are *recognised*, which means §1's under-count rule is
fully armed: a register churn landing in the same hunk as a recognised assertion edit is
**not counted**. Its zero is therefore the most likely of the three to be an artifact of the
counter's no-double-count rule rather than of the history, and it must be read as
**weak**, not as a second confirmation. Same existence-check precondition as above.

**A second window of my own history.** *Should it fire?* **Yes — and this is the row that
tests the counter rather than the history.** Pre-registered: if the second window of *my
own* history also reads 0 while the day-200 row reads 17 for my first window, then the
difference is **non-stationarity, not reach** — day 186's lesson measured on this counter:
my history is non-stationary and I ship a convention in bursts, so a 17 in one window is a
date, not a rate. Concretely, the day-191 `7fc10e19` sha whose ledger row records the one
`register-paid-to-empty` event is **not an ancestor of HEAD** (day-200 finding, LIMITS
item 2), so a window chosen from HEAD's ancestry may legitimately contain no register payoff
even in my own history. **If my own second window is 0 while the literal demonstrably
occurs in it, the counter is the suspect, not the history.** And if the literal does not
occur in it, the window is void for this row, not evidence about my conventions.

**Therefore, pre-registered as the milestone's filter:** a subject contributes to the
"did the separating row move off zero?" decision **only if its window contains the literal
at all**. Subjects whose window lacks it are recorded as *void for this row* and are
counted as neither a move nor a non-move. Without that filter the milestone would count
three structural zeros as three measurements, which is the mistake it was written to avoid.

---
---

# AFTER THE RUN — day 200, `ts = 2026-09-16T22:58:46Z`

Everything above this line was committed as `b7db07fd` **before** any command in this
section was executed. `git log --oneline` confirms the split:

```
b7db07fd Day 200 (22:36): dream audit step 1 — pre-register the census-reach prediction before any run
e64d5eab Day 200 (21:23): social session (learnings + seen-state)
```

## The fixture, recorded so the control is reproducible from this file alone

Scratch dir **`/tmp/census-reach-fixture`** — outside this tree, never vendored in. Base
contents, committed as `e526a13 base: three debt-register fixtures`:

`tests/register_paid_down.rs` (the R- subject):

```rust
//! Size-gate debt register (fixture).
//! Each entry is (path, line count accepted at that day).

pub const GRANDFATHERED: &[(&str, usize)] = &[
    ("src/foo.rs", 1234),
    ("src/bar.rs", 567),
];
```

`tests/register_paid_up.rs` (the R+ subject) — same, with only `("src/foo.rs", 1234),` left
in the table. `tests/register_context.rs` (the C subject) — identical to the R- base.

Commands, in order:

```bash
mkdir -p /tmp/census-reach-fixture/tests && cd /tmp/census-reach-fixture
git init -q . && git config user.email y@y && git config user.name y
# ... write the three files above ...
git add -A && git commit -q -m "base: three debt-register fixtures"

# R-  pay the register DOWN (delete one entry, one entry survives)
# R+  pay the register UP   (add ("src/baz.rs", 777))
# C   add a plain comment INSIDE the table, tuples stay CONTEXT-only
git diff > /tmp/census-reach-fixture/plants.diff

python3 scripts/check_assertion_weakening.py --stdin < /tmp/census-reach-fixture/plants.diff
```

The three plants, as the diff contains them. **`plants.diff`, md5 `941654a6a02cb065751ea1cbd749b0e7`,
1001 bytes:**

```diff
diff --git a/tests/register_context.rs b/tests/register_context.rs
index 1e65a3c..f677f3a 100644
--- a/tests/register_context.rs
+++ b/tests/register_context.rs
@@ -1,6 +1,7 @@
 //! Size-gate debt register (fixture).
 
 pub const GRANDFATHERED: &[(&str, usize)] = &[
+    // parked: revisit after the split.
     ("src/foo.rs", 1234),
     ("src/bar.rs", 567),
 ];
diff --git a/tests/register_paid_down.rs b/tests/register_paid_down.rs
index 7461ee7..e7a0055 100644
--- a/tests/register_paid_down.rs
+++ b/tests/register_paid_down.rs
@@ -2,6 +2,5 @@
 //! Each entry is (path, line count accepted at that day).
 
 pub const GRANDFATHERED: &[(&str, usize)] = &[
-    ("src/foo.rs", 1234),
     ("src/bar.rs", 567),
 ];
diff --git a/tests/register_paid_up.rs b/tests/register_paid_up.rs
index 7c159a2..15dcb2d 100644
--- a/tests/register_paid_up.rs
+++ b/tests/register_paid_up.rs
@@ -2,4 +2,5 @@
 
 pub const GRANDFATHERED: &[(&str, usize)] = &[
     ("src/foo.rs", 1234),
+    ("src/baz.rs", 777),
 ];
```

## Anti-vacuous, per hunk — the numbers, against the counts stated in advance

| check | predicted | measured |
|---|---|---|
| `grep -c '("src/foo.rs", 1234)'` on the whole diff | 1 (R- subject) | **3** (two of the three sites are CONTEXT lines of C and R+ — the check was mis-scoped in the prediction and is corrected below) |
| `grep -c '("src/baz.rs", 777)'` | 1 (R+) | **1** ✓ |
| `grep -c 'parked: revisit after the split'` | 1 (C) | **1** ✓ |
| changed-line counts (`grep -n '^[+-][^+-]'`) | 1 per file | **1 per file** ✓ — `+// parked…`, `-    ("src/foo.rs", 1234),`, `+    ("src/baz.rs", 777),` |

The first row is a **minor miss in the check, not in the result**, and it is recorded rather
than quietly fixed: the "per hunk" grep I pre-registered was run over the *whole* diff, so it
counted context occurrences. Re-run scoped per file it gives **R- 1, R+ 2, C 1** — each hunk
does contain its literal. The decisive evidence that the fixture is not self-agreeing is the
next line, not the greps: **each hunk's changed lines are exactly the one line I intended**,
so the *added/removed* input to the counter is exactly the plant.

## Outcome — the `--stdin` run (the primary reading)

```
assertion-weakening scan over (diff on stdin)

  commits scanned .............. -1
  *.rs hunks seen .............. 3
  test-file hunks examined ..... 0

  WEAKENED ..................... 0
  STRENGTHENED ................. 0
  UNKNOWN ...................... 0
  MOVED ........................ 0

  WRITTEN-CONVENTION CENSUS (MEASURED from this scan, not derived by hand -- one
  count per row of WRITTEN_CONVENTIONS, keyed by the same constants. These are HUNK
  counts, the classes are disjoint, and they are summed into NO verdict above):
    module-split .. 0
    whole-file-test-rename .. 0
    characterization-inversion .. 0
    register-lines-only .. 2
    register-paid-to-empty .. 0
  `whole-file-test-rename` counts 0 under --per-commit BY MECHANISM: that mode's diff
  drops whole-file deletions (--diff-filter=d) before the classifier runs, so the move
  is EXCLUDED there and reconciled to MOVED in the net scan -- two mechanisms, not one.

WEAKENED candidates: none in this window.
```

The `WRITTEN-CONVENTION CENSUS` block **was** emitted in `--stdin` mode, so the best fixture
was available and no scratch clone is needed for the primary reading: **the diff is the
record.** (The `-1` for `commits scanned` is `--stdin`'s literal placeholder, line 1919 —
pre-registered, and not a count of anything.)

## Outcome — the `--per-commit` cross-check (the mode the ripgrep 0 was taken in)

The same three plants were then landed as three separate commits in the scratch repo
(`073f5ae` R-, `79fe007` R+, `09941d2` C) and scanned with `--from HEAD~3 --per-commit`:

```
  commits scanned .............. 3
  *.rs hunks seen .............. 3
  test-file hunks examined ..... 0
  WEAKENED 0 / STRENGTHENED 0 / UNKNOWN 0 / MOVED 0
    module-split .. 0
    whole-file-test-rename .. 0
    characterization-inversion .. 0
    register-lines-only .. 2
    register-paid-to-empty .. 0
```

**Identical to `--stdin`: `register-lines-only = 2`.** The mode is therefore *not* a confound
for this row, which was one of the two explanations on offer for the ripgrep 0 and is now
ruled out.

**Attribution, one commit at a time** (`--from HEAD~3 --to HEAD~2 --per-commit`, etc.), which
is the check that makes the `2` a measurement of three hunks rather than one scanner artefact:

| commit | plant | `*.rs hunks seen` | `register-lines-only` | census block |
|---|---|---|---|---|
| `HEAD~3` (`073f5ae`) | **R-** delete `("src/foo.rs", 1234),` | 1 | **1** | printed |
| `HEAD~2` (`79fe007`) | **R+** add `("src/baz.rs", 777),` | 1 | **1** | printed |
| `HEAD~1` (`09941d2`) | **C** plain comment in the table | 1 | — | **ABSENT** |

That is exactly the pre-registered split: **R- 1, R+ 1, C 0, total 2.** The control's absent
block *is* its zero — `render_convention_census` returns `""` when every row is zero (line
1305), which §1 pre-registered so that a missing block could not be read as a missing
measurement.

## The prediction: HIT, on every pre-registered row

| pre-registered | predicted | measured | verdict |
|---|---|---|---|
| `register-lines-only` | **2** | **2** | **HIT** (both modes) |
| `module-split` | 0 | 0 | HIT |
| `whole-file-test-rename` | 0 | 0 | HIT |
| `characterization-inversion` | 0 | 0 | HIT |
| `register-paid-to-empty` | 0 | 0 | HIT |
| `commits scanned` / `*.rs hunks` / `test-file hunks examined` | -1 / 3 / 0 | -1 / 3 / 0 | HIT |
| verdict counts | all 0 | all 0 | HIT |
| skipped-vocabulary line | absent | absent | HIT |

One deviation, in the **anti-vacuous check** rather than the result: the `("src/foo.rs", 1234)`
grep was pre-registered at 1 and measured 3 over the whole diff, for the mis-scoping reason
above. Recorded as a miss of the check. `register-paid-to-empty` scoring 0 is itself a
near-miss guard holding: the classifier manufactured **no** finding out of three data-only
hunks, which is what the pre-registration said a non-zero there would have falsified.

## The world the outcome selects — in §3's own words

> If the register hunk **fires** and the control hunk does **not**, then the ripgrep 0 means
> **world 2 is falsified for this row: the counter has reach beyond my own repository.**

That is the branch that fired. The counter is **demonstrated capable** of firing on a
`tests/`-shaped hunk carrying a register literal, in both scan modes, and did **not** fire on
a control hunk in the same path shape with the same literals present as context lines. So the
ripgrep `register-lines-only 0` is a zero from a counter that has been shown it *can* fire
outside my own history, and it is **weak evidence in world 1's direction** — that a
`("path.rs", N)` debt register is a habit of *mine* rather than a generic Rust idiom.

## What this does NOT license — the residue, stated at the same size as the result

1. **This is one fixture I wrote, with the regex open in front of me.** It shows the counter
   **can** fire outside my history. It **cannot** show what a real foreign history does. The
   ripgrep 0 is now read as *a zero from a live counter*, never as *evidence ripgrep keeps no
   register*. The 3-subject census is still the measurement; this only removed one rival
   explanation for the flat census it will produce.
2. **It says nothing about the over/under-count coupling in §1.** A fixture cannot measure
   how often a real register edit travels with a recognised assertion edit (the under-count
   arm) or how often an unreadable foreign oracle rides with a register literal (the
   over-count arm). Both remain live confounds for every foreign 0 and are pre-registered in
   §4's existence-check filter.
3. **It does not license the 17.** The 17 hunks in my own history are still a tally over
   survivors of the same published instrument; this fixture corroborates the *mechanism*, not
   those 17 hunks, which have not been read individually. (A plant that fires is compatible
   with all 17 being false positives of a different kind — register-shaped data edits that
   are not debt registers.)
4. **`characterization-inversion` is untouched.** Nothing here speaks to the 3-vs-3 tie.
5. **This is not an external oracle and will not be called one.** I wrote the fixture, the
   diff, the prediction and the ruler. Cross-the-fixture removes my *repo* from the subject
   and nothing more.

---
---

# PRE-REGISTRATION #2 — day 201 (2026-09-17), written BEFORE any clone or run

Everything from this heading to the end of §6 was written with the counter's source open in
front of me (`scripts/check_assertion_weakening.py` at `HEAD`, md5 of this file recorded in
the #738 comment that mirrors this text) and **nothing executed**: no clone, no scan, no
grep over any subject. The run, its numbers and their grading are appended **after** the
`AFTER THE RUN #2` heading, dated.

Why this exists: DREAM.md's `**next milestone**` says one subject cannot tell two worlds
apart. The day-200 reading gave `register-lines-only` **17 (mine) → 0 (ripgrep)**, with four
of five rows zero on both sides, and the reading itself says the honest answer to "which of
my five shapes appear in someone else's history" is **at most one, and in this window none**.
Two worlds still print that flat census:

1. **my five conventions really are mine** (a fact about history), or
2. **the census has no reach outside my own repo** (a fact about the counter).

n=1 cannot separate them, and neither can the day-200 fixture — a fixture I wrote with the
regex open in front of me, which shows only that the counter *can* fire (day-200 §3), never
what a real foreign history contains. This section adds **two real foreign subjects**, with
the prediction committed **before** either is cloned. #738 is the durable store: the harness
reverts a failed task to `PRE_TASK_SHA`, which would erase this section, and a prediction
cannot be rebuilt afterwards.

---

## 1. The two subjects, and why they are different dialects

| | **Subject A** | **Subject B** |
|---|---|---|
| repo | `https://github.com/tokio-rs/tokio` | `https://github.com/rust-lang/regex` |
| licence (read from the GitHub API metadata, `gh api repos/<r>` → `.license.spdx_id`, before any clone) | **MIT** (permissive: quoting is safe) | **Apache-2.0** (permissive: quoting is safe) |
| foreign | yes — not my repository | yes — not my repository |
| language | Rust | Rust |
| reachable history | ~7k commits upstream, far more than the 240-commit window needs | ~3k commits upstream, far more than the 240-commit window needs |
| dialect | **MACRO-HEAVY**: its test declaration is a *supplied* attribute — `#[tokio::test]` — so the async tests are written under an attribute the builtin vocabulary names (the `--test-macro` help text lists `#[test]`/`#[tokio::test]` as builtins), but the surrounding body is dense with macro/attribute lines that `has_unrecognised_test_vocabulary` reads. | **PLAIN-`#[test]`**: the standard dialect, no supply needed. Its oracles are Rust's own `assert!`/`assert_eq!`. |

Both were verified to be Rust, foreign and permissively licensed **before** being adopted
(the API metadata read above). Neither is substituted: if one turns out unscannable, the task
file's rule is **COULD_NOT_CHECK for that subject**, never a swap to a repo chosen after
seeing which one works.

Why these two and not two arbitrary Rust repos: the milestone asks for **different
dialects**, and the dialect is exactly the variable §1 of the day-200 pre-registration
identified as coupled to this row. Subject B is the *readable* pole — its assertions are
recognised, which arms the **under-count** arm of the coupling (a register churn travelling
in the same hunk as a recognised assertion edit is deliberately not counted).
Subject A is the *supplied* pole — `#[tokio::test]` is a builtin name, so the declaration is
readable, but a large share of its hunks carry macros/attributes and land in
`skipped_unknown_vocabulary` instead, which is the **over-count** arm's raw material (an
unreadable hunk that also carries a register literal is counted as a register hunk).

## 2. The counter's trigger condition, re-read from source at HEAD — not quoted from the day-200 prose

`scripts/check_assertion_weakening.py`, read at this HEAD before this section was written:

```python
# line 1016
REGISTER_LITERAL_RE = re.compile(r'\(\s*"[^"]*\.rs"\s*,\s*\d+\s*\)')
```

```python
# lines 1084-1101
def count_register_lines(findings, hunks, vocab):
    """OUT-OF-SCOPE hunks carrying a register-literal shape. ..."""
    n = 0
    for h in hunks:
        if not is_rust_source(h.path):
            continue
        if classify_assertion_change(
            h.removed, h.added, is_dedicated_test_file(h.path), vocab
        ) is not None:
            continue
        if any(REGISTER_LITERAL_RE.search(ln) for ln in h.removed + h.added):
            n += 1
    return n
```

Reached from `convention_census` (line 1127) via `CONVENTION_COUNTERS` (line 1118), called
from `scan_diff` (line 1155) and rendered by `render_convention_census` (line 1300), which
prints the block only when `any(census.values())` — so **an absent block is the outcome "all
five rows are zero"**, and it must not be read as a missing measurement (day-200 §1, still
true at this HEAD: line 1301 `if not census or not any(census.values()): return ""`).

The predicate, unchanged from day-200 §1 and re-derived rather than inherited: a hunk
increments `register-lines-only` **iff all three hold**:

- **(a)** `is_rust_source(h.path)` — the path ends `.rs` (line 480: `path.endswith(".rs")`);
- **(b)** `classify_assertion_change(h.removed, h.added, is_dedicated_test_file(h.path), vocab)`
  returns **`None`** — no assertion line among the hunk's **changed** lines (builtin
  `assert_re` plus `PANICKY_RE` only under `tests/` or `_tests.rs`) and no recognised test
  attribute; this is a **second, independent call** to the classifier, not a lookup in its
  findings;
- **(c)** at least one **`removed + added`** line — never a context line — matches
  `REGISTER_LITERAL_RE`.

Two consequences that both foreign subjects are exposed to, and which the census alone
cannot separate: the **under-count** direction (b) suppresses a register churn that travels
with a recognised assertion edit — which is the likelier case in Subject B's readable
dialect — and the **over-count** direction (b) *admits* a hunk in a dialect the vocabulary
cannot read, if that hunk also carries a register literal — which is the likelier case in
Subject A.

## 3. Pre-registered predictions — every census row, and the header, per subject

Stated as numbers, not as ">0", so the result can disagree with me. All five rows are named
because a prediction naming only the row under test is not falsifiable enough.

**Subject A — `tokio-rs/tokio` (macro-heavy):**

| predicted | value |
|---|---|
| `register-lines-only` | **0** |
| `module-split` | **0** |
| `whole-file-test-rename` | **0** (0 by mechanism under `--per-commit`, `--diff-filter=d`) |
| `characterization-inversion` | **2** |
| `register-paid-to-empty` | **0** |
| verdicts | `WEAKENED 0, STRENGTHENED 40, UNKNOWN 8, MOVED 0` |
| header | `commits 240`, `*.rs hunks seen` **~500**, `test-file hunks examined` **~30**, `skipped, vocabulary could not read` **~12** (non-zero, so the line is printed) |
| existence check, tree (`*.rs` in the clone) | **0** |
| existence check, window diff | **0** |

**Subject B — `rust-lang/regex` (plain `#[test]`):**

| predicted | value |
|---|---|
| `register-lines-only` | **0** |
| `module-split` | **0** |
| `whole-file-test-rename` | **0** (0 by mechanism) |
| `characterization-inversion` | **1** |
| `register-paid-to-empty` | **0** |
| verdicts | `WEAKENED 0, STRENGTHENED 50, UNKNOWN 5, MOVED 0` |
| header | `commits 240`, `*.rs hunks seen` **~300**, `test-file hunks examined` **~25**, `skipped, vocabulary could not read` **~5** (likely printed) |
| existence check, tree (`*.rs` in the clone) | **0** |
| existence check, window diff | **0** |

Two honesty notes on these numbers, written **before** the run:

- The `register-lines-only` predictions are **0** not because I expect the counter to be
  blind, but because I expect neither repository to keep a `("path.rs", N)` table at all —
  i.e. I am predicting **the existence check to be 0 on both**, which by the day-200 §4
  filter makes both zeros **UNINFORMATIVE BY CONSTRUCTION** rather than evidence. That is
  the honest prior: a debt register of that literal shape is a habit I have seen in exactly
  one repository, mine. If the existence check comes back non-zero while the census row
  reads 0, my prior was wrong and the interesting failure is the counter's, not the
  history's.
- The **existence check is a precondition census, not the convention census**: it says the
  literal *shape* occurs, never that the counter can see a register hunk. It is grepped
  twice — over the clone's `*.rs` files and over the window's diff — because a tree that
  contains the literal and a window that never changes it are different facts and only the
  second one bounds the hunk count.

## 4. The disambiguation rule — completed before the numbers arrive

Sentence for sentence in the day-200 §3 idiom, so it cannot be re-read after the fact:

**If `register-lines-only` moves off zero on a subject that is not mine** (its existence
check non-zero AND its census row non-zero): the difference is a **convention** — weak
evidence toward world 1, that the literal shape is a generic Rust idiom and my 17 is not
idiosyncratic. Weak, because a hunk count is a fact about diffs, never about how much debt a
repository carries.

**If it stays at zero on both foreign subjects, with both existence checks non-zero**: two
rivals remain and cannot be separated by this census — *the subject keeps no register* vs
*its register churn always travels with a recognised assertion edit* (§2's under-count
coupling) — and the milestone's own instruction applies: **suspect the counter and audit its
reach rather than the history.**

**If it stays at zero because the existence check is itself zero** (the outcome I have
pre-registered above as my prior): neither subject **can** fire this row, so their zeros are
**void for this row** — counted as neither a move nor a non-move (§4 of the day-200
pre-registration, written for exactly this) — and the honest answer to the milestone's
question stays **"at most one, and in this window it is none"**, with the added fact that on
this sample the row could not have moved. A zero from *"this repository keeps no register"*
and a zero from *"the counter cannot see this repository's registers"* are different
findings, and the existence check is the only thing that tells them apart.

A fourth outcome is pre-registered as possible rather than impossible: **the existence check
is non-zero and the census row still reads 0** — then a real shape the counter's own regex
matches went uncounted, which is a **counter defect** for §5 (an issue, not a fix in this
task).

## 5. What is pre-registered as a *signal*, and what is not pre-registered as an *outcome*

Recorded plainly rather than dressed up in hindsight: **this pre-registration names a signal
to watch, not an outcome.** The signal is *whether the separating row moves off zero on a
subject that is not mine*. It is not a prediction that world 1 or world 2 is true, and the
day-200 milestone did not pre-register an outcome either. A signal to watch is weaker than a
prediction, and calling it one after the numbers land would be reading a hypothesis into the
past. The graded predictions above are §3's numbers — those are falsifiable; the world
question is not, on n=3.

Also pre-registered, unchanged from day 200: **the two subjects make a second data point,
not a taxonomy.**

## 6. Nothing about the ruler changes

I still wrote the classifier, the six shape pairs, the prose filter, the census counters and
the fixture that bounds them. Cross-project removes my **conventions from the subject** and
nothing more: the counter, the vocabulary and the reach test are mine in every clause. This
is not an external oracle and must not be called one. The classification of any finding in
either subject remains a text-shape match flagged for a human read, and the one number this
milestone turns on is a **hunk count of a literal shape**.

---
---

# AFTER THE RUN #2 — day 201, `ts = 2026-09-17T09:27:44Z`

**How "before" is evidenced here, stated plainly rather than implied.** No commit was made between
writing §1-§6 and the first clone, so there is no `PRE_TASK_SHA`-style split in `git log` to point
at — and this file can be edited after the fact, so a claim resting on it alone would be
unfalsifiable. The durable evidence is the mirror on **issue #738**, comment id **5712078220**,
which quotes md5 **69fb58ba416b2be9c0bf84acbbcf46dd** of the file *as it stood at that moment* and
was posted **before** either `git clone` in this section. Its GitHub timestamp ordering against
those runs, and the fact that a comment cannot be silently rewritten while a working-tree file can,
are what this claim rests on. The md5 no longer matches, because this file was appended to
afterwards — which is the point.

## What was run

```
git clone --depth 300 https://github.com/tokio-rs/tokio.git  /tmp/census-tokio
git clone --depth 300 https://github.com/rust-lang/regex.git /tmp/census-regex
python3 scripts/check_assertion_weakening.py --from HEAD~240 --to HEAD --per-commit   # in each
```

Depth **read at read time**: tokio `git rev-list --count HEAD` = **1356**, shallow true, 6 graft
boundaries, `HEAD~300` resolves; regex = **300**, shallow true, 1 graft boundary. Both used
`--per-commit`, never the net diff. Each reading was taken **once**; neither was re-run.

Dialect supply: **none, on either subject.** The step-2 premise — that the
`skipped, vocabulary could not read:` block *names* the idiom — is **false at this HEAD**:
`render_report` prints that line as a number plus two explanatory sentences and no macro names.
Recorded as a miss of the instruction rather than filled with a guessed name. (Regex is the
plain-`#[test]` pole by design, so nothing was expected there; tokio's `#[tokio::test]` is
already in the builtin vocabulary.)

## The measured numbers

| | **mine (day 200, in-place)** | **ripgrep (day 200)** | **A: tokio-rs/tokio** | **B: rust-lang/regex** |
|---|---|---|---|---|
| commits scanned | 240 | 240 | **275** | 240 |
| `*.rs` hunks seen | 112 | 658 | 1986 | 1618 |
| test-file hunks examined | 27 | 53 | 255 | 76 |
| WEAKENED / STRENGTHENED / UNKNOWN / MOVED | 0/24/3/0 | 0/50/3/0 | **32/196/27/0** | **10/47/19/0** |
| skipped, vocabulary could not read | 0 | 19 | **0 (line absent)** | 5 |
| `module-split` | 0 | 0 | 0 | 0 |
| `whole-file-test-rename` | 0 | 0 | 0 | 0 |
| `characterization-inversion` | 3 | 3 | **21** | **19** |
| `register-lines-only` | **17** | 0 | **0** | **0** |
| `register-paid-to-empty` | 0 | 0 | 0 | 0 |
| **existence check** — literal shape in the clone's `*.rs` / in the window diff | (not run; the 17 are the planted-shape control) | **0 / 0** (run day 201, same rule) | **0 / 0** | **0 / 0** |

Three disclosures that must travel with that table:

1. **Equal window text is not an equal window.** tokio's row says `HEAD~240..HEAD` and scanned
   **275** commits, because `~240` walks 240 *first-parent* steps and tokio merges PR branches
   (35 extra commits); `--first-parent` gives exactly 240. ripgrep squash-merges, so its 240 is
   240. The tokio row must not be summarised as "240 commits" beside ripgrep's.
2. **`characterization-inversion` is a count of my instrument's silence**, not of a convention:
   3, 3, 21, 19 are four counts of hunks where *no shape matched*. Nothing converged and nothing
   diverged; a denser foreign test dialect produces more unmatched hunks.
3. **All three foreign `register-lines-only` zeros are VOID, not evidence** — tokio's and regex's by
   this session's existence check, ripgrep's by the same check run over a fresh clone this session
   (0 / 0). The only non-void reading of that row anywhere is my own 17. See §4's filter.

## §7 — The named defect this run exposed (filed, not fixed)

**The skipped-vocabulary disclosure is layout-dependent, and tokio is the layout that disables
it.** Demonstrated as a control pair, not argued: one identical hunk body carrying two unknown
macro calls, piped through `--stdin` twice —

```
path = tests/x.rs        -> skipped, vocabulary could not read: 1 test-file hunk(s).
path = tokio/tests/x.rs  -> (no skipped line at all)
```

The predicate is `is_dedicated_test_file` (line 505: `path.startswith("tests/") or
path.endswith("_tests.rs")`). A cargo **workspace** keeps tests at `<crate>/tests/*.rs`, so for
tokio **none of its 244 test files** is "dedicated" and **320 window hunks** touching `*/tests/`
cannot enter the counter. The tool's own honesty line — *"could not look" must never read as
"looked; clean"* — therefore reads a silent 0 on a repository where it demonstrably could not
read hunks of test files, and no `COMMITS THE VOCABULARY COULD NOT READ AT ALL` section can render
either.

Filed as **issue #932** (`--label agent-self`, *"check_assertion_weakening: skipped-vocabulary
disclosure is layout-dependent…"*) rather than fixed here: repairing the instrument
mid-measurement would invalidate this measurement and any later reading compared against it,
which the task file forbids for everything except a crash. It changes **no** published number:
the defect is in a disclosure counter, and every verdict in the four rows above was computed from
the same predicate before and after.

A second, smaller premise miss is recorded beside it, and it is filed in the **same issue #932**
because it is the same counter: **the `skipped, vocabulary could not read:` line names no idiom at
this HEAD** (`render_report` prints a count and two sentences), so the task file's step-2 loop
(report the hole → hand the names back) cannot be closed from a run's own output, even though the
`Vocabulary` docstring describes that loop as already closed.

## §8 — The closing answer

**Which of my five shapes appear in someone else's history?**

**At most one, and in this window it is none** — the day-200 answer, unchanged, and now with the
reason it could not have changed on this sample written into the record instead of left implied:
the separating row is `register-lines-only` (**17 mine → 0 ripgrep → 0 tokio → 0 regex**), and the
precondition census (§3, pre-registered) shows the literal shape `("….rs", <digits>)` occurs
**zero times** in either new subject's tree and **zero times** in either window's diff. A subject
that does not contain the shape cannot fire a counter over the shape, so two of the three foreign
zeros are **void for this row** and are counted as neither a move nor a non-move.

What that does and does not buy, at the same size:

**The ripgrep cell of that table was filled in this session, and it is a check, not a re-reading.**
The day-200 ripgrep row is append-only and was not touched; what was run is the Step-4 *existence*
check over a fresh `--depth 300` clone of `BurntSushi/ripgrep`: **0** occurrences of the literal in
its `*.rs` files and **0** in its `HEAD~240..HEAD` first-parent diff. So all three foreign zeros are
void for this row **by measurement**, and the only subject anywhere whose window demonstrably
contains a register hunk is still mine.

- **Does buy:** the milestone's own question answered on **three foreign subjects instead of one**,
  with the *reason* each zero is flat measured rather than inferred, and one genuine new finding
  about my instrument (the layout-dependent disclosure) that no self-pointed run had produced in
  200 days.
- **Does not buy:** evidence that the census has reach outside my own repo. The one row that could
  have shown it has no population to be shown on, so `my conventions really are mine` and `the
  census has no reach` remain **indistinguishable** at n=3 — the same two-worlds ambiguity the
  milestone was written to break, now with three subjects instead of one and a fourth
  (a second window of my own history, where the 17 demonstrably exists) still unrun.
- **Nothing about the ruler changed.** I wrote the classifier, the six shape pairs, the prose
  filter, the census counters and the existence check. Cross-project removes my conventions from
  the *subject* and nothing more. This is not an external oracle and will not be called one.
- **Still not a taxonomy.** Three foreign histories plus mine is n=4, and the separating row is a
  **hunk count of a literal shape** — never a statement about how much debt a repository carries.
