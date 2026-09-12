#!/usr/bin/env python3
"""
extract_trajectory.py — Build the YOUR TRAJECTORY block injected into Phase A1
(assess) and Phase A2 (plan) prompts. Aggregates audit-log session evidence,
git log, and gh run history into a structured markdown summary so yoyo sees
ground truth about its own recent trajectory before deciding what to work on.

Inputs (env vars):
  YOYO_AUDIT_DIR       Path to audit-log worktree's `sessions/` directory.
  YOYO_REPO            owner/repo slug for `gh` calls (e.g. "yologdev/yoyo-evolve").
  YOYO_DAY             Current day number (used only for window calc + display).
  YOYO_TRAJECTORY_OUT  Output file path. Default: .yoyo/session_staging/trajectory.md.

Output:
  Writes a single markdown blob to YOYO_TRAJECTORY_OUT. ~1-2KB target, hard-capped
  at 100 lines / 2KB. Always exits 0; failure modes degrade per-section and write
  "(no trajectory data yet)" if no signal could be gathered.
"""
import json
import os
import re
import subprocess
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

# ── Configuration constants ──────────────────────────────────────────────
WINDOW_SESSIONS = 10           # last N sessions in the outcomes section
WINDOW_DAYS = 14               # git log window
MAX_FAILED_RUNS = 5            # cap on `gh run view --log-failed` calls
# Rows fetched by the green-since probe. Deliberately NOT 1, and this comment is
# the reason a future reader must not "simplify" it back. `--limit 1` makes a
# single row the WHOLE answer, so any ordering surprise from `gh` becomes the
# verdict with nothing to correct it. Measured 2026-08-25 on this repo:
#   gh run list --workflow ci.yml --status success --limit 1  -> 2026-07-24T18:32:13Z
#   gh run list --workflow ci.yml --status success --limit 5  -> 2026-08-25T14:43:20Z
# Same command, same moment, month-apart answers. (Re-measured later that day the
# `--limit 1` row was correct again, i.e. the misordering is intermittent, which
# is the worst kind to depend on.) With N rows the true newest is recoverable by
# max() over the parsed stamps, so the probe stops depending on position 0.
#
# SUPERSEDED, recorded rather than erased (Day 179): the paragraph above claims
# the `--limit 10` + max() pair CURED the stale answer. It cured the ORDERING
# half and not the PAGING half — max() over ten stale rows is still stale.
# Measured 2026-08-26: the first call of
#   gh run list --workflow ci.yml --status success --limit 10
# returned a page whose newest row was 2026-08-25T23:54:38Z, silently OMITTING
# the 2026-08-26T05:10:56Z success the unfiltered listing returned in the same
# minute; three immediate re-runs of the identical argv then included it, and a
# `--status completed` probe returned it every time. So `--status success` is a
# server-side filter that intermittently serves a stale page, and the probe now
# asks for `completed` and filters client-side.
#
# 20 is a JUDGMENT THRESHOLD, not a measurement — nothing measured says 20 is
# right. It is large enough that an ordinary red streak still contains a success
# inside the page, and small enough to stay a single API page. When the page is
# FULL and holds no success the probe reports `checked=False` rather than
# guessing; see `newest_success_from_runs`.
GREEN_PROBE_LIMIT = 20
# The workflow whose runs are the payload of "has CI gone green?". Asking for
# ANY workflow is a container check: `Sponsors Refresh` runs ~every 40 minutes
# and would answer the question by accident. Only this file compiles Rust.
CI_WORKFLOW_FILE = "ci.yml"
GH_RUN_VIEW_TIMEOUT = 10       # seconds per gh run view
GH_RUN_LIST_TIMEOUT = 10       # seconds for gh run list
STUCK_ON_THRESHOLD = 3         # ≥N attempts AND 0 successes → flag
# Hard caps on the final YOUR TRAJECTORY block. The byte cap was raised from
# 2048 → 3072 on Day 142: the epistemic blind-spot section (added Day 141)
# renders LAST and was being decapitated by the old cap — a steering channel
# that usually arrives truncated is a dormant mechanism. 3KB ≈ 750 tokens.
TOTAL_LINE_CAP = 100
TOTAL_BYTE_CAP = 3072

# ── Helpers ──────────────────────────────────────────────────────────────


def warn(msg: str) -> None:
    print(f"extract_trajectory: WARN: {msg}", file=sys.stderr)


def run_cmd(cmd: list[str], timeout: int = 10) -> tuple[int, str, str]:
    """Run a command, capture output. Returns (rc, stdout, stderr). Never raises.
    Uses start_new_session=True so a TimeoutExpired SIGKILLs the entire process
    group (including grandchildren like git/curl spawned by gh), not just the
    immediate child — prevents zombie buildup over many sessions."""
    try:
        r = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            start_new_session=True,
        )
        return r.returncode, r.stdout, r.stderr
    except subprocess.TimeoutExpired as e:
        warn(f"timed out after {timeout}s: {' '.join(cmd[:3])}...")
        # Best-effort kill of the whole process group; subprocess.run already
        # killed the immediate child but grandchildren may persist.
        try:
            if e.pid is not None:
                os.killpg(os.getpgid(e.pid), 9)  # SIGKILL
        except (ProcessLookupError, PermissionError, OSError):
            pass
        return 124, "", "timeout"
    except (FileNotFoundError, OSError) as e:
        warn(f"command failed: {' '.join(cmd[:3])}... — {e}")
        return 1, "", str(e)


def strip_ansi(s: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*[a-zA-Z]", "", s)


def truncate_lines(s: str, n: int) -> str:
    lines = s.splitlines()
    if len(lines) <= n:
        return s
    return "\n".join(lines[:n] + [f"... ({len(lines) - n} more lines truncated)"])


# ── Section 1: Recent session outcomes ───────────────────────────────────


SESSION_DIR_RE = re.compile(r"^day-(\d+)-(.*)$")


def session_sort_key(name: str) -> tuple[int, int, str]:
    """Sort key for a session directory name like `day-N-<timestamp>`.

    Returns (matched, day_number, timestamp) so that sorting descending puts
    the newest session first by *name*, never by mtime — a fresh CI checkout
    gives every file the same checkout-time mtime, so mtime ordering silently
    degrades to git's lexicographic path order where day-99 beats day-139.
    Unparseable names get (0, 0, "") and rank below all parsed ones.
    """
    m = SESSION_DIR_RE.match(name)
    if not m:
        return (0, 0, "")
    return (1, int(m.group(1)), m.group(2))


def load_outcomes(audit_dir: Path) -> list[dict]:
    """Read last N outcome.json files, sorted newest-first by session dir name
    (`day-N-<timestamp>`), falling back to mtime only for dirs that don't match
    the pattern (those rank below all parsed ones).
    Returns dicts unchanged from outcome.json — sort metadata is kept on a
    side tuple, never mutated into the parsed object (defends against keys
    like `_mtime` colliding with future schema additions)."""
    if not audit_dir.exists() or not audit_dir.is_dir():
        return []
    entries: list[tuple[tuple[int, int, str], float, dict]] = []
    for child in audit_dir.iterdir():
        if not child.is_dir():
            continue
        outcome = child / "outcome.json"
        if not outcome.is_file():
            continue
        try:
            data = json.loads(outcome.read_text(errors="replace"))
        except (OSError, json.JSONDecodeError, UnicodeDecodeError) as e:
            warn(f"skipped malformed {outcome}: {e}")
            continue
        try:
            mtime = outcome.stat().st_mtime
        except OSError as e:
            warn(f"could not stat {outcome}: {e}")
            mtime = 0.0
        entries.append((session_sort_key(child.name), mtime, data))
    entries.sort(key=lambda t: (t[0], t[1]), reverse=True)
    # Return only the data dicts, but keep the original keys intact.
    return [t[2] for t in entries[:WINDOW_SESSIONS]]


def render_outcomes(outcomes: list[dict]) -> str:
    if not outcomes:
        return ""
    lines = ["## Recent session outcomes (last {})".format(len(outcomes))]
    for o in outcomes:
        day = o.get("day", "?")
        ts = (o.get("ts") or "").replace("T", " ").rstrip("Z")
        attempted = o.get("tasks_attempted", 0)
        succeeded = o.get("tasks_succeeded", 0)
        build_ok = o.get("build_ok", False)
        test_ok = o.get("test_ok", False)
        reverted = o.get("reverted", False)

        if reverted:
            icon = "❌"
            note = "REVERTED entire session"
        elif attempted == 0:
            icon = "•"
            note = "no tasks attempted"
        elif succeeded == attempted and build_ok and test_ok:
            icon = "✅"
            note = "build OK, tests OK"
        else:
            icon = "⚠️"
            issues = []
            if succeeded < attempted:
                issues.append(f"{attempted - succeeded} task(s) reverted")
            if not build_ok:
                issues.append("build broken")
            if not test_ok:
                issues.append("tests broken")
            note = ", ".join(issues) or "partial"

        lines.append(f"day-{day} ({ts}): tasks {succeeded}/{attempted} {icon} — {note}")
    return "\n".join(lines)


# ── Section 2: Per-task success rate from git log ────────────────────────


# Match commit messages like:
#   "Day 49 (16:24): Wire remaining useful bare subcommands (Task 3)"
#   "Day 57 (14:37): /watch multi-command support — run lint AND test in sequence (Task 2)"
TASK_COMMIT_RE = re.compile(
    r"^Day\s+(\d+)\s+\([^)]+\):\s+(.+?)\s+\(Task\s+\d+\)\s*$"
)
REVERT_COMMIT_RE = re.compile(
    r"^Day\s+\d+\s+\([^)]+\):\s+revert session changes", re.IGNORECASE
)

# Verbatim title the harness writes when Phase A2 produces no task files.
# Source of truth: scripts/evolve.sh (protected — cannot be imported, so this is a copy).
# Guarded against drift by test_fallback_title_matches_evolve_sh in run_self_tests().
FALLBACK_TASK_TITLE = "Self-improvement (small, committed)"


def collect_task_commits() -> tuple[list[tuple[int, str]], int]:
    """Return ([(day, title), ...], revert_commits_in_window)."""
    rc, stdout, _ = run_cmd(
        ["git", "log", f"--since={WINDOW_DAYS} days ago", "--format=%s"],
        timeout=15,
    )
    if rc != 0:
        return [], 0
    tasks = []
    reverts = 0
    for line in stdout.splitlines():
        m = TASK_COMMIT_RE.match(line)
        if m:
            tasks.append((int(m.group(1)), m.group(2).strip()))
            continue
        if REVERT_COMMIT_RE.match(line):
            reverts += 1
    return tasks, reverts


def render_task_success(tasks: list[tuple[int, str]]) -> str:
    if not tasks:
        return ""
    # Group by title; count attempts. Without ground truth on success per-task,
    # we treat the FIRST appearance of a title as 1 attempt; a re-appearance
    # within the window as another attempt. A title that appears with later
    # work on the same area without the agent re-trying it is a likely success.
    # That heuristic is weak — but it's the best we can do from commit messages
    # alone. We surface STUCK only when the threshold is unambiguous.
    title_attempts: defaultdict[str, list[int]] = defaultdict(list)
    for day, title in tasks:
        title_attempts[title].append(day)

    lines = ["## Per-task activity (last {} days)".format(WINDOW_DAYS)]
    stuck_titles = []
    for title, days in sorted(title_attempts.items(), key=lambda kv: -len(kv[1])):
        attempts = len(days)
        if attempts >= STUCK_ON_THRESHOLD:
            stuck_titles.append((title, attempts, days))
        # Cap output at top 5 most-active titles
        if len(lines) > 6:
            continue
        last_day = max(days)
        truncated_title = title[:60] + ("…" if len(title) > 60 else "")
        mark = (
            "  ← planner fallback, no task was chosen"
            if title == FALLBACK_TASK_TITLE
            else ""
        )
        lines.append(
            f"\"{truncated_title}\": {attempts} attempt(s), last day-{last_day}{mark}"
        )

    if stuck_titles:
        lines.append("")
        lines.append("⚠️ Possibly stuck (≥{} attempts in window):".format(STUCK_ON_THRESHOLD))
        for title, attempts, days in stuck_titles[:3]:
            t = title[:60] + ("…" if len(title) > 60 else "")
            lines.append(f"  - \"{t}\": {attempts}× (days {min(days)}-{max(days)})")
    return "\n".join(lines)


def render_unchosen_sessions(tasks: list[tuple[int, str]]) -> str:
    """Name my degraded mode in the durable record.

    When Phase A2 produces no task files the harness writes its own task and
    the impl agent does "something small". That session commits, tests green,
    and in the log is indistinguishable from one I chose — the only tell is
    the contentless title. Say so, so next session's planner does not inherit
    a history that looks deliberate throughout.

    Zero fallbacks renders "" — absence should cost no bytes in the cap budget.
    """
    fallback_days = [day for day, title in tasks if title == FALLBACK_TASK_TITLE]
    if not fallback_days:
        return ""
    return (
        "## Unchosen sessions (planner fallback)\n"
        f"{len(fallback_days)} of the last {len(tasks)} self-driven task commits "
        f"were planner fallbacks (last: day-{max(fallback_days)}).\n"
        "Phase A wrote no task file; the harness picked the work. "
        "No target chosen, no guess recorded."
    )


# ── Section 3: Reverts in window (already counted above) ─────────────────


def _int_or_zero(value: object) -> int:
    """Outcome JSON is written by a shell script — read every number defensively."""
    if isinstance(value, bool):
        return 0
    if isinstance(value, int):
        return value
    return 0


def count_task_reverts(outcomes: list[dict]) -> tuple[int, int]:
    """Return (reverted_task_count, sessions_with_reverts) from loaded outcomes.

    Per-task reverts are the COMMON case and leave no commit: the harness does a
    `git reset --hard` and files an agent-revert issue, so REVERT_COMMIT_RE (which
    only matches the whole-session build-failure commit) can never see them. The
    evidence is already in the outcome dicts as attempted-minus-succeeded.
    """
    reverted_tasks = 0
    sessions = 0
    for o in outcomes:
        attempted = _int_or_zero(o.get("tasks_attempted", 0))
        succeeded = _int_or_zero(o.get("tasks_succeeded", 0))
        lost = max(0, attempted - succeeded)
        reverted_tasks += lost
        # A whole-session revert is a session with reverts too, even if the
        # per-task counters happen to agree (attempted == succeeded).
        if lost > 0 or bool(o.get("reverted", False)):
            sessions += 1
    return reverted_tasks, sessions


def render_reverts(reverts: int, outcomes: list[dict]) -> str:
    """Report the two revert signals as distinct named things — never summed.

    They measure different events: a whole-session revert COMMIT (build failure
    after the session) vs a per-task `git reset --hard` that leaves no commit.
    Summing them would invent a number that counts nothing.
    """
    total_sessions = len(outcomes)
    if total_sessions == 0:
        return ""
    reverted_tasks, revert_sessions = count_task_reverts(outcomes)
    if reverted_tasks == 0 and reverts == 0:
        return (
            "## Reverts in window\n"
            f"0 task reverts in last ~{total_sessions} sessions, "
            f"0 whole-session revert commits in {WINDOW_DAYS} days."
        )
    lines = ["## Reverts in window"]
    if reverted_tasks:
        lines.append(
            f"{reverted_tasks} task(s) reverted across {revert_sessions} of the last "
            f"~{total_sessions} sessions (per-task resets, no commit)."
        )
    else:
        lines.append(f"0 task reverts in last ~{total_sessions} sessions.")
    lines.append(
        f"{reverts} whole-session revert commit(s) in last {WINDOW_DAYS} days."
    )
    return "\n".join(lines)


# --- Subsystem concentration (the monoculture gate) ---------------------------
#
# Day 150 lesson: "A real bug inside the zone I resolved to leave is the perfect
# alibi — audit the topic histogram, not the task's merits." Its prescription was
# concrete: count how many of the last ~N self-driven diffs touched the same
# subsystem; at half or more, the in-zone idea goes to the tracker and the slot
# goes elsewhere. A stopping rule is only load-bearing if something checks it at
# selection time (Day 129/134), and the planner reads exactly one thing at
# selection time — this block. So the gate lives here, as arithmetic with no
# interpretive slack (Day 137–139: judgment-worded rules get renegotiated by the
# impulse they exist to check).
CONCENTRATION_WARN_RATIO = 0.5   # ≥ half the window in one subsystem → warn
CONCENTRATION_MAX_ROWS = 5       # rows rendered before the tail is summarised
CONCENTRATION_MIN_COMMITS = 4    # below this the ratio is noise, not a signal

# Sentinel prefix for the subsystem git log format. A path line can never start
# with it, so commit boundaries are unambiguous — unlike blank-line detection,
# which silently merged every commit in parse_git_log_name_only for months
# (Day 147).
SUBSYSTEM_LOG_MARK = "@@COMMIT@@"

# Crude, auditable, ordered path → subsystem table. First matching prefix wins.
# Only families that span several files (so the file stem alone would scatter
# them) need an entry; everything else is handled by the documented fallback in
# classify_subsystem().
SUBSYSTEM_MAP: tuple[tuple[str, str], ...] = (
    ("src/commands_risk", "risk"),
    ("src/commands_git", "git"),
    ("src/format/", "format"),
    ("src/prompt", "prompt"),
    ("src/cli", "cli"),
    ("src/help", "help"),
    ("src/tool", "tools"),
)

# Explicit third value: a path the table and the fallback both decline to name.
# Never absorbed into a convenient neighbour (Day 144).
SUBSYSTEM_UNKNOWN = "other (unclassified)"


def classify_subsystem(path: str) -> str:
    """Pure path → subsystem label. No subprocess, so self-tests can drive it.

    Rules, in order:
      1. First matching prefix in SUBSYSTEM_MAP.
      2. `src/<stem>.rs` → family key: for `commands_<family>[_<rest>].rs` the
         family token (`commands_fork.rs` → `fork`); otherwise the stem with any
         trailing `_<word>` dropped (`prompt_retry.rs` → `prompt`,
         `repl.rs` → `repl`).
      3. Anything else → SUBSYSTEM_UNKNOWN.
    """
    p = path.strip()
    if not p:
        return SUBSYSTEM_UNKNOWN
    for prefix, label in SUBSYSTEM_MAP:
        if p.startswith(prefix):
            return label
    if not (p.startswith("src/") and p.endswith(".rs")):
        return SUBSYSTEM_UNKNOWN
    stem = p[len("src/"):-len(".rs")]
    if "/" in stem:  # a subdirectory the table doesn't name
        return SUBSYSTEM_UNKNOWN
    parts = stem.split("_")
    if parts[0] == "commands" and len(parts) >= 2:
        return parts[1]
    if len(parts) >= 2:
        return "_".join(parts[:-1])
    return stem


def parse_subsystem_commits(raw: str) -> list[tuple[str, list[str]]]:
    """Pure parser for `git log --name-only --format=<MARK>%s -- src/` output.

    Returns [(subject, [paths...]), ...] for ONLY those commits whose subject
    matches TASK_COMMIT_RE — i.e. self-driven task commits. Routine sweeps
    (social/synthesize sessions touching dozens of files) do not match that
    shape and are dropped here; getting that exclusion right is the whole value
    of the number.
    """
    commits: list[tuple[str, list[str]]] = []
    subject: str | None = None
    paths: list[str] = []

    def flush() -> None:
        nonlocal subject, paths
        if subject is not None and TASK_COMMIT_RE.match(subject) and paths:
            commits.append((subject, paths))
        subject, paths = None, []

    for line in raw.splitlines():
        if line.startswith(SUBSYSTEM_LOG_MARK):
            flush()
            subject = line[len(SUBSYSTEM_LOG_MARK):].strip()
            continue
        stripped = line.strip()
        if not stripped or subject is None:
            continue
        paths.append(stripped)
    flush()
    return commits


def count_subsystems(
    commits: list[tuple[str, list[str]]],
) -> tuple[list[tuple[str, int]], int]:
    """Pure counter. Returns ([(label, commits_touching_it), ...] desc, total).

    A commit counts once per DISTINCT subsystem it touched, so a diff spanning
    two subsystems is evidence for both but cannot inflate either.
    """
    counts: Counter[str] = Counter()
    for _subject, paths in commits:
        for label in {classify_subsystem(p) for p in paths}:
            counts[label] += 1
    ordered = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))
    return ordered, len(commits)


def collect_subsystem_commits() -> str | None:
    """The only subprocess half. Returns raw git output, or None on failure.

    Fail-soft renders nothing (file convention) but warn()s — a fail-soft path
    without a replacement signal is fail-silent (Day 139/145), and warn() output
    is captured to trajectory.stderr.log and surfaced by the cron.
    """
    rc, stdout, stderr = run_cmd(
        [
            "git",
            "log",
            f"--since={WINDOW_DAYS} days ago",
            "--name-only",
            f"--format={SUBSYSTEM_LOG_MARK}%s",
            "--",
            "src/",
        ],
        timeout=15,
    )
    if rc != 0:
        warn(
            "subsystem concentration section skipped: "
            f"`git log --name-only` rc={rc} {stderr.strip()[:120]}"
        )
        return None
    return stdout


def render_subsystem_concentration(
    counts: list[tuple[str, int]], total: int
) -> str:
    """Pure renderer. Short by design — this is steering data, not diagnostics."""
    if total < CONCENTRATION_MIN_COMMITS:
        return ""
    header = f"## Subsystem concentration (last {total} self-driven task commits)"
    lines = [header]
    for label, n in counts[:CONCENTRATION_MAX_ROWS]:
        lines.append(f"{label}: {n}/{total}")
    rest = counts[CONCENTRATION_MAX_ROWS:]
    if rest:
        lines.append(f"(+{len(rest)} other subsystem(s) with fewer)")
    top_label, top_n = counts[0]
    if top_n / total >= CONCENTRATION_WARN_RATIO:
        lines.append(
            f"⚠️ {top_label} took {top_n} of the last {total} self-driven diffs — "
            "send this session's self-driven slot to a different subsystem; "
            "file the in-zone idea instead."
        )
    return "\n".join(lines)


# ── Section 4: Recurring CI errors via gh run view --log-failed ──────────


ERROR_LINE_RE = re.compile(r"(error|panicked|FAILED|fatal)", re.IGNORECASE)

# Passing Rust test lines that may contain error-like words in the test name
# e.g. "test watch::tests::test_watch_result_failed_with_error ... ok"
PASSING_TEST_RE = re.compile(r"test\s+\S+\s+\.\.\.\s+ok", re.IGNORECASE)

# Test result summary line that passed (may mention "0 failed" which matches ERROR_LINE_RE)
# e.g. "test result: ok. 3823 passed; 0 failed;"
# NOTE: "test result: FAILED." IS a real error — only skip "ok." lines
TEST_RESULT_OK_RE = re.compile(r"test result:\s*ok\.", re.IGNORECASE)


def fingerprint_error_line(line: str) -> str:
    """Normalize an error line to a clusterable fingerprint."""
    s = strip_ansi(line).strip()
    # Strip GitHub Actions log prefix: <word> <word> ... <timestamp>
    # e.g. "social unknown step 2026-04-15T15:31:42.5342991Z error: auth"
    # The timestamp has format YYYY-MM-DDTHH:MM:SS[.fraction]Z
    s = re.sub(
        r"^(?:[A-Za-z_][\w-]*\s+)*"              # zero or more word prefixes
        r"\d{4}-\d{2}-\d{2}T[\d:.]+Z?\s*",        # ISO timestamp with subseconds
        "", s
    )
    # Strip leading log timestamps (standalone, at start of line)
    s = re.sub(r"^\d{4}-\d{2}-\d{2}T?[\d:.,Z+ ]*\s*", "", s)
    # Strip CI step prefixes like "build |" or "test │"
    s = re.sub(r"^[A-Za-z_-]+\s*[\|│]\s*", "", s)
    # Normalize file:line:column to file:N:N
    s = re.sub(r":\d+:\d+", ":N:N", s)
    s = re.sub(r":\d+\b", ":N", s)
    # Normalize hex addresses (0x7fff1234abcd) and UUIDs
    s = re.sub(r"0x[0-9a-fA-F]{4,}", "<HEX>", s)
    s = re.sub(
        r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
        "<UUID>", s
    )
    # Lowercase, collapse whitespace, truncate to 80 chars
    return re.sub(r"\s+", " ", s.lower())[:80]


def run_age_days(created_at, now: datetime):
    """Age of an ISO-8601 timestamp in days. Pure.

    Returns None for a missing / non-string / unparseable stamp — deliberately
    NOT 0.0. "I don't know when this ran" is a different fact from "it ran just
    now", and folding the first into the second is exactly how resolved history
    gets rendered as current state."""
    if not isinstance(created_at, str):
        return None
    s = created_at.strip()
    if not s:
        return None
    if s.endswith("Z"):
        s = s[:-1] + "+00:00"
    try:
        dt = datetime.fromisoformat(s)
    except ValueError:
        return None
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return (now - dt).total_seconds() / 86400.0


def partition_failed_runs(runs, now: datetime, window_days: int = WINDOW_DAYS):
    """Split `gh run list` records into three states. Pure, no I/O.

    Returns (in_window, too_old, undated), each a list of (run, age_days) pairs
    (age is None only in `undated`). Three states, none folded into another: an
    undated run is neither silently kept nor silently dropped — it is counted
    and reported, because a shrinking denominator inside my own meter is the
    defect I keep fixing elsewhere.

    The window boundary is INCLUSIVE: a run exactly `window_days` old is still
    in window."""
    in_window: list = []
    too_old: list = []
    undated: list = []
    for run in runs or []:
        record = run if isinstance(run, dict) else {}
        age = run_age_days(record.get("createdAt"), now)
        if age is None:
            undated.append((record, None))
        elif age <= window_days:
            in_window.append((record, age))
        else:
            too_old.append((record, age))
    return in_window, too_old, undated


def format_run_age(age) -> str:
    """Human age label for a cluster's newest run. Pure."""
    if age is None:
        return "undated"
    if age < 1:
        return "last <1d ago"
    return f"last {int(age)}d ago"


@dataclass
class CiScan:
    """What the CI-error probe actually observed — not just its clusters.

    `ok` distinguishes "checked" from "could not check": an empty cluster list
    is reachable from three different worlds (no failures, gh query failed,
    repo unset) and "could not check" must never read as "checked; clean"."""

    ok: bool
    clusters: list = field(default_factory=list)  # (fingerprint, run_ids, newest_age_days)
    in_window: int = 0
    excluded_old: int = 0
    undated: int = 0
    fetch_errors: int = 0
    reason: str = ""
    # ISO-8601 stamp of the NEWEST in-window failure, or None when there are
    # none. This is the anchor `green_since_verdict` compares a later success
    # against. Deliberately the newest failure OVERALL rather than the newest
    # one that produced a rendered cluster: every rendered row comes from an
    # in-window failure, so "every failure below predates it" stays true, and
    # an unreadable-log run can only make the green-since claim HARDER to
    # earn — the conservative direction for a line whose whole job is to stop
    # over-claiming.
    newest_failure_ts: str | None = None


@dataclass
class GreenScan:
    """Has CI passed since the newest failure? Three states, none folded.

    `checked` separates "the query ran and answered" from "I could not ask".
    A `checked=False` scan must never render as "no successes" and must never
    render as green: collapsing could-not-check into either is precisely the
    defect this whole probe exists to prevent ("was red" must not read as "is
    red", and its mirror, "could not check" must not read as "checked; clean").

    `rows` / `successes` / `unreadable` are OBSERVATION, not decision: nothing
    reads them to choose a branch, they exist so `green_probe_receipt` can
    record what the page actually held. `rows is None` means no page was ever
    inspected (the payload was not a list, or the caller never asked) and is
    deliberately kept apart from `rows == 0`, which is a real empty listing.

    `newest_row_ts` is the one exception and the Day-180 addition: it is the
    newest `createdAt` on the page REGARDLESS of conclusion, and it is the
    freshness evidence. `--status completed` includes failures, so the newest
    failure the same session already fetched MUST appear on a fresh page — if
    the page's newest row predates it, the listing cannot be current and no
    verdict is available. See `page_is_stale`.
    """

    newest_success_ts: str | None = None
    checked: bool = False
    rows: int | None = None
    successes: int = 0
    unreadable: int = 0
    newest_row_ts: str | None = None


# Branch names for the green-since decision. These are the receipt's payload.
# The Day-179 21:30 render preserved the SENTENCE and not which branch produced
# it, so the single fact needed to grade the verdict afterwards was the one fact
# nothing recorded. Named constants rather than literals so the decision, the
# rendered sentence and the audit line cannot drift into three vocabularies.
GREEN_BRANCH_NO_FAILURES = "no-in-window-failures"
GREEN_BRANCH_COULD_NOT_CHECK = "could-not-check"
GREEN_BRANCH_GONE_GREEN = "gone-green"
GREEN_BRANCH_STILL_LIVE = "still-live"
GREEN_BRANCH_STALE_PAGE = "stale-page"


def page_is_stale(newest_row_ts, newest_failure_ts) -> bool:
    """Can this completed-runs page possibly be current? Pure.

    A self-consistency check that costs zero extra API calls, because both
    numbers are already in hand: `green_probe_argv` and `failed_runs_argv` name
    the SAME workflow, and a failed run IS a completed run. So on a fresh
    `--status completed` page the newest failure this same session already
    fetched must appear on it — the page's newest row can never be older.

    Measured Day 180, same argv three minutes apart: the harness got a full page
    of 20 completed `ci.yml` runs whose newest row was 2026-08-25T23:54:38Z,
    four hours OLDER than a failure (2026-08-26T03:49:23Z) the same run had just
    fetched. Arithmetically impossible for a fresh page. A hand-run at 07:22Z saw
    2026-08-27T03:23:45Z and said gone-green. `max()` over a stale page is still
    stale: the staleness is in the PAGE, not in the ordering within it.

    Either side missing or unparseable -> False. An unknown is not a defect, and
    inventing one here would trade a false "red" for a false "cannot check"."""
    if not newest_row_ts or not newest_failure_ts:
        return False
    # Ages against ONE shared instant, so the boolean is a property of the two
    # stamps alone and `now` cancels out. `run_age_days` is reused rather than a
    # second ISO-8601 parser written; older reads as a LARGER age.
    now = datetime.now(timezone.utc)
    row_age = run_age_days(newest_row_ts, now)
    fail_age = run_age_days(newest_failure_ts, now)
    if row_age is None or fail_age is None:
        return False
    # STRICT: a page whose newest row is exactly the newest failure is fresh
    # enough to have seen it. Ties go to "not stale", the direction that leaves
    # today's behaviour unchanged.
    return row_age > fail_age


def green_probe_argv(repo: str) -> list[str]:
    """argv for the green-since probe. Pure, so a test can assert its shape.

    `--status completed`, NOT `--status success`: the server-side success filter
    intermittently serves a stale page (see GREEN_PROBE_LIMIT for the measured
    reading), so the success filter lives client-side in
    `newest_success_from_runs` and `conclusion` is fetched to do it. The
    `--workflow` filter is load-bearing and must not be dropped — without it a
    `Sponsors Refresh` run answers "has CI gone green?" by accident."""
    return [
        "gh", "run", "list", "--repo", repo,
        "--workflow", CI_WORKFLOW_FILE,
        "--status", "completed", "--limit", str(GREEN_PROBE_LIMIT),
        "--json", "createdAt,workflowName,conclusion",
    ]


def failed_runs_argv(repo: str) -> list[str]:
    """argv for the failed-run scan. Pure, so a test can assert its shape."""
    return [
        "gh", "run", "list", "--repo", repo,
        "--workflow", CI_WORKFLOW_FILE,
        "--status", "failure", "--limit", str(MAX_FAILED_RUNS),
        "--json", "databaseId,createdAt,name,workflowName",
    ]


def newest_success_from_runs(runs, limit: int) -> GreenScan:
    """Newest successful run from an already-parsed `gh run list` payload. Pure.

    The payload comes from `--status completed`, so this filters to
    `conclusion == "success"` itself, then takes the MAX over every parseable
    `createdAt` rather than trusting index 0 — see GREEN_PROBE_LIMIT for both
    measured reasons. Rows with a missing or unparseable stamp are SKIPPED: an
    unreadable row is neither the newest nor evidence that no success exists.

    `limit` is the page size the caller asked `gh` for, and it is what separates
    the two ways a page can hold no success. Folding them would make a long red
    streak render as "CI has never been green" — a STRONGER false alarm than the
    stale page this probe was fixed for. "Could not see far enough" must never
    read as "there is nothing there".

    States, none folded into another:
      * not a list                          -> checked=False  (cannot ask)
      * ≥1 parseable success row            -> ts=max, checked=True
      * no success, but ≥1 UNREADABLE row   -> checked=False  (NOT "no successes")
      * no success, page FULL  (len>=limit) -> checked=False  (cannot see far enough)
      * no success, page SHORT (len<limit)  -> ts=None, checked=True (real answer;
                                               the whole listing was seen, and an
                                               empty list is its shortest case)
    """
    if not isinstance(runs, list):
        return GreenScan(checked=False)
    best_ts: str | None = None
    best_age: float | None = None
    newest_row_ts: str | None = None
    newest_row_age: float | None = None
    unreadable = 0
    successes = 0
    # One shared `now` so every row is measured against the same instant, and
    # `run_age_days` is reused rather than a second ISO-8601 parser written —
    # smallest age == newest, exactly as the failure side already does it.
    now = datetime.now(timezone.utc)
    for record in runs:
        if not isinstance(record, dict):
            unreadable += 1
            continue
        # Freshness evidence is collected from EVERY parseable row, before the
        # success filter and before the conclusion is even read: `--status
        # completed` includes failures, and a failure is exactly the row whose
        # absence proves the page is stale. See `page_is_stale`.
        row_age = run_age_days(record.get("createdAt"), now)
        if row_age is not None and (newest_row_age is None or row_age < newest_row_age):
            newest_row_age, newest_row_ts = row_age, record["createdAt"]
        conclusion = record.get("conclusion")
        if not isinstance(conclusion, str):
            # A row that cannot be classified is not a non-success. Calling it
            # one would be "could not check" reading as "checked; clean".
            unreadable += 1
            continue
        if conclusion != "success":
            continue
        successes += 1
        age = row_age
        if age is None:
            unreadable += 1
            continue
        if best_age is None or age < best_age:
            best_age, best_ts = age, record["createdAt"]
    seen = GreenScan(
        rows=len(runs),
        successes=successes,
        unreadable=unreadable,
        newest_row_ts=newest_row_ts,
    )
    if best_ts is not None:
        seen.newest_success_ts, seen.checked = best_ts, True
        return seen
    if unreadable:
        # Rows came back but the ones that mattered were not readable. "Could
        # not check" must not read as "checked; no successes".
        return seen
    if len(runs) >= limit:
        # A full page of non-successes: the newest success may simply be one row
        # past the horizon. That is an unknown, not an absence.
        return seen
    # Short page: the entire listing was seen and it holds no success. A real
    # observation, NOT a failure to check.
    seen.checked = True
    return seen


def newest_successful_run(repo: str) -> GreenScan:
    """The most recent SUCCESSFUL CI run, via one `gh run list` call. I/O half.

    Fail-soft by contract: any missing repo, non-zero exit, unparseable JSON,
    or rows with no readable `createdAt` yields `checked=False` — never an
    exception, never a fabricated timestamp. The extractor must never block a
    session.

    Callers must gate this on there actually being an in-window failure to
    qualify, so a healthy session makes zero extra `gh` calls."""
    if not repo:
        return GreenScan(checked=False)
    rc, stdout, _stderr = run_cmd(green_probe_argv(repo), timeout=GH_RUN_LIST_TIMEOUT)
    if rc != 0:
        warn(f"gh run list --status completed rc={rc} — green-since check unavailable")
        return GreenScan(checked=False)
    try:
        runs = json.loads(stdout)
    except json.JSONDecodeError as e:
        warn(f"gh run list --status completed returned non-JSON: {e}")
        return GreenScan(checked=False)
    return newest_success_from_runs(runs, GREEN_PROBE_LIMIT)


def green_verdict_branch(newest_failure_ts, green: "GreenScan | None",
                         now: datetime) -> str:
    """Which branch of the green-since decision fires. Pure.

    THE decision, stated once. `green_since_verdict` maps this to a sentence and
    `green_probe_receipt` names it verbatim, so the line the planner reads and
    the line the audit stream keeps can never disagree about what happened. A
    second copy of these conditions would be exactly the drift that makes a
    receipt worthless."""
    if not newest_failure_ts:
        return GREEN_BRANCH_NO_FAILURES
    if green is None or not green.checked:
        return GREEN_BRANCH_COULD_NOT_CHECK
    fail_age = run_age_days(newest_failure_ts, now)
    success_age = (
        run_age_days(green.newest_success_ts, now) if green.newest_success_ts else None
    )
    if fail_age is None or (green.newest_success_ts and success_age is None):
        # An unparseable stamp on either side is an unknown, and an unknown
        # must not be promoted into either confident answer.
        return GREEN_BRANCH_COULD_NOT_CHECK
    # STRICT: a success stamped exactly at the newest failure is NOT green-since.
    # Ties go to "still live", the conservative direction. Written on ages, so
    # strictly-newer reads as strictly-SMALLER here.
    if success_age is not None and success_age < fail_age:
        return GREEN_BRANCH_GONE_GREEN
    # Staleness is consulted HERE and nowhere else, and the position is the
    # whole rule. It must not pre-empt gone-green: if a success on the page is
    # already newer than the newest failure, the answer is known and the page's
    # freshness is moot. It must come before still-live: a page that cannot be
    # current is no evidence that CI is red, and asserting red from it is the
    # mirror of my own rule — "could not check" must never read as "checked;
    # clean", and equally must never read as "confirmed red".
    if page_is_stale(green.newest_row_ts, newest_failure_ts):
        return GREEN_BRANCH_STALE_PAGE
    return GREEN_BRANCH_STILL_LIVE


GREEN_COULD_NOT_RUN_SENTENCE = (
    "green-since check could not run — this claims neither that the "
    "failures below are live nor that they are cured"
)


def green_since_verdict(newest_failure_ts, green: "GreenScan | None", now: datetime):
    """One sentence: has CI passed since the newest failure below? Pure.

    A thin map from `green_verdict_branch` to prose — the decision lives there
    and nowhere else. Returns None ONLY on the no-in-window-failure branch, the
    world that already prints its own honest line and stays byte-identical."""
    branch = green_verdict_branch(newest_failure_ts, green, now)
    if branch == GREEN_BRANCH_NO_FAILURES:
        return None
    if branch == GREEN_BRANCH_COULD_NOT_CHECK:
        return GREEN_COULD_NOT_RUN_SENTENCE
    if branch == GREEN_BRANCH_GONE_GREEN:
        success_age = run_age_days(green.newest_success_ts, now)
        return (
            f"CI has gone green since ({format_run_age(success_age)}): every failure "
            "below predates it. Not proof the causes are fixed — a flaky test passes "
            "sometimes — only that CI is not red on these patterns now."
        )
    if branch == GREEN_BRANCH_STALE_PAGE:
        return (
            "green-since check could not run — the completed-runs listing's newest "
            f"row ({green.newest_row_ts}) predates a failure this same run already "
            f"fetched ({newest_failure_ts}), so the page cannot be current and CI's "
            "state could not be determined. This claims neither that the failures "
            "below are live nor that they are cured."
        )
    return (
        "no successful run has landed since the newest failure below — these are live"
    )


def green_probe_receipt(scan: "CiScan | None", green: "GreenScan | None",
                        branch: str, limit: int) -> str:
    """One line recording what the green-since probe actually SAW. Pure.

    This is a grader, not a fifth correctness fix. The probe has been declared
    fixed four times (Day 178: workflow-blindness, then max() over rows; Day 179:
    --status completed, then the short/full page split), and every one of those
    was verified by hand-running it in the session that wrote it — which verifies
    the CHANNEL, not the consumer. Nothing in the loop has ever graded a live
    answer. On Day 179 at 21:30 the block rendered "these are live" over cured
    failures while a hand-run three minutes later said green, and WHICH BRANCH
    fired is unrecoverable because the probe recorded nothing.

    So it names, verbatim, every input the verdict was computed from:
      * branch      — the one fact the 21:30 render did not preserve
      * green_rows  — page size returned ("not-asked" when the cost guard skipped
                      the call, "unknown" when the payload was not a list)
      * successes / unreadable / page=short|full — short means the listing was
                      exhausted, full means the probe could not see far enough;
                      folding those two is the strongest false alarm available
      * newest_success / newest_failure — the two stamps that were compared
      * newest_row    — the newest row of ANY conclusion, which is the freshness
                        evidence `page_is_stale` reads. Day 180: the receipt
                        earned its keep by showing a page whose newest row was
                        four hours older than a failure the same run had already
                        fetched, so it must carry that input too
      * in_window / too_old / undated   — the failure side's own partition
    """
    if green is None:
        rows = "not-asked"
        page = "n/a"
        successes = "n/a"
        unreadable = "n/a"
        success_ts = "not-asked"
        checked = "n/a"
        row_ts = "not-asked"
    else:
        rows = "unknown" if green.rows is None else str(green.rows)
        # `page` answers the short/full question from the same `limit` the query
        # asked for, so the receipt cannot claim a horizon the probe never had.
        page = "n/a" if green.rows is None else ("full" if green.rows >= limit else "short")
        successes = str(green.successes)
        unreadable = str(green.unreadable)
        success_ts = green.newest_success_ts or "none"
        checked = "true" if green.checked else "false"
        # The freshness input the stale-page decision reads. The receipt is the
        # grader, so it must carry every input the verdict was computed from.
        row_ts = green.newest_row_ts or "none"
    if scan is None:
        fail_ts, in_window, too_old, undated = "n/a", "n/a", "n/a", "n/a"
    else:
        fail_ts = scan.newest_failure_ts or "none"
        in_window, too_old, undated = (
            str(scan.in_window), str(scan.excluded_old), str(scan.undated)
        )
    return (
        f"extract_trajectory: green-probe: branch={branch} "
        f"green_rows={rows} page={page} limit={limit} successes={successes} "
        f"unreadable={unreadable} checked={checked} newest_success={success_ts} "
        f"newest_row={row_ts} newest_failure={fail_ts} "
        f"failures_in_window={in_window} too_old={too_old} undated={undated}"
    )


def collect_failed_ci_fingerprints(repo: str, now: datetime | None = None) -> CiScan:
    """Fingerprint recent FAILED CI runs, but only those inside WINDOW_DAYS.

    `gh run list --status failure --limit N` returns the N most recent failures
    *ever*, with no date filter — so a repo that has been green for months still
    yields rows under a header claiming they are "in window". The age filter runs
    BEFORE the expensive `gh run view --log-failed` loop, so it also saves calls.

    It also has no WORKFLOW filter by default, which the header "Recurring CI
    errors" silently mis-states: measured 2026-08-25 over the 20 most recent
    failures on this repo, 11 were Dream / Sponsors Refresh / Skill Evolution /
    Deploy Pages / Social / Evolution runs, none of which compile Rust. The argv
    (`failed_runs_argv`) pins the scan to CI_WORKFLOW_FILE so the header is true.

    `now` is injectable so the self-tests are deterministic."""
    now = now or datetime.now(timezone.utc)
    if not repo:
        warn("YOYO_REPO empty — cannot check recent CI failures")
        return CiScan(ok=False, reason="YOYO_REPO unset")
    rc, stdout, stderr = run_cmd(failed_runs_argv(repo), timeout=GH_RUN_LIST_TIMEOUT)
    if rc != 0:
        warn(f"gh run list rc={rc}: {(stderr or '').strip()[:200]}")
        return CiScan(ok=False, reason=f"gh run list failed (rc={rc})")
    try:
        runs = json.loads(stdout)
    except json.JSONDecodeError as e:
        warn(f"gh run list returned non-JSON: {e}")
        return CiScan(ok=False, reason="gh run list returned non-JSON")

    in_window, too_old, undated = partition_failed_runs(runs, now)
    scan = CiScan(
        ok=True,
        in_window=len(in_window),
        excluded_old=len(too_old),
        undated=len(undated),
    )
    if in_window:
        # Smallest age == newest run. Read the stamp back off the record so the
        # anchor is the timestamp gh reported, never one reconstructed from a
        # float age.
        newest_record = min(in_window, key=lambda pair: pair[1])[0]
        ts = newest_record.get("createdAt")
        scan.newest_failure_ts = ts if isinstance(ts, str) and ts.strip() else None
    if not in_window:
        return scan

    fingerprints: defaultdict[str, list[str]] = defaultdict(list)
    newest: dict[str, float] = {}
    for run, age in in_window:
        run_id = str(run.get("databaseId") or "")
        if not run_id:
            continue
        rc2, log_stdout, stderr2 = run_cmd(
            ["gh", "run", "view", run_id, "--repo", repo, "--log-failed"],
            timeout=GH_RUN_VIEW_TIMEOUT,
        )
        if rc2 != 0:
            scan.fetch_errors += 1
            warn(f"gh run view {run_id} rc={rc2}: {(stderr2 or '').strip()[:120]}")
            continue
        tail = log_stdout.splitlines()[-50:]
        seen_in_run = set()
        for ln in tail:
            if ERROR_LINE_RE.search(ln):
                stripped = strip_ansi(ln)
                # Skip passing test lines (e.g. test names containing "error")
                if PASSING_TEST_RE.search(stripped):
                    continue
                # Skip passing test result summaries (e.g. "test result: ok. 3823 passed; 0 failed;")
                if TEST_RESULT_OK_RE.search(stripped):
                    continue
                fp = fingerprint_error_line(ln)
                if fp and fp not in seen_in_run:
                    fingerprints[fp].append(run_id)
                    seen_in_run.add(fp)
                    if fp not in newest or age < newest[fp]:
                        newest[fp] = age
    if scan.fetch_errors and not fingerprints:
        warn(f"all {scan.fetch_errors} gh run view fetch(es) failed — no fingerprints")
    scan.clusters = sorted(
        ((fp, ids, newest.get(fp)) for fp, ids in fingerprints.items()),
        key=lambda c: -len(c[1]),
    )
    return scan


def render_ci_errors(scan: CiScan | None, green: "GreenScan | None" = None,
                     now: datetime | None = None) -> str:
    """Render the CI section. Every world gets its own sentence.

    Rows carry the age of their newest run, and the header names the window, so
    a four-day-old resolved failure cannot read as "CI is failing right now".

    `green` adds ONE line under the header answering the cheap, checkable
    question the age filter cannot: has any run SUCCEEDED since the newest
    failure below? Recent, real and already repaired was a fourth state this
    partition did not have, and it failed in the alarming direction — priority
    0 of the planning prompt is "fix CI failures", so a cured defect rendered
    as live can outrank the whole rest of the plan.

    Row shape, header text and every other branch are unchanged: nothing is
    dropped, filtered or re-ranked. This adds one sentence of context."""
    if scan is None:
        return ""
    if not scan.ok:
        reason = f" ({scan.reason})" if scan.reason else ""
        return (
            f"## CI: could not check recent failures{reason} "
            "— this is NOT a clean bill of health"
        )
    notes = []
    if scan.excluded_old:
        notes.append(f"{scan.excluded_old} older failure(s) outside the window, not shown")
    if scan.undated:
        notes.append(f"{scan.undated} undated run(s) excluded")
    note = f" ({'; '.join(notes)})" if notes else ""
    if not scan.clusters:
        if scan.in_window:
            return (
                f"## CI: {scan.in_window} failed run(s) in last {WINDOW_DAYS} days, "
                f"but no error lines could be read from their logs{note}"
            )
        return f"## CI: no failed runs in last {WINDOW_DAYS} days{note}"
    lines = [f"## Recurring CI errors (failed runs, last {WINDOW_DAYS} days){note}"]
    verdict = green_since_verdict(
        scan.newest_failure_ts, green, now or datetime.now(timezone.utc)
    )
    if verdict:
        lines.append(verdict)
    for fp, run_ids, age in scan.clusters[:5]:
        n = len(run_ids)
        # Truncate fingerprint to keep line tidy
        fp_short = fp[:90]
        lines.append(f"[{n}×, {format_run_age(age)}] {fp_short}")
    return "\n".join(lines)


# ── Section 5: Provider/API health from audit.jsonl files ────────────────


AUDIT_FILE_SIZE_CAP = 10 * 1024 * 1024  # 10MB per file — guard against runaway audit.jsonl


# --- Provider-error detection (Day 196) -----------------------------------
#
# The measured defect: Day 195 ran 5 sessions and produced ZERO task commits,
# Day 194 added 2 more; every one filed `tasks N/N ✅` with a `success`
# workflow conclusion, and this very section rendered
# `10 sessions, no provider errors detected.` into the planner's briefing
# while `scripts/measure_abstentions.py` read the same world and reported
# `[EXCLUDED_PROVIDER_ERROR: 7 line(s)]` on 7 of 8 logs. Two of my own
# instruments disagreed and the one in my prompt said "healthy".
#
# Two independent causes, both real:
#   1. WRONG STREAM. The scan read `audit.jsonl` only. The rate-limit text is
#      emitted by the yoyo binary to its own stderr, which `evolve.sh` tees
#      into `transcripts/*.log` — the same stream mismatch
#      `measure_abstentions.py` was repaired for on Day 175.
#   2. WRONG REGEX. The old `PROVIDER_ERROR_RE` matched `rate_limit` with an
#      UNDERSCORE; the binary prints `Rate limited` — space, past tense. It
#      could not match the string on any stream.
# Fixing either alone is a non-fix: (2) without (1) has nothing to read, and
# (1) without (2) reads the right file with a matcher that cannot fire.
#
# THE LANDMINE, and why the prose filter below is load-bearing rather than
# ceremony: my transcripts, journals, CLAUDE.md and session write-ups contain
# the literal string `Rate limited` constantly — the write-up for THIS change
# contains all three shapes verbatim. A bare substring scan reports my own
# writing as provider evidence, and the contamination grows monotonically
# every session. That is exactly the defect that forced
# `measure_abstentions.py` to be rebuilt, so the two rules it proved out are
# copied rather than re-invented: anchor to the whole emitted line, and run
# every candidate through a prose filter whose rejects are COUNTED.
#
# These are a deliberate second copy of that file's `is_quoted_prose` /
# `classify_line` pair, not an import — the same reasoning written at
# `counterfactual_green.py`'s second copy of `TASK_COMMIT_RE`: two hand-run
# scripts with different lifecycles, where sharing costs a cross-import for
# one predicate. The shapes below were captured VERBATIM from the real
# transcripts (day-194/195/196), never hand-typed: 43 × the retry-after line,
# 43 × the no-fallback line, 15 × the stopped-retrying line.
# TWO-STAGE, and the ORDER is the whole correctness of it (Day 196, eval-fix).
# The first landing anchored every pattern to `^...$` and then consulted the
# prose filter. That made the counted-reject STRUCTURALLY DEAD: a markdown
# table cell or a backticked quote fails `^\s*error:` before the filter is
# ever reached, so `prose_rejected` could never be non-zero and the two guards
# asserting it were red. A guard that cannot fire is the "cannot fail loudly"
# defect wearing the opposite sign, and it is quieter than the bug.
#
# So the CANDIDATE stage is deliberately LOOSE — the bare substring scan the
# task warns is contaminated by my own prose — and the prose filter runs over
# exactly that population. Only a candidate that survives the filter is then
# held to the ANCHOR. Loose-then-filter-then-anchor is what makes the reject
# counter reachable, and it is the only ordering under which the filter is
# load-bearing rather than ceremony.
#
# `anchor` is None for the three legacy token alternatives: they were always
# substring matches, so the loose match IS the rule there and narrowing them
# would be a narrowing nobody asked for.
_PROVIDER_MARKER_PATTERNS = [
    # `  error: Rate limited, retry after Some(84004000)ms`
    # Leading whitespace is REQUIRED to be tolerated in the anchor: the binary's
    # stderr is indented 2 spaces in the teed transcript, so an anchor without
    # \s* fires on nothing at all. Measured, not assumed.
    (
        "rate_limited",
        re.compile(r"error:\s*Rate limited,\s*retry after\s+Some\(\d+\)ms"),
        re.compile(r"^\s*error:\s*Rate limited,\s*retry after\s+Some\(\d+\)ms\s*$"),
    ),
    # `  ⏳ stopped retrying on purpose: the provider says its rate limit resets in ~23h 20m. …`
    (
        "retry_giveup",
        re.compile(r"stopped retrying on purpose:\s*the provider says its rate limit resets in"),
        re.compile(r"^\s*\u23f3?\s*stopped retrying on purpose:\s*the provider says its rate limit resets in\s*~\S+.*$"),
    ),
    # `  API error with no fallback configured. Exiting.`
    (
        "no_fallback",
        re.compile(r"API error with no fallback configured\.\s*Exiting\."),
        re.compile(r"^\s*API error with no fallback configured\.\s*Exiting\.\s*$"),
    ),
    # The three pre-existing alternatives are KEPT verbatim. They may match
    # genuine structured audit.jsonl lines, and dropping them would be a
    # narrowing nobody asked for — this task widens the scan, it does not
    # trade one blind spot for another.
    ("audit_error_object", re.compile(r'"type"\s*:\s*"error"', re.IGNORECASE), None),
    ("provider_error_token", re.compile(r"provider_error", re.IGNORECASE), None),
    ("rate_limit_token", re.compile(r"rate_limit", re.IGNORECASE), None),
]

_PROVIDER_GREP_PREFIX_RE = re.compile(r"^\s*(?:[\w./-]+:)?\d+:")
_PROVIDER_BLOCKQUOTE_RE = re.compile(r"^\s*>")


def is_provider_prose(line: str) -> bool:
    """True when the line carries the tells of MY writing, not the harness's stdout.

    Pure. The four tells are the ones `measure_abstentions.py` proved out: a
    markdown pipe (a table cell), a backtick (an inline quote), a `NNN:` or
    `path:NNN:` grep prefix, and a leading `>` blockquote. Verified against the
    real emissions before shipping: none of the three live shapes contains a
    pipe or a backtick, so this filter costs no true positives today.
    """
    line = strip_ansi(line).rstrip("\n")
    if "|" in line:
        return True
    if "`" in line:
        return True
    if _PROVIDER_BLOCKQUOTE_RE.match(line):
        return True
    if _PROVIDER_GREP_PREFIX_RE.match(line):
        return True
    return False


def classify_provider_error_line(line: str) -> str | None:
    """LOOSE candidate match: does this line carry a provider-emission's text?

    Pure. Returns a marker name or None, and says NOTHING about provenance or
    about shape — `is_provider_prose` owns the first and
    `provider_line_is_emission` the second. This is deliberately the bare
    substring scan the task warns is contaminated by my own writing, because a
    filter can only be load-bearing over a population wider than the thing it
    filters down to.

    Order is preserved and load-bearing: `rate_limited` is tried before the
    legacy `rate_limit` token, so the real emission reports its own marker
    rather than the catch-all's.
    """
    line = strip_ansi(line).rstrip("\n").rstrip()
    for marker, candidate, _anchor in _PROVIDER_MARKER_PATTERNS:
        if candidate.search(line):
            return marker
    return None


def provider_line_is_emission(marker: str, line: str) -> bool:
    """ANCHORED match: is this line the shape the harness actually emits?

    Pure. A candidate that carries the text but not the shape is text ABOUT an
    emission, not one. `anchor is None` means the loose match is the whole rule
    (the three legacy token alternatives), so those are emissions by
    definition and this returns True.
    """
    line = strip_ansi(line).rstrip("\n").rstrip()
    for name, _candidate, anchor in _PROVIDER_MARKER_PATTERNS:
        if name != marker:
            continue
        return anchor is None or bool(anchor.search(line))
    return False


# The two reject reasons are DISTINCT VALUES and are never folded into each
# other: "my own prose quoting the emission" and "carries the text in a shape
# the harness never emits" are different facts with different remedies, and a
# counter wearing the other's name is the mislabelling this section is about.
PROVIDER_REJECT_PROSE = "prose"
PROVIDER_REJECT_UNANCHORED = "unanchored"


def classify_provider_error(line: str) -> tuple[str | None, str | None]:
    """Return (marker, reject_reason) — the composition, in a fixed order.

    Candidate -> prose filter -> anchor. Every candidate that is declined is
    declined WITH A REASON that the caller counts; nothing is dropped
    silently, because a shrinking denominator inside my own meter is the
    defect this whole section exists to avoid.

    The prose filter runs BEFORE the anchor on purpose. A quoted line can be
    both prose-shaped and unanchored, and "I wrote this" is the more specific
    and more actionable of the two.
    """
    marker = classify_provider_error_line(line)
    if marker is None:
        return (None, None)
    if is_provider_prose(line):
        return (None, PROVIDER_REJECT_PROSE)
    if not provider_line_is_emission(marker, line):
        return (None, PROVIDER_REJECT_UNANCHORED)
    return (marker, None)


@dataclass
class ProviderScan:
    """What the provider-health walk actually observed.

    `streams` names the files that were READ, so the rendered claim can never
    be read wider than its evidence — the clean branch used to say "no
    provider errors detected", which reads as *the provider was fine*, when
    what it supports is *no provider-error lines in the files I opened*.
    """

    sessions: int = 0
    hits: int = 0
    prose_rejected: int = 0
    streams: tuple = ()
    # Counted separately from `prose_rejected`, never summed into it: a line
    # carrying the emission's text in a shape the harness never emits is a
    # different fact from a line I wrote quoting one.
    unanchored_rejected: int = 0
    # Day 196 (second task): files the scan could NOT read WHOLE — an OSError,
    # or a file past AUDIT_FILE_SIZE_CAP whose tail was never scanned. Counted
    # and named, NEVER summed into `hits` or into either reject count: three
    # different facts with three different remedies. A rejected line was READ
    # and judged; an unread stream was never judged at all, and reporting the
    # second as silence is "could not check" reading as "checked; clean" —
    # the refusal the pre-push hook and `CiScan`'s could-not-run branch make.
    # The direction matters: a terminal rate limit lands at the END of a
    # transcript, which is exactly the half a head-bounded read drops.
    unread_streams: int = 0


# Three states for the audit-log directory, and none of them may be folded into
# a neighbour (#843). `main` used to substitute `Path("/dev/null")` for an unset
# `YOYO_AUDIT_DIR` — a placeholder that RENDERS PLAUSIBLY: it is a real path, so
# nothing upstream could see the value was missing, and the failure surfaced four
# frames away as `NotADirectoryError` from `iterdir()` rather than as "the
# variable is unset". `evolve.sh` always sets the var, so this only ever bit
# hand-runs, i.e. exactly how this script gets debugged.
#
# UNSET and UNUSABLE are DIFFERENT FACTS: "nobody told me where to look" is not
# "I was told, and the place is not there". Collapsing the second into silence is
# the reading this file has already been corrected twice for on the green probe —
# "could not check" must never read as "checked; clean".
AUDIT_DIR_UNSET = "unset"
AUDIT_DIR_UNUSABLE = "unusable"
AUDIT_DIR_OK = "ok"


def classify_audit_dir(raw, exists, is_dir) -> str:
    """Pure: which of the three audit-dir states does this env value name?

    `raw` is the env value exactly as read (`None` or `""` when unset);
    `exists` / `is_dir` are injected predicates over the path string, so all
    filesystem work stays at the call site and this half is reachable from the
    self-tests — the split `newest_success_from_runs` vs `newest_successful_run`
    already uses, for the same reason.

    A whitespace-only value counts as UNSET: it names no path, and treating it
    as a misconfiguration would report a directory that was never asked for.
    """
    if raw is None or not str(raw).strip():
        return AUDIT_DIR_UNSET
    path = str(raw)
    if not exists(path) or not is_dir(path):
        return AUDIT_DIR_UNUSABLE
    return AUDIT_DIR_OK


def resolve_audit_dir(raw) -> tuple[str, Path | None]:
    """I/O half: classify the env value and hand back a Path only when usable.

    Returns `(state, path_or_None)`. `None` is deliberate — a caller cannot
    accidentally scan a directory that does not exist, because there is no
    plausible-looking stand-in to scan. Fail-soft: an OSError from the
    filesystem probes reports UNUSABLE rather than raising, since this
    extractor must never block a session.
    """

    def _exists(p: str) -> bool:
        try:
            return Path(p).exists()
        except OSError:
            return False

    def _is_dir(p: str) -> bool:
        try:
            return Path(p).is_dir()
        except OSError:
            return False

    state = classify_audit_dir(raw, _exists, _is_dir)
    return state, (Path(str(raw)) if state == AUDIT_DIR_OK else None)


def scan_provider_lines(lines) -> tuple[int, int, int]:
    """Fold a stream of lines into (hits, prose_rejected, unanchored_rejected).

    Pure — this is the ONE statement of the fold, so the self-tests drive the
    same rule `_scan_provider_stream` runs over a real file rather than a
    second copy of it that agrees the day it is written and diverges forever
    after (the `significant_braces` precedent).
    """
    hits = 0
    prose = 0
    unanchored = 0
    for line in lines:
        marker, reason = classify_provider_error(line)
        if marker is not None:
            hits += 1
        elif reason == PROVIDER_REJECT_PROSE:
            prose += 1
        elif reason == PROVIDER_REJECT_UNANCHORED:
            unanchored += 1
    return hits, prose, unanchored


def _scan_provider_stream(path: Path) -> tuple[int, int, int, int]:
    """Scan one file. Returns (hits, prose, unanchored, unread).

    Bounded by the existing AUDIT_FILE_SIZE_CAP — no new budget was invented.
    Fail-soft: an OSError warns and contributes nothing, because this
    extractor must never block a session.

    `unread` is 1 when this file could NOT be read WHOLE — an OSError, or a
    file past the cap whose tail was never scanned. It is a THIRD fact, never
    summed into hits or into either reject count: a rejected line was read and
    judged, an unread file was never judged at all, and rendering the second
    as the first is "could not check" reading as "checked; clean" — the exact
    refusal the pre-push hook and `CiScan`'s could-not-run branch already make.
    """
    try:
        size = path.stat().st_size
        capped = size > AUDIT_FILE_SIZE_CAP
        if capped:
            warn(f"{path} is {size} bytes (>{AUDIT_FILE_SIZE_CAP}); scanning first {AUDIT_FILE_SIZE_CAP}B only")

        def _bounded(f):
            bytes_read = 0
            for line in f:
                bytes_read += len(line)
                if bytes_read > AUDIT_FILE_SIZE_CAP:
                    break
                yield line

        with path.open(encoding="utf-8", errors="replace") as f:
            h, pr, ur = scan_provider_lines(_bounded(f))
        return h, pr, ur, (1 if capped else 0)
    except OSError as e:
        warn(f"skipped {path}: {e}")
        return 0, 0, 0, 1


def collect_provider_errors(audit_dir: Path) -> ProviderScan:
    """Walk the already-fetched session dirs for provider-error lines.

    BRANCH A fired (Day 196, measured before any edit): the rate-limit text
    lives in `transcripts/*.log` and NOT in `audit.jsonl`. Measured on three
    real day-195 dirs — `grep -c 'Rate limited' audit.jsonl` returned 0 on
    every one, while all six transcripts per dir carried it. So the scan reads
    BOTH streams: `audit.jsonl` keeps its three structured alternatives, and
    the transcripts are where the harness's teed stderr actually lands.

    Zero new `gh` calls and zero network: the worktree these dirs live in was
    already fetched by `evolve.sh` step 1c, so this is the same walk with one
    more `glob` per directory. Each file is bounded by AUDIT_FILE_SIZE_CAP.

    A session counts as examined when it has EITHER stream, so a dir carrying
    transcripts but no audit.jsonl is no longer invisible to the denominator.

    ORDERING IS THE WHOLE REACH, and it is why Day 196's both-streams fix read
    as a no-op (Day 196, second task). This walk used a raw
    `sorted(..., reverse=True)` — LEXICOGRAPHIC — while its two siblings
    (`load_outcomes`, `collect_usage_coverage`) both key on `session_sort_key`.
    Lexicographically `day-99-*` > `day-195-*` because `'9' > '1'`, so the
    WINDOW_SESSIONS window filled up with day-9x dirs from months ago and never
    reached day-194/195 at all. Measured on a fixture that reproduces the live
    line byte-identically: 10 sessions examined, ZERO hits, ZERO rejects — and
    reject counts print only when > 0, so "no candidate lines" and "no session
    I read had one" rendered the same clean bill. The classifier was never
    blind; the window simply pointed at the wrong decade of sessions.
    `session_sort_key` is REUSED rather than re-derived: a second ordering rule
    agrees the day it is written and diverges forever after.
    """
    if not audit_dir.exists():
        return ProviderScan()
    sessions = 0
    hits = 0
    prose_rejected = 0
    unanchored_rejected = 0
    unread = 0
    saw_audit = False
    saw_transcripts = False
    for child in sorted(
        audit_dir.iterdir(), key=lambda c: session_sort_key(c.name), reverse=True
    ):
        if not child.is_dir():
            continue
        audit = child / "audit.jsonl"
        try:
            transcripts = sorted((child / "transcripts").glob("*.log"))
        except OSError as e:
            warn(f"skipped transcripts of {child}: {e}")
            transcripts = []
            unread += 1
        streams = ([audit] if audit.is_file() else []) + transcripts
        if not streams:
            continue
        sessions += 1
        if audit.is_file():
            saw_audit = True
        if transcripts:
            saw_transcripts = True
        for stream in streams:
            h, pr, ur, un = _scan_provider_stream(stream)
            hits += h
            prose_rejected += pr
            unanchored_rejected += ur
            unread += un
        if sessions >= WINDOW_SESSIONS:
            break
    names = []
    if saw_audit:
        names.append("audit.jsonl")
    if saw_transcripts:
        names.append("transcripts/*.log")
    return ProviderScan(
        sessions, hits, prose_rejected, tuple(names), unanchored_rejected, unread
    )


def render_provider_health(
    sessions: int,
    hits: int,
    state: str = AUDIT_DIR_OK,
    prose_rejected: int = 0,
    streams: tuple = (),
    unanchored_rejected: int = 0,
    unread_streams: int = 0,
) -> str:
    """Render the provider-health section, or an honest one-line refusal.

    The two non-OK states get ONE line each (header inline, the shape
    `render_ci_errors` already uses for its could-not-check note): this section
    renders before the epistemic block, TOTAL_BYTE_CAP is tight, and the
    epistemic section has been truncated away once already (Day 142).

    Day 196 NARROWED THE CLEAN CLAIM. It used to read
    `N sessions, no provider errors detected.`, which reads as *the provider
    was fine* — and it said exactly that over 7 consecutive sessions that were
    killed by a 23-hour rate limit and produced zero task commits. What the
    evidence supports is *no provider-error lines in the files I opened*, so
    the sentence now names the streams that were read.

    An EMPTY `streams` names no stream at all rather than falling back to a
    literal `in audit.jsonl`: the caller recorded nothing about what it read,
    and asserting a file we cannot show we opened is the same fabrication as
    the old clean line claiming health. So the two OK branches lost their
    trailing location clause when `streams` is empty, and the three self-tests
    that pinned the old wording were UPDATED rather than deleted (a fixture
    pinning a superseded convention that outlives its replacement converts a
    defect into a green invariant). The two AUDIT_DIR_* refusal branches are
    byte-identical and their assertions are untouched — that is #843's
    regression guard and it stays green unedited.

    `prose_rejected` and `unanchored_rejected` are reported only when > 0,
    are never summed into EACH OTHER (two different facts, two remedies), and
    are NEVER summed into the hit count: a line whose shape matched but whose provenance is my own
    writing is a different fact from a provider error, and folding them is the
    contamination defect this whole section exists to avoid.

    THREE STATED LIMITS, AND LIMIT 3 IS THE ONE THAT BOUNDS THE WHOLE SECTION:

    1. IT DETECTS A LINE, NOT AN OUTAGE. A hit means the harness printed a
       provider-error line, never that the provider was actually down, and
       never that the session failed to recover — a run that hit one 429,
       retried and finished green scores identically to the 7 dead day-195
       sessions. The count is evidence to read, not a verdict.
    2. THE HARNESS'S OWN STDOUT IS STILL UNREAD. Branch A put the rate-limit
       text in `transcripts/*.log`, which this now reads — but a session dir
       carries AGENT transcripts and not the harness's own stdout
       (`measure_abstentions.py`'s Day-177 finding), so a provider error that
       appears ONLY in the workflow log is invisible here. That is why the
       clean line names the streams it read instead of claiming health:
       silence over an unread stream must not read as clean.
    3. THIS MAKES THE CAUSE LEGIBLE; IT STOPS NOTHING. The harness still runs
       every phase after a terminal rate limit and still files the session as
       `tasks N/N ✅`. `scripts/evolve.sh` is protected, so that half cannot be
       fixed from here — this section only ensures the next planner can see
       why a day produced nothing.
    """
    if state == AUDIT_DIR_UNSET:
        return (
            "## Provider/API health: not checked — YOYO_AUDIT_DIR is unset, "
            "so there is no audit-log to scan (this is normal for a hand-run)."
        )
    if state == AUDIT_DIR_UNUSABLE:
        return (
            "## Provider/API health: not checked — YOYO_AUDIT_DIR is set but is "
            "not a readable directory. This is not 'no provider errors'."
        )
    if sessions == 0:
        # ANTI-VACUOUS, and it is asserted BEFORE any clean branch: a walk that
        # examined ZERO sessions has no evidence of anything, and rendering it
        # as silence let "I found no session dirs" pass for "I checked and the
        # provider was fine" — this section's own subject wearing the opposite
        # sign, and quieter than the bug. SUPERSEDED BEHAVIOUR, recorded rather
        # than erased: this used to `return ""`, and the self-test that pinned
        # the empty string was INVERTED rather than deleted (Day 148 — a
        # fixture pinning a superseded convention that outlives its replacement
        # converts a defect into a green invariant).
        return (
            "## Provider/API health: not checked — the audit-log directory "
            "holds no session directories, so nothing was scanned. This is not "
            "'no provider errors'."
        )
    scanned = f" in {', '.join(streams)}" if streams else ""
    rejects = []
    if prose_rejected > 0:
        rejects.append(f"{prose_rejected} prose-shaped")
    if unanchored_rejected > 0:
        rejects.append(f"{unanchored_rejected} unanchored")
    tail = f" ({', '.join(rejects)} line(s) rejected)" if rejects else ""
    # A THIRD fact with a THIRD remedy: never summed into `hits` or into either
    # reject count, and reported only when > 0 so a fully-read window is
    # byte-identical to before. A file skipped for size or refused by the OS
    # was never judged, so a clean line that does not mention it is claiming
    # coverage it does not have.
    if unread_streams > 0:
        tail += (
            f" {unread_streams} file(s) could NOT be read whole (size cap or "
            f"OS error) — this window is INCOMPLETE, not clean."
        )
    if hits == 0:
        # Says what was READ, never that the provider was healthy.
        return (
            f"## Provider/API health\n{sessions} sessions, no provider-error "
            f"lines{scanned}.{tail}"
        )
    return (
        f"## Provider/API health\n{sessions} sessions, {hits} provider error "
        f"hit(s){scanned}.{tail}"
    )


# --- Usage-record coverage (#848 follow-up) -------------------------------
#
# #848 landed the PRODUCER: `emit_output` writes one {"type":"usage", ...}
# line per run into `.yoyo/audit.jsonl`, which `evolve.sh` pushes to the
# audit-log branch. Nothing asked whether it is still producing.
#
# The defect that hid for 102 days was never a wrong number — the dashboard's
# cost figure was frozen at $1,077.59, which is not a zero, so no non-zero
# check could see it. What was missing was a CONSUMER that notices a channel
# going quiet. This is that consumer.
#
# FOUR states, and no two of them may fold into each other:
#   RECORDED       the producer wrote a usage line here
#   ABSENT         the session ran on a binary that COULD log usage, and did not
#   UNREADABLE     *could not check* — the file would not open or would not parse
#   NOT_MEASURABLE the session predates the producer, so it *could not have*
# "the session ran and logged no usage" is an error, while "I could not read
# the file" is *could not check* — collapsing them rebuilds the frozen-number
# defect one layer down, which is the exact failure this detector exists for.
#
# NOT_MEASURABLE is the Day-181 correction. Shipped with three states, this
# detector's first live render said "8 session(s) ran and logged NO usage line"
# about eight sessions that predated the producer entirely — a false alarm in
# the block where priority 0 is "fix CI failures, this overrides everything
# else". Same class as the CI green-since probe: **"was red" must not read as
# "is red"**. It also self-heals in ~10 sessions as the window slides, which is
# worse rather than better — an alarm that decays on its own trains me to
# discount the line permanently, right before a real freeze looks identical.
USAGE_RECORDED = "recorded"
USAGE_ABSENT = "absent"
USAGE_UNREADABLE = "unreadable"
USAGE_NOT_MEASURABLE = "not_measurable"

# The #848 usage producer landed in 8a633cff, committed 2026-08-28T00:26:33Z.
# Re-derive with: git log -1 --format=%cI 8a633cff
#
# A CONSTANT with its provenance in the comment, deliberately not a `git`
# lookup: the harness checkout is shallow (already documented for
# `git_added_ts`), so a `git show` on a sha can fail and would silently turn
# the boundary into "unknown" — trading a false alarm for a silent one, which
# is the direction I cannot see.
#
# Note the commit SUBJECT reads "Day 180 (23:26)" — that is the session label,
# not the commit time. Keying on the journal heading would be off by an hour.
USAGE_PRODUCER_LANDED_TS = "2026-08-28T00:26:33Z"
USAGE_PRODUCER_SHA = "8a633cff"


def classify_session_usage(lines) -> str:
    """Pure: does this session's audit.jsonl carry at least one usage record?

    `lines` is an iterable of raw JSONL strings. Returns one of
    USAGE_RECORDED / USAGE_ABSENT / USAGE_UNREADABLE.

    The predicate is `type == "usage"`, never "has a type" and never "is not
    a tool call": #848's compatibility rule is that a line with NO `type` key
    still means a tool call, because `write_audit_entry` deliberately emits
    none. So a file of pure tool-call lines is ABSENT — the producer is silent
    — and is emphatically not UNREADABLE.

    Blank lines are not corruption (a trailing newline is normal JSONL), so
    they are skipped without counting toward anything. A file with no
    non-blank lines at all, and a file where every non-blank line fails to
    parse, are both UNREADABLE: in neither case did we manage to look.
    """
    seen = 0
    parsed = 0
    for raw in lines:
        if not str(raw).strip():
            continue
        seen += 1
        try:
            obj = json.loads(raw)
        except (json.JSONDecodeError, TypeError, ValueError):
            continue
        parsed += 1
        if isinstance(obj, dict) and obj.get("type") == "usage":
            return USAGE_RECORDED
    if seen == 0 or parsed == 0:
        return USAGE_UNREADABLE
    return USAGE_ABSENT


COMPACT_STAMP_RE = re.compile(r"^\d{8}T\d{6}Z$")


def compact_utc_stamp(s):
    """Pure: normalise a UTC timestamp to the compact form YYYYMMDDTHHMMSSZ.

    ONE statement of "what instant is this string", so nothing anywhere
    re-derives it. Accepts both shapes that actually occur here: the compact
    session-directory stamp (`20260828T020932Z`, SESSION_DIR_RE group 2) and
    the ISO-8601 form git prints (`2026-08-28T00:26:33Z`).

    Returns None for anything else — a malformed stamp must stay *unknown*,
    never be coerced into a comparable value. Two normalised stamps compare
    correctly with plain `<` because the compact Zulu form is fixed-width and
    big-endian; no datetime parsing, so nothing here can raise.
    """
    if not isinstance(s, str):
        return None
    t = s.strip().replace("-", "").replace(":", "")
    return t if COMPACT_STAMP_RE.match(t) else None


def session_dir_stamp(name):
    """Pure: the comparable instant of a session directory name.

    `day-181-20260828T020932Z` -> `20260828T020932Z`. None when the name does
    not match SESSION_DIR_RE or the stamp is malformed.
    """
    m = SESSION_DIR_RE.match(name or "")
    if not m:
        return None
    return compact_utc_stamp(m.group(2))


def apply_usage_boundary(verdict, session_ts, boundary_ts=USAGE_PRODUCER_LANDED_TS):
    """Pure: demote an ABSENT verdict to NOT_MEASURABLE when the session
    predates the producer. Everything else passes through unchanged.

    Deliberately separate from `classify_session_usage`, which is a pure
    function of file CONTENTS and knows nothing about time. Three rules, and
    each of them is the whole correctness of this:

    1. Only ABSENT is ever demoted. RECORDED passes through even if it somehow
       predates the boundary — an observation beats a claim — and UNREADABLE
       passes through too, because *could not check* is not the same fact as
       *could not have*.
    2. An unparseable or missing session timestamp is NOT demoted; it stays
       ABSENT. Promoting an unknown into the comfortable bucket is the
       absence-absorbed-by-a-convenient-neighbour defect, and it fails toward
       silence, which is the direction I cannot see.
    3. The boundary is STRICT-BEFORE. A session stamped exactly at the
       boundary is measurable.
    """
    if verdict != USAGE_ABSENT:
        return verdict
    stamp = compact_utc_stamp(session_ts)
    boundary = compact_utc_stamp(boundary_ts)
    if stamp is None or boundary is None:
        return verdict
    return USAGE_NOT_MEASURABLE if stamp < boundary else verdict


@dataclass
class UsageCoverage:
    """Five numbers that are never summed into each other."""

    recorded: int = 0
    absent: int = 0
    unreadable: int = 0
    not_measurable: int = 0
    examined: int = 0


def usage_coverage(verdicts) -> UsageCoverage:
    """Pure: fold per-session verdicts into a coverage tally.

    `examined` counts every session we looked at, including the unreadable
    and the not-measurable ones — a shrinking denominator inside my own meter
    is the defect this whole family of checks exists to prevent. In
    particular `not_measurable` is NEVER summed into `absent`: a session that
    predates the producer did not fail to log, it could not have logged.
    """
    cov = UsageCoverage()
    for v in verdicts:
        cov.examined += 1
        if v == USAGE_RECORDED:
            cov.recorded += 1
        elif v == USAGE_ABSENT:
            cov.absent += 1
        elif v == USAGE_NOT_MEASURABLE:
            cov.not_measurable += 1
        else:
            cov.unreadable += 1
    return cov


def collect_usage_coverage(audit_dir: Path) -> UsageCoverage:
    """I/O half: classify the last N sessions' audit.jsonl files.

    Walks the same `sessions/day-*/` directories `load_outcomes` already
    walks — they are in the audit-log worktree the harness already fetched,
    so this makes NO new network call and no new `gh` call. Reads are bounded
    by AUDIT_FILE_SIZE_CAP exactly as `collect_provider_errors` bounds its
    own, so a runaway audit.jsonl cannot be slurped whole.
    """
    if not audit_dir.exists() or not audit_dir.is_dir():
        return UsageCoverage()
    verdicts: list[str] = []
    for child in sorted(audit_dir.iterdir(), key=lambda c: session_sort_key(c.name), reverse=True):
        if not child.is_dir():
            continue
        audit = child / "audit.jsonl"
        if not audit.is_file():
            continue
        try:
            size = audit.stat().st_size
            if size > AUDIT_FILE_SIZE_CAP:
                warn(
                    f"{audit} is {size} bytes (>{AUDIT_FILE_SIZE_CAP}); "
                    f"scanning first {AUDIT_FILE_SIZE_CAP}B only"
                )

            def _bounded(path=audit):
                with path.open(encoding="utf-8", errors="replace") as f:
                    read = 0
                    for line in f:
                        read += len(line)
                        if read > AUDIT_FILE_SIZE_CAP:
                            break
                        yield line

            verdicts.append(
                apply_usage_boundary(
                    classify_session_usage(_bounded()), session_dir_stamp(child.name)
                )
            )
        except OSError as e:
            warn(f"skipped {audit}: {e}")
            # Could not open it at all — that is *could not check*, and it is
            # counted rather than dropped. Routed through the same boundary
            # call so the rule has ONE statement; it is a no-op here by
            # `apply_usage_boundary`'s rule 1 (only ABSENT is ever demoted).
            verdicts.append(
                apply_usage_boundary(USAGE_UNREADABLE, session_dir_stamp(child.name))
            )
        if len(verdicts) >= WINDOW_SESSIONS:
            break
    return usage_coverage(verdicts)


def render_usage_coverage(cov: UsageCoverage, state: str = AUDIT_DIR_OK) -> str:
    """Render coverage — k of N sessions carrying >= 1 usage record.

    COVERAGE, never magnitude: no token total and no dollar figure, because a
    frozen number is not a zero and a non-zero check cannot see this. Frozen
    shows up as 0/N on the very next session; partial silence as k/N.

    Anti-vacuous, in BOTH denominators: `examined == 0` renders the
    could-not-check line, NEVER a healthy "0 of 0" — and so does a window
    whose sessions are ALL not-measurable, because "0 of 0 measurable
    sessions" would reintroduce that exact defect one denominator down. A
    detector that reports success on an empty scan is the "cannot fail
    loudly" defect wearing the opposite sign, and it is quieter than the bug
    it was built for.

    `not_measurable` is reported as its OWN clause and is never summed into
    `absent`, and it leaves the denominator: coverage is k of the sessions
    that *could* have logged. A partially-measurable window is still real
    data, so a non-zero `not_measurable` is not a refusal; a wholly
    unmeasurable one is.

    Held to at most 3 lines: this renders before the epistemic block, which
    absorbs all truncation pressure and has been cut away once already.
    """
    if state == AUDIT_DIR_UNSET:
        return (
            "## Usage records: not checked — YOYO_AUDIT_DIR is unset, so there "
            "is no audit-log to scan (normal for a hand-run). Not a clean bill."
        )
    if state == AUDIT_DIR_UNUSABLE:
        return (
            "## Usage records: not checked — YOYO_AUDIT_DIR is set but is not a "
            "readable directory. This is NOT 'no usage records missing'."
        )
    if cov.examined == 0:
        return (
            "## Usage records: not checked — 0 sessions had an audit.jsonl to "
            "read. This is NOT 'no usage records missing'."
        )
    measurable = cov.examined - cov.not_measurable
    if measurable <= 0:
        return (
            f"## Usage records: not measurable — all {cov.examined} session(s) "
            f"in the window predate the #848 producer ({USAGE_PRODUCER_SHA}). "
            f"This is NOT 'no usage records missing'."
        )
    predate = (
        f"{cov.not_measurable} session(s) predate the #848 producer "
        f"({USAGE_PRODUCER_SHA}) and cannot be measured"
    )
    if cov.absent == 0 and cov.unreadable == 0:
        if cov.not_measurable == 0:
            return (
                f"## Usage records\n"
                f"{cov.recorded} of {cov.examined} sessions carry >=1 usage record "
                f"(#848 channel is live)."
            )
        return (
            f"## Usage records\n"
            f"{cov.recorded} of {measurable} measurable sessions carry >=1 usage "
            f"record (#848 channel is live).\n"
            f"{predate}."
        )
    lines = [
        "## Usage records",
        (
            f"{cov.recorded} of {measurable} measurable sessions carry >=1 usage record."
            if cov.not_measurable
            else f"{cov.recorded} of {cov.examined} sessions carry >=1 usage record."
        ),
    ]
    detail = []
    if cov.absent:
        detail.append(
            f"{cov.absent} session(s) ran and logged NO usage line — the #848 "
            f"producer wrote nothing there"
        )
    if cov.unreadable:
        detail.append(
            f"{cov.unreadable} session(s) could not be read (not the same as "
            f"'no usage')"
        )
    if cov.not_measurable:
        detail.append(predate)
    lines.append("; ".join(detail) + ".")
    return "\n".join(lines)


# --- Module-size headroom (from tests/module_size.rs, fail-soft) ---
#
# WHY THIS EXISTS. `tests/module_size.rs` has three branches; two of them are
# deliberately NON-fatal (Day 165/166 repriced them, because a gate whose only
# remedy is a whole-task revert eats the correct work sitting beside the
# violation — #719 and #739, the second dying to a FOUR-line overshoot). Those
# two branches warn to the stderr of a *passing* test, and the only consumer of
# `cargo test` in the evolve loop reads the EXIT CODE. So nothing read them:
# Day 174 paid off 11 entries carrying up to +480 lines of absorbed drift, and
# by Day 183 three more warnings had accumulated unread, one of them a file 8
# lines from FATAL with an open issue queued against it.
#
# This is the reader, on my side of the protected-`evolve.sh` boundary. It does
# NOT make the gate stricter — the consts and branches are untouched.
#
# SINGLE AUTHORITY: every number is parsed out of `tests/module_size.rs`
# itself. Two hand-written copies of a rule agree on the day they are written
# and diverge forever after, which is the whole duplication lesson.

MODULE_GATE_REL_PATH = "tests/module_size.rs"

# Report register drift once it passes this fraction of the gate's own drift
# grace band. A judgment threshold, not a measurement: it exists so the +1/+2
# creep that makes up most of a register's noise does not crowd the section,
# while a file heading for the fatal branch is named well before it arrives.
MODULE_DRIFT_REPORT_FRACTION = 0.25

MODULE_GATE_OK = "module-gate-ok"
MODULE_GATE_UNREADABLE = "module-gate-unreadable"


@dataclass
class ModuleGateSpec:
    """The gate's own rules, parsed from its source. Never a second copy."""

    max_lines: int
    overshoot_grace: int
    drift_grace: int
    register: dict  # path -> recorded lines
    ok: bool


@dataclass
class ModuleRisk:
    """What the gate would say, plus the number it never prints: headroom."""

    # (path, lines, headroom_to_fatal) for the worst unlisted file over the cap.
    worst_unlisted: tuple | None
    # (path, lines, recorded, headroom_to_fatal, kind) for the worst register
    # drift past MODULE_DRIFT_REPORT_FRACTION of the grace band. ONE slot, both
    # directions: the register is one register, so this picks its worst offender
    # exactly as `worst_unlisted` picks the unlisted set's. `kind` is
    # "growth" | "shrink" | "paid-off" and decides the WORDING, so two facts
    # with two different remedies are never folded into one sentence.
    # Indices 0..3 are the pre-#885 shape and are deliberately unchanged.
    worst_drift: tuple | None
    scanned: int


def count_rs_lines(text: str) -> int:
    """Match Rust's `content.lines().count()` exactly.

    Deliberately NOT `str.splitlines()`, which also splits on \\x0b, \\x0c and
    \\u2028 — Rust's `lines()` splits on \\n only (stripping a trailing \\r).
    A counter that disagrees with the gate is worse than no counter.
    """
    if not text:
        return 0
    return text.count("\n") + (0 if text.endswith("\n") else 1)


_GATE_CONST_RE = re.compile(
    r"const\s+(MAX_MODULE_LINES|OVERSHOOT_GRACE_LINES|REGISTER_DRIFT_GRACE_LINES)"
    r"\s*:\s*usize\s*=\s*([0-9_]+)\s*;"
)
_GATE_ENTRY_RE = re.compile(r'\(\s*"(src/[^"]+\.rs)"\s*,\s*([0-9_]+)\s*\)')


def parse_module_gate(text: str) -> ModuleGateSpec:
    """Parse the gate's consts and register out of its own source.

    `ok=False` whenever anything needed is missing — a const absent, or ZERO
    register entries. Zero entries means the parse broke (the register has
    been non-empty since the gate landed), and a scanner that finds nothing
    and passes is this very defect wearing the opposite sign.
    """
    empty = ModuleGateSpec(0, 0, 0, {}, False)
    if not text:
        return empty
    consts = {m.group(1): int(m.group(2).replace("_", "")) for m in _GATE_CONST_RE.finditer(text)}
    needed = ("MAX_MODULE_LINES", "OVERSHOOT_GRACE_LINES", "REGISTER_DRIFT_GRACE_LINES")
    if any(k not in consts for k in needed):
        return empty
    # Only the register literal, so the `("src/a.rs", 500)` pairs inside the
    # gate's own unit-test fixtures are never mistaken for real entries.
    start = text.find("GRANDFATHERED_OVERSIZED_MODULES: &[(&str, usize)] = &[")
    if start < 0:
        return empty
    end = text.find("\n];", start)
    if end < 0:
        return empty
    register = {
        m.group(1): int(m.group(2).replace("_", ""))
        for m in _GATE_ENTRY_RE.finditer(text[start:end])
    }
    if not register:
        return empty
    return ModuleGateSpec(
        consts["MAX_MODULE_LINES"],
        consts["OVERSHOOT_GRACE_LINES"],
        consts["REGISTER_DRIFT_GRACE_LINES"],
        register,
        True,
    )


def module_size_risks(spec: ModuleGateSpec, files) -> ModuleRisk:
    """Pure: which module is closest to turning a session red, and by how much.

    `files` is [(relpath, lines)]. Reports HEADROOM TO FATAL — the number the
    gate itself never prints. The gate says "you are 42 over"; it never says
    "8 more lines reverts your session", and that is the whole point of this
    section rather than a restatement of the warning.

    Register drift is read in BOTH directions (#885 half 2). The gate has been
    fatal on a >100-line shrink since Day 187 and that warning had the same
    non-reader the growth warning had for eight days: it goes to the stderr of
    a PASSING test, and the only consumer of `cargo test` in the evolve loop
    reads the exit code. `MODULE_DRIFT_REPORT_FRACTION` is read by both
    directions — one statement of "how much drift is worth mentioning", never
    a second threshold to drift against the first.
    """
    worst_unlisted = None
    worst_drift = None
    drift_floor = spec.drift_grace * MODULE_DRIFT_REPORT_FRACTION

    def bid(candidate):
        # Smallest headroom wins; ties go to the first seen, matching the
        # pre-existing `<` on the unlisted side.
        nonlocal worst_drift
        if worst_drift is None or candidate[3] < worst_drift[3]:
            worst_drift = candidate

    for path, lines in files:
        recorded = spec.register.get(path)
        if recorded is None:
            if lines > spec.max_lines:
                headroom = spec.max_lines + spec.overshoot_grace - lines
                if worst_unlisted is None or headroom < worst_unlisted[2]:
                    worst_unlisted = (path, lines, headroom)
        elif lines < spec.max_lines:
            # A different fact with a different remedy, so it is checked FIRST
            # and is NOT gated by the report floor: the debt is fully paid, the
            # gate is fatal on it right now, and the fix is to DELETE the entry
            # rather than to update the number. Headroom 0 = already fatal, so
            # it correctly outranks every ordinary drift in the bid.
            bid((path, lines, recorded, 0, "paid-off"))
        elif lines > recorded:
            drift = lines - recorded
            if drift > drift_floor:
                bid((path, lines, recorded, recorded + spec.drift_grace - lines, "growth"))
        elif lines < recorded:
            shrink = recorded - lines
            if shrink > drift_floor:
                bid((path, lines, recorded, spec.drift_grace - shrink, "shrink"))
    return ModuleRisk(worst_unlisted, worst_drift, len(files))


def collect_module_sizes(root: Path) -> tuple:
    """I/O half, at ONE call site. Pure line-counting — no subprocess.

    Never shells `cargo` (#832: a nested cargo rebuilds over the shared
    `target/debug/yoyo` uplift path and reddened CI for three sessions).
    """
    try:
        gate_text = (root / MODULE_GATE_REL_PATH).read_text(errors="replace")
    except OSError as e:
        warn(f"could not read {MODULE_GATE_REL_PATH}: {e}")
        return ModuleGateSpec(0, 0, 0, {}, False), ModuleRisk(None, None, 0)
    spec = parse_module_gate(gate_text)
    files = []
    src = root / "src"
    for p in sorted(src.rglob("*.rs")) if src.is_dir() else []:
        try:
            files.append((p.relative_to(root).as_posix(), count_rs_lines(p.read_text(errors="replace"))))
        except OSError:
            continue
    return spec, module_size_risks(spec, files)


def render_module_sizes(spec: ModuleGateSpec, risk: ModuleRisk) -> str:
    """Three states, none folded into another.

    OK renders NOTHING — silent is the common case and the entire regression
    surface. AT RISK is at most 2 lines. COULD NOT CHECK says so out loud and
    states explicitly that it is not a clean bill: "could not check" must
    never read as "checked; clean".

    Anti-vacuous FIRST: a walk finding zero `src/**/*.rs` files refuses rather
    than rendering OK, because a scanner that finds nothing and passes is this
    defect wearing the opposite sign, and it is quieter than the bug.
    """
    if risk.scanned == 0:
        return (
            "## Module sizes: not checked — the scan found 0 files under src/. "
            "This is NOT 'no modules at risk'."
        )
    if not spec.ok:
        return (
            f"## Module sizes: not checked — could not parse {MODULE_GATE_REL_PATH} "
            f"(consts or register unreadable). This is NOT 'no modules at risk'."
        )
    if risk.worst_unlisted is None and risk.worst_drift is None:
        return ""
    lines = ["## Module sizes (the size gate warns but only fails on the exit code)"]
    if risk.worst_unlisted is not None:
        path, n, headroom = risk.worst_unlisted
        lines.append(
            f"{path} is {n} lines, {n - spec.max_lines} past the "
            f"{spec.max_lines}-line cap and UNLISTED — {headroom} more line(s) "
            f"makes `cargo test` FATAL, which reverts the whole task. "
            f'Fix: split it, or add ("{path}", {n}) to '
            f"GRANDFATHERED_OVERSIZED_MODULES."
        )
    if risk.worst_drift is not None:
        path, n, recorded, headroom, kind = risk.worst_drift
        if kind == "paid-off":
            lines.append(
                f"{path} is {n} lines vs its recorded {recorded} — it has dropped "
                f"BELOW the {spec.max_lines}-line cap entirely, which `cargo test` "
                f"treats as FATAL right now. Fix: DELETE its "
                f"GRANDFATHERED_OVERSIZED_MODULES entry — the debt is paid, so the "
                f"remedy is removal, not a new number."
            )
        elif kind == "shrink":
            lines.append(
                f"{path} is {n} lines vs its recorded {recorded} (-{recorded - n} shrink) "
                f"— {headroom} more line(s) of shrink makes it FATAL. "
                f'Fix: paste ("{path}", {n}) over its entry.'
            )
        else:
            lines.append(
                f"{path} is {n} lines vs its recorded {recorded} (+{n - recorded} drift) "
                f"— {headroom} more line(s) makes it FATAL. "
                f'Fix: paste ("{path}", {n}) over its entry.'
            )
    return "\n".join(lines)


# --- Productivity cross-check (claimed successes vs observed task commits) ---
#
# The measured defect (Day 196): SEVEN consecutive sessions produced zero
# commits while every instrument reported success. `journals/JOURNAL.md`
# records `Session commits: no commits made.` for Day 194 20:33, Day 194
# 22:59, Day 195 03:34, Day 195 10:48, Day 195 16:04, Day 195 20:39 and
# Day 195 23:04; the last real task commits are Day 194 16:00. Meanwhile the
# outcomes section above rendered every Day-195 row as `tasks 1/1 ✅ — build
# OK, tests OK` and `gh run list` reported `success` for each. ~36 hours of
# loop time produced nothing and three independent failure-reporting paths
# called it health.
#
# The chain (read from run log 34656500584): Phase A2 produced zero task
# files -> the harness wrote its fallback task -> the fallback committed
# nothing -> `tasks_succeeded` was incremented anyway -> the evaluator
# rendered no verdict line -> `accepted UNVERIFIED` -> no revert, no issue.
# `scripts/evolve.sh` is protected, so all of that is out of reach. THE
# READER IS NOT.
#
# This file already held BOTH halves and crossed them nowhere:
#   * `load_outcomes()`       -> the CLAIM       (`tasks_succeeded`)
#   * `collect_task_commits()` -> the OBSERVATION (TASK_COMMIT_RE over git log)
# A producer that silently stopped with no consumer that notices is the same
# shape as the #848 cost figure frozen at $1,077.59 for 102 days, and it gets
# the same remedy the Day-181 usage-coverage and Day-183 module-size readers
# got: a cross-check rendered into the briefing every session, on my side of
# the protected-file boundary.

PRODUCTIVITY_OK = "productivity-ok"
PRODUCTIVITY_IDLE = "productivity-idle"
PRODUCTIVITY_NO_CLAIMS = "productivity-no-claims"
PRODUCTIVITY_COULD_NOT_CHECK = "productivity-could-not-check"


@dataclass
class Productivity:
    """Counts, never a single number — and four states, none folded.

    `idle_days` / `idle_claimed` carry the evidence the alarm line needs so a
    reader can connect it to the `N/N ✅` rows it contradicts. `claiming_days`
    and `days_observed` are reported so an OK line states its own denominator
    rather than asserting health over an unstated population.
    """

    state: str = PRODUCTIVITY_COULD_NOT_CHECK
    idle_days: tuple[int, ...] = ()
    idle_claimed: int = 0
    claiming_days: int = 0
    days_observed: int = 0


def claims_by_day(outcomes: list[dict]) -> dict[int, int]:
    """Sum `tasks_succeeded` per day number across the outcome window.

    Defensive in the same way every other reader here is: a row with no
    parseable `day`, or a non-integer `tasks_succeeded`, contributes nothing
    rather than raising or coercing a malformed value into a comparable one.
    A day present with zero claims is KEPT as an explicit 0 — that is a real
    observation ("this day attempted nothing or reverted") and is the
    near-miss the alarm must not fire on.
    """
    claims: dict[int, int] = {}
    for o in outcomes:
        day = o.get("day")
        if isinstance(day, bool) or not isinstance(day, int):
            continue
        got = o.get("tasks_succeeded", 0)
        if isinstance(got, bool) or not isinstance(got, int) or got < 0:
            got = 0
        claims[day] = claims.get(day, 0) + got
    return claims


def classify_productivity(
    day_claims: dict[int, int], days_with_task_commits: set[int]
) -> Productivity:
    """Pure: does every day that CLAIMED a success have a task commit?

    ANTI-VACUOUS, AND IT IS ASSERTED FIRST. An empty `days_with_task_commits`
    means the git collection returned nothing at all, and the answer is
    COULD_NOT_CHECK — never "every day is idle". A scanner that finds nothing
    and reports a full-blown alarm is this very defect wearing the opposite
    sign, and it would fire on every run where `git log` failed. Same rule for
    an empty outcome window: with no claims read, nothing can be missing.

    A DAY CLAIMING ZERO SUCCESSES WITH NO COMMITS IS NOT IDLE. That is an
    honest session that attempted nothing or reverted, and flagging it would
    be a false alarm on the one signal whose entire value is that it fires
    rarely. Only `tasks_succeeded > 0` with zero observed task commits is the
    alarm.

    NO_CLAIMS is its own state and must not render as the alarm: a window
    where nothing was attempted has nothing missing from it.
    """
    if not day_claims:
        return Productivity(state=PRODUCTIVITY_COULD_NOT_CHECK)
    if not days_with_task_commits:
        return Productivity(
            state=PRODUCTIVITY_COULD_NOT_CHECK,
            days_observed=0,
            claiming_days=sum(1 for n in day_claims.values() if n > 0),
        )
    claiming = {day: n for day, n in day_claims.items() if n > 0}
    if not claiming:
        return Productivity(
            state=PRODUCTIVITY_NO_CLAIMS,
            claiming_days=0,
            days_observed=len(days_with_task_commits),
        )
    idle = sorted(day for day in claiming if day not in days_with_task_commits)
    if idle:
        return Productivity(
            state=PRODUCTIVITY_IDLE,
            idle_days=tuple(idle),
            idle_claimed=sum(claiming[day] for day in idle),
            claiming_days=len(claiming),
            days_observed=len(days_with_task_commits),
        )
    return Productivity(
        state=PRODUCTIVITY_OK,
        claiming_days=len(claiming),
        days_observed=len(days_with_task_commits),
    )


def render_productivity(prod: Productivity) -> str:
    """At most 2 lines, header included — the epistemic block renders LAST and
    absorbs all truncation pressure (Day 142, load-bearing: it has been cut
    away before, and TOTAL_BYTE_CAP is 3072), so this must not crowd it.

    Prints one short line even when healthy, following
    `render_usage_coverage`'s precedent rather than `render_module_sizes`':
    the whole point is that the question is asked out loud every session
    instead of asked nowhere at all.

    THREE STATED LIMITS, AND LIMIT 1 LEADS, because a reader that reads as a
    gate is exactly how this goes stale:

    1. IT IS A READER, NOT A GATE. Nothing fails, reverts or files an issue
       when it says a day was idle. Whether it changes anything depends on a
       planner reading it — the same dependency the tests/module_size.rs
       branch-2 warning had for the eight days nobody acted on it.
    2. WINDOW MISMATCH. The outcome window is WINDOW_SESSIONS (10) and the git
       window is WINDOW_DAYS (14). Ten sessions is roughly two days, so days
       sit well inside the git window in practice — but a day whose commits
       fell outside it would false-alarm. Stated, deliberately not coded
       around: boundary logic would grow this past one pass.
    3. It sees TASK commits only (TASK_COMMIT_RE). A session that produced
       only a `session wrap-up` or a `cargo fmt` commit is correctly counted
       as having produced nothing.
    """
    if prod.state == PRODUCTIVITY_COULD_NOT_CHECK:
        return (
            "## Productivity: not checked — no claimed successes to read, or "
            "no task commits found in the git window at all. This is a "
            "REFUSAL, not 'every claimed success landed'."
        )
    if prod.state == PRODUCTIVITY_NO_CLAIMS:
        return (
            "## Productivity\n"
            f"No day in the window claimed a success, so nothing is missing "
            f"({prod.days_observed} day(s) did produce task commits)."
        )
    if prod.state == PRODUCTIVITY_IDLE:
        days = ", ".join(f"day-{d}" for d in prod.idle_days)
        return (
            "## Productivity\n"
            f"IDLE: {days} claimed {prod.idle_claimed} success(es) and produced "
            f"ZERO task commits — the outcome rows above report those days as "
            f"successes, and they shipped nothing."
        )
    return (
        "## Productivity\n"
        f"All {prod.claiming_days} claiming day(s) produced >=1 task commit. "
        f"Says work LANDED, never that it was GOOD."
    )


# --- Counterfactual pairing coverage (DREAM milestone reader) ---

VERDICT_LEDGER_REL_PATH = "dreams/counterfactual_verdicts.jsonl"
PAIRING_LEDGER_REL_PATH = "dreams/assertion_pairings.jsonl"

UNEARNED_VERDICT = "UNEARNED"
PAIR_SIGNAL_VALUE = "PAIR_SIGNAL"

# `check_assertion_weakening.py --pair-verdicts` is the producer; this is its
# reader. The remedy is printed verbatim when coverage is incomplete, because a
# reader surface that states a problem without its pasteable fix is half a signal.
PAIRING_REMEDY = (
    "python3 scripts/check_assertion_weakening.py --pair-verdicts "
    f"{VERDICT_LEDGER_REL_PATH} --record {PAIRING_LEDGER_REL_PATH}"
)

PAIRING_OK = "pairing-ok"
PAIRING_NO_UNEARNED = "pairing-no-unearned"
PAIRING_COULD_NOT_CHECK = "pairing-could-not-check"

# A verdict row with no `splice_depth` key predates Day 187's
# `--splice-src-tests` and was read at tests-only depth BY CONSTRUCTION.
# Returning "unknown" here would invent a third depth nobody ever read at.
#
# This is a DELIBERATE second copy of `check_assertion_weakening.py::row_depth`,
# for the same reason `TASK_COMMIT_RE` is a second copy of that file's: sharing
# would mean importing across two hand-run scripts with different lifecycles for
# one three-line rule. It is data-shaped, not policy-shaped.
PAIRING_DEPTH_TESTS_ONLY = "tests"


def _verdict_depth(row: dict) -> str:
    depth = row.get("splice_depth")
    if isinstance(depth, str) and depth.strip():
        return depth.strip()
    return PAIRING_DEPTH_TESTS_ONLY


@dataclass
class PairingCoverage:
    """Five numbers that are NEVER summed with each other.

    `unparseable` is counted and reported rather than silently dropped: a
    shrinking denominator inside my own meter is the defect this whole family
    of checks is about (`UngradedScan.unparseable_excluded` is the precedent).

    `pairings_orphaned` — a pairing row whose sha is not an UNEARNED verdict —
    is zero today and is kept because it is the direction that says the two
    files DISAGREE, which no coverage fraction can express.
    """

    state: str = PAIRING_OK
    unearned: int = 0
    paired: int = 0
    unpaired: int = 0
    pairings_orphaned: int = 0
    unparseable: int = 0
    signal: int = 0
    # depth -> (unearned, paired). Per depth, NEVER pooled: DREAM.md is
    # explicit, and the two depths were measured against different
    # counterfactual trees, so a pooled count answers a question nobody asked.
    by_depth: dict = field(default_factory=dict)


def classify_pairing_coverage(
    verdict_text: str | None, pairing_text: str | None
) -> PairingCoverage:
    """Is every UNEARNED counterfactual verdict paired with an assertion scan?

    Pure over the ledger TEXT so it is table-testable without a file, matching
    `verdict_rows`'s discipline in the producer.

    Three rules, each with its own self-test row:

      1. `verdict_text is None` (missing/unreadable ledger) -> COULD_NOT_CHECK.
         Never "0 unpaired": "could not check" must never read as "checked;
         clean", the same refusal `CiScan`'s could-not-run branch and the
         pre-push hook already make.
      2. Unparseable non-blank lines in EITHER file are counted and reported.
      3. ANTI-VACUOUS, checked FIRST among the data branches: zero UNEARNED
         rows is its OWN state, never healthy coverage. With no rows there is
         nothing to be covered, and a scanner that finds nothing and reports a
         clean bill is this defect wearing the opposite sign — quieter than
         the bug.

    A sha appearing twice in the verdict ledger is one commit, so it is counted
    once; first occurrence wins, exactly as the producer does.

    `pairing_text is None` is NOT a refusal: a fresh clone that has never run
    the pairing mode genuinely has zero pairings, and `0 of N` is the signal
    this reader exists to raise.
    """
    if verdict_text is None:
        return PairingCoverage(state=PAIRING_COULD_NOT_CHECK)

    unparseable = 0

    def _rows(text: str | None):
        nonlocal unparseable
        for line in (text or "").splitlines():
            if not line.strip():
                continue
            try:
                row = json.loads(line)
            except (ValueError, TypeError):
                unparseable += 1
                continue
            if isinstance(row, dict):
                yield row
            else:
                unparseable += 1

    unearned: dict = {}
    for row in _rows(verdict_text):
        if row.get("verdict") != UNEARNED_VERDICT:
            continue
        sha = row.get("sha")
        if not isinstance(sha, str) or not sha.strip():
            continue
        unearned.setdefault(sha.strip(), _verdict_depth(row))

    paired_shas: dict = {}
    for row in _rows(pairing_text):
        sha = row.get("sha")
        if not isinstance(sha, str) or not sha.strip():
            continue
        paired_shas.setdefault(sha.strip(), row.get("pairing"))

    if not unearned:
        return PairingCoverage(
            state=PAIRING_NO_UNEARNED,
            pairings_orphaned=len(paired_shas),
            unparseable=unparseable,
        )

    by_depth: dict = {}
    paired = 0
    signal = 0
    for sha, depth in unearned.items():
        seen_u, seen_p = by_depth.get(depth, (0, 0))
        hit = sha in paired_shas
        if hit:
            paired += 1
            if paired_shas[sha] == PAIR_SIGNAL_VALUE:
                signal += 1
        by_depth[depth] = (seen_u + 1, seen_p + (1 if hit else 0))

    return PairingCoverage(
        state=PAIRING_OK,
        unearned=len(unearned),
        paired=paired,
        unpaired=len(unearned) - paired,
        pairings_orphaned=sum(1 for s in paired_shas if s not in unearned),
        unparseable=unparseable,
        signal=signal,
        by_depth=by_depth,
    )


def collect_pairing_coverage(root: Path) -> PairingCoverage:
    """I/O half, at ONE call site. Two `open()` calls, nothing else.

    Shells NOTHING — no `cargo`, no `gh`, no network (#832: a nested cargo
    rebuilds over the shared `target/debug/yoyo` uplift path and reddened CI
    for three sessions).

    A MISSING pairing ledger reads as empty text, not as a refusal: never
    having run the pairing mode is genuinely zero coverage, which is the
    signal. A missing VERDICT ledger is a refusal, because with no verdicts
    read there is nothing to say about coverage either way.
    """
    try:
        verdict_text = (root / VERDICT_LEDGER_REL_PATH).read_text(errors="replace")
    except OSError as e:
        warn(f"could not read {VERDICT_LEDGER_REL_PATH}: {e}")
        return PairingCoverage(state=PAIRING_COULD_NOT_CHECK)
    try:
        pairing_text = (root / PAIRING_LEDGER_REL_PATH).read_text(errors="replace")
    except FileNotFoundError:
        pairing_text = ""
    except OSError as e:
        warn(f"could not read {PAIRING_LEDGER_REL_PATH}: {e}")
        return PairingCoverage(state=PAIRING_COULD_NOT_CHECK)
    return classify_pairing_coverage(verdict_text, pairing_text)


def render_pairing_coverage(cov: PairingCoverage) -> str:
    """At most 2 lines, header included — the epistemic block renders LAST and
    absorbs all truncation pressure (Day 142), so this must not crowd it.

    Prints one short line even when coverage is COMPLETE, following
    `render_usage_coverage`'s precedent rather than `render_module_sizes`':
    the whole point is that the milestone's status is asked out loud every
    session instead of asked nowhere at all. A producer that stops with no
    consumer that notices is how a cost figure sat frozen for 102 days.

    THE STATED LIMIT IS IN THE RENDERED TEXT, not only here: this reports
    whether the pairing was RUN, never whether a verdict is RIGHT. A row
    paired by a classifier I wrote, over a commit I wrote, is still
    self-referential — DREAM.md says so in its own milestone text. And it is
    a READER, not a gate: nothing fails or reverts when it says 3 of 4.
    """
    if cov.state == PAIRING_COULD_NOT_CHECK:
        return (
            f"## Counterfactual pairing: not checked — could not read "
            f"{VERDICT_LEDGER_REL_PATH}. This is NOT 'coverage is complete'."
        )
    junk = f" {cov.unparseable} unparseable line(s)." if cov.unparseable else ""
    if cov.state == PAIRING_NO_UNEARNED:
        return (
            f"## Counterfactual pairing: nothing to cover — 0 UNEARNED verdicts "
            f"recorded, so coverage is undefined, not complete.{junk}"
        )
    depths = ", ".join(
        f"{d} {p}/{u}" for d, (u, p) in sorted(cov.by_depth.items())
    )
    orphans = (
        f" {cov.pairings_orphaned} pairing(s) match no UNEARNED verdict."
        if cov.pairings_orphaned
        else ""
    )
    if cov.unpaired:
        return (
            f"## Counterfactual pairing\n"
            f"{cov.paired} of {cov.unearned} UNEARNED rows paired, {cov.signal} "
            f"PAIR_SIGNAL ({depths}). {cov.unpaired} unpaired — run: "
            f"{PAIRING_REMEDY}{orphans}{junk}"
        )
    return (
        f"## Counterfactual pairing\n"
        f"{cov.paired} of {cov.unearned} UNEARNED rows paired, {cov.signal} "
        f"PAIR_SIGNAL ({depths}). Says pairing RAN, never that a verdict is "
        f"RIGHT.{orphans}{junk}"
    )


# --- Epistemic blind spots (from `yoyo risk epistemic`, fail-soft) ---

# 3, not 5: the section renders last and absorbs all truncation pressure —
# three intact entries beat five truncated ones (Day 141).
EPISTEMIC_TOP_N = 3

# Per-entry length clamp: keep each blind-spot bullet to one short line so
# the whole section fits the byte budget even with reasons attached.
EPISTEMIC_ENTRY_MAX_CHARS = 90

# Report entry line: "   1. src/commands_search.rs                   5.0"
EPISTEMIC_ENTRY_RE = re.compile(r"^\s*\d+\.\s+(\S+)\s+(\d+(?:\.\d+)?)\s*$")

# Verbose reasons from the report, compacted to fit the byte budget.
# (Day 163 / #726: the "reactive/emerging disagree" reason is gone — the
# epistemic ranking's disagreement signal was the last live consumer of the
# emerging column deleted in #724. Nothing emits that string any more.)
EPISTEMIC_STALE_RE = re.compile(r"last seen (\d+) snapshots ago, no graded event since")
# Study history (dreams/experiments.jsonl), NOT validation grading — kept as its
# own compaction so the planner can see the expedition it already sent.
EPISTEMIC_STUDIED_RE = re.compile(r"studied by graded experiment \(day (\d+), ([^)]+)\)")
# Sibling shape (#711): a round that named the file but recorded no grade. Kept
# as its OWN pattern, not folded into the one above — "I walked past this file"
# and "I graded a guess about it" are different facts, and an uncompacted new
# reason would silently eat the block's byte budget.
EPISTEMIC_VISITED_RE = re.compile(r"visited by ungraded experiment \(day (\d+)\)")
# Prefixes of the compacted study reasons — used to hoist them ahead of the
# other reasons so the per-entry clamp can never be the thing that hides them.
STUDIED_COMPACT_PREFIX = "studied d"
VISITED_COMPACT_PREFIX = "visited d"
# Header of the never-forecast section — a hard stop for entry parsing.
EPISTEMIC_NEVER_FORECAST_RE = re.compile(r"never forecast")
# A row inside that section: "  ◦ src/commands_skill.rs (risk 0.3)". Anchored on
# the ◦ glyph AND the "(risk " suffix so the "... (+N more)" tail and the two
# explanatory prose lines below it can never be mistaken for paths.
EPISTEMIC_NEVER_FORECAST_ROW_RE = re.compile(r"^\s*◦\s+(\S+)\s+\(risk\s")
# How many never-forecast paths reach the planner. They are unranked (a list
# where everything is equally unknown ranks nothing), so this is a byte budget,
# not a top-N: the section renders last and TOTAL_BYTE_CAP is 3KB.
EPISTEMIC_NEVER_FORECAST_SHOWN = 2


def never_forecast_clause(paths: list[str], shown_max: int) -> str:
    """The value half of the "never forecast" line: the shown paths, plus an
    **in-band** marker naming how many were dropped.

    Two independent caps bound this list and only one of them used to say so.
    The Rust renderer caps its own rows at `NEVER_FORECAST_SAMPLE = 5` and
    marks that cut (`... (+N more)`); this collector then re-cut the survivors
    to `EPISTEMIC_NEVER_FORECAST_SHOWN` and marked nothing. Measured Day 175:
    5 dark files existed, the Rust side rendered all 5, and the planner was
    handed 2 with no indication that 3 more were behind them — under a hint
    reading "the never-forecast files are the darkest, point the self-driven
    slot at one of these". The header line carrying the true count is consumed
    and discarded by the parser, so the number survived nowhere.

    Byte-identical to the old join whenever nothing is dropped, which is the
    common case and the regression risk.
    """
    shown = paths[:shown_max] if shown_max > 0 else []
    clause = ", ".join(shown)
    hidden = len(paths) - len(shown)
    if hidden > 0:
        # Same shape as the Rust renderer's own marker, deliberately: a reader
        # meeting both should not have to learn two elision vocabularies.
        clause = f"{clause} (+{hidden} more)" if clause else f"({hidden} more)"
    return clause


def compact_epistemic_reason(reason: str) -> str:
    """Shrink a verbose report reason to a few words (2KB total budget)."""
    m = EPISTEMIC_STALE_RE.search(reason)
    if m:
        return f"stale ({m.group(1)} snapshots)"
    m = EPISTEMIC_STUDIED_RE.search(reason)
    if m:
        # Keep the word "studied" and the grade: this is study history, not
        # validation grading, and the planner must be able to tell the two
        # apart at a glance (never rewrite it as "graded").
        return f"studied d{m.group(1)} ({m.group(2)[:12]})"
    m = EPISTEMIC_VISITED_RE.search(reason)
    if m:
        # Never say "graded" here: the round produced nothing scored.
        return f"visited d{m.group(1)} (no grade)"
    return reason[:60]


def parse_epistemic_output(
    text: str, top_n: int = EPISTEMIC_TOP_N
) -> tuple[list[str], list[str]]:
    """Parse `yoyo risk epistemic` report output.

    Returns `(ranked_lines, never_forecast_paths)`. The two halves are
    deliberately separate values: the first is a *ranking*, the second is a set
    of files no prediction column has ever named, which nothing ranks. Merging
    them would dress the dark half up as the bottom of the lit one.

    Pure (no subprocess) so the self-tests can exercise it on canned output.
    Returns ([], []) when nothing parseable is found (empty states, garbage);
    the second half is [] for reports with no never-forecast section at all.
    """
    entries: list[tuple[str, str, list[str]]] = []
    never_forecast: list[str] = []
    in_never_forecast = False
    for raw in strip_ansi(text).splitlines():
        # The "never forecast" section sits below the ranked entries and is a
        # different kind of claim (files with NO prediction at all). Stop
        # collecting ranked entries there so its rows can never be absorbed as
        # ranked entries or appended as reasons to the last one. Explicit
        # guard, not a reliance on the section's ◦ glyph differing from the •
        # reason bullet. Scanning continues past it (Day 163) only to harvest
        # the section's own rows — the ranked half is finished at this line.
        if EPISTEMIC_NEVER_FORECAST_RE.search(raw):
            in_never_forecast = True
            continue
        if in_never_forecast:
            m = EPISTEMIC_NEVER_FORECAST_ROW_RE.match(raw)
            if m:
                never_forecast.append(m.group(1))
            continue
        m = EPISTEMIC_ENTRY_RE.match(raw)
        if m:
            entries.append((m.group(1), m.group(2), []))
            continue
        stripped = raw.strip()
        if stripped.startswith("•") and entries:
            entries[-1][2].append(compact_epistemic_reason(stripped.lstrip("•").strip()))
    out = []
    for path, score, reasons in entries[:top_n]:
        line = f"- {path} ({score})"
        if reasons:
            # Study history first. The per-entry clamp below drops the tail, and
            # "I already sent an expedition here and graded it" is the reason
            # most likely to change the planner's choice — losing it to the
            # clamp would defeat the whole read-back (Day 151). Stable sort, so
            # everything else keeps report order.
            reasons = sorted(
                reasons,
                key=lambda r: not r.startswith(
                    (STUDIED_COMPACT_PREFIX, VISITED_COMPACT_PREFIX)
                ),
            )
            line += " — " + "; ".join(reasons)
        if len(line) > EPISTEMIC_ENTRY_MAX_CHARS:
            line = line[: EPISTEMIC_ENTRY_MAX_CHARS - 1] + "…"
        out.append(line)
    return out, never_forecast


def find_yoyo_binary() -> str | None:
    """Locate the built yoyo binary (evolve.sh builds before running us)."""
    for p in ("target/debug/yoyo", "target/release/yoyo"):
        if Path(p).is_file() and os.access(p, os.X_OK):
            return p
    return None


def collect_epistemic_blind_spots() -> tuple[list[str], list[str]]:
    """Run `yoyo risk epistemic` and compact its output into
    `(ranked_lines, never_forecast_paths)`. Fail-soft: every skip path warn()s
    (fail-soft without a freshness signal is fail-silent — Day 139) and returns
    ([], []).

    An empty ranked list is NOT a skip when never-forecast paths exist: the
    dark half is the half the ranking structurally cannot see, so dropping it
    for want of a ranking would reproduce the exact bug this reads back."""
    binary = find_yoyo_binary()
    if binary is None:
        warn("epistemic section skipped: yoyo binary not found in target/{debug,release}")
        return [], []
    rc, stdout, _stderr = run_cmd([binary, "risk", "epistemic"], timeout=15)
    if rc != 0:
        warn(f"epistemic section skipped: `{binary} risk epistemic` rc={rc}")
        return [], []
    entries, never_forecast = parse_epistemic_output(stdout)
    if not entries and not never_forecast:
        warn("epistemic section skipped: no parseable entries in report output")
        return [], []
    return entries, never_forecast


def render_epistemic(entries: list[str], never_forecast: list[str] | None = None) -> str:
    """≤10 lines incl. header. Empty entries AND empty never-forecast → honest
    one-line fallback so the planner sees the section exists but is starving,
    not silence.

    `never_forecast` is rendered as ONE clearly-unranked line, because the
    ranked half is generated from files a prediction column already named —
    i.e. by the same attention this section exists to correct. With no
    never-forecast paths the output is byte-identical to the pre-Day-163
    wording (pinned by a self-test)."""
    header = "## Epistemic blind spots (files graded outcomes have taught the model least about)"
    if not entries and not never_forecast:
        return header + "\n(no epistemic data yet)"
    lines = [header] + entries[:EPISTEMIC_TOP_N]
    if never_forecast:
        shown = never_forecast_clause(never_forecast, EPISTEMIC_NEVER_FORECAST_SHOWN)
        lines.append(f"- never forecast (0 predictions ever, unranked): {shown}")
        lines.append(
            "(planner hint: point the self-driven slot at one of these — the never-forecast "
            "files are the darkest, the ranking cannot see them — guess first, grade after)"
        )
    else:
        lines.append(
            "(planner hint: prefer pointing the self-driven slot at one of these — guess first, grade after)"
        )
    return "\n".join(lines)


# ── Final assembly ───────────────────────────────────────────────────────


def cap_output(
    output: str, line_cap: int = TOTAL_LINE_CAP, byte_cap: int = TOTAL_BYTE_CAP
) -> str:
    """Apply the hard line + byte caps to the final trajectory block.

    Bytes-cap reserves room for the truncation marker so the FINAL output
    stays under byte_cap (the marker itself was previously appended after
    the cap, allowing the file to exceed it by ~37 bytes). Pure so the
    self-tests can assert the epistemic steering section survives capping.
    """
    output = truncate_lines(output, line_cap)
    truncation_marker = "\n... (truncated to fit token budget)\n"
    marker_bytes = len(truncation_marker.encode("utf-8"))
    if len(output.encode("utf-8")) > byte_cap:
        budget = byte_cap - marker_bytes
        b = output.encode("utf-8")[:budget]
        # Back off to last newline within b for clean cut
        idx = b.rfind(b"\n")
        if idx > 0:
            b = b[:idx]
        output = b.decode("utf-8", errors="ignore") + truncation_marker
    return output


def main() -> int:
    audit_dir_str = os.environ.get("YOYO_AUDIT_DIR", "")
    repo = os.environ.get("YOYO_REPO", "")
    day = os.environ.get("YOYO_DAY", "?")
    out_path_str = os.environ.get(
        "YOYO_TRAJECTORY_OUT", ".yoyo/session_staging/trajectory.md"
    )
    out_path = Path(out_path_str)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    # Drop any stale output from a prior session — guards against the case
    # where extractor errors mid-run and a partial file survives. Matches
    # the contract evolve.sh expects: file present iff this run wrote it.
    try:
        out_path.unlink()
    except FileNotFoundError:
        pass
    except OSError as e:
        warn(f"could not unlink stale {out_path}: {e}")

    # Three states, none folded (#843). No plausible-looking placeholder: an
    # unset var yields `None`, so neither scan below can be handed a path that
    # was never named. `audit_dir_state` is what the section renders.
    audit_dir_state, audit_dir = resolve_audit_dir(audit_dir_str)

    header = (
        f"# YOUR TRAJECTORY\n\n"
        f"Last computed: {datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%MZ')}. "
        f"Day {day}. Window: last {WINDOW_SESSIONS} sessions / {WINDOW_DAYS} days.\n"
    )

    # Gather all sections (each falls back to "" silently on no-data)
    # Both audit-log scans are skipped outright when the directory is unset or
    # unusable — there is no placeholder to walk, which is the whole of #843.
    outcomes = load_outcomes(audit_dir) if audit_dir is not None else []
    tasks, reverts = collect_task_commits()
    provider_scan = (
        collect_provider_errors(audit_dir) if audit_dir is not None else ProviderScan()
    )
    usage_cov = (
        collect_usage_coverage(audit_dir) if audit_dir is not None else UsageCoverage()
    )
    ci_scan = collect_failed_ci_fingerprints(repo)
    # One shared instant for the CI section, so the rendered verdict and the
    # stderr receipt are provably about the same moment rather than two clocks.
    ci_now = datetime.now(timezone.utc)
    # Cost guard: only ask "has CI gone green since?" when there is actually a
    # failure to qualify. Gated on `clusters` rather than `in_window` because
    # the clusters branch is the ONLY one that renders the verdict — a healthy
    # session (the common case) makes zero extra `gh` calls and its output is
    # byte-identical to before.
    green_scan = (
        newest_successful_run(repo) if (ci_scan.ok and ci_scan.clusters) else None
    )

    sections: list[str] = []
    s = render_outcomes(outcomes)
    if s:
        sections.append(s)
    s = render_task_success(tasks)
    if s:
        sections.append(s)
    # Immediately after per-task activity, and mid-order on purpose: a session
    # nobody chose must be legible in the same glance as the task rows, and
    # appended signals die first under TOTAL_BYTE_CAP (Day 150).
    s = render_unchosen_sessions(tasks)
    if s:
        sections.append(s)
    s = render_reverts(reverts, outcomes)
    if s:
        sections.append(s)
    # Placed mid-order deliberately: a capped surface has an implicit sacrifice
    # order equal to its render order, and an appended signal starts last
    # (Day 145). This is short, steering data — it must survive the cap.
    raw_subsystem_log = collect_subsystem_commits()
    if raw_subsystem_log is not None:
        sub_counts, sub_total = count_subsystems(
            parse_subsystem_commits(raw_subsystem_log)
        )
        s = render_subsystem_concentration(sub_counts, sub_total)
        if s:
            sections.append(s)
    s = render_ci_errors(ci_scan, green_scan, ci_now)
    # The receipt, emitted UNCONDITIONALLY — including when the verdict is
    # correct. This must NOT be "optimised" to fire only on the suspicious
    # branch: a record that appears only when the answer is already wrong gives
    # no baseline to compare against, which is the wrong direction. stderr is
    # the seam that already works — scripts/evolve.sh (protected) captures it to
    # $SESSION_STAGING/trajectory.stderr.log and surfaces head -20 in the cron,
    # so the consumer exists and needs no harness change. ONE line, so it cannot
    # crowd the existing warn() diagnostics out of that head -20.
    print(
        green_probe_receipt(
            ci_scan,
            green_scan,
            green_verdict_branch(ci_scan.newest_failure_ts, green_scan, ci_now),
            GREEN_PROBE_LIMIT,
        ),
        file=sys.stderr,
    )
    # A "could not check" CI note is honest, but it is not trajectory DATA —
    # it must not suppress the global "(no trajectory data yet)" state below.
    ci_unknown = bool(s) and not ci_scan.ok
    if s:
        sections.append(s)
    s = render_provider_health(
        provider_scan.sessions,
        provider_scan.hits,
        audit_dir_state,
        prose_rejected=provider_scan.prose_rejected,
        streams=provider_scan.streams,
        unanchored_rejected=provider_scan.unanchored_rejected,
        unread_streams=provider_scan.unread_streams,
    )
    # Same rule as `ci_unknown` above: a "could not check" provider note is
    # honest, but it is not trajectory DATA and must not suppress the global
    # "(no trajectory data yet)" state below.
    #
    # `sessions == 0` is part of the test for the SAME reason `usage_unknown`
    # below carries `usage_cov.examined == 0`: the zero-session branch is a
    # REFUSAL ("nothing was scanned"), not a reading, so counting it as data
    # would let a scanner that found nothing suppress the honest global
    # "(no trajectory data yet)" state — this section's own subject wearing
    # the opposite sign.
    provider_unknown = bool(s) and (
        audit_dir_state != AUDIT_DIR_OK or provider_scan.sessions == 0
    )
    if s:
        sections.append(s)
    # #848 follow-up: is the usage channel still producing? Same rule as
    # `ci_unknown` / `provider_unknown` above — a "could not check" line is
    # honest but is not trajectory DATA, so it must not suppress the global
    # "(no trajectory data yet)" state below. `examined == 0` is one of those
    # could-not-check states, never a healthy 0-of-0.
    #
    # Day 181: a NON-ZERO `not_measurable` is deliberately NOT one of them. A
    # partially-measurable window still carries real coverage data about the
    # sessions that could have logged, so it stays data. A WHOLLY
    # unmeasurable window (every session predates the producer) is a refusal
    # and is counted as one — same anti-vacuous invariant as `examined == 0`,
    # applied to the second denominator rather than a new policy.
    s = render_usage_coverage(usage_cov, audit_dir_state)
    usage_unknown = bool(s) and (
        audit_dir_state != AUDIT_DIR_OK
        or usage_cov.examined == 0
        or usage_cov.examined - usage_cov.not_measurable <= 0
    )
    if s:
        sections.append(s)
    # Day 183: the module-size gate's two non-fatal branches warn to the stderr
    # of a PASSING test, and the only consumer of `cargo test` in the loop reads
    # the exit code — so nothing read them and three warnings accumulated, one
    # of them 8 lines from fatal. This is that reader. Same rule as `ci_unknown`
    # / `provider_unknown` / `usage_unknown` above: a "could not check" line is
    # honest but is not trajectory DATA, so it must not suppress the global
    # "(no trajectory data yet)" state below.
    module_spec, module_risk = collect_module_sizes(Path.cwd())
    s = render_module_sizes(module_spec, module_risk)
    module_unknown = bool(s) and (module_risk.scanned == 0 or not module_spec.ok)
    if s:
        sections.append(s)
    # DREAM milestone reader: is every UNEARNED counterfactual verdict paired
    # with an assertion-direction scan? `--pair-verdicts` is a HAND-RUN mode,
    # so without this nothing in the loop notices when a new UNEARNED row
    # lands unpaired — a producer that stops with no consumer that notices is
    # exactly how the dashboard's cost figure sat frozen for 102 days.
    #
    # Same rule as `ci_unknown` / `provider_unknown` / `usage_unknown` /
    # `module_unknown` above: a "could not check" line is honest but is not
    # trajectory DATA, so it must not suppress the global "(no trajectory data
    # yet)" state below. The anti-vacuous "nothing to cover" state is a
    # refusal too — 0 UNEARNED rows means coverage is undefined, not complete.
    # Day 196: cross the CLAIM (`tasks_succeeded`, from outcome.json) against
    # the OBSERVATION (task commits in git log). Seven consecutive sessions
    # produced zero commits while every instrument reported success; both
    # halves were already collected here and crossed nowhere. Reuses
    # `outcomes` and `tasks` above — NO second git call and no cargo (#832: a
    # nested cargo rebuilds over the shared target/debug/yoyo uplift path and
    # reddened CI for three sessions). Joined on the DAY NUMBER, never on
    # timestamps: the session directory stamp is written when the audit is
    # PUSHED (Day 195's 23:04 session is `day-195-...T232116Z`) while the
    # commit subject's (HH:MM) is SESSION_TIME from the START — they do not
    # match and must not be compared. Both collectors already carry the day.
    # Same rule as `ci_unknown` / `provider_unknown` / `usage_unknown` /
    # `module_unknown` / `pairing_unknown`: a "could not check" line is honest
    # but is not trajectory DATA and must not suppress the global
    # "(no trajectory data yet)" state below.
    productivity = classify_productivity(
        claims_by_day(outcomes), {day for day, _ in tasks}
    )
    s = render_productivity(productivity)
    productivity_unknown = bool(s) and productivity.state == PRODUCTIVITY_COULD_NOT_CHECK
    if s:
        sections.append(s)
    pairing_cov = collect_pairing_coverage(Path.cwd())
    s = render_pairing_coverage(pairing_cov)
    pairing_unknown = bool(s) and pairing_cov.state != PAIRING_OK
    if s:
        sections.append(s)
    # Always rendered when any signal exists (it has its own honest fallback
    # line) so the planner sees the epistemic view even when it's starving.
    # Skipped only when there is no trajectory data at all AND no epistemic
    # data — preserving the honest global "(no trajectory data yet)" state.
    epistemic_entries, epistemic_never = collect_epistemic_blind_spots()
    data_sections = (
        len(sections)
        - (1 if ci_unknown else 0)
        - (1 if provider_unknown else 0)
        - (1 if usage_unknown else 0)
        - (1 if module_unknown else 0)
        - (1 if pairing_unknown else 0)
        - (1 if productivity_unknown else 0)
    )
    if data_sections or epistemic_entries or epistemic_never:
        sections.append(render_epistemic(epistemic_entries, epistemic_never))

    if data_sections or epistemic_entries or epistemic_never:
        body = "\n\n".join(sections)
    else:
        # Keep the CI "could not check" note (if any) beside the no-data line:
        # not knowing is its own state and gets said out loud.
        body = "\n\n".join(
            ["(no trajectory data yet — audit-log is empty and no recent task commits found)"]
            + sections
        )

    output = header + "\n" + body + "\n"
    output = cap_output(output)

    try:
        out_path.write_text(output)
    except OSError as e:
        warn(f"could not write {out_path}: {e}")
        return 1
    return 0


def run_self_tests() -> int:
    """Self-tests for fingerprint clustering. Run with --test flag."""
    failures = 0

    def assert_eq(label: str, got: str, want: str) -> None:
        nonlocal failures
        if got != want:
            print(f"  FAIL: {label}")
            print(f"    got:  {got!r}")
            print(f"    want: {want!r}")
            failures += 1
        else:
            print(f"  ok: {label}")

    print("=== fingerprint_error_line self-tests ===\n")

    # 1. GH Actions prefixes with different timestamps cluster together
    line_a = "social unknown step 2026-04-15T15:31:42.5342991Z error: auth token expired"
    line_b = "social unknown step 2026-04-08T07:12:03.8992940Z error: auth token expired"
    fp_a = fingerprint_error_line(line_a)
    fp_b = fingerprint_error_line(line_b)
    assert_eq("GH Actions auth errors cluster", fp_a, fp_b)
    # Verify the prefix was actually stripped
    assert_eq("GH Actions prefix stripped", fp_a, "error: auth token expired")

    # 2. Different GH Actions workflows with same error cluster
    line_c = "evolve build test 2026-04-20T10:00:00.1Z FAILED: cargo test exit code 1"
    line_d = "evolve build test 2026-04-21T14:30:00.9999Z FAILED: cargo test exit code 1"
    fp_c = fingerprint_error_line(line_c)
    fp_d = fingerprint_error_line(line_d)
    assert_eq("different workflow timestamps cluster", fp_c, fp_d)

    # 3. Standalone ISO timestamps at line start still stripped
    line_e = "2026-04-15T15:31:42Z error: something broke"
    line_f = "2026-04-08T07:12:03Z error: something broke"
    fp_e = fingerprint_error_line(line_e)
    fp_f = fingerprint_error_line(line_f)
    assert_eq("standalone timestamps cluster", fp_e, fp_f)

    # 4. Hex addresses are normalized
    line_g = "panicked at 0x7fff1234abcd: null pointer"
    line_h = "panicked at 0xdeadbeef9876: null pointer"
    fp_g = fingerprint_error_line(line_g)
    fp_h = fingerprint_error_line(line_h)
    assert_eq("hex addresses cluster", fp_g, fp_h)
    assert_eq("hex replaced with placeholder", "panicked at <hex>: null pointer", fp_g)

    # 5. UUIDs are normalized
    line_i = "error: session 550e8400-e29b-41d4-a716-446655440000 not found"
    line_j = "error: session a1b2c3d4-e5f6-7890-abcd-ef1234567890 not found"
    fp_i = fingerprint_error_line(line_i)
    fp_j = fingerprint_error_line(line_j)
    assert_eq("UUIDs cluster", fp_i, fp_j)

    # 6. file:line:column normalised
    line_k = "error[E0308]: src/main.rs:42:10: type mismatch"
    line_l = "error[E0308]: src/main.rs:99:5: type mismatch"
    fp_k = fingerprint_error_line(line_k)
    fp_l = fingerprint_error_line(line_l)
    assert_eq("file:line:col clusters", fp_k, fp_l)

    # 7. ANSI codes stripped
    line_m = "\x1b[31merror\x1b[0m: something failed"
    fp_m = fingerprint_error_line(line_m)
    assert_eq("ANSI stripped", fp_m, "error: something failed")

    # 8. Subsecond precision doesn't prevent clustering
    line_n = "ci build run 2026-01-01T00:00:00.1Z fatal: git push rejected"
    line_o = "ci build run 2026-06-15T23:59:59.9999999Z fatal: git push rejected"
    fp_n = fingerprint_error_line(line_n)
    fp_o = fingerprint_error_line(line_o)
    assert_eq("subsecond precision clusters", fp_n, fp_o)

    # --- False-positive filtering tests ---
    print("\n=== false-positive filter self-tests ===\n")

    def would_fingerprint(line: str) -> bool:
        """Simulate the filtering logic in collect_failed_ci_fingerprints."""
        if not ERROR_LINE_RE.search(line):
            return False
        stripped = strip_ansi(line)
        if PASSING_TEST_RE.search(stripped):
            return False
        if TEST_RESULT_OK_RE.search(stripped):
            return False
        return True

    def assert_filtered(label: str, line: str) -> None:
        """Assert that a line is NOT fingerprinted (filtered out)."""
        nonlocal failures
        if would_fingerprint(line):
            print(f"  FAIL: {label} — expected filtered, but was fingerprinted")
            failures += 1
        else:
            print(f"  ok: {label}")

    def assert_kept(label: str, line: str) -> None:
        """Assert that a line IS fingerprinted (kept)."""
        nonlocal failures
        if not would_fingerprint(line):
            print(f"  FAIL: {label} — expected fingerprinted, but was filtered")
            failures += 1
        else:
            print(f"  ok: {label}")

    # 9. Passing test with "error" in name is filtered
    assert_filtered(
        "passing test with 'error' in name filtered",
        "test watch::tests::test_watch_result_failed_with_error ... ok",
    )

    # 10. FAILED test with "error" in name IS still fingerprinted
    assert_kept(
        "failed test with 'error' in name kept",
        "test watch::tests::test_watch_result_failed_with_error ... FAILED",
    )

    # 11. Real compiler error IS still fingerprinted
    assert_kept(
        "real compiler error kept",
        "error[E0308]: mismatched types",
    )

    # 12. Passing test result summary is filtered
    assert_filtered(
        "passing test result summary filtered",
        "test result: ok. 3823 passed; 0 failed;",
    )

    # 13. Failed test result summary IS still fingerprinted
    assert_kept(
        "failed test result summary kept",
        "test result: FAILED. 3823 passed; 1 failed;",
    )

    # 14. Passing test with ANSI codes is still filtered
    assert_filtered(
        "passing test with ANSI codes filtered",
        "\x1b[32mtest\x1b[0m some::test_with_error ... \x1b[32mok\x1b[0m",
    )

    # --- session_sort_key self-tests ---
    print("\n=== session_sort_key self-tests ===\n")

    def assert_true(label: str, cond: bool) -> None:
        nonlocal failures
        if not cond:
            print(f"  FAIL: {label}")
            failures += 1
        else:
            print(f"  ok: {label}")

    # 15. Numeric ordering beats the lexicographic trap (day-99 vs day-100)
    assert_true(
        "day-100 outranks day-99 (lexicographic trap)",
        session_sort_key("day-100-x") > session_sort_key("day-99-z"),
    )

    # 16. The exact observed bug: day-139 must outrank day-97/98/99
    assert_true(
        "day-139 outranks day-99",
        session_sort_key("day-139-20260717T000300Z") > session_sort_key("day-99-20260607T120000Z"),
    )

    # 17. Same day: later timestamp string wins
    assert_true(
        "same day, later timestamp wins",
        session_sort_key("day-42-20260601T180000Z") > session_sort_key("day-42-20260601T060000Z"),
    )

    # 18. Unparseable dir name doesn't crash and ranks below all parsed ones
    weird = session_sort_key("not-a-session-dir")
    assert_true("unparseable name yields fallback key", weird == (0, 0, ""))
    assert_true(
        "unparseable name ranks below parsed sessions",
        weird < session_sort_key("day-1-a"),
    )

    # --- epistemic parser self-tests ---
    print("\n=== parse_epistemic_output self-tests ===\n")

    canned = (
        "\n\x1b[1m\x1b[36m🔍 Epistemic view — where graded outcomes have taught the model least\x1b[0m\n\n"
        "   1. src/commands_search.rs                   5.0\n"
        "      • predicted 28×, never graded\n"
        "      • last seen 7 snapshots ago, no graded event since\n"
        "   2. src/commands_spawn.rs                    5.0\n"
        "      • predicted 13×, never graded\n"
        "   3. src/a.rs   3.0\n"
        "   4. src/b.rs   3.0\n"
        "   5. src/c.rs   3.0\n"
        "   6. src/d.rs   2.0\n"
        "\n  high score = the model is blindest here\n"
    )
    parsed, _ = parse_epistemic_output(canned)
    assert_true("top_n cap: 6 entries -> 3 lines", len(parsed) == 3)
    assert_eq(
        "entry with reasons compacts and joins",
        parsed[0],
        "- src/commands_search.rs (5.0) — predicted 28×, never graded; stale (7 snapshots)",
    )
    assert_eq("entry without reasons is bare", parsed[2], "- src/a.rs (3.0)")

    # 19b. Stale reason compacts
    assert_eq(
        "stale reason compacts",
        compact_epistemic_reason("last seen 7 snapshots ago, no graded event since"),
        "stale (7 snapshots)",
    )

    # 19b2. Study-history reason (Day 151, dreams/experiments.jsonl) compacts to
    # its own shape and is NOT conflated with validation grading — the entry
    # keeps saying "never graded" while also saying it was studied.
    assert_eq(
        "studied reason compacts",
        compact_epistemic_reason("studied by graded experiment (day 150, miss)"),
        "studied d150 (miss)",
    )
    canned_studied = (
        "\n\x1b[1m\x1b[36m🔍 Epistemic view — where graded outcomes have taught the model least\x1b[0m\n\n"
        "   1. src/commands_fork.rs                   2.5\n"
        "  • predicted 12×, never graded\n"
        "  • last seen 37 snapshots ago, no graded event since\n"
        "  • studied by graded experiment (day 150, miss)\n"
        "\n  high score = the model is blindest here\n"
    )
    parsed_studied, _ = parse_epistemic_output(canned_studied)
    assert_eq(
        "studied reason survives the per-entry clamp (hoisted to the front)",
        parsed_studied[0],
        "- src/commands_fork.rs (2.5) — studied d150 (miss); "
        "predicted 12×, never graded; stale (3…",
    )
    assert_true(
        "studied entry still reports the ungraded validation ledger",
        "never graded" in parsed_studied[0],
    )
    assert_true(
        "studied entry still respects the per-entry clamp",
        len(parsed_studied[0]) <= EPISTEMIC_ENTRY_MAX_CHARS,
    )

    # 19b3. Visited-but-ungraded reason (#711, Day 163). Its own compaction and
    # its own words: a round that recorded no grade must never read as one that
    # did, and an uncompacted new reason shape would eat the byte budget.
    assert_eq(
        "visited-ungraded reason compacts",
        compact_epistemic_reason("visited by ungraded experiment (day 159)"),
        "visited d159 (no grade)",
    )
    assert_true(
        "visited compaction never claims a grade",
        "studied" not in compact_epistemic_reason("visited by ungraded experiment (day 159)"),
    )
    canned_visited = (
        "\n\x1b[1m\x1b[36m🔍 Epistemic view\x1b[0m\n\n"
        "   1. src/commands_todo.rs                    2.5\n"
        "  • predicted 12×, never graded\n"
        "  • last seen 6 snapshots ago, no graded event since\n"
        "  • visited by ungraded experiment (day 159)\n"
        "\n  high score = the model is blindest here\n"
    )
    parsed_visited, _ = parse_epistemic_output(canned_visited)
    assert_true(
        "visited reason survives the per-entry clamp (hoisted to the front)",
        parsed_visited[0].startswith("- src/commands_todo.rs (2.5) — visited d159 (no grade)"),
    )
    assert_true(
        "visited entry still reports the ungraded validation ledger",
        "never graded" in parsed_visited[0],
    )
    assert_true(
        "visited entry still respects the per-entry clamp",
        len(parsed_visited[0]) <= EPISTEMIC_ENTRY_MAX_CHARS,
    )

    # 19c. Empty states / garbage yield [] (the fail-soft path)
    assert_true(
        "empty-state report yields no entries",
        parse_epistemic_output("  no snapshots yet — run `yoyo risk snapshot` first\n")
        == ([], []),
    )
    assert_true(
        "garbage yields no entries", parse_epistemic_output("total junk\n42\n") == ([], [])
    )

    # 19d. Fallback rendering carries the honest starvation line
    assert_true(
        "render_epistemic fallback line present",
        "(no epistemic data yet)" in render_epistemic([]),
    )
    rendered = render_epistemic(parsed)
    assert_true(
        "rendered section stays within 5 lines incl. header",
        len(rendered.splitlines()) <= 5,
    )
    assert_true("planner hint present", "planner hint" in rendered)

    # 19e. Per-entry clamp: an absurdly long path still yields a <=90-char line
    long_path = "src/" + "x" * 120 + ".rs"
    clamped, _ = parse_epistemic_output(f"   1. {long_path}   5.0\n")
    assert_true(
        "oversized entry clamped to 90 chars",
        len(clamped) == 1 and len(clamped[0]) <= EPISTEMIC_ENTRY_MAX_CHARS,
    )

    # 19f. The "never forecast" section (Day 149) must not leak into the ranked
    # entries: its rows are bulleted (◦), not `N. path score`, and the parser
    # stops collecting at the header line so its bullets can't be appended to
    # the last ranked entry's reasons.
    canned_never = (
        "\n\x1b[1m\x1b[36m🔍 Epistemic view — where graded outcomes have taught the model least\x1b[0m\n\n"
        "   1. src/commands_risk.rs                   5.0\n"
        "  • predicted 12×, never graded\n"
        "   2. src/watch.rs                           2.5\n"
        "\n  high score = the model is blindest here\n"
        "\n  ⚠ never forecast — 37 scored files have never appeared in any prediction\n"
        "  ◦ src/update.rs (risk 3.2)\n"
        "  ◦ src/hooks.rs (risk 1.8)\n"
        "    ... (+35 more)\n"
        "    the ranking above cannot see these — it is built from files I once guessed about.\n"
        "    Files with no recent churn have no risk score and are invisible to both views.\n"
    )
    parsed_never, _ = parse_epistemic_output(canned_never)
    assert_eq(
        "never-forecast section yields only the ranked entries",
        " | ".join(parsed_never),
        "- src/commands_risk.rs (5.0) — predicted 12×, never graded | - src/watch.rs (2.5)",
    )
    assert_true(
        "never-forecast paths never parsed as ranked entries",
        not any("src/update.rs" in line or "src/hooks.rs" in line for line in parsed_never),
    )
    assert_true(
        "never-forecast caveat not appended as a reason",
        not any("cannot see these" in line for line in parsed_never),
    )

    # 19g. The dark half (Day 163): the never-forecast paths are now RETURNED,
    # not just stopped at. The fixture below is verbatim `yoyo risk epistemic`
    # output — a hand-typed fixture pins my belief about the input, not the
    # input (Day 147). Recaptured on Day 163 after #726's sibling removed the
    # reactive/emerging disagreement signal: every ranked entry it used to
    # produce is gone, so the old capture no longer described any reachable
    # output. What ranks now is never-graded / stale / study history.
    canned_real = (
        "\n"
        "🔍 Epistemic view — where graded outcomes have taught the model least\n"
        "\n"
        "   1. src/commands_fork.rs                     1.5\n"
        "      • predicted 14×, never graded\n"
        "      • last seen 60 snapshots ago, no graded event since\n"
        "      • studied by graded experiment (day 150, miss)\n"
        "   2. src/commands_config.rs                   1.5\n"
        "      • predicted 17×, never graded\n"
        "      • last seen 58 snapshots ago, no graded event since\n"
        "   3. src/commands_risk_epistemic.rs           0.5\n"
        "      • last seen 6 snapshots ago, no graded event since\n"
        "   4. src/commands_plan.rs                     0.5\n"
        "      • last seen 85 snapshots ago, no graded event since\n"
        "\n"
        "  note: tied scores are ordered by current risk score (higher first), then path\n"
        "\n"
        "  high score = the model is blindest here; an outcome touching these files teaches the most\n"
        "\n"
        "  ⚠ never forecast — 27 scored files have never appeared in any prediction\n"
        "  ◦ src/commands_skill.rs (risk 0.3)\n"
        "  ◦ src/commands_move.rs (risk 0.3)\n"
        "  ◦ src/commands_lint.rs (risk 0.3)\n"
        "  ◦ src/commands_git_review.rs (risk 0.3)\n"
        "  ◦ src/commands_map.rs (risk 0.3)\n"
        "    ... (+22 more)\n"
        "    the ranking above cannot see these — it is built from files I once guessed about.\n"
        "    Files with no recent churn have no risk score and are invisible to both views.\n"
    )
    real_entries, real_never = parse_epistemic_output(canned_real)
    assert_eq(
        "never-forecast paths parsed out of real report output",
        real_never,
        [
            "src/commands_skill.rs",
            "src/commands_move.rs",
            "src/commands_lint.rs",
            "src/commands_git_review.rs",
            "src/commands_map.rs",
        ],
    )
    assert_eq(
        "ranked entries unchanged by the presence of the never-forecast section",
        " | ".join(real_entries),
        # Entry 1 carries three reasons and overruns EPISTEMIC_ENTRY_MAX_CHARS,
        # so it is clamped mid-reason — study history is hoisted first exactly
        # so the clamp can never be the thing that hides it.
        "- src/commands_fork.rs (1.5) — studied d150 (miss); predicted 14×, never graded; stale (6… | "
        "- src/commands_config.rs (1.5) — predicted 17×, never graded; stale (58 snapshots) | "
        "- src/commands_risk_epistemic.rs (0.5) — stale (6 snapshots)",
    )
    assert_true(
        "the `... (+N more)` tail is not mistaken for a path",
        not any("more" in p for p in real_never),
    )

    # 19h. Rendering the dark half: passing an empty list must be byte-identical
    # to not passing one at all (no silent wording drift for the common case).
    assert_eq(
        "render_epistemic(entries, []) is byte-identical to render_epistemic(entries)",
        render_epistemic(real_entries, []),
        render_epistemic(real_entries),
    )
    rendered_never = render_epistemic(real_entries, real_never)
    assert_true(
        "rendered block names a never-forecast file",
        "src/commands_skill.rs" in rendered_never,
    )
    assert_true(
        "never-forecast line is labelled as unranked",
        "unranked" in rendered_never and "never forecast" in rendered_never,
    )
    assert_true("planner hint survives the dark half", "planner hint" in rendered_never)
    assert_true(
        "epistemic section stays within 10 lines incl. header",
        len(rendered_never.splitlines()) <= 10,
    )
    assert_true(
        "at most 2 never-forecast paths are rendered",
        "src/commands_lint.rs" not in rendered_never,
    )
    # 19h-bis (Day 175, blind round 72). The cut that the assertion directly
    # above pins must be MARKED. This collector re-cuts a list the Rust
    # renderer already cut and used to mark nothing, so the planner was handed
    # 2 of 5 dark rooms under a hint calling them "the darkest" — with the
    # header line carrying the real count consumed and discarded at the parse
    # step. Asserted at the emission point: the string render_epistemic
    # returns, which is the string that reaches the planner prompt.
    assert_true(
        "render_epistemic marks its own never-forecast cut in-band",
        "(+" in rendered_never.split("never forecast", 1)[1].splitlines()[0],
    )
    five_dark = render_epistemic(
        real_entries,
        ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs", "src/e.rs"],
    )
    never_line = [ln for ln in five_dark.splitlines() if "never forecast" in ln][0]
    assert_eq(
        "5 dark rooms, 2 shown, the other 3 named as dropped",
        never_line,
        "- never forecast (0 predictions ever, unranked): src/a.rs, src/b.rs (+3 more)",
    )
    # Near-miss guard: a discriminator tested only on the side that fires is
    # vacuous green. Exactly at the cap nothing is dropped, so the line must be
    # byte-identical to the pre-fix ", ".join output — no marker at all.
    at_cap = render_epistemic(real_entries, ["src/a.rs", "src/b.rs"])
    at_cap_line = [ln for ln in at_cap.splitlines() if "never forecast" in ln][0]
    assert_eq(
        "at the cap the line is byte-identical to the old join (no marker)",
        at_cap_line,
        "- never forecast (0 predictions ever, unranked): src/a.rs, src/b.rs",
    )
    assert_eq(
        "never_forecast_clause is byte-identical under budget",
        never_forecast_clause(["src/a.rs"], 2),
        "src/a.rs",
    )
    # 19i. Dark half alone: zero ranked entries but a live never-forecast list
    # must still render (this is exactly the starving-meter case).
    only_never = render_epistemic([], ["src/commands_skill.rs"])
    assert_true(
        "never-forecast-only report still renders a section",
        "src/commands_skill.rs" in only_never and "planner hint" in only_never,
    )


    # --- subsystem concentration self-tests ---
    print("\n=== subsystem concentration self-tests ===\n")

    # 30. Pure classifier: table entries and the documented fallback.
    for path, want in [
        ("src/commands_risk.rs", "risk"),
        ("src/commands_risk_accuracy.rs", "risk"),
        ("src/commands_git_pr.rs", "git"),
        ("src/format/mod.rs", "format"),
        ("src/prompt_retry.rs", "prompt"),
        ("src/commands_fork.rs", "fork"),
        ("src/commands_search.rs", "search"),
        ("src/repl.rs", "repl"),
        ("src/setup.rs", "setup"),
        ("src/agent_builder.rs", "agent"),
        # Explicit third value — never absorbed into a neighbour (Day 144).
        ("scripts/evolve.sh", SUBSYSTEM_UNKNOWN),
        ("src/bin/other/thing.rs", SUBSYSTEM_UNKNOWN),
        ("", SUBSYSTEM_UNKNOWN),
    ]:
        assert_eq(f"classify {path!r}", classify_subsystem(path), want)

    # 31. VERBATIM FIXTURE. Captured by running, in this repo:
    #   git log --since="14 days ago" --name-only \
    #     --format="@@COMMIT@@%s" -- src/ | head -60
    # Pasted unedited (Day 147-148: a fixture must be captured, not authored —
    # hand-written fixtures encode the same wrong belief as the parser).
    REAL_GIT_LOG_FIXTURE = """\
@@COMMIT@@Day 150 (17:26): Give #631 the knob I keep refusing to flip — opt-in `--continue-on-silence` (Task 2, eval-fix 1)

src/repl.rs
@@COMMIT@@Day 150 (17:26): Give #631 the knob I keep refusing to flip — opt-in `--continue-on-silence` (Task 2)

src/cli.rs
src/help.rs
src/repl.rs
@@COMMIT@@Day 150 (17:26): DREAM chosen experiment — point the blind-spot map at `commands_fork.rs` (outside the risk subsystem), guess first (Task 1)

src/commands_fork.rs
@@COMMIT@@help: pin --help subcommand list against dispatch_sub.rs routing

src/help.rs
@@COMMIT@@Day 150 (02:28): Self-improvement (small, committed) (Task 1)

src/commands_risk_weights.rs
@@COMMIT@@Day 149 (17:42): A config file with no reachable key must not lock the user out of the wizard (#628, second half) (Task 3)

src/setup.rs
@@COMMIT@@Day 149 (17:42): Setup wizard must persist the API key it asked for (#628) (Task 2)

src/setup.rs
@@COMMIT@@epistemic: pin never-forecast parser guard in extract_trajectory self-tests + document section in CLAUDE.md

src/commands_risk_epistemic.rs
@@COMMIT@@Day 149 (17:42): DREAM — the blind-spot map is blind to files it never predicted: add an explicit "never forecast" section (Task 1)

src/commands_risk_epistemic.rs
@@COMMIT@@Day 149 (11:20): Self-improvement (small, committed) (Task 1)

src/update.rs
@@COMMIT@@Day 149 (02:50): Show the raw stop state, not just the verdict — `--verbose` turn-end debug line (Task 3)

src/repl.rs
@@COMMIT@@Day 149 (02:50): Pin the Agent-alive-across-drain invariant (yoagent 0.13.3 drop-kills-run) (Task 2)

src/prompt.rs
@@COMMIT@@Day 149: breadth split — fixture tests, CLAUDE.md, and the experiment

src/commands_risk_accuracy.rs
@@COMMIT@@Day 149 (02:50): DREAM — split failure-day recall by outcome breadth (is 48% blindness, or unpredictable-by-construction?) (Task 1)

src/commands_risk_accuracy.rs
@@COMMIT@@/def, /refs: resolve the callee, not the last argument

src/commands_search.rs
@@COMMIT@@Day 148 (16:59): DREAM — make `yoyo risk harvest` reachable in a shallow clone (GitHub compare API), then actually run it (Task 1)

src/commands_risk.rs
@@COMMIT@@Day 148 (11:14): social session (learnings + seen-state)

src/agent_builder.rs
src/banner.rs
src/cli.rs
src/cli_config.rs
src/commands.rs
src/commands_ast_grep.rs
src/commands_bg.rs
src/commands_config.rs
"""

    parsed = parse_subsystem_commits(REAL_GIT_LOG_FIXTURE)
    subjects = [s for s, _ in parsed]

    # The sweep commit ("social session (learnings + seen-state)") touches
    # dozens of files and would swamp the histogram. It is not a task-shaped
    # subject, so it must be absent along with every file it carried.
    assert_true(
        "sweep commit excluded from parsed commits",
        not any("social session" in s for s in subjects),
    )
    swept = {p for _s, paths in parsed for p in paths}
    assert_true(
        "sweep-only files never enter the histogram",
        "src/agent_builder.rs" not in swept and "src/cli_config.rs" not in swept,
    )
    # The eval-fix re-commit of Task 2 is not TASK_COMMIT_RE-shaped either, so
    # one task cannot be double-counted.
    assert_true(
        "eval-fix commit not counted as a separate task commit",
        not any("eval-fix" in s for s in subjects),
    )
    assert_eq(
        "verbatim fixture yields the real task-commit count",
        str(len(parsed)),
        "11",
    )

    counts, total = count_subsystems(parsed)
    assert_eq("fixture total commits", str(total), "11")
    assert_eq("fixture top subsystem is risk", counts[0][0], "risk")
    assert_eq("fixture risk count", str(counts[0][1]), "4")
    assert_eq(
        "fixture histogram is the real one",
        repr(dict(counts)),
        repr(
            {
                "risk": 4,
                "repl": 2,
                "setup": 2,
                "cli": 1,
                "fork": 1,
                "help": 1,
                "prompt": 1,
                "update": 1,
            }
        ),
    )

    # 32. Renderer: below the warn ratio there is a histogram and no gate line.
    rendered = render_subsystem_concentration(counts, total)
    assert_true(
        "histogram header present",
        "## Subsystem concentration (last 11 self-driven task commits)" in rendered,
    )
    assert_true("histogram row present", "risk: 4/11" in rendered)
    assert_true("no warning below the ratio", "⚠️" not in rendered)

    # 33. Renderer: at/above CONCENTRATION_WARN_RATIO the gate fires with the
    # arithmetic stated, not a judgment word.
    warned = render_subsystem_concentration([("risk", 7), ("repl", 3), ("setup", 2)], 14)
    assert_true("warning fires at half the window", "⚠️" in warned)
    assert_true(
        "warning states the arithmetic",
        "risk took 7 of the last 14 self-driven diffs" in warned,
    )
    assert_true(
        "warning prescribes the action",
        "file the in-zone idea instead" in warned,
    )

    # 34. Too few commits → no section at all (a ratio over 2 commits is noise).
    assert_eq(
        "tiny sample renders nothing",
        render_subsystem_concentration([("risk", 2)], 2),
        "",
    )

    # 35. Malformed / empty input is survivable, not a crash.
    assert_eq("empty log parses to nothing", str(len(parse_subsystem_commits(""))), "0")
    assert_eq(
        "orphan path lines without a commit header are ignored",
        str(len(parse_subsystem_commits("src/repl.rs\nsrc/cli.rs\n"))),
        "0",
    )

    # --- cap_output self-tests: the steering channel must survive ---
    print("\n=== cap_output self-tests ===\n")

    # Build a synthetic full-length trajectory at today's observed section
    # sizes (Day 141: the realistic six-section output ran ~2.3-2.6KB, and
    # the old 2048-byte cap cut the last-rendered epistemic section down to
    # 1 of its 5 entries).
    synth_header = (
        "# YOUR TRAJECTORY\n\n"
        "Last computed: 2026-07-19T16:58Z. Day 141. Window: last 10 sessions / 14 days.\n"
    )
    outcomes_sec = "## Recent session outcomes (last 10)\n" + "\n".join(
        f"- day-{130 + i}: 3 planned / 3 committed, 0 reverted, evaluator PASS x3" for i in range(10)
    )
    tasks_sec = "## Per-task activity (git log, last 14 days)\n" + "\n".join(
        f"- Day {130 + i}: task committed — improve subsystem {i} (src/commands_foo.rs)"
        for i in range(8)
    )
    reverts_sec = "## Reverts in window\n- 1 revert across 10 sessions (day-135, task 2)"
    ci_sec = "## Recurring CI error fingerprints\n" + "\n".join(
        f"- 3x: error[E030{i}]: mismatched types in src/commands_bar.rs" for i in range(5)
    )
    provider_sec = "## Provider/API health\n10 sessions, 2 provider error hit(s) in audit.jsonl."
    epistemic_first = "- src/commands.rs (5.0) — predicted 23×, never graded; stale (7 snapshots)"
    epistemic_sec = render_epistemic(
        [
            epistemic_first,
            "- src/commands_spawn.rs (5.0) — predicted 13×, never graded",
            "- src/watch.rs (3.0) — stale (7 snapshots)",
        ]
    )
    # The concentration gate renders in main() immediately after reverts, so the
    # synthetic block must place it there too — its cap survival depends on that
    # position, not on its size (Day 145: sacrifice order == render order).
    concentration_sec = render_subsystem_concentration(
        [("risk", 7), ("repl", 3), ("setup", 2), ("git", 1), ("help", 1)], 14
    )
    synth = (
        synth_header
        + "\n"
        + "\n\n".join(
            [
                outcomes_sec,
                tasks_sec,
                reverts_sec,
                concentration_sec,
                ci_sec,
                provider_sec,
                epistemic_sec,
            ]
        )
        + "\n"
    )

    # 20. Realistic full-length output fits the new cap with the epistemic
    # section (header AND first entry) fully intact — the steering channel
    # survives.
    capped = cap_output(synth)
    assert_true(
        "capped output stays under TOTAL_BYTE_CAP",
        len(capped.encode("utf-8")) <= TOTAL_BYTE_CAP,
    )
    assert_true(
        "epistemic header survives capping",
        "## Epistemic blind spots" in capped,
    )
    assert_true(
        "first epistemic entry survives capping",
        epistemic_first in capped,
    )

    # 21. Pin why the cap was raised: under the OLD 2048-byte cap the same
    # realistic output loses the epistemic entries (the observed Day 141 bug).
    old_capped = cap_output(synth, byte_cap=2048)
    assert_true(
        "old 2048 cap would truncate (regression rationale)",
        "(truncated to fit token budget)" in old_capped,
    )

    # 22a. The concentration gate survives capping — header, the top row, AND
    # the warning line. A gate the planner only sees on short days is not a gate.
    assert_true(
        "concentration header survives capping",
        "## Subsystem concentration" in capped,
    )
    assert_true(
        "concentration top row survives capping",
        "risk: 7/14" in capped,
    )
    assert_true(
        "concentration warning survives capping",
        "took 7 of the last 14 self-driven diffs" in capped,
    )

    # 22. Oversized input still gets the marker and stays under the cap
    huge = synth + ("\n## Padding\n" + "z" * 4000)
    huge_capped = cap_output(huge)
    assert_true(
        "oversized input capped with marker",
        len(huge_capped.encode("utf-8")) <= TOTAL_BYTE_CAP
        and "(truncated to fit token budget)" in huge_capped,
    )

    # --- planner-fallback (unchosen session) self-tests ---
    print("\n=== planner fallback self-tests ===\n")

    # The whole reason a hand-typed copy of evolve.sh's title is safe to keep:
    # this guard fails loudly if the two ever drift. evolve.sh is a sibling of
    # this script. A guard that silently passes when it cannot check is the
    # fail-silent shape I keep shipping, so a missing/unreadable file is a FAIL.
    evolve_sh = Path(__file__).resolve().parent / "evolve.sh"
    try:
        evolve_src = evolve_sh.read_text()
        drift_ok = FALLBACK_TASK_TITLE in evolve_src
    except OSError as e:
        print(f"  (could not read {evolve_sh}: {e})")
        drift_ok = False
    assert_true("test_fallback_title_matches_evolve_sh", drift_ok)

    FALLBACK_TASKS = [
        (152, FALLBACK_TASK_TITLE),
        (152, "Give my degraded mode a signature"),
        (151, FALLBACK_TASK_TITLE),
        (150, "DREAM chosen experiment — point the blind-spot map elsewhere"),
    ]

    unchosen = render_unchosen_sessions(FALLBACK_TASKS)
    assert_true(
        "fallback count and last day are right",
        "2 of the last 4 self-driven task commits" in unchosen
        and "last: day-152" in unchosen,
    )
    assert_true(
        "real tasks are not counted as fallbacks",
        "Give my degraded mode" not in unchosen
        and "DREAM chosen experiment" not in unchosen,
    )
    assert_eq(
        "zero fallbacks renders nothing at all",
        render_unchosen_sessions(
            [(151, "A real chosen task"), (150, "Another real one")]
        ),
        "",
    )
    assert_eq("no task commits renders nothing", render_unchosen_sessions([]), "")

    rows = render_task_success(FALLBACK_TASKS)
    assert_true(
        "fallback row is annotated in per-task activity",
        f'"{FALLBACK_TASK_TITLE}": 2 attempt(s), last day-152'
        "  ← planner fallback, no task was chosen" in rows,
    )
    assert_true(
        "chosen rows carry no fallback annotation",
        all(
            "planner fallback" not in line
            for line in rows.splitlines()
            if FALLBACK_TASK_TITLE not in line
        ),
    )

    # --- Reverts in window (per-task resets + whole-session commits) ---------
    print("\n=== revert counting self-tests ===\n")

    def outcome(attempted: int, succeeded: int, reverted: bool = False) -> dict:
        return {
            "tasks_attempted": attempted,
            "tasks_succeeded": succeeded,
            "reverted": reverted,
        }

    GREEN = [outcome(2, 2), outcome(1, 1), outcome(3, 3)]

    # 1. Nothing reverted at all: the all-clear must say what was CHECKED.
    #    A bare "0 reverts" is the sentence that misled the planner for months.
    assert_eq(
        "zero reverts names both signals",
        render_reverts(0, GREEN),
        "## Reverts in window\n"
        "0 task reverts in last ~3 sessions, "
        f"0 whole-session revert commits in {WINDOW_DAYS} days.",
    )

    # 2. Per-task resets only — invisible to REVERT_COMMIT_RE, visible here.
    PER_TASK = [outcome(2, 1), outcome(1, 1), outcome(2, 0)]
    assert_eq(
        "per-task resets are counted and named",
        render_reverts(0, PER_TASK),
        "## Reverts in window\n"
        "3 task(s) reverted across 2 of the last ~3 sessions "
        "(per-task resets, no commit).\n"
        f"0 whole-session revert commit(s) in last {WINDOW_DAYS} days.",
    )

    # 3. Whole-session revert commit only — the old signal, still reported,
    #    still named as its own thing.
    assert_eq(
        "whole-session commits reported separately",
        render_reverts(1, GREEN),
        "## Reverts in window\n"
        "0 task reverts in last ~3 sessions.\n"
        f"1 whole-session revert commit(s) in last {WINDOW_DAYS} days.",
    )

    # 4. Both — the two numbers must never be summed into one.
    assert_eq(
        "both signals render as two distinct lines",
        render_reverts(2, PER_TASK),
        "## Reverts in window\n"
        "3 task(s) reverted across 2 of the last ~3 sessions "
        "(per-task resets, no commit).\n"
        f"2 whole-session revert commit(s) in last {WINDOW_DAYS} days.",
    )

    # 5. A whole-session revert counts as a session with reverts even when the
    #    per-task counters agree (absence gets its own value, Day 144).
    assert_eq(
        "whole-session flag counts the session",
        count_task_reverts([outcome(1, 1, reverted=True), outcome(1, 1)]),
        (0, 1),
    )

    # 6. Defensive reads: the JSON is written by a shell script.
    assert_eq(
        "junk counters degrade to zero, never crash",
        count_task_reverts(
            [{"tasks_attempted": "two", "tasks_succeeded": None}, {}]
        ),
        (0, 0),
    )
    assert_eq(
        "succeeded > attempted never goes negative",
        count_task_reverts([outcome(1, 3)]),
        (0, 0),
    )

    # 7. No outcomes at all → no section (the section can't speak about nothing).
    assert_eq("no outcomes renders nothing", render_reverts(0, []), "")

    # 8. The section stays inside its 3-line budget under TOTAL_LINE_CAP.
    assert_true(
        "section is at most 3 lines",
        max(
            len(render_reverts(r, o).splitlines())
            for r, o in ((0, GREEN), (0, PER_TASK), (1, GREEN), (2, PER_TASK))
        )
        <= 3,
    )

    # --- CI section: age filter + three render worlds ----------------------
    # The defect: `gh run list --status failure --limit N` has no date filter,
    # so the N most recent failures EVER were rendered under a header claiming
    # they were "in window". Four-day-old resolved history read as current
    # state, and the healthier the loop gets the staler the section becomes.
    print("\n=== CI age filter / render self-tests ===\n")

    NOW = datetime(2026, 8, 22, 12, 0, tzinfo=timezone.utc)

    def run_at(stamp, run_id="1"):
        return {"databaseId": run_id, "createdAt": stamp}

    # 1. run_age_days: a real stamp, both Z and offset spellings.
    assert_eq(
        "age of a 4-day-old run",
        f"{run_age_days('2026-08-18T12:00:00Z', NOW):.2f}",
        "4.00",
    )
    assert_eq(
        "offset-form stamp parses the same",
        f"{run_age_days('2026-08-18T12:00:00+00:00', NOW):.2f}",
        "4.00",
    )
    # 2. Absence gets its own value — never 0.0, which would read as "just now".
    for label, bad in (
        ("missing stamp", None),
        ("empty stamp", ""),
        ("junk stamp", "not-a-date"),
        ("non-string stamp", 12345),
    ):
        assert_eq(f"{label} → None, not 0", repr(run_age_days(bad, NOW)), "None")

    # 3. partition_failed_runs: three states, none folded into another.
    in_w, old_w, undated = partition_failed_runs(
        [
            run_at("2026-08-18T12:00:00Z", "in"),      # 4d
            run_at("2026-08-01T12:00:00Z", "old"),     # 21d
            run_at(None, "undated"),
        ],
        NOW,
    )
    assert_eq("one run in window", str([r["databaseId"] for r, _ in in_w]), "['in']")
    assert_eq("one run too old", str([r["databaseId"] for r, _ in old_w]), "['old']")
    assert_eq(
        "one run undated (counted, not dropped)",
        str([r["databaseId"] for r, _ in undated]),
        "['undated']",
    )

    # 4. Boundary discriminator — BOTH sides pinned. A check tested only where
    #    it blocks is vacuously green.
    exactly = datetime(2026, 8, 22, 12, 0, tzinfo=timezone.utc)
    at_edge, past_edge, _ = partition_failed_runs(
        [
            run_at("2026-08-08T12:00:00Z", "edge"),        # exactly 14d
            run_at("2026-08-08T11:59:00Z", "just-over"),   # 14d + 1min
        ],
        exactly,
    )
    assert_eq(
        f"exactly {WINDOW_DAYS}d is INSIDE the window (inclusive)",
        str([r["databaseId"] for r, _ in at_edge]),
        "['edge']",
    )
    assert_eq(
        f"a minute past {WINDOW_DAYS}d is outside",
        str([r["databaseId"] for r, _ in past_edge]),
        "['just-over']",
    )

    # 5. World A — failures in window: rows carry age, header names the window.
    assert_eq(
        "in-window clusters render age-labelled rows",
        render_ci_errors(
            CiScan(
                ok=True,
                clusters=[("boom", ["1", "2"], 4.2), ("thud", ["3"], 0.3)],
                in_window=3,
            )
        ),
        f"## Recurring CI errors (failed runs, last {WINDOW_DAYS} days)\n"
        "[2×, last 4d ago] boom\n"
        "[1×, last <1d ago] thud",
    )

    # 6. World B — checked and clean. ONE line, and the excluded counts are
    #    reported rather than swallowed.
    assert_eq(
        "clean window says so out loud, naming what was excluded",
        render_ci_errors(CiScan(ok=True, excluded_old=3, undated=1)),
        f"## CI: no failed runs in last {WINDOW_DAYS} days "
        "(3 older failure(s) outside the window, not shown; 1 undated run(s) excluded)",
    )
    assert_true(
        "clean-case output is a single line (byte budget)",
        len(render_ci_errors(CiScan(ok=True, excluded_old=3)).splitlines()) == 1,
    )

    # 7. World C — could not check. Must NOT read as a clean bill of health.
    unknown = render_ci_errors(CiScan(ok=False, reason="gh run list failed (rc=1)"))
    assert_eq(
        "failed query names itself and refuses the clean reading",
        unknown,
        "## CI: could not check recent failures (gh run list failed (rc=1)) "
        "— this is NOT a clean bill of health",
    )
    assert_true(
        "the could-not-check line never claims zero failures",
        "no failed runs" not in unknown,
    )

    # 8. The sharp in-between: failures ARE in window but every log fetch died.
    #    Reporting "clean" here would be the exact lie this task is about.
    assert_eq(
        "in-window failures with unreadable logs are not 'clean'",
        render_ci_errors(CiScan(ok=True, in_window=2, fetch_errors=2)),
        f"## CI: 2 failed run(s) in last {WINDOW_DAYS} days, "
        "but no error lines could be read from their logs",
    )

    # 9. green_since_verdict — the fourth state the age partition never had:
    #    recent, real, and ALREADY REPAIRED. Three verdicts plus the near-miss.
    gnow = datetime(2026, 8, 25, 12, 0, 0, tzinfo=timezone.utc)
    fail_ts = "2026-08-23T12:00:00Z"

    assert_true(
        "no in-window failure to qualify -> None (that branch is untouched)",
        green_since_verdict(None, GreenScan(checked=True), gnow) is None,
    )

    # A success strictly NEWER than the newest failure: the whole point.
    green_v = green_since_verdict(
        fail_ts, GreenScan(newest_success_ts="2026-08-25T06:00:00Z", checked=True), gnow
    )
    assert_true(
        "a later success reports CI has gone green since",
        "gone green since" in green_v and "predates it" in green_v,
    )
    assert_true(
        "the green verdict names its own limit rather than implying repair",
        "Not proof the causes are fixed" in green_v and "flaky" in green_v,
    )

    # An OLDER success: the failures really are live, and we say so.
    assert_eq(
        "an older success means the failures below are still live",
        green_since_verdict(
            fail_ts,
            GreenScan(newest_success_ts="2026-08-22T12:00:00Z", checked=True),
            gnow,
        ),
        "no successful run has landed since the newest failure below — these are live",
    )
    # Checked, and there are genuinely zero successful runs. Real observation.
    assert_eq(
        "zero successful runs is a real answer, not a could-not-check",
        green_since_verdict(fail_ts, GreenScan(newest_success_ts=None, checked=True), gnow),
        "no successful run has landed since the newest failure below — these are live",
    )

    # THE NEAR MISS: equal timestamps. Strict >, so a tie is NOT green-since.
    # A discriminator tested only on the side that fires is vacuous green.
    assert_eq(
        "a success stamped exactly at the newest failure is NOT green-since",
        green_since_verdict(fail_ts, GreenScan(newest_success_ts=fail_ts, checked=True), gnow),
        "no successful run has landed since the newest failure below — these are live",
    )

    # 10. checked=False must never render as green and never as "no successes".
    unchecked = green_since_verdict(fail_ts, GreenScan(checked=False), gnow)
    assert_true(
        "could-not-run says so and claims neither live nor cured",
        "could not run" in unchecked
        and "neither" in unchecked
        and "gone green" not in unchecked,
    )
    assert_true(
        "a None green scan is could-not-run, never a silent 'still live'",
        "could not run" in (green_since_verdict(fail_ts, None, gnow) or ""),
    )
    # An unparseable stamp on either side is an unknown, not a confident answer.
    assert_true(
        "an unparseable success stamp degrades to could-not-run",
        "could not run"
        in green_since_verdict(fail_ts, GreenScan(newest_success_ts="not-a-date", checked=True), gnow),
    )

    # 11. Emission point: the string the planner actually receives. A
    #     could-not-check scan must carry no claim of greenness anywhere.
    cured = CiScan(
        ok=True,
        clusters=[("boom", ["1", "2"], 0.9)],
        in_window=2,
        newest_failure_ts=fail_ts,
    )
    rendered_unchecked = render_ci_errors(cured, GreenScan(checked=False), gnow)
    assert_true(
        "rendered could-not-check section makes no greenness claim",
        "gone green" not in rendered_unchecked and "could not run" in rendered_unchecked,
    )
    assert_true(
        "the verdict sits under the header, above the cluster rows",
        rendered_unchecked.splitlines()[0].startswith("## Recurring CI errors")
        and "could not run" in rendered_unchecked.splitlines()[1]
        and rendered_unchecked.splitlines()[2].startswith("[2×,"),
    )
    assert_true(
        "no cluster is dropped, filtered or re-ranked by the new line",
        "boom" in rendered_unchecked,
    )
    # And with a real later success, the same rows render under a green verdict.
    rendered_green = render_ci_errors(
        cured, GreenScan(newest_success_ts="2026-08-25T06:00:00Z", checked=True), gnow
    )
    assert_true(
        "green-since verdict renders above the same untouched rows",
        "gone green since" in rendered_green and "[2×, last <1d ago] boom" in rendered_green,
    )
    # A scan carrying no failure anchor renders byte-identically to before:
    # nothing to qualify means nothing to say.
    assert_eq(
        "no failure anchor -> section is byte-identical to the pre-change render",
        render_ci_errors(
            CiScan(ok=True, clusters=[("boom", ["1", "2"], 0.9)], in_window=2)
        ),
        f"## Recurring CI errors (failed runs, last {WINDOW_DAYS} days)\n"
        "[2×, last <1d ago] boom",
    )

    # 11. newest_success_from_runs — the QUERY-shaped half of the green-since
    #     probe. Section 9 above drives `green_since_verdict` with fabricated
    #     timestamps, so it stayed green over a probe that asked the wrong
    #     question entirely (any workflow, and position 0 of a `--limit 1`).
    #     Survivors follow the assertion; the assertion stopped at the function
    #     boundary I found convenient.
    def _ok(ts):
        return {"createdAt": ts, "conclusion": "success", "workflowName": "CI"}

    assert_eq(
        "newest is taken by max(), NOT position 0 — the `--limit 1` shape",
        newest_success_from_runs(
            [
                _ok("2026-07-24T18:32:13Z"),   # stale row first
                _ok("2026-08-25T14:43:20Z"),   # the real newest
                _ok("2026-08-25T11:17:13Z"),
            ],
            10,
        ).newest_success_ts,
        "2026-08-25T14:43:20Z",
    )
    # `--status completed` returns every conclusion, so the success filter moved
    # client-side. A NEWER failure must not win: the filter is the whole point.
    mixed = newest_success_from_runs(
        [
            {"createdAt": "2026-08-26T09:00:00Z", "conclusion": "failure"},
            {"createdAt": "2026-08-26T08:00:00Z", "conclusion": "cancelled"},
            _ok("2026-08-26T05:10:56Z"),
            _ok("2026-08-25T23:54:38Z"),
        ],
        10,
    )
    assert_true(
        "only conclusion=success rows count — a newer failure does not win",
        mixed.checked and mixed.newest_success_ts == "2026-08-26T05:10:56Z",
    )
    # The new third state. A FULL page with no success means the probe could not
    # see far enough — never "CI has never been green", which would be a
    # STRONGER false alarm than the stale page this fix removes.
    full_no_success = newest_success_from_runs(
        [{"createdAt": f"2026-08-2{i}T00:00:00Z", "conclusion": "failure"} for i in range(4)],
        4,
    )
    assert_true(
        "FULL page with zero successes -> checked=False (cannot see far enough)",
        full_no_success.checked is False and full_no_success.newest_success_ts is None,
    )
    # The near-miss guard: a discriminator tested only on the side that fires is
    # vacuous green. A SHORT page means the whole listing was seen, so "no
    # success exists" is a real observation.
    short_no_success = newest_success_from_runs(
        [{"createdAt": "2026-08-20T00:00:00Z", "conclusion": "failure"}],
        4,
    )
    assert_true(
        "SHORT page with zero successes -> checked=True, ts=None (genuinely none)",
        short_no_success.checked is True and short_no_success.newest_success_ts is None,
    )
    skipped_missing = newest_success_from_runs(
        [
            {"conclusion": "success", "workflowName": "CI"},
            _ok("2026-08-25T14:43:20Z"),
        ],
        10,
    )
    assert_true(
        "a success row with no createdAt is skipped while a good row still wins",
        skipped_missing.checked
        and skipped_missing.newest_success_ts == "2026-08-25T14:43:20Z",
    )
    skipped_bad = newest_success_from_runs(
        [{"createdAt": "not-a-date", "conclusion": "success"}, _ok("2026-08-25T14:43:20Z")],
        10,
    )
    assert_true(
        "an unparseable createdAt is skipped the same way",
        skipped_bad.checked
        and skipped_bad.newest_success_ts == "2026-08-25T14:43:20Z",
    )
    all_bad = newest_success_from_runs(
        [{"createdAt": "nope", "conclusion": "success"}, {"x": 1}], 10
    )
    assert_true(
        "rows present but NONE readable -> checked=False, not 'no successes'",
        all_bad.checked is False and all_bad.newest_success_ts is None,
    )
    # An empty list is a real answer, not a failure to check — and it is the
    # shortest possible short page, so it lands in the same honest branch.
    empty = newest_success_from_runs([], 10)
    assert_true(
        "genuine empty list -> ts=None, checked=True (real observation)",
        empty.checked is True and empty.newest_success_ts is None,
    )
    assert_true(
        "a non-list payload cannot be asked -> checked=False",
        newest_success_from_runs({"createdAt": "2026-08-25T14:43:20Z"}, 10).checked is False,
    )

    # 12. argv shape. DELIBERATELY WEAK: this proves the workflow filter is
    #     PRESENT in the command, never that the answer it returns is right —
    #     "could not check" must not read as "checked; clean". It exists because
    #     the shipped bug was invisible at every other layer: the probe asked
    #     `gh` for `workflowName` and never read it, so a `Sponsors Refresh` run
    #     (~every 40 min) answered "has CI gone green?" by accident.
    probe_argv = green_probe_argv("owner/repo")
    assert_true(
        "green probe asks the CI workflow specifically",
        "--workflow" in probe_argv and CI_WORKFLOW_FILE in probe_argv,
    )
    assert_true(
        "green probe does not depend on a single row (--limit is not 1)",
        probe_argv[probe_argv.index("--limit") + 1] != "1"
        and int(probe_argv[probe_argv.index("--limit") + 1]) == GREEN_PROBE_LIMIT,
    )
    assert_true(
        "green probe asks for completed runs and filters success CLIENT-side",
        probe_argv[probe_argv.index("--status") + 1] == "completed"
        and "success" not in probe_argv
        and "conclusion" in probe_argv[probe_argv.index("--json") + 1],
    )
    fail_argv = failed_runs_argv("owner/repo")
    assert_true(
        "failure scan is filtered to CI too — the header says 'Recurring CI errors'",
        "--workflow" in fail_argv and CI_WORKFLOW_FILE in fail_argv,
    )
    assert_true(
        "both queries still name the repo they were handed",
        "owner/repo" in probe_argv and "owner/repo" in fail_argv,
    )

    # 13. The RECEIPT. This probe has been declared fixed four times and graded
    #     zero times: every fix was verified by hand-running it in the session
    #     that wrote it, which verifies the CHANNEL and not the consumer. On Day
    #     179 at 21:30 the block said "these are live" over cured failures while
    #     a hand-run three minutes later said green — and which branch fired is
    #     unrecoverable, because nothing recorded it. These tests pin the one
    #     fact that was missing.
    print("\n=== green_probe_receipt self-tests ===\n")
    rnow = datetime(2026, 8, 26, 12, 0, tzinfo=timezone.utc)
    rscan = CiScan(
        ok=True,
        clusters=[("boom", ["1", "2"], 0.5)],
        in_window=3,
        excluded_old=2,
        undated=1,
        newest_failure_ts="2026-08-26T06:00:00Z",
    )
    # Each named branch, driven through the SAME function the renderer consults,
    # so a receipt can never disagree with the sentence beside it.
    branch_cases = [
        (
            GREEN_BRANCH_GONE_GREEN,
            rscan.newest_failure_ts,
            GreenScan(newest_success_ts="2026-08-26T09:00:00Z", checked=True),
        ),
        (
            GREEN_BRANCH_STILL_LIVE,
            rscan.newest_failure_ts,
            GreenScan(newest_success_ts="2026-08-25T09:00:00Z", checked=True),
        ),
        (GREEN_BRANCH_COULD_NOT_CHECK, rscan.newest_failure_ts, GreenScan(checked=False)),
        (GREEN_BRANCH_COULD_NOT_CHECK, rscan.newest_failure_ts, None),
        (GREEN_BRANCH_NO_FAILURES, None, GreenScan(checked=True)),
    ]
    for want_branch, fail_ts, gscan in branch_cases:
        got_branch = green_verdict_branch(fail_ts, gscan, rnow)
        assert_eq(f"branch fires: {want_branch}", got_branch, want_branch)
        line = green_probe_receipt(rscan, gscan, got_branch, GREEN_PROBE_LIMIT)
        assert_true(
            f"receipt names branch={want_branch} verbatim",
            f"branch={want_branch} " in line,
        )
    # A tie goes to still-live (strict comparison), the conservative direction.
    assert_eq(
        "success stamped exactly at the newest failure is NOT green-since",
        green_verdict_branch(
            "2026-08-26T06:00:00Z",
            GreenScan(newest_success_ts="2026-08-26T06:00:00Z", checked=True),
            rnow,
        ),
        GREEN_BRANCH_STILL_LIVE,
    )
    # The branch decision and the rendered sentence must stay welded: every
    # branch except no-in-window-failures produces prose, and that one alone
    # produces None.
    for want_branch, fail_ts, gscan in branch_cases:
        sentence = green_since_verdict(fail_ts, gscan, rnow)
        assert_true(
            f"verdict prose matches branch {want_branch}",
            (sentence is None) == (want_branch == GREEN_BRANCH_NO_FAILURES),
        )

    # short vs full page, IN THE RECEIPT — a discriminator tested only on the
    # side that fires is vacuous green, and folding these two is the strongest
    # false alarm this probe can produce ("could not see far enough" reading as
    # "there is nothing there").
    full_scan = newest_success_from_runs(
        [{"createdAt": f"2026-08-2{i}T00:00:00Z", "conclusion": "failure"} for i in range(4)],
        4,
    )
    short_scan = newest_success_from_runs(
        [{"createdAt": "2026-08-20T00:00:00Z", "conclusion": "failure"}], 4
    )
    assert_true(
        "receipt reports page=full when the probe could not see far enough",
        "page=full " in green_probe_receipt(rscan, full_scan, GREEN_BRANCH_COULD_NOT_CHECK, 4)
        and "green_rows=4 " in green_probe_receipt(
            rscan, full_scan, GREEN_BRANCH_COULD_NOT_CHECK, 4
        ),
    )
    assert_true(
        "receipt reports page=short when the whole listing was seen",
        "page=short " in green_probe_receipt(rscan, short_scan, GREEN_BRANCH_STILL_LIVE, 4)
        and "green_rows=1 " in green_probe_receipt(
            rscan, short_scan, GREEN_BRANCH_STILL_LIVE, 4
        ),
    )
    # rows=None (payload was not a list) must NOT read as a short page: nobody
    # looked is not "the listing was exhausted".
    assert_true(
        "a non-list payload reports green_rows=unknown page=n/a, never short",
        "green_rows=unknown " in green_probe_receipt(
            rscan, newest_success_from_runs({"a": 1}, 4), GREEN_BRANCH_COULD_NOT_CHECK, 4
        )
        and "page=n/a " in green_probe_receipt(
            rscan, newest_success_from_runs({"a": 1}, 4), GREEN_BRANCH_COULD_NOT_CHECK, 4
        ),
    )
    # The cost guard (`ci_scan.ok and ci_scan.clusters`) means green is None on a
    # healthy session. That is "not asked", which is not "no successes".
    not_asked = green_probe_receipt(rscan, None, GREEN_BRANCH_COULD_NOT_CHECK, 20)
    assert_true(
        "green=None reports not-asked rather than fabricating a zero",
        "green_rows=not-asked " in not_asked and "newest_success=not-asked " in not_asked,
    )
    counted = newest_success_from_runs(
        [
            {"createdAt": "2026-08-26T05:00:00Z", "conclusion": "success"},
            {"createdAt": "2026-08-25T05:00:00Z", "conclusion": "success"},
            {"createdAt": "2026-08-24T05:00:00Z", "conclusion": "failure"},
            {"conclusion": None},
        ],
        20,
    )
    assert_true(
        "receipt counts successes and unreadable rows separately",
        "successes=2 " in green_probe_receipt(rscan, counted, GREEN_BRANCH_GONE_GREEN, 20)
        and "unreadable=1 " in green_probe_receipt(
            rscan, counted, GREEN_BRANCH_GONE_GREEN, 20
        ),
    )
    assert_true(
        "receipt is exactly ONE line — it shares head -20 with warn() diagnostics",
        all(
            len(green_probe_receipt(rscan, g, b, 20).splitlines()) == 1
            for b, _f, g in branch_cases
        ),
    )
    assert_true(
        "receipt carries the failure side's own partition, not just the green side",
        "newest_failure=2026-08-26T06:00:00Z " in not_asked
        and "failures_in_window=3 " in not_asked
        and "too_old=2 " in not_asked
        and "undated=1" in not_asked,
    )

    # 13b. STALE PAGE. The fifth touch on this probe and the first that is a
    #      DETECTOR rather than a correctness fix: four fixes chased the API's
    #      behaviour and lost, so this one only refuses to assert a verdict the
    #      data cannot support. Costs zero extra API calls — both queries name
    #      the same workflow and a failure IS a completed run, so the newest
    #      failure already in hand must appear on a fresh completed page.
    print("\n=== page_is_stale / stale-page branch self-tests ===\n")
    # The harness's real numbers, 2026-08-27T07:19Z: a full page of 20 whose
    # newest row was FOUR HOURS older than a failure the same run had fetched.
    harness_row, harness_fail = "2026-08-25T23:54:38Z", "2026-08-26T03:49:23Z"
    # The hand-run's numbers three minutes later, which said gone-green.
    handrun_row = "2026-08-27T03:23:45Z"
    assert_true(
        "the harness's own impossible page is detected as stale",
        page_is_stale(harness_row, harness_fail),
    )
    assert_true(
        "NEAR-MISS: the hand-run's fresh page is not stale",
        not page_is_stale(handrun_row, harness_fail),
    )
    assert_true(
        "BOUNDARY: newest row exactly at the newest failure is fresh enough",
        not page_is_stale(harness_fail, harness_fail),
    )
    for label, row_ts, fail_ts in [
        ("row missing", None, harness_fail),
        ("failure missing", harness_row, None),
        ("row unparseable", "not-a-date", harness_fail),
        ("failure unparseable", harness_row, "not-a-date"),
        ("both missing", None, None),
    ]:
        assert_true(
            f"unknown is not stale ({label}) — do not invent a defect",
            not page_is_stale(row_ts, fail_ts),
        )
    # Now the branch, driven through the SAME function the renderer consults.
    stale_scan = GreenScan(
        newest_success_ts="2026-08-25T20:00:00Z",
        checked=True,
        rows=20,
        successes=19,
        newest_row_ts=harness_row,
    )
    assert_eq(
        "the harness reading renders stale-page, NOT still-live",
        green_verdict_branch(harness_fail, stale_scan, rnow),
        GREEN_BRANCH_STALE_PAGE,
    )
    # NEAR-MISS GUARD: a discriminator tested only on the side that fires is
    # vacuous green, and this one must not start eating healthy readings.
    fresh_scan = GreenScan(
        newest_success_ts=handrun_row, checked=True, rows=20, successes=17,
        newest_row_ts=handrun_row,
    )
    assert_eq(
        "the hand-run reading still renders gone-green",
        green_verdict_branch(harness_fail, fresh_scan, rnow),
        GREEN_BRANCH_GONE_GREEN,
    )
    # Staleness must NOT pre-empt gone-green: if a success on the page is
    # already newer than the newest failure, the answer is known.
    assert_eq(
        "a success newer than the failure wins even on an otherwise stale page",
        green_verdict_branch(
            harness_fail,
            GreenScan(
                newest_success_ts="2026-08-26T09:00:00Z", checked=True, rows=20,
                newest_row_ts=harness_row,
            ),
            rnow,
        ),
        GREEN_BRANCH_GONE_GREEN,
    )
    # The genuinely-red path is the one thing that must not regress: a page
    # whose newest row is NEWER than the newest failure but holds no success.
    genuinely_red = GreenScan(
        newest_success_ts="2026-08-20T00:00:00Z", checked=True, rows=20, successes=1,
        newest_row_ts="2026-08-26T10:00:00Z",
    )
    assert_eq(
        "a fresh page with no newer success is STILL still-live",
        green_verdict_branch(harness_fail, genuinely_red, rnow),
        GREEN_BRANCH_STILL_LIVE,
    )
    assert_eq(
        "a scan with no newest_row_ts at all is still-live, byte-identical to before",
        green_verdict_branch(
            harness_fail,
            GreenScan(newest_success_ts="2026-08-20T00:00:00Z", checked=True),
            rnow,
        ),
        GREEN_BRANCH_STILL_LIVE,
    )
    # could-not-check and no-in-window-failures are untouched by the new branch.
    assert_eq(
        "an unchecked scan is could-not-check even when its page looks stale",
        green_verdict_branch(
            harness_fail, GreenScan(checked=False, newest_row_ts=harness_row), rnow
        ),
        GREEN_BRANCH_COULD_NOT_CHECK,
    )
    assert_eq(
        "no in-window failure still short-circuits before any freshness question",
        green_verdict_branch(None, stale_scan, rnow),
        GREEN_BRANCH_NO_FAILURES,
    )
    # The sentence must claim neither red nor green, and must name both stamps.
    stale_sentence = green_since_verdict(harness_fail, stale_scan, rnow)
    assert_true(
        "stale-page prose names both timestamps it compared",
        harness_row in stale_sentence and harness_fail in stale_sentence,
    )
    assert_true(
        "stale-page prose claims neither live nor cured",
        "could not be determined" in stale_sentence
        and "these are live" not in stale_sentence
        and "gone green" not in stale_sentence,
    )
    # The receipt is the grader: it must carry the input the new decision reads.
    stale_receipt = green_probe_receipt(
        rscan, stale_scan, GREEN_BRANCH_STALE_PAGE, GREEN_PROBE_LIMIT
    )
    assert_true(
        "receipt names branch=stale-page and the newest_row it read",
        f"branch={GREEN_BRANCH_STALE_PAGE} " in stale_receipt
        and f"newest_row={harness_row} " in stale_receipt,
    )
    assert_true(
        "receipt reports newest_row=none / not-asked rather than fabricating a stamp",
        "newest_row=none " in green_probe_receipt(
            rscan, GreenScan(checked=True), GREEN_BRANCH_STILL_LIVE, 20
        )
        and "newest_row=not-asked " in green_probe_receipt(
            rscan, None, GREEN_BRANCH_COULD_NOT_CHECK, 20
        ),
    )
    assert_true(
        "receipt stays exactly ONE line with the new field",
        len(stale_receipt.splitlines()) == 1,
    )
    # The freshness evidence comes from EVERY parseable row, not only successes:
    # a page of pure failures still knows how fresh it is.
    all_failures = newest_success_from_runs(
        [
            {"createdAt": "2026-08-24T00:00:00Z", "conclusion": "failure"},
            {"createdAt": "2026-08-25T23:54:38Z", "conclusion": "failure"},
            {"createdAt": "2026-08-23T00:00:00Z", "conclusion": "cancelled"},
        ],
        20,
    )
    assert_eq(
        "newest_row_ts is the newest row of ANY conclusion",
        all_failures.newest_row_ts,
        harness_row,
    )
    assert_true(
        "collecting newest_row_ts did not disturb the success filter",
        all_failures.newest_success_ts is None and all_failures.successes == 0,
    )
    mixed = newest_success_from_runs(
        [
            {"createdAt": "2026-08-26T10:00:00Z", "conclusion": "failure"},
            {"createdAt": "2026-08-25T05:00:00Z", "conclusion": "success"},
            {"createdAt": "bogus", "conclusion": "success"},
        ],
        20,
    )
    assert_true(
        "newest_row_ts and newest_success_ts are separate facts",
        mixed.newest_row_ts == "2026-08-26T10:00:00Z"
        and mixed.newest_success_ts == "2026-08-25T05:00:00Z"
        and mixed.unreadable == 1,
    )
    assert_true(
        "an unreadable-only page reports no newest_row_ts, never a guess",
        newest_success_from_runs([{"conclusion": "failure"}], 20).newest_row_ts is None
        and newest_success_from_runs({"a": 1}, 4).newest_row_ts is None,
    )

    # 14. The rendered block must not move by ONE BYTE. TOTAL_LINE_CAP /
    #     TOTAL_BYTE_CAP are tight and the epistemic section renders last (it has
    #     already been truncated away once, Day 142), so the receipt belongs in
    #     the audit stream and never in the planner's budget. These five strings
    #     were captured by running the PRE-change code.
    print("\n=== rendered block is byte-identical (receipt is stderr-only) ===\n")
    hdr = (
        "## Recurring CI errors (failed runs, last 14 days) (2 older failure(s) "
        "outside the window, not shown; 1 undated run(s) excluded)"
    )
    rows = "[2×, last <1d ago] boom\n[1×, last 1d ago] bang"
    bscan = CiScan(
        ok=True,
        clusters=[("boom", ["1", "2"], 0.5), ("bang", ["3"], 1.2)],
        in_window=3,
        excluded_old=2,
        undated=1,
        newest_failure_ts="2026-08-26T06:00:00Z",
    )
    assert_eq(
        "gone-green render unchanged",
        render_ci_errors(
            bscan, GreenScan(newest_success_ts="2026-08-26T09:00:00Z", checked=True), rnow
        ),
        f"{hdr}\nCI has gone green since (last <1d ago): every failure below predates "
        "it. Not proof the causes are fixed — a flaky test passes sometimes — only "
        f"that CI is not red on these patterns now.\n{rows}",
    )
    assert_eq(
        "still-live render unchanged",
        render_ci_errors(
            bscan, GreenScan(newest_success_ts="2026-08-25T09:00:00Z", checked=True), rnow
        ),
        f"{hdr}\nno successful run has landed since the newest failure below — "
        f"these are live\n{rows}",
    )
    could_not = (
        f"{hdr}\ngreen-since check could not run — this claims neither that the "
        f"failures below are live nor that they are cured\n{rows}"
    )
    assert_eq(
        "could-not-check render unchanged",
        render_ci_errors(bscan, GreenScan(checked=False), rnow),
        could_not,
    )
    assert_eq(
        "green=None render unchanged",
        render_ci_errors(bscan, None, rnow),
        could_not,
    )
    assert_eq(
        "stale-page render — the NEW branch, pinned like its four siblings",
        render_ci_errors(
            bscan,
            GreenScan(
                newest_success_ts="2026-08-25T09:00:00Z", checked=True, rows=20,
                newest_row_ts="2026-08-26T01:00:00Z",
            ),
            rnow,
        ),
        f"{hdr}\ngreen-since check could not run — the completed-runs listing's "
        "newest row (2026-08-26T01:00:00Z) predates a failure this same run already "
        "fetched (2026-08-26T06:00:00Z), so the page cannot be current and CI's "
        "state could not be determined. This claims neither that the failures below "
        f"are live nor that they are cured.\n{rows}",
    )
    assert_eq(
        "no-in-window-failure render unchanged (no verdict line at all)",
        render_ci_errors(
            CiScan(ok=True, clusters=[("boom", ["1"], 0.5)], in_window=1), None, rnow
        ),
        "## Recurring CI errors (failed runs, last 14 days)\n[1×, last <1d ago] boom",
    )

    # --- #843: audit-dir classification, three states, none folded ---
    print("\n=== classify_audit_dir / render_provider_health self-tests ===\n")

    def yes(_p: str) -> bool:
        return True

    def no(_p: str) -> bool:
        return False

    # State 1: unset. Nothing to scan — and NOT a misconfiguration report.
    assert_eq("unset env value classifies as UNSET", classify_audit_dir(None, yes, yes), AUDIT_DIR_UNSET)
    assert_eq("empty string classifies as UNSET", classify_audit_dir("", yes, yes), AUDIT_DIR_UNSET)
    assert_eq("whitespace-only classifies as UNSET", classify_audit_dir("   ", yes, yes), AUDIT_DIR_UNSET)

    # State 2: set but unusable. Two distinct causes, one state — and it must
    # NOT be reported as "unset", which would blame the wrong thing.
    assert_eq(
        "set but nonexistent classifies as UNUSABLE",
        classify_audit_dir("/tmp/nope", no, no), AUDIT_DIR_UNUSABLE,
    )
    assert_eq(
        "set but not a directory classifies as UNUSABLE (the /dev/null shape)",
        classify_audit_dir("/dev/null", yes, no), AUDIT_DIR_UNUSABLE,
    )

    # State 3: the harness path. Near-miss guard — a discriminator tested only
    # on the side that fires is vacuous green.
    assert_eq(
        "existing directory classifies as OK",
        classify_audit_dir("/tmp/sessions", yes, yes), AUDIT_DIR_OK,
    )

    # The rendered strings: one line each for the two refusals, and the OK path
    # byte-identical to before (whole-string compare, never a `contains`).
    assert_eq(
        "UNSET renders one honest line naming the variable",
        render_provider_health(0, 0, AUDIT_DIR_UNSET),
        "## Provider/API health: not checked — YOYO_AUDIT_DIR is unset, "
        "so there is no audit-log to scan (this is normal for a hand-run).",
    )
    assert_eq(
        "UNUSABLE renders a DIFFERENT line and refuses the clean-bill reading",
        render_provider_health(0, 0, AUDIT_DIR_UNUSABLE),
        "## Provider/API health: not checked — YOYO_AUDIT_DIR is set but is "
        "not a readable directory. This is not 'no provider errors'.",
    )
    # SUPERSEDED WORDING, recorded rather than erased (Day 196). These three
    # used to assert `... 3 provider error hit(s) in audit.jsonl.` and
    # `... no provider errors detected.`, and the second one is the sentence
    # this task exists to kill: it read as *the provider was fine* while 7
    # consecutive rate-limited sessions produced zero task commits. The
    # trailing ` in audit.jsonl` went with it, because with no `streams`
    # recorded the caller cannot show it opened that file — naming it anyway
    # is the same fabrication one clause over. Updated, not deleted: a fixture
    # pinning a superseded convention that outlives its replacement converts a
    # defect into a green invariant (Day 148).
    assert_eq(
        "OK with hits names no stream when the caller recorded none",
        render_provider_health(7, 3, AUDIT_DIR_OK),
        "## Provider/API health\n7 sessions, 3 provider error hit(s).",
    )
    assert_eq(
        "OK with no hits says what was READ, never that the provider was fine",
        render_provider_health(7, 0, AUDIT_DIR_OK),
        "## Provider/API health\n7 sessions, no provider-error lines.",
    )
    # SUPERSEDED BEHAVIOUR, recorded rather than erased (Day 196, second task).
    # This assertion read `render_provider_health(0, 0, AUDIT_DIR_OK) == ""`
    # from #843 until now, i.e. it pinned SILENCE for a walk that examined zero
    # sessions. That is this section's own subject wearing the opposite sign:
    # "I found no session dirs" and "I checked and there was nothing" rendered
    # identically, and the first is quieter than the bug. INVERTED rather than
    # deleted — a fixture pinning a superseded convention that outlives its
    # replacement converts a defect into a green invariant (Day 148).
    assert_eq(
        "ANTI-VACUOUS: zero sessions REFUSES rather than rendering silence",
        render_provider_health(0, 0, AUDIT_DIR_OK),
        "## Provider/API health: not checked — the audit-log directory holds "
        "no session directories, so nothing was scanned. This is not 'no "
        "provider errors'.",
    )
    assert_eq(
        "default state argument keeps every existing call site reachable",
        render_provider_health(4, 1), "## Provider/API health\n4 sessions, 1 provider error hit(s).",
    )

    # The I/O half must hand back NO path for the two refusals — the whole of
    # #843 is that there is no plausible-looking placeholder left to walk.
    st, p = resolve_audit_dir("")
    assert_eq("resolve: unset yields no path", f"{st}/{p}", f"{AUDIT_DIR_UNSET}/None")
    st, p = resolve_audit_dir("/dev/null")
    assert_eq("resolve: /dev/null yields no path", f"{st}/{p}", f"{AUDIT_DIR_UNUSABLE}/None")
    st, p = resolve_audit_dir("/tmp")
    assert_eq("resolve: a real directory yields that path", f"{st}/{p}", f"{AUDIT_DIR_OK}//tmp")

    # --- Provider-error detection (Day 196) -------------------------------
    # Pinned at the EMISSION POINT: every assertion below runs the real fold
    # (`scan_provider_lines`, the same one `_scan_provider_stream` runs over a
    # file) and then asserts the string `render_provider_health` returns.
    print("\n=== provider-error detection self-tests ===\n")

    # The three real shapes, lifted VERBATIM from run 34619700898's transcript.
    real_rate_limit = "  error: Rate limited, retry after Some(38965000)ms"
    real_giveup = (
        "  \u23f3 stopped retrying on purpose: the provider says its rate "
        "limit resets in ~10h 49m."
    )
    real_no_fallback = "  API error with no fallback configured. Exiting."

    # (2) ANTI-VACUOUS, ASSERTED FIRST. A scanner that finds nothing and
    # reports a clean bill is this very defect wearing the opposite sign, and
    # it is quieter than the bug — so prove the fixture really does fire
    # before any "must be zero" row below is allowed to mean anything.
    anti_hits, anti_prose, anti_unanchored = scan_provider_lines([real_rate_limit])
    assert_eq(
        "ANTI-VACUOUS: a genuine unquoted rate-limit line really is detected",
        f"hits={anti_hits} prose={anti_prose} unanchored={anti_unanchored}",
        "hits=1 prose=0 unanchored=0",
    )

    # (1) All three real shapes detected, each by its own marker, so a regex
    # that collapsed two of them into one cannot pass by luck.
    assert_eq(
        "the three real emissions each match their own marker",
        "/".join(
            str(classify_provider_error_line(x))
            for x in (real_rate_limit, real_giveup, real_no_fallback)
        ),
        "rate_limited/retry_giveup/no_fallback",
    )
    # ...and at the emission point, all three in one stream are reported.
    h, pr, ur = scan_provider_lines([real_rate_limit, real_giveup, real_no_fallback])
    assert_eq(
        "all three real shapes reach the rendered line as hits",
        render_provider_health(1, h, AUDIT_DIR_OK, pr, ("transcripts/*.log",), ur),
        "## Provider/API health\n1 sessions, 3 provider error hit(s) in "
        "transcripts/*.log.",
    )

    # (3) NEAR-MISS GUARD — my own prose is the false-positive population, and
    # it grows every session. The last line is lifted VERBATIM from this
    # task's own file, which is the case that makes the point: writing about
    # the defect must never register as the defect.
    prose_lines = [
        "| `error: Rate limited, retry after Some(38965000)ms` | 11x | dead |",
        "The log says `API error with no fallback configured. Exiting.` here.",
        "scripts/extract_trajectory.py:1171:error: Rate limited, retry after Some(1)ms",
        "> error: Rate limited, retry after Some(38965000)ms",
        "Each log carries 11x `429`, 6x `529`, 7x `API error with no fallback "
        "configured. Exiting.`",
    ]
    p_hits, p_prose, p_unanchored = scan_provider_lines(prose_lines)
    assert_eq(
        "NEAR-MISS: five prose shapes yield ZERO hits and are all COUNTED",
        f"hits={p_hits} prose={p_prose} unanchored={p_unanchored}",
        "hits=0 prose=5 unanchored=0",
    )
    assert_eq(
        "NEAR-MISS: the rendered line reports the rejects, never as hits",
        render_provider_health(
            1, p_hits, AUDIT_DIR_OK, p_prose, ("transcripts/*.log",), p_unanchored
        ),
        "## Provider/API health\n1 sessions, no provider-error lines in "
        "transcripts/*.log. (5 prose-shaped line(s) rejected)",
    )

    # (4) NEAR-MISS GUARD — a genuinely clean session set still renders the
    # healthy branch and claims NOTHING false. The old wording ("no provider
    # errors detected") is asserted ABSENT: that sentence is the defect.
    clean_hits, clean_prose, clean_unanchored = scan_provider_lines(
        ['{"tool":"read_file","success":true}', "Phase A1: Assessment (900s)..."]
    )
    clean_render = render_provider_health(
        10,
        clean_hits,
        AUDIT_DIR_OK,
        clean_prose,
        ("audit.jsonl", "transcripts/*.log"),
        clean_unanchored,
    )
    assert_eq(
        "NEAR-MISS: a clean set names the streams it read and nothing more",
        clean_render,
        "## Provider/API health\n10 sessions, no provider-error lines in "
        "audit.jsonl, transcripts/*.log.",
    )
    assert_eq(
        "NEAR-MISS: the superseded 'errors detected' claim is gone for good",
        str("no provider errors detected" in clean_render), "False",
    )

    # The prose filter must not eat a true positive: the three real shapes
    # carry no pipe, no backtick, no grep prefix and no blockquote.
    assert_eq(
        "the prose filter rejects none of the three real emissions",
        "/".join(
            str(is_provider_prose(x))
            for x in (real_rate_limit, real_giveup, real_no_fallback)
        ),
        "False/False/False",
    )

    # (5) The two AUDIT_DIR_* refusal branches keep their PRE-EXISTING,
    # UNEDITED assertions above — that is #843's regression guard, and it is
    # deliberately not re-asserted here (re-asserting a guarded property is
    # ceremony and inflates what a red would mean).

    # --- Usage-record coverage (#848 follow-up) ---------------------------
    # The predicate is `type == "usage"`, never "has a type": a line with NO
    # type key is a tool call by #848's own compatibility rule, so a file of
    # pure tool-call lines is ABSENT (the producer is silent) and must not be
    # mistaken for UNREADABLE.
    tool_call = '{"tool":"read_file","duration_ms":12,"success":true}'
    usage_line = '{"type":"usage","model":"claude-opus-5","input_tokens":10,"cost_usd":0.4}'

    assert_eq(
        "one usage line among tool calls classifies RECORDED (the real shape)",
        classify_session_usage([tool_call, usage_line, tool_call]),
        USAGE_RECORDED,
    )
    assert_eq(
        "tool-call lines with no `type` key classify ABSENT, not UNREADABLE",
        classify_session_usage([tool_call, tool_call]),
        USAGE_ABSENT,
    )
    assert_eq(
        "a line with a non-usage type is still ABSENT",
        classify_session_usage(['{"type":"error","msg":"x"}']),
        USAGE_ABSENT,
    )
    assert_eq(
        "every non-blank line unparseable classifies UNREADABLE",
        classify_session_usage(["not json at all", "{oops"]),
        USAGE_UNREADABLE,
    )
    assert_eq(
        "an empty file is UNREADABLE (we never managed to look), not ABSENT",
        classify_session_usage([]),
        USAGE_UNREADABLE,
    )
    # Blank lines are not corruption and a trailing newline is normal JSONL.
    assert_eq(
        "blank lines and a trailing newline do not change a RECORDED verdict",
        classify_session_usage(["", usage_line, "   ", "\n"]),
        USAGE_RECORDED,
    )
    assert_eq(
        "blank lines and a trailing newline do not change an ABSENT verdict",
        classify_session_usage(["", tool_call, "\n"]),
        USAGE_ABSENT,
    )
    assert_eq(
        "a file of nothing but blank lines is UNREADABLE, not ABSENT",
        classify_session_usage(["", "  ", "\n"]),
        USAGE_UNREADABLE,
    )

    # The fold keeps four distinct numbers; nothing is summed into anything.
    cov = usage_coverage(
        [USAGE_RECORDED, USAGE_RECORDED, USAGE_RECORDED, USAGE_ABSENT, USAGE_UNREADABLE]
    )
    assert_eq(
        "fold keeps recorded/absent/unreadable/examined distinct",
        f"{cov.recorded}/{cov.absent}/{cov.unreadable}/{cov.examined}",
        "3/1/1/5",
    )
    assert_eq(
        "an unreadable session still counts toward examined (no shrinking denominator)",
        usage_coverage([USAGE_UNREADABLE]).examined,
        1,
    )

    # Anti-vacuous: an empty scan is could-not-check, NEVER a healthy 0 of 0.
    empty_render = render_usage_coverage(UsageCoverage(), AUDIT_DIR_OK)
    assert_true(
        "examined == 0 renders could-not-check, not a healthy 0 of 0",
        "not checked" in empty_render and "0 of 0" not in empty_render,
    )
    assert_true(
        "the empty-scan line refuses to read as a clean bill",
        "NOT 'no usage records missing'" in empty_render,
    )
    # Near-miss guard: the side that must NOT fire. A discriminator tested
    # only where it fires is vacuous green.
    healthy = render_usage_coverage(UsageCoverage(recorded=10, examined=10), AUDIT_DIR_OK)
    assert_true(
        "an all-recorded fold renders the healthy line",
        "10 of 10 sessions carry >=1 usage record" in healthy,
    )
    assert_true(
        "the healthy line claims nothing is missing and does not refuse",
        "NO usage line" not in healthy and "not checked" not in healthy,
    )
    absent_render = render_usage_coverage(
        UsageCoverage(recorded=7, absent=3, examined=10), AUDIT_DIR_OK
    )
    assert_true(
        "an absent count is named out loud as sessions that logged nothing",
        "3 session(s) ran and logged NO usage line" in absent_render,
    )
    assert_true(
        "coverage is reported as k of N, never as a token total or a dollar figure",
        "7 of 10" in absent_render
        and "$" not in absent_render
        and "token" not in absent_render,
    )
    mixed = render_usage_coverage(
        UsageCoverage(recorded=6, absent=3, unreadable=1, examined=10), AUDIT_DIR_OK
    )
    assert_true(
        "absent and unreadable are reported separately, never summed",
        "3 session(s) ran and logged NO usage line" in mixed
        and "1 session(s) could not be read" in mixed,
    )
    # The audit-dir refusals reuse #843's three states verbatim.
    assert_true(
        "an unset audit dir refuses rather than reporting coverage",
        "not checked" in render_usage_coverage(UsageCoverage(), AUDIT_DIR_UNSET),
    )
    assert_true(
        "an unusable audit dir says outright it is not 'no records missing'",
        "NOT 'no usage records missing'"
        in render_usage_coverage(UsageCoverage(), AUDIT_DIR_UNUSABLE),
    )

    # --- The #848 producer boundary (Day 181) -----------------------------
    # A session that PREDATES the producer did not fail to log; it could not
    # have. Shipped without this, the detector's first live render alarmed
    # about eight such sessions.
    before = "20260827T221700Z"  # day-180, pre-producer
    after = "20260828T020932Z"  # day-181, post-producer
    at_boundary = "20260828T002633Z"  # exactly 8a633cff's commit instant

    assert_eq(
        "an ABSENT session predating the producer is NOT_MEASURABLE, not absent",
        apply_usage_boundary(USAGE_ABSENT, before),
        USAGE_NOT_MEASURABLE,
    )
    # Near-miss guard: the side that must NOT fire.
    assert_eq(
        "an ABSENT session after the producer stays ABSENT (a real alarm)",
        apply_usage_boundary(USAGE_ABSENT, after),
        USAGE_ABSENT,
    )
    # Rule 3: strict-before. Pin BOTH sides of the boundary.
    assert_eq(
        "a session stamped exactly at the boundary is measurable, not demoted",
        apply_usage_boundary(USAGE_ABSENT, at_boundary),
        USAGE_ABSENT,
    )
    assert_eq(
        "one second before the boundary is demoted",
        apply_usage_boundary(USAGE_ABSENT, "20260828T002632Z"),
        USAGE_NOT_MEASURABLE,
    )
    # Rule 1: only ABSENT is ever demoted.
    assert_eq(
        "RECORDED predating the boundary passes through — observation beats claim",
        apply_usage_boundary(USAGE_RECORDED, before),
        USAGE_RECORDED,
    )
    assert_eq(
        "UNREADABLE passes through — 'could not check' is not 'could not have'",
        apply_usage_boundary(USAGE_UNREADABLE, before),
        USAGE_UNREADABLE,
    )
    # Rule 2: an unknown stamp is NOT promoted into the comfortable bucket.
    for label, bad in (
        ("None", None),
        ("empty", ""),
        ("malformed", "not-a-stamp"),
        ("wrong width", "2026828T02Z"),
        ("non-string", 20260828),
    ):
        assert_eq(
            f"an unparseable session stamp ({label}) stays ABSENT, never demoted",
            apply_usage_boundary(USAGE_ABSENT, bad),
            USAGE_ABSENT,
        )

    # The stamp normaliser accepts exactly the two shapes that occur.
    assert_eq(
        "the compact session stamp normalises to itself",
        compact_utc_stamp(after),
        "20260828T020932Z",
    )
    assert_eq(
        "the ISO-8601 form git prints normalises to the compact form",
        compact_utc_stamp(USAGE_PRODUCER_LANDED_TS),
        "20260828T002633Z",
    )
    assert_eq(
        "an offset-bearing stamp is unknown rather than coerced",
        compact_utc_stamp("2026-08-28T00:26:33+00:00"),
        None,
    )
    assert_eq(
        "a session directory name yields its stamp",
        session_dir_stamp("day-181-20260828T020932Z"),
        "20260828T020932Z",
    )
    assert_eq(
        "a directory name that is not a session yields None",
        session_dir_stamp("transcripts"),
        None,
    )

    # The fold keeps not_measurable distinct and never sums it into absent.
    cov4 = usage_coverage(
        [USAGE_RECORDED, USAGE_ABSENT, USAGE_UNREADABLE, USAGE_NOT_MEASURABLE, USAGE_NOT_MEASURABLE]
    )
    assert_eq(
        "fold keeps recorded/absent/unreadable/not_measurable/examined distinct",
        f"{cov4.recorded}/{cov4.absent}/{cov4.unreadable}/{cov4.not_measurable}/{cov4.examined}",
        "1/1/1/2/5",
    )

    # The render: not-measurable leaves the denominator and gets its own clause.
    boundary_render = render_usage_coverage(
        UsageCoverage(recorded=2, not_measurable=6, examined=8), AUDIT_DIR_OK
    )
    assert_true(
        "coverage is k of the MEASURABLE sessions, not of every session",
        "2 of 2 measurable sessions carry >=1 usage record" in boundary_render,
    )
    assert_true(
        "pre-producer sessions are named as their own clause, never as 'logged NO usage'",
        f"6 session(s) predate the #848 producer ({USAGE_PRODUCER_SHA})"
        in boundary_render
        and "NO usage line" not in boundary_render,
    )
    assert_true(
        "the boundary render stays within the 3-line budget",
        len(boundary_render.splitlines()) <= 3,
    )
    # The live Day-181 shape: 2 recorded, 6 pre-producer, 2 genuinely absent.
    live = render_usage_coverage(
        UsageCoverage(recorded=2, absent=2, not_measurable=6, examined=10), AUDIT_DIR_OK
    )
    assert_true(
        "a real absent count still alarms while pre-producer sessions do not",
        "2 of 4 measurable sessions" in live
        and "2 session(s) ran and logged NO usage line" in live
        and "6 session(s) predate the #848 producer" in live,
    )
    assert_true(
        "the mixed boundary render stays within the 3-line budget",
        len(live.splitlines()) <= 3,
    )
    # Anti-vacuous in the SECOND denominator: a wholly pre-producer window is
    # a refusal, never a healthy "0 of 0 measurable".
    all_pre = render_usage_coverage(
        UsageCoverage(not_measurable=9, examined=9), AUDIT_DIR_OK
    )
    assert_true(
        "a wholly unmeasurable window refuses instead of rendering 0 of 0",
        "not measurable" in all_pre
        and "0 of 0" not in all_pre
        and "NOT 'no usage records missing'" in all_pre,
    )
    # Near-miss guards: with no pre-producer sessions, every pre-existing
    # branch is byte-identical. A discriminator tested only where it fires is
    # vacuous green.
    assert_eq(
        "with not_measurable == 0 the healthy line is byte-identical",
        render_usage_coverage(UsageCoverage(recorded=10, examined=10), AUDIT_DIR_OK),
        "## Usage records\n10 of 10 sessions carry >=1 usage record (#848 channel is live).",
    )
    assert_eq(
        "with not_measurable == 0 the absent line is byte-identical",
        render_usage_coverage(
            UsageCoverage(recorded=7, absent=3, examined=10), AUDIT_DIR_OK
        ),
        "## Usage records\n7 of 10 sessions carry >=1 usage record.\n"
        "3 session(s) ran and logged NO usage line — the #848 producer wrote "
        "nothing there.",
    )

    # --- Module-size reader (Day 183) -------------------------------------
    # The gate warns on the stderr of a PASSING test and the only consumer of
    # `cargo test` in the evolve loop reads the exit code, so nothing read it.
    # Everything below is driven with FABRICATED inputs — no filesystem, no
    # `gh`, no `cargo` (#832) — so the decision is graded, not the wiring.
    print("\n=== module-size reader self-tests ===\n")

    # count_rs_lines must agree with Rust's `content.lines().count()`. A
    # counter that disagrees with the gate is worse than no counter.
    assert_eq("count_rs_lines: empty text is 0 lines", str(count_rs_lines("")), "0")
    assert_eq("count_rs_lines: trailing newline is not a line", str(count_rs_lines("a\nb\n")), "2")
    assert_eq("count_rs_lines: no trailing newline still counts", str(count_rs_lines("a\nb")), "2")
    assert_eq("count_rs_lines: one line with newline", str(count_rs_lines("a\n")), "1")
    # The splitlines() trap: Python splits on \x0b/\x0c/\u2028, Rust does not.
    assert_eq(
        "count_rs_lines: a vertical tab is NOT a line break (Rust splits on \\n only)",
        str(count_rs_lines("a\x0bb\n")),
        "1",
    )

    # A faithful fixture of the gate's own shape, including the `2_000`
    # underscore literal and an interleaved comment.
    gate_fixture = (
        "const MAX_MODULE_LINES: usize = 2_000;\n"
        "const OVERSHOOT_GRACE_LINES: usize = 50;\n"
        "const REGISTER_DRIFT_GRACE_LINES: usize = 100;\n"
        "const GRANDFATHERED_OVERSIZED_MODULES: &[(&str, usize)] = &[\n"
        '    // a comment between entries\n'
        '    ("src/cli.rs", 5349),\n'
        '    ("src/format/mod.rs", 2629),\n'
        "];\n"
        "fn fixture() { let _ = [(\"src/fake_fixture.rs\", 500)]; }\n"
    )
    spec = parse_module_gate(gate_fixture)
    assert_true("parse_module_gate: a well-formed gate parses ok", spec.ok)
    assert_eq("parse_module_gate: the 2_000 underscore literal parses", str(spec.max_lines), "2000")
    assert_eq("parse_module_gate: overshoot grace parses", str(spec.overshoot_grace), "50")
    assert_eq("parse_module_gate: drift grace parses", str(spec.drift_grace), "100")
    assert_eq("parse_module_gate: both register entries parse", str(len(spec.register)), "2")
    assert_eq(
        "parse_module_gate: a nested path entry keeps its full relpath",
        str(spec.register.get("src/format/mod.rs")),
        "2629",
    )
    # Only the register LITERAL is scanned, so a `("src/x.rs", N)` pair sitting
    # in the gate's own unit-test fixtures can never masquerade as an entry.
    assert_true(
        "parse_module_gate: a pair outside the register literal is not an entry",
        "src/fake_fixture.rs" not in spec.register,
    )
    # Anti-vacuous, in every direction a broken parse can fail.
    assert_true("parse_module_gate: empty text refuses", not parse_module_gate("").ok)
    assert_true(
        "parse_module_gate: a missing const refuses rather than defaulting",
        not parse_module_gate(
            gate_fixture.replace("const OVERSHOOT_GRACE_LINES: usize = 50;\n", "")
        ).ok,
    )
    assert_true(
        "parse_module_gate: ZERO register entries refuses (a broken parse is not a clean one)",
        not parse_module_gate(
            "const MAX_MODULE_LINES: usize = 2_000;\n"
            "const OVERSHOOT_GRACE_LINES: usize = 50;\n"
            "const REGISTER_DRIFT_GRACE_LINES: usize = 100;\n"
            "const GRANDFATHERED_OVERSIZED_MODULES: &[(&str, usize)] = &[\n];\n"
        ).ok,
    )
    assert_true(
        "parse_module_gate: a missing register literal refuses",
        not parse_module_gate(
            "const MAX_MODULE_LINES: usize = 2_000;\n"
            "const OVERSHOOT_GRACE_LINES: usize = 50;\n"
            "const REGISTER_DRIFT_GRACE_LINES: usize = 100;\n"
        ).ok,
    )

    # A crash-safe reader: a neutered scan must make these assertions FAIL,
    # not raise, or the run aborts before the near-miss guards below and the
    # positive control cannot show that they stay green.
    def field(tup, i: int) -> str:
        return "<none>" if tup is None else str(tup[i])

    # module_size_risks: the AT-RISK side. Numbers are this session's real
    # measurement — src/prompt_retry.rs at 2042 was 8 lines from fatal.
    risk = module_size_risks(spec, [("src/prompt_retry.rs", 2042), ("src/main.rs", 100)])
    assert_true("module_size_risks: an unlisted file over the cap is caught", risk.worst_unlisted is not None)
    assert_eq(
        "module_size_risks: headroom to FATAL is max + grace - lines (the number the gate never prints)",
        field(risk.worst_unlisted, 2),
        "8",
    )
    assert_eq("module_size_risks: the scanned count is the whole denominator", str(risk.scanned), "2")
    # Near-miss guard: the side that must NOT fire. A discriminator tested
    # only where it fires is vacuous green.
    clean = module_size_risks(spec, [("src/main.rs", 100), ("src/format/mod.rs", 2629)])
    assert_true(
        "module_size_risks: a clean tree reports NO unlisted risk",
        clean.worst_unlisted is None,
    )
    assert_true(
        "module_size_risks: a listed file exactly at its recorded lines is not drift",
        clean.worst_drift is None,
    )
    assert_true(
        "module_size_risks: a file exactly AT the cap is not over it (boundary)",
        module_size_risks(spec, [("src/x.rs", 2000)]).worst_unlisted is None,
    )
    assert_true(
        "module_size_risks: one line past the cap IS over it (the other side of the boundary)",
        module_size_risks(spec, [("src/x.rs", 2001)]).worst_unlisted is not None,
    )
    # Drift: reported only past MODULE_DRIFT_REPORT_FRACTION of the grace band,
    # so the +1/+2 creep that made up most of Day 174's list stays quiet.
    drift = module_size_risks(spec, [("src/format/mod.rs", 2690), ("src/cli.rs", 5350)])
    assert_true("module_size_risks: drift past the report floor is caught", drift.worst_drift is not None)
    assert_eq(
        "module_size_risks: the reported drift is the WORST (smallest headroom), not the first seen",
        field(drift.worst_drift, 0),
        "src/format/mod.rs",
    )
    assert_eq(
        "module_size_risks: drift headroom is recorded + drift_grace - lines",
        field(drift.worst_drift, 3),
        "39",
    )
    assert_true(
        "module_size_risks: a +1 drift is under the floor and stays silent",
        module_size_risks(spec, [("src/cli.rs", 5350)]).worst_drift is None,
    )

    # --- #885 half 2: the mirror direction. Day 187 made a >100-line SHRINK
    # fatal, and that warning had the same non-reader the growth warning had
    # for eight days. Same register slot, same report floor, own wording.
    shrink = module_size_risks(spec, [("src/cli.rs", 5300)])  # recorded 5349, -49
    assert_true("module_size_risks: shrink past the report floor is caught", shrink.worst_drift is not None)
    assert_eq(
        "module_size_risks: shrink is tagged as its own kind, never as growth",
        field(shrink.worst_drift, 4),
        "shrink",
    )
    assert_eq(
        "module_size_risks: shrink headroom is drift_grace - shrink (100 - 49)",
        field(shrink.worst_drift, 3),
        "51",
    )
    # Both sides of the SAME floor the growth side uses — a discriminator
    # tested only where it fires is vacuous green. 100 * 0.25 = 25.
    assert_true(
        "module_size_risks: a 25-line shrink is AT the floor and stays silent",
        module_size_risks(spec, [("src/cli.rs", 5324)]).worst_drift is None,
    )
    assert_true(
        "module_size_risks: a 26-line shrink is past the floor and speaks (other side)",
        module_size_risks(spec, [("src/cli.rs", 5323)]).worst_drift is not None,
    )
    # A listed file that has dropped BELOW the cap is a different fact with a
    # different remedy (delete the entry), so it is NOT folded into ordinary
    # shrink drift and is NOT gated by the report floor — the gate is fatal on
    # it right now, so its headroom is 0 and it outranks every ordinary drift.
    paid = module_size_risks(spec, [("src/cli.rs", 1999)])
    assert_eq(
        "module_size_risks: a listed file below the cap gets its own kind, not 'shrink'",
        field(paid.worst_drift, 4),
        "paid-off",
    )
    assert_eq(
        "module_size_risks: a paid-off entry reads as already fatal (headroom 0)",
        field(paid.worst_drift, 3),
        "0",
    )
    both = module_size_risks(spec, [("src/cli.rs", 1999), ("src/format/mod.rs", 2690)])
    assert_eq(
        "module_size_risks: the already-fatal paid-off entry outranks ordinary growth drift",
        field(both.worst_drift, 0),
        "src/cli.rs",
    )
    # Render: each direction says the number the gate never prints, and the
    # paid-off remedy is DELETE rather than a new number.
    shrink_out = render_module_sizes(spec, shrink)
    assert_true(
        "render: the shrink line names shrink headroom, not growth headroom",
        "51 more line(s) of shrink makes it FATAL" in shrink_out,
    )
    assert_true(
        "render: the shrink line prints the literal register line to paste",
        '("src/cli.rs", 5300)' in shrink_out,
    )
    assert_true(
        "render: AT-RISK stays at most 2 lines plus its header with shrink in play",
        len(render_module_sizes(spec, both).splitlines()) <= 3,
    )
    paid_out = render_module_sizes(spec, paid)
    assert_true(
        "render: the paid-off remedy is DELETE the entry, not paste a new number",
        "DELETE its GRANDFATHERED_OVERSIZED_MODULES entry" in paid_out,
    )
    assert_true(
        "render: the paid-off line does NOT tell the reader to paste an entry back",
        '("src/cli.rs", 1999)' not in paid_out,
    )
    # Near-miss guard: a register whose entries all match renders the empty
    # string byte-identically. Asserted with == on the whole render, never a
    # `contains` — silence is the entire regression surface.
    assert_eq(
        "render: entries matching their recorded counts render the empty string, byte-identically",
        render_module_sizes(spec, module_size_risks(spec, [("src/cli.rs", 5349), ("src/format/mod.rs", 2629)])),
        "",
    )

    # render_module_sizes: three states, none folded into another.
    at_risk = render_module_sizes(spec, risk)
    assert_true(
        "render: the AT-RISK line names the headroom and that FATAL reverts the task",
        "8 more line(s) makes `cargo test` FATAL" in at_risk,
    )
    assert_true(
        "render: the AT-RISK line prints the literal register line to paste",
        '("src/prompt_retry.rs", 2042)' in at_risk,
    )
    assert_true(
        "render: AT-RISK is held to at most 2 lines plus its header",
        len(at_risk.splitlines()) <= 3,
    )
    assert_true(
        "render: a drift risk renders its own line with its own headroom",
        "39 more line(s) makes it FATAL" in render_module_sizes(spec, drift),
    )
    # Near-miss guard: OK renders NOTHING. Silent is the common case and the
    # entire regression surface — asserted as byte-identical emptiness, not
    # as the absence of some substring.
    assert_eq("render: a clean scan renders the empty string, byte-identically", render_module_sizes(spec, clean), "")
    # COULD NOT CHECK, both ways in. Anti-vacuous is asserted FIRST in the
    # renderer, so a zero-file walk refuses even when the spec parsed fine.
    zero_files = render_module_sizes(spec, ModuleRisk(None, None, 0))
    assert_true(
        "render: a walk finding zero src files refuses rather than rendering OK",
        "not checked" in zero_files and "found 0 files" in zero_files,
    )
    assert_true(
        "render: the zero-file refusal says outright it is NOT a clean bill",
        "NOT 'no modules at risk'" in zero_files,
    )
    unparsed = render_module_sizes(ModuleGateSpec(0, 0, 0, {}, False), ModuleRisk(None, None, 94))
    assert_true(
        "render: an unparseable gate file refuses rather than rendering OK",
        "not checked" in unparsed and MODULE_GATE_REL_PATH in unparsed,
    )
    assert_true(
        "render: the unparseable-gate refusal also refuses to read as 'checked; clean'",
        "NOT 'no modules at risk'" in unparsed,
    )

    print("\n=== classify_pairing_coverage / render_pairing_coverage self-tests ===\n")

    def _v(sha, verdict="UNEARNED", depth=None):
        row = {"sha": sha, "verdict": verdict}
        if depth is not None:
            row["splice_depth"] = depth
        return json.dumps(row)

    def _p(sha, pairing="PAIR_INNOCENT_BY_MECHANISM"):
        return json.dumps({"sha": sha, "pairing": pairing})

    # ANTI-VACUOUS, asserted FIRST: a ledger with zero UNEARNED rows is its own
    # state, never healthy coverage. A scanner that finds nothing and reports a
    # clean bill is this defect wearing the opposite sign — quieter than the bug.
    vacuous = classify_pairing_coverage(_v("aaa", verdict="EARNED"), "")
    assert_true(
        "anti-vacuous: zero UNEARNED rows is its OWN state, not healthy coverage",
        vacuous.state == PAIRING_NO_UNEARNED and vacuous.unearned == 0,
    )
    assert_true(
        "anti-vacuous: the render says coverage is UNDEFINED, not complete",
        "nothing to cover" in render_pairing_coverage(vacuous)
        and "not complete" in render_pairing_coverage(vacuous),
    )

    # A missing/unreadable VERDICT ledger is a refusal, never "0 unpaired":
    # "could not check" must never read as "checked; clean".
    refused = classify_pairing_coverage(None, _p("aaa"))
    assert_true(
        "could-not-check: a missing verdict ledger refuses",
        refused.state == PAIRING_COULD_NOT_CHECK,
    )
    assert_true(
        "could-not-check: the render refuses to read as 'coverage is complete'",
        "not checked" in render_pairing_coverage(refused)
        and "NOT 'coverage is complete'" in render_pairing_coverage(refused),
    )

    # Full coverage — the live shape today: 4 UNEARNED, 4 paired, 0 PAIR_SIGNAL,
    # split across two depths. A row with NO `splice_depth` key is tests-only
    # BY CONSTRUCTION (it predates --splice-src-tests), never "unknown".
    full_v = "\n".join(
        [_v("s1"), _v("s2"), _v("s3", depth="src+tests"), _v("s4", depth="src+tests")]
    )
    full_p = "\n".join([_p("s1"), _p("s2"), _p("s3"), _p("s4")])
    full = classify_pairing_coverage(full_v, full_p)
    assert_true(
        "full coverage: 4 of 4 paired, 0 unpaired, 0 PAIR_SIGNAL",
        full.state == PAIRING_OK
        and (full.unearned, full.paired, full.unpaired, full.signal) == (4, 4, 0, 0),
    )
    assert_true(
        "depth split is PER DEPTH and never pooled (a missing key means tests-only)",
        full.by_depth == {"tests": (2, 2), "src+tests": (2, 2)},
    )
    rendered_full = render_pairing_coverage(full)
    assert_true(
        "render: full coverage still speaks, and names both depths",
        "4 of 4 UNEARNED rows paired" in rendered_full
        and "tests 2/2" in rendered_full
        and "src+tests 2/2" in rendered_full,
    )
    assert_true(
        "render: the STATED LIMIT is in the rendered text, not only in a comment",
        "RAN, never that a verdict is\nRIGHT" in rendered_full
        or "RAN, never that a verdict is RIGHT" in rendered_full.replace("\n", " "),
    )
    assert_true(
        "render: at most 2 lines, header included (the epistemic block renders LAST)",
        len(rendered_full.splitlines()) <= 2,
    )

    # Partial coverage names the count AND the pasteable remedy verbatim.
    partial = classify_pairing_coverage(full_v, _p("s1"))
    assert_true(
        "partial coverage: 1 of 4 paired, 3 unpaired",
        (partial.paired, partial.unpaired) == (1, 3),
    )
    rendered_partial = render_pairing_coverage(partial)
    assert_true(
        "render: an unpaired row names the count and the remedy VERBATIM",
        "3 unpaired" in rendered_partial and PAIRING_REMEDY in rendered_partial,
    )
    assert_true(
        "render: the partial line also holds the 2-line ceiling",
        len(rendered_partial.splitlines()) <= 2,
    )

    # PAIR_SIGNAL — the cell this whole vein exists to find — is counted, and
    # only for shas that are actually UNEARNED verdicts.
    signalled = classify_pairing_coverage(full_v, "\n".join([_p("s1", "PAIR_SIGNAL"), _p("s2")]))
    assert_true(
        "PAIR_SIGNAL is counted separately from paired",
        (signalled.paired, signalled.signal) == (2, 1),
    )

    # Unparseable lines are COUNTED and REPORTED, never silently dropped — a
    # shrinking denominator inside my own meter is the defect this vein is about.
    junky = classify_pairing_coverage(full_v + "\n{not json\n", full_p + "\n{{{\n")
    assert_true(
        "unparseable lines in EITHER file are counted, not dropped",
        junky.unparseable == 2 and junky.unearned == 4,
    )
    assert_true(
        "render: unparseable lines are reported out loud",
        "unparseable" in render_pairing_coverage(junky),
    )

    # The direction that says the two files DISAGREE.
    orphaned = classify_pairing_coverage(_v("s1"), "\n".join([_p("s1"), _p("zzz")]))
    assert_true(
        "a pairing whose sha is not an UNEARNED verdict is counted as orphaned",
        orphaned.pairings_orphaned == 1 and orphaned.paired == 1,
    )

    # A sha recorded twice is ONE commit (the live ledger has one), counted once.
    duped = classify_pairing_coverage("\n".join([_v("s1"), _v("s1")]), _p("s1"))
    assert_true(
        "a sha appearing twice is one commit, counted once",
        (duped.unearned, duped.paired) == (1, 1),
    )

    # A MISSING pairing ledger is genuinely zero coverage, NOT a refusal: never
    # having run the pairing mode is the signal, not an inability to look.
    never_run = classify_pairing_coverage(full_v, None)
    assert_true(
        "a missing PAIRING ledger reads as 0 coverage, not as could-not-check",
        never_run.state == PAIRING_OK and (never_run.paired, never_run.unpaired) == (0, 4),
    )

    print("\n=== claims_by_day / classify_productivity self-tests ===\n")

    # Every fixture below is FABRICATED — plain dicts and sets, no filesystem,
    # no git, no audit-log. The classifier is pure over already-parsed data and
    # must be drivable with nothing mounted.

    # ANTI-VACUOUS, AND IT IS ASSERTED FIRST. An empty observation set means the
    # git collection returned nothing at all; answering "every day is idle" there
    # is this very defect wearing the opposite sign, and it would fire on every
    # run where `git log` failed.
    blind = classify_productivity({195: 5}, set())
    assert_true(
        "anti-vacuous: an empty commit set is COULD_NOT_CHECK, never 'every day idle'",
        blind.state == PRODUCTIVITY_COULD_NOT_CHECK and blind.idle_days == (),
    )
    blind_render = render_productivity(blind)
    # Asserted on the RENDERED STRING, not merely on the tally: the string is
    # what reaches the planner prompt, and the refusal has to survive rendering.
    assert_true(
        "the empty-commit-set line refuses out loud rather than reading as a clean bill",
        "REFUSAL" in blind_render
        and "not checked" in blind_render
        and "IDLE" not in blind_render,
    )
    assert_true(
        "an empty outcome window is COULD_NOT_CHECK too: no claims read, nothing missing",
        classify_productivity({}, {194, 195}).state == PRODUCTIVITY_COULD_NOT_CHECK,
    )

    # THE REAL STREAK, as a fixture. Day 195 ran five sessions, every instrument
    # reported success, and `git log` carries no Day-195 task commit at all.
    streak = classify_productivity({194: 4, 195: 5}, {194})
    assert_true(
        "the measured streak: {195: 5} claimed vs {194} observed is IDLE naming day 195",
        streak.state == PRODUCTIVITY_IDLE
        and streak.idle_days == (195,)
        and streak.idle_claimed == 5,
    )
    streak_render = render_productivity(streak)
    assert_true(
        "the IDLE line names the day, the claimed total, and the rows it contradicts",
        "day-195" in streak_render
        and "5 success(es)" in streak_render
        and "ZERO task commits" in streak_render
        and "outcome rows above" in streak_render,
    )
    assert_true(
        "a claiming day that DID commit is not dragged into the alarm",
        "day-194" not in streak_render,
    )
    multi = classify_productivity({193: 1, 194: 2, 195: 5}, {194})
    assert_true(
        "several idle days are listed in order and their claims summed, never averaged",
        multi.idle_days == (193, 195) and multi.idle_claimed == 6,
    )

    # NEAR-MISS GUARD — THE ENTIRE REGRESSION SURFACE. Every claiming day has a
    # task commit, so the OK line renders and the alarm does not. A discriminator
    # tested only on the side that fires is vacuous green.
    #
    # The pass-through claim and the denominator claim are asserted SEPARATELY,
    # not folded into one row: they are two facts with two different remedies,
    # and bundling them means a red cannot say which half moved. Only the first
    # is the near-miss guard proper — it is the row that must survive a
    # neutered classifier, which is what proves it tests the pass-through.
    healthy = classify_productivity({194: 4, 195: 2}, {194, 195})
    assert_true(
        "near-miss: every claiming day has a task commit -> OK state, no idle days",
        healthy.state == PRODUCTIVITY_OK and healthy.idle_days == (),
    )
    healthy_render = render_productivity(healthy)
    assert_true(
        "near-miss: the healthy line renders the OK sentence, not the alarm",
        "produced >=1 task commit" in healthy_render,
    )
    assert_true(
        "the healthy line carries no alarm vocabulary and no refusal",
        "IDLE" not in healthy_render
        and "ZERO task commits" not in healthy_render
        and "REFUSAL" not in healthy_render,
    )
    assert_true(
        "the OK line states its own denominator rather than asserting bare health",
        "All 2 claiming day(s)" in healthy_render and healthy.claiming_days == 2,
    )
    assert_true(
        "an extra observed day with no claim does not disturb the OK verdict",
        classify_productivity({195: 2}, {193, 194, 195}).state == PRODUCTIVITY_OK,
    )

    # A DAY CLAIMING ZERO SUCCESSES WITH NO COMMITS IS NOT IDLE. That is an
    # honest session that attempted nothing or reverted; flagging it would be a
    # false alarm on the one signal whose whole value is that it fires rarely.
    zero_claim = classify_productivity({194: 3, 195: 0}, {194})
    assert_true(
        "a day claiming ZERO with no commits is not idle — honest abstention, not a miss",
        zero_claim.state == PRODUCTIVITY_OK and zero_claim.idle_days == (),
    )
    all_zero = classify_productivity({194: 0, 195: 0}, {194})
    assert_true(
        "a window where nothing was claimed is NO_CLAIMS, its own state",
        all_zero.state == PRODUCTIVITY_NO_CLAIMS and all_zero.claiming_days == 0,
    )
    no_claims_render = render_productivity(all_zero)
    assert_true(
        "the no-claims line says nothing is missing and does not render as the alarm",
        "nothing is missing" in no_claims_render
        and "IDLE" not in no_claims_render
        and "REFUSAL" not in no_claims_render,
    )

    # The join key is the DAY NUMBER. Session dir stamps are written at audit
    # PUSH time and commit subjects carry SESSION_TIME from the start, so the
    # two never match and must never be compared.
    assert_eq(
        "claims_by_day sums tasks_succeeded per day across that day's sessions",
        claims_by_day(
            [
                {"day": 195, "tasks_succeeded": 1},
                {"day": 195, "tasks_succeeded": 1},
                {"day": 194, "tasks_succeeded": 2},
            ]
        ),
        {195: 2, 194: 2},
    )
    assert_eq(
        "a day present with zero claims is KEPT as an explicit 0, not dropped",
        claims_by_day([{"day": 195, "tasks_succeeded": 0}]),
        {195: 0},
    )
    assert_eq(
        "a row with no parseable day contributes nothing rather than raising",
        claims_by_day(
            [{"tasks_succeeded": 3}, {"day": "195", "tasks_succeeded": 3}, {"day": 1}]
        ),
        {1: 0},
    )
    assert_eq(
        "a malformed tasks_succeeded is read as 0, never coerced into a comparable",
        claims_by_day(
            [
                {"day": 195, "tasks_succeeded": "many"},
                {"day": 195, "tasks_succeeded": -4},
                {"day": 195, "tasks_succeeded": True},
            ]
        ),
        {195: 0},
    )

    print(f"\n{'ALL PASSED' if failures == 0 else f'{failures} FAILURE(S)'}")
    return 1 if failures else 0


USAGE = """usage: extract_trajectory.py [--test] [--help]

Aggregate audit-log session outcomes, git log and recent CI runs into a
`YOUR TRAJECTORY` markdown block for the Phase A1/A2 planning prompts.

options:
  --test    run the self-tests and exit non-zero on failure
  --help    show this message and exit

environment:
  YOYO_AUDIT_DIR        directory of `sessions/day-*/` audit-log checkouts
  YOYO_REPO             owner/name, enables the `gh`-backed CI sections
  YOYO_DAY              day number for the header
  YOYO_TRAJECTORY_OUT   output path (default .yoyo/session_staging/trajectory.md)

Fail-soft by contract: every section degrades to an honest note rather than
raising, so this never blocks a session."""


if __name__ == "__main__":
    if "--help" in sys.argv or "-h" in sys.argv:
        print(USAGE)
        sys.exit(0)
    if "--test" in sys.argv:
        sys.exit(run_self_tests())
    sys.exit(main())
