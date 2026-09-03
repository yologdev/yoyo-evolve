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
NO_PRE_EXISTING_TEST_EDIT = "NO_PRE_EXISTING_TEST_EDIT"
EARNED = "EARNED"
UNEARNED = "UNEARNED"
INCONCLUSIVE = "INCONCLUSIVE"
COULD_NOT_CHECK = "COULD_NOT_CHECK"
BASELINE_RED = "BASELINE_RED"
REGISTER_DRIFT = "REGISTER_DRIFT"

# The six a live run can produce. NO_TEST_CHANGE and NO_PRE_EXISTING_TEST_EDIT are both
# decided by the DIFF, before any run, so neither is in here.
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

# --------------------------------------------------------------------------------------
# #880 -- the three states of "which test failed?". NEVER folded into each other, and the
# middle one is the whole point of there being three:
#
#   FAILING_NAMES        a failure block was found and the names were read.
#   FAILING_NO_BLOCK     the run REPORTED a failure and the capture carries no libtest
#                        failure block, so the names could not be read. This is NOT "no
#                        failures". Rendering it as a bare empty list would be "could not
#                        check" reading as "checked; clean", the one collapse this whole
#                        instrument refuses.
#   FAILING_NO_OUTPUT    the capture is empty or whitespace only -- nobody looked, or the
#                        output was lost. A different fact again from either neighbour.
#
# These are NOT verdicts and never enter RUN_VERDICTS: a verdict says what happened to the
# green, this says whether the reason is legible.
FAILING_NAMES = "names"
FAILING_NO_BLOCK = "no_failure_block"
FAILING_NO_OUTPUT = "no_output"

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


def classify_failing_tests(output: str) -> tuple[str, list[str]]:
    """WHICH tests failed, from the capture a verdict was already read out of (#880).

    Pure over the captured string, exactly like `classify_baseline` and
    `classify_counterfactual` beside it, so it is table-testable with fabricated output
    and never runs cargo.

    ONE PARSER, NOT TWO. The name extraction delegates to `failing_test_names`, which
    already existed and is what `attribute_failures` uses for REGISTER_DRIFT attribution.
    A second extractor would be two copies of one rule agreeing the day they are written
    and diverging forever after -- the `significant_braces` / `char_literal_len`
    precedent. What was missing was never the parser; it was putting its answer on the
    LEDGER ROW.

    Returns `(status, names)` where status is one of the three FAILING_* values above.
    The list is empty for both non-`FAILING_NAMES` statuses, which is exactly why the
    status must be read and never the list alone: `failing_test_names` returns `[]` for
    "the capture had no failure block" AND for "there was no capture", and those are two
    different facts with two different remedies.
    """
    if not output or not output.strip():
        return FAILING_NO_OUTPUT, []
    names = failing_test_names(output)
    if names:
        return FAILING_NAMES, names
    return FAILING_NO_BLOCK, []


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


TEST_DIFF_NONE = "TEST_DIFF_NONE"
TEST_DIFF_ADD_ONLY = "TEST_DIFF_ADD_ONLY"
TEST_DIFF_TOUCHES_PRE_EXISTING = "TEST_DIFF_TOUCHES_PRE_EXISTING"

# The fourth value, and it belongs to the SHAPE LOOKUP rather than to the classifier: the
# `git diff` failed, the commit has no parent (root / shallow boundary), the output would
# not parse, or the answer contradicts what the census already measured. It is NEITHER
# `add_only` NOR `signal_bearing`.
#
# CHECK THE ERROR DIRECTION BEFORE PICKING A FALLBACK (Day 186, #875). Folding an unknown
# into `add_only` UNDERSTATES the reachable denominator, which is the flattering direction
# — it makes the milestone look closer than it is. Folding it into `signal_bearing`
# OVERSTATES it, which is the exact defect #875 exists to remove. So it gets its own name,
# is counted and reported, and is summed into neither.
SHAPE_UNKNOWN = "SHAPE_UNKNOWN"


def classify_test_diff_shape(rows) -> str:
    """Is this commit's `tests/` diff strictly ADD-ONLY? Decided from the diff alone.

    Day 186. `rows` are `(status, path)` pairs as produced by
    `git diff --name-status <parent> <sha> -- tests/`. Scope is decided by
    `top_level_test_files`, NEVER by a second `tests/*.rs` predicate — two copies of a
    rule agree the day they are written and diverge forever after.

    WHY THIS EXISTS. Measured Day 185: the void rate was 6 of 12, and all six voids were
    the SAME shape — a commit adding exactly one new top-level `tests/*.rs` and modifying
    no pre-existing one, so `parent_test_pathspec` returned an empty `kept` and the run
    refused. Those six are not unanswerable. They are answerable BY ARGUMENT, and running
    them is pure cost (~3m07s of cargo per reading):

      1. For every test file the commit did NOT touch, the parent version IS the post-task
         version, so laying it back is a no-op.
      2. The only remaining difference is the added file(s), which did not exist at the
         parent, so the correct counterfactual OMITS them.
      3. The counterfactual tree is therefore exactly the post-task tree minus the added
         test files.
      4. The post-task tree is green — the commit landed, and `scripts/evolve.sh` reverts
         anything that is not.
      5. Removing a test file cannot turn a green run red: top-level test files are
         separate crates and nothing compiles *against* them.

    Hence the verdict is deterministically earned, provable without running anything.

    AND IT IS A **VACUOUS** EARNED, which is the whole reason it gets its own name. It
    says only "you weakened no pre-existing assertion, because you touched none." Folding
    it into `EARNED` would inflate the numerator of the exact rate DREAM.md asks for with
    commits that could not possibly have come out unearned — the denominator-inflation
    defect this instrument exists to refuse.

    Four rules, each its own table row:

      * Zero in-scope rows -> `TEST_DIFF_NONE`. That is the existing `NO_TEST_CHANGE`
        territory and must NOT become the new state. ANTI-VACUOUS: an empty input can
        never yield ADD_ONLY, which is this defect wearing the opposite sign.
      * Every in-scope row is an add (`A`), and there is at least one -> `ADD_ONLY`.
      * Any `M`, `D` or `R*` on an in-scope path -> `TOUCHES_PRE_EXISTING`, even when adds
        sit in the same diff. A rename touches the pre-existing set. THIS IS THE NEAR-MISS
        GUARD AND IT IS THE HALF THAT MATTERS: misclassifying a mixed diff as add-only
        would skip a run that could have produced the first `UNEARNED`.
      * A status letter the parser does not recognise -> `TOUCHES_PRE_EXISTING`, i.e. fail
        toward RUNNING it. An unknown must never be promoted into the comfortable bucket
        (Day 144).

    THE STATED LIMIT, and it is narrower than the name suggests: this proves the commit
    weakened no pre-existing **top-level** assertion. It says nothing about a
    `#[cfg(test)]` assertion inside `src/` — those ~157k lines ride the counterfactual
    tree at their POST-task version and are structurally outside this instrument (#870,
    which this does NOT close).
    """
    saw_add = False
    for status, path in rows:
        if not top_level_test_files([path]):
            continue
        if status.strip().upper() == "A":
            saw_add = True
        else:
            # M / D / R* / C* / T / anything unrecognised: run it.
            return TEST_DIFF_TOUCHES_PRE_EXISTING
    return TEST_DIFF_ADD_ONLY if saw_add else TEST_DIFF_NONE


def parse_name_status(text: str) -> list:
    """`git diff --name-status` text -> `(status, path)` pairs. Renames yield BOTH paths.

    A rename row is `R100<TAB>old<TAB>new`; both sides are emitted so an in-scope old path
    cannot hide behind an out-of-scope new one (or the reverse). The classifier treats any
    non-`A` in-scope status as touching the pre-existing set, so either hit is enough.
    """
    out = []
    for line in text.splitlines():
        parts = line.rstrip("\n").split("\t")
        if len(parts) < 2 or not parts[0].strip():
            continue
        status = parts[0].strip()
        for path in parts[1:]:
            if path.strip():
                out.append((status, path.strip()))
    return out


def parent_test_pathspec(
    post_task_test_files: list[str], parent_test_files
) -> tuple[list[str], list[str]]:
    """Split the rollback set into what EXISTS at the parent and what does not.

    Day 185. The counterfactual lays PRE-task `tests/` over POST-task `src/`, so a test
    file the commit CREATED has nothing older to lay back — `git checkout <parent> --
    tests/new.rs` fails with `pathspec ... did not match any file(s) known to git` and,
    before this, **aborted the whole run**. Measured over the first 7 verdicts: 2 were
    COULD_NOT_CHECK and both had exactly that cause (`5c82fef5` adds
    tests/git_chokepoint.rs, `db04d300` adds tests/gasp_session_end_guard.rs).

    That void is **not random**. This repo ships invariant gates constantly — nine in
    ~20 days — so aborting systematically removes GATE-LANDING commits, i.e. the
    population most likely to be *about* assertions, from the readable denominator.

    The verdict was honest for the new file and wrong for its neighbours: pre-task tests
    genuinely have nothing to say about a file that did not exist, but they have plenty
    to say about the ones that did. So the pathspec is the INTERSECTION, and the truthful
    overlay is "the tests that existed before, as they were before" — the new files are
    simply ABSENT.

    Returns `(kept, absent)`, both in the caller's order so the pathspec is stable:

      * `kept`   — files present at the parent. This is the checkout pathspec.
      * `absent` — files the commit created. REPORTED, never silently dropped: a partial
                   overlay reported as a whole one is the same category error as
                   "could not check" reading as "checked; clean", and a shrinking
                   denominator inside my own meter is the defect this instrument exists
                   to refuse.

    THE DANGEROUS BRANCH IS THE CALLER'S, AND IT IS AN EMPTY `kept`. If every test file
    the commit touches is new there is nothing older to lay back; proceeding would run
    POST-task tests against POST-task `src/`, which is just the baseline again, and would
    manufacture a **false EARNED**. Widening what counts as readable is exactly the
    direction that can invent an earned green, so the caller must branch on it explicitly
    rather than falling through.

    Byte-identical when the commit adds no new test file — `kept` is the input list and
    `absent` is empty. That is every one of the 7 readings taken so far and the whole
    regression surface.
    """
    parent = set(parent_test_files or ())
    kept = [p for p in post_task_test_files if p in parent]
    absent = [p for p in post_task_test_files if p not in parent]
    return kept, absent


# --------------------------------------------------------------------------------------
# #870 slice 1: the `#[cfg(test)]` splicer. PURE, AND DELIBERATELY UNWIRED.
#
# Three states, none folded into another. They live beside the verdict constants because
# they are the same kind of thing one file-granularity down, and they are NOT in
# RUN_VERDICTS: nothing here produces a verdict, writes a ledger line, or is called by
# `run_counterfactual`. See `splice_test_module`'s docstring for what this does not do.
# --------------------------------------------------------------------------------------

SPLICE_OK = "SPLICE_OK"
SPLICE_NO_POST_MARKER = "SPLICE_NO_POST_MARKER"
SPLICE_NO_PRE_MARKER = "SPLICE_NO_PRE_MARKER"


def test_module_start(src: str) -> int | None:
    """Line index of a file's **module-level** `#[cfg(test)] mod` marker, or `None`.

    ONE STATEMENT OF THE RULE, read by both halves of `splice_test_module`. Two copies of
    a rule agree the day they are written and diverge forever after, which is why
    `commands_refactor::significant_braces` was made `pub(crate)` rather than copied.

    THE RULE, AND ITS PROVENANCE. `tests/git_chokepoint.rs` truncates each file at the
    `#[cfg(test)]` attribute line **followed by a column-0 `mod`**, and its module doc
    records why a bare `#[cfg(test)]` split is wrong. Measured here rather than inherited:
    `src/git.rs` carries item-level `#[cfg(test)]` attributes at lines 8, 44 and 77 — at
    **column 0**, followed by `const` and `fn` — and its real test module is at line 947.
    So requiring column 0 on the *attribute* discriminates nothing. The column-0 `mod` on
    the **next** line is the whole discriminator, and a bare split would blank the
    chokepoint file itself, i.e. hand the caller an empty production half that compiles to
    nothing and reads as a catastrophic result.

    Returns the index of the ATTRIBUTE line, not of the `mod` line: the attribute belongs
    to the test half, so splicing at the attribute keeps each side's own gate with it.

    The first marker wins. A file with two module-level test modules is not a shape that
    occurs in `src/` today, and taking the last one would silently drop production code
    sitting between them into the pre-task half.
    """
    lines = src.splitlines()
    # Assembled at runtime, the discipline every source-level guard in this repo uses
    # (`src/git.rs:1757` builds the same needle with `format!`). This file is never
    # pointed at itself today — the splicer reads `src/*.rs` — but a needle that cannot
    # match its own source costs nothing and removes the question.
    needle = "#[cfg" + "(test)]"
    for i in range(len(lines) - 1):
        if lines[i].strip() != needle:
            continue
        nxt = lines[i + 1]
        # Column-0 `mod` ONLY. An indented `mod`, a `fn`, a `const` or a blank line all
        # mean this attribute guards an item, not the test module.
        if nxt.startswith("mod ") or nxt.startswith("mod\t"):
            return i
    return None


def splice_test_module(post_task_src: str, pre_task_src: str) -> tuple[str, str | None]:
    """Build one `src/` file's counterfactual tree: post-task production + pre-task tests.

    #870 slice 1, Day 187. Returns `(status, text)`.

    WHY. The backward counterfactual lays PRE-task `tests/` over POST-task `src/`, which
    reaches the 12 top-level `tests/*.rs` and **nothing else**. Rust buries unit tests
    inside 91 `src/` files behind `#[cfg(test)]`, and those ride the counterfactual tree
    at their POST-task version — so a commit that loosened a `#[cfg(test)]` assertion
    inside `src/` is invisible. That is not a marginal blind spot for the question
    DREAM.md actually asks: of 199 fix-loop commits, **11** touch a top-level `tests/*.rs`
    while **~88** edit assertion-shaped code inside `src/`. The fix-loop arm holds **1**
    signal-bearing commit, and no amount of reading moves it while the wall stands.

    This is the same claim the `tests/` overlay already makes, one file-granularity down:
    a `src/` file's counterfactual is its post-task **production** half plus its pre-task
    **test** half.

    THREE STATES, NONE FOLDED INTO ANOTHER:

      * `SPLICE_OK`               -> both sides carry a module-level marker; `text` is the
                                     spliced source.
      * `SPLICE_NO_POST_MARKER`   -> the post-task file has no `#[cfg(test)] mod`. A
                                     DIFFERENT FACT: the commit deleted the test module,
                                     or the file never had one. `text` is `None`.
      * `SPLICE_NO_PRE_MARKER`    -> the pre-task file has none, i.e. the test module is
                                     **new in this commit**. `text` is `None`.

    `NO_PRE_MARKER` is the add-only shape one file-granularity down and is **vacuous** for
    exactly the reason `NO_PRE_EXISTING_TEST_EDIT` is: you cannot have weakened a
    pre-existing assertion in a module that did not exist. Collapsing it into
    `NO_POST_MARKER` — or either into a verdict — would be the denominator-inflation
    defect this whole instrument exists to refuse.

    IDENTITY IS THE REGRESSION SURFACE. When both halves are byte-identical the splice
    returns the input byte-for-byte, because `post[:i] + pre[i:]` with `pre == post` is
    `post`. Any future wiring inherits that property, so it is asserted with `==` on the
    whole string rather than a `contains`.

    THREE LIMITS, stated here and not only in CLAUDE.md:

      1. It is a **text scan, not a Rust parser**. A `#[cfg(test)]` inside a string
         literal or a comment would be mis-read. There are none in `src/` today.
      2. It matches the **module-level** marker only, so per-item `#[cfg(test)]` functions
         scattered outside a `mod` block are invisible to it — and those are exactly what
         `src/git.rs` carries at lines 8/44/77, which stay on the production side.
      3. **This does not make the fix-loop arm measurable, and it closes nothing.** It is
         one piece of #870 option 2. Nothing is wired: `run_counterfactual`,
         `select_runnable`, the ledger, the census and every verdict state are untouched,
         no verdict changes, no ledger line is written, and the fix-loop arm is still 1
         signal-bearing commit after this lands.
    """
    post_start = test_module_start(post_task_src)
    if post_start is None:
        return SPLICE_NO_POST_MARKER, None
    pre_start = test_module_start(pre_task_src)
    if pre_start is None:
        return SPLICE_NO_PRE_MARKER, None

    # keepends=True so line endings survive verbatim: a splice that normalises CRLF or
    # eats a trailing newline is a different file, and the identity assertion is the only
    # thing that would notice.
    post_lines = post_task_src.splitlines(keepends=True)
    pre_lines = pre_task_src.splitlines(keepends=True)
    return SPLICE_OK, "".join(post_lines[:post_start]) + "".join(pre_lines[pre_start:])


def src_splice_candidates(rows) -> list[str]:
    """Paths a `src/` splice may be attempted on, from `git diff --name-status` rows.

    #870 slice 2, Day 187. The selector half of the splicer: slice 1 built
    `splice_test_module` and wired it to nothing; this decides WHICH files it is offered.

    IN SCOPE: a MODIFIED (`M`) path under `src/` ending `.rs`. Nothing else, and each
    exclusion has its own reason rather than a shared shrug:

      * `A` (added)   -> no parent version exists; there is nothing older to lay back.
      * `D` (deleted) -> the file is gone from the post-task tree; there is nothing to
                         splice INTO.
      * `R*` (rename) -> the parent path differs from the post-task path, so resolving
                         which pre-task file to read is a separate decision. `R100 old new`
                         yields BOTH paths from `parse_name_status`, and neither is `M`,
                         so both are correctly out of scope.
      * outside `src/`, or not `.rs` -> not this overlay's business. `tests/` is handled
                         by `parent_test_pathspec`, and a non-Rust file has no
                         `#[cfg(test)]` module to move.
      * an unrecognised status letter -> OUT of scope, deliberately.

    THE SAFETY PROPERTY, and it decides every ambiguous case above: **every refusal here
    must fail toward EARNED, never toward UNEARNED.** Skipping a file leaves it at its
    post-task version, i.e. exactly today's shallower reading — the worst outcome is a
    reading no deeper than the 25 already in the ledger. Splicing a file we understood
    wrongly could break a test the commit never touched and manufacture a false
    `UNEARNED`, which is a public accusation that a past commit's green was bought with
    test edits, and it is the one verdict I have committed never to re-run until it moves.
    So when in doubt, leave the file alone.

    It reuses `parse_name_status` rather than parsing status lines a second time — two
    copies of a rule agree the day they are written and diverge forever after.

    Order is first-seen and duplicates are dropped, so the caller's splice order is
    stable and a path cannot be rewritten twice.
    """
    seen: set = set()
    out: list[str] = []
    for status, path in rows or ():
        if str(status).strip() != "M":
            continue
        p = str(path).strip()
        if not p.startswith("src/") or not p.endswith(".rs"):
            continue
        if p in seen:
            continue
        seen.add(p)
        out.append(p)
    return out


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


def census_shape_split(rows: list[CensusRow], shape_of) -> dict:
    """Split the BEHAVIOURAL count by test-diff shape. Pure: `shape_of` is injected.

    Day 186, #875. WHY THIS EXISTS, and it is a correction to my own headline rather than
    a new feature. Every running tally has been phrased "N of 45, M short of >=20", where
    45 is the behavioural count. But an ADD-ONLY commit can never yield a classifiable
    verdict — `classify_test_diff_shape` answers it from the diff as
    `NO_PRE_EXISTING_TEST_EDIT`, a VACUOUS earned that is deliberately excluded from the
    rate — so some fraction of that 45 was never reachable. Measured Day 186: 5 of 8
    sampled behavioural commits were add-only. The behavioural figure OVERSTATES the
    reachable denominator, and this says by how much.

    THREE STATES, NEVER TWO, and the third is the safety property:

      * `signal_bearing`  — TEST_DIFF_TOUCHES_PRE_EXISTING. The ONLY commits that can
                            produce EARNED / UNEARNED / INCONCLUSIVE. This, not
                            `behavioural`, is the denominator DREAM.md's rate can use.
      * `add_only`        — TEST_DIFF_ADD_ONLY. Answered from the diff, outside the rate.
      * `shape_unknown`   — SHAPE_UNKNOWN. Counted and reported, summed into neither. See
                            the constant for why the error direction forbids a fallback.

    `TEST_DIFF_NONE` coming back for a row the census already called behavioural is a
    DISAGREEMENT between two lookups (`--name-only` over the whole commit vs
    `--name-status` scoped to `tests/`), not a fourth shape, so it lands in
    `shape_unknown` — the bucket for "the lookup did not give a usable answer".

    INJECTION IS THE POINT: `shape_of(sha) -> str` keeps this table-testable with
    fabricated shapes and no git at all, the same discipline `added_ts` uses in
    `never_forecast_files` and the resolved title uses in `revisit_add_at`.

    COST, stated so nobody widens it by accident: `shape_of` is asked ONLY for rows the
    census already classified behavioural (~47 across all three populations today), never
    for the ~1014 task commits. Do not move this into the main log walk.
    """
    beh = [r for r in rows if r.behavioural]
    counts = {"behavioural": len(beh), "signal_bearing": 0, "add_only": 0,
              "shape_unknown": 0}
    for r in beh:
        shape = shape_of(r.sha)
        if shape == TEST_DIFF_TOUCHES_PRE_EXISTING:
            counts["signal_bearing"] += 1
        elif shape == TEST_DIFF_ADD_ONLY:
            counts["add_only"] += 1
        else:
            counts["shape_unknown"] += 1
    # ANTI-VACUOUS, and it is asserted by the RENDERER first (see
    # `render_population_block`): behavioural > 0 with EVERY lookup unknown must say so
    # out loud rather than print "0 add-only". A scanner that finds nothing and reports a
    # clean split is this very defect wearing the opposite sign, and it is quieter.
    counts["all_unknown"] = bool(beh) and counts["shape_unknown"] == len(beh)
    return counts


def census_shape_split_by_population(rows: list[CensusRow], shape_of) -> dict:
    """One shape split per population, never summed — same rule as `census_by_population`.

    The populations stay separate for DREAM.md's reason: the fix-loop arm IS the
    pre-registered guess, and pooling it away destroys the only question the widened
    `TASK_COMMIT_RE` was widened to ask.
    """
    return {
        pop: census_shape_split([r for r in rows if r.population == pop], shape_of)
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


def commit_test_diff_shape(root: str, sha: str) -> str:
    """The I/O half of the shape split: what shape is THIS commit's `tests/` diff?

    Day 186, #875. Returns one of `TEST_DIFF_TOUCHES_PRE_EXISTING` / `TEST_DIFF_ADD_ONLY`
    / `TEST_DIFF_NONE`, or `SHAPE_UNKNOWN` when the question could not be asked.

    EVERY PIECE HERE ALREADY EXISTED — this reuses and never re-derives. `run_cmd` issues
    the diff, `parse_name_status` parses it, `classify_test_diff_shape` classifies it, and
    scope comes from `top_level_test_files` inside that classifier. There is deliberately
    no second `tests/*.rs` predicate: two copies of a rule agree the day they are written
    and diverge forever after (the `significant_braces` precedent, which #875 names).

    A `<sha>^` that does not resolve — a root commit, or a shallow-clone boundary — is
    `SHAPE_UNKNOWN`, not a crash and not a fallback into either real bucket.
    """
    rc, out = run_cmd(
        ["git", "-C", root, "diff", "--name-status", f"{sha}^", sha, "--", "tests/"],
        timeout=60,
    )
    if rc != 0:
        return SHAPE_UNKNOWN
    return classify_test_diff_shape(parse_name_status(out))


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


def run_counterfactual(
    root: str, sha: str, timeout: int, target: str | None = None,
    splice_src: bool = False,
) -> tuple[str, str, tuple[str, list[str]] | None, dict | None]:
    """Public contract: `(verdict, detail, failing, splice)` — a thin wrapper over `_run_counterfactual`.

    `failing` is the `(status, names)` pair from `classify_failing_tests` for the two
    verdicts whose CAUSE is legible in a capture — BASELINE_RED and UNEARNED — and `None`
    everywhere else, meaning **no claim was made**, which is not the same as "no failures".

    `splice` (#870 slice 2) is `None` when `splice_src` is False — the DEFAULT — so the
    verdict, the detail string, the ledger row and the git/cargo calls made are all
    byte-identical to every one of the 25 readings already taken. When True it is
    `{"spliced": N, "refused": M}`, recorded so rows of different DEPTH can never be
    pooled into one rate.

    THIS IS A WRAPPER RATHER THAN 14 EDITED RETURNS ON PURPOSE. `_run_counterfactual` has
    fourteen return sites and only two of them ever see a capture worth naming; widening
    the arity of all fourteen to thread a value twelve of them cannot supply is churn that
    buys nothing and risks a mis-edited branch in the one function the whole tool hangs
    on. The sink is written at exactly the branches that already have the value, and is
    inert everywhere else.
    """
    sink: dict = {"failing": None, "splice": None}
    verdict, detail = _run_counterfactual(root, sha, timeout, target, sink, splice_src)
    return verdict, detail, sink["failing"], sink["splice"]


def _run_counterfactual(
    root: str,
    sha: str,
    timeout: int,
    target: str | None = None,
    sink: dict | None = None,
    splice_src: bool = False,
) -> tuple[str, str]:
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

    # ---- ADD-ONLY (Day 186): answerable by ARGUMENT, so no cargo runs at all. ----------
    # Decided from the diff, so it sits here with NO_TEST_CHANGE — ahead of everything a
    # run can produce. BASELINE_RED, INCONCLUSIVE and REGISTER_DRIFT are all run-derived
    # and cannot compete with it, because no run happens. This is also where the
    # throughput comes from: ~3m07s of cargo per reading, skipped on 6 of the first 12.
    rc_ns, ns_out = run_cmd(
        ["git", "-C", root, "diff", "--name-status", parent, sha, "--", "tests/"],
        timeout=60,
    )
    if rc_ns != 0:
        # Could not read the shape -> fall through and RUN it. An unknown must never be
        # promoted into the comfortable bucket (Day 144).
        pass
    elif classify_test_diff_shape(parse_name_status(ns_out)) == TEST_DIFF_ADD_ONLY:
        return NO_PRE_EXISTING_TEST_EDIT, (
            "tests/ diff is strictly ADD-ONLY ("
            + ", ".join(rollback)
            + ") — no pre-existing assertion could have been weakened, so the "
            "counterfactual is the post-task tree minus the added file(s) and is green "
            "by construction. VACUOUS earned: excluded from the rate. No cargo run."
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
            # #880: the capture is in hand HERE and was thrown away until Day 187, which
            # is why all three recorded BASELINE_RED rows say `1 failed` and cannot say
            # WHICH. Read it from the same string `classify_baseline` just read.
            if sink is not None:
                sink["failing"] = classify_failing_tests(out)
            return baseline, "BASELINE (parent whole): " + summarise(out)

        # ---- COUNTERFACTUAL: post-task src/, pre-task tests/. -------------------------
        rc, out = run_cmd(["git", "-C", wt, "checkout", "--detach", sha], timeout=120)
        if rc != 0:
            return COULD_NOT_CHECK, f"checkout of {sha[:8]} failed: {out.strip()[:200]}"

        # Lay the PRE-task BEHAVIOURAL tests back over the POST-task src/. Debt registers
        # are deliberately NOT rolled back — see behavioural_test_files.
        #
        # Day 185: only the files that EXISTED at the parent may be laid back. A commit
        # that CREATES a test file has nothing older for it, and checking out the whole
        # post-task list used to fail the pathspec and ABORT the run — 2 of the first 7
        # verdicts were voided that way, and not randomly: this repo ships invariant
        # gates constantly, so the abort removed gate-landing commits (the population
        # most likely to be about assertions) from the readable denominator.
        rc_ls, ls_out = run_cmd(
            ["git", "-C", root, "ls-tree", "--name-only", parent, "--", "tests/"],
            timeout=60,
        )
        if rc_ls != 0:
            return COULD_NOT_CHECK, f"ls-tree of parent tests/ failed (rc={rc_ls})"
        kept, absent = parent_test_pathspec(
            rollback, top_level_test_files(ls_out.splitlines())
        )
        absent_note = ""
        if absent:
            # REPORTED, never silently dropped: a partial overlay reported as a whole one
            # is the same category error as "could not check" reading as "checked; clean".
            absent_note = (
                f"PARTIAL OVERLAY: {len(absent)} test file(s) did not exist at the parent "
                f"and were left at their shipped version ({', '.join(absent)}). "
            )
            print(f"    {absent_note.strip()}", flush=True)
        if not kept:
            # THE DANGEROUS BRANCH, explicit and never a fall-through. Every touched test
            # file is new, so there is nothing older to lay back; proceeding would run
            # POST-task tests against POST-task src/ — just the baseline again — and would
            # manufacture a FALSE EARNED. Widening what counts as readable is exactly the
            # direction that can invent an earned green.
            return COULD_NOT_CHECK, (
                "every touched test file is NEW at this commit ("
                + ", ".join(absent)
                + ") — nothing older to lay back, so there is no counterfactual to run"
            )
        rc, out = run_cmd(
            ["git", "-C", wt, "checkout", parent, "--"] + kept, timeout=120
        )
        if rc != 0:
            return COULD_NOT_CHECK, f"checkout of pre-tests failed: {out.strip()[:200]}"

        # ---- #870 slice 2: lay the pre-task `#[cfg(test)]` blocks back too. -----------
        # DEFAULT OFF. With the flag off nothing below runs, no extra git call is made,
        # and the tree handed to cargo is byte-identical to the 25 readings already taken.
        #
        # WHY IT EXISTS: ~157k lines of unit tests live inside 91 `src/` files behind
        # `#[cfg(test)]` and ride the counterfactual tree at their POST-task version, so a
        # commit that loosened one of them is invisible. Splicing makes the reading
        # strictly DEEPER. It does NOT close #870 — the census and selector still classify
        # by top-level `tests/*.rs`, so the fix-loop arm stays at 1 signal-bearing commit.
        #
        # EVERY REFUSAL FAILS TOWARD `EARNED`. Skipping a file leaves it at its post-task
        # version, i.e. today's shallower reading; splicing a file we understood wrongly
        # could break a test the commit never touched and manufacture a false `UNEARNED`.
        if splice_src:
            spliced = 0
            refused = 0
            rc_ss, ss_out = run_cmd(
                ["git", "-C", root, "diff", "--name-status", parent, sha, "--", "src/"],
                timeout=60,
            )
            if rc_ss != 0:
                # Could not read the shape -> splice nothing. Shallower, never wronger.
                print("    SPLICE: could not read src/ diff — no file spliced", flush=True)
            else:
                for rel in src_splice_candidates(parse_name_status(ss_out)):
                    rc_pre, pre_text = run_cmd(
                        ["git", "-C", root, "show", f"{parent}:{rel}"], timeout=60
                    )
                    if rc_pre != 0:
                        refused += 1
                        continue
                    abs_path = os.path.join(wt, rel)
                    try:
                        with open(abs_path, "r", encoding="utf-8") as fh:
                            post_text = fh.read()
                    except (OSError, UnicodeDecodeError):
                        refused += 1
                        continue
                    st_sp, text = splice_test_module(post_text, pre_text)
                    if st_sp != SPLICE_OK or text is None:
                        # SPLICE_NO_PRE_MARKER (the module is NEW in this commit) leaves
                        # the file alone in this slice, and the reason is here so nobody
                        # "simplifies" it later: deleting a test module that did not exist
                        # at the parent is the NO_PRE_EXISTING_TEST_EDIT argument one
                        # granularity down and deserves its own decision. Leaving it makes
                        # the counterfactual WEAKER, which is the safe direction — it can
                        # never manufacture an UNEARNED.
                        refused += 1
                        continue
                    try:
                        with open(abs_path, "w", encoding="utf-8") as fh:
                            fh.write(text)
                    except OSError:
                        refused += 1
                        continue
                    spliced += 1
            if sink is not None:
                sink["splice"] = {"spliced": spliced, "refused": refused}
            print(
                f"    SPLICE (src+tests depth): {spliced} file(s) spliced, "
                f"{refused} candidate(s) left alone",
                flush=True,
            )

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
                absent_note
                + "BASELINE: green. COUNTERFACTUAL: red, but VOID — "
                + why + ". " + summarise(out)
            )
        # `absent_note` rides EVERY return, not just the VOID one. This is the path the
        # Day-185 intersection fix creates, so it is the exact path on which a partial
        # overlay must not be recorded as a whole one — a verdict that silently drops the
        # absent files is the shrinking-denominator defect this instrument exists to
        # refuse, one layer inside the instrument itself.
        if sink is not None:
            # The higher-value half of #880. This branch carries UNEARNED — the single
            # most consequential verdict this milestone produces — and the name of the
            # test that failed was discarded from a string already read twice, by
            # `classify_counterfactual` and by `apply_register_drift`. Read once more,
            # here, from that same capture. `ledger_line` gates on the verdict, so an
            # EARNED or INCONCLUSIVE reaching this line is recorded byte-identically.
            sink["failing"] = classify_failing_tests(out)
        return verdict, (
            absent_note + "BASELINE: green. COUNTERFACTUAL: " + summarise(out)
        )
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


def render_population_block(label: str, summary: dict, note: str = "",
                            split: dict | None = None) -> list[str]:
    """The same five figures the census has always printed, for ONE population.

    Extracted so the two populations cannot drift apart in the report: one statement of
    the shape, called twice. A second copy would agree the day it was written.

    `split` (Day 186, #875) adds the shape three-way under the behavioural figure and
    names the REACHABLE denominator, which is `signal_bearing` and not `behavioural`.
    Omitted -> the block is byte-identical to before, which is every caller that does not
    ask for a split and is the whole regression surface.
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
    if split is not None and split["behavioural"]:
        # ANTI-VACUOUS, ASSERTED FIRST: behavioural commits exist but not one shape lookup
        # answered. Saying "0 add-only" there would be this very defect wearing the
        # opposite sign, and quieter than the bug.
        if split["all_unknown"]:
            out.append(
                f"        COULD NOT CHECK ...... all {split['shape_unknown']} shape "
                "lookup(s) failed"
            )
            out.append(
                "        This is a REFUSAL, not '0 add-only': the reachable denominator "
                "is UNKNOWN here."
            )
        else:
            out.append(
                f"        SIGNAL-BEARING ....... {split['signal_bearing']}"
                "   <- the REACHABLE denominator (can yield EARNED/UNEARNED/INCONCLUSIVE)"
            )
            out.append(
                f"        add-only ............. {split['add_only']}"
                "   (answered from the diff; VACUOUS earned, outside the rate)"
            )
            out.append(
                f"        shape UNKNOWN ........ {split['shape_unknown']}"
                "   (summed into neither)"
            )
    if summary["task_commits"]:
        addr = 100.0 * summary["addressable"] / summary["task_commits"]
        beh = 100.0 * summary["behavioural"] / summary["task_commits"]
        out.append(f"    addressable rate ........... {addr:.0f}%")
        out.append(f"    BEHAVIOURAL rate ........... {beh:.0f}%")
    out.append("")
    return out



def render_census(rows, summary, window, limit, note=None, by_pop=None,
                  shape_split=None) -> str:
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
    # `shape_split` is OPTIONAL and defaults to absent rather than being computed here:
    # it costs one `git diff` per behavioural commit, and a pure renderer must not shell
    # out. Omitted -> the block is byte-identical to the pre-#875 report.
    sp = shape_split or {}
    out.extend(
        render_population_block(
            "PLAIN — first-attempt deliveries, no fix-loop suffix",
            by_pop[POP_PLAIN],
            split=sp.get(POP_PLAIN),
        )
    )
    out.extend(
        render_population_block(
            "FIX-LOOP — subject carries eval-fix / build-fix",
            by_pop[POP_FIX_LOOP],
            note="   <- DREAM.md's pre-registered population",
            split=sp.get(POP_FIX_LOOP),
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
                split=sp.get(POP_UNKNOWN_SUFFIX),
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


def ledger_line(
    sha, parent, day, subject, population, verdict, baseline, ts, depth,
    failing=None, splice=None,
):
    """One JSON line for one completed counterfactual. Pure: no I/O and no clock.

    `ts` and `depth` are passed IN rather than read here, so the record is a pure
    function of its inputs and can be pinned by a self-test byte-for-byte.

    #870 slice 2: `splice` is `None` when `--splice-src-tests` was OFF, which is the
    DEFAULT and every one of the 25 rows already recorded — that row is byte-identical to
    the pre-slice-2 shape, asserted with `==` on the whole string rather than a `contains`.
    When it is on, the row carries `src_spliced` (files rewritten), `src_splice_refused`
    (candidates left alone) and the marker `splice_depth: "src+tests"`.

    THE MARKER IS NOT BOOKKEEPING, IT IS THE THING THAT STOPS A DISHONEST RATE. The 10
    classifiable verdicts already recorded were measured against a SHALLOWER
    counterfactual (post-task `src/` tests riding at their post-task version); pooling
    them with deeper ones would answer a question DREAM.md did not ask. A row with no
    marker is `tests`-only depth, and the two must never be averaged together.

    #880: `failing` is the `(status, names)` pair from `classify_failing_tests`, and THE
    VERDICT GATE LIVES HERE rather than at the call sites — one statement of the rule, so
    a caller that hands over names for the wrong verdict cannot corrupt a row. The two
    fields are emitted for BASELINE_RED and UNEARNED only:

      * those are the two verdicts whose CAUSE is a failing test, and the only two where
        "which one?" is the question the row could not answer;
      * REGISTER_DRIFT is deliberately excluded even though it is DERIVED from failing
        names — it is a void attributed by diff shape, its row already carries that
        attribution, and a second place the names live is how the two drift apart;
      * every other verdict's row is BYTE-IDENTICAL to before, which is 20 of the 22 rows
        already in the ledger and the whole regression surface.

    `failing is None` on a gated verdict omits the fields rather than inventing a status:
    no claim was made, and "no claim" must not render as "no failures".
    """
    row = {
        "sha": sha,
        "parent": parent,
        "day": day,
        "subject": subject,
        "population": population,
        "verdict": verdict,
        "baseline": baseline,
        "ts": ts,
        "window_depth": depth,
    }
    if verdict in (BASELINE_RED, UNEARNED) and failing is not None:
        status, names = failing
        row["failing_tests"] = list(names)
        row["failing_tests_status"] = status
    if splice is not None:
        # Emitted for EVERY verdict when the flag is on, including the ones that return
        # before a splice is attempted (both counts are then 0). The marker answers "how
        # deep was this reading?", which is a property of the RUN, not of the outcome.
        row["src_spliced"] = int(splice.get("spliced", 0))
        row["src_splice_refused"] = int(splice.get("refused", 0))
        row["splice_depth"] = "src+tests"
    return json.dumps(row, sort_keys=True)


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
    if verdict in (NO_TEST_CHANGE, NO_PRE_EXISTING_TEST_EDIT):
        # Both are decided by the DIFF and return before the worktree is even created.
        return "not-run"
    return "unknown"


def order_by_shape_tier(rows, shape_of):
    """Reorder candidates so SIGNAL-BEARING commits come first. Pure: `shape_of` injected.

    Returns `(ordered, counts)`. Day 186, #882.

    WHY THIS EXISTS, measured rather than assumed. Day 186's reading session took 8
    readings and produced ZERO classifiable outcomes: 5 of the 8 were add-only, answered
    from the diff for free by `NO_PRE_EXISTING_TEST_EDIT`. `select_runnable` draws
    newest-first, and the newest stretch of this repo is dense in gate-landing commits —
    which ARE the add-only shape. #875 then measured the population rate at 13 of 45
    (29%), not 62%, so newest-first is not a random sample: it draws from a local density.

    THREE TIERS, in this order, and each rule is a self-test row:

      1. `TEST_DIFF_TOUCHES_PRE_EXISTING` — the ONLY commits that can produce
         EARNED / UNEARNED / INCONCLUSIVE, so they go first and a ~3m07s cargo pair
         buys a classification instead of a refusal.
      2. `TEST_DIFF_ADD_ONLY` — answered from the diff at zero cargo cost.
      3. `SHAPE_UNKNOWN` (and `TEST_DIFF_NONE`, which for a row the census already called
         behavioural is a DISAGREEMENT between two lookups, not a fourth shape — same
         bucketing `census_shape_split` uses, one rule and not two).

    PREFERENCE, NEVER EXCLUSION. Add-only and unknown rows stay reachable and are never
    dropped: they cost nothing to answer, the ledger wants them recorded, and filtering
    them out would make it permanently incomplete while making an unfalsifiable claim
    about what was skipped. An unknown must not be promoted into the comfortable bucket
    (Day 144) and must not be silently discarded either — a shrinking denominator inside
    my own meter is the defect this whole instrument is about.

    STABLE WITHIN A TIER: the existing newest-first order is preserved inside each tier,
    so the ONLY thing that changes is tier grouping. Newest-first is still a cost decision
    within a tier (a newer commit's parent is warmer in the shared CARGO_TARGET_DIR).

    PERMUTATION INVARIANT: the output is a permutation of the input — same elements, same
    count, nothing added, nothing lost. That is the cheapest possible regression guard and
    it fails loudly the moment a tier is ever dropped.

    THE STATED LIMIT: this changes what is SAMPLED. It does not make any commit more
    answerable, and it does not grow the reachable denominator by one.
    """
    tiers = {"signal_bearing": [], "add_only": [], "shape_unknown": []}
    for row in rows:
        shape = shape_of(row.sha)
        if shape == TEST_DIFF_TOUCHES_PRE_EXISTING:
            tiers["signal_bearing"].append(row)
        elif shape == TEST_DIFF_ADD_ONLY:
            tiers["add_only"].append(row)
        else:
            tiers["shape_unknown"].append(row)
    ordered = tiers["signal_bearing"] + tiers["add_only"] + tiers["shape_unknown"]
    counts = {k: len(v) for k, v in tiers.items()}
    return ordered, counts


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
    parser.add_argument(
        "--splice-src-tests",
        action="store_true",
        help=(
            "#870: also lay pre-task #[cfg(test)] blocks back over post-task src/, "
            "making the reading strictly DEEPER. DEFAULT OFF -- off is byte-identical "
            "to every reading already recorded. Rows taken with it carry "
            "splice_depth=src+tests and must never be pooled with rows without it."
        ),
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
        # #875: the shape split, computed ONLY over rows already classified behavioural
        # (~47 today, not the ~1014 task commits). The resolver is injected so the tally
        # stays pure and table-testable.
        shape_split = census_shape_split_by_population(
            rows, lambda sha: commit_test_diff_shape(root, sha)
        )
        print(render_census(rows, summary, window, args.limit, note,
                            shape_split=shape_split))
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

        todo = select_runnable(rows, args.population, recorded, None)
        # #882: order by tier BEFORE applying --max-runs, or the bound would slice the
        # newest-first list and the preference could never take effect. The resolver is
        # INJECTED (the `added_ts` / `revisit_add_at` discipline) so the self-tests need
        # no git. One `git diff --name-status` per unrecorded candidate; no cargo.
        todo, tier_counts = order_by_shape_tier(
            todo, lambda sha: commit_test_diff_shape(root, sha)
        )
        if args.max_runs is not None:
            todo = todo[: args.max_runs]
        eligible = [
            r for r in rows if r.population == args.population and r.behavioural
        ]
        print(f"window: {window}")
        print(
            f"batch: population [{args.population}] has {len(eligible)} behavioural "
            f"commit(s); {len(recorded & {r.sha for r in eligible})} already recorded; "
            f"running {len(todo)} now (--max-runs {args.max_runs})."
        )
        # A silent reordering is invisible, and the next reading session needs to know
        # whether there were signal-bearing commits left to pick AT ALL.
        print(
            f"tiers: {tier_counts['signal_bearing']} signal-bearing (can produce a "
            f"classification), {tier_counts['add_only']} add-only (answered from the "
            f"diff, outside the rate), {tier_counts['shape_unknown']} shape-unknown "
            "— run in that order, none dropped."
        )
        if tier_counts["signal_bearing"] == 0:
            print(
                "tiers: NO signal-bearing candidates remain for this population. Every "
                "reading below is answerable from the diff and CANNOT move the "
                "classifiable count — the reachable denominator is exhausted here."
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
                verdict, detail, failing, splice = run_counterfactual(
                    root, row.sha, args.timeout, target=target,
                    splice_src=args.splice_src_tests,
                )
                baseline = baseline_from_verdict(verdict)
                print(f"  verdict: {verdict}   (baseline: {baseline})")
                if failing is not None and verdict in (BASELINE_RED, UNEARNED):
                    # #880: say it on stdout too, not only in the ledger. A row nobody
                    # reads until later is still better than the summary line alone, but
                    # the reader watching the run is the one who can act on it now.
                    f_status, f_names = failing
                    print(f"  failing tests: {f_status} {f_names}")
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
                            verdict, baseline, ts, depth, failing,
                        ),
                    )
                    if err_w:
                        print(f"  WARNING: ledger append failed: {err_w}", file=sys.stderr)
        finally:
            shutil.rmtree(shared, ignore_errors=True)

    if args.commit:
        verdict, detail, failing, splice = run_counterfactual(
            root, args.commit, args.timeout, splice_src=args.splice_src_tests
        )
        print("")
        print(f"counterfactual verdict for {args.commit[:12]}: {verdict}")
        if failing is not None and verdict in (BASELINE_RED, UNEARNED):
            # #880: the SAME line the batch path prints. Both doors, one policy --
            # wiring one and not the other is the shape this repo has shipped nine
            # times, and it is what the evaluator caught here: the arity moved and
            # this call site did not, so the single-commit path raised ValueError at
            # runtime while `--test` stayed green (Python has no compile check).
            f_status, f_names = failing
            print(f"  failing tests: {f_status} {f_names}")
        if detail:
            print("  ---")
            for ln in detail.splitlines():
                print(f"  {ln}")

        if args.record:
            # Day 185. `--record` was PARSED, ADVERTISED IN --help, AND READ BY NOTHING on
            # this path: the append lived in the `--max-runs` arm only, so a single-commit
            # reading printed its verdict and left the ledger untouched. A flag with a
            # description and no consumer is the shape that costs a session -- this one
            # cost exactly that: the mandated verification reading ran for three minutes,
            # the verdict scrolled past, and the ledger stayed at 7 lines, which reads
            # from the outside as "the reading was never taken".
            #
            # Same append, same per-verdict flush, same pure `ledger_line` as the batch
            # arm -- the resolution I/O is here at the call site and the record itself
            # stays a pure function of its inputs.
            rc_s, out_s = run_cmd(
                ["git", "-C", root, "rev-parse", args.commit], timeout=60
            )
            full_sha = out_s.strip().splitlines()[0] if rc_s == 0 else args.commit
            rc_p, out_p = run_cmd(
                ["git", "-C", root, "rev-parse", f"{args.commit}^"], timeout=60
            )
            parent = out_p.strip().splitlines()[0] if rc_p == 0 else ""
            rc_su, out_su = run_cmd(
                ["git", "-C", root, "log", "-1", "--format=%s", args.commit], timeout=60
            )
            subject = out_su.strip().splitlines()[0] if rc_su == 0 else ""
            m = TASK_COMMIT_RE.match(subject)
            day = m.group(1) if m else ""
            rc_d, out_d = run_cmd(
                ["git", "-C", root, "rev-list", "--count", "HEAD"], timeout=60
            )
            depth = out_d.strip() if rc_d == 0 else "?"
            ts = (
                datetime.datetime.now(datetime.timezone.utc)
                .replace(microsecond=0)
                .isoformat()
                .replace("+00:00", "Z")
            )
            err_w = append_ledger(
                args.record,
                ledger_line(
                    full_sha, parent, day, subject, subject_population(subject),
                    verdict, baseline_from_verdict(verdict), ts, depth, failing,
                ),
            )
            if err_w:
                print(f"  WARNING: ledger append failed: {err_w}", file=sys.stderr)
            else:
                print(f"  recorded to {args.record}")

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

    # -- classify_test_diff_shape (Day 186): add-only is answerable WITHOUT a cargo run --
    A, M, D = "A", "M", "D"
    # Anti-vacuous, asserted FIRST: an empty diff can never be ADD_ONLY.
    check("empty diff is NONE, never ADD_ONLY",
          classify_test_diff_shape([]) == TEST_DIFF_NONE)
    check("out-of-scope-only diff is NONE",
          classify_test_diff_shape([(A, "src/lib.rs"), (M, "tests/common/mod.rs")])
          == TEST_DIFF_NONE)
    # The real shape of all 6 historical voids, verified by git diff --name-status.
    check("one added top-level test file is ADD_ONLY",
          classify_test_diff_shape([(A, "tests/git_chokepoint.rs")])
          == TEST_DIFF_ADD_ONLY)
    check("two adds are ADD_ONLY",
          classify_test_diff_shape([(A, "tests/a.rs"), (A, "tests/b.rs")])
          == TEST_DIFF_ADD_ONLY)
    check("adds beside out-of-scope noise are still ADD_ONLY",
          classify_test_diff_shape([(A, "tests/a.rs"), (M, "src/cli.rs"),
                                    (M, "tests/common/mod.rs")])
          == TEST_DIFF_ADD_ONLY)
    # THE NEAR-MISS GUARD, and it is the half that matters: a mixed diff misread as
    # add-only would SKIP a run that could have produced the first UNEARNED.
    check("an add beside a modify TOUCHES_PRE_EXISTING",
          classify_test_diff_shape([(A, "tests/new.rs"), (M, "tests/old.rs")])
          == TEST_DIFF_TOUCHES_PRE_EXISTING)
    check("a lone modify TOUCHES_PRE_EXISTING",
          classify_test_diff_shape([(M, "tests/old.rs")])
          == TEST_DIFF_TOUCHES_PRE_EXISTING)
    check("a delete TOUCHES_PRE_EXISTING",
          classify_test_diff_shape([(D, "tests/old.rs")])
          == TEST_DIFF_TOUCHES_PRE_EXISTING)
    check("a rename TOUCHES_PRE_EXISTING",
          classify_test_diff_shape([("R100", "tests/old.rs")])
          == TEST_DIFF_TOUCHES_PRE_EXISTING)
    # An unknown status must fail toward RUNNING it, never into the comfortable bucket.
    check("an unrecognised status TOUCHES_PRE_EXISTING",
          classify_test_diff_shape([(A, "tests/a.rs"), ("Z", "tests/b.rs")])
          == TEST_DIFF_TOUCHES_PRE_EXISTING)
    # parse_name_status: a rename emits BOTH paths, so an in-scope side cannot hide.
    check("name-status parses adds",
          parse_name_status("A\ttests/a.rs\nM\tsrc/x.rs")
          == [("A", "tests/a.rs"), ("M", "src/x.rs")])
    check("rename emits both paths",
          parse_name_status("R100\ttests/old.rs\ttests/new.rs")
          == [("R100", "tests/old.rs"), ("R100", "tests/new.rs")])
    check("blank lines are skipped", parse_name_status("\n\nA\ttests/a.rs\n")
          == [("A", "tests/a.rs")])
    check("a renamed test file end-to-end TOUCHES_PRE_EXISTING",
          classify_test_diff_shape(
              parse_name_status("R100\ttests/old.rs\ttests/new.rs"))
          == TEST_DIFF_TOUCHES_PRE_EXISTING)
    # The new verdict is its own value and is NOT run-producible.
    check("NO_PRE_EXISTING_TEST_EDIT is not a run verdict",
          NO_PRE_EXISTING_TEST_EDIT not in RUN_VERDICTS)
    check("NO_PRE_EXISTING_TEST_EDIT is distinct from every other state",
          len({NO_TEST_CHANGE, NO_PRE_EXISTING_TEST_EDIT, EARNED, UNEARNED,
               INCONCLUSIVE, COULD_NOT_CHECK, BASELINE_RED, REGISTER_DRIFT}) == 8)

    # -- census_shape_split (Day 186, #875): the behavioural count, split by shape -------
    # WHY: every running tally reads "N of 45", but an add-only commit can NEVER yield a
    # classifiable verdict, so `behavioural` OVERSTATES the reachable denominator. The
    # resolver is injected, so this is a pure table over fabricated shapes with no git.
    def _row(sha, subject="Day 1 (00:00): x (Task 1)", files=("tests/a.rs",), reg=()):
        return CensusRow(sha, subject, list(files), set(reg))

    shapes = {
        "sig1": TEST_DIFF_TOUCHES_PRE_EXISTING,
        "sig2": TEST_DIFF_TOUCHES_PRE_EXISTING,
        "add1": TEST_DIFF_ADD_ONLY,
        "unk1": SHAPE_UNKNOWN,
    }
    mixed_rows = [_row("sig1"), _row("sig2"), _row("add1"), _row("unk1")]
    sp = census_shape_split(mixed_rows, lambda sha: shapes[sha])
    check("split counts signal-bearing", sp["signal_bearing"] == 2)
    check("split counts add-only", sp["add_only"] == 1)
    check("split counts shape-unknown", sp["shape_unknown"] == 1)
    check("split carries the behavioural total", sp["behavioural"] == 4)
    # NEVER SUMMED, and the three must exhaust the behavioural count with nothing lost:
    # a shrinking denominator inside my own meter is the defect this all exists for.
    check("the three states exhaust behavioural, nothing dropped",
          sp["signal_bearing"] + sp["add_only"] + sp["shape_unknown"]
          == sp["behavioural"])
    # THE NEAR-MISS GUARD and it is the half that matters: the positive control neuters
    # the resolver to ADD_ONLY, and this row must stay signal-bearing under a correct one.
    check("a TOUCHES_PRE_EXISTING row tallies as signal-bearing",
          census_shape_split([_row("sig1")],
                             lambda _s: TEST_DIFF_TOUCHES_PRE_EXISTING)
          ["signal_bearing"] == 1)
    check("an ADD_ONLY row is NOT signal-bearing",
          census_shape_split([_row("add1")], lambda _s: TEST_DIFF_ADD_ONLY)
          ["signal_bearing"] == 0)
    # ERROR DIRECTION: an unknown joins NEITHER real bucket. Folding it into add_only
    # understates the denominator (flattering); into signal_bearing overstates it, which
    # is the exact defect #875 removes.
    unk = census_shape_split([_row("unk1")], lambda _s: SHAPE_UNKNOWN)
    check("an unknown shape is neither add-only nor signal-bearing",
          unk["add_only"] == 0 and unk["signal_bearing"] == 0
          and unk["shape_unknown"] == 1)
    # A disagreement between the two lookups (`--name-only` over the commit vs
    # `--name-status` scoped to tests/) is NOT a fourth shape: it is an unusable answer.
    check("TEST_DIFF_NONE on a behavioural row lands in shape_unknown",
          census_shape_split([_row("sig1")], lambda _s: TEST_DIFF_NONE)
          ["shape_unknown"] == 1)
    # COST GUARD: only behavioural rows are asked. A register-only row is addressable and
    # must never spend a `git diff`.
    asked = []
    reg_row = _row("regonly", files=("tests/module_size.rs",),
                   reg=("tests/module_size.rs",))
    sp_reg = census_shape_split(
        [reg_row, _row("sig1")],
        lambda sha: (asked.append(sha), TEST_DIFF_TOUCHES_PRE_EXISTING)[1],
    )
    check("only behavioural rows are asked for a shape", asked == ["sig1"])
    check("a register-only row is outside the split", sp_reg["behavioural"] == 1)
    # ANTI-VACUOUS: behavioural > 0 with EVERY lookup unknown is a REFUSAL, not a clean
    # split. Asserted on the flag AND at the emission point below.
    allunk = census_shape_split([_row("unk1"), _row("unk2")],
                                lambda _s: SHAPE_UNKNOWN)
    check("all-unknown raises the refusal flag", allunk["all_unknown"] is True)
    check("a partial unknown does NOT raise it", sp["all_unknown"] is False)
    check("zero behavioural rows do not raise it",
          census_shape_split([], lambda _s: SHAPE_UNKNOWN)["all_unknown"] is False)
    # Populations stay separate and are never summed (the DREAM.md rule).
    pop_rows = [
        _row("sig1", "Day 1 (00:00): x (Task 1)"),
        _row("add1", "Day 1 (00:00): x (Task 1, eval-fix 2)"),
    ]
    by_shape = census_shape_split_by_population(pop_rows, lambda sha: shapes[sha])
    check("per-population split keeps plain separate",
          by_shape[POP_PLAIN]["signal_bearing"] == 1
          and by_shape[POP_PLAIN]["add_only"] == 0)
    check("per-population split keeps fix-loop separate",
          by_shape[POP_FIX_LOOP]["add_only"] == 1
          and by_shape[POP_FIX_LOOP]["signal_bearing"] == 0)
    check("per-population split has an unknown-suffix bucket",
          by_shape[POP_UNKNOWN_SUFFIX]["behavioural"] == 0)

    # -- render: the split appears, names the reachable denominator, and refuses ---------
    blk = "\n".join(render_population_block("P", census_summary(mixed_rows), split=sp))
    check("rendered split names SIGNAL-BEARING", "SIGNAL-BEARING" in blk)
    check("rendered split names the reachable denominator",
          "REACHABLE denominator" in blk)
    check("rendered split shows add-only", "add-only ............. 1" in blk)
    check("rendered split shows shape UNKNOWN", "shape UNKNOWN" in blk)
    refusal = "\n".join(
        render_population_block("P", census_summary([_row("unk1"), _row("unk2")]),
                                split=allunk)
    )
    check("all-unknown renders a REFUSAL, not a clean split",
          "COULD NOT CHECK" in refusal and "REFUSAL" in refusal)
    check("the refusal never claims 0 add-only",
          "add-only ............. 0" not in refusal)
    # NEAR-MISS: with no split the block is byte-identical to the pre-#875 report, which
    # is every caller that does not ask and the whole regression surface.
    plain_blk = "\n".join(render_population_block("P", census_summary(mixed_rows)))
    check("omitting the split leaves the block unchanged",
          "SIGNAL-BEARING" not in plain_blk and "shape UNKNOWN" not in plain_blk)

    # -- parent_test_pathspec (Day 185): lay back only what EXISTED at the parent --------
    # THE NEAR-MISS GUARD, and the half that matters: a commit that adds NO new test file
    # must produce a BYTE-IDENTICAL pathspec and an empty absent list. That is every one
    # of the 7 readings taken so far and the whole regression surface, so it is a full
    # equality and never a `contains` — a discriminator tested only on the side that
    # fires is vacuous green.
    kept, absent = parent_test_pathspec(
        ["tests/integration.rs", "tests/module_size.rs"],
        ["tests/integration.rs", "tests/module_size.rs", "tests/other.rs"],
    )
    check(
        "no new test file -> pathspec is byte-identical to the input",
        kept == ["tests/integration.rs", "tests/module_size.rs"],
        kept,
    )
    check("no new test file -> nothing reported absent", absent == [], absent)

    # The defect itself: a commit that CREATES a test file keeps its neighbours instead of
    # aborting the whole run. `5c82fef5` adds tests/git_chokepoint.rs.
    kept, absent = parent_test_pathspec(
        ["tests/git_chokepoint.rs", "tests/integration.rs"],
        ["tests/integration.rs", "tests/module_size.rs"],
    )
    check(
        "a new test file is dropped from the pathspec, its neighbours are kept",
        kept == ["tests/integration.rs"],
        kept,
    )
    check(
        "the new test file is REPORTED absent, never silently dropped",
        absent == ["tests/git_chokepoint.rs"],
        absent,
    )

    # THE DANGEROUS BRANCH: every touched test file is new, so there is nothing older to
    # lay back. An empty `kept` must be visible to the caller as empty — proceeding would
    # run POST-task tests against POST-task src/ (just the baseline again) and manufacture
    # a FALSE EARNED. This is the safety property of the whole change.
    kept, absent = parent_test_pathspec(
        ["tests/git_chokepoint.rs", "tests/gasp_session_end_guard.rs"],
        ["tests/integration.rs"],
    )
    check("all-new test set -> empty pathspec (caller must refuse)", kept == [], kept)
    check(
        "all-new test set -> every file reported absent",
        absent == ["tests/git_chokepoint.rs", "tests/gasp_session_end_guard.rs"],
        absent,
    )

    # A parent with no tests/ at all is the same dangerous branch, not a crash.
    kept, absent = parent_test_pathspec(["tests/integration.rs"], [])
    check("empty parent set -> empty pathspec", kept == [], kept)
    check("empty parent set -> the file is reported absent", absent == ["tests/integration.rs"], absent)

    # Order is the CALLER'S order, so the emitted pathspec is stable across runs.
    kept, _ = parent_test_pathspec(
        ["tests/b.rs", "tests/a.rs"], ["tests/a.rs", "tests/b.rs"]
    )
    check("kept preserves the caller's order, not the parent's", kept == ["tests/b.rs", "tests/a.rs"], kept)

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

    # -- #880: WHICH test failed -------------------------------------------------------
    # The first UNEARNED verdict ever taken (1b502eacb937) is recorded as unknown cause,
    # because the row kept `test result: FAILED. 87 passed; 1 failed` and threw the name
    # away. Same defect on the three BASELINE_RED rows. These pin the repair.

    # ANTI-VACUOUS, AND IT IS ASSERTED FIRST. An extractor that finds nothing and reports
    # a clean status is this very defect wearing the opposite sign, and it is quieter than
    # the bug -- so the first thing checked is that a fixture which genuinely contains a
    # failure block yields a NON-EMPTY list. Every assertion below is worthless if this
    # one can pass on an extractor that returns [] for everything.
    real_block = (
        "running 88 tests\n"
        "test module_size::tests::grandfathered_ok ... ok\n"
        "test module_size::src_modules_respect_the_size_gate ... FAILED\n"
        "\n"
        "failures:\n"
        "\n"
        "---- module_size::src_modules_respect_the_size_gate stdout ----\n"
        "thread 'x' panicked at tests/module_size.rs:266:9:\n"
        "\n"
        "failures:\n"
        "    module_size::src_modules_respect_the_size_gate\n"
        "\n"
        "test result: FAILED. 87 passed; 1 failed; 0 ignored\n"
    )
    st_real, names_real = classify_failing_tests(real_block)
    check("ANTI-VACUOUS: a real failure block yields a NON-EMPTY name list",
          len(names_real) > 0, (st_real, names_real))
    check("classify_failing_tests reports FAILING_NAMES on a real block",
          st_real == FAILING_NAMES, (st_real, names_real))
    check("classify_failing_tests names the failing test verbatim",
          names_real == ["module_size::src_modules_respect_the_size_gate"], names_real)

    # THE THREE STATES ARE NEVER FOLDED INTO EACH OTHER. All three carry an empty-or-not
    # list, so the STATUS is the thing that must be read: `failing_test_names` returns []
    # for "the capture had no failure block" AND for "there was no capture", and those are
    # two different facts with two different remedies. Rendering either as an empty list
    # alone would be "could not check" reading as "checked; clean".
    st_nb, names_nb = classify_failing_tests(
        "running 3 tests\ntest result: FAILED. 2 passed; 1 failed; 0 ignored\n"
    )
    check("output present but NO failure block -> FAILING_NO_BLOCK (NOT 'no failures')",
          st_nb == FAILING_NO_BLOCK, (st_nb, names_nb))
    check("FAILING_NO_BLOCK carries no invented names", names_nb == [], names_nb)
    for empty in ("", "   ", "\n\n\t\n"):
        st_no, names_no = classify_failing_tests(empty)
        check(f"empty capture {empty!r} -> FAILING_NO_OUTPUT (nobody looked)",
              st_no == FAILING_NO_OUTPUT, (st_no, names_no))
    check("the three statuses are three DISTINCT values",
          len({FAILING_NAMES, FAILING_NO_BLOCK, FAILING_NO_OUTPUT}) == 3)

    # The streaming `... FAILED` shape alone (no trailing block) is still readable, and a
    # test named in BOTH shapes is one failure rather than two.
    st_s, names_s = classify_failing_tests(
        "test a::b ... FAILED\ntest c::d ... ok\ntest result: FAILED. 1 passed; 1 failed\n"
    )
    check("streaming FAILED line alone is read", (st_s, names_s) == (FAILING_NAMES, ["a::b"]),
          (st_s, names_s))
    check("a test named in both shapes is deduped",
          classify_failing_tests(real_block)[1].count(
              "module_size::src_modules_respect_the_size_gate") == 1, names_real)

    # ON THE ROW: the two gated verdicts carry both fields...
    for gated in (BASELINE_RED, UNEARNED):
        row_g = json.loads(ledger_line(
            "abc", "def", "184", "subj", POP_PLAIN, gated,
            baseline_from_verdict(gated), "T", "42", (st_real, names_real),
        ))
        check(f"{gated} row carries failing_tests",
              row_g.get("failing_tests") == names_real, row_g)
        check(f"{gated} row carries failing_tests_status",
              row_g.get("failing_tests_status") == FAILING_NAMES, row_g)
    # ...and a gated verdict with NO claim omits the fields rather than inventing a
    # status: "no claim was made" must not render as "no failures".
    row_none = json.loads(ledger_line(
        "abc", "def", "184", "subj", POP_PLAIN, UNEARNED, "green", "T", "42", None))
    check("a gated verdict with failing=None omits BOTH fields",
          "failing_tests" not in row_none and "failing_tests_status" not in row_none,
          row_none)

    # NEAR-MISS GUARD, and it is the half that matters: every OTHER verdict's row must be
    # BYTE-IDENTICAL to what the pre-change code emitted -- 20 of the 22 rows already in
    # dreams/counterfactual_verdicts.jsonl, i.e. the entire regression surface. The EARNED
    # literal below is the real on-disk shape (verified against the live ledger, whose 22
    # rows all carry exactly these nine keys); full-string equality, never a `contains`.
    # It earned its keep before it shipped: a hand-typed lowercase "earned" failed it,
    # which is why the literal is transcribed from a real row rather than reasoned out.
    # REGISTER_DRIFT is in this list deliberately: it is a void attributed by diff shape
    # and its row already carries that attribution, so a second place the names live is
    # how the two drift apart.
    pre_change_earned = (
        '{"baseline": "green", "day": "184", "parent": "def", "population": "plain", '
        '"sha": "abc", "subject": "subj", "ts": "T", "verdict": "EARNED", '
        '"window_depth": "42"}'
    )
    check("EARNED row is BYTE-IDENTICAL to the pre-change capture",
          ledger_line("abc", "def", "184", "subj", POP_PLAIN, EARNED, "green", "T", "42",
                      (st_real, names_real)) == pre_change_earned,
          ledger_line("abc", "def", "184", "subj", POP_PLAIN, EARNED, "green", "T", "42",
                      (st_real, names_real)))
    pre_change_keys = {"sha", "parent", "day", "subject", "population", "verdict",
                       "baseline", "ts", "window_depth"}
    for ungated in (EARNED, COULD_NOT_CHECK, NO_PRE_EXISTING_TEST_EDIT,
                    NO_TEST_CHANGE, INCONCLUSIVE, REGISTER_DRIFT):
        row_u = json.loads(ledger_line(
            "abc", "def", "184", "subj", POP_PLAIN, ungated,
            baseline_from_verdict(ungated), "T", "42", (st_real, names_real),
        ))
        check(f"{ungated} row keeps EXACTLY the pre-change key set",
              set(row_u.keys()) == pre_change_keys, sorted(row_u.keys()))

    # -- THE SINK, NOT THE ROW ---------------------------------------------------------
    # Every PRODUCTION `ledger_line(` call must hand over the `failing` value. The checks
    # above prove the ROW SHAPE; nothing proved the WIRING, and that gap shipped: the
    # batch `--record` path -- the one every reading session actually uses -- computed
    # `failing` and never passed it, so the two fields landed on the single-commit path
    # only and the bulk path kept writing `1 failed` without saying which. The tests
    # drive `ledger_line` directly and Python has no compile check, so nothing anywhere
    # could fail. This is that missing consumer, and it is the SECOND time this one diff
    # grew a deaf door (the stdout print half was caught first) -- so the guard is over
    # the whole production region rather than over the two sites known today.
    #
    # Deliberately WEAK, and it says so: it proves the argument is PRESENT at each call
    # site, never that the value handed over is the right one.
    with open(__file__, encoding="utf-8") as _f:
        _src = _f.read()
    # Slice to production so this test's own text cannot vouch for itself, and assemble
    # both needles at runtime so the scan cannot match the lines doing the scanning.
    _prod = _src.split("\ndef " + "run_self_tests")[0]
    _call = "ledger_line" + "("
    _sites = []
    _i = 0
    while True:
        _i = _prod.find(_call, _i)
        if _i < 0:
            break
        if _prod[max(0, _i - 4):_i] == "def ":       # the definition, not a call
            _i += len(_call)
            continue
        _depth, _j = 0, _i + len(_call) - 1
        while _j < len(_prod):                        # match parens to bound THIS call
            if _prod[_j] == "(":
                _depth += 1
            elif _prod[_j] == ")":
                _depth -= 1
                if _depth == 0:
                    break
            _j += 1
        _sites.append(_prod[_i:_j + 1])
        _i = _j + 1
    # ANTI-VACUOUS, ASSERTED FIRST: a scanner that finds nothing and passes is this very
    # defect wearing the opposite sign, and it is quieter than the bug.
    check("ANTI-VACUOUS: the scan finds BOTH production ledger_line call sites",
          len(_sites) >= 2, len(_sites))
    for _n, _site in enumerate(_sites, 1):
        check(f"production ledger_line call site {_n} hands over `failing`",
              "failing" in _site, _site)

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

    # order_by_shape_tier (#882). ANTI-VACUOUS FIRST: a selector that finds nothing and
    # returns nothing is this defect wearing the opposite sign, and it is quieter.
    _tier_rows = [_row("t1"), _row("t2"), _row("t3"), _row("t4")]
    all_unk, unk_counts = order_by_shape_tier(_tier_rows, lambda _s: SHAPE_UNKNOWN)
    check("ANTI-VACUOUS: an all-unknown input returns ALL of them, never empty",
          [r.sha for r in all_unk] == ["t1", "t2", "t3", "t4"], [r.sha for r in all_unk])
    check("all-unknown counts land in shape_unknown, summed into neither other tier",
          unk_counts == {"signal_bearing": 0, "add_only": 0, "shape_unknown": 4},
          unk_counts)

    # NEAR-MISS GUARD, and it is the half that matters: when every row is signal-bearing
    # the change must do NOTHING. Full list equality, not a membership check — a
    # discriminator tested only on the side that fires is vacuous green.
    all_sig, sig_counts = order_by_shape_tier(
        _tier_rows, lambda _s: TEST_DIFF_TOUCHES_PRE_EXISTING
    )
    check("NEAR-MISS: an all-signal-bearing input comes back BYTE-IDENTICAL",
          all_sig == _tier_rows, [r.sha for r in all_sig])
    check("all-signal-bearing counts", sig_counts["signal_bearing"] == 4, sig_counts)

    _shapes = {
        "t1": TEST_DIFF_ADD_ONLY,
        "t2": SHAPE_UNKNOWN,
        "t3": TEST_DIFF_TOUCHES_PRE_EXISTING,
        "t4": TEST_DIFF_ADD_ONLY,
    }
    mixed, mixed_counts = order_by_shape_tier(_tier_rows, lambda s: _shapes[s])
    check("THREE TIERS: signal-bearing, then add-only, then unknown LAST",
          [r.sha for r in mixed] == ["t3", "t1", "t4", "t2"], [r.sha for r in mixed])
    check("stable WITHIN a tier: t1 still precedes t4 (newest-first preserved)",
          mixed.index(_tier_rows[0]) < mixed.index(_tier_rows[3]))
    check("tier counts are reported separately, never summed",
          mixed_counts == {"signal_bearing": 1, "add_only": 2, "shape_unknown": 1},
          mixed_counts)
    check("PERMUTATION INVARIANT: same elements, same count, nothing lost",
          sorted(r.sha for r in mixed) == sorted(r.sha for r in _tier_rows)
          and len(mixed) == len(_tier_rows))
    # TEST_DIFF_NONE for a row the census called behavioural is a DISAGREEMENT between
    # two lookups, not a fourth shape -- same bucketing `census_shape_split` uses.
    _none, none_counts = order_by_shape_tier(_tier_rows, lambda _s: TEST_DIFF_NONE)
    check("TEST_DIFF_NONE lands in shape_unknown, and is still never dropped",
          len(_none) == 4 and none_counts["shape_unknown"] == 4, none_counts)
    check("an empty candidate list is empty, not an error",
          order_by_shape_tier([], lambda _s: SHAPE_UNKNOWN) == ([], {
              "signal_bearing": 0, "add_only": 0, "shape_unknown": 0}))

    # The COMPOSITION main performs: filter unbounded, order by tier, THEN bound. This is
    # the one that matters — bounding first would slice the newest-first list and the
    # preference could never take effect, which is the whole defect #882 fixes.
    _c_rows = [_row("c1"), _row("c2"), _row("c3")]
    _c_shapes = {"c1": TEST_DIFF_ADD_ONLY, "c2": TEST_DIFF_ADD_ONLY,
                 "c3": TEST_DIFF_TOUCHES_PRE_EXISTING}
    _picked = select_runnable(_c_rows, POP_PLAIN, set(), None)
    _picked, _ = order_by_shape_tier(_picked, lambda s: _c_shapes[s])
    check("ordering happens BEFORE --max-runs: the 1 signal-bearing commit is chosen",
          [r.sha for r in _picked[:1]] == ["c3"], [r.sha for r in _picked[:1]])
    check("bounding AFTER ordering still leaves add-only reachable, never excluded",
          [r.sha for r in _picked] == ["c3", "c1", "c2"], [r.sha for r in _picked])

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

    # -- #870 slice 1: the `#[cfg(test)]` splicer. PURE, AND WIRED TO NOTHING -----------
    # The needle is assembled the same way the function assembles it, so the fixtures and
    # the code under test agree by construction rather than by transcription.
    CFG = "#[cfg" + "(test)]"

    # ANTI-VACUOUS, ASSERTED FIRST: a fixture that GENUINELY carries a module-level marker
    # must be found. A finder that finds nothing and reports a clean result is this defect
    # wearing the opposite sign, and it is quieter than the bug.
    real = f"pub fn f() {{}}\n\n{CFG}\nmod tests {{\n    // t\n}}\n"
    st, txt = splice_test_module(real, real)
    check("splicer: a genuine module marker IS found", st == SPLICE_OK, st)
    check("splicer: SPLICE_OK carries non-empty text", bool(txt), txt)
    check(
        "splicer: the marker index is the ATTRIBUTE line, not the mod line",
        test_module_start(real) == 2,
        test_module_start(real),
    )

    # THE HAPPY PATH: post-task production half + pre-task test half. Asserted on the WHOLE
    # string with `==`, never a `contains` — a partial assertion is a green light over the
    # part of the value it does not inspect.
    post = f"fn prod() -> u8 {{ 2 }}\n\n{CFG}\nmod tests {{\n    // NEW assertion\n}}\n"
    pre = f"fn prod() -> u8 {{ 1 }}\n\n{CFG}\nmod tests {{\n    // OLD assertion\n}}\n"
    want = f"fn prod() -> u8 {{ 2 }}\n\n{CFG}\nmod tests {{\n    // OLD assertion\n}}\n"
    st, txt = splice_test_module(post, pre)
    check("splicer: post production + pre tests, byte-exact", st == SPLICE_OK, st)
    check("splicer: spliced text is exactly the counterfactual tree", txt == want, txt)

    # THE NEAR-MISS GUARD THAT DECIDES CORRECTNESS, and it is `src/git.rs`'s real shape:
    # item-level `#[cfg(test)]` attributes AT COLUMN 0 (lines 8, 44, 77 there), followed by
    # `const` and `fn`, with the real test module far below (line 947). Requiring column 0
    # on the attribute discriminates nothing; the column-0 `mod` on the NEXT line is the
    # whole rule. A bare split here would blank the chokepoint file's production half.
    gitlike = (
        f"{CFG}\n"
        'const DESTRUCTIVE: &[&str] = &["push"];\n'
        "\n"
        f"{CFG}\n"
        "fn resolve_git_invocation() {}\n"
        "\n"
        "pub fn run_git() {}\n"
        "\n"
        f"{CFG}\n"
        "mod tests {\n"
        "    // the real test module\n"
        "}\n"
    )
    check(
        "splicer: an item-level cfg(test) is NOT the module marker",
        test_module_start(gitlike) == 8,
        test_module_start(gitlike),
    )
    st, txt = splice_test_module(gitlike, gitlike)
    check("splicer: the git.rs shape still splices", st == SPLICE_OK, st)
    check(
        "splicer: production after an item-level guard is NOT truncated away",
        txt is not None and "pub fn run_git() {}" in txt,
        txt,
    )
    check("splicer: the git.rs shape round-trips byte-for-byte", txt == gitlike, txt)

    # NO_PRE_MARKER — the test module is NEW in this commit. Vacuous, exactly like
    # NO_PRE_EXISTING_TEST_EDIT: you cannot weaken an assertion in a module that did not
    # exist. NO_POST_MARKER is a DIFFERENT fact (deleted, or never had one).
    only_post = f"fn a() {{}}\n{CFG}\nmod tests {{}}\n"
    plain = "fn a() {}\n"
    st, txt = splice_test_module(only_post, plain)
    check("splicer: a new test module -> NO_PRE_MARKER", st == SPLICE_NO_PRE_MARKER, st)
    check("splicer: NO_PRE_MARKER yields no text", txt is None, txt)

    st, txt = splice_test_module(plain, only_post)
    check("splicer: a deleted test module -> NO_POST_MARKER", st == SPLICE_NO_POST_MARKER, st)
    check("splicer: NO_POST_MARKER yields no text", txt is None, txt)

    # Precedence, pinned rather than left accidental: with neither side carrying a marker
    # the post-task side answers first.
    st, _ = splice_test_module(plain, plain)
    check("splicer: neither side marked -> NO_POST_MARKER", st == SPLICE_NO_POST_MARKER, st)

    # IDENTITY IS THE ENTIRE REGRESSION SURFACE FOR ANY FUTURE WIRING. CRLF and a missing
    # trailing newline are included on purpose: a splice that normalises either is a
    # different file, and nothing but this assertion would notice.
    crlf = f"fn a() {{}}\r\n{CFG}\r\nmod tests {{}}"
    st, txt = splice_test_module(crlf, crlf)
    check("splicer: identity preserves CRLF and a missing trailing newline",
          st == SPLICE_OK and txt == crlf, txt)

    # The splice states are NOT verdicts. Nothing here produces one, and a later reader
    # folding them into RUN_VERDICTS would put a file-granularity status into the rate.
    for _s in (SPLICE_OK, SPLICE_NO_POST_MARKER, SPLICE_NO_PRE_MARKER):
        check(f"splicer: {_s} is not a run verdict", _s not in RUN_VERDICTS, _s)

    # ---- #870 slice 2: the SELECTOR half. ---------------------------------------------
    # ANTI-VACUOUS, ASSERTED FIRST: a fixture holding a genuine `M src/*.rs` row must
    # yield a non-empty list. A selector that finds nothing and reports a clean pass is
    # this defect wearing the opposite sign, and it is quieter than the bug.
    real_rows = parse_name_status("M\tsrc/foo.rs\nM\tsrc/format/mod.rs\n")
    check("splice-select: a genuine M src/*.rs row IS selected",
          src_splice_candidates(real_rows) == ["src/foo.rs", "src/format/mod.rs"],
          src_splice_candidates(real_rows))

    # Every exclusion is its own row, and every one fails toward EARNED (leave it alone).
    for _label, _text in [
        ("A (added: no parent version to lay back)", "A\tsrc/new.rs\n"),
        ("D (deleted: nothing to splice into)", "D\tsrc/gone.rs\n"),
        ("R100 (rename: parent path differs)", "R100\tsrc/old.rs\tsrc/new.rs\n"),
        ("tests/ (parent_test_pathspec's job)", "M\ttests/module_size.rs\n"),
        ("non-.rs under src/", "M\tsrc/notes.md\n"),
        ("outside src/", "M\tCargo.toml\n"),
        ("unrecognised status letter", "X\tsrc/weird.rs\n"),
    ]:
        check(f"splice-select: {_label} is OUT of scope",
              src_splice_candidates(parse_name_status(_text)) == [],
              _text)

    check("splice-select: an empty row list yields []",
          src_splice_candidates([]) == [], "not empty")
    check("splice-select: None yields [] rather than raising",
          src_splice_candidates(None) == [], "not empty")
    # A mixed diff keeps only the in-scope half, in first-seen order, deduped.
    mixed = parse_name_status(
        "A\tsrc/added.rs\nM\tsrc/b.rs\nM\ttests/x.rs\nM\tsrc/a.rs\nM\tsrc/b.rs\n"
    )
    check("splice-select: mixed diff keeps M src/*.rs only, ordered and deduped",
          src_splice_candidates(mixed) == ["src/b.rs", "src/a.rs"],
          src_splice_candidates(mixed))

    # DEFAULT-OFF BYTE-IDENTITY: the whole regression surface is the 25 readings already
    # in the ledger plus every future one taken without the flag. A row built with no
    # splice info must be byte-identical to the pre-slice-2 shape.
    _base = ledger_line("abc", "def", "184", "subj", POP_PLAIN, EARNED, "green", "T", "42")
    check("splice-ledger: flag OFF is byte-identical to the pre-slice-2 row",
          _base == ledger_line("abc", "def", "184", "subj", POP_PLAIN, EARNED,
                               "green", "T", "42", None, None),
          _base)
    _off = json.loads(_base)
    check("splice-ledger: flag OFF carries NO splice fields",
          not any(k in _off for k in ("src_spliced", "src_splice_refused", "splice_depth")),
          sorted(_off))
    _on = json.loads(ledger_line("abc", "def", "184", "subj", POP_PLAIN, EARNED,
                                 "green", "T", "42", None,
                                 {"spliced": 3, "refused": 2}))
    check("splice-ledger: flag ON records both counts and the depth marker",
          _on.get("src_spliced") == 3 and _on.get("src_splice_refused") == 2
          and _on.get("splice_depth") == "src+tests",
          _on)

    if failures:
        print(f"SELF-TESTS FAILED ({len(failures)}):", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print(f"ALL PASSED (self-tests over {len(RUN_VERDICTS)} run verdicts + census fold)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
