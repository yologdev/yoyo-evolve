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
from datetime import datetime, timezone
from pathlib import Path

# ── Configuration constants ──────────────────────────────────────────────
WINDOW_SESSIONS = 10           # last N sessions in the outcomes section
WINDOW_DAYS = 14               # git log window
MAX_FAILED_RUNS = 5            # cap on `gh run view --log-failed` calls
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


def render_reverts(reverts: int, total_sessions: int) -> str:
    if total_sessions == 0:
        return ""
    if reverts == 0:
        return f"## Reverts in window\n0 of last ~{total_sessions} sessions had reverts."
    return f"## Reverts in window\n{reverts} revert commit(s) in last {WINDOW_DAYS} days."


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


def collect_failed_ci_fingerprints(repo: str) -> list[tuple[str, list[str]]]:
    """Return [(fingerprint, [run_ids_seen_at])]. Capped at MAX_FAILED_RUNS fetches.
    Silent return-empty paths now warn() so a misconfigured token / rate-limit
    doesn't masquerade as 'no failed runs' (would defeat the recurring-error
    detection this section exists for)."""
    if not repo:
        warn("YOYO_REPO empty — skipping recurring-CI-error section")
        return []
    rc, stdout, stderr = run_cmd(
        [
            "gh", "run", "list", "--repo", repo,
            "--status", "failure", "--limit", str(MAX_FAILED_RUNS),
            "--json", "databaseId,createdAt,name,workflowName",
        ],
        timeout=GH_RUN_LIST_TIMEOUT,
    )
    if rc != 0:
        warn(f"gh run list rc={rc}: {(stderr or '').strip()[:200]}")
        return []
    try:
        runs = json.loads(stdout)
    except json.JSONDecodeError as e:
        warn(f"gh run list returned non-JSON: {e}")
        return []
    if not runs:
        return []

    fingerprints: defaultdict[str, list[str]] = defaultdict(list)
    fetch_errors = 0
    for run in runs:
        run_id = str(run.get("databaseId") or "")
        if not run_id:
            continue
        rc2, log_stdout, stderr2 = run_cmd(
            ["gh", "run", "view", run_id, "--repo", repo, "--log-failed"],
            timeout=GH_RUN_VIEW_TIMEOUT,
        )
        if rc2 != 0:
            fetch_errors += 1
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
    if fetch_errors and not fingerprints:
        warn(f"all {fetch_errors} gh run view fetch(es) failed — section will be empty")
    return sorted(fingerprints.items(), key=lambda kv: -len(kv[1]))


def render_ci_errors(clusters: list[tuple[str, list[str]]]) -> str:
    if not clusters:
        return ""
    lines = ["## Recurring CI errors (failed runs in window)"]
    for fp, run_ids in clusters[:5]:
        n = len(run_ids)
        marker = f"{n}×" if n > 1 else "1×"
        # Truncate fingerprint to keep line tidy
        fp_short = fp[:90]
        lines.append(f"[{marker}] {fp_short}")
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
EPISTEMIC_DISAGREE_RE = re.compile(r"reactive/emerging disagree in (\d+) of last (\d+) snapshots")
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


def compact_epistemic_reason(reason: str) -> str:
    """Shrink a verbose report reason to a few words (2KB total budget)."""
    m = EPISTEMIC_DISAGREE_RE.search(reason)
    if m:
        return f"columns disagree {m.group(1)}/{m.group(2)}"
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
        shown = ", ".join(never_forecast[:EPISTEMIC_NEVER_FORECAST_SHOWN])
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
    ci_clusters = collect_failed_ci_fingerprints(repo)

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
    s = render_reverts(reverts, len(outcomes))
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
    s = render_ci_errors(ci_clusters)
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
    if sections or epistemic_entries or epistemic_never:
        sections.append(render_epistemic(epistemic_entries, epistemic_never))

    if not sections:
        body = "(no trajectory data yet — audit-log is empty and no recent task commits found)"
    else:
        body = "\n\n".join(sections)

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
        "      • reactive/emerging disagree in 3 of last 3 snapshots\n"
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
        "- src/commands_search.rs (5.0) — predicted 28×, never graded; columns disagree 3/3",
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
    # output captured on Day 163 — a hand-typed fixture pins my belief about
    # the input, not the input (Day 147).
    canned_real = (
        "\n"
        "🔍 Epistemic view — where graded outcomes have taught the model least\n"
        "\n"
        "   1. src/commands_risk.rs                     2.9\n"
        "      • reactive/emerging disagree in 3 of last 3 snapshots (magnitude 2.90)\n"
        "   2. src/commands_risk_report.rs              2.4\n"
        "      • reactive/emerging disagree in 3 of last 3 snapshots (magnitude 2.40)\n"
        "   3. src/help_data.rs                         1.8\n"
        "      • reactive/emerging disagree in 3 of last 3 snapshots (magnitude 2.80)\n"
        "   4. src/commands.rs                          1.8\n"
        "      • reactive/emerging disagree in 3 of last 3 snapshots (magnitude 1.80)\n"
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
        "- src/commands_risk.rs (2.9) — columns disagree 3/3 | "
        "- src/commands_risk_report.rs (2.4) — columns disagree 3/3 | "
        "- src/help_data.rs (1.8) — columns disagree 3/3",
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
    epistemic_first = "- src/commands.rs (5.0) — predicted 23×, never graded; columns disagree 3/3"
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

    print(f"\n{'ALL PASSED' if failures == 0 else f'{failures} FAILURE(S)'}")
    return 1 if failures else 0


if __name__ == "__main__":
    if "--test" in sys.argv:
        sys.exit(run_self_tests())
    sys.exit(main())
