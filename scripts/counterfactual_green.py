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

SIX STATES, NONE FOLDED INTO ANOTHER
------------------------------------
`NO_TEST_CHANGE` (nothing to counterfactual — the counterfactual tree *is* the shipped
tree, and this must never be counted as EARNED), `EARNED`, `UNEARNED`, `INCONCLUSIVE`
(pre-tests do not compile: an honest API rename and a hidden break are indistinguishable
here), `BASELINE_RED` (see below), and `COULD_NOT_CHECK` (worktree/checkout/cargo/timeout
failure), which is never folded into any verdict — the same refusal the pre-push hook and
`CiScan`'s could-not-run branch already make.

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
import os
import re
import shutil
import subprocess
import sys
import tempfile

# --------------------------------------------------------------------------------------
# The five states. None folds into another.
# --------------------------------------------------------------------------------------

NO_TEST_CHANGE = "NO_TEST_CHANGE"
EARNED = "EARNED"
UNEARNED = "UNEARNED"
INCONCLUSIVE = "INCONCLUSIVE"
COULD_NOT_CHECK = "COULD_NOT_CHECK"
BASELINE_RED = "BASELINE_RED"

# The five a live run can produce. NO_TEST_CHANGE is decided by the diff, before any run.
RUN_VERDICTS = (EARNED, UNEARNED, INCONCLUSIVE, COULD_NOT_CHECK, BASELINE_RED)

# Internal sentinel, deliberately NOT a run verdict: the baseline passed, so the
# comparison is licensed and the counterfactual may proceed. It never reaches a report.
BASELINE_OK = "BASELINE_OK"

# Second copy of scripts/extract_trajectory.py:219 — see the module doc for why.
TASK_COMMIT_RE = re.compile(r"^Day\s+(\d+)\s+\([^)]+\):\s+(.+?)\s+\(Task\s+\d+\)\s*$")

# Test files that are DEBT REGISTERS rather than behavioural tests. Their contents are
# recorded line counts and named exceptions, so a PRE-task copy laid over POST-task src/
# fails by construction whenever a module grew — and that failure is the gate working as
# designed (updating the register is the compliant remedy the gate itself prints), not
# evidence of a loosened assertion. Counting those as UNEARNED would be a confident wrong
# verdict, which is the exact over-reach this whole milestone is about.
#
# Only tests/module_size.rs qualifies today: it is the one gate whose register encodes a
# MEASUREMENT of src/ (line counts) that legitimately moves with every commit. The other
# gates' registers name files, tests or reasons, which do not drift when src/ grows.
REGISTER_TEST_FILES = frozenset({"tests/module_size.rs"})

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
# Census — the deliverable. Pure half here, git I/O at the call site.
# --------------------------------------------------------------------------------------


class CensusRow:
    """One task commit and whether it is addressable by the counterfactual."""

    __slots__ = ("sha", "subject", "test_files")

    def __init__(self, sha: str, subject: str, test_files: list[str]):
        self.sha = sha
        self.subject = subject
        self.test_files = test_files

    @property
    def addressable(self) -> bool:
        return bool(self.test_files)

    @property
    def behavioural(self) -> bool:
        """True when at least one touched test file is NOT a debt register.

        This is the denominator that carries signal. A commit touching only
        tests/module_size.rs is addressable in the mechanical sense and worthless in the
        evidential one — see REGISTER_TEST_FILES.
        """
        return any(p not in REGISTER_TEST_FILES for p in self.test_files)

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


def census_summary(rows: list[CensusRow]) -> dict:
    """Fold census rows. Anti-vacuous: zero task commits is a refusal, not a zero.

    Three tiers, and `behavioural` is the one that carries signal. `register_only` is
    reported separately rather than summed into `addressable`, because folding it in
    produces a flattering headline over commits whose counterfactual verdict is decided
    by construction (see REGISTER_TEST_FILES).
    """
    return {
        "task_commits": len(rows),
        "addressable": sum(1 for r in rows if r.addressable),
        "behavioural": sum(1 for r in rows if r.behavioural),
        "register_only": sum(1 for r in rows if r.addressable and not r.behavioural),
        "not_addressable": sum(1 for r in rows if not r.addressable),
    }


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


def collect_census(root: str, limit: int | None) -> tuple[list[CensusRow], str, str]:
    """Walk the log, classify each task commit. Returns (rows, window, error)."""
    rc, out = run_cmd(["git", "-C", root, "log", "--format=%H%x09%s"], timeout=60)
    if rc != 0:
        return [], "", f"git log failed (rc={rc})"

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
        rows.append(CensusRow(sha, subject, top_level_test_files(out2.splitlines())))

    rc3, out3 = run_cmd(["git", "-C", root, "rev-list", "--count", "HEAD"], timeout=60)
    depth = out3.strip() if rc3 == 0 else "?"
    rc4, out4 = run_cmd(["git", "-C", root, "rev-parse", "--is-shallow-repository"])
    shallow = out4.strip() == "true" if rc4 == 0 else False
    window = (
        f"{len(all_lines)} commits reachable from HEAD "
        f"({depth} total, shallow={'yes' if shallow else 'no'})"
    )
    return rows, window, ""


def run_counterfactual(root: str, sha: str, timeout: int) -> tuple[str, str]:
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
    if not changed:
        # Nothing to counterfactual: the counterfactual tree IS the shipped tree. No
        # baseline is needed and none is run — there is no comparison to license.
        return NO_TEST_CHANGE, "no top-level tests/*.rs touched"

    # LANDMINE 1: a scratch worktree under mkdtemp, never the live tree, never the repo.
    tmp = tempfile.mkdtemp(prefix="yoyo-counterfactual-")
    wt = os.path.join(tmp, "wt")
    # LANDMINE 2: our own CARGO_TARGET_DIR, shared across BOTH runs of this commit so the
    # second build is warm. Kept out of the repo's own target/ — #832: a nested cargo
    # build over the shared target/debug/yoyo clobbers the binary every integration test
    # resolves through env!("CARGO_BIN_EXE_yoyo"), and it reddened main for three sessions
    # while reading as flakiness.
    target = os.path.join(tmp, "target")
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

        # Lay the PRE-task tests back over the POST-task src/.
        rc, out = run_cmd(
            ["git", "-C", wt, "checkout", parent, "--"] + changed, timeout=120
        )
        if rc != 0:
            # A test file that did not exist at the parent cannot be checked out. That is
            # a brand-new test file, which is not a counterfactual question at all.
            return COULD_NOT_CHECK, f"checkout of pre-tests failed: {out.strip()[:200]}"

        rc, out = run_cmd(["cargo", "test"], cwd=wt, timeout=timeout, env=env)
        verdict = classify_counterfactual(rc, out)
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
  3. A DEBT REGISTER IS NOT A BEHAVIOURAL TEST. tests/module_size.rs records line counts
     of src/, so a PRE-task copy laid over POST-task src/ fails by construction whenever
     a module grew — and that failure is the gate working as designed, since updating the
     register is the compliant remedy the gate itself prints. Those commits are counted
     as REGISTER-ONLY and kept OUT of the behavioural denominator. Read the BEHAVIOURAL
     rate; the addressable rate is the flattering one.
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


def render_census(rows, summary, window, limit) -> str:
    out = []
    scope = f"last {limit} task commits" if limit else "all reachable task commits"
    out.append(f"counterfactual-green census over {scope}")
    out.append(f"  window ....................... {window}")
    out.append("")
    out.append(f"  task commits found ........... {summary['task_commits']}")
    out.append(f"  NO_TEST_CHANGE ............... {summary['not_addressable']}")
    out.append(f"  touch any tests/*.rs ......... {summary['addressable']}")
    out.append(
        f"    of which REGISTER-ONLY ..... {summary['register_only']}"
        "   (debt-register bookkeeping; verdict decided by construction, no signal)"
    )
    out.append(
        f"    of which BEHAVIOURAL ....... {summary['behavioural']}"
        "   <- the denominator that carries signal"
    )
    out.append("")
    if summary["task_commits"]:
        addr = 100.0 * summary["addressable"] / summary["task_commits"]
        beh = 100.0 * summary["behavioural"] / summary["task_commits"]
        out.append(f"  addressable rate ............. {addr:.0f}%")
        out.append(f"  BEHAVIOURAL rate ............. {beh:.0f}%   <- read this one")
        out.append("")
    for r in rows:
        if r.behavioural:
            mark = "*"
        elif r.addressable:
            mark = "r"
        else:
            mark = " "
        files = ", ".join(r.test_files) if r.test_files else "(none)"
        out.append(f"  {mark} {r.sha[:8]}  {r.subject[:58]}")
        out.append(f"      tests/*.rs touched: {files}")
    out.append("")
    out.append("  legend: * behavioural   r register-only   (blank) NO_TEST_CHANGE")
    return "\n".join(out)


def main(argv):
    parser = argparse.ArgumentParser(
        prog="counterfactual_green.py",
        description=(
            "Was this green EARNED? Rebuild post-task src/ with pre-task tests/ and run "
            "cargo test. Five states, none folded into another."
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
        "--timeout",
        type=int,
        default=DEFAULT_TIMEOUT_SECS,
        metavar="S",
        help=f"ceiling for one cargo test run (default {DEFAULT_TIMEOUT_SECS}s)",
    )
    parser.add_argument("--test", action="store_true", help="run self-tests and exit")
    args = parser.parse_args(argv)

    if args.test:
        return run_self_tests()

    if not args.census and not args.commit:
        parser.print_help()
        return 2

    root = repo_root()
    status = 0

    if args.census:
        rows, window, err = collect_census(root, args.limit)
        if err:
            print(f"COULD NOT CHECK: {err}", file=sys.stderr)
            print(LIMITS, file=sys.stderr)
            return 1
        summary = census_summary(rows)
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
            print(LIMITS, file=sys.stderr)
            return 1
        print(render_census(rows, summary, window, args.limit))
        status = 0

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
        "six distinct states",
        len({NO_TEST_CHANGE, EARNED, UNEARNED, INCONCLUSIVE, COULD_NOT_CHECK,
             BASELINE_RED}) == 6,
    )

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
        "Day 183 (11:42): One bounded retry (Task 1, eval-fix 1)",
    ]
    for s in not_task:
        check(f"non-task subject rejected: {s[:30]}", TASK_COMMIT_RE.match(s) is None)

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
    # A commit touching ONLY a debt register is addressable and carries no signal. This
    # row is the near-miss guard for the tier below it: without it, a 64% headline reads
    # as measurable when the real denominator is a fraction of that.
    reg = CensusRow("d" * 40, "Day 1 (0:0): register bump (Task 1)", ["tests/module_size.rs"])
    check("register-only is addressable", reg.addressable, reg)
    check("register-only is NOT behavioural", not reg.behavioural, reg)
    mixed = CensusRow(
        "e" * 40, "Day 1 (0:0): gate + bump (Task 1)",
        ["tests/git_chokepoint.rs", "tests/module_size.rs"],
    )
    check("a mixed commit IS behavioural", mixed.behavioural, mixed)
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
    check("limits state the intent boundary", "intent is not" in LIMITS.lower())
    check("limits state the src/ scope", "165k" in LIMITS)
    check("limits state the register caveat", "DEBT REGISTER" in LIMITS)

    if failures:
        print(f"SELF-TESTS FAILED ({len(failures)}):", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print(f"ALL PASSED (self-tests over {len(RUN_VERDICTS)} run verdicts + census fold)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
