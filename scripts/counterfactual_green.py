#!/usr/bin/env python3
"""Was this green EARNED? Run the counterfactual backwards over my own history.

WHY THIS EXISTS
---------------
Mutation testing (readings #1-#8, CLAUDE.md) answered *would a future break be caught?*
It never asked **was this green earned?** 123 of my 156 graded risk events are green days,
so four fifths of what my self-model has learned is a claim `cargo test` makes about the
*absence* of a defect — and I wrote that ruler, in the same session, in the same act, as
the code it judges. `scripts/evolve.sh` allows up to **10 build-fix attempts** and then up
to **9 eval-fix attempts** on a rejected task, and nothing in that loop forbids satisfying
the gate by loosening an assertion instead of fixing the code.

`scripts/check_assertion_weakening.py` is the **static diff** half: it lists deleted and
loosened assertions. greenproof's README draws the line I had missed by name — *"the
static diff is not a proof. The verdict is what to act on."* This file is the verdict half.

THE MECHANISM
-------------
For a task commit `C` with parent `P`, the counterfactual tree is **post-task `src/` plus
pre-task `tests/`**. Run `cargo test` on it. Nothing needs to be snapshotted forward:
every task commit has a parent, so the pre-task tests are already in git. If the code
fails the tests it started with while the shipped tree is green, the green came from the
test edits rather than from the code.

THE COMPILE-VS-ASSERT DISCRIMINATOR *IS* THE DESIGN
---------------------------------------------------
`cargo test` exits non-zero for **both** a compile failure and an assertion failure, so
the classifier must read the **captured output**, never the exit code alone. Without that
discriminator `UNEARNED` is arithmetically **unreachable** — every failure would land in
one bucket — and an unreachable branch inside my own meter is skill-evolve's evt-0011
verbatim (`score < 0.3` was unreachable, so `retire` never fired across 16 events). Every
state below therefore carries its own self-test row: reachability is *asserted*, not
assumed.

SEVEN STATES, NONE FOLDED INTO ANOTHER
--------------------------------------
`NO_TEST_CHANGE` (nothing to counterfactual — the counterfactual tree *is* the shipped
tree, and this must never be counted as EARNED), `EARNED`, `UNEARNED`, `INCONCLUSIVE`
(pre-tests do not compile: an honest API rename and a hidden break are indistinguishable
here), `BASELINE_RED` (see below), `REGISTER_DRIFT` (#867 — every failing test lives in a
file whose pre->post diff was pure register bookkeeping, so the red was manufactured by
the overlay; VOID, not clean, and never counted in either column), and `COULD_NOT_CHECK`
(worktree/checkout/cargo/timeout failure), which is never folded into any verdict — the
same refusal the pre-push hook and `CiScan`'s could-not-run branch already make.

This heading read "SIX STATES" for one session after `REGISTER_DRIFT` landed, and "FIVE
STATES" for one session after `BASELINE_RED` landed. A hand-maintained count beside a
machine-maintained tuple goes stale silently in exactly one direction — downward — so
`len(RUN_VERDICTS)` is what the self-tests assert and what `--test` prints; this prose is
a reader's summary of it and never the authority.

THE BASELINE GATE, AND WHY IT IS NOT OPTIONAL
---------------------------------------------
The first version of this tool ran pre-task `tests/` against post-task `src/` and read the
result. It never established that those pre-task tests passed against pre-task `src/`.
Without that baseline an `UNEARNED` verdict is **unfalsifiable**: a flaky test, environment
drift, a feature-gated file or a shallow-clone artifact produces exactly the same red as a
genuine unearned green, and it would have been published as a finding.

So the parent is run **whole** first — pre-task `tests/` AND pre-task `src/` — through the
same worktree, the same `cargo test`, the same `CARGO_TARGET_DIR`. If it is not a control
run the same way, it is not a control. A red baseline short-circuits: the counterfactual is
skipped entirely and the verdict is `BASELINE_RED`, which is a third fact distinct from
both neighbours — `UNEARNED` means the code fails an assertion it started with,
`COULD_NOT_CHECK` means the machinery broke, and `BASELINE_RED` means the machinery worked
fine and **the reference point is broken**, so nothing can be concluded either way.

This is the dominant documented failure mode of the method, not a hypothetical: arXiv
2606.16062 audited an LLM-judge loop reporting 10 of 11 tasks fixed, and Docker
re-verification of 8 sampled cases found **6 invalid — the generated tests did not run on
the gold solution at all**. Their sentence is the one that matters: *"The judge correctly
read the test code and reasoned about what it would check **if it ran**."* Their gold-sanity
gate caught a 61.9% per-augmentation defect rate the judge had missed. Applied here it is
my own rule — *"could not check" must never read as "checked; clean"* — one layer below
where I had already applied it.

THREE LANDMINES, EACH FATAL IF IGNORED
--------------------------------------
1. **Never `git checkout` in the live working tree.** The harness reverts a failed task
   with `git reset --hard`, so a script that mutates `tests/` in place can destroy the
   session that is running it. Everything happens in `git worktree add --detach` under
   `tempfile.mkdtemp()`, never under the repo, with `git worktree remove --force` in a
   `finally`.
2. **The scratch run sets its own `CARGO_TARGET_DIR`.** #832: a nested cargo build over
   the shared `target/debug/yoyo` clobbers the binary every integration test resolves
   through `env!("CARGO_BIN_EXE_yoyo")`. It reddened `main` for three sessions and read as
   flakiness. Sharing the target dir "to save time" reproduces that defect exactly.
3. **Nothing here may be invoked from a `#[test]`.** The standing rule (#832, enforced by
   `tests/cargo_spawning_tests.rs`) is that no `#[test]` under `src/` may spawn cargo.
   This is a hand-run script: it is not wired into `cargo test`, no Rust test shells it,
   and it takes no register entry.

SELF-CONTAMINATION, CHECKED FORWARD
-----------------------------------
`scripts/measure_abstentions.py` exists because a meter I wrote matched my own prose
*about* the meter. Asked forward here: this file contains the literal strings
`error[E0425]` and `test result:` in its own source and self-tests. The anchor is that the
classifier only ever reads **captured subprocess output**, never a file and never a diff —
so a `.py` script or a `.md` document cannot enter the denominator however much cargo
vocabulary it carries. Compile markers are additionally anchored to **column 0 of a line**,
because that is where rustc emits them and a panic message quoting one is indented.

SHAPE
-----
Pure decision functions (`classify_counterfactual`, `census_from_log`, `has_compile_error`
and the small predicates they call) with all I/O at the call sites, a real `argparse`
whose `--help` works (`scripts/measure_abstentions.py` shipped without one and raised
`FileNotFoundError: '--help'` for a whole day — not repeating that), a `--test` flag that
exits non-zero on failure, and the stated limits printed on every run including a clean one.

TASK_COMMIT_RE IS A SECOND COPY, DELIBERATELY
---------------------------------------------
`scripts/extract_trajectory.py:219` carries the same regex. Sharing it would mean either
importing a 4,400-line module (which shells `gh` and `yoyo` at import-adjacent call sites)
or extracting a third shared module for one 2-line pattern. The copy is pinned by a
self-test that feeds it real subjects from this repo's own log, so drift fails loudly here
rather than silently shrinking the census denominator.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

# --------------------------------------------------------------------------------------
# The seven states. None folds into another.
# --------------------------------------------------------------------------------------

NO_TEST_CHANGE = "NO_TEST_CHANGE"
EARNED = "EARNED"
UNEARNED = "UNEARNED"
INCONCLUSIVE = "INCONCLUSIVE"
COULD_NOT_CHECK = "COULD_NOT_CHECK"
BASELINE_RED = "BASELINE_RED"
REGISTER_DRIFT = "REGISTER_DRIFT"

# The six a live run can produce. NO_TEST_CHANGE is decided by the diff, before any run.
RUN_VERDICTS = (
    EARNED,
    UNEARNED,
    INCONCLUSIVE,
    COULD_NOT_CHECK,
    BASELINE_RED,
    REGISTER_DRIFT,
)

# Internal sentinel, deliberately NOT a run verdict: the baseline passed, so the
# comparison is licensed and the counterfactual may proceed. It never reaches a report.
BASELINE_OK = "BASELINE_OK"

# Second copy of scripts/extract_trajectory.py:219 — see the module doc for why.
#
# WIDENED (Day 184, #868), and the widening IS the deliverable. The pattern used to end
# `\(Task\s+\d+\)`, anchoring the subject to STOP at the task number — so every
# `(Task 1, eval-fix 2)` / `(Task 2, build-fix 3)` retry commit matched NOTHING. Measured
# over the Day-184 deepened window: 434 plain task commits visible, 184 fix-loop commits
# invisible, and 0 of the 20 behavioural commits was a fix-loop commit.
#
# That is the worst direction a sampling bug can run: it excluded exactly the population
# DREAM.md's milestone is ABOUT (the pre-registered guess is that fix-loop pressure is
# where unearned green lives) while the surviving sample looked healthy. The optional
# `(?:,\s*[^)]+)?` group admits the suffix; group 3 CAPTURES it, so the classifier reads a
# parsed value rather than re-scanning the raw subject with a second pattern.
TASK_COMMIT_RE = re.compile(
    r"^Day\s+(\d+)\s+\([^)]+\):\s+(.+?)\s+\(Task\s+\d+(?:,\s*([^)]+))?\)\s*$"
)

# The two suffix markers `scripts/evolve.sh` writes when a task needed the fix loop, and
# the two DREAM.md names. Matched as substrings of the CAPTURED suffix only — never of the
# whole subject, or a task whose title happens to discuss "eval-fix" would misclassify
# itself. That is the same self-contamination trap `measure_abstentions.py` was built
# around: my own prose about a marker must never score as the marker.
FIX_LOOP_MARKERS = ("eval-fix", "build-fix")

# SUPERSEDED (Day 183, #867), recorded rather than erased: this was
#     REGISTER_TEST_FILES = frozenset({"tests/module_size.rs"})
# a hand-listed set of "debt register" test files. It was a per-FILE filter while the real
# property is per-ASSERTION, and it was the `MECHANICAL_SUBJECTS` / `GLOBAL_SETTERS` shape:
# a hand-written list with no authority behind it, which goes stale silently and needs its
# own drift guard to stay honest. It also could not express the case that produced this
# instrument's only live reading — tests/git_chokepoint.rs mixes a debt register AND a
# two-direction ratchet BESIDE 12 genuinely behavioural tests, so excluding the file loses
# the 12 and including it manufactures an UNEARNED out of bookkeeping.
#
# What replaced it is DERIVED FROM THE DIFF OF THE COMMIT UNDER TEST — see
# `test_diff_is_register_only` below. That covers tests/module_size.rs too (its diffs are
# register-only by construction), so the hand-list is redundant as well as stale-prone.

# A changed line can only carry an assertion if something SURVIVES stripping its string
# literals and comments. These are the residue characters that cannot: tuple scaffolding,
# separators, and the bare integers a line-count register is made of.
REGISTER_RESIDUE_CHARS = set("(),; \t0123456789")

# rustc/cargo compile-failure markers, anchored to column 0 of a line because that is
# where rustc emits them. A panic message that quotes one is indented and must not match.
COMPILE_ERROR_RES = (
    re.compile(r"^error\[E\d+\]", re.MULTILINE),
    re.compile(r"^error: could not compile", re.MULTILINE),
    re.compile(r"^error: aborting due to", re.MULTILINE),
    re.compile(r"^error: expected ", re.MULTILINE),
    re.compile(r"^error: cannot find ", re.MULTILINE),
)

# libtest's own summary lines.
TEST_RESULT_OK_RE = re.compile(r"^test result: ok\.", re.MULTILINE)
TEST_RESULT_FAILED_RE = re.compile(r"^test result: FAILED\.", re.MULTILINE)

# Default ceiling for one counterfactual `cargo test`. A judgment threshold, not a
# measurement: a cold scratch target dir builds the whole crate from zero, and past this
# point an honest COULD_NOT_CHECK beats an open-ended wait.
DEFAULT_TIMEOUT_SECS = 1800

# Ceiling for one bounded `git fetch --deepen`. Generous relative to the fetch it guards
# (a few hundred commits of this repo) and finite on purpose: an instrument that can hang
# is an instrument nobody runs.
DEEPEN_TIMEOUT_SECS = 600


def has_compile_error(output: str) -> bool:
    """True when the captured output carries a rustc compile-failure marker."""
    return any(rx.search(output) for rx in COMPILE_ERROR_RES)


def classify_counterfactual(exit_code: int, output: str) -> str:
    """Classify one counterfactual `cargo test` run from its exit code AND its output.

    The output is load-bearing: `cargo test` exits non-zero for both a compile failure and
    an assertion failure, so an exit-code-only reading makes UNEARNED unreachable.

    Precedence, in order, and the first rule is the one that matters:

    1. A compile error anywhere => INCONCLUSIVE, **even if a `test result:` line is also
       present**. A build that did not finish cannot have run the assertions it is being
       judged on: cargo compiles and runs test binaries one at a time, so an earlier
       target can report `ok` while a later one fails to build. Judging that as UNEARNED
       would convict on assertions that never ran, and as EARNED would acquit on a
       fraction of the suite.
    2. `test result: FAILED` or a non-zero exit with no compile error => UNEARNED.
    3. Exit 0 with at least one `test result: ok.` => EARNED.
    4. Exit 0 with no test-result line at all => COULD_NOT_CHECK, not EARNED. A run that
       compiled and executed zero tests proves nothing; calling it earned is the
       anti-vacuous defect wearing its quietest face.
    """
    if has_compile_error(output):
        return INCONCLUSIVE
    if TEST_RESULT_FAILED_RE.search(output):
        return UNEARNED
    if exit_code != 0:
        return UNEARNED
    if TEST_RESULT_OK_RE.search(output):
        return EARNED
    return COULD_NOT_CHECK


def classify_baseline(exit_code: int, output: str) -> str:
    """Classify the BASELINE run: parent `tests/` against parent `src/`, whole.

    WHY A BASELINE AT ALL
    ---------------------
    Without it an UNEARNED verdict is **unfalsifiable**. The counterfactual runs pre-task
    `tests/` against post-task `src/` and reads the result — but a flaky test, environment
    drift, a feature-gated file or a shallow-clone artifact produces exactly the same red
    as a genuine unearned green, and nothing here could tell them apart. Establishing that
    the reference point is itself green is what converts "the tests fail" into "the tests
    fail *because of the code change*".

    This is independently the dominant failure mode of the whole method. arXiv 2606.16062
    audited an LLM-judge loop reporting 10 of 11 tasks fixed; Docker re-verification of 8
    sampled cases found **6 invalid — the generated tests did not run on the gold solution
    at all**. Their sentence is the one that matters: *"The judge correctly read the test
    code and reasoned about what it would check **if it ran**."* Their gold-sanity gate
    caught a 61.9% per-augmentation defect rate the judge had missed.

    It is also my own standing rule — *"could not check" must never read as "checked;
    clean"* — applied one layer below where I had already applied it.

    THREE OUTCOMES, AND THE COMPILE-VS-ASSERT SPLIT IS THE SAME ONE AS ABOVE
    -----------------------------------------------------------------------
    1. A compile error => COULD_NOT_CHECK, **not** BASELINE_RED. A build that did not
       finish cannot have run the assertions it is being judged on, so this is the
       machinery failing, not the reference point being broken. Same discriminator, same
       reason, as `classify_counterfactual`'s rule 1.
    2. `test result: FAILED`, or a non-zero exit with no compile error => BASELINE_RED.
       The machinery worked and the reference point is broken: nothing can be concluded
       about this commit in **either** direction.
    3. Exit 0 with at least one `test result: ok.` => BASELINE_OK, the comparison is
       licensed. Exit 0 with no test-result line at all => COULD_NOT_CHECK, because a run
       that compiled and executed zero tests is not a green baseline; it is no baseline.
    """
    if has_compile_error(output):
        return COULD_NOT_CHECK
    if TEST_RESULT_FAILED_RE.search(output):
        return BASELINE_RED
    if exit_code != 0:
        return BASELINE_RED
    if TEST_RESULT_OK_RE.search(output):
        return BASELINE_OK
    return COULD_NOT_CHECK


# --------------------------------------------------------------------------------------
# Register-drift attribution. Derived from the diff, never from a hand-listed filename.
# --------------------------------------------------------------------------------------


def strip_rust_strings_and_comments(line: str, in_string: bool) -> tuple[str, bool]:
    """Remove string literals and `//` comments from one line. Returns (residue, state).

    `in_string` carries across lines because a Rust string literal may be continued with a
    trailing backslash, which is exactly how a debt register writes a long `reason` field:

        "NO structural blocker — this duplicates run_git_in_dir(toplevel, [...]) \\
         exactly and is a genuine conversion candidate.",

    The second line has no quote character at all and is pure string content. Without the
    carried state it would look like bare prose and read as substantive code.

    Escapes are honoured (`\\"` does not close a string). Raw strings are NOT modelled: a
    `r#"..."#` is treated as an ordinary quote, which can only make the residue LARGER and
    therefore only ever pushes a verdict toward UNEARNED. That is the safe direction, and
    it is deliberate rather than an oversight.
    """
    out = []
    i = 0
    n = len(line)
    while i < n:
        ch = line[i]
        if in_string:
            if ch == "\\":
                i += 2
                continue
            if ch == '"':
                in_string = False
            i += 1
            continue
        if ch == '"':
            in_string = True
            i += 1
            continue
        if ch == "/" and i + 1 < n and line[i + 1] == "/":
            break  # `//`, `///` and `//!` all end the line
        out.append(ch)
        i += 1
    # A trailing backslash OUTSIDE a string is not a continuation in Rust, so state only
    # persists when the quote was genuinely left open.
    return "".join(out), in_string


def is_register_literal_line(line: str, in_string: bool = False) -> bool:
    """True when this changed line CANNOT carry an assertion.

    The rule is one sentence: strip the line's string literals and comments, and ask
    whether anything but tuple scaffolding survives. A debt-register entry is a tuple of
    literals, so its residue is punctuation and digits:

        ("src/cli.rs", 5349),               -> `(, ),`  + digits   -> True
        (                                    -> `(`                 -> True
            "list_project_files",            -> `,`                 -> True
        ),                                   -> `),`                -> True
        //! ...prose...                      -> ``                  -> True
        (blank)                              -> ``                  -> True

    while anything that could fail is made of identifiers:

        assert_eq!(got, "x");                -> `assert_eq!(got, );` -> False
        let n = count_rs_lines(p);           -> `let n = (p);`       -> False
        fn every_site_is_registered() {      -> `fn ...() {`         -> False

    `in_string` is the carried state from `strip_rust_strings_and_comments`; the default
    exists so a single line can be asked about on its own.
    """
    residue, _ = strip_rust_strings_and_comments(line, in_string)
    return all(c in REGISTER_RESIDUE_CHARS for c in residue)


def test_diff_is_register_only(diff_text: str) -> bool:
    """True when ONE test file's diff changed nothing but debt-register bookkeeping.

    This is the derivation that replaced the hand-listed `REGISTER_TEST_FILES`. It is a
    property of the commit under test, computed from `git diff P..C -- tests/`, i.e. data
    the run already has in hand.

    Two conditions, and both are load-bearing:

    1. Every added and every removed line is `is_register_literal_line`. Context lines and
       the `---`/`+++`/`@@` headers are not changes and are skipped. The two sides carry
       SEPARATE string state, because `-` and `+` lines interleave in the hunk body and
       threading one state through both would let a removed open-quote swallow an added
       assertion.
    2. At least one paren appears in the changed residue. This is the near-miss guard, and
       it exists for one specific hole: a multi-line `assert_eq!` whose only changed line
       is its expected string,

           assert_eq!(
               got,
               "expected",     <- the only changed line
           );

       strips to `,`, which passes condition 1 on its own. Requiring evidence that a TUPLE
       was actually touched rejects that block, because it carries no paren at all.

    EMPTY DIFF IS FALSE, NOT TRUE. "Nothing changed" is not "only bookkeeping changed", and
    folding them would let a file with no diff launder a real failure into a void verdict.

    THE RESIDUAL HOLE, STATED RATHER THAN IMPLIED: a commit that changes BOTH a register
    tuple AND an assertion's expected string, in the SAME file, passes — the register's
    paren vouches for the string line. It is narrow, and every other branch of the
    attribution runs the conservative way (an unmappable or non-attributable failing test
    is never register drift), so a real failure still has to slip past all of them.
    """
    added_state = False
    removed_state = False
    changed = 0
    saw_paren = False

    for line in diff_text.splitlines():
        if line.startswith(("+++", "---")):
            continue
        if line.startswith("+"):
            body = line[1:]
            entry_state = added_state
            residue, added_state = strip_rust_strings_and_comments(body, added_state)
        elif line.startswith("-"):
            body = line[1:]
            entry_state = removed_state
            residue, removed_state = strip_rust_strings_and_comments(body, removed_state)
        else:
            # Context, `@@` headers, `\ No newline at end of file`. Not a change.
            continue
        changed += 1
        # ONE STATEMENT OF THE RULE, and this is why the strip happens twice. The loop
        # needs the residue itself (for the paren evidence below) and the carried state;
        # the JUDGMENT "can this line hold an assertion?" is asked of
        # `is_register_literal_line` rather than re-spelled here. A second inline copy of
        # `all(c in REGISTER_RESIDUE_CHARS ...)` would agree the day it was written and
        # drift forever after — the defect this whole file's derivation replaced. Both
        # calls strip the same body from the same entry state through the same pure
        # function, so they cannot disagree.
        if not is_register_literal_line(body, entry_state):
            return False
        if "(" in residue or ")" in residue:
            saw_paren = True

    return changed > 0 and saw_paren


def failing_test_names(output: str) -> list[str]:
    """Names of the tests libtest reported as failing, from the captured run output.

    ONE parser, not two: `classify_counterfactual` reads the same capture for its
    `test result:` lines, and a second reader of the same text is how two answers about
    one run start to disagree.

    Both shapes libtest emits are read — the streaming `test NAME ... FAILED` line and the
    `failures:` block that lists them again — and the result is deduped, so a test named in
    both is one failure, not two.
    """
    names = []
    seen = set()

    def add(name: str) -> None:
        name = name.strip()
        if name and name not in seen:
            seen.add(name)
            names.append(name)

    for m in re.finditer(r"^test\s+(\S+)\s+\.\.\.\s+FAILED", output, re.MULTILINE):
        add(m.group(1))

    # The trailing `failures:` block indents each name by four spaces.
    in_block = False
    for line in output.splitlines():
        if re.match(r"^failures:\s*$", line):
            in_block = True
            continue
        if in_block:
            if line.startswith("    ") and line.strip() and "::" not in line[:4]:
                candidate = line.strip()
                if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_:]*", candidate):
                    add(candidate)
                    continue
            if line.strip() == "":
                continue
            in_block = False
    return names


def attribute_failures(
    failing: list[str],
    pre_test_sources: dict,
    register_only: dict,
) -> tuple[bool, str]:
    """Is EVERY failing test attributable to register drift? Returns (verdict, reason).

    `pre_test_sources` maps `tests/x.rs` -> its PRE-task contents (the tree that was
    actually run). `register_only` maps `tests/x.rs` -> bool from
    `test_diff_is_register_only`.

    THE CONSERVATIVE DIRECTION IS THE SAFETY PROPERTY, AND IT IS WRITTEN HERE SO NOBODY
    "SIMPLIFIES" IT LATER: a failing test name that maps to ZERO files, or to MORE THAN
    ONE, is NOT register drift. An unattributable failure must never be laundered into
    bookkeeping — that would convert a real unearned green into a void verdict, which is
    the one direction this whole instrument exists to avoid.

    Likewise a failure set that is EMPTY is not register drift: something failed, and if
    the parser could not name it we do not get to say what it was.
    """
    if not failing:
        return False, "no failing test could be named in the capture"

    for name in failing:
        owners = [
            path
            for path, src in pre_test_sources.items()
            if re.search(r"\bfn\s+" + re.escape(name) + r"\s*\(", src)
        ]
        if len(owners) != 1:
            return False, (
                f"{name!r} maps to {len(owners)} pre-task test file(s) — not attributable"
            )
        if not register_only.get(owners[0], False):
            return False, (
                f"{name!r} lives in {owners[0]}, whose diff is not register-only"
            )
    return True, "every failing test lives in a file whose diff was register-only"


def apply_register_drift(
    verdict: str,
    output: str,
    pre_test_sources: dict,
    register_only: dict,
) -> tuple[str, str]:
    """The #867 decision, as one pure function: does this verdict become REGISTER_DRIFT?

    Returns `(verdict, why)`, where `why` is the empty string when nothing changed.

    THE PRECEDENCE IS THE SAFETY PROPERTY AND IT IS ENCODED HERE, NOT AT THE CALL SITE.
    The branch fires ONLY on `UNEARNED`. `INCONCLUSIVE` (pre-tests did not compile against
    post-src/) and `BASELINE_RED` (the reference point is itself broken) therefore keep
    absolute precedence untouched, because a build that did not finish cannot have run the
    assertions it is being judged on. `EARNED`, `NO_TEST_CHANGE` and `COULD_NOT_CHECK`
    likewise pass through byte-identically — a green is never rewritten into a void.

    It lives beside `attribute_failures` rather than inline in `run_counterfactual` for
    one reason: `run_counterfactual` shells two cargo invocations, so anything inlined
    there is unpinnable by a self-test. This is the composition an actual run performs,
    and it is exercised by the same rows a live read would take.
    """
    if verdict != UNEARNED:
        return verdict, ""
    failing = failing_test_names(output)
    drift, why = attribute_failures(failing, pre_test_sources, register_only)
    if not drift:
        return UNEARNED, why
    return REGISTER_DRIFT, why + ". Failing: " + ", ".join(failing)


# --------------------------------------------------------------------------------------
# Census — the deliverable. Pure half here, git I/O at the call site.
# --------------------------------------------------------------------------------------

# DREAM.md's milestone asks for the rate "reported **separately** for commits whose
# subject carries an `eval-fix` or `build-fix` suffix". These are those two populations,
# plus the third value that keeps an unrecognised suffix from being absorbed by whichever
# neighbour is convenient (Day 144: absence, and anything I cannot classify, gets its own
# name). They are NEVER summed: a fix-loop commit and a first-attempt commit are different
# facts, and pooling them would make the widened pattern actively worse than the narrow
# one it replaced — it would add 184 commits to a denominator whose whole purpose is to be
# split.
POP_PLAIN = "plain"
POP_FIX_LOOP = "fix-loop"
POP_UNKNOWN_SUFFIX = "unknown-suffix"


def subject_population(subject: str) -> str | None:
    """Which population does this commit subject belong to? `None` if not a task commit.

    THREE states, none folded into another:

      * `POP_PLAIN`        — a task commit with no suffix: a first-attempt delivery.
      * `POP_FIX_LOOP`     — the suffix carries `eval-fix` or `build-fix`, i.e. the task
                             needed the fix loop. This is DREAM.md's pre-registered
                             population.
      * `POP_UNKNOWN_SUFFIX` — a suffix is present and matches neither marker. NOT called
                             fix-loop (that would be a claim I cannot support) and NOT
                             called plain (it demonstrably carries something), so it gets
                             its own count and is reported apart. A suffix shape nobody
                             enumerated is exactly the kind of thing that should surface
                             rather than join the comfortable bucket.

    The marker test runs against the CAPTURED suffix, never the whole subject: a task
    whose title discusses "build-fix" (this very session's does) must not classify itself
    as a fix-loop commit. That is the self-contamination trap — my own prose about a
    marker scoring as the marker — that `measure_abstentions.py` exists because of.
    """
    m = TASK_COMMIT_RE.match(subject)
    if not m:
        return None
    suffix = m.group(3)
    if not suffix:
        return POP_PLAIN
    if any(marker in suffix for marker in FIX_LOOP_MARKERS):
        return POP_FIX_LOOP
    return POP_UNKNOWN_SUFFIX


class CensusRow:
    """One task commit and whether it is addressable by the counterfactual."""

    __slots__ = ("sha", "subject", "test_files", "register_only", "population")

    def __init__(self, sha: str, subject: str, test_files: list[str],
                 register_only: frozenset | set | None = None,
                 population: str | None = None):
        self.sha = sha
        self.subject = subject
        self.test_files = test_files
        # Paths whose diff in THIS commit was register-only, derived by
        # `test_diff_is_register_only`. Never a hand-listed filename.
        self.register_only = frozenset(register_only or ())
        # Which of DREAM.md's two populations this commit belongs to. Derived from the
        # subject by `subject_population`, carried rather than recomputed so the row and
        # the fold cannot disagree about the same commit.
        self.population = population or subject_population(subject) or POP_PLAIN

    @property
    def is_fix_loop(self) -> bool:
        """True iff the subject carries an `eval-fix` / `build-fix` suffix.

        This is the split DREAM.md's milestone pre-registers: the guess is that fix-loop
        pressure — 10 build-fix then 9 eval-fix attempts, with nothing in the loop
        forbidding a pass bought by loosening an assertion — is where unearned green
        lives. An unrecognised suffix is deliberately NOT True here (see
        `subject_population`); it is counted apart and never summed into either side.
        """
        return self.population == POP_FIX_LOOP

    @property
    def addressable(self) -> bool:
        return bool(self.test_files)

    @property
    def behavioural(self) -> bool:
        """True when at least one touched test file's diff was NOT pure bookkeeping.

        This is the denominator that carries signal. A commit whose only test-file changes
        are debt-register entries is addressable in the mechanical sense and worthless in
        the evidential one, because its counterfactual verdict is decided by construction.

        DERIVED, NOT NAMED (Day 183, #867): the split used to be a hand-listed set of
        filenames, which could not express a file that is BOTH — tests/git_chokepoint.rs
        carries a debt register beside 12 behavioural tests. The question is now asked of
        the commit's diff, per file.
        """
        return any(p not in self.register_only for p in self.test_files)

    def __repr__(self) -> str:  # pragma: no cover - debugging aid
        return f"CensusRow({self.sha[:8]}, addressable={self.addressable})"


def parse_log_lines(lines: list[str]) -> list[tuple[str, str]]:
    """Return [(sha, subject), ...] for lines whose subject is a task commit.

    Input lines are `<sha>\\t<subject>` as emitted by `git log --format=%H%x09%s`.
    """
    out = []
    for line in lines:
        if "\t" not in line:
            continue
        sha, _, subject = line.partition("\t")
        sha = sha.strip()
        subject = subject.strip()
        if not sha or not TASK_COMMIT_RE.match(subject):
            continue
        out.append((sha, subject))
    return out


def top_level_test_files(paths: list[str]) -> list[str]:
    """Filter to top-level `tests/*.rs` — never `tests/common/mod.rs`, never `src/`.

    The scope limit is deliberate and is stated in the report: unit tests live inside 91
    `src/` files behind `#[cfg(test)]` and cannot be lifted out without dragging the
    production code along, so they are structurally outside this instrument.
    """
    out = []
    for p in paths:
        p = p.strip()
        if not p.startswith("tests/") or not p.endswith(".rs"):
            continue
        if p.count("/") != 1:  # tests/common/mod.rs and friends are not top-level
            continue
        out.append(p)
    return out


def behavioural_test_files(paths: list[str], register_only=None) -> list[str]:
    """The subset of top-level test files that are EVIDENCE rather than BOOKKEEPING.

    THIS IS THE SET THAT MAY BE ROLLED BACK, and the distinction is not cosmetic. A debt
    register encodes line counts OF `src/`, so laying a pre-task
    copy over post-task `src/` fails **by construction** whenever a module grew — the gate
    working exactly as designed, since pasting the updated register line is the compliant
    remedy it prints. Rolling one back would manufacture an `UNEARNED` out of bookkeeping,
    which is the same defect the census already refuses to make when it splits
    register-only commits out of the behavioural denominator.

    DERIVED FROM THE DIFF (Day 183, #867), never from a hand-listed filename: `register_only`
    maps each touched path to `test_diff_is_register_only` for THIS commit. The hand-list
    could not express tests/git_chokepoint.rs, which carries a debt register AND a
    two-direction ratchet beside 12 genuinely behavioural tests — excluding the file loses
    the 12, including it manufactures an UNEARNED out of bookkeeping.

    Register files are left at their SHIPPED (post-task) version, so the gate matches the
    `src/` it is measuring and only behavioural assertions are counterfactualled.
    """
    return [p for p in top_level_test_files(paths) if p not in (register_only or ())]


def census_summary(rows: list[CensusRow]) -> dict:
    """Fold census rows. Anti-vacuous: zero task commits is a refusal, not a zero.

    Three tiers, and `behavioural` is the one that carries signal. `register_only` is
    reported separately rather than summed into `addressable`, because folding it in
    produces a flattering headline over commits whose counterfactual verdict is decided
    by construction (see `test_diff_is_register_only`).
    """
    return {
        "task_commits": len(rows),
        "addressable": sum(1 for r in rows if r.addressable),
        "behavioural": sum(1 for r in rows if r.behavioural),
        "register_only": sum(1 for r in rows if r.addressable and not r.behavioural),
        "not_addressable": sum(1 for r in rows if not r.addressable),
    }


def census_by_population(rows: list[CensusRow]) -> dict:
    """Fold the SAME five figures once per population, and never sum them (Day 184, #868).

    DREAM.md's milestone asks for the rate "reported **separately** for commits whose
    subject carries an `eval-fix` or `build-fix` suffix" — that split IS the pre-registered
    guess, that fix-loop pressure is where unearned green lives. Pooling the two would
    destroy the only question the widened `TASK_COMMIT_RE` was widened to ask, so the fold
    is per-population and there is deliberately no combined figure anywhere in the return.

    `POP_UNKNOWN_SUFFIX` gets its own entry for the same reason: a suffix shape nobody
    enumerated must not be absorbed into whichever neighbour is convenient. Its count is
    normally 0, and the renderer stays silent when it is — but a silent absorption is the
    defect this whole family of checks is about, so the bucket exists before it is needed.
    """
    return {
        pop: census_summary([r for r in rows if r.population == pop])
        for pop in (POP_PLAIN, POP_FIX_LOOP, POP_UNKNOWN_SUFFIX)
    }


# --------------------------------------------------------------------------------------
# Window depth. The census denominator is bounded by how much history is reachable, and
# on this harness that is a per-run fact: `scripts/evolve.sh` checks out shallow and is
# protected, so a `git fetch --deepen` typed by hand is gone by the next session. The
# enabler therefore lives HERE, on my side of that boundary (the standing "an enabler on
# my side of every immovable boundary" rule), rather than in an invocation I retype --
# "any token added to an invocation purely to make it run is an unfiled bug report".
# --------------------------------------------------------------------------------------

# Four states, and none is folded into a neighbour. "I did not ask", "there was nothing
# to ask for", "I asked and got more" and "I asked and got nothing" are four different
# facts ABOUT THE DENOMINATOR BELOW, and only the last one means the reported window is
# smaller than the one requested. Folding DID_NOT_TAKE into TOOK would be "could not
# check" reading as "checked; clean" inside the one instrument built to stop me
# over-claiming.
DEEPEN_NOT_REQUESTED = "not-requested"
DEEPEN_ALREADY_DEEP = "already-deep"
DEEPEN_TOOK = "took"
DEEPEN_DID_NOT_TAKE = "attempted-did-not-take"

# The prefix every deepen STATUS line carries, stated ONCE so the renderer and the
# near-miss guard that checks for its absence cannot drift apart. Two hand-written copies
# agree the day they are written and diverge forever after -- and the first version of
# that guard asserted the bare word "deepen" was absent, which collided with the shallow
# warning's own remedy sentence ("re-run with --deepen N") and made a correct renderer
# fail a test written against a different property. A status line is not a mention.
DEEPEN_STATUS_PREFIX = "  deepen ....................... "


def classify_deepen(is_shallow, rc, before, after) -> tuple[str, str]:
    """Did the bounded history fetch actually buy sample? Returns (state, detail).

    `is_shallow` is THREE-VALUED: True / False / None ("the probe itself failed"). None is
    deliberately NOT folded into False -- "I could not tell whether the clone is shallow"
    must never render as "the log is already complete", which would silently promote an
    unknown into the comfortable bucket (Day 144).

    The success test is that the COMMIT COUNT MOVED, not that git exited 0. A fetch can
    succeed and buy nothing (already at the remote's tip, or a refspec that resolves to
    what we have), and an unreadable count is not evidence of movement either -- both are
    DID_NOT_TAKE, because the only claim this function is allowed to make is about the
    window the census is about to walk.
    """
    if is_shallow is False:
        return (
            DEEPEN_ALREADY_DEEP,
            "the clone is not shallow, so no fetch was attempted",
        )
    probe = "" if is_shallow else " (shallowness probe failed; fetch attempted anyway)"
    if rc != 0:
        return (
            DEEPEN_DID_NOT_TAKE,
            f"`git fetch --deepen` exited {rc}{probe}",
        )
    if before is None or after is None:
        return (
            DEEPEN_DID_NOT_TAKE,
            f"`git fetch --deepen` exited 0 but the commit count was unreadable{probe}",
        )
    if after > before:
        return (
            DEEPEN_TOOK,
            f"{before} -> {after} commits reachable (+{after - before}){probe}",
        )
    return (
        DEEPEN_DID_NOT_TAKE,
        f"`git fetch --deepen` exited 0 and the commit count did not move "
        f"({before} commits){probe}",
    )


def window_note(shallow, depth, deepen_state, deepen_detail) -> list[str]:
    """The lines under the window header. Both branches SPEAK, on purpose.

    A census reporting `0 behavioural` over a 59-commit window and one reporting `0` over
    a 400-commit window are DIFFERENT FINDINGS and must not render alike -- the first is
    a statement about the clone, the second about the loop. So a shallow window says out
    loud that it bounds the denominator DOWNWARD, and a complete log says out loud that
    it does not, rather than staying silent and letting absence of a warning read as
    absence of a bound.
    """
    out = []
    if deepen_state == DEEPEN_TOOK:
        out.append(f"{DEEPEN_STATUS_PREFIX}TOOK: {deepen_detail}")
    elif deepen_state == DEEPEN_DID_NOT_TAKE:
        out.append(f"{DEEPEN_STATUS_PREFIX}ATTEMPTED AND DID NOT TAKE: {deepen_detail}")
        out.append(
            "                                 the window below is the one that already "
            "existed, not the one asked for."
        )
    elif deepen_state == DEEPEN_ALREADY_DEEP:
        out.append(f"{DEEPEN_STATUS_PREFIX}NOT NEEDED: {deepen_detail}")

    if shallow:
        out.append(
            f"  !! SHALLOW WINDOW ({depth} commits) -- this BOUNDS THE DENOMINATOR BELOW "
            "DOWNWARD."
        )
        out.append(
            "     A behavioural count of 0 here is a fact about the clone depth as much "
            "as about the loop."
        )
        out.append(
            "     Re-run with --deepen N to buy sample; the depth reported above is the "
            "one actually obtained."
        )
    else:
        out.append(
            f"  complete log ({depth} commits) -- the denominator below is NOT bounded "
            "by clone depth."
        )
    return out


# --------------------------------------------------------------------------------------
# I/O half. Every landmine lives here, contained.
# --------------------------------------------------------------------------------------


def run_cmd(args, cwd=None, timeout=120, env=None):
    """Run a command, capture everything, never raise. Returns (rc, stdout+stderr)."""
    try:
        p = subprocess.run(
            args,
            cwd=cwd,
            env=env,
            capture_output=True,
            text=True,
            timeout=timeout,
            errors="replace",
        )
        return p.returncode, (p.stdout or "") + (p.stderr or "")
    except subprocess.TimeoutExpired:
        return 124, f"[counterfactual_green: timed out after {timeout}s]"
    except (OSError, ValueError) as exc:
        return 127, f"[counterfactual_green: could not run {args[0]}: {exc}]"


def repo_root() -> str:
    rc, out = run_cmd(["git", "rev-parse", "--show-toplevel"])
    if rc != 0:
        return os.getcwd()
    return out.strip().splitlines()[0] if out.strip() else os.getcwd()


def commit_count(root: str):
    """Commits reachable from HEAD, or None if the count could not be read."""
    rc, out = run_cmd(["git", "-C", root, "rev-list", "--count", "HEAD"], timeout=60)
    if rc != 0:
        return None
    try:
        return int(out.strip().splitlines()[0])
    except (ValueError, IndexError):
        return None


def is_shallow_repo(root: str):
    """True / False / None -- None means the probe itself failed, and stays distinct."""
    rc, out = run_cmd(["git", "-C", root, "rev-parse", "--is-shallow-repository"])
    if rc != 0:
        return None
    return out.strip() == "true"


def deepen_repo(root: str, n: int) -> tuple[str, str]:
    """Bounded history fetch, so the census denominator can grow. Returns (state, detail).

    NEVER `--unshallow`. An unbounded history fetch is an unknown cost with no ceiling on
    a repo of this age; a bounded deepen is the entire point, and the bound is the
    operator's number rather than mine.

    FAIL SOFT, AND SAY WHICH WAY. No network, no remote, a non-zero exit or a count that
    did not move all leave the census running over whatever window already exists -- and
    all of them SAY SO. A silent fall-through here would be "could not check" reading as
    "checked; clean" inside the one instrument built to stop me over-claiming.

    It does not touch the working tree: `git fetch` writes only to the object store and
    refs. (The harness reverts a failed task with `git reset --hard`, so a script that
    mutated the tree could destroy the session running it.)
    """
    shallow = is_shallow_repo(root)
    if shallow is False:
        # Short-circuit: nothing to buy, so nothing is spent.
        return classify_deepen(False, 0, None, None)
    before = commit_count(root)
    rc, _ = run_cmd(
        ["git", "-C", root, "fetch", "--deepen", str(n)], timeout=DEEPEN_TIMEOUT_SECS
    )
    after = commit_count(root)
    return classify_deepen(shallow, rc, before, after)


def collect_census(root: str, limit: int | None):
    """Walk the log, classify each task commit.

    Returns (rows, window, shallow, depth, error). `shallow` is THREE-VALUED (True /
    False / None) and is carried out rather than collapsed here, because the depth of the
    window is a finding about the denominator, not a decoration on the header.
    """
    rc, out = run_cmd(["git", "-C", root, "log", "--format=%H%x09%s"], timeout=60)
    if rc != 0:
        return [], "", None, "?", f"git log failed (rc={rc})"

    all_lines = out.splitlines()
    pairs = parse_log_lines(all_lines)
    if limit is not None:
        pairs = pairs[:limit]

    rows = []
    for sha, subject in pairs:
        rc2, out2 = run_cmd(
            ["git", "-C", root, "diff", "--name-only", f"{sha}^", sha],
            timeout=60,
        )
        if rc2 != 0:
            # A commit whose parent is unreachable (shallow boundary) cannot be diffed.
            # Recorded as not-addressable rather than dropped: a shrinking denominator
            # inside my own meter is the defect this whole family of checks is about.
            rows.append(CensusRow(sha, subject, []))
            continue
        touched = top_level_test_files(out2.splitlines())
        # Ask the DIFF, per file, rather than a hand-listed filename (#867).
        reg = set()
        for path in touched:
            rc3, diff = run_cmd(
                ["git", "-C", root, "diff", f"{sha}^", sha, "--", path], timeout=60
            )
            # A diff we could not read is NOT register-only: an unreadable change must
            # never be laundered into bookkeeping. Same conservative direction as
            # `attribute_failures`.
            if rc3 == 0 and test_diff_is_register_only(diff):
                reg.add(path)
        rows.append(CensusRow(sha, subject, touched, reg))

    rc3, out3 = run_cmd(["git", "-C", root, "rev-list", "--count", "HEAD"], timeout=60)
    depth = out3.strip() if rc3 == 0 else "?"
    shallow = is_shallow_repo(root)
    window = (
        f"{len(all_lines)} commits reachable from HEAD "
        f"({depth} total, shallow="
        f"{'yes' if shallow else ('no' if shallow is False else 'UNKNOWN')})"
    )
    return rows, window, shallow, depth, ""


def run_counterfactual(root: str, sha: str, timeout: int, target: str | None = None) -> tuple[str, str]:
    """Baseline the parent, then build post-src/ + pre-tests/ and run cargo test.

    Returns (verdict, detail). Every landmine from the module doc is enforced here.

    ORDER IS THE DESIGN: the baseline runs FIRST, and a red baseline short-circuits.
    That is correct (there is nothing to learn from comparing against a broken reference)
    and it is the cost control — it skips the expensive second cargo invocation on exactly
    the commits that cannot answer either way.

    The baseline is the parent commit **whole** — pre-task `tests/` AND pre-task `src/` —
    run through the same worktree, the same `cargo test`, the same `CARGO_TARGET_DIR`. If
    it is not a control run the same way, it is not a control.
    """
    rc, out = run_cmd(["git", "-C", root, "rev-parse", f"{sha}^"], timeout=60)
    if rc != 0:
        return COULD_NOT_CHECK, f"no parent for {sha[:8]} (shallow boundary?)"
    parent = out.strip().splitlines()[0]

    rc, out = run_cmd(
        ["git", "-C", root, "diff", "--name-only", parent, sha], timeout=60
    )
    if rc != 0:
        return COULD_NOT_CHECK, f"git diff failed (rc={rc})"
    changed = top_level_test_files(out.splitlines())
    # Per-file: was this file's change nothing but debt-register bookkeeping? Derived
    # from the diff of the commit under test (#867), never from a hand-listed filename.
    register_only = set()
    pre_sources = {}
    for path in changed:
        rc_d, dtext = run_cmd(
            ["git", "-C", root, "diff", parent, sha, "--", path], timeout=60
        )
        if rc_d == 0 and test_diff_is_register_only(dtext):
            register_only.add(path)
        # The PRE-task contents are what actually runs, so that is what a failing test
        # name is mapped against.
        rc_s, src = run_cmd(["git", "-C", root, "show", f"{parent}:{path}"], timeout=60)
        if rc_s == 0:
            pre_sources[path] = src
    if not changed:
        # Nothing to counterfactual: the counterfactual tree IS the shipped tree. No
        # baseline is needed and none is run — there is no comparison to license.
        return NO_TEST_CHANGE, "no top-level tests/*.rs touched"

    # Only BEHAVIOURAL test files may be rolled back. A debt register encodes line counts
    # of `src/`, so a pre-task copy over post-task `src/` fails by construction and would
    # manufacture an UNEARNED out of bookkeeping. Registers stay at the shipped version.
    rollback = behavioural_test_files(out.splitlines(), register_only)
    if not rollback:
        return NO_TEST_CHANGE, (
            "only debt-register file(s) touched ("
            + ", ".join(changed)
            + ") — verdict decided by construction, no signal"
        )

    # LANDMINE 1: a scratch worktree under mkdtemp, never the live tree, never the repo.
    tmp = tempfile.mkdtemp(prefix="yoyo-counterfactual-")
    wt = os.path.join(tmp, "wt")
    # LANDMINE 2: our own CARGO_TARGET_DIR, shared across BOTH runs of this commit so the
    # second build is warm. Kept out of the repo's own target/ — #832: a nested cargo
    # build over the shared target/debug/yoyo clobbers the binary every integration test
    # resolves through env!("CARGO_BIN_EXE_yoyo"), and it reddened main for three sessions
    # while reading as flakiness.
    # A BATCH shares one target dir across all its runs (adjacent commits share
    # dependencies, so runs 2..N are mostly warm -- the difference between one reading
    # per session and several). A single `--commit` passes None and gets its own, which
    # is byte-identical to the pre-ledger behaviour. Either way it is under mkdtemp and
    # NEVER the repo's own target/ (#832).
    target = target or os.path.join(tmp, "target")
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = target
    env.pop("RUSTFLAGS", None)

    def summarise(text: str) -> str:
        tail = "\n".join(
            ln for ln in text.splitlines() if ln.startswith(("test result:", "error"))
        )
        return tail[-1200:] if tail else text.strip()[-400:]

    try:
        # The worktree starts AT THE PARENT, so the first thing it can run is the control.
        rc, out = run_cmd(
            ["git", "-C", root, "worktree", "add", "--detach", wt, parent], timeout=300
        )
        if rc != 0:
            return COULD_NOT_CHECK, f"worktree add failed (rc={rc}): {out.strip()[:200]}"

        # ---- BASELINE: parent tests against parent src, whole. ------------------------
        rc, out = run_cmd(["cargo", "test"], cwd=wt, timeout=timeout, env=env)
        baseline = classify_baseline(rc, out)
        if baseline != BASELINE_OK:
            return baseline, "BASELINE (parent whole): " + summarise(out)

        # ---- COUNTERFACTUAL: post-task src/, pre-task tests/. -------------------------
        rc, out = run_cmd(["git", "-C", wt, "checkout", "--detach", sha], timeout=120)
        if rc != 0:
            return COULD_NOT_CHECK, f"checkout of {sha[:8]} failed: {out.strip()[:200]}"

        # Lay the PRE-task BEHAVIOURAL tests back over the POST-task src/. Debt registers
        # are deliberately NOT rolled back — see behavioural_test_files.
        rc, out = run_cmd(
            ["git", "-C", wt, "checkout", parent, "--"] + rollback, timeout=120
        )
        if rc != 0:
            # A test file that did not exist at the parent cannot be checked out. That is
            # a brand-new test file, which is not a counterfactual question at all.
            return COULD_NOT_CHECK, f"checkout of pre-tests failed: {out.strip()[:200]}"

        rc, out = run_cmd(["cargo", "test"], cwd=wt, timeout=timeout, env=env)
        verdict = classify_counterfactual(rc, out)
        # REGISTER DRIFT (#867), decided by the pure `apply_register_drift` so the branch
        # this shells cargo for is the same one the self-tests pin. It fires ONLY on an
        # UNEARNED, so INCONCLUSIVE and COULD_NOT_CHECK keep absolute precedence: a build
        # that did not finish cannot have run the assertions it is being judged on, and
        # that ordering must not move.
        verdict, why = apply_register_drift(verdict, out, pre_sources, {
            path: path in register_only for path in changed
        })
        if verdict == REGISTER_DRIFT:
            return verdict, (
                "BASELINE: green. COUNTERFACTUAL: red, but VOID — "
                + why + ". " + summarise(out)
            )
        return verdict, "BASELINE: green. COUNTERFACTUAL: " + summarise(out)
    finally:
        # LANDMINE 1, the other half: always give the worktree back.
        run_cmd(["git", "-C", root, "worktree", "remove", "--force", wt], timeout=120)
        shutil.rmtree(tmp, ignore_errors=True)


# --------------------------------------------------------------------------------------
# The limits. Printed on EVERY run, including a clean one.
# --------------------------------------------------------------------------------------

LIMITS = """\
LIMITS OF THIS READING (printed on every run, clean or not — "could not check" must never
read as "checked; clean"):

  1. A VERDICT IS MECHANICALLY CHECKABLE; INTENT IS NOT. An UNEARNED says the code fails
     an assertion it started with. It NEVER says anyone loosened one deliberately — a
     legitimate behaviour change that correctly updates its test looks identical. This is
     the same limit tests/blind_round_grades.rs states about grades (presence is
     checkable, honesty is not) and tests/doc_version_claims.rs states about markers.
     Claiming otherwise would be the exact over-reach this whole milestone is about.
  2. SCOPE: the 12 top-level tests/*.rs (8,995 lines), NOT the ~165k lines of src/. Rust
     buries unit tests inside 91 src/ files behind #[cfg(test)], and those cannot be
     lifted out without dragging the production code along. That half stays unmeasured,
     and one number here must never be read as a rate over the whole suite.
  3. A DEBT REGISTER IS NOT A BEHAVIOURAL TEST. A register records line counts of src/,
     so a PRE-task copy laid over POST-task src/ fails by construction whenever a module
     grew — and that failure is the gate working as designed, since updating the register
     is the compliant remedy the gate itself prints. Those commits are counted as
     REGISTER-ONLY and kept OUT of the behavioural denominator. Read the BEHAVIOURAL
     rate; the addressable rate is the flattering one. The split is DERIVED from each
     commit's own diff, not from a hand-listed filename — a file can be both (
     tests/git_chokepoint.rs carries a register beside 12 behavioural tests).
  3a. REGISTER_DRIFT MEANS THE VERDICT IS **VOID**, NOT CLEAN. It says every failing test
     lives in a file whose diff was pure bookkeeping, so the red was manufactured by the
     overlay rather than by the code. It is NEITHER evidence of an earned green NOR of an
     unearned one — do not add it to either column. It is also attributed CONSERVATIVELY:
     a failing test whose name maps to zero or several files, or that lives in a file with
     a substantive diff, keeps the verdict at UNEARNED. A genuine behavioural failure
     sitting beside bookkeeping still wins.
  4. INCONCLUSIVE IS NOT A NEAR-MISS. Pre-tests that fail to COMPILE against post-src/
     are exactly what an honest API rename produces, and exactly what a hidden break
     produces. The state exists because the two are indistinguishable from here, not
     because the run was sloppy.
  5. A BASELINE_RED CONCLUDES NOTHING, IN EITHER DIRECTION. It says the parent commit's
     own tests do not pass against the parent commit's own src/ — so the reference point
     is broken and the comparison is void. It is NOT a weak UNEARNED and NOT a machinery
     failure; those are COULD_NOT_CHECK. Read it as "this commit is unmeasurable here".
  6. A SMALL GAP IS NOT PROOF OF COMPLIANCE. Copied verbatim from SpecBench (arXiv
     2605.21384), whose held-out axis catches what a backward counterfactual structurally
     cannot: the pre-task tests never composed the NEW features either, so
     feature-isolation failures are invisible to this method. A green counterfactual says
     the old assertions still hold; it says nothing about whether the new behaviour was
     specified correctly.
  7. IT SEES ONLY SURVIVING HISTORY. scripts/evolve.sh reverts a failed task with
     `git reset --hard PRE_TASK_SHA`, so an unearned green inside a REVERTED task is
     invisible forever — and the sessions most likely to contain the behaviour are the
     ones whose evidence was destroyed. A clean census is a statement about survivors.
  8. A WINDOW IN COMMITS IS NOT A WINDOW IN TIME. This harness runs on a shallow clone, so
     the whole reachable log can span a day or two rather than the weeks a commit count
     suggests. The window line above states the depth measured; read it, don't infer it.
"""


def render_population_block(label: str, summary: dict, note: str = "") -> list[str]:
    """The same five figures the census has always printed, for ONE population.

    Extracted so the two populations cannot drift apart in the report: one statement of
    the shape, called twice. A second copy would agree the day it was written.
    """
    out = [f"  [{label}]{note}"]
    out.append(f"    task commits found ......... {summary['task_commits']}")
    out.append(f"    NO_TEST_CHANGE ............. {summary['not_addressable']}")
    out.append(f"    touch any tests/*.rs ....... {summary['addressable']}")
    out.append(
        f"      of which REGISTER-ONLY ... {summary['register_only']}"
        "   (bookkeeping; verdict decided by construction)"
    )
    out.append(
        f"      of which BEHAVIOURAL ..... {summary['behavioural']}"
        "   <- the denominator that carries signal"
    )
    if summary["task_commits"]:
        addr = 100.0 * summary["addressable"] / summary["task_commits"]
        beh = 100.0 * summary["behavioural"] / summary["task_commits"]
        out.append(f"    addressable rate ........... {addr:.0f}%")
        out.append(f"    BEHAVIOURAL rate ........... {beh:.0f}%")
    out.append("")
    return out


def render_census(rows, summary, window, limit, note=None, by_pop=None) -> str:
    out = []
    scope = f"last {limit} task commits" if limit else "all reachable task commits"
    out.append(f"counterfactual-green census over {scope}")
    out.append(f"  window ....................... {window}")
    out.extend(note or [])
    out.append("")

    # TWO POPULATIONS, REPORTED SEPARATELY AND NEVER SUMMED (Day 184, #868).
    # DREAM.md's milestone asks for the rate "reported separately for commits whose
    # subject carries an eval-fix or build-fix suffix" — that split is the pre-registered
    # guess. A pooled headline would destroy the only question the widened
    # TASK_COMMIT_RE was widened to ask, so there is no pooled rate here on purpose.
    by_pop = by_pop if by_pop is not None else census_by_population(rows)
    out.extend(
        render_population_block(
            "PLAIN — first-attempt deliveries, no fix-loop suffix",
            by_pop[POP_PLAIN],
        )
    )
    out.extend(
        render_population_block(
            "FIX-LOOP — subject carries eval-fix / build-fix",
            by_pop[POP_FIX_LOOP],
            note="   <- DREAM.md's pre-registered population",
        )
    )
    # Silent when zero, which is the normal case: an unrecognised suffix is a shape
    # nobody enumerated, so it speaks only when it exists rather than adding a
    # permanent "0" that trains the reader to skip the block.
    if by_pop[POP_UNKNOWN_SUFFIX]["task_commits"]:
        out.extend(
            render_population_block(
                "UNKNOWN-SUFFIX — a suffix matching neither marker",
                by_pop[POP_UNKNOWN_SUFFIX],
                note="   <- NOT summed into either population above",
            )
        )

    out.append(f"  (all task commits, both populations: {summary['task_commits']})")
    out.append("")
    for r in rows:
        if r.behavioural:
            mark = "*"
        elif r.addressable:
            mark = "r"
        else:
            mark = " "
        pop = "F" if r.is_fix_loop else (
            "?" if r.population == POP_UNKNOWN_SUFFIX else " "
        )
        files = ", ".join(r.test_files) if r.test_files else "(none)"
        out.append(f"  {mark}{pop} {r.sha[:8]}  {r.subject[:58]}")
        out.append(f"      tests/*.rs touched: {files}")
    out.append("")
    out.append("  legend: * behavioural   r register-only   (blank) NO_TEST_CHANGE")
    out.append("          F fix-loop commit   ? unrecognised suffix")
    return "\n".join(out)


# --------------------------------------------------------------------------------------
# The ledger. The reading ACCUMULATES ACROSS SESSIONS.
# --------------------------------------------------------------------------------------
#
# WHY THIS EXISTS AT ALL. One counterfactual is TWO `cargo test` invocations, so the >=20
# task commits DREAM.md asks for is many hours -- far more than one session's budget.
# Without a ledger the reading needs one unbroken block and therefore never happens; with
# one, it accumulates a few verdicts per session until the milestone is reachable. Four
# consecutive sessions changed this instrument and exactly ONE live counterfactual had
# ever been run; this is the scaffolding that makes the reading the deliverable instead.
#
# Every design choice below falls out of one sentence: A PROCESS KILLED BY THE SESSION
# BUDGET MUST KEEP EVERY VERDICT IT EARNED.
#
#   * APPEND PER VERDICT, never batch at the end -- and fsync. A batched write loses the
#     whole session precisely when the budget kills the process mid-run, which is exactly
#     when it WILL be killed. A verdict costs ~10 minutes of cargo; losing one to a tidy
#     exit is the expensive mistake.
#   * A MISSING OR UNREADABLE LEDGER IS ITS OWN STATE, and neither may read as "all
#     done". That is "could not check" rendering as "checked; clean" -- the refusal this
#     whole file is built around -- so the fail-safe direction is to skip NOTHING and say
#     so on stderr.

LEDGER_MISSING = "ledger-missing"
LEDGER_UNREADABLE = "ledger-unreadable"
LEDGER_READ = "ledger-read"


def ledger_line(sha, parent, day, subject, population, verdict, baseline, ts, depth):
    """One JSON line for one completed counterfactual. Pure: no I/O and no clock.

    `ts` and `depth` are passed IN rather than read here, so the record is a pure
    function of its inputs and can be pinned by a self-test byte-for-byte.
    """
    return json.dumps(
        {
            "sha": sha,
            "parent": parent,
            "day": day,
            "subject": subject,
            "population": population,
            "verdict": verdict,
            "baseline": baseline,
            "ts": ts,
            "window_depth": depth,
        },
        sort_keys=True,
    )


def parse_ledger(text):
    """Return (set of shas already recorded, count of lines that would not parse).

    Malformed lines are COUNTED, never silently dropped: a shrinking denominator inside
    my own meter is the defect this entire family of checks exists for. A blank line is
    normal JSONL and is neither -- it is not corruption and it is not a record.
    """
    shas = set()
    malformed = 0
    for raw in text.splitlines():
        line = raw.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except (ValueError, TypeError):
            malformed += 1
            continue
        if not isinstance(obj, dict):
            malformed += 1
            continue
        sha = obj.get("sha")
        if not isinstance(sha, str) or not sha:
            malformed += 1
            continue
        shas.add(sha)
    return shas, malformed


def baseline_from_verdict(verdict):
    """What the BASELINE did, derived from the verdict `run_counterfactual` returned.

    Derived rather than threaded out, deliberately: this task's deliverable is the
    reading, so it adds no new return value to the function the whole tool hangs on.
    The derivation is forced by ONE property of `run_counterfactual` -- a baseline that
    is not BASELINE_OK SHORT-CIRCUITS -- so any verdict reached downstream of that branch
    implies the baseline was green:

      EARNED / UNEARNED / INCONCLUSIVE / REGISTER_DRIFT -> green (past the short-circuit)
      BASELINE_RED                                      -> red
      NO_TEST_CHANGE  -> not-run: it returns before the worktree is even created.
      COULD_NOT_CHECK -> unknown, and that is honest rather than lazy. The machinery can
                         fail BEFORE the baseline (no parent, git diff, worktree add) or
                         AFTER it (checkout of sha / of pre-tests), and the verdict alone
                         cannot tell those apart. Reporting "green" here would be a guess
                         wearing an observation's clothes.
    """
    if verdict in (EARNED, UNEARNED, INCONCLUSIVE, REGISTER_DRIFT):
        return "green"
    if verdict == BASELINE_RED:
        return "red"
    if verdict == NO_TEST_CHANGE:
        return "not-run"
    return "unknown"


def select_runnable(rows, population, recorded, max_runs):
    """Which commits to run, NEWEST FIRST. Pure.

    Newest-first is a cost decision, not a cosmetic one: a newer commit's parent is
    closer to HEAD, so its dependency set is the one already warm in the shared
    CARGO_TARGET_DIR and its build is likeliest to resolve against the current
    toolchain. `rows` already arrive newest-first from `git log`.

    Only BEHAVIOURAL commits of the requested population are eligible -- a register-only
    commit's verdict is decided by construction (LIMITS item 3) and would spend ten
    minutes of cargo to learn nothing.
    """
    out = []
    for row in rows:
        if row.population != population:
            continue
        if not row.behavioural:
            continue
        if row.sha in recorded:
            continue
        out.append(row)
        if max_runs is not None and len(out) >= max_runs:
            break
    return out


def read_ledger(path):
    """(state, shas, malformed). I/O half of `parse_ledger`.

    FAIL-SAFE DIRECTION: a missing or unreadable ledger returns an EMPTY sha set, so
    nothing is skipped. An unreadable file must never be able to silence the whole run.
    """
    if not os.path.exists(path):
        return LEDGER_MISSING, set(), 0
    try:
        with open(path, "r", encoding="utf-8") as fh:
            text = fh.read()
    except OSError:
        return LEDGER_UNREADABLE, set(), 0
    shas, malformed = parse_ledger(text)
    return LEDGER_READ, shas, malformed


def append_ledger(path, line):
    """Append ONE line and fsync it. Returns "" on success, else the error text.

    The fsync is the point: a verdict costs ~10 minutes of cargo, and a process killed
    by the session budget must not lose one to a buffer that never reached disk.

    Returning the error text rather than raising is deliberate: a ledger failure must
    never destroy the verdict that was just earned, so the caller prints a WARNING and
    keeps going. The one thing it must not do is fail silently -- an empty return is the
    success sentinel, so a writer that cannot write has to say which error stopped it.
    """
    try:
        parent = os.path.dirname(path)
        if parent:
            os.makedirs(parent, exist_ok=True)
        with open(path, "a", encoding="utf-8") as fh:
            fh.write(line + "\n")
            fh.flush()
            os.fsync(fh.fileno())
    except OSError as exc:
        return str(exc)
    return ""


def main(argv):
    parser = argparse.ArgumentParser(
        prog="counterfactual_green.py",
        description=(
            "Was this green EARNED? Rebuild post-task src/ with pre-task tests/ and run "
            "cargo test. Seven states, none folded into another."
        ),
        epilog=(
            "Hand-run only. Never invoked from a #[test] (#832: no #[test] under src/ "
            "may spawn cargo), and it never touches the live working tree."
        ),
    )
    parser.add_argument(
        "--census",
        action="store_true",
        help="count how many task commits are addressable by the counterfactual",
    )
    parser.add_argument(
        "--commit",
        metavar="SHA",
        help="run ONE counterfactual end-to-end against this task commit",
    )
    parser.add_argument(
        "--limit", type=int, metavar="N", help="census: only the last N task commits"
    )
    parser.add_argument(
        "--deepen",
        type=int,
        metavar="N",
        help=(
            "before the census, fetch N more commits of history if the clone is shallow "
            "(bounded on purpose -- never --unshallow). Fails soft and says so."
        ),
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=DEFAULT_TIMEOUT_SECS,
        metavar="S",
        help=f"ceiling for one cargo test run (default {DEFAULT_TIMEOUT_SECS}s)",
    )
    parser.add_argument(
        "--max-runs",
        type=int,
        metavar="N",
        help=(
            "BATCH: run up to N counterfactuals over the behavioural commits of one "
            "population, newest first. This is the flag that takes a READING."
        ),
    )
    parser.add_argument(
        "--population",
        default=POP_PLAIN,
        choices=[POP_PLAIN, POP_FIX_LOOP, POP_UNKNOWN_SUFFIX],
        help=(
            "batch: which arm to read (default %(default)s). Never summed with another "
            "-- DREAM.md pre-registers the fix-loop arm as a SEPARATE question."
        ),
    )
    parser.add_argument(
        "--record",
        metavar="PATH",
        help=(
            "append one JSON line per completed verdict, immediately. This is what "
            "makes the reading accumulate across sessions instead of needing one block."
        ),
    )
    parser.add_argument(
        "--resume",
        action="store_true",
        help="skip shas already present in --record (a missing ledger skips nothing)",
    )
    parser.add_argument("--test", action="store_true", help="run self-tests and exit")
    args = parser.parse_args(argv)

    if args.test:
        return run_self_tests()

    if not args.census and not args.commit and args.max_runs is None:
        parser.print_help()
        return 2

    root = repo_root()
    status = 0

    if args.census:
        # Deepen BEFORE walking the log, or the census measures the old window and
        # reports the new depth -- a guard that reads the world after its own action.
        deepen_state, deepen_detail = DEEPEN_NOT_REQUESTED, ""
        if args.deepen is not None:
            deepen_state, deepen_detail = deepen_repo(root, args.deepen)
        rows, window, shallow, depth, err = collect_census(root, args.limit)
        if err:
            print(f"COULD NOT CHECK: {err}", file=sys.stderr)
            print(LIMITS, file=sys.stderr)
            return 1
        summary = census_summary(rows)
        note = window_note(shallow, depth, deepen_state, deepen_detail)
        # ANTI-VACUOUS, asserted first: a scan finding no task commits at all must fail
        # loudly rather than report "0 addressable". A scanner that finds nothing and
        # passes is this very defect wearing the opposite sign, and it is quieter.
        if summary["task_commits"] == 0:
            print(
                "COULD NOT CHECK: zero task commits found in "
                f"{window}. This is a refusal, NOT '0 addressable' — either the log is "
                "empty, the clone is too shallow, or TASK_COMMIT_RE has drifted from "
                "scripts/evolve.sh's commit subjects.",
                file=sys.stderr,
            )
            print("\n".join(note), file=sys.stderr)
            print(LIMITS, file=sys.stderr)
            return 1
        print(render_census(rows, summary, window, args.limit, note))
        status = 0

    if args.max_runs is not None:
        # ---- BATCH: the reading. -------------------------------------------------------
        if args.resume and not args.record:
            print(
                "--resume needs --record: there is no ledger to resume FROM.",
                file=sys.stderr,
            )
            return 2
        if args.deepen is not None and not args.census:
            deepen_repo(root, args.deepen)
        rows, window, _shallow, depth, err = collect_census(root, args.limit)
        if err:
            print(f"COULD NOT CHECK: {err}", file=sys.stderr)
            print(LIMITS, file=sys.stderr)
            return 1

        recorded = set()
        if args.resume:
            state, recorded, malformed = read_ledger(args.record)
            # Three states, none folded -- and neither absent one may read as "all done".
            if state == LEDGER_MISSING:
                print(
                    f"resume: no ledger at {args.record} yet — nothing recorded, "
                    "so nothing is skipped.",
                    file=sys.stderr,
                )
            elif state == LEDGER_UNREADABLE:
                print(
                    f"resume: the ledger at {args.record} EXISTS and could not be READ. "
                    "This is NOT 'all done' — skipping nothing and running from the top.",
                    file=sys.stderr,
                )
            else:
                extra = (
                    f"; {malformed} malformed line(s) COUNTED, not dropped"
                    if malformed
                    else ""
                )
                print(
                    f"resume: {len(recorded)} sha(s) already recorded in "
                    f"{args.record}{extra}.",
                    file=sys.stderr,
                )

        todo = select_runnable(rows, args.population, recorded, args.max_runs)
        eligible = [
            r for r in rows if r.population == args.population and r.behavioural
        ]
        print(f"window: {window}")
        print(
            f"batch: population [{args.population}] has {len(eligible)} behavioural "
            f"commit(s); {len(recorded & {r.sha for r in eligible})} already recorded; "
            f"running {len(todo)} now (newest first, --max-runs {args.max_runs})."
        )
        sys.stdout.flush()

        # ONE shared target dir for the whole batch: adjacent commits share dependencies,
        # so runs 2..N are mostly warm. Under mkdtemp, never the repo's target/ (#832).
        shared = tempfile.mkdtemp(prefix="yoyo-counterfactual-batch-")
        target = os.path.join(shared, "target")
        try:
            for i, row in enumerate(todo, 1):
                rc_p, out_p = run_cmd(
                    ["git", "-C", root, "rev-parse", f"{row.sha}^"], timeout=60
                )
                parent = out_p.strip().splitlines()[0] if rc_p == 0 else ""
                m = TASK_COMMIT_RE.match(row.subject)
                day = m.group(1) if m else ""

                print(f"\n[{i}/{len(todo)}] {row.sha[:12]} {row.subject[:70]}")
                sys.stdout.flush()
                verdict, detail = run_counterfactual(
                    root, row.sha, args.timeout, target=target
                )
                baseline = baseline_from_verdict(verdict)
                print(f"  verdict: {verdict}   (baseline: {baseline})")
                for ln in detail.splitlines()[:6]:
                    print(f"  {ln}")
                sys.stdout.flush()

                if args.record:
                    # APPEND PER VERDICT, right here. A process killed by the session
                    # budget on run i+1 must keep every verdict it already earned.
                    ts = (
                        datetime.datetime.now(datetime.timezone.utc)
                        .replace(microsecond=0)
                        .isoformat()
                        .replace("+00:00", "Z")
                    )
                    err_w = append_ledger(
                        args.record,
                        ledger_line(
                            row.sha, parent, day, row.subject, row.population,
                            verdict, baseline, ts, depth,
                        ),
                    )
                    if err_w:
                        print(f"  WARNING: ledger append failed: {err_w}", file=sys.stderr)
        finally:
            shutil.rmtree(shared, ignore_errors=True)

    if args.commit:
        verdict, detail = run_counterfactual(root, args.commit, args.timeout)
        print("")
        print(f"counterfactual verdict for {args.commit[:12]}: {verdict}")
        if detail:
            print("  ---")
            for ln in detail.splitlines():
                print(f"  {ln}")

    print(LIMITS, file=sys.stderr)
    return status


# --------------------------------------------------------------------------------------
# Self-tests. Fabricated captures only — never runs cargo.
# --------------------------------------------------------------------------------------


def run_self_tests():
    failures = []

    def check(name, cond, got=None):
        if not cond:
            failures.append(f"{name} (got: {got!r})")

    # -- reachability: one row per state, so no branch can be arithmetically dead --------
    # EARNED
    v = classify_counterfactual(0, "running 42 tests\ntest result: ok. 42 passed; 0 failed\n")
    check("EARNED reachable", v == EARNED, v)

    # UNEARNED — an assertion failed, the build was fine.
    v = classify_counterfactual(
        101,
        "running 42 tests\n"
        "thread 'x' panicked at src/a.rs:1:1:\nassertion failed\n"
        "test result: FAILED. 41 passed; 1 failed; 0 ignored\n",
    )
    check("UNEARNED reachable", v == UNEARNED, v)

    # INCONCLUSIVE — the pre-tests do not compile against post-src/.
    v = classify_counterfactual(
        101,
        "error[E0425]: cannot find function `foo` in this scope\n"
        "error: could not compile `yoyo` (bin \"yoyo\" test) due to 1 previous error\n",
    )
    check("INCONCLUSIVE reachable", v == INCONCLUSIVE, v)

    # COULD_NOT_CHECK — compiled, ran nothing. Anti-vacuous: this is NOT earned.
    v = classify_counterfactual(0, "   Compiling yoyo v0.1.0\n    Finished test profile\n")
    check("rc0 with no test result is COULD_NOT_CHECK", v == COULD_NOT_CHECK, v)

    # -- PRECEDENCE: a capture carrying BOTH a compile error and a test-result line ------
    # INCONCLUSIVE wins. A build that did not finish cannot have run the assertions it is
    # being judged on. This is its own row because it is the rule most likely to be
    # "simplified" away by a later reader.
    both = (
        "running 12 tests\n"
        "test result: ok. 12 passed; 0 failed\n"
        "error[E0308]: mismatched types\n"
        "error: could not compile `yoyo` (test \"integration\") due to 1 previous error\n"
    )
    v = classify_counterfactual(101, both)
    check("compile error beats a passing test-result line", v == INCONCLUSIVE, v)
    both_failed = (
        "test result: FAILED. 11 passed; 1 failed\n"
        "error[E0432]: unresolved import `crate::gone`\n"
    )
    v = classify_counterfactual(101, both_failed)
    check("compile error beats a FAILED test-result line", v == INCONCLUSIVE, v)

    # -- near-miss guards: what must NOT read as a compile error ------------------------
    # An indented rustc-looking string inside a panic message is test output, not a build
    # failure. Compile markers are anchored to column 0 for exactly this reason.
    v = classify_counterfactual(
        101,
        "running 3 tests\n"
        "thread 't' panicked at tests/a.rs:9:5:\n"
        "  expected error[E0425] in the message but found none\n"
        "test result: FAILED. 2 passed; 1 failed\n",
    )
    check("indented E-code in a panic is still UNEARNED", v == UNEARNED, v)
    v = classify_counterfactual(
        0,
        "warning: unused variable `x`\n"
        "warning: `yoyo` (lib) generated 1 warning\n"
        "test result: ok. 5 passed; 0 failed\n",
    )
    check("warnings do not make it INCONCLUSIVE", v == EARNED, v)

    # -- non-zero exit with no marker at all is UNEARNED, not silently clean -------------
    v = classify_counterfactual(1, "some unhelpful output\n")
    check("bare non-zero exit is UNEARNED", v == UNEARNED, v)

    # -- THE BASELINE GATE ---------------------------------------------------------------
    # Without a baseline an UNEARNED is unfalsifiable: a flaky test, environment drift or
    # a shallow-clone artifact produces exactly the same red as a genuine unearned green.
    # Table-driven over fabricated exit codes and captured output, same as its sibling.
    baseline_table = [
        # (name, rc, output, expected)
        (
            "green baseline licenses the comparison",
            0,
            "running 42 tests\ntest result: ok. 42 passed; 0 failed\n",
            BASELINE_OK,
        ),
        (
            "red baseline is BASELINE_RED",
            101,
            "running 42 tests\n"
            "thread 'x' panicked at src/a.rs:1:1:\nassertion failed\n"
            "test result: FAILED. 41 passed; 1 failed; 0 ignored\n",
            BASELINE_RED,
        ),
        (
            "bare non-zero baseline exit is BASELINE_RED",
            1,
            "some unhelpful output\n",
            BASELINE_RED,
        ),
        # A baseline that will not COMPILE is the machinery failing, not the reference
        # point being broken. Same compile-vs-assert discriminator as the counterfactual
        # half, and for the same reason: a build that did not finish cannot have run the
        # assertions it is being judged on.
        (
            "baseline that fails to compile is COULD_NOT_CHECK, not BASELINE_RED",
            101,
            "error[E0425]: cannot find function `foo` in this scope\n"
            "error: could not compile `yoyo` (bin \"yoyo\" test) due to 1 previous error\n",
            COULD_NOT_CHECK,
        ),
        (
            "compile error beats a FAILED line in the baseline too",
            101,
            "test result: FAILED. 11 passed; 1 failed\n"
            "error[E0432]: unresolved import `crate::gone`\n",
            COULD_NOT_CHECK,
        ),
        # Anti-vacuous: a run that compiled and executed zero tests is not a green
        # baseline, it is no baseline.
        (
            "baseline that ran zero tests is COULD_NOT_CHECK, not OK",
            0,
            "   Compiling yoyo v0.1.0\n    Finished test profile\n",
            COULD_NOT_CHECK,
        ),
        # An indented rustc-looking string inside a panic is test output, not a build
        # failure — the column-0 anchor, exercised on this side of the split too.
        (
            "indented E-code in a baseline panic is still BASELINE_RED",
            101,
            "thread 't' panicked at tests/a.rs:9:5:\n"
            "  expected error[E0425] in the message but found none\n"
            "test result: FAILED. 2 passed; 1 failed\n",
            BASELINE_RED,
        ),
    ]
    for name, rc_, out_, expected in baseline_table:
        got_ = classify_baseline(rc_, out_)
        check(f"baseline: {name}", got_ == expected, got_)

    # A red baseline must NEVER be reported as UNEARNED. That conflation is the whole
    # defect this gate closes: it would publish an environment artifact as a finding.
    red = classify_baseline(101, "test result: FAILED. 41 passed; 1 failed\n")
    check("red baseline is not UNEARNED", red != UNEARNED, red)
    check("red baseline is not COULD_NOT_CHECK", red != COULD_NOT_CHECK, red)
    check("BASELINE_OK is not a run verdict", BASELINE_OK not in RUN_VERDICTS)
    check("BASELINE_RED is a run verdict", BASELINE_RED in RUN_VERDICTS)
    check(
        "seven distinct states",
        len({NO_TEST_CHANGE, EARNED, UNEARNED, INCONCLUSIVE, COULD_NOT_CHECK,
             BASELINE_RED, REGISTER_DRIFT}) == 7,
    )
    check("REGISTER_DRIFT is a run verdict", REGISTER_DRIFT in RUN_VERDICTS)

    # NEAR-MISS GUARD, and it is the half that matters: a GREEN baseline followed by a RED
    # counterfactual must STILL classify UNEARNED. The gate must not eat the finding it
    # exists to protect. A discriminator tested only on the side that fires is vacuous
    # green, and this row is the side that must NOT fire.
    green_baseline = classify_baseline(0, "test result: ok. 42 passed; 0 failed\n")
    red_counterfactual = classify_counterfactual(
        101,
        "running 42 tests\n"
        "thread 'x' panicked at tests/git_chokepoint.rs:1:1:\nassertion failed\n"
        "test result: FAILED. 41 passed; 1 failed\n",
    )
    check("near-miss: baseline green", green_baseline == BASELINE_OK, green_baseline)
    check(
        "near-miss: green baseline + red counterfactual is STILL UNEARNED",
        red_counterfactual == UNEARNED,
        red_counterfactual,
    )
    # ...and the same pair still reaches EARNED when the counterfactual is green, so the
    # gate is not silently converting every verdict into itself.
    check(
        "near-miss: green baseline + green counterfactual is still EARNED",
        classify_counterfactual(0, "test result: ok. 42 passed; 0 failed\n") == EARNED,
    )

    # -- has_compile_error, directly -----------------------------------------------------
    check("compile marker found", has_compile_error("error[E0001]: bad\n"))
    check("could-not-compile found", has_compile_error("error: could not compile `x`\n"))
    check("no marker in clean output", not has_compile_error("test result: ok. 1 passed\n"))
    check("indented marker not found", not has_compile_error("   error[E0001]: bad\n"))

    # -- TASK_COMMIT_RE, the second copy, against REAL subjects from this repo ----------
    real = [
        "Day 183 (11:42): Blind round 91 on src/dispatch_sub.rs — the #1 dark room (Task 2)",
        "Day 182 (20:35): #863 swept at the chokepoint — git path-quoting (Task 1)",
    ]
    for s in real:
        check(f"real task subject matches: {s[:30]}", TASK_COMMIT_RE.match(s) is not None)
    not_task = [
        "Day 183 (14:32): assessment",
        "Day 183 (11:42): session wrap-up",
        "Day 183: bump skill-evolve counter (2)",
        "dream: progress (day 183)",
    ]
    for s in not_task:
        check(f"non-task subject rejected: {s[:30]}", TASK_COMMIT_RE.match(s) is None)

    # SUPERSEDED ASSERTION (Day 184, #868), recorded rather than erased. This list used to
    # carry a sixth entry:
    #
    #     "Day 183 (11:42): One bounded retry (Task 1, eval-fix 1)",
    #
    # i.e. it asserted that a fix-loop commit is NOT a task commit. That was a true record
    # of the narrow `TASK_COMMIT_RE` and it was pinning the DEFECT: 184 fix-loop commits in
    # the deepened window were invisible to the census, and they are precisely the
    # population DREAM.md's milestone pre-registers. The fixture is not deleted — it moves
    # below, where the same string is asserted to match and to classify as `POP_FIX_LOOP`.
    # A fixture that outlives its fix converts a defect into a green invariant.
    superseded_fixture = "Day 183 (11:42): One bounded retry (Task 1, eval-fix 1)"
    check(
        "superseded: the fix-loop fixture that used to be rejected now matches",
        TASK_COMMIT_RE.match(superseded_fixture) is not None,
    )
    check(
        "superseded: and it classifies as fix-loop, not merely as visible",
        subject_population(superseded_fixture) == POP_FIX_LOOP,
        subject_population(superseded_fixture),
    )

    # -- NEAR-MISS GUARD, and it is the half that matters --------------------------------
    # This widening's whole risk is changing what an ALREADY-VISIBLE commit means. A plain
    # `(Task N)` subject must still match AND still parse to byte-identical values, so the
    # assertion is on the parsed day and title verbatim rather than on `is not None` — a
    # regex that matched but captured a different title would sail past a match-only check.
    plain_subject = "Day 182 (20:35): #863 swept at the chokepoint — git path-quoting (Task 1)"
    m_plain = TASK_COMMIT_RE.match(plain_subject)
    check("near-miss: plain task subject still matches", m_plain is not None)
    if m_plain:
        check("near-miss: plain day parsed byte-for-byte", m_plain.group(1) == "182",
              m_plain.group(1))
        check(
            "near-miss: plain title parsed byte-for-byte",
            m_plain.group(2) == "#863 swept at the chokepoint — git path-quoting",
            m_plain.group(2),
        )
        check("near-miss: plain subject captures no suffix", m_plain.group(3) is None,
              m_plain.group(3))
    check(
        "near-miss: plain subject classifies as POP_PLAIN",
        subject_population(plain_subject) == POP_PLAIN,
        subject_population(plain_subject),
    )

    # A fix-loop subject parses its day and title byte-for-byte too — the suffix must be
    # peeled off the title, not absorbed into it, or the census would key rows on a string
    # that differs from the plain twin's.
    m_fix = TASK_COMMIT_RE.match("Day 184 (00:08): thing here (Task 1, eval-fix 2)")
    check("fix-loop subject matches", m_fix is not None)
    if m_fix:
        check("fix-loop day parsed", m_fix.group(1) == "184", m_fix.group(1))
        check("fix-loop title excludes the suffix", m_fix.group(2) == "thing here",
              m_fix.group(2))
        check("fix-loop suffix captured", m_fix.group(3) == "eval-fix 2", m_fix.group(3))

    # -- subject_population / is_fix_loop, in BOTH directions ----------------------------
    # A discriminator tested only on the side that fires is vacuous green.
    for subj, expected in [
        ("Day 184 (00:08): thing (Task 1)", POP_PLAIN),
        ("Day 184 (00:08): thing (Task 1, eval-fix 1)", POP_FIX_LOOP),
        ("Day 184 (00:08): thing (Task 2, build-fix 3)", POP_FIX_LOOP),
        ("Day 184 (00:08): thing (Task 2,  build-fix 12)", POP_FIX_LOOP),
        # A suffix that matches NEITHER marker is its own third state. Calling it
        # fix-loop would be a claim I cannot support; calling it plain would let an
        # unrecognised shape join the comfortable bucket (Day 144).
        ("Day 184 (00:08): thing (Task 1, checkpoint-retry)", POP_UNKNOWN_SUFFIX),
        ("cargo fmt", None),
        ("Merge pull request #1 from x/y", None),
        ("Day 183 (14:32): assessment", None),
    ]:
        got = subject_population(subj)
        check(f"population: {subj[:38]}", got == expected, got)

    # SELF-CONTAMINATION GUARD: the marker test runs against the CAPTURED SUFFIX, never
    # the whole subject. This session's own commit subject contains the literal string
    # "build-fix" in its TITLE, so a whole-subject scan would score this very task as a
    # fix-loop commit — my own prose about a marker scoring as the marker, which is the
    # founding defect of `measure_abstentions.py`.
    contaminated = "Day 184 (04:20): make the census see build-fix commits (Task 1)"
    check(
        "self-contamination: a marker in the TITLE is not a fix-loop commit",
        subject_population(contaminated) == POP_PLAIN,
        subject_population(contaminated),
    )

    # -- the same question at the EMISSION POINT: the row a caller receives --------------
    # `subject_population` is one layer below `CensusRow`; the census reads `is_fix_loop`,
    # so that is where the split has to be true.
    row_plain = CensusRow("a" * 40, "Day 184 (00:08): thing (Task 1)", ["tests/x.rs"])
    row_fix = CensusRow("b" * 40, "Day 184 (00:08): thing (Task 1, eval-fix 1)",
                        ["tests/x.rs"])
    row_unknown = CensusRow("c" * 40, "Day 184 (00:08): thing (Task 1, checkpoint-retry)",
                            ["tests/x.rs"])
    check("row: plain commit is not fix-loop", row_plain.is_fix_loop is False,
          row_plain.is_fix_loop)
    check("row: fix-loop commit is fix-loop", row_fix.is_fix_loop is True,
          row_fix.is_fix_loop)
    check("row: unknown suffix is NOT counted as fix-loop", row_unknown.is_fix_loop is False,
          row_unknown.is_fix_loop)
    check("row: unknown suffix keeps its own population",
          row_unknown.population == POP_UNKNOWN_SUFFIX, row_unknown.population)

    # -- the fold: two populations, NEVER summed ------------------------------------------
    # Pooling them would destroy the only question the widening was made to ask, so the
    # fold is asserted to keep them apart rather than merely to count correctly.
    folded = census_by_population([row_plain, row_fix, row_unknown])
    check("fold: plain population holds exactly its own commit",
          folded[POP_PLAIN]["task_commits"] == 1, folded[POP_PLAIN]["task_commits"])
    check("fold: fix-loop population holds exactly its own commit",
          folded[POP_FIX_LOOP]["task_commits"] == 1, folded[POP_FIX_LOOP]["task_commits"])
    check("fold: unknown suffix is counted apart from both",
          folded[POP_UNKNOWN_SUFFIX]["task_commits"] == 1,
          folded[POP_UNKNOWN_SUFFIX]["task_commits"])
    check(
        "fold: the two populations are disjoint, never pooled",
        folded[POP_PLAIN]["task_commits"] + folded[POP_FIX_LOOP]["task_commits"] == 2,
    )

    # -- parse_log_lines ------------------------------------------------------------------
    pairs = parse_log_lines(
        [
            "abc123\tDay 183 (11:42): Real work here (Task 1)",
            "def456\tDay 183 (11:42): session wrap-up",
            "malformed-no-tab",
            "",
        ]
    )
    check("log parser found one task commit", len(pairs) == 1, pairs)
    check("log parser kept the sha", pairs[0][0] == "abc123", pairs)

    # -- top_level_test_files: the scope limit, in both directions ------------------------
    paths = [
        "tests/module_size.rs",
        "tests/integration.rs",
        "tests/common/mod.rs",
        "src/git.rs",
        "src/commands_risk.rs",
        "CLAUDE.md",
        "tests/notes.md",
    ]
    got = top_level_test_files(paths)
    check("top-level tests kept", got == ["tests/module_size.rs", "tests/integration.rs"], got)
    check("src/ never enters", not any(p.startswith("src/") for p in got), got)
    check("nested tests/ excluded", "tests/common/mod.rs" not in got, got)

    # -- census fold, and NO_TEST_CHANGE is never EARNED ---------------------------------
    rows = [
        CensusRow("a" * 40, "Day 1 (0:0): x (Task 1)", ["tests/a.rs"]),
        CensusRow("b" * 40, "Day 1 (0:0): y (Task 2)", []),
        CensusRow("c" * 40, "Day 1 (0:0): z (Task 3)", ["tests/a.rs", "tests/b.rs"]),
    ]
    s = census_summary(rows)
    check("census counts commits", s["task_commits"] == 3, s)
    check("census counts addressable", s["addressable"] == 2, s)
    check("census counts non-addressable", s["not_addressable"] == 1, s)
    check(
        "addressable + not == total",
        s["addressable"] + s["not_addressable"] == s["task_commits"],
        s,
    )

    # -- the register-only split: the finding, in both directions ------------------------
    # DERIVED from each commit's diff (#867), never a hand-listed filename. The census
    # must not report a register bump as measurable when the real denominator is smaller.
    REG_DIFF = (
        "--- a/tests/module_size.rs\n"
        "+++ b/tests/module_size.rs\n"
        "@@ -1,1 +1,1 @@\n"
        '-    ("src/cli.rs", 5349),\n'
        '+    ("src/cli.rs", 5400),\n'
    )
    BEHAVIOURAL_DIFF = (
        "--- a/tests/git_chokepoint.rs\n"
        "+++ b/tests/git_chokepoint.rs\n"
        "@@ -1,1 +1,1 @@\n"
        '-    assert_eq!(scan.sites.len(), 70);\n'
        '+    assert_eq!(scan.sites.len(), 71);\n'
    )
    check("a register bump diff is register-only", test_diff_is_register_only(REG_DIFF))
    check(
        "an assert_eq! diff is NOT register-only",
        not test_diff_is_register_only(BEHAVIOURAL_DIFF),
    )

    # -- is_register_literal_line, directly: the rule test_diff_is_register_only asks -----
    # Table-tested in BOTH directions, because a discriminator exercised only where it
    # fires is vacuous green — and this predicate's whole job is to say NO to assertions.
    register_line_table = [
        # (line, carried in_string, expected, why)
        ('    ("src/cli.rs", 5349),', False, True, "a two-field register tuple"),
        ('    ("src/commands_search.rs", "list_project_files", "why..."),',
         False, True, "the three-field 08a9e36f shape"),
        ("    (", False, True, "a tuple opened across lines"),
        ('        "list_project_files",', False, True, "a lone tuple field"),
        ("    ),", False, True, "a tuple closed across lines"),
        ("//! module prose", False, True, "a doc comment carries no assertion"),
        ("    // an ordinary comment", False, True, "a line comment likewise"),
        ("", False, True, "a blank line"),
        ("   ", False, True, "a whitespace-only line"),
        ("     exactly and is a conversion candidate.\",", True, True,
         "a continued string literal is pure content"),
        ('    assert_eq!(got, "x");', False, False, "an assertion"),
        ("    let n = count_rs_lines(p);", False, False, "a binding"),
        ("fn every_site_is_registered() {", False, False, "a fn signature"),
        ("    const MAX_MODULE_LINES: usize = 2000;", False, False, "a const"),
        ("    panic!();", False, False, "a bare panic still names an identifier"),
    ]
    for line_, state_, expected_, why_ in register_line_table:
        got_ = is_register_literal_line(line_, state_)
        check(f"register line ({why_})", got_ == expected_, got_)

    # EMPTY DIFF IS FALSE, NOT TRUE. "Nothing changed" is not "only bookkeeping changed";
    # folding them would let a file with no diff at all launder a real failure into a void
    # verdict, which is the one direction this instrument exists to avoid.
    check("an empty diff is NOT register-only", not test_diff_is_register_only(""))
    check(
        "a context-only diff is NOT register-only",
        not test_diff_is_register_only(
            "--- a/tests/module_size.rs\n"
            "+++ b/tests/module_size.rs\n"
            "@@ -1,1 +1,1 @@\n"
            '     ("src/cli.rs", 5349),\n'
        ),
    )

    # -- failing_test_names: both libtest shapes, deduped ---------------------------------
    LIBTEST_OUT = (
        "running 13 tests\n"
        "test every_direct_git_invocation_is_at_the_chokepoint_or_registered ... FAILED\n"
        "test classify_table ... ok\n"
        "\n"
        "failures:\n"
        "\n"
        "    every_direct_git_invocation_is_at_the_chokepoint_or_registered\n"
        "\n"
        "test result: FAILED. 12 passed; 1 failed\n"
    )
    check(
        "failing_test_names reads both shapes and dedupes",
        failing_test_names(LIBTEST_OUT)
        == ["every_direct_git_invocation_is_at_the_chokepoint_or_registered"],
        failing_test_names(LIBTEST_OUT),
    )
    check(
        "failing_test_names finds nothing in a green run",
        failing_test_names("test result: ok. 42 passed; 0 failed\n") == [],
    )

    # -- attribute_failures: the conservative direction is the safety property ------------
    PRE_SOURCES = {
        "tests/git_chokepoint.rs":
            "fn every_direct_git_invocation_is_at_the_chokepoint_or_registered() {}\n"
            "fn classify_table() {}\n",
        "tests/module_size.rs": "fn src_modules_respect_the_size_gate() {}\n",
    }
    ratchet = ["every_direct_git_invocation_is_at_the_chokepoint_or_registered"]
    ok_, _ = attribute_failures(ratchet, PRE_SOURCES, {"tests/git_chokepoint.rs": True})
    check("a ratchet failure in a register-only file IS drift", ok_)
    # NEAR-MISS GUARD: same file, same test, but the diff touched something substantive.
    ok_, _ = attribute_failures(ratchet, PRE_SOURCES, {"tests/git_chokepoint.rs": False})
    check("...and is NOT drift when that file's diff was behavioural", not ok_)
    # NEAR-MISS GUARD: a genuine failure sitting beside bookkeeping still wins.
    ok_, _ = attribute_failures(
        ratchet + ["src_modules_respect_the_size_gate"],
        PRE_SOURCES,
        {"tests/git_chokepoint.rs": True, "tests/module_size.rs": False},
    )
    check("one non-attributable failure keeps the whole set UNEARNED", not ok_)
    # Unmappable in BOTH directions: zero owners, and more than one.
    ok_, _ = attribute_failures(["no_such_test"], PRE_SOURCES,
                                {"tests/git_chokepoint.rs": True})
    check("a name owned by ZERO files is not drift", not ok_)
    ok_, _ = attribute_failures(
        ["shared_name"],
        {"tests/a.rs": "fn shared_name() {}\n", "tests/b.rs": "fn shared_name() {}\n"},
        {"tests/a.rs": True, "tests/b.rs": True},
    )
    check("a name owned by TWO files is not drift", not ok_)
    ok_, _ = attribute_failures([], PRE_SOURCES, {"tests/git_chokepoint.rs": True})
    check("an unnameable failure set is not drift", not ok_)

    # -- apply_register_drift: the composition an actual run performs ---------------------
    # The real 08a9e36f shape: the commit deleted the register line AND converted the
    # bypass, so laying the pre-task file over post-task src/ fires the ratchet BY
    # CONSTRUCTION. The gate worked; the instrument used to mis-read it.
    CHOKEPOINT_REG_DIFF = (
        "--- a/tests/git_chokepoint.rs\n"
        "+++ b/tests/git_chokepoint.rs\n"
        "@@ -1,1 +1,0 @@\n"
        '-    ("src/commands_search.rs", "list_project_files", "no blocker"),\n'
    )
    check(
        "the real 08a9e36f deleted-tuple shape is register-only",
        test_diff_is_register_only(CHOKEPOINT_REG_DIFF),
    )
    drift_map = {"tests/git_chokepoint.rs": test_diff_is_register_only(CHOKEPOINT_REG_DIFF)}
    v_, _ = apply_register_drift(UNEARNED, LIBTEST_OUT, PRE_SOURCES, drift_map)
    check("register-only diff + ratchet failure -> REGISTER_DRIFT", v_ == REGISTER_DRIFT, v_)
    # NEAR-MISS GUARD, and it is the half that matters: the SAME file, but a diff that
    # touched a real assertion. A void verdict must never eat a genuine finding.
    v_, _ = apply_register_drift(
        UNEARNED, LIBTEST_OUT, PRE_SOURCES,
        {"tests/git_chokepoint.rs": test_diff_is_register_only(BEHAVIOURAL_DIFF)},
    )
    check("behavioural diff + same failure stays UNEARNED", v_ == UNEARNED, v_)
    # PRECEDENCE: every other verdict passes through byte-identically. INCONCLUSIVE and
    # BASELINE_RED must win outright — a build that did not finish cannot have run the
    # assertions it is being judged on — and a green is never rewritten into a void.
    for pass_through in (INCONCLUSIVE, BASELINE_RED, EARNED, NO_TEST_CHANGE,
                         COULD_NOT_CHECK):
        v_, why_ = apply_register_drift(pass_through, LIBTEST_OUT, PRE_SOURCES, drift_map)
        check(f"{pass_through} outranks the drift branch", v_ == pass_through, v_)
        check(f"{pass_through} carries no drift reason", why_ == "", why_)

    reg = CensusRow("d" * 40, "Day 1 (0:0): register bump (Task 1)",
                    ["tests/module_size.rs"], {"tests/module_size.rs"})
    check("register-only is addressable", reg.addressable, reg)
    check("register-only is NOT behavioural", not reg.behavioural, reg)
    mixed = CensusRow(
        "e" * 40, "Day 1 (0:0): gate + bump (Task 1)",
        ["tests/git_chokepoint.rs", "tests/module_size.rs"],
        {"tests/module_size.rs"},
    )
    check("a mixed commit IS behavioural", mixed.behavioural, mixed)
    # ...and the case the hand-list could not express: the SAME file, register-only in
    # this commit. That is 08a9e36f, and it is the whole point of #867.
    chokepoint_reg = CensusRow(
        "h" * 40, "Day 1 (0:0): convert the bypass (Task 2)",
        ["tests/git_chokepoint.rs"], {"tests/git_chokepoint.rs"},
    )
    check(
        "a behavioural-file commit CAN be register-only when its diff says so",
        not chokepoint_reg.behavioural,
        chokepoint_reg,
    )

    # -- the ROLLBACK set: registers are never laid back over post-task src/ --------------
    mixed_paths = ["src/commands_search.rs", "tests/git_chokepoint.rs", "tests/module_size.rs"]
    check(
        "rollback set drops the debt register",
        behavioural_test_files(mixed_paths, {"tests/module_size.rs"})
        == ["tests/git_chokepoint.rs"],
        behavioural_test_files(mixed_paths, {"tests/module_size.rs"}),
    )
    # NEAR-MISS GUARD, and it is the half that matters: the behavioural file is still
    # rolled back. A filter that ate the finding it exists to protect would be worse than
    # no filter, and a discriminator tested only on the side that fires is vacuous green.
    check(
        "rollback set KEEPS the behavioural file",
        "tests/git_chokepoint.rs"
        in behavioural_test_files(mixed_paths, {"tests/module_size.rs"}),
    )
    check(
        "a register-only commit has an EMPTY rollback set",
        behavioural_test_files(["tests/module_size.rs"], {"tests/module_size.rs"}) == [],
    )
    # ...while still being seen as a touched test file, so the two questions stay distinct.
    check(
        "register-only still counts as a touched top-level test file",
        top_level_test_files(["tests/module_size.rs"]) == ["tests/module_size.rs"],
    )
    plain = CensusRow("f" * 40, "Day 1 (0:0): real test (Task 1)", ["tests/integration.rs"])
    check("a non-register test is behavioural", plain.behavioural, plain)
    none = CensusRow("g" * 40, "Day 1 (0:0): no tests (Task 1)", [])
    check("no test files is not behavioural", not none.behavioural, none)
    s2 = census_summary([reg, mixed, plain, none])
    check("fold counts behavioural", s2["behavioural"] == 2, s2)
    check("fold counts register-only", s2["register_only"] == 1, s2)
    check("fold counts addressable", s2["addressable"] == 3, s2)
    check(
        "register_only + behavioural == addressable",
        s2["register_only"] + s2["behavioural"] == s2["addressable"],
        s2,
    )
    check("behavioural never exceeds addressable", s2["behavioural"] <= s2["addressable"], s2)

    check("NO_TEST_CHANGE is not EARNED", NO_TEST_CHANGE != EARNED)
    check("COULD_NOT_CHECK is its own value", COULD_NOT_CHECK not in (EARNED, UNEARNED,
                                                                     INCONCLUSIVE))
    check("five distinct states", len({NO_TEST_CHANGE, EARNED, UNEARNED, INCONCLUSIVE,
                                       COULD_NOT_CHECK}) == 5)

    # -- an empty census is a REFUSAL, and the fold says so honestly ----------------------
    s0 = census_summary([])
    check("empty census reports zero commits", s0["task_commits"] == 0, s0)
    check("empty census reports zero addressable", s0["addressable"] == 0, s0)

    # -- render carries the scope limit and the denominator -------------------------------
    out = render_census(rows, s, "11 commits", None)
    check("render names the signal denominator", "BEHAVIOURAL" in out, out[:300])
    check("render names the register tier", "REGISTER-ONLY" in out, out[:300])
    check("render names the window", "11 commits" in out, out[:300])

    # -- window depth: four deepen states, none folded into a neighbour -------------------
    # The success test is MOVEMENT, not exit status: a fetch can exit 0 and buy nothing.
    st, det = classify_deepen(True, 0, 59, 400)
    check("deepen that moved the count TOOK", st == DEEPEN_TOOK, (st, det))
    check("deepen detail names both counts", "59" in det and "400" in det, det)
    check("deepen detail names the gain", "+341" in det, det)

    st, det = classify_deepen(True, 0, 59, 59)
    check("exit 0 with no movement DID NOT TAKE", st == DEEPEN_DID_NOT_TAKE, (st, det))
    check("no-movement detail says so", "did not move" in det, det)

    st, det = classify_deepen(True, 128, 59, 59)
    check("non-zero exit DID NOT TAKE", st == DEEPEN_DID_NOT_TAKE, (st, det))
    check("failure detail names the exit status", "128" in det, det)

    # An unreadable count is not evidence of movement either.
    st, _ = classify_deepen(True, 0, None, 400)
    check("unreadable before DID NOT TAKE", st == DEEPEN_DID_NOT_TAKE, st)
    st, _ = classify_deepen(True, 0, 59, None)
    check("unreadable after DID NOT TAKE", st == DEEPEN_DID_NOT_TAKE, st)

    # NEAR-MISS GUARD: a complete log is not a failure, it is nothing to buy.
    st, det = classify_deepen(False, 0, None, None)
    check("already-deep is its own state", st == DEEPEN_ALREADY_DEEP, (st, det))
    check("already-deep says no fetch ran", "no fetch" in det, det)

    # None (probe failed) must NOT be folded into False -- "I could not tell" is not
    # "the log is already complete".
    st, det = classify_deepen(None, 0, 59, 400)
    check("unknown shallowness still fetches", st == DEEPEN_TOOK, (st, det))
    check("unknown shallowness is disclosed", "probe failed" in det, det)
    check(
        "four deepen states are distinct",
        len({DEEPEN_NOT_REQUESTED, DEEPEN_ALREADY_DEEP, DEEPEN_TOOK,
             DEEPEN_DID_NOT_TAKE}) == 4,
    )

    # -- the window note: BOTH branches speak, and they do not render alike ---------------
    shallow_lines = window_note(True, "59", DEEPEN_NOT_REQUESTED, "")
    joined = "\n".join(shallow_lines)
    check("shallow window is named", "SHALLOW WINDOW" in joined, joined)
    check("shallow window names the depth", "59" in joined, joined)
    check("shallow window says it bounds downward", "DOWNWARD" in joined, joined)
    deep_lines = window_note(False, "400", DEEPEN_NOT_REQUESTED, "")
    deep_joined = "\n".join(deep_lines)
    check("complete log also speaks", "complete log" in deep_joined, deep_joined)
    check("complete log denies the bound", "NOT bounded" in deep_joined, deep_joined)
    check("the two windows do not render alike", joined != deep_joined)

    # A deepen that did not take must SAY the window is the pre-existing one --
    # "could not check" must never read as "checked; clean".
    failed = "\n".join(window_note(True, "59", DEEPEN_DID_NOT_TAKE, "exited 128"))
    check("failed deepen is loud", "DID NOT TAKE" in failed, failed)
    check("failed deepen carries the status", "128" in failed, failed)
    check("failed deepen disowns the window", "not the one asked for" in failed, failed)
    took = "\n".join(window_note(False, "400", DEEPEN_TOOK, "59 -> 400 commits (+341)"))
    check("successful deepen reports the gain", "TOOK" in took and "+341" in took, took)
    # NEAR-MISS GUARD: no --deepen means no deepen STATUS line -- asserted against the
    # one shared prefix constant, never against the bare word "deepen", which the shallow
    # warning legitimately uses to name its own remedy. The first version of this guard
    # asserted the word and so failed a correct renderer: a mention is not a status line.
    quiet = "\n".join(window_note(True, "59", DEEPEN_NOT_REQUESTED, ""))
    check(
        "unrequested deepen renders no deepen status line",
        DEEPEN_STATUS_PREFIX not in quiet,
        quiet,
    )
    # ...and none of the three verdict words leaks either, so the guard cannot pass by
    # the prefix merely being reworded out from under it.
    check(
        "unrequested deepen renders no deepen verdict",
        not any(w in quiet for w in ("TOOK", "DID NOT TAKE", "NOT NEEDED")),
        quiet,
    )
    # The other side of the same discriminator: when a deepen IS requested, the status
    # line must be present -- a guard checked only where it fires is vacuous green.
    for state in (DEEPEN_TOOK, DEEPEN_DID_NOT_TAKE, DEEPEN_ALREADY_DEEP):
        loud = "\n".join(window_note(True, "59", state, "detail"))
        check(f"requested deepen renders a status line ({state})",
              DEEPEN_STATUS_PREFIX in loud, loud)
    # The shallow warning DOES name its remedy -- that is the behaviour the old guard
    # mistook for a defect, so it is pinned rather than left to chance.
    check("shallow window names --deepen as the remedy", "--deepen" in quiet, quiet)

    # The note reaches the rendered census (the string an operator actually reads).
    rendered = render_census(rows, s, "11 commits", None, shallow_lines)
    check("render carries the window note", "SHALLOW WINDOW" in rendered, rendered[:400])

    check("limits state the intent boundary", "intent is not" in LIMITS.lower())
    check("limits state the src/ scope", "165k" in LIMITS)
    check("limits state the register caveat", "DEBT REGISTER" in LIMITS)

    # -- THE LEDGER: the reading must accumulate across sessions ------------------------
    # baseline_from_verdict: derived from run_counterfactual's short-circuit, so every
    # one of the seven states gets a row rather than a shrug.
    for verdict, expect in (
        (EARNED, "green"), (UNEARNED, "green"), (INCONCLUSIVE, "green"),
        (REGISTER_DRIFT, "green"), (BASELINE_RED, "red"),
        (NO_TEST_CHANGE, "not-run"), (COULD_NOT_CHECK, "unknown"),
    ):
        got = baseline_from_verdict(verdict)
        check(f"baseline_from_verdict({verdict}) == {expect}", got == expect, got)

    line = ledger_line("abc", "def", "184", "subj", POP_PLAIN, EARNED, "green", "T", "42")
    back = json.loads(line)
    check("ledger_line round-trips the sha", back["sha"] == "abc", back)
    check("ledger_line carries the verdict", back["verdict"] == EARNED, back)
    check("ledger_line is ONE line", "\n" not in line, line)

    # A malformed line is COUNTED, never silently dropped -- a shrinking denominator
    # inside my own meter is the defect this family of checks exists for.
    shas, malformed = parse_ledger(
        line + "\n"
        + "{not json at all\n"
        + "\n"                       # blank: normal JSONL, neither record nor corruption
        + '{"no_sha": 1}\n'
        + '["a list"]\n'
    )
    check("parse_ledger keeps the good sha", shas == {"abc"}, shas)
    check("parse_ledger COUNTS the 3 malformed lines", malformed == 3, malformed)

    # select_runnable: population filter, behavioural filter, resume skip, newest-first.
    def _row(sha, pop_suffix="", reg=()):
        subj = f"Day 180 (12:00): thing {sha} (Task 1{pop_suffix})"
        return CensusRow(sha, subj, ["tests/a.rs"], set(reg))

    rows = [_row("s1"), _row("s2"), _row("s3")]
    rows.append(CensusRow("s4", "Day 180 (12:00): x (Task 1, eval-fix 1)", ["tests/a.rs"]))
    rows.append(CensusRow("s5", "Day 180 (12:00): y (Task 1)", ["tests/r.rs"], {"tests/r.rs"}))

    picked = [r.sha for r in select_runnable(rows, POP_PLAIN, set(), None)]
    check("select_runnable takes plain behavioural, newest first",
          picked == ["s1", "s2", "s3"], picked)
    picked = [r.sha for r in select_runnable(rows, POP_PLAIN, {"s1"}, None)]
    check("--resume SKIPS a recorded sha", picked == ["s2", "s3"], picked)
    picked = [r.sha for r in select_runnable(rows, POP_PLAIN, set(), 2)]
    check("--max-runs bounds the batch", picked == ["s1", "s2"], picked)
    picked = [r.sha for r in select_runnable(rows, POP_FIX_LOOP, set(), None)]
    check("populations are never summed", picked == ["s4"], picked)
    check("register-only commits are not runnable",
          "s5" not in [r.sha for r in select_runnable(rows, POP_PLAIN, set(), None)])

    _tmp = tempfile.mkdtemp(prefix="yoyo-ledger-selftest-")
    try:
        missing = os.path.join(_tmp, "nope.jsonl")
        state, shas, _ = read_ledger(missing)
        # THE FAIL-SAFE DIRECTION: an absent ledger skips NOTHING. If this ever returned a
        # non-empty set, "could not check" would be reading as "checked; clean".
        check("missing ledger is its own state", state == LEDGER_MISSING, state)
        check("missing ledger skips nothing", shas == set(), shas)

        path = os.path.join(_tmp, "led.jsonl")
        # APPEND PER VERDICT: three separate calls, order preserved, each independently
        # durable. A process killed after the second must still hold the first two.
        for sha in ("aaa", "bbb", "ccc"):
            err = append_ledger(
                path, ledger_line(sha, "p", "1", "s", POP_PLAIN, EARNED, "green", "T", "9")
            )
            check(f"append_ledger({sha}) succeeded", err == "", err)
        # Read back WITHOUT assuming the file exists. A writer that appends nothing must
        # produce a clean red here, not a traceback: a self-test that crashes destroys the
        # very summary that says which OTHER tests were fine, which is what makes the
        # positive control unreadable.
        if os.path.exists(path):
            with open(path, "r", encoding="utf-8") as fh:
                got = [json.loads(ln)["sha"] for ln in fh.read().splitlines() if ln.strip()]
        else:
            got = "NO LEDGER FILE WAS WRITTEN"
        check("ledger appends per verdict, in order", got == ["aaa", "bbb", "ccc"], got)
        state, shas, _ = read_ledger(path)
        check("read_ledger sees every appended sha",
              state == LEDGER_READ and shas == {"aaa", "bbb", "ccc"}, (state, shas))
    finally:
        shutil.rmtree(_tmp, ignore_errors=True)

    if failures:
        print(f"SELF-TESTS FAILED ({len(failures)}):", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print(f"ALL PASSED (self-tests over {len(RUN_VERDICTS)} run verdicts + census fold)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
