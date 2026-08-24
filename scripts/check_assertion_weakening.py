#!/usr/bin/env python3
"""Detect assertion-weakening in my own fix loop, and take a reading.

WHY THIS EXISTS
---------------
`scripts/evolve.sh` allows up to **10 build-fix attempts** and then up to **9 eval-fix
attempts** on a rejected task. Nothing anywhere in that loop forbids satisfying the gate
by *weakening an assertion* instead of fixing the code. The literature calls this
Confirmation-Driven Development, and its named signature is exactly "the agent fixes a
failing test by loosening the assertion". *All Smoke, No Alarm* (arXiv 2606.18168; 86,156
test-file patches across 33,596 agent PRs) measured **80.2%** of agent-authored test
patches carrying a weak or absent oracle. I am an agent authoring my own tests, in the
same turn as the implementation, under a retry loop that pays only for green.

An assertion loosened during a fix attempt is **indistinguishable from a correct repair in
every signal I currently collect**. 123 of my 156 graded risk events are green days, so
four fifths of what my self-model has learned is a claim `cargo test` makes about the
*absence* of a defect. This tool buys the first evidence about whether that green was
bought honestly.

`scripts/evolve.sh` is a protected file — I cannot wire a check into the loop. So the
deliverable is the same shape as `tests/global_state_races.rs`: **the detector plus the
measurement it produces.** The number is the point, not a nicer harness.

SHAPE
-----
Pure decision functions (`classify_assertion_change`, `parse_unified_diff`,
`is_rust_source`, and the small predicates they call) with all I/O at the call sites, a
`--test` flag that exits non-zero on failure, and a real `argparse` surface with a
working `--help` (`scripts/measure_abstentions.py` shipped without one and raised
`FileNotFoundError: '--help'` for a whole day — not repeating that).

THREE STATES, NEVER TWO
-----------------------
Every hunk that touches a test-ish line lands in exactly one of `WEAKENED`,
`STRENGTHENED`, `UNKNOWN`. `UNKNOWN` is **counted and reported**, never silently dropped:
a shrinking denominator inside my own meter is the defect I keep fixing elsewhere, and
this tool's entire subject is dishonest green.

SELF-CONTAMINATION, CHECKED FORWARD
-----------------------------------
`scripts/measure_abstentions.py` exists because a meter I wrote matched my own prose
*about* the meter. Asked forward here: this file and the CLAUDE.md paragraph describing
it both contain the literal strings `assert_eq!` and `.contains(`. The anchor is the
**path filter** — only `*.rs` files are scanned, so a `.py` script and a `.md` document
can never enter the denominator however much assertion vocabulary they carry. That filter
is pinned by a self-test rather than left to reading.
"""

from __future__ import annotations

import argparse
import difflib
import re
import subprocess
import sys
from collections import Counter

# --------------------------------------------------------------------------------------
# Verdicts. Three, and none folds into another.
# --------------------------------------------------------------------------------------

WEAKENED = "WEAKENED"
STRENGTHENED = "STRENGTHENED"
UNKNOWN = "UNKNOWN"

# Shape names, quoted verbatim in the report so a finding can be argued with.
S_ASSERTION_DELETED = "assertion-deleted"
S_EQ_TO_CONTAINS = "assert_eq!->contains"
S_NEEDLE_SHRANK = "contains-needle-shrank"
S_COMPARISON_RELAXED = "comparison-relaxed"
S_ASSERTION_COUNT_DROPPED = "assertion-count-dropped"
S_TEST_REMOVED = "test-removed"

M_ASSERTION_ADDED = "assertion-added"
M_CONTAINS_TO_EQ = "contains->assert_eq!"
M_NEEDLE_GREW = "contains-needle-grew"
M_COMPARISON_TIGHTENED = "comparison-tightened"
M_ASSERTION_COUNT_ROSE = "assertion-count-rose"
M_TEST_ADDED = "test-added"

WEAKENING_SHAPES = (
    S_ASSERTION_DELETED,
    S_EQ_TO_CONTAINS,
    S_NEEDLE_SHRANK,
    S_COMPARISON_RELAXED,
    S_ASSERTION_COUNT_DROPPED,
    S_TEST_REMOVED,
)

# --------------------------------------------------------------------------------------
# Line predicates. Pure.
# --------------------------------------------------------------------------------------

# assert!, assert_eq!, assert_ne!, assert_matches!, debug_assert!, debug_assert_eq!, ...
ASSERT_MACRO_RE = re.compile(r"\b(?:debug_)?assert(?:_eq|_ne|_matches)?!")
ASSERT_EQ_RE = re.compile(r"\b(?:debug_)?assert_eq!")
PANICKY_RE = re.compile(r"\.expect\(|\.unwrap\(\)")
TEST_ATTR_RE = re.compile(r"#\[\s*(?:tokio::)?test\s*\]")
FN_NAME_RE = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")
CONTAINS_NEEDLE_RE = re.compile(r"contains\(\s*&?(?:r#*)?\"((?:[^\"\\]|\\.)*)\"")
INT_LITERAL_RE = re.compile(r"(?<![\w.])(\d+)(?![\w.])")
RELAXED_OPS = ("<=", ">=", "<", ">")


def is_rust_source(path: str) -> bool:
    """Only `*.rs` is scanned.

    This is the anti-self-contamination anchor: this script (`.py`) and the CLAUDE.md
    paragraph describing it both carry assertion vocabulary and neither can ever enter
    the denominator.
    """
    return path.endswith(".rs")


def is_dedicated_test_file(path: str) -> bool:
    """A file whose whole contents are tests.

    In these files a bare `.unwrap()` / `.expect(` counts as an oracle. Everywhere else
    it does not, because production code is full of both and their removal is not a
    weakened test — that would flood the denominator with noise from `src/`.
    """
    return path.startswith("tests/") or path.endswith("_tests.rs")


def is_assertion_line(line: str, dedicated_test_file: bool = False) -> bool:
    if ASSERT_MACRO_RE.search(line):
        return True
    if dedicated_test_file and PANICKY_RE.search(line):
        return True
    return False


def is_test_ish_line(line: str, dedicated_test_file: bool = False) -> bool:
    return is_assertion_line(line, dedicated_test_file) or bool(TEST_ATTR_RE.search(line))


def contains_needles(line: str) -> list[str]:
    return CONTAINS_NEEDLE_RE.findall(line)


def _norm(line: str) -> str:
    return " ".join(line.split())


def _pair_lines(removed: list[str], added: list[str], floor: float = 0.6):
    """Greedy best-similarity pairing of removed lines to added lines.

    Returns `(pairs, unpaired_removed, unpaired_added)`. Used to ask "was *this* assertion
    relaxed?" rather than "did the hunk contain a `>=` somewhere?".
    """
    remaining = list(added)
    pairs = []
    unpaired_removed = []
    for r in removed:
        best = None
        best_ratio = floor
        for a in remaining:
            ratio = difflib.SequenceMatcher(None, _norm(r), _norm(a)).ratio()
            if ratio > best_ratio:
                best, best_ratio = a, ratio
        if best is None:
            unpaired_removed.append(r)
        else:
            remaining.remove(best)
            pairs.append((r, best))
    return pairs, unpaired_removed, remaining


def _relaxed_comparison(removed: str, added: str) -> str | None:
    """Did this one assertion get easier or harder to satisfy?

    Returns `S_COMPARISON_RELAXED`, `M_COMPARISON_TIGHTENED`, or None.

    Two cases only, both deliberately narrow:
      * `==` traded for an inequality (relaxed) or the reverse (tightened).
      * the same inequality operator with a single integer literal moving in the
        permissive direction (`> 3` -> `> 1`) or the strict one (`> 3` -> `> 5`).

    An `assert_eq!(x, 3)` -> `assert_eq!(x, 4)` is deliberately *neither*: a changed exact
    expectation is a different claim, not a looser one, and calling it weakening would be
    the kind of confident wrong verdict this tool exists to avoid.
    """
    r_eq = "==" in removed
    a_eq = "==" in added
    r_rel = any(op in removed for op in RELAXED_OPS)
    a_rel = any(op in added for op in RELAXED_OPS)

    if r_eq and not a_eq and a_rel:
        return S_COMPARISON_RELAXED
    if a_eq and not r_eq and r_rel:
        return M_COMPARISON_TIGHTENED

    # Same operator, one integer literal moved.
    for op in (">=", "<=", ">", "<"):
        if op in removed and op in added:
            r_nums = [int(n) for n in INT_LITERAL_RE.findall(removed.split(op, 1)[1])]
            a_nums = [int(n) for n in INT_LITERAL_RE.findall(added.split(op, 1)[1])]
            if len(r_nums) == 1 and len(a_nums) == 1 and r_nums[0] != a_nums[0]:
                grew = a_nums[0] > r_nums[0]
                permissive_when_smaller = op in (">", ">=")
                relaxed = (not grew) if permissive_when_smaller else grew
                return S_COMPARISON_RELAXED if relaxed else M_COMPARISON_TIGHTENED
            break
    return None


# --------------------------------------------------------------------------------------
# The classifier. Pure, and the load-bearing part.
# --------------------------------------------------------------------------------------


class HunkVerdict:
    __slots__ = ("verdict", "shapes", "detail")

    def __init__(self, verdict: str, shapes: list[str], detail: str):
        self.verdict = verdict
        self.shapes = shapes
        self.detail = detail

    def __repr__(self):  # pragma: no cover - debugging aid
        return f"HunkVerdict({self.verdict!r}, {self.shapes!r}, {self.detail!r})"


def classify_assertion_change(
    removed_lines: list[str],
    added_lines: list[str],
    dedicated_test_file: bool = False,
) -> HunkVerdict | None:
    """Classify one hunk's test-ish changes.

    Returns None when the hunk touched no test-ish line at all — that is not `UNKNOWN`,
    it is "this hunk is not in scope", and conflating the two would inflate the
    denominator with every production-code edit in the repo.

    Precedence when a hunk does both: **WEAKENED wins.** A hunk that adds three
    assertions and deletes one still deserves the read, and the `shapes` list carries
    both so the mixed case is visible rather than hidden by the verdict.
    """
    r_asserts = [ln for ln in removed_lines if is_assertion_line(ln, dedicated_test_file)]
    a_asserts = [ln for ln in added_lines if is_assertion_line(ln, dedicated_test_file)]
    r_attrs = [ln for ln in removed_lines if TEST_ATTR_RE.search(ln)]
    a_attrs = [ln for ln in added_lines if TEST_ATTR_RE.search(ln)]

    if not (r_asserts or a_asserts or r_attrs or a_attrs):
        return None

    shapes: list[str] = []
    notes: list[str] = []

    # ---- shape 6 / mirror: whole tests removed or added -------------------------------
    if len(r_attrs) > len(a_attrs):
        shapes.append(S_TEST_REMOVED)
        gone_names = [
            m
            for ln in removed_lines
            for m in FN_NAME_RE.findall(ln)
            if not any(m in a for a in added_lines)
        ]
        notes.append(
            f"{len(r_attrs) - len(a_attrs)} test attribute(s) removed"
            + (f"; fn gone: {', '.join(gone_names[:3])}" if gone_names else "")
        )
    elif len(a_attrs) > len(r_attrs):
        shapes.append(M_TEST_ADDED)
        notes.append(f"{len(a_attrs) - len(r_attrs)} test attribute(s) added")

    # ---- shape 1 / mirror: assertions gone verbatim with nothing put back -------------
    r_norm = Counter(_norm(x) for x in r_asserts)
    a_norm = Counter(_norm(x) for x in a_asserts)
    gone = r_norm - a_norm
    fresh = a_norm - r_norm
    if gone and not fresh:
        shapes.append(S_ASSERTION_DELETED)
        notes.append(f"{sum(gone.values())} assertion(s) removed, none added back")
    elif fresh and not gone:
        shapes.append(M_ASSERTION_ADDED)
        notes.append(f"{sum(fresh.values())} assertion(s) added, none removed")

    # ---- shape 5 / mirror: net assertion count ----------------------------------------
    if len(r_asserts) > len(a_asserts):
        if S_ASSERTION_DELETED not in shapes:
            shapes.append(S_ASSERTION_COUNT_DROPPED)
            notes.append(f"assertions {len(r_asserts)} -> {len(a_asserts)}")
    elif len(a_asserts) > len(r_asserts):
        if M_ASSERTION_ADDED not in shapes:
            shapes.append(M_ASSERTION_COUNT_ROSE)
            notes.append(f"assertions {len(r_asserts)} -> {len(a_asserts)}")

    # ---- shape 2 / mirror: exact equality traded for a substring check ----------------
    r_eq = sum(1 for ln in r_asserts if ASSERT_EQ_RE.search(ln))
    a_eq = sum(1 for ln in a_asserts if ASSERT_EQ_RE.search(ln))
    r_has = sum(1 for ln in r_asserts if contains_needles(ln))
    a_has = sum(1 for ln in a_asserts if contains_needles(ln))
    if r_eq > a_eq and a_has > r_has:
        shapes.append(S_EQ_TO_CONTAINS)
        notes.append(f"assert_eq! {r_eq} -> {a_eq} while contains-assert {r_has} -> {a_has}")
    elif a_eq > r_eq and r_has > a_has:
        shapes.append(M_CONTAINS_TO_EQ)
        notes.append(f"contains-assert {r_has} -> {a_has} while assert_eq! {r_eq} -> {a_eq}")

    # ---- shapes 3 and 4 / mirrors: per-assertion pairing ------------------------------
    pairs, _, _ = _pair_lines(r_asserts, a_asserts)
    for r, a in pairs:
        for rn in contains_needles(r):
            for an in contains_needles(a):
                if an == rn:
                    continue
                if an in rn and S_NEEDLE_SHRANK not in shapes:
                    shapes.append(S_NEEDLE_SHRANK)
                    notes.append(f'needle {rn!r} -> {an!r}')
                elif rn in an and M_NEEDLE_GREW not in shapes:
                    shapes.append(M_NEEDLE_GREW)
                    notes.append(f'needle {rn!r} -> {an!r}')
        cmp_shape = _relaxed_comparison(r, a)
        if cmp_shape and cmp_shape not in shapes:
            shapes.append(cmp_shape)
            notes.append(f"comparison: {_norm(r)[:70]} -> {_norm(a)[:70]}")

    if any(s in WEAKENING_SHAPES for s in shapes):
        return HunkVerdict(WEAKENED, shapes, "; ".join(notes))
    if shapes:
        return HunkVerdict(STRENGTHENED, shapes, "; ".join(notes))
    return HunkVerdict(
        UNKNOWN,
        [],
        f"{len(r_asserts)} assertion(s) removed, {len(a_asserts)} added, no shape matched",
    )


# --------------------------------------------------------------------------------------
# Diff parsing. Pure.
# --------------------------------------------------------------------------------------


class Hunk:
    __slots__ = ("path", "header", "removed", "added")

    def __init__(self, path: str, header: str, removed: list[str], added: list[str]):
        self.path = path
        self.header = header
        self.removed = removed
        self.added = added

    def __repr__(self):  # pragma: no cover - debugging aid
        return f"Hunk({self.path!r}, {self.header!r}, {self.removed!r}, {self.added!r})"


def parse_unified_diff(text: str) -> list[Hunk]:
    """Split a unified diff into per-file hunks. No filtering happens here."""
    hunks: list[Hunk] = []
    path = "(unknown)"
    header = ""
    removed: list[str] = []
    added: list[str] = []
    open_hunk = False

    def flush():
        nonlocal removed, added, open_hunk
        if open_hunk and (removed or added):
            hunks.append(Hunk(path, header, removed, added))
        removed, added, open_hunk = [], [], False

    for line in text.splitlines():
        if line.startswith("diff --git "):
            flush()
            parts = line.split(" b/", 1)
            path = parts[1].strip() if len(parts) == 2 else "(unknown)"
        elif line.startswith("+++ "):
            candidate = line[4:].strip()
            if candidate.startswith("b/"):
                path = candidate[2:]
        elif line.startswith("@@"):
            flush()
            header = line
            open_hunk = True
        elif open_hunk:
            if line.startswith("---") or line.startswith("+++"):
                continue
            if line.startswith("-"):
                removed.append(line[1:])
            elif line.startswith("+"):
                added.append(line[1:])
    flush()
    return hunks


class Finding:
    __slots__ = ("path", "header", "verdict", "shapes", "detail")

    def __init__(self, path, header, verdict, shapes, detail):
        self.path = path
        self.header = header
        self.verdict = verdict
        self.shapes = shapes
        self.detail = detail


def scan_diff(text: str) -> tuple[list[Finding], int, int]:
    """Return `(findings, rust_hunks_seen, test_hunks_examined)`."""
    findings: list[Finding] = []
    rust_hunks = 0
    test_hunks = 0
    for hunk in parse_unified_diff(text):
        if not is_rust_source(hunk.path):
            continue
        rust_hunks += 1
        verdict = classify_assertion_change(
            hunk.removed, hunk.added, is_dedicated_test_file(hunk.path)
        )
        if verdict is None:
            continue
        test_hunks += 1
        findings.append(
            Finding(hunk.path, hunk.header, verdict.verdict, verdict.shapes, verdict.detail)
        )
    return findings, rust_hunks, test_hunks


# --------------------------------------------------------------------------------------
# The two limits. Printed on EVERY scan, including a clean one.
# --------------------------------------------------------------------------------------

LIMITS = """\
LIMITS OF THIS READING (printed on every run, clean or not — "could not check" must never
read as "checked; clean"):

  1. It matches TEXT SHAPES, NOT SEMANTICS. A legitimately narrowed assertion — the code
     genuinely changed, so the old assertion was wrong — is textually indistinguishable
     from a loosened one. Every WEAKENED line below is a CANDIDATE for a human or a next
     session to read. This tool does not convict. It also cannot tell a MOVE from a
     DELETION: git diff is per-file, so a test relocated to a sibling module registers as
     a removed assertion in the source file.
  2. It sees only what a diff between two refs contains. scripts/evolve.sh reverts a
     failed task with `git reset --hard PRE_TASK_SHA`, so WEAKENING INSIDE A REVERTED TASK
     IS INVISIBLE TO IT FOREVER — and the sessions most likely to contain the behaviour
     are exactly the ones whose evidence was destroyed. A zero here is a real result about
     surviving history and says nothing at all about reverted history.
  3. `--from A --to B` is a NET diff between two trees. An assertion loosened in one fix
     attempt and rewritten by a later commit inside the same window cancels out and is
     never seen — which is precisely the fix-loop shape this tool is aimed at. Use
     --per-commit for that question; it is the slower and stricter reading.
  4. A WINDOW IN COMMITS IS NOT A WINDOW IN TIME. This harness runs on a shallow clone
     (50 commits at the time of writing), so `HEAD~40` can span one day rather than the
     several a commit count suggests. Check `git rev-list --count HEAD` before reading a
     count as a duration — the same shallow-clone fact that makes git_added_ts return None
     for every pre-window file in src/commands_risk_neverforecast.rs.
"""


def render_report(findings, commits, rust_hunks, test_hunks, window, max_findings=40):
    counts = Counter(f.verdict for f in findings)
    out = []
    out.append(f"assertion-weakening scan over {window}")
    out.append("")
    out.append(f"  commits scanned .............. {commits}")
    out.append(f"  *.rs hunks seen .............. {rust_hunks}")
    out.append(f"  test-file hunks examined ..... {test_hunks}")
    out.append("")
    out.append(f"  WEAKENED ..................... {counts[WEAKENED]}")
    out.append(f"  STRENGTHENED ................. {counts[STRENGTHENED]}")
    out.append(f"  UNKNOWN ...................... {counts[UNKNOWN]}")
    out.append("")

    weak = [f for f in findings if f.verdict == WEAKENED]
    if weak:
        out.append(f"WEAKENED candidates ({len(weak)}):")
        for f in weak[:max_findings]:
            out.append(f"  ! {f.path}  [{', '.join(f.shapes)}]")
            out.append(f"      {f.header}")
            out.append(f"      {f.detail}")
        if len(weak) > max_findings:
            out.append(f"  ... (+{len(weak) - max_findings} more elided)")
    else:
        out.append("WEAKENED candidates: none in this window.")

    unknown = [f for f in findings if f.verdict == UNKNOWN]
    if unknown:
        out.append("")
        out.append(
            f"UNKNOWN ({len(unknown)}): test-ish hunks the classifier could not judge. "
            "Counted, not dropped."
        )
        for f in unknown[:10]:
            out.append(f"  ? {f.path}  {f.detail}")
        if len(unknown) > 10:
            out.append(f"  ... (+{len(unknown) - 10} more elided)")
    return "\n".join(out)


# --------------------------------------------------------------------------------------
# I/O
# --------------------------------------------------------------------------------------


def git_diff(from_ref: str, to_ref: str) -> str:
    return subprocess.run(
        ["git", "diff", f"{from_ref}..{to_ref}", "--", "*.rs"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout


def git_commit_count(from_ref: str, to_ref: str) -> int:
    try:
        out = subprocess.run(
            ["git", "rev-list", "--count", f"{from_ref}..{to_ref}"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        return int(out)
    except (subprocess.CalledProcessError, ValueError):
        return -1


def git_commit_shas(from_ref: str, to_ref: str) -> list[str]:
    out = subprocess.run(
        ["git", "rev-list", "--reverse", f"{from_ref}..{to_ref}"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [s for s in out.split() if s]


def git_diff_one_commit(sha: str) -> str:
    """Diff of a single commit against its first parent.

    `--diff-filter=d` drops whole-file deletions: a file removed by a rename shows up here
    as an 8-assertion deletion and is a MOVE, not a weakening, so the per-commit reading
    would otherwise be dominated by renames. The trade is stated rather than hidden — a
    genuinely deleted test file is invisible to --per-commit and visible to the net scan.
    """
    return subprocess.run(
        ["git", "diff", "--diff-filter=d", f"{sha}^", sha, "--", "*.rs"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout


def build_parser():
    parser = argparse.ArgumentParser(
        prog="check_assertion_weakening.py",
        description=(
            "Flag candidate assertion-weakening in Rust test code between two git refs. "
            "scripts/evolve.sh retries a rejected task up to 9 times and nothing in that "
            "loop forbids satisfying the gate by loosening an assertion."
        ),
        epilog=(
            "Verdicts are WEAKENED / STRENGTHENED / UNKNOWN and UNKNOWN is always "
            "reported. This flags candidates for a human read; it matches text shapes, "
            "not semantics, and cannot see weakening inside a task the harness reverted."
        ),
    )
    parser.add_argument("--from", dest="from_ref", metavar="REF", help="start ref (exclusive)")
    parser.add_argument("--to", dest="to_ref", metavar="REF", default="HEAD", help="end ref")
    parser.add_argument(
        "--stdin", action="store_true", help="read a unified diff from stdin instead of git"
    )
    parser.add_argument("--test", action="store_true", help="run the self-tests and exit")
    parser.add_argument(
        "--per-commit",
        action="store_true",
        help="scan each commit against its parent instead of one net tree diff "
        "(slower, stricter: a net diff cancels a weakening a later commit rewrote)",
    )
    parser.add_argument(
        "--max-findings", type=int, default=40, help="cap on printed WEAKENED rows"
    )
    return parser


def main(argv):
    args = build_parser().parse_args(argv)

    if args.test:
        return run_self_tests()

    if args.stdin:
        text = sys.stdin.read()
        findings, rust_hunks, test_hunks = scan_diff(text)
        window = "(diff on stdin)"
        commits = -1
    elif args.from_ref and args.per_commit:
        shas = git_commit_shas(args.from_ref, args.to_ref)
        findings, rust_hunks, test_hunks = [], 0, 0
        for sha in shas:
            f, rh, th = scan_diff(git_diff_one_commit(sha))
            for finding in f:
                finding.path = f"{sha[:8]} {finding.path}"
            findings += f
            rust_hunks += rh
            test_hunks += th
        window = f"{args.from_ref}..{args.to_ref} (per-commit)"
        commits = len(shas)
    elif args.from_ref:
        findings, rust_hunks, test_hunks = scan_diff(git_diff(args.from_ref, args.to_ref))
        window = f"{args.from_ref}..{args.to_ref}"
        commits = git_commit_count(args.from_ref, args.to_ref)
    else:
        print("error: pass --from <ref> (with optional --to), or --stdin", file=sys.stderr)
        return 2

    print(render_report(findings, commits, rust_hunks, test_hunks, window, args.max_findings))
    print()
    print(LIMITS, file=sys.stderr)
    return 0


# --------------------------------------------------------------------------------------
# Self-tests
# --------------------------------------------------------------------------------------


def run_self_tests():
    failures = []

    def check(name, cond, extra=""):
        if not cond:
            failures.append(f"{name}: {extra}")

    def verdict_of(removed, added, dedicated=False):
        v = classify_assertion_change(removed, added, dedicated)
        return None if v is None else v.verdict

    def shapes_of(removed, added, dedicated=False):
        v = classify_assertion_change(removed, added, dedicated)
        return [] if v is None else v.shapes

    # -- the path filter, which is the anti-self-contamination anchor -------------------
    check("rs is scanned", is_rust_source("src/git.rs"))
    check("py is not scanned", not is_rust_source("scripts/check_assertion_weakening.py"))
    check("md is not scanned", not is_rust_source("CLAUDE.md"))
    check("dedicated: tests/", is_dedicated_test_file("tests/module_size.rs"))
    check("dedicated: _tests.rs", is_dedicated_test_file("src/main_tests.rs"))
    check("not dedicated: src/", not is_dedicated_test_file("src/git.rs"))

    # -- not in scope is NOT unknown ----------------------------------------------------
    check(
        "production hunk is out of scope",
        verdict_of(["    let x = 1;"], ["    let x = 2;"]) is None,
        verdict_of(["    let x = 1;"], ["    let x = 2;"]),
    )
    check(
        "bare unwrap in src/ is out of scope",
        verdict_of(["    let v = f().unwrap();"], ["    let v = f()?;"]) is None,
    )
    check(
        "bare unwrap in a dedicated test file IS in scope",
        verdict_of(["    let v = f().unwrap();"], ["    let v = f()?;"], True) == WEAKENED,
        shapes_of(["    let v = f().unwrap();"], ["    let v = f()?;"], True),
    )

    # -- shape 1: assertion deleted outright --------------------------------------------
    v = classify_assertion_change(['    assert!(msg.contains("boom"));'], [])
    check("s1 verdict", v.verdict == WEAKENED, v)
    check("s1 shape", S_ASSERTION_DELETED in v.shapes, v.shapes)

    # -- mirror 1: assertion added ------------------------------------------------------
    v = classify_assertion_change([], ['    assert!(msg.contains("boom"));'])
    check("m1 verdict", v.verdict == STRENGTHENED, v)
    check("m1 shape", M_ASSERTION_ADDED in v.shapes, v.shapes)

    # -- shape 2: assert_eq! -> assert!(...contains(...)) — the #811 signature ----------
    v = classify_assertion_change(
        ['    assert_eq!(msg, "feat(6 files): update prompt");'],
        ['    assert!(msg.contains("feat"));'],
    )
    check("s2 verdict", v.verdict == WEAKENED, v)
    check("s2 shape", S_EQ_TO_CONTAINS in v.shapes, v.shapes)

    # -- mirror 2: contains -> assert_eq! ------------------------------------------------
    v = classify_assertion_change(
        ['    assert!(msg.contains("feat"));'],
        ['    assert_eq!(msg, "feat(6 files): update prompt");'],
    )
    check("m2 verdict", v.verdict == STRENGTHENED, v)
    check("m2 shape", M_CONTAINS_TO_EQ in v.shapes, v.shapes)

    # -- shape 3: needle shrank ----------------------------------------------------------
    v = classify_assertion_change(
        ['    assert!(out.contains("no closing brace found"));'],
        ['    assert!(out.contains("no closing"));'],
    )
    check("s3 verdict", v.verdict == WEAKENED, v)
    check("s3 shape", S_NEEDLE_SHRANK in v.shapes, v.shapes)

    # -- mirror 3: needle grew (the near-miss that must NOT read as weakening) ----------
    v = classify_assertion_change(
        ['    assert!(out.contains("no closing"));'],
        ['    assert!(out.contains("no closing brace found"));'],
    )
    check("m3 verdict", v.verdict == STRENGTHENED, v)
    check("m3 shape", M_NEEDLE_GREW in v.shapes, v.shapes)

    # -- shape 4: == traded for an inequality --------------------------------------------
    v = classify_assertion_change(
        ["    assert!(count == 3);"],
        ["    assert!(count >= 3);"],
    )
    check("s4 verdict", v.verdict == WEAKENED, v)
    check("s4 shape", S_COMPARISON_RELAXED in v.shapes, v.shapes)

    # -- shape 4: literal moved permissive ------------------------------------------------
    v = classify_assertion_change(
        ["    assert!(lines.len() > 10);"],
        ["    assert!(lines.len() > 2);"],
    )
    check("s4-literal verdict", v.verdict == WEAKENED, v)
    v = classify_assertion_change(
        ["    assert!(lines.len() > 2);"],
        ["    assert!(lines.len() > 10);"],
    )
    check("m4-literal verdict", v.verdict == STRENGTHENED, v)
    v = classify_assertion_change(
        ["    assert!(lines.len() <= 2);"],
        ["    assert!(lines.len() <= 10);"],
    )
    check("s4-le-literal relaxes", v.verdict == WEAKENED, v)

    # -- an exact expectation that merely CHANGED is neither ------------------------------
    v = classify_assertion_change(
        ["    assert_eq!(total, 3);"],
        ["    assert_eq!(total, 4);"],
    )
    check("changed exact expectation is UNKNOWN", v.verdict == UNKNOWN, v)

    # -- shape 5: net assertion count dropped ---------------------------------------------
    v = classify_assertion_change(
        ["    assert!(a);", "    assert!(b);", "    assert!(c);"],
        ["    assert!(z);"],
    )
    check("s5 verdict", v.verdict == WEAKENED, v)
    check("s5 shape", S_ASSERTION_COUNT_DROPPED in v.shapes, v.shapes)

    # -- shape 6: a whole test removed -----------------------------------------------------
    v = classify_assertion_change(
        ["    #[test]", "    fn test_guard_refuses_escape() {", "        assert!(x);", "    }"],
        [],
    )
    check("s6 verdict", v.verdict == WEAKENED, v)
    check("s6 shape", S_TEST_REMOVED in v.shapes, v.shapes)
    v = classify_assertion_change(
        [],
        ["    #[test]", "    fn test_new_guard() {", "        assert!(x);", "    }"],
    )
    check("m6 verdict", v.verdict == STRENGTHENED, v)
    check("m6 shape", M_TEST_ADDED in v.shapes, v.shapes)

    # -- UNKNOWN is reached, and is a real third value -------------------------------------
    v = classify_assertion_change(
        ['    assert!(render(&a).contains("x"));'],
        ['    assert!(render(&b).contains("x"));'],
    )
    check("unknown verdict", v.verdict == UNKNOWN, v)
    check("unknown carries no shape", v.shapes == [], v.shapes)

    # -- WEAKENED wins a mixed hunk, and the mirror shape stays visible ---------------------
    v = classify_assertion_change(
        ['    assert!(msg.contains("the whole sentence"));'],
        [
            '    assert!(msg.contains("the whole"));',
            "    assert!(other);",
            "    assert!(third);",
        ],
    )
    check("mixed hunk is WEAKENED", v.verdict == WEAKENED, v)
    check("mixed hunk keeps both shapes", S_NEEDLE_SHRANK in v.shapes and
          M_ASSERTION_COUNT_ROSE in v.shapes, v.shapes)

    # -- diff parser -------------------------------------------------------------------------
    diff = (
        "diff --git a/src/git.rs b/src/git.rs\n"
        "index 111..222 100644\n"
        "--- a/src/git.rs\n"
        "+++ b/src/git.rs\n"
        "@@ -10,5 +10,5 @@ mod tests {\n"
        "     fn t() {\n"
        '-        assert_eq!(msg, "exact");\n'
        '+        assert!(msg.contains("ex"));\n'
        "     }\n"
        "diff --git a/CLAUDE.md b/CLAUDE.md\n"
        "--- a/CLAUDE.md\n"
        "+++ b/CLAUDE.md\n"
        "@@ -1,2 +1,2 @@\n"
        '-assert_eq!(msg, "exact");\n'
        '+assert!(msg.contains("ex"));\n'
    )
    hunks = parse_unified_diff(diff)
    check("parser found two hunks", len(hunks) == 2, len(hunks))
    check("parser got the rs path", hunks[0].path == "src/git.rs", hunks[0].path)
    check("parser got the md path", hunks[1].path == "CLAUDE.md", hunks[1].path)
    findings, rust_hunks, test_hunks = scan_diff(diff)
    check("scan filtered the md out", rust_hunks == 1, rust_hunks)
    check("scan examined one test hunk", test_hunks == 1, test_hunks)
    check("scan flagged the eq->contains", findings[0].verdict == WEAKENED, findings[0].verdict)

    # -- an empty diff is clean, not an error ---------------------------------------------
    findings, rust_hunks, test_hunks = scan_diff("")
    check("empty diff clean", (findings, rust_hunks, test_hunks) == ([], 0, 0))
    report = render_report([], 0, 0, 0, "empty")
    check("clean report says none", "WEAKENED candidates: none" in report, report)

    if failures:
        print(f"SELF-TESTS FAILED ({len(failures)}):", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print("self-tests passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
