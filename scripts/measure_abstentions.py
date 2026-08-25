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
Superseded and kept rather than erased: see "RECORDED NEGATIVE FINDING — ... for the
*counts*" below, which restates this after re-checking on day 175. Still true, and still
the reason `fallback_phases` must not be read as a planner-fallback record.

THE STREAM MISMATCH, AND THE SECOND STREAM (day 175)
----------------------------------------------------
Every marker below is anchored to a line `scripts/evolve.sh` prints to the **workflow
log**. A `sessions/day-*/` directory contains agent transcripts and files the harness
writes into it — *not* the harness's own stdout. So pointing this tool at a session
directory reads `fallback=0` by construction, whatever happened.

Measured, not inferred: `day-174-20260821T232342Z` and `day-175-20260822T021038Z` both
read `fallback=0` while both ran the planner-fallback path and committed nothing but a
wrap-up. Both carry `transcripts/plan_retry.log` and an `unverified_task_1.md`.

So a session directory is read for **structural artifacts** as well — files the harness
writes, which my own prose cannot forge (the #810 contamination defect that forced the
anchored-line design). `classify_structure` is pure and reports three-valued fields:
`False` is observed-and-absent, `UNKNOWN` is this-stream-cannot-say. The two streams are
printed **side by side and never summed** — they have different coverage, and adding
them would double-count exactly when both are available (the same rule already applied
to evolve.sh:1567/1605 vs 1610).

RECORDED NEGATIVE FINDING — there is no structured source for the *counts*
--------------------------------------------------------------------------
`outcome.json` carries a `fallback_phases` field. It looks like the answer and is not:
`scripts/evolve.sh:583` populates it from `STAGE_NAME` when a phase is served by the
**fallback provider** (a different model answered), *not* when the planner falls back to
a single task. Its twelve keys, enumerated over all 453 directories on `audit-log`, carry
no A1 abstention, no planner fallback, and no record of the session's commits. Checked
day 174, re-checked day 175; do not spend the same twenty minutes.

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

And it cannot tell you whether a session produced any output. Nothing in a session
directory records the commits a session made — not one of `outcome.json`'s twelve keys,
not any file. So `zero_output` is reported as **unknown**, always, and the tool says why
rather than deriving it from `tasks_succeeded`. (`tasks_succeeded: 1` for a session that
committed nothing is `scripts/evolve.sh`'s own defect; it is a protected file and cannot
be fixed here. This makes the defect visible from the outside, which is all that can be
done from here.) Likewise `plan_retry` means a planner retry artifact is present — that
is evidence *toward* a fallback session, never a synonym for one.

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

THE FOURTH BUCKET — PROVIDER ERRORS (day 175, @yuanhao on #810)
---------------------------------------------------------------
A zero-task outcome has at least three distinct causes, and only ONE of them grades
#808: closing silence (the gate SHOULD fire), an API error / rate limit (the gate MUST
NOT fire — `piped_should_continue` requires `!had_error`, and continuing after a
provider refusal would burn the remaining budget re-hitting the same limit), and a
genuine early stop with an error (same). Counting the second as a gradeable miss
manufactures evidence against #808 out of Anthropic's rate limiter.

Both post-fix sessions that scored as gradeable were rate-limit failures: 08-21T21:25
and 08-22T01:17, each ending its planner turn on ``error: Rate limited, retry after
Some(14454000)ms`` — a ~4h retry-after. Every healthy post-fix session (03:42, 06:41,
12:40, 15:22) has zero such lines. So a session carrying provider-error evidence
alongside its zero-task outcome becomes ``EXCLUDED_PROVIDER_ERROR``: out of numerator
and denominator both, reported as its own bucket, **never summed** into ``INELIGIBLE``,
``UNKNOWN_AGE`` or the zero-abstention exclusions. Different fact, different remedy.

**Coarseness, named rather than dressed up:** this is SESSION-level presence. Nothing
attributes the error to the phase that produced the zero-task outcome, because the
concatenated lines of a session directory carry no cheap phase anchor. It is a judgment
threshold, not a measurement, and the printed output says so. ``MIN_GRADEABLE_SESSIONS``
is unchanged at 4, so ``NOT YET GRADEABLE`` becomes more common — which is correct.

With no provider-error lines anywhere the output is byte-identical to the pre-bucket
tool (verified by running both).

Usage::

    python3 scripts/measure_abstentions.py <logfile|dir> [...]
    python3 scripts/measure_abstentions.py --since-sha <sha> <logfile|dir> [...]
    python3 scripts/measure_abstentions.py --since 2026-08-21T21:25:00Z <dir> [...]
    python3 scripts/measure_abstentions.py --test
"""

import argparse
import json
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

# The FOURTH exclusion state (#810, @yuanhao 2026-08-22). Reported as its own bucket and
# never summed into the three above: "the provider refused this turn" is a different
# fact from "this session predates the fix" and from "I cannot date this session", and
# each has a different remedy.
EXCLUDED_PROVIDER_ERROR = "EXCLUDED_PROVIDER_ERROR"

# The FIFTH exclusion state (#810, @yuanhao 2026-08-24) — the fourth *bucket* in the
# verdict, and the one that guards the input itself rather than the session. An empty or
# truncated log scored IDENTICALLY to a genuinely healthy session:
#
#     $ printf '' > empty.log
#     $ python3 scripts/measure_abstentions.py empty.log
#     empty.log   abstentions=0  firings=0  fallback=0  gradeable=no  prose_excluded=0
#
# How it bit: a shell-quoting slip made three `gh run view --log` redirects write 0 bytes,
# and the tool reported three clean sessions. "Could not check" must never read as
# "checked; clean" — the refusal the pre-push hook makes, the refusal CiScan's
# could-not-run branch makes, the refusal UngradedScan's unread-lines clause makes.
EXCLUDED_NO_HARNESS_OUTPUT = "EXCLUDED_NO_HARNESS_OUTPUT"

# The next exclusion state (#810, @yuanhao 2026-08-25), layered ON TOP of the
# no-harness-output one rather than replacing it. Buckets are referred to BY NAME and
# never by ordinal here: the two sentences above already disagree about which ordinal
# EXCLUDED_NO_HARNESS_OUTPUT holds, and inventing an agreement they never had would be
# a worse record than the collision.
#
# The measured defect. Three real workflow logs (32667319793, 32679577526, 32688121916,
# ~107 KB each) scored `abstentions=0 firings=0 fallback=0 gradeable=no` — the row a
# healthy zero-abstention session prints. Every one of them died BEFORE Phase A1:
# `cargo test` failed at the build-verification step, three startup attempts per run,
# zero phases, zero tasks (the sonnet-5 preset-pricing episode, fixed at f9073e8d /
# 0577bfe7). EXCLUDED_NO_HARNESS_OUTPUT missed them because it asks `markers == 0` and
# a run that started and immediately died carries 3 SESSION_HEADER markers.
#
# Why that inverts the metric: "the harness printed something" and "the harness got far
# enough to be measurable" are different questions, and only the second licenses
# scoring a session. A STARVED loop and a HEALTHY loop produced the identical row —
# those sessions were not clean and not merely unmeasured, they were destroyed, and the
# instrument reported the outcome as indistinguishable from health. That is my own
# "could not check must never read as checked; clean" rule failing in the sharpest
# direction available, inside the meter that decides whether #808 works.
#
# SESSION_HEADER is deliberately NOT weakened. @yuanhao is explicit and correct that it
# is real evidence a log *is* a session log; this is a separate eligibility test on top
# of that fact, not a retreat from it.
EXCLUDED_NO_PHASE_REACHED = "EXCLUDED_NO_PHASE_REACHED"

# The three states of `classify_phase_reach`. None is folded into another, and the
# first is what keeps the two buckets DISJOINT: a log with no markers at all belongs to
# EXCLUDED_NO_HARNESS_OUTPUT and must not also be counted as having reached no phase.
PHASE_REACH_NO_OUTPUT = "PHASE_REACH_NO_OUTPUT"
PHASE_REACH_NONE = "PHASE_REACH_NONE"
PHASE_REACH_MEASURABLE = "PHASE_REACH_MEASURABLE"

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

# Provider-error evidence (#810, @yuanhao 2026-08-22). A zero-task outcome has at least
# three distinct causes and only ONE of them is gradeable for #808: closing silence (the
# gate SHOULD fire), an API error / rate limit (the gate MUST NOT fire — `piped_should_
# continue` requires `!had_error`, and continuing after a provider refusal would burn the
# remaining budget re-hitting the same limit), and a genuine early stop with an error
# (same). Counting the second as a gradeable miss manufactures evidence against #808 out
# of Anthropic's rate limiter.
#
# Anchored exactly as `_MARKER_PATTERNS` are: from the emission's first word to
# end-of-line, with only the numbers loose, and a runner `job\tjob\tTIMESTAMP` prefix
# allowed by the leading `(?:^|\s)`. Every candidate still runs through
# `is_quoted_prose` (see `classify_provider_error`) — the whole reason that machinery
# exists is that my own prose contaminated this meter, and #810's own thread carries the
# string "Rate limited" inside a markdown table.
_PROVIDER_ERROR_PATTERNS = (
    # src/prompt.rs:725/736/752 print `\n{RED}  error: {msg}{RESET}`, where {msg} is the
    # provider's own text. This is the exact shape both #810 sessions carried.
    re.compile(r"(?:^|\s)error: Rate limited, retry after Some\(\d+\)ms$"),
    # src/prompt.rs:756/897/1071/1399 print `{YELLOW}  💡 {diagnostic}{RESET}`; this is
    # the first line of `diagnose_api_error`'s rate-limit branch (prompt_retry.rs:497).
    re.compile(r"(?:^|\s)💡 Rate limited by provider '[^']*'\.$"),
    # The same `error:` emission carrying any other rate-limit wording the provider may
    # use. Anchored at `error:` and run to end-of-line; the body is loose because it is
    # the PROVIDER's string, not the harness's, and the phrase list mirrors
    # `diagnose_api_error`'s own rate-limit branch (prompt_retry.rs:491-495).
    re.compile(
        r"(?:^|\s)error: .*(?:rate limit|rate_limit|too many requests|429).*$",
        re.IGNORECASE,
    ),
)

# Tells that a line is my own writing (or a tool quoting a file) rather than harness
# stdout: a markdown table pipe, a backtick, a grep-style "NNN:" / "path:NNN:" prefix,
# or a leading ">" blockquote.
_GREP_PREFIX_RE = re.compile(r"^\s*[\w./~-]*:?\d+:")
_BLOCKQUOTE_RE = re.compile(r"^\s*>")

# Harness-output evidence (#810, @yuanhao 2026-08-24). These six lines are `echo`
# statements to stdout in `scripts/evolve.sh` — read at HEAD, not trusted from a list:
#
#   :136   echo "=== Day $DAY ($DATE $SESSION_TIME) ==="
#   :1167  echo "  Phase A1: Assessment (${ASSESS_TIMEOUT}s)..."
#   :1291  echo "  Phase A2: Planning (${PLAN_TIMEOUT}s)..."
#   :1709  echo "  Phase B: Implementation..."
#   :1784  echo "  → Task $TASK_NUM: $task_title [$task_kind]"
#   :3586  echo "=== Day $DAY complete ==="
#
# The anchor is a MARKER, not a byte count, and that is the whole design: byte size
# misses the subtler half of this bug — a truncated or partially-downloaded log which
# HAS bytes and no phase line, and which scores as clean today.
#
# Deliberately NOT anchored on: `=== YOUR TRAJECTORY ===` and the sibling `=== SECTION
# ===` banners near evolve.sh:1175/:1313. Those live inside heredocs composing *prompt
# text*, not harness stdout, so a log containing nothing but an echoed prompt would
# match them and read as a real session.
#
# Each is the whole emitted line from its first word to end-of-line, with only counters,
# timeouts and task titles loose, and `(?:^|\s)` to allow `gh run view`'s
# `job\tjob\tTIMESTAMP` prefix. Presence of ANY ONE of them means the log carries
# harness output.
SESSION_HEADER = "SESSION_HEADER"
PHASE_A1 = "PHASE_A1"
PHASE_A2 = "PHASE_A2"
PHASE_B = "PHASE_B"
TASK_START = "TASK_START"
SESSION_FOOTER = "SESSION_FOOTER"

# The markers that prove the loop got PAST build verification. SESSION_HEADER and
# SESSION_FOOTER are deliberately absent: they are bookends the harness prints whether
# or not any work happened, and SESSION_HEADER's presence in a destroyed run is the
# whole reason EXCLUDED_NO_HARNESS_OUTPUT could not see #810's three logs.
_PHASE_MARKERS = frozenset({PHASE_A1, PHASE_A2, PHASE_B, TASK_START})

_HARNESS_OUTPUT_PATTERNS = (
    (
        SESSION_HEADER,
        re.compile(r"(?:^|\s)=== Day \d+ \(\d{4}-\d{2}-\d{2} \d{2}:\d{2}\) ===$"),
    ),
    (PHASE_A1, re.compile(r"(?:^|\s)Phase A1: Assessment \(\d+s\)\.\.\.$")),
    (PHASE_A2, re.compile(r"(?:^|\s)Phase A2: Planning \(\d+s\)\.\.\.$")),
    (PHASE_B, re.compile(r"(?:^|\s)Phase B: Implementation\.\.\.$")),
    (TASK_START, re.compile(r"(?:^|\s)→ Task \d+: .*\[[^\[\]]*\]$")),
    (SESSION_FOOTER, re.compile(r"(?:^|\s)=== Day \d+ complete ===$")),
)


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


def classify_provider_error_line(line):
    """Pure: True when the line shape-matches a provider-error emission.

    Shape only — says nothing about provenance, exactly as `classify_line` doesn't.
    """
    line = strip_ansi(line).rstrip("\n").rstrip()
    for pattern in _PROVIDER_ERROR_PATTERNS:
        if pattern.search(line):
            return True
    return False


def classify_provider_error(line):
    """Return (is_provider_error, excluded_as_prose) — the two halves combined.

    A line whose shape matches but whose provenance is my own writing is reported as
    excluded, never silently dropped. Here the direction of the damage is the sharp
    one: a false positive **shrinks** my own denominator by excluding a session that
    really did test the gate, so the prose filter is load-bearing rather than cosmetic.
    """
    if not classify_provider_error_line(line):
        return (False, False)
    if is_quoted_prose(line):
        return (False, True)
    return (True, False)


def classify_harness_output_line(line):
    """Pure: the harness-marker name this line shape-matches, else None.

    Shape only — says nothing about provenance, exactly as `classify_line` doesn't.
    """
    line = strip_ansi(line).rstrip("\n").rstrip()
    for marker, pattern in _HARNESS_OUTPUT_PATTERNS:
        if pattern.search(line):
            return marker
    return None


def classify_harness_output(line):
    """Return (marker, excluded_as_prose) — the two halves combined.

    The prose filter is not ceremony here, it is the defect this tool was born from:
    #810's own thread, its task file and the CLAUDE.md paragraph documenting this fix
    all contain the literal strings `Phase A1` and `Phase A2`. A markdown-quoted or
    grep-prefixed mention must NOT count as harness output — otherwise a log holding
    only my own prose *about* the detector is judged a real session, which is the
    permissive direction and the one that reinstates the bug.
    """
    marker = classify_harness_output_line(line)
    if marker is None:
        return (None, False)
    if is_quoted_prose(line):
        return (None, True)
    return (marker, False)


def has_harness_output(lines):
    """Pure: True when any line carries an UNQUOTED harness marker.

    The decision half of the fourth bucket. Presence, not a count: one real phase line
    is enough to prove the log carries harness stdout.
    """
    for line in lines:
        marker, _prose = classify_harness_output(line)
        if marker is not None:
            return True
    return False


def phase_reach_markers(lines):
    """Pure: the SET of UNQUOTED harness-marker names appearing in `lines`.

    The fold `classify_phase_reach` consumes. A set, not a count: the question it
    answers is *which* markers were reached, and the count half already lives in
    `SessionCounts.harness_markers`.
    """
    seen = set()
    for line in lines:
        marker, _prose = classify_harness_output(line)
        if marker is not None:
            seen.add(marker)
    return seen


def classify_phase_reach(markers):
    """Pure: three states over the set of unquoted harness markers seen.

    - no markers at all -> PHASE_REACH_NO_OUTPUT. Not this bucket's business; it is
      EXCLUDED_NO_HARNESS_OUTPUT's, and returning a distinct value here is what keeps
      the two disjoint rather than double-counting an empty log.
    - markers present, but every one of them a BOOKEND -> PHASE_REACH_NONE. The harness
      spoke and then died before any measurable phase (#810: three ~107 KB logs
      carrying three `=== Day N (...) ===` headers each and nothing else).
    - any PHASE marker present -> PHASE_REACH_MEASURABLE. Byte-identical to the
      pre-fix behaviour, which is every healthy session and was the regression risk.

    The split is bookends vs phases, and that is a deliberate widening of #810's task
    wording ("markers present but no PHASE_A1"). The bookends print whether or not any
    work happened; a phase marker is printed only once `scripts/evolve.sh` is past
    build verification, which is the property being tested. Anchoring on PHASE_A1
    ALONE would exclude a head-truncated log carrying `Phase B: Implementation...` and
    `→ Task 1: ...` — over-firing, which costs real data, where under-firing only
    leaves today's behaviour. Both real inputs are unaffected: the starved logs carry
    no phase marker at all, the healthy ones carry every kind.

    **Stated limit:** this distinguishes "reached a measurable phase" from "did not".
    It says nothing about whether the session was any GOOD — a full session that
    reached Phase A1 and produced garbage scores exactly as an honest one does.
    """
    if not markers:
        return PHASE_REACH_NO_OUTPUT
    if markers & _PHASE_MARKERS:
        return PHASE_REACH_MEASURABLE
    return PHASE_REACH_NONE


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
        # Provider-error evidence (#810). Session-level presence — see
        # `provider_error_excluded` for the coarseness disclosure.
        self.provider_errors = 0
        # Harness-output evidence (#810, 2026-08-24). THREE states, and the third is
        # the point: an int is a real observation folded by `count_lines`; None means
        # no line stream was folded at all, which is reachable only from a hand-built
        # fixture (every production path goes through `session_from_path` ->
        # `count_lines`). An unobserved stream must not be reported as an absent one.
        self.harness_markers = None
        # Harness-marker lines that were my own prose. Reported in the row ONLY when
        # the session is excluded — that is the one place the number is a decision
        # input ("your log has phase lines and every one of them is quoted"). Keeping
        # it out of `prose_excluded` is deliberate: that counter is rendered on every
        # healthy row, and folding these in would change the healthy path's output.
        self.harness_prose_excluded = 0
        # The SET of unquoted marker names seen, folded beside the count above (#810,
        # 2026-08-25). None for the same reason `harness_markers` is: no stream was
        # folded at all, which is "nobody looked", not "nothing was there".
        self.harness_marker_names = None
        # Age and eligibility. The default is ELIGIBLE so that every path with no
        # `--since*` boundary behaves and renders exactly as it did before #810's
        # boundary landed.
        self.ts = None
        # True when `self.ts` came from the operator's `--session-ts` rather than from
        # the filename (#810, 2026-08-25). An asserted stamp is a CLAIM BY ME, not an
        # observation, and this tool exists because I once let my own writing score as
        # data — so the two are never merged into one indistinguishable field. Observed
        # always beats asserted; see `session_from_path`.
        self.ts_asserted = False
        self.eligibility = ELIGIBLE
        # The second (structural) stream, or None when this input was not a session
        # directory. Never folded into the counts above.
        self.structure = None

    @property
    def abstentions(self):
        # 1567 and 1605 are the two arms of one if/elif: one event, one count.
        # 1610 is a consequence of the same event, so it is reported, not added.
        return self.a1 + self.zero_tasks

    @property
    def provider_error_excluded(self):
        """The fourth exclusion state: a zero-task outcome WITH provider-error evidence.

        The gate is *required* not to fire after a provider refusal, so such a session
        cannot grade #808 in either direction — it leaves the numerator and the
        denominator both, and is reported as its own bucket, never summed into
        INELIGIBLE, UNKNOWN_AGE, or the zero-abstention exclusions.

        **Coarseness, named rather than dressed up:** @yuanhao's wording is "a provider
        error in the same phase as the zero-task outcome". This is SESSION-level
        presence — nothing here attributes the error to the phase that produced the
        zero-task outcome, because a session directory's concatenated lines carry no
        cheap phase anchor. It is a judgment threshold, not a measurement, and the only
        evidence for it is that every healthy post-#808 session (03:42, 06:41, 12:40,
        15:22) had zero such lines while both zero-task sessions had them. A session
        that hit a rate limit in an unrelated phase and *also* abstained for closing
        silence would be excluded here — a lost data point, which is the safe direction
        for a meter that must not manufacture evidence.
        """
        return self.abstentions > 0 and self.provider_errors > 0

    @property
    def no_harness_output(self):
        """The fourth bucket: a line stream WAS folded and carried no harness marker.

        `harness_markers is None` (no stream folded — hand-built fixture only) is
        deliberately NOT this state: "nobody looked" is not "nothing was there".
        """
        return self.harness_markers == 0

    @property
    def no_phase_reached(self):
        """Markers were seen, but none of them was PHASE_A1 (#810, 2026-08-25).

        Disjoint from `no_harness_output` by construction: an empty marker set
        classifies as PHASE_REACH_NO_OUTPUT, which is that bucket's state, not this
        one's. An unfolded stream (`harness_marker_names is None`) is neither.
        """
        if self.harness_marker_names is None:
            return False
        return classify_phase_reach(self.harness_marker_names) == PHASE_REACH_NONE

    @property
    def gradeable(self):
        return (
            self.abstentions > 0
            and not self.provider_error_excluded
            and not self.no_harness_output
            and not self.no_phase_reached
        )

    def row(self):
        # The eligibility suffix is emitted ONLY when the session is not eligible, and
        # the provider-error suffix ONLY when such a line was seen, so a run with no
        # boundary and no provider errors prints byte-identically to before.
        suffix = "" if self.eligibility == ELIGIBLE else f" [{self.eligibility}]"
        if self.ts_asserted:
            # Emitted only when an asserted stamp was actually USED, so a run with no
            # `--session-ts` (and any run whose inputs all carry their own stamp)
            # prints byte-identically to before. The marker is what keeps an operator
            # claim from reading as an observation on the row.
            suffix += " [ts=asserted]"
        if self.no_harness_output:
            suffix += f" [{EXCLUDED_NO_HARNESS_OUTPUT}]"
            if self.harness_prose_excluded:
                suffix += (
                    f" [harness_markers_quoted={self.harness_prose_excluded}]"
                )
        elif self.no_phase_reached:
            # `elif`, not `if`: the two buckets are disjoint, and rendering both on one
            # row would suggest a session can sit in each.
            suffix += f" [{EXCLUDED_NO_PHASE_REACHED}]"
            if self.harness_prose_excluded:
                suffix += (
                    f" [harness_markers_quoted={self.harness_prose_excluded}]"
                )
        if self.provider_errors:
            suffix += (
                f" [{EXCLUDED_PROVIDER_ERROR}: {self.provider_errors} line(s)]"
                if self.provider_error_excluded
                else f" [provider_error_lines={self.provider_errors}]"
            )
        return (
            f"  {self.name:<44} abstentions={self.abstentions:<3} "
            f"firings={self.firings:<3} fallback={self.fallback:<3} "
            f"gradeable={'yes' if self.gradeable else 'no ':<3} "
            f"prose_excluded={self.prose_excluded}{suffix}"
        )


def count_lines(name, lines):
    """Pure: fold an iterable of lines into a SessionCounts."""
    counts = SessionCounts(name)
    counts.harness_markers = 0
    counts.harness_marker_names = set()
    for line in lines:
        # Harness-output evidence first, and deliberately WITHOUT `continue`: its six
        # shapes are disjoint from the marker and provider-error shapes, and falling
        # through guarantees every pre-existing counter is byte-identical to before.
        harness_marker, harness_prose = classify_harness_output(line)
        if harness_marker is not None:
            counts.harness_markers += 1
            counts.harness_marker_names.add(harness_marker)
        elif harness_prose:
            counts.harness_prose_excluded += 1
        # Provider-error evidence next; its shapes are disjoint from the markers', so
        # the order is for readability, not precedence. Prose exclusions from BOTH
        # detectors land in the one `prose_excluded` counter — same fact, same remedy.
        is_provider_error, provider_prose = classify_provider_error(line)
        if provider_prose:
            counts.prose_excluded += 1
            continue
        if is_provider_error:
            counts.provider_errors += 1
            continue
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


# ---------------------------------------------------------------------------
# The second stream: structural artifacts inside the session directory.
# ---------------------------------------------------------------------------
#
# THE STREAM MISMATCH (day 175). Every marker above is anchored to a line
# `scripts/evolve.sh` prints to the **workflow log**. A `sessions/day-*/` directory on
# the `audit-log` branch does not contain the harness's stdout — it contains agent
# transcripts and files the harness writes. So measuring a session *directory* for
# planner fallbacks reads 0 by construction, no matter what happened.
#
# Measured, not inferred: `day-174-20260821T232342Z` and `day-175-20260822T021038Z`
# both read `fallback=0` while both ran the planner-fallback path and committed nothing
# but a wrap-up. Both carry `transcripts/plan_retry.log` and an `unverified_task_1.md`.
#
# Why artifacts rather than more greps: these files are written by the harness into the
# directory. They cannot be produced by my own prose, which is the #810 contamination
# defect that forced the anchored-line design in the first place. Their presence is a
# fact about the run, not a string that happened to appear in a log.
#
# ENUMERATED BY LISTING ALL 453 SESSION DIRECTORIES ON `audit-log` (day 175), not from
# memory and not from the task file that asked for this:
#   top level    outcome.json 453, trajectory.stderr.log 453, transcripts/ 453,
#                audit.jsonl 447, trajectory.md 441,
#                eval_verdict_task<N>_attempt<N>.md 371, eval_checklist.log 118,
#                unverified_task_<N>.md 11
#   transcripts/ plan.log 453, assess.log 453, eval_task<N>_attempt<N>.log 1514,
#                task_<N>_attempt<N>.log 893, fix_* 693, respond.log 308, bfix_* 115,
#                plan_retry.log 62
#   outcome.json keys, union over all 453:
#                build_ok, day, reverted, session_time, session_type, tasks_attempted,
#                tasks_succeeded, test_ok, ts (453 each);
#                applied_pattern_keys 309; fallback_phases 131; model 131
#
# NOTE, because the task that commissioned this said otherwise: `plan_retry.log` is in
# `transcripts/`, NOT at the top level. Coded against the listing, not the description.

# The explicit third value. `False` means observed-and-not-present; UNKNOWN means this
# stream cannot answer. Absence must not collapse into False — this repo has paid for
# that distinction repeatedly (UngradedScan.unkeyed_excluded, NeverForecast's
# unknown-age branch, JobSnapshot.runtime).
UNKNOWN = "unknown"

# A planner retry ran. Evidence TOWARD a fallback session, never a synonym for one: the
# retry can also succeed and produce tasks. 62 of 453 directories carry it.
PLAN_RETRY_ARTIFACT = "transcripts/plan_retry.log"

# The evaluator produced no verdict for a task. 11 of 453 directories carry one.
UNVERIFIED_ARTIFACT_RE = re.compile(r"^unverified_task_\d+\.md$")

# Why `zero_output` is UNKNOWN and stays UNKNOWN. Nothing in a session directory records
# which commits the session made: not one of the twelve outcome.json keys above, and no
# file in the directory. `tasks_succeeded: 1` for a session that committed nothing is
# `scripts/evolve.sh`'s own defect (protected file, cannot be fixed here) — and deriving
# "zero output" from `tasks_succeeded` alone would be inventing the number this tool
# exists to stop inventing. An honest unknown is the deliverable.
NO_COMMIT_EVIDENCE = (
    "commit evidence is not carried in the session directory stream "
    "(no outcome.json key and no file records the session's commits)"
)


class StructuralFacts:
    """Named facts about one session directory. Every field is True / False / UNKNOWN.

    Named by what is *observable*, never by what it suggests: `plan_retry` says a
    planner retry artifact is present, not "this was a fallback session".
    """

    def __init__(self):
        self.plan_retry = UNKNOWN
        self.unverified = UNKNOWN
        self.tasks_attempted = UNKNOWN
        self.tasks_succeeded = UNKNOWN
        self.zero_output = UNKNOWN

    @staticmethod
    def _render(value):
        if value is True:
            return "yes"
        if value is False:
            return "no"
        if value is UNKNOWN:
            return UNKNOWN
        return str(value)

    def row(self):
        return (
            "    structure: "
            f"plan_retry={self._render(self.plan_retry)} "
            f"unverified={self._render(self.unverified)} "
            f"tasks_attempted={self._render(self.tasks_attempted)} "
            f"tasks_succeeded={self._render(self.tasks_succeeded)} "
            f"zero_output={self._render(self.zero_output)}"
        )

    def disagreement(self, log_fallback_count):
        """The finding, made legible instead of silently reconciled.

        Returns a line when the two streams disagree, else None. It states the
        *coverage* gap rather than asserting a fallback happened, because
        `plan_retry` is evidence toward one and not a synonym for one.
        """
        if self.plan_retry is True and log_fallback_count == 0:
            return (
                "    ! streams disagree: log-derived fallback=0 but a planner retry "
                f"artifact is present ({PLAN_RETRY_ARTIFACT}). The harness's stdout is "
                "not in the session directory, so a planner fallback in this session "
                "is invisible to the log-derived count."
            )
        return None


def classify_structure(relpaths, outcome):
    """Pure: named structural facts from what was OBSERVED. No filesystem work here.

    `relpaths`   POSIX-style paths relative to the session directory, or None when
                 there was no listing to take (a plain log file is not a session dir) —
                 then every listing-derived field is UNKNOWN, not False.
    `outcome`    the parsed outcome.json dict, or None when it was not observed — then
                 every outcome-derived field is UNKNOWN, not zero.
    """
    facts = StructuralFacts()

    if relpaths is not None:
        names = {str(p).replace(os.sep, "/") for p in relpaths}
        facts.plan_retry = PLAN_RETRY_ARTIFACT in names
        facts.unverified = any(UNVERIFIED_ARTIFACT_RE.match(n) for n in names)

    if isinstance(outcome, dict):
        for field in ("tasks_attempted", "tasks_succeeded"):
            value = outcome.get(field, UNKNOWN)
            # A present-but-wrong-typed value is UNKNOWN too: a string where an int
            # belongs is not observable as a count.
            setattr(facts, field, value if isinstance(value, int) else UNKNOWN)

    # Deliberately not derived. See NO_COMMIT_EVIDENCE.
    facts.zero_output = UNKNOWN
    return facts


def summarize_structure(sessions):
    """Pure: the aggregate structural block, or None when no directory was read.

    Reported BESIDE the log-derived counts and never summed into them — the two streams
    have different coverage, and adding them would double-count exactly when both are
    available. Same rule the file already applies to evolve.sh:1567/1605 vs 1610.
    """
    observed = [s for s in sessions if s.structure is not None]
    if not observed:
        return None
    listed = [s for s in observed if s.structure.plan_retry is not UNKNOWN]
    retries = [s for s in listed if s.structure.plan_retry is True]
    unverified = [
        s for s in observed if s.structure.unverified is True
    ]
    disagree = [
        s for s in observed if s.structure.disagreement(s.fallback) is not None
    ]
    unknown_outcome = [
        s for s in observed if s.structure.tasks_succeeded is UNKNOWN
    ]
    return (
        "structural signals — a SECOND stream, reported beside the log-derived counts "
        "above and never summed into them:\n"
        f"  session directories read: {len(observed)} "
        f"({len(listed)} with a readable listing)\n"
        f"  planner retry artifact present: {len(retries)}\n"
        f"  unverified evaluator artifact present: {len(unverified)}\n"
        f"  outcome.json missing or unreadable: {len(unknown_outcome)} "
        "(counts reported as unknown, not zero)\n"
        f"  streams disagree (retry artifact present, log-derived fallback=0): "
        f"{len(disagree)}\n"
        f"  zero_output: unknown for all {len(observed)} — {NO_COMMIT_EVIDENCE}"
    )


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
    # #810 (2026-08-25): how many rows were aged by the operator's `--session-ts`
    # rather than by their own filename. Emitted only when > 0, so every run without
    # the flag is byte-identical. This count is what makes the operator's claim
    # AUDITABLE after the fact — an asserted age is a claim by me, and a meter that
    # reads a stream I also write must never let my claim pass as an observation.
    asserted = sum(1 for s in sessions if s.ts_asserted)
    if asserted:
        header += (
            f"asserted age: {asserted} row(s) used an operator-asserted timestamp "
            f"(--session-ts) because their filename carried none. That is a CLAIM, "
            f"not an observation: it says every such input is from at/after that "
            f"instant, which is checkable only by whoever fetched them.\n"
        )
    if boundary_label is not None:
        ineligible = [s for s in sessions if s.eligibility == INELIGIBLE]
        unknown = [s for s in sessions if s.eligibility == UNKNOWN_AGE]
        sessions = [s for s in sessions if s.eligibility == ELIGIBLE]
        stamp = boundary_ts.isoformat() if boundary_ts is not None else "unresolved"
        header += (
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
    # The fourth bucket. A zero-task outcome with provider-error evidence cannot grade
    # #808 in either direction — the gate is REQUIRED not to fire after a provider
    # refusal — so it leaves numerator and denominator both, and is named here rather
    # than folded into any of the three exclusions above.
    # The fifth exclusion state / fourth bucket (#810, 2026-08-24). An input carrying
    # no unquoted harness marker was not measured at all, so it must not stand in for a
    # measured zero. It takes PRECEDENCE over the provider-error bucket below (an
    # untrustworthy input cannot be evidence of anything, including a rate limit), and
    # the two lists are kept disjoint so nothing is counted twice.
    no_output_excluded = [s for s in sessions if s.no_harness_output]
    if no_output_excluded:
        header += (
            f"excluded (no harness output): {len(no_output_excluded)} session(s) "
            f"carried no unquoted `scripts/evolve.sh` phase marker — empty, truncated "
            f"or the wrong stream, so nothing was measured. Excluded from numerator "
            f"and denominator both, because an absent input must never stand in for a "
            f"measured zero.\n"
        )
    # Disjoint from the bucket above by construction (an empty marker set classifies as
    # PHASE_REACH_NO_OUTPUT, not PHASE_REACH_NONE), so no extra filtering is needed
    # here — but it IS reported separately and never summed into any other bucket.
    no_phase_excluded = [s for s in sessions if s.no_phase_reached]
    if no_phase_excluded:
        header += (
            f"excluded (no phase reached): {len(no_phase_excluded)} session(s) carried "
            f"harness output but never reached a measurable phase (no Phase A1/A2/B "
            f"and no task start — only the bookend `=== Day N ===` lines the harness "
            f"prints whether or not work happened). The loop started and died before "
            f"any phase, so a starved session would otherwise print the row a healthy "
            f"one prints. Excluded from numerator and denominator both. Says only that "
            f"no measurable phase was reached, never that the session was any good.\n"
        )
    provider_excluded = [
        s
        for s in sessions
        if s.provider_error_excluded
        and not s.no_harness_output
        and not s.no_phase_reached
    ]
    if provider_excluded:
        header += (
            f"excluded (provider error): {len(provider_excluded)} session(s) had a "
            f"zero-task outcome alongside provider-error evidence — the gate is "
            f"required NOT to fire after a provider refusal, so these grade nothing "
            f"and are excluded from numerator and denominator both. Session-level "
            f"presence, not per-phase attribution: a judgment threshold, not a "
            f"measurement.\n"
        )
    prov_clause = (
        f", {len(provider_excluded)} excluded for provider errors"
        if provider_excluded
        else ""
    )
    if no_output_excluded:
        prov_clause += (
            f", {len(no_output_excluded)} excluded for no harness output"
        )
    if no_phase_excluded:
        prov_clause += (
            f", {len(no_phase_excluded)} excluded for reaching no phase"
        )
    other_excluded = (
        len(provider_excluded) + len(no_output_excluded) + len(no_phase_excluded)
    )
    if n < MIN_GRADEABLE_SESSIONS:
        return header + (
            f"NOT YET GRADEABLE: {n} of {MIN_GRADEABLE_SESSIONS} gradeable sessions "
            f"({len(sessions)} session(s) read, "
            f"{len(sessions) - n - other_excluded} with zero "
            f"abstentions and so excluded from both numerator and denominator"
            f"{prov_clause}). "
            f"Recording the wait, not a verdict."
        )
    fired = sum(1 for s in gradeable if s.firings > 0)
    return header + (
        f"VERDICT: of {n} gradeable sessions, the gate fired in {fired} "
        f"({len(sessions)} session(s) read, "
        f"{len(sessions) - n - other_excluded} excluded for zero "
        f"abstentions{prov_clause})."
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


def read_outcome(path):
    """I/O: the parsed outcome.json under a session directory, or None.

    None covers all three failure shapes — absent, unreadable, unparseable — because
    the caller treats them identically: this stream did not carry the counts, which is
    UNKNOWN and not zero.
    """
    try:
        with open(
            os.path.join(path, "outcome.json"), "r", encoding="utf-8", errors="replace"
        ) as fh:
            outcome = json.load(fh)
    except (OSError, ValueError):
        return None
    return outcome if isinstance(outcome, dict) else None


def session_from_path(path, asserted_ts=None):
    """A file is one session; a directory is one session made of all files under it.

    `asserted_ts` (an aware datetime from `--session-ts`, or None) is the operator's
    claim about inputs whose NAME cannot carry a stamp — the workflow log, which is the
    only stream carrying harness markers and therefore the only gradeable one. It is
    applied HERE, at the call site, and never inside `session_timestamp`, which stays
    exactly what it was: a pure reader of what the harness already wrote.

    Precedence is fixed and one-directional: **observed beats asserted, always.** A
    filename-stamped input ignores `--session-ts` entirely, so the flag can never
    overwrite a fact with a claim.
    """
    if os.path.isdir(path):
        lines = []
        relpaths = []
        for root, _dirs, files in os.walk(path):
            for fname in sorted(files):
                full = os.path.join(root, fname)
                relpaths.append(os.path.relpath(full, path))
                lines.extend(read_lines(full))
        name = os.path.basename(path.rstrip("/")) or path
        counts = count_lines(name, lines)
        # I/O here, decision in `classify_structure` — the listing and the parsed
        # outcome are handed over; nothing under this line touches the filesystem.
        counts.structure = classify_structure(relpaths, read_outcome(path))
    else:
        name = os.path.basename(path)
        counts = count_lines(name, read_lines(path))
        # A plain log file is not a session directory: there is no listing to take, so
        # the structural stream stays absent rather than reporting a listing of False.
    counts.ts = session_timestamp(name)
    if counts.ts is None and asserted_ts is not None:
        counts.ts = asserted_ts
        counts.ts_asserted = True
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
    # Deliberately NOT in the `boundary` mutually-exclusive group: this is not a
    # boundary, it is an INPUT AGE, and composing it with --since-sha/--since is the
    # whole point (#810, 2026-08-25). The two correct guards — "only the workflow log
    # carries harness markers" and "age comes from a day-N-<ts> filename" — used to
    # compose into a capability that could not be exercised at all.
    parser.add_argument(
        "--session-ts",
        metavar="ISO8601",
        help="age for inputs whose FILENAME carries no day-N-<ts> stamp (e.g. a "
        "`gh run view --log` capture). Observed beats asserted: a filename-stamped "
        "input ignores this. Rows that used it are marked [ts=asserted] and counted "
        "in the verdict. Composes with --since-sha/--since.",
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
    asserted_ts = None
    if args.session_ts:
        asserted_ts = parse_timestamp(args.session_ts)
        if asserted_ts is None:
            # Same refusal shape as an unresolvable --since-sha: an unparseable claim
            # must not silently become "no claim", which would put every nameless log
            # back into UNKNOWN_AGE while the operator believes it was aged.
            print(
                f"error: could not parse --session-ts {args.session_ts!r}. "
                f"Refusing to grade rather than silently treating the inputs as "
                f"unknown-age.",
                file=sys.stderr,
            )
            return 2
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

    sessions = [session_from_path(p, asserted_ts) for p in args.paths]
    apply_boundary(sessions, boundary_ts)
    print(f"#810 abstention/firing measurement over {len(sessions)} session(s):")
    for s in sessions:
        print(s.row())
        # The structural stream prints on its OWN lines, under the row it belongs to.
        # A plain log file has no structure block, so an old-style invocation prints
        # byte-identically to before.
        if s.structure is not None:
            print(s.structure.row())
            note = s.structure.disagreement(s.fallback)
            if note is not None:
                print(note)
    print()
    structural = summarize_structure(sessions)
    if structural is not None:
        print(structural)
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
            # A real log carries harness stdout; without a phase marker this
            # fixture would be EXCLUDED_NO_HARNESS_OUTPUT (#810, 2026-08-24).
            "  Phase B: Implementation...",
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

    # 10. The structural stream (day 175). A directory listing is a second, independent
    # source with different coverage from the workflow log.
    real_listing = [
        "outcome.json",
        "trajectory.md",
        "trajectory.stderr.log",
        "unverified_task_1.md",
        "transcripts/assess.log",
        "transcripts/plan.log",
        "transcripts/plan_retry.log",
        "transcripts/respond.log",
        "transcripts/task_01_attempt1.log",
    ]
    real_outcome = {
        "day": 175,
        "ts": "2026-08-22T02:10:38Z",
        "build_ok": True,
        "test_ok": True,
        "tasks_attempted": 1,
        "tasks_succeeded": 1,
        "reverted": False,
    }
    facts = classify_structure(real_listing, real_outcome)
    check("plan_retry present", facts.plan_retry, True)
    check("unverified present", facts.unverified, True)
    check("tasks_attempted read", facts.tasks_attempted, 1)
    check("tasks_succeeded read", facts.tasks_succeeded, 1)
    # zero_output is never derived from tasks_succeeded, however tempting.
    check("zero_output stays unknown", facts.zero_output, UNKNOWN)

    # The near-miss side, so the discriminator is not tested only where it fires: a
    # listing WITHOUT the artifact reports False (observed-and-absent), not unknown.
    plain_listing = [
        "outcome.json",
        "audit.jsonl",
        "eval_verdict_task1_attempt1.md",
        "transcripts/assess.log",
        "transcripts/plan.log",
        "transcripts/task_01_attempt1.log",
    ]
    plain = classify_structure(plain_listing, {"tasks_attempted": 2, "tasks_succeeded": 2})
    check("no retry artifact → False", plain.plan_retry, False)
    check("no retry artifact is not unknown", plain.plan_retry is UNKNOWN, False)
    check("no unverified artifact → False", plain.unverified, False)
    # A top-level plan_retry.log must NOT count: the artifact lives in transcripts/,
    # and this was the one detail the commissioning task got wrong.
    check(
        "top-level plan_retry.log does not match",
        classify_structure(["plan_retry.log"], None).plan_retry,
        False,
    )

    # No outcome.json → the outcome-derived fields are UNKNOWN, never zero.
    no_outcome = classify_structure(real_listing, None)
    check("missing outcome → attempted unknown", no_outcome.tasks_attempted, UNKNOWN)
    check("missing outcome → succeeded unknown", no_outcome.tasks_succeeded, UNKNOWN)
    check("missing outcome does not zero the count", no_outcome.tasks_succeeded == 0, False)
    check("listing still read without outcome", no_outcome.plan_retry, True)
    # A present-but-wrong-typed value is unobservable as a count, so it is unknown too.
    check(
        "string count is unknown",
        classify_structure([], {"tasks_succeeded": "1"}).tasks_succeeded,
        UNKNOWN,
    )
    # A key absent from an otherwise valid outcome.json is unknown, not zero.
    check(
        "absent key is unknown",
        classify_structure([], {"day": 175}).tasks_attempted,
        UNKNOWN,
    )

    # No listing at all (a plain log file) → the listing-derived fields are UNKNOWN.
    no_listing = classify_structure(None, real_outcome)
    check("no listing → plan_retry unknown", no_listing.plan_retry, UNKNOWN)
    check("no listing → unverified unknown", no_listing.unverified, UNKNOWN)
    check("no listing → outcome still read", no_listing.tasks_succeeded, 1)

    # 11. The disagreement is legible, and only where it exists.
    check(
        "retry artifact + log fallback=0 disagrees",
        "streams disagree" in (facts.disagreement(0) or ""),
        True,
    )
    check(
        "disagreement names the artifact",
        PLAN_RETRY_ARTIFACT in (facts.disagreement(0) or ""),
        True,
    )
    check("both streams saw it → no disagreement", facts.disagreement(1), None)
    check("no retry artifact → no disagreement", plain.disagreement(0), None)
    check("unknown listing → no disagreement", no_listing.disagreement(0), None)
    check("row names every field", facts.row().count("="), 5)
    check("row renders unknown as a word", "zero_output=unknown" in facts.row(), True)

    # 12. THE NO-REGRESSION PIN. The structural stream changes no existing number, and
    # a session with no structure block renders and grades exactly as before.
    structural_session = count_lines(
        "day-175-20260822T021038Z",
        [
            "  Phase B: Implementation...",
            "ordinary log noise",
            "  ⚡ auto-continuing (1/5 — more work pending)...",
        ],
    )
    before_row = structural_session.row()
    before_counts = (
        structural_session.abstentions,
        structural_session.firings,
        structural_session.fallback,
        structural_session.gradeable,
    )
    structural_session.structure = classify_structure(real_listing, real_outcome)
    check("structure does not touch the row", structural_session.row(), before_row)
    check(
        "structure does not touch the counts",
        (
            structural_session.abstentions,
            structural_session.firings,
            structural_session.fallback,
            structural_session.gradeable,
        ),
        before_counts,
    )
    check(
        "structure does not touch the verdict",
        grade([structural_session] + four),
        grade(four + [mk("z", 0, 1)]),
    )

    # 13. The aggregate block: reported beside, never summed into, the log counts.
    check("no directories read → no block", summarize_structure(four), None)
    block = summarize_structure([structural_session])
    check("block says it is a second stream", "never summed" in block, True)
    check("block counts the retry artifact", "planner retry artifact present: 1" in block, True)
    check("block counts the disagreement", "streams disagree" in block, True)
    check("block states the commit-evidence limit", NO_COMMIT_EVIDENCE in block, True)
    unknown_session = count_lines("nameless", [])
    unknown_session.structure = classify_structure(None, None)
    unknown_block = summarize_structure([unknown_session])
    check(
        "unknown outcome is counted, not dropped",
        "outcome.json missing or unreadable: 1" in unknown_block,
        True,
    )
    check(
        "unreadable listing is disclosed",
        "(0 with a readable listing)" in unknown_block,
        True,
    )

    # 11. The provider-error bucket (#810, @yuanhao 2026-08-22). The detector in BOTH
    # directions first: the real emissions match, and the same strings wearing my prose
    # tells do not — this issue's own thread carries "Rate limited" in a markdown table.
    check(
        "API-path rate-limit line matches",
        classify_provider_error_line(
            "  error: Rate limited, retry after Some(14454000)ms"
        ),
        True,
    )
    check(
        "diagnostic hint line matches",
        classify_provider_error_line("  💡 Rate limited by provider 'anthropic'."),
        True,
    )
    check(
        "coloured emission matches after ANSI strip",
        classify_provider_error_line(
            "\x1b[31m  error: Rate limited, retry after Some(9)ms\x1b[0m"
        ),
        True,
    )
    check(
        "runner tab prefix is allowed",
        classify_provider_error_line(
            "job\tjob\t2026-08-22T01:17:03.1234567Z   error: Rate limited, "
            "retry after Some(14454000)ms"
        ),
        True,
    )
    check(
        "other 429 wording still matches",
        classify_provider_error_line("  error: HTTP 429 too many requests"),
        True,
    )
    # Both directions of the prose filter — the near-miss that must pass through, and
    # the three tells that must not.
    check(
        "markdown table cell is prose",
        classify_provider_error("| 08-21T21:25 | error: Rate limited, retry after "
                                "Some(14454000)ms |"),
        (False, True),
    )
    check(
        "backticked quote is prose",
        classify_provider_error(
            "The planner turn ended with `this`:   error: Rate limited, retry "
            "after Some(1)ms"
        ),
        (False, True),
    )
    check(
        "grep prefix is prose",
        classify_provider_error(
            "logs/run.log:42:  error: Rate limited, retry after Some(1)ms"
        ),
        (False, True),
    )
    check(
        "real emission is not prose",
        classify_provider_error("  error: Rate limited, retry after Some(14454000)ms"),
        (True, False),
    )
    check(
        "ordinary log noise is neither",
        classify_provider_error("  Running `cargo test`"),
        (False, False),
    )
    # A provider-error line must not be mistaken for an abstention marker, and vice
    # versa — the two detectors read disjoint shapes.
    check(
        "abstention marker is not a provider error",
        classify_provider_error_line(
            "Planning agent produced 0 tasks — falling back to single task."
        ),
        False,
    )

    # `count_lines` folds it: the shaped line counts, the quoted twin is reported as
    # prose rather than silently dropped.
    prov_session = count_lines(
        "day-175-20260821T212500Z",
        [
            "  Phase B: Implementation...",
            "WARNING: No assessment produced — planning agent will read source "
            "directly (slower).",
            "  error: Rate limited, retry after Some(14454000)ms",
            "  💡 Rate limited by provider 'anthropic'.",
            "| a | error: Rate limited, retry after Some(3)ms |",
            "Planning agent produced 0 tasks — falling back to single task.",
        ],
    )
    check("provider errors counted", prov_session.provider_errors, 2)
    check("quoted provider error reported", prov_session.prose_excluded, 1)
    check("abstention still counted", prov_session.abstentions, 1)
    check("fallback still counted", prov_session.fallback, 1)
    check("rate-limited session is not gradeable", prov_session.gradeable, False)
    check("bucket fires", prov_session.provider_error_excluded, True)
    check(
        "row names the bucket",
        f"[{EXCLUDED_PROVIDER_ERROR}: 2 line(s)]" in prov_session.row(),
        True,
    )

    # THE NEAR-MISS GUARD: an abstention with NO provider error must grade exactly as it
    # does today. A discriminator tested only on the side that blocks is vacuous green,
    # and this one silently shrinks my own denominator when it is wrong.
    clean_session = count_lines(
        "day-175-20260822T034200Z",
        [
            # A real log carries harness stdout; without a phase marker this
            # fixture would be EXCLUDED_NO_HARNESS_OUTPUT (#810, 2026-08-24).
            "  Phase B: Implementation...",
            "WARNING: No assessment produced — planning agent will read source "
            "directly (slower).",
            "  Running `cargo test`",
        ],
    )
    check("clean session has no provider errors", clean_session.provider_errors, 0)
    check("clean session still gradeable", clean_session.gradeable, True)
    check("clean session bucket silent", clean_session.provider_error_excluded, False)
    check("clean row carries no suffix", "[" in clean_session.row(), False)

    # A provider error in a session with NO abstention changes nothing that is graded —
    # such a session was already excluded for zero abstentions, so the bucket must not
    # claim it. The line is still surfaced on the row, labelled as evidence, not a
    # verdict.
    noisy_zero = count_lines(
        "day-175-20260822T064100Z",
        [
            "  Phase B: Implementation...",
            "  error: Rate limited, retry after Some(5)ms",
        ],
    )
    check("no abstention → bucket silent", noisy_zero.provider_error_excluded, False)
    check("no abstention → not gradeable anyway", noisy_zero.gradeable, False)
    check(
        "line still disclosed on the row",
        "[provider_error_lines=1]" in noisy_zero.row(),
        True,
    )

    # `grade`: the bucket leaves BOTH halves and is never summed into the others.
    def mk_prov(name, abstentions, firings, provider_errors):
        s = mk(name, abstentions, firings)
        s.provider_errors = provider_errors
        return s

    mixed = [
        mk("a", 1, 1),
        mk("b", 1, 0),
        mk("c", 1, 1),
        mk("d", 1, 0),
        mk_prov("rate-1", 2, 0, 3),
        mk_prov("rate-2", 2, 0, 15),
        mk("zero", 0, 0),
    ]
    gv = grade(mixed)
    check("rate-limited pair leaves the denominator", "of 4 gradeable sessions" in gv, True)
    check("rate-limited pair leaves the numerator", "the gate fired in 2" in gv, True)
    check("bucket named in the header", "excluded (provider error): 2 session(s)" in gv, True)
    check("bucket named in the verdict", "2 excluded for provider errors" in gv, True)
    check(
        "zero-abstention count is not inflated by the bucket",
        "1 excluded for zero abstentions" in gv,
        True,
    )
    check("coarseness disclosed", "not per-phase attribution" in gv, True)
    # Below the floor the wait names it too, and MIN_GRADEABLE_SESSIONS is unchanged.
    waiting = grade([mk("a", 1, 1), mk_prov("rate", 2, 0, 4)])
    check("wait names the bucket", "1 excluded for provider errors" in waiting, True)
    check("wait still refuses a verdict", waiting.endswith("Recording the wait, not a verdict."), True)
    check("floor unchanged", MIN_GRADEABLE_SESSIONS, 4)
    # Byte-identity: with no provider errors anywhere, `grade` returns the pre-change
    # string verbatim.
    check(
        "no provider errors → pre-change verdict verbatim",
        grade([mk("a", 1, 1), mk("b", 1, 0), mk("c", 1, 1), mk("d", 1, 0), mk("z", 0, 0)]),
        "VERDICT: of 4 gradeable sessions, the gate fired in 2 "
        "(5 session(s) read, 1 excluded for zero abstentions).",
    )

    # ------------------------------------------------------------------
    # 14. THE FOURTH BUCKET (#810, @yuanhao 2026-08-24): an empty or marker-less log
    # must not score as a clean session. "Could not check" is not "checked; clean".
    # ------------------------------------------------------------------

    # (a) The reported reproduction, verbatim: a 0-byte log.
    empty = count_lines("empty.log", [])
    check("empty log has no harness output", empty.no_harness_output, True)
    check("empty log is not gradeable", empty.gradeable, False)
    check(
        "empty log row names the bucket",
        f"[{EXCLUDED_NO_HARNESS_OUTPUT}]" in empty.row(),
        True,
    )

    # (b) Bytes but no marker — the truncated/partial download a byte-size anchor
    # would wave through. This is the half the marker anchor exists for.
    truncated = count_lines(
        "truncated.log",
        [
            "  Running `cargo build`",
            "   Compiling yoyo v0.1.0 (/home/runner/work/yoyo-evolve/yoyo-evolve)",
            "    Finished dev [unoptimized + debuginfo] target(s) in 41.02s",
            "ordinary log noise with plenty of bytes and no phase line at all",
        ],
    )
    check("truncated log has bytes", len(truncated.name) > 0, True)
    check("truncated log excluded", truncated.no_harness_output, True)

    # (c) THE NEAR-MISS GUARD, and the positive control in miniature: ONE unquoted
    # marker is enough, and every other number is what it was before this bucket
    # existed. If the marker set is wrong, EVERY real session excludes itself, the
    # denominator goes to zero and the tool says NOT YET GRADEABLE forever — the same
    # "cannot fail loudly" defect wearing the opposite sign, and quieter.
    real = count_lines(
        "real.log",
        [
            "  Phase A2: Planning (5400s)...",
            "  WARNING: No assessment produced — planning agent will read source "
            "directly (slower).",
            "  ⚡ auto-continuing (1/5 — more work pending)...",
        ],
    )
    check("one marker is enough", real.no_harness_output, False)
    check("marker does not disturb abstentions", real.abstentions, 1)
    check("marker does not disturb firings", real.firings, 1)
    check("real session still gradeable", real.gradeable, True)
    check("real row carries no exclusion suffix", "[" in real.row(), False)

    # (d) A log whose ONLY marker occurrence is my own prose is still excluded — the
    # #810 contamination defect, one level up. This file, #810's thread and the
    # CLAUDE.md paragraph documenting the fix all contain these strings.
    prose_only = count_lines(
        "prose.log",
        [
            # A backticked mention in my own prose, ending at the marker.
            "the harness `echo`es:   Phase A1: Assessment (5400s)...",
            "scripts/evolve.sh:1291:  Phase A2: Planning (5400s)...",
            "> === Day 177 complete ===",
        ],
    )
    check("quoted markers do not count", prose_only.no_harness_output, True)
    check("quoted markers are counted, not dropped", prose_only.harness_prose_excluded, 3)
    check(
        "row discloses the quoted count",
        "[harness_markers_quoted=3]" in prose_only.row(),
        True,
    )

    # (e) Each of the six markers, individually. A marker set is only real where every
    # member is exercised — one unexercised member is a silent hole in the anchor.
    for label, line in (
        (SESSION_HEADER, "=== Day 177 (2026-08-24 22:41) ==="),
        (PHASE_A1, "  Phase A1: Assessment (5400s)..."),
        (PHASE_A2, "  Phase A2: Planning (5400s)..."),
        (PHASE_B, "  Phase B: Implementation..."),
        (TASK_START, "  → Task 2: #810 — the fourth bucket [evolve]"),
        (SESSION_FOOTER, "=== Day 177 complete ==="),
    ):
        check(f"marker {label} recognised", classify_harness_output_line(line), label)
        check(f"marker {label} unquoted", classify_harness_output(line), (label, False))
        # And with `gh run view --log`'s runner prefix in front.
        prefixed = f"evolve\tevolve\t2026-08-24T22:41:03.1234567Z {line.strip()}"
        check(f"marker {label} survives the runner prefix",
              classify_harness_output_line(prefixed), label)

    # The prompt-text banners are deliberately NOT markers: a log holding only an
    # echoed prompt must not read as a real session.
    check(
        "prompt banner is not a harness marker",
        classify_harness_output_line("=== YOUR TRAJECTORY ==="),
        None,
    )
    check(
        "pure fold agrees with the per-line half",
        has_harness_output(["noise", "  Phase B: Implementation..."]),
        True,
    )
    check("pure fold on an empty stream", has_harness_output([]), False)

    # (f) The verdict names the bucket and folds it into nothing else.
    def mk_noout(name, abstentions, firings):
        c = mk(name, abstentions, firings)
        c.harness_markers = 0
        return c

    def mk_seen(name, abstentions, firings):
        c = mk(name, abstentions, firings)
        c.harness_markers = 3
        return c

    v = grade([mk_seen("a", 1, 1), mk_seen("b", 1, 0), mk_noout("dead", 1, 0)])
    check("verdict names the bucket", "excluded (no harness output): 1 session(s)" in v, True)
    check("bucket named in the tail", "1 excluded for no harness output" in v, True)
    check("not folded into provider errors", "excluded for provider errors" in v, False)
    check("not folded into zero abstentions", "0 with zero abstentions" in v, True)
    check("marker-less session left the denominator", "2 of 4 gradeable sessions" in v, True)
    check("floor still unchanged", MIN_GRADEABLE_SESSIONS, 4)

    # (g) Byte-identity for the healthy path: with harness output everywhere, `grade`
    # returns the pre-change string verbatim. This is the whole common path and the
    # regression risk.
    check(
        "harness output present → pre-change verdict verbatim",
        grade([
            mk_seen("a", 1, 1), mk_seen("b", 1, 0), mk_seen("c", 1, 1),
            mk_seen("d", 1, 0), mk_seen("z", 0, 0),
        ]),
        "VERDICT: of 4 gradeable sessions, the gate fired in 2 "
        "(5 session(s) read, 1 excluded for zero abstentions).",
    )

    # (h) The third state: a hand-built SessionCounts folded NO line stream, so
    # "nobody looked" must not be reported as "nothing was there".
    check("unobserved stream is not the bucket", mk("hand", 1, 0).no_harness_output, False)
    check("unobserved stream stays gradeable", mk("hand", 1, 0).gradeable, True)

    # -- EXCLUDED_NO_PHASE_REACHED (#810, @yuanhao 2026-08-25) --------------------
    # A destroyed session must not score like a healthy one. The three real logs
    # (32667319793, 32679577526, 32688121916) died at build verification and carry
    # three `=== Day N (...) ===` headers each — harness output present, no phase.

    # (a) The pure classifier, all three states, none folded into another.
    check("no markers is the other bucket's state",
          classify_phase_reach(set()), PHASE_REACH_NO_OUTPUT)
    check("bookends only -> no phase reached",
          classify_phase_reach({SESSION_HEADER}), PHASE_REACH_NONE)
    check("header+footer is still bookends only",
          classify_phase_reach({SESSION_HEADER, SESSION_FOOTER}), PHASE_REACH_NONE)
    check("Phase A1 -> measurable",
          classify_phase_reach({SESSION_HEADER, PHASE_A1}), PHASE_REACH_MEASURABLE)
    # The widening over #810's literal wording: a head-truncated log that lost its
    # A1 line but carries later phases has demonstrably done work. Under-fire here
    # rather than over-fire, because over-firing destroys real data points.
    for late in (PHASE_A2, PHASE_B, TASK_START):
        check(f"{late} alone is still measurable",
              classify_phase_reach({SESSION_HEADER, late}), PHASE_REACH_MEASURABLE)

    # (b) The set fold, and the prose filter surviving into it. `Phase A1` appears in
    # #810's thread, in this file, and in CLAUDE.md — a log holding only my prose
    # about the detector must NOT read as having reached Phase A1.
    check("fold collects unquoted names",
          phase_reach_markers(["  Phase A1: Assessment (5400s)...",
                               "=== Day 177 (2026-08-24 22:41) ==="]),
          {PHASE_A1, SESSION_HEADER})
    check("fold drops quoted prose",
          phase_reach_markers(["=== Day 177 (2026-08-24 22:41) ===",
                               "the harness prints `  Phase A1: Assessment (5400s)...`",
                               "| Phase A1: Assessment (5400s)... |"]),
          {SESSION_HEADER})

    # (c) End to end: the starved shape is excluded, and its quoted-prose twin lands
    # in the SAME bucket rather than sneaking into the measurable set.
    starved = count_lines("starved.log", [
        "=== Day 177 (2026-08-24 22:41) ===",
        "  error: test failed, to rerun pass `--bin yoyo`",
        "=== Day 177 (2026-08-24 23:05) ===",
    ])
    check("starved shape excluded", starved.no_phase_reached, True)
    check("starved is not the marker-less bucket", starved.no_harness_output, False)
    check("starved is not gradeable", starved.gradeable, False)
    check("row names the bucket",
          f"[{EXCLUDED_NO_PHASE_REACHED}]" in starved.row(), True)

    prose_phase = count_lines("prose_phase.log", [
        "=== Day 177 (2026-08-24 22:41) ===",
        "scripts/evolve.sh:1167:  Phase A1: Assessment (5400s)...",
    ])
    check("quoted Phase A1 does not rescue a starved log",
          prose_phase.no_phase_reached, True)
    check("quoted Phase A1 is counted, not dropped",
          prose_phase.harness_prose_excluded, 1)

    # (d) THE NEGATIVE CONTROL. My most recent injury sets the direction I guard, and
    # last time I shipped a bucket verified only where it fires. A log that reached a
    # phase must be untouched — same row, same gradeability.
    healthy = count_lines("healthy.log", [
        "=== Day 177 (2026-08-24 22:41) ===",
        "  Phase A1: Assessment (5400s)...",
        "  Phase A2: Planning (5400s)...",
        "  WARNING: No assessment produced — planning agent will read source "
        "directly (slower).",
        "=== Day 177 complete ===",
    ])
    check("healthy log reaches a phase", healthy.no_phase_reached, False)
    check("healthy log still gradeable", healthy.gradeable, True)
    check("healthy row carries no exclusion suffix", "[" in healthy.row(), False)

    # (e) Disjointness: an empty log belongs to ONE bucket, not two.
    empty_both = count_lines("empty.log", [])
    check("empty log is the marker-less bucket", empty_both.no_harness_output, True)
    check("empty log is NOT this bucket", empty_both.no_phase_reached, False)
    check("empty row names one bucket only",
          f"[{EXCLUDED_NO_PHASE_REACHED}]" in empty_both.row(), False)

    # (f) An unfolded stream is neither — "nobody looked" is not "nothing was there".
    check("unobserved stream is not this bucket", mk("hand", 1, 0).no_phase_reached,
          False)

    # (g) The verdict reports it separately and sums it into nothing.
    def mk_starved(name, abstentions, firings):
        c = mk(name, abstentions, firings)
        c.harness_markers = 3
        c.harness_marker_names = {SESSION_HEADER}
        return c

    def mk_phased(name, abstentions, firings):
        c = mk(name, abstentions, firings)
        c.harness_markers = 5
        c.harness_marker_names = {SESSION_HEADER, PHASE_A1}
        return c

    v5 = grade([mk_phased("a", 1, 1), mk_phased("b", 1, 0),
                mk_starved("dead", 1, 0), mk_noout("empty", 1, 0)])
    check("verdict names the new bucket",
          "excluded (no phase reached): 1 session(s)" in v5, True)
    check("new bucket named in the tail",
          "1 excluded for reaching no phase" in v5, True)
    check("still names the marker-less bucket separately",
          "excluded (no harness output): 1 session(s)" in v5, True)
    check("buckets are not summed", "2 session(s)" in v5, False)
    check("starved session left the denominator",
          "2 of 4 gradeable sessions" in v5, True)
    check("MIN_GRADEABLE_SESSIONS unchanged", MIN_GRADEABLE_SESSIONS, 4)

    # (h) A log that reaches a phase produces the byte-identical verdict it did before
    # this bucket existed — the common path, and the regression risk.
    check(
        "measurable sessions verdict unchanged",
        grade([mk_phased("a", 1, 1), mk_phased("b", 1, 1),
               mk_phased("c", 1, 0), mk_phased("d", 1, 0)]),
        "VERDICT: of 4 gradeable sessions, the gate fired in 2 "
        "(4 session(s) read, 0 excluded for zero abstentions).",
    )

    # 11. `--session-ts`: an operator-asserted age for inputs whose FILENAME carries
    # none (#810, 2026-08-25). Two individually-correct guards — "only the workflow log
    # carries harness markers" and "age comes from a day-N-<ts> filename" — composed
    # into a capability that could not be exercised: the only gradeable stream is the
    # one that can never be aged. These drive the real `session_from_path`, because the
    # precedence rule lives at that call site and nowhere else.
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        nameless = os.path.join(tmp, "run-32777115768.log")
        with open(nameless, "w", encoding="utf-8") as fh:
            fh.write("Phase A1: Assessment (600s)...\n")
        stamped_dir = os.path.join(tmp, "day-177-20260824T030000Z")
        os.makedirs(stamped_dir)
        with open(os.path.join(stamped_dir, "log"), "w", encoding="utf-8") as fh:
            fh.write("Phase A1: Assessment (600s)...\n")

        asserted = parse_timestamp("2026-08-24T12:00:00Z")

        # (a) State 3 — no filename stamp and no assertion → UNKNOWN_AGE, exactly as
        # before. This is the byte-identity direction.
        s_none = session_from_path(nameless)
        check("no stamp, no assertion → ts is None", s_none.ts, None)
        check("no stamp, no assertion → not marked asserted", s_none.ts_asserted, False)
        check(
            "no stamp, no assertion → UNKNOWN_AGE",
            classify_eligibility(s_none.ts, parse_timestamp("2026-08-21T00:00:00Z")),
            UNKNOWN_AGE,
        )

        # (b) State 2 — no filename stamp, assertion given → the asserted stamp is
        # used AND marked, so a claim can never read as an observation.
        s_asserted = session_from_path(nameless, asserted)
        check("asserted stamp used", s_asserted.ts, asserted)
        check("asserted stamp marked", s_asserted.ts_asserted, True)
        check("asserted row carries the marker", "[ts=asserted]" in s_asserted.row(), True)
        check(
            "asserted stamp reaches the boundary decision",
            classify_eligibility(
                s_asserted.ts, parse_timestamp("2026-08-21T00:00:00Z")
            ),
            ELIGIBLE,
        )

        # (c) State 1 / precedence — observed beats asserted, ALWAYS. A wildly wrong
        # assertion must not overwrite a stamp the harness itself wrote.
        s_stamped = session_from_path(stamped_dir, parse_timestamp("2020-01-01T00:00:00Z"))
        check(
            "filename stamp wins over assertion",
            s_stamped.ts,
            parse_timestamp("2026-08-24T03:00:00Z"),
        )
        check("filename-stamped row is not marked", s_stamped.ts_asserted, False)
        check("filename-stamped row has no marker", "[ts=asserted]" in s_stamped.row(), False)

    # (d) The verdict counts the assertion, and only when one was used — the near-miss
    # side, since a clause that fires unconditionally is not a discriminator.
    a1 = mk("asserted-1", 1, 0)
    a1.ts = parse_timestamp("2026-08-24T12:00:00Z")
    a1.ts_asserted = True
    a2 = mk("observed-1", 1, 0)
    a2.ts = parse_timestamp("2026-08-24T12:00:00Z")
    v_asserted = grade([a1, a2], "abc1234", parse_timestamp("2026-08-21T00:00:00Z"))
    check("verdict counts asserted rows", "asserted age: 1 row(s)" in v_asserted, True)
    check(
        "asserted clause does not swallow the boundary header",
        "boundary: abc1234" in v_asserted,
        True,
    )
    check(
        "no assertion → no clause",
        "asserted age:" in grade([a2], "abc1234", parse_timestamp("2026-08-21T00:00:00Z")),
        False,
    )

    # (e) --session-ts is NOT mutually exclusive with the boundary flags: composing
    # them IS the fix, so an accidental exclusive group would silently reinstate the
    # dead end.
    composed = build_parser().parse_args(
        ["--since", "2026-08-21T00:00:00Z", "--session-ts", "2026-08-24T00:00:00Z", "x.log"]
    )
    check("composes with --since", composed.since, "2026-08-21T00:00:00Z")
    check("composes with --since (ts)", composed.session_ts, "2026-08-24T00:00:00Z")
    composed_sha = build_parser().parse_args(
        ["--since-sha", "c46d8453", "--session-ts", "2026-08-24T00:00:00Z", "x.log"]
    )
    check("composes with --since-sha", composed_sha.since_sha, "c46d8453")

    # (f) An unparseable assertion refuses to grade, exactly as an unresolvable
    # --since-sha does: a claim that could not be read must not become "no claim".
    check(
        "unparseable --session-ts refuses",
        main(["--session-ts", "not-a-date", "x.log"]),
        2,
    )
    check("parse_timestamp is the one parser", parse_timestamp("not-a-date"), None)

    if failures:
        for f in failures:
            print(f"FAIL {f}")
        print(f"\n{len(failures)} self-test failure(s)")
        return 1
    print("all self-tests passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
