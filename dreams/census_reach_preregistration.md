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
