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


PROVIDER_ERROR_RE = re.compile(r'"type"\s*:\s*"error"|provider_error|rate_limit', re.IGNORECASE)


AUDIT_FILE_SIZE_CAP = 10 * 1024 * 1024  # 10MB per file — guard against runaway audit.jsonl


def collect_provider_errors(audit_dir: Path) -> tuple[int, int]:
    """Return (sessions_examined, total_provider_error_hits).
    Streams audit.jsonl line-by-line so a multi-MB file doesn't slurp into
    memory. Per-file size cap (10MB) protects against pathological cases."""
    if not audit_dir.exists():
        return 0, 0
    sessions = 0
    hits = 0
    for child in sorted(audit_dir.iterdir(), reverse=True):
        if not child.is_dir():
            continue
        audit = child / "audit.jsonl"
        if not audit.is_file():
            continue
        sessions += 1
        try:
            size = audit.stat().st_size
            if size > AUDIT_FILE_SIZE_CAP:
                warn(f"{audit} is {size} bytes (>{AUDIT_FILE_SIZE_CAP}); scanning first {AUDIT_FILE_SIZE_CAP}B only")
            with audit.open(encoding="utf-8", errors="replace") as f:
                bytes_read = 0
                for line in f:
                    bytes_read += len(line)
                    if bytes_read > AUDIT_FILE_SIZE_CAP:
                        break
                    if PROVIDER_ERROR_RE.search(line):
                        hits += 1
        except OSError as e:
            warn(f"skipped {audit}: {e}")
        if sessions >= WINDOW_SESSIONS:
            break
    return sessions, hits


def render_provider_health(sessions: int, hits: int) -> str:
    if sessions == 0:
        return ""
    if hits == 0:
        return f"## Provider/API health\n{sessions} sessions, no provider errors detected."
    return f"## Provider/API health\n{sessions} sessions, {hits} provider error hit(s) in audit.jsonl."


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

    audit_dir = Path(audit_dir_str) if audit_dir_str else Path("/dev/null")

    header = (
        f"# YOUR TRAJECTORY\n\n"
        f"Last computed: {datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%MZ')}. "
        f"Day {day}. Window: last {WINDOW_SESSIONS} sessions / {WINDOW_DAYS} days.\n"
    )

    # Gather all sections (each falls back to "" silently on no-data)
    outcomes = load_outcomes(audit_dir)
    tasks, reverts = collect_task_commits()
    sessions_audited, provider_hits = collect_provider_errors(audit_dir)
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
    s = render_provider_health(sessions_audited, provider_hits)
    if s:
        sections.append(s)
    # Always rendered when any signal exists (it has its own honest fallback
    # line) so the planner sees the epistemic view even when it's starving.
    # Skipped only when there is no trajectory data at all AND no epistemic
    # data — preserving the honest global "(no trajectory data yet)" state.
    epistemic_entries, epistemic_never = collect_epistemic_blind_spots()
    data_sections = len(sections) - (1 if ci_unknown else 0)
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
