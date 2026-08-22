#!/usr/bin/env python3
"""Measure the #808 auto-continue gate against A1/planner abstentions (issue #810).

WHY THIS EXISTS
---------------
#810 proposed grading the #808 fix with three bare greps over a session log::

    grep -c "auto-continuing"
    grep -c "No assessment produced"
    grep -c "falling back to single task"

Two of those match *my own prose*. The Day-174 comment I posted on #810 contains,
verbatim inside a markdown table cell::

    | abstentions (`No assessment produced` / `produced 0 tasks` / `changed no files`) | 0 |

That comment is composed and posted during Phase C **of the session being measured**, so
the string lands in that session's own log. Measured, not hypothetical: run 32480314380
(12:06Z, day 174) grepped as 1 abstention while the same session measured over its
transcripts had 0. Same session, two numbers, and the difference was my own writing.

The contamination grows monotonically — every future Phase C comment on #810 adds more
copies of the marker strings, inflating the *denominator*, so the gate looks
progressively more broken than it is. #810's sample floor is 4 gradeable sessions and the
sample is not there yet; anchoring the instrument before the data accumulates is the same
discipline #810 used on itself.

The rule: **anchor on the harness's full emitted line, not a substring** (my own banked
lesson: never widen a match to a bare substring of a tag my own harness authors).

RECORDED NEGATIVE FINDING — there is no structured source to use instead
-----------------------------------------------------------------------
`sessions/day-*/outcome.json` on the `audit-log` branch carries a `fallback_phases`
field. It looks like the answer and is not: `scripts/evolve.sh:583` populates it from
`STAGE_NAME` when a phase is served by the **fallback provider** (a different model
answered), *not* when the planner falls back to a single task. No field in `outcome.json`
records an A1 abstention or a planner fallback. Checked on day 174; do not spend the same
twenty minutes.

WHAT COUNTS
-----------
The harness lines (read out of `scripts/evolve.sh`, a protected file — read only):

  evolve.sh:1273  "  WARNING: No assessment produced — planning agent will read source
                   directly (slower)."
  evolve.sh:1567  "  Planning agent produced 0 tasks — one corrective retry (early-stop
                   suspected)."
  evolve.sh:1605  "  Planning agent produced 0 tasks and budget (Ns) is under the ~4350s
                   a retry+task needs — straight to fallback."
  evolve.sh:1610  "  Planning agent produced 0 tasks — falling back to single task."

and the gate's own firing (`src/main.rs:643`):

  "  ⚡ auto-continuing (n/N — more work pending)..."

Double-counting is avoided deliberately: 1567 and 1605 are the two arms of one `if/elif`
that fires exactly once per planner abstention, so both map to PLANNER_ZERO_TASKS, while
1610 (PLANNER_FALLBACK) is a *consequence* of the same event and is reported separately
rather than added. An abstention event is therefore
``A1_ABSTENTION count + PLANNER_ZERO_TASKS count``.

WHAT IT CANNOT DO
-----------------
It checks that an abstention marker was *emitted*, never why the model abstained, and it
says nothing about whether a firing was the *right* continuation. Presence is mechanically
checkable; correctness is not.

THE ELIGIBILITY BOUNDARY (@yuanhao on #810, 2026-08-21)
-------------------------------------------------------
A session whose head predates the fix under test **could not** have fired the gate. Read
without a boundary, every one of those sessions lands in the denominator as a miss, so the
verdict combines "could not fire" with "did not fire" — a category error, and one that
gets worse-looking as more pre-fix history accumulates in the window. Measured: the first
run of this tool reported `of 16 gradeable sessions, the gate fired in 0` where all 16
predated #808's fix.

`--since-sha <sha>` (resolved with `git show -s --format=%cI`) or `--since <ISO8601>`
marks the boundary. Each session then lands in exactly one of three states, and none is
folded into another:

  INELIGIBLE   strictly before the boundary — excluded from numerator AND denominator
  ELIGIBLE     at or after the boundary (the boundary is inclusive)
  UNKNOWN_AGE  no parseable timestamp — NOT eligible and NOT ineligible; counted in its
               own bucket and reported, never quietly promoted

Session age comes from the `sessions/day-N-YYYYMMDDTHHMMSSZ` directory name
(`scripts/evolve.sh:3471`), so it needs no git and works on a bare `audit-log` checkout.
With no boundary flag every session is eligible and the output is byte-identical to the
pre-boundary tool.

Usage::

    python3 scripts/measure_abstentions.py <logfile|dir> [...]
    python3 scripts/measure_abstentions.py --since-sha <sha> <logfile|dir> [...]
    python3 scripts/measure_abstentions.py --since 2026-08-21T21:25:00Z <dir> [...]
    python3 scripts/measure_abstentions.py --test
"""

import argparse
import os
import re
import subprocess
import sys
from datetime import datetime, timezone

# ── Markers ───────────────────────────────────────────────────────────────────
A1_ABSTENTION = "A1_ABSTENTION"
PLANNER_ZERO_TASKS = "PLANNER_ZERO_TASKS"
PLANNER_FALLBACK = "PLANNER_FALLBACK"
AUTO_CONTINUE_FIRING = "AUTO_CONTINUE_FIRING"

# Sessions below this many *gradeable* sessions get a wait, never a verdict (#810).
MIN_GRADEABLE_SESSIONS = 4

# Eligibility relative to a `--since-sha` / `--since` boundary. THREE states, and none
# of them is folded into another. A session whose head predates the fix under test
# could not have exercised it, so reporting it as "did not fire" is a category error:
# "could not fire" and "did not fire" are different facts and only the second grades
# the gate. UNKNOWN_AGE is the abstention case getting its own name (my standing rule:
# a silently-absorbed unknown inside my own meter is the defect this tool exists for)
# — it is neither eligible nor ineligible, is excluded from both halves, and is
# COUNTED and reported rather than quietly promoted into the comfortable bucket.
ELIGIBLE = "ELIGIBLE"
INELIGIBLE = "INELIGIBLE"
UNKNOWN_AGE = "UNKNOWN_AGE"

# `sessions/day-N-YYYYMMDDTHHMMSSZ` — written by scripts/evolve.sh:3471.
SESSION_DIR_TS_RE = re.compile(r"^day-\d+-(\d{8}T\d{6}Z)$")
COMPACT_TS_RE = re.compile(r"^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$")

ANSI_RE = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")

# Anchored shapes. Each is the harness's WHOLE emitted line from its first word to
# end-of-line, with only the genuinely variable parts loose (\d+ counters, the budget
# seconds). A log line may carry a runner prefix (gh run view emits
# "job\tjob\tTIMESTAMP  <line>"), so the marker text must start at line start or after
# whitespace — but it must run to the end of the line, which is what stops a bare
# substring inside a sentence of mine from matching.
_MARKER_PATTERNS = (
    (
        A1_ABSTENTION,
        re.compile(
            r"(?:^|\s)WARNING: No assessment produced — planning agent will read "
            r"source directly \(slower\)\.$"
        ),
    ),
    (
        PLANNER_ZERO_TASKS,
        re.compile(
            r"(?:^|\s)Planning agent produced 0 tasks — one corrective retry "
            r"\(early-stop suspected\)\.$"
        ),
    ),
    (
        PLANNER_ZERO_TASKS,
        re.compile(
            r"(?:^|\s)Planning agent produced 0 tasks and budget \(\d+s\) is under "
            r"the ~4350s a retry\+task needs — straight to fallback\.$"
        ),
    ),
    (
        PLANNER_FALLBACK,
        re.compile(
            r"(?:^|\s)Planning agent produced 0 tasks — falling back to single task\.$"
        ),
    ),
    (
        AUTO_CONTINUE_FIRING,
        # The glyph is the anchor: my prose says "auto-continuing", the gate says "⚡".
        re.compile(r"(?:^|\s)⚡ auto-continuing \(\d+/\d+ — more work pending\)\.\.\.$"),
    ),
)

# Tells that a line is my own writing (or a tool quoting a file) rather than harness
# stdout: a markdown table pipe, a backtick, a grep-style "NNN:" / "path:NNN:" prefix,
# or a leading ">" blockquote.
_GREP_PREFIX_RE = re.compile(r"^\s*[\w./~-]*:?\d+:")
_BLOCKQUOTE_RE = re.compile(r"^\s*>")


def strip_ansi(text):
    """Remove ANSI escapes so a coloured emission matches its plain shape."""
    return ANSI_RE.sub("", text)


def is_quoted_prose(line):
    """True when the line carries the tells of my writing, not the harness's stdout."""
    line = strip_ansi(line).rstrip("\n")
    if "|" in line:
        return True
    if "`" in line:
        return True
    if _BLOCKQUOTE_RE.match(line):
        return True
    if _GREP_PREFIX_RE.match(line):
        return True
    return False


def classify_line(line):
    """Shape-match one line against the anchored harness emissions.

    Pure. Returns a marker name or None. Says nothing about provenance — see
    `is_quoted_prose` for that half, and `classify` for the two combined.
    """
    line = strip_ansi(line).rstrip("\n").rstrip()
    for marker, pattern in _MARKER_PATTERNS:
        if pattern.search(line):
            return marker
    return None


def classify(line):
    """Return (marker, excluded_as_prose).

    A line whose *shape* matches but whose provenance is my own writing is reported as
    excluded, never silently dropped — a shrinking denominator inside my own meter is
    the defect I keep fixing elsewhere.
    """
    marker = classify_line(line)
    if marker is None:
        return (None, False)
    if is_quoted_prose(line):
        return (None, True)
    return (marker, False)


def parse_timestamp(text):
    """Pure: an aware UTC datetime from a compact or ISO-8601 stamp, else None.

    Accepts the compact `YYYYMMDDTHHMMSSZ` form the session directories use and the
    ISO-8601 form `git show -s --format=%cI` and `--since` produce. A naive stamp is
    read as UTC so a comparison can never raise; anything unparseable returns None,
    which the caller must treat as UNKNOWN, never as a boundary decision.
    """
    if not text:
        return None
    text = text.strip()
    m = COMPACT_TS_RE.match(text)
    if m:
        text = "{}-{}-{}T{}:{}:{}+00:00".format(*m.groups())
    elif text.endswith("Z"):
        text = text[:-1] + "+00:00"
    try:
        dt = datetime.fromisoformat(text)
    except ValueError:
        return None
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc)


def session_timestamp(name):
    """Pure: the UTC datetime encoded in a `day-N-YYYYMMDDTHHMMSSZ` name, else None.

    No git, no network — it reads the name the harness already wrote, so this works on
    a bare `audit-log` checkout. A plain log file with no stamp in its name has UNKNOWN
    age, which is its own bucket.
    """
    m = SESSION_DIR_TS_RE.match((name or "").rstrip("/"))
    if not m:
        return None
    return parse_timestamp(m.group(1))


def classify_eligibility(session_ts, boundary_ts):
    """Pure: ELIGIBLE / INELIGIBLE / UNKNOWN_AGE for one session against a boundary.

    No boundary → every session is eligible (the pre-boundary behaviour, byte-for-byte).
    The boundary is **inclusive**: a session stamped exactly at it is ELIGIBLE, because
    the boundary marks the moment the change landed, and strictly-before is the only
    interval that could not have exercised it.
    """
    if boundary_ts is None:
        return ELIGIBLE
    if session_ts is None:
        return UNKNOWN_AGE
    if session_ts < boundary_ts:
        return INELIGIBLE
    return ELIGIBLE


class SessionCounts:
    """Per-session tallies. Plain data; every field is derived by `count_lines`."""

    def __init__(self, name):
        self.name = name
        self.a1 = 0
        self.zero_tasks = 0
        self.fallback = 0
        self.firings = 0
        self.prose_excluded = 0
        # Age and eligibility. The default is ELIGIBLE so that every path with no
        # `--since*` boundary behaves and renders exactly as it did before #810's
        # boundary landed.
        self.ts = None
        self.eligibility = ELIGIBLE

    @property
    def abstentions(self):
        # 1567 and 1605 are the two arms of one if/elif: one event, one count.
        # 1610 is a consequence of the same event, so it is reported, not added.
        return self.a1 + self.zero_tasks

    @property
    def gradeable(self):
        return self.abstentions > 0

    def row(self):
        # The eligibility suffix is emitted ONLY when the session is not eligible, so a
        # run with no boundary prints byte-identically to the pre-boundary tool.
        suffix = "" if self.eligibility == ELIGIBLE else f" [{self.eligibility}]"
        return (
            f"  {self.name:<44} abstentions={self.abstentions:<3} "
            f"firings={self.firings:<3} fallback={self.fallback:<3} "
            f"gradeable={'yes' if self.gradeable else 'no ':<3} "
            f"prose_excluded={self.prose_excluded}{suffix}"
        )


def count_lines(name, lines):
    """Pure: fold an iterable of lines into a SessionCounts."""
    counts = SessionCounts(name)
    for line in lines:
        marker, excluded = classify(line)
        if excluded:
            counts.prose_excluded += 1
        elif marker == A1_ABSTENTION:
            counts.a1 += 1
        elif marker == PLANNER_ZERO_TASKS:
            counts.zero_tasks += 1
        elif marker == PLANNER_FALLBACK:
            counts.fallback += 1
        elif marker == AUTO_CONTINUE_FIRING:
            counts.firings += 1
    return counts


def apply_boundary(sessions, boundary_ts):
    """Pure-ish: stamp each session's eligibility against the boundary. Mutates in place
    and returns the list, so the decision lives in `classify_eligibility` alone."""
    for s in sessions:
        s.eligibility = classify_eligibility(s.ts, boundary_ts)
    return sessions


def grade(sessions, boundary_label=None, boundary_ts=None):
    """Pure: render the #810 verdict for a list of SessionCounts.

    @yuanhao's corrected rule: a session with zero abstentions is excluded from BOTH
    numerator and denominator — the gate cannot fire where there was nothing to
    continue. Below the sample floor the verdict is refused and the wait is printed
    instead: "not yet gradeable" is its own state, not "the gate failed".

    His second correction (#810, 2026-08-21) adds the eligibility boundary: a session
    whose head predates the fix under test is INELIGIBLE, not a miss, and is dropped
    from both halves before any of the above runs. Without it the verdict keeps
    combining "could not fire" with "did not fire" and gets worse-looking as more
    pre-fix history accumulates. The boundary is reported, the exclusions are reported,
    and the unknown-age bucket is reported — a shrinking denominator inside my own
    meter is the exact defect this tool exists to avoid.

    With no boundary the output is byte-identical to the pre-boundary tool.
    """
    header = ""
    if boundary_label is not None:
        ineligible = [s for s in sessions if s.eligibility == INELIGIBLE]
        unknown = [s for s in sessions if s.eligibility == UNKNOWN_AGE]
        sessions = [s for s in sessions if s.eligibility == ELIGIBLE]
        stamp = boundary_ts.isoformat() if boundary_ts is not None else "unresolved"
        header = (
            f"boundary: {boundary_label} ({stamp}) — sessions strictly before it are "
            f"ineligible\n"
            f"excluded: {len(ineligible)} session(s) predate the boundary — could not "
            f"fire, not graded\n"
            f"unknown age: {len(unknown)} session(s) — neither eligible nor "
            f"ineligible, excluded from both halves and reported rather than "
            f"absorbed\n"
        )
    gradeable = [s for s in sessions if s.gradeable]
    n = len(gradeable)
    if n < MIN_GRADEABLE_SESSIONS:
        return header + (
            f"NOT YET GRADEABLE: {n} of {MIN_GRADEABLE_SESSIONS} gradeable sessions "
            f"({len(sessions)} session(s) read, {len(sessions) - n} with zero "
            f"abstentions and so excluded from both numerator and denominator). "
            f"Recording the wait, not a verdict."
        )
    fired = sum(1 for s in gradeable if s.firings > 0)
    return header + (
        f"VERDICT: of {n} gradeable sessions, the gate fired in {fired} "
        f"({len(sessions)} session(s) read, {len(sessions) - n} excluded for zero "
        f"abstentions)."
    )


# ── I/O (kept at the call sites; everything above is pure) ────────────────────


def read_lines(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        return fh.readlines()


def resolve_sha_timestamp(sha):
    """I/O: the committer date of `sha` as an ISO-8601 string, or None.

    Kept out of every decision function on purpose — `classify_eligibility` takes a
    resolved timestamp and nothing else, so the boundary rule stays pure and testable.
    """
    try:
        proc = subprocess.run(
            ["git", "show", "-s", "--format=%cI", sha],
            capture_output=True,
            text=True,
            timeout=10,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if proc.returncode != 0:
        return None
    return proc.stdout.strip() or None


def session_from_path(path):
    """A file is one session; a directory is one session made of all files under it."""
    if os.path.isdir(path):
        lines = []
        for root, _dirs, files in os.walk(path):
            for fname in sorted(files):
                lines.extend(read_lines(os.path.join(root, fname)))
        name = os.path.basename(path.rstrip("/")) or path
        counts = count_lines(name, lines)
    else:
        name = os.path.basename(path)
        counts = count_lines(name, read_lines(path))
    counts.ts = session_timestamp(name)
    return counts


def build_parser():
    parser = argparse.ArgumentParser(
        prog="measure_abstentions.py",
        description=(
            "Measure the #808 auto-continue gate against A1/planner abstentions "
            "(issue #810)."
        ),
        epilog=(
            "With no --since-sha/--since the output is identical to the pre-boundary "
            "tool. With a boundary, sessions strictly before it are INELIGIBLE "
            "('could not fire'), sessions with no parseable timestamp are UNKNOWN_AGE, "
            "and both are excluded from numerator and denominator and reported."
        ),
    )
    parser.add_argument(
        "paths",
        nargs="*",
        metavar="logfile|dir",
        help="session directories (sessions/day-N-<ts>/) or plain log files",
    )
    parser.add_argument(
        "--test",
        action="store_true",
        help="run the self-tests and exit",
    )
    boundary = parser.add_mutually_exclusive_group()
    boundary.add_argument(
        "--since-sha",
        metavar="SHA",
        help="eligibility boundary: the committer date of this commit "
        "(git show -s --format=%%cI)",
    )
    boundary.add_argument(
        "--since",
        metavar="ISO8601",
        help="eligibility boundary as a timestamp, e.g. 2026-08-21T21:25:00Z "
        "(no git needed)",
    )
    return parser


def main(argv):
    args = build_parser().parse_args(argv)
    if args.test:
        return run_self_tests()
    if not args.paths:
        print(__doc__.strip().splitlines()[0])
        print("usage: python3 scripts/measure_abstentions.py <logfile|dir> [...]")
        print("       python3 scripts/measure_abstentions.py --test")
        return 2

    boundary_label = None
    boundary_ts = None
    if args.since_sha:
        raw = resolve_sha_timestamp(args.since_sha)
        if raw is None:
            print(
                f"error: could not resolve --since-sha {args.since_sha!r} "
                f"(git show -s --format=%cI failed). Refusing to grade: an "
                f"unresolvable boundary must not silently become 'no boundary'.",
                file=sys.stderr,
            )
            return 2
        boundary_ts = parse_timestamp(raw)
        # Just the sha — `grade` already prints the resolved stamp, and printing it
        # twice reads as two different facts.
        boundary_label = args.since_sha
    elif args.since:
        boundary_ts = parse_timestamp(args.since)
        boundary_label = args.since
    if boundary_label is not None and boundary_ts is None:
        print(
            f"error: could not parse boundary timestamp from {boundary_label!r}. "
            f"Refusing to grade rather than grading against an unknown boundary.",
            file=sys.stderr,
        )
        return 2

    sessions = [session_from_path(p) for p in args.paths]
    apply_boundary(sessions, boundary_ts)
    print(f"#810 abstention/firing measurement over {len(sessions)} session(s):")
    for s in sessions:
        print(s.row())
    print()
    print(grade(sessions, boundary_label, boundary_ts))
    return 0


# ── Self-tests ────────────────────────────────────────────────────────────────


def run_self_tests():
    failures = []

    def check(label, actual, expected):
        if actual != expected:
            failures.append(f"{label}: expected {expected!r}, got {actual!r}")

    # 1. The four real harness lines classify correctly (verbatim from evolve.sh).
    check(
        "evolve.sh:1273",
        classify_line(
            "  WARNING: No assessment produced — planning agent will read source "
            "directly (slower)."
        ),
        A1_ABSTENTION,
    )
    check(
        "evolve.sh:1567",
        classify_line(
            "  Planning agent produced 0 tasks — one corrective retry "
            "(early-stop suspected)."
        ),
        PLANNER_ZERO_TASKS,
    )
    check(
        "evolve.sh:1605",
        classify_line(
            "  Planning agent produced 0 tasks and budget (1200s) is under the ~4350s "
            "a retry+task needs — straight to fallback."
        ),
        PLANNER_ZERO_TASKS,
    )
    check(
        "evolve.sh:1610",
        classify_line("  Planning agent produced 0 tasks — falling back to single task."),
        PLANNER_FALLBACK,
    )
    # A gh-run-view prefixed copy of the same line still counts.
    check(
        "runner-prefixed harness line",
        classify_line(
            "evolve\tevolve\t2026-08-21T12:06:01.1234567Z   Planning agent produced "
            "0 tasks — falling back to single task."
        ),
        PLANNER_FALLBACK,
    )

    # 2. THE REGRESSION GUARD — the exact table cell from my day-174 #810 comment.
    my_table_cell = (
        "| abstentions (`No assessment produced` / `produced 0 tasks` / "
        "`changed no files`) | 0 |"
    )
    check("my #810 table cell is prose", is_quoted_prose(my_table_cell), True)
    check("my #810 table cell is not an abstention", classify(my_table_cell), (None, False))

    # 3. A `grep -n` echo of evolve.sh:1273 is prose, not an abstention.
    grep_echo = (
        "1273:  WARNING: No assessment produced — planning agent will read source "
        "directly (slower)."
    )
    check("grep -n echo shape-matches", classify_line(grep_echo), A1_ABSTENTION)
    check("grep -n echo is prose", is_quoted_prose(grep_echo), True)
    check("grep -n echo excluded", classify(grep_echo), (None, True))
    path_grep_echo = (
        "scripts/evolve.sh:1610:  Planning agent produced 0 tasks — falling back to "
        "single task."
    )
    check("path:NNN: echo excluded", classify(path_grep_echo), (None, True))

    # 4. The glyph anchors the firing; bare prose "auto-continuing" does not count.
    check(
        "real firing",
        classify_line("  ⚡ auto-continuing (2/5 — more work pending)..."),
        AUTO_CONTINUE_FIRING,
    )
    check(
        "coloured firing",
        classify_line("\x1b[2m  ⚡ auto-continuing (1/5 — more work pending)...\x1b[0m"),
        AUTO_CONTINUE_FIRING,
    )
    check(
        "prose about auto-continuing",
        classify_line("the gate never fired, so auto-continuing did not happen"),
        None,
    )
    # A quoted firing inside a table is rejected TWICE over: the trailing table syntax
    # already breaks the end-of-line anchor, and the provenance tells fire too. Assert
    # both halves rather than only the one that happens to run first.
    table_firing = "| `⚡ auto-continuing (2/5 — more work pending)...` | 0 |"
    check("backticked firing: shape rejected", classify_line(table_firing), None)
    check("backticked firing: prose tells fire", is_quoted_prose(table_firing), True)
    check("backticked firing: not counted", classify(table_firing), (None, False))
    check(
        "blockquoted harness line",
        classify("> Planning agent produced 0 tasks — falling back to single task."),
        (None, True),
    )

    # 5. Counting folds correctly and does not double-count one planner abstention.
    session = count_lines(
        "fixture",
        [
            "  WARNING: No assessment produced — planning agent will read source "
            "directly (slower).",
            "  Planning agent produced 0 tasks — one corrective retry "
            "(early-stop suspected).",
            "  Planning agent produced 0 tasks — falling back to single task.",
            "  ⚡ auto-continuing (1/5 — more work pending)...",
            # Shape-matches but is my own writing quoted back: counted as excluded.
            grep_echo,
            # Shape does not match at all (trailing table syntax): not counted anywhere.
            my_table_cell,
            "ordinary log noise",
        ],
    )
    check("fixture abstentions", session.abstentions, 2)
    check("fixture fallback reported separately", session.fallback, 1)
    check("fixture firings", session.firings, 1)
    check("fixture prose excluded", session.prose_excluded, 1)
    check("fixture gradeable", session.gradeable, True)

    # 6. Below the floor prints the wait, not a verdict.
    def mk(name, abstentions, firings):
        s = SessionCounts(name)
        s.a1 = abstentions
        s.firings = firings
        return s

    three = [mk("a", 1, 1), mk("b", 1, 0), mk("c", 1, 1)]
    verdict = grade(three)
    check("3 gradeable → wait", verdict.startswith("NOT YET GRADEABLE"), True)
    check("3 gradeable → no verdict", "VERDICT" in verdict, False)
    check("wait names the count", "3 of 4 gradeable sessions" in verdict, True)

    four = three + [mk("d", 2, 0)]
    verdict4 = grade(four)
    check("4 gradeable → verdict", verdict4.startswith("VERDICT"), True)
    check("4 gradeable → fired count", "the gate fired in 2" in verdict4, True)

    # Zero-abstention sessions are excluded from BOTH sides.
    with_zeros = four + [mk("e", 0, 0), mk("f", 0, 0)]
    v = grade(with_zeros)
    check("zeros excluded from denominator", "of 4 gradeable sessions" in v, True)
    check("zeros counted as excluded", "2 excluded for zero abstentions" in v, True)

    # 7. The eligibility boundary (@yuanhao on #810). Timestamp parsing first.
    check(
        "compact session stamp parses",
        parse_timestamp("20260821T120601Z").isoformat(),
        "2026-08-21T12:06:01+00:00",
    )
    check(
        "ISO stamp with Z parses",
        parse_timestamp("2026-08-21T21:25:00Z").isoformat(),
        "2026-08-21T21:25:00+00:00",
    )
    check(
        "git %cI offset stamp parses",
        parse_timestamp("2026-08-21T23:25:00+02:00").isoformat(),
        "2026-08-21T21:25:00+00:00",
    )
    check("junk stamp is None", parse_timestamp("not-a-date"), None)
    check("empty stamp is None", parse_timestamp(""), None)
    check(
        "session dir name yields its stamp",
        session_timestamp("day-174-20260821T120601Z").isoformat(),
        "2026-08-21T12:06:01+00:00",
    )
    check("plain log name yields no stamp", session_timestamp("run-12345.log"), None)
    check(
        "trailing slash still parses",
        session_timestamp("day-9-20260101T000000Z/") is not None,
        True,
    )

    # The three states, each pinned on its own. Boundary chosen so one session sits
    # strictly before it, one exactly ON it, and one after.
    boundary = parse_timestamp("2026-08-21T12:00:00Z")
    before = parse_timestamp("2026-08-21T11:59:59Z")
    exactly = parse_timestamp("2026-08-21T12:00:00Z")
    after = parse_timestamp("2026-08-21T12:00:01Z")
    check("strictly before → ineligible", classify_eligibility(before, boundary), INELIGIBLE)
    # Both sides of the discriminator, so it is not tested only where it blocks.
    check("exactly at → eligible", classify_eligibility(exactly, boundary), ELIGIBLE)
    check("after → eligible", classify_eligibility(after, boundary), ELIGIBLE)
    check("no stamp → unknown", classify_eligibility(None, boundary), UNKNOWN_AGE)
    check("no boundary → eligible", classify_eligibility(before, None), ELIGIBLE)
    check("no boundary, no stamp → eligible", classify_eligibility(None, None), ELIGIBLE)

    # 8. An ineligible session leaves BOTH halves; an unknown one is counted, not dropped.
    def mk_ts(name, abstentions, firings, stamp):
        s = mk(name, abstentions, firings)
        s.ts = parse_timestamp(stamp) if stamp else None
        return s

    pool = [
        mk_ts("old-1", 1, 0, "2026-08-20T00:00:00Z"),
        mk_ts("old-2", 1, 0, "2026-08-21T11:59:59Z"),
        mk_ts("edge", 1, 1, "2026-08-21T12:00:00Z"),
        mk_ts("new", 1, 0, "2026-08-22T00:00:00Z"),
        mk_ts("nameless", 1, 1, None),
    ]
    apply_boundary(pool, boundary)
    check("old-1 ineligible", pool[0].eligibility, INELIGIBLE)
    check("edge eligible (inclusive boundary)", pool[2].eligibility, ELIGIBLE)
    check("nameless unknown", pool[4].eligibility, UNKNOWN_AGE)
    bounded = grade(pool, "abc1234", boundary)
    check("verdict names the boundary", "boundary: abc1234" in bounded, True)
    check("verdict names the boundary stamp", "2026-08-21T12:00:00+00:00" in bounded, True)
    check("two predate the boundary", "excluded: 2 session(s) predate" in bounded, True)
    check("unknown age reported", "unknown age: 1 session(s)" in bounded, True)
    # 5 read, 2 ineligible, 1 unknown → 2 eligible, both with abstentions → below floor.
    check("boundary shrinks the pool", "2 of 4 gradeable sessions" in bounded, True)
    check("below floor still refuses a verdict", "NOT YET GRADEABLE" in bounded, True)
    check("below floor prints no verdict", "VERDICT" in bounded, False)
    # The ineligible session that FIRED nothing must not appear in the numerator, and the
    # unknown one that DID fire must not either — both halves, not just the denominator.
    check("ineligible not in numerator", "fired in" in bounded, False)
    over_floor = [
        mk_ts("old", 1, 0, "2026-08-20T00:00:00Z"),
        mk_ts("a", 1, 1, "2026-08-22T00:00:00Z"),
        mk_ts("b", 1, 0, "2026-08-22T01:00:00Z"),
        mk_ts("c", 1, 1, "2026-08-22T02:00:00Z"),
        mk_ts("d", 1, 0, "2026-08-22T03:00:00Z"),
        mk_ts("unknown-fired", 1, 1, None),
    ]
    apply_boundary(over_floor, boundary)
    v2 = grade(over_floor, "2026-08-21T12:00:00Z", boundary)
    check("4 eligible gradeable", "of 4 gradeable sessions" in v2, True)
    check("only eligible firings counted", "the gate fired in 2" in v2, True)

    # 9. With no boundary, grading is identical to the pre-change result — including for
    # sessions that carry timestamps, so adding the field changed nothing by itself.
    apply_boundary(over_floor, None)
    check(
        "no boundary → every session eligible",
        [s.eligibility for s in over_floor],
        [ELIGIBLE] * 6,
    )
    check(
        "no boundary → pre-change verdict verbatim",
        grade(over_floor),
        "VERDICT: of 6 gradeable sessions, the gate fired in 3 "
        "(6 session(s) read, 0 excluded for zero abstentions).",
    )
    check("no boundary → no boundary header", grade(over_floor).startswith("VERDICT"), True)
    check("no boundary → row carries no suffix", "[" in over_floor[0].row(), False)
    apply_boundary(over_floor, boundary)
    check("ineligible row is labelled", "[INELIGIBLE]" in over_floor[0].row(), True)

    if failures:
        for f in failures:
            print(f"FAIL {f}")
        print(f"\n{len(failures)} self-test failure(s)")
        return 1
    print("all self-tests passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
