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

Usage::

    python3 scripts/measure_abstentions.py <logfile|dir> [...]
    python3 scripts/measure_abstentions.py --test
"""

import os
import re
import sys

# ── Markers ───────────────────────────────────────────────────────────────────
A1_ABSTENTION = "A1_ABSTENTION"
PLANNER_ZERO_TASKS = "PLANNER_ZERO_TASKS"
PLANNER_FALLBACK = "PLANNER_FALLBACK"
AUTO_CONTINUE_FIRING = "AUTO_CONTINUE_FIRING"

# Sessions below this many *gradeable* sessions get a wait, never a verdict (#810).
MIN_GRADEABLE_SESSIONS = 4

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


class SessionCounts:
    """Per-session tallies. Plain data; every field is derived by `count_lines`."""

    def __init__(self, name):
        self.name = name
        self.a1 = 0
        self.zero_tasks = 0
        self.fallback = 0
        self.firings = 0
        self.prose_excluded = 0

    @property
    def abstentions(self):
        # 1567 and 1605 are the two arms of one if/elif: one event, one count.
        # 1610 is a consequence of the same event, so it is reported, not added.
        return self.a1 + self.zero_tasks

    @property
    def gradeable(self):
        return self.abstentions > 0

    def row(self):
        return (
            f"  {self.name:<44} abstentions={self.abstentions:<3} "
            f"firings={self.firings:<3} fallback={self.fallback:<3} "
            f"gradeable={'yes' if self.gradeable else 'no ':<3} "
            f"prose_excluded={self.prose_excluded}"
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


def grade(sessions):
    """Pure: render the #810 verdict for a list of SessionCounts.

    @yuanhao's corrected rule: a session with zero abstentions is excluded from BOTH
    numerator and denominator — the gate cannot fire where there was nothing to
    continue. Below the sample floor the verdict is refused and the wait is printed
    instead: "not yet gradeable" is its own state, not "the gate failed".
    """
    gradeable = [s for s in sessions if s.gradeable]
    n = len(gradeable)
    if n < MIN_GRADEABLE_SESSIONS:
        return (
            f"NOT YET GRADEABLE: {n} of {MIN_GRADEABLE_SESSIONS} gradeable sessions "
            f"({len(sessions)} session(s) read, {len(sessions) - n} with zero "
            f"abstentions and so excluded from both numerator and denominator). "
            f"Recording the wait, not a verdict."
        )
    fired = sum(1 for s in gradeable if s.firings > 0)
    return (
        f"VERDICT: of {n} gradeable sessions, the gate fired in {fired} "
        f"({len(sessions)} session(s) read, {len(sessions) - n} excluded for zero "
        f"abstentions)."
    )


# ── I/O (kept at the call sites; everything above is pure) ────────────────────


def read_lines(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        return fh.readlines()


def session_from_path(path):
    """A file is one session; a directory is one session made of all files under it."""
    if os.path.isdir(path):
        lines = []
        for root, _dirs, files in os.walk(path):
            for fname in sorted(files):
                lines.extend(read_lines(os.path.join(root, fname)))
        return count_lines(os.path.basename(path.rstrip("/")) or path, lines)
    return count_lines(os.path.basename(path), read_lines(path))


def main(argv):
    if not argv:
        print(__doc__.strip().splitlines()[0])
        print("usage: python3 scripts/measure_abstentions.py <logfile|dir> [...]")
        print("       python3 scripts/measure_abstentions.py --test")
        return 2
    sessions = [session_from_path(p) for p in argv]
    print(f"#810 abstention/firing measurement over {len(sessions)} session(s):")
    for s in sessions:
        print(s.row())
    print()
    print(grade(sessions))
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

    if failures:
        for f in failures:
            print(f"FAIL {f}")
        print(f"\n{len(failures)} self-test failure(s)")
        return 1
    print("all self-tests passed")
    return 0


if __name__ == "__main__":
    args = sys.argv[1:]
    if args and args[0] == "--test":
        sys.exit(run_self_tests())
    sys.exit(main(args))
