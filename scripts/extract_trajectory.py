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
        lines.append(f"\"{truncated_title}\": {attempts} attempt(s), last day-{last_day}")

    if stuck_titles:
        lines.append("")
        lines.append("⚠️ Possibly stuck (≥{} attempts in window):".format(STUCK_ON_THRESHOLD))
        for title, attempts, days in stuck_titles[:3]:
            t = title[:60] + ("…" if len(title) > 60 else "")
            lines.append(f"  - \"{t}\": {attempts}× (days {min(days)}-{max(days)})")
    return "\n".join(lines)


# ── Section 3: Reverts in window (already counted above) ─────────────────


def render_reverts(reverts: int, total_sessions: int) -> str:
    if total_sessions == 0:
        return ""
    if reverts == 0:
        return f"## Reverts in window\n0 of last ~{total_sessions} sessions had reverts."
    return f"## Reverts in window\n{reverts} revert commit(s) in last {WINDOW_DAYS} days."


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


def compact_epistemic_reason(reason: str) -> str:
    """Shrink a verbose report reason to a few words (2KB total budget)."""
    m = EPISTEMIC_DISAGREE_RE.search(reason)
    if m:
        return f"columns disagree {m.group(1)}/{m.group(2)}"
    m = EPISTEMIC_STALE_RE.search(reason)
    if m:
        return f"stale ({m.group(1)} snapshots)"
    return reason[:60]


def parse_epistemic_output(text: str, top_n: int = EPISTEMIC_TOP_N) -> list[str]:
    """Parse `yoyo risk epistemic` report output into compact bullet lines.

    Pure (no subprocess) so the self-tests can exercise it on canned output.
    Returns [] when nothing parseable is found (empty states, garbage).
    """
    entries: list[tuple[str, str, list[str]]] = []
    for raw in strip_ansi(text).splitlines():
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
            line += " — " + "; ".join(reasons)
        if len(line) > EPISTEMIC_ENTRY_MAX_CHARS:
            line = line[: EPISTEMIC_ENTRY_MAX_CHARS - 1] + "…"
        out.append(line)
    return out


def find_yoyo_binary() -> str | None:
    """Locate the built yoyo binary (evolve.sh builds before running us)."""
    for p in ("target/debug/yoyo", "target/release/yoyo"):
        if Path(p).is_file() and os.access(p, os.X_OK):
            return p
    return None


def collect_epistemic_blind_spots() -> list[str]:
    """Run `yoyo risk epistemic` and compact its top entries. Fail-soft:
    every skip path warn()s (fail-soft without a freshness signal is
    fail-silent — Day 139) and returns []."""
    binary = find_yoyo_binary()
    if binary is None:
        warn("epistemic section skipped: yoyo binary not found in target/{debug,release}")
        return []
    rc, stdout, _stderr = run_cmd([binary, "risk", "epistemic"], timeout=15)
    if rc != 0:
        warn(f"epistemic section skipped: `{binary} risk epistemic` rc={rc}")
        return []
    entries = parse_epistemic_output(stdout)
    if not entries:
        warn("epistemic section skipped: no parseable entries in report output")
        return []
    return entries


def render_epistemic(entries: list[str]) -> str:
    """≤8 lines incl. header. Empty entries → honest one-line fallback so the
    planner sees the section exists but is starving, not silence."""
    header = "## Epistemic blind spots (files graded outcomes have taught the model least about)"
    if not entries:
        return header + "\n(no epistemic data yet)"
    lines = [header] + entries[:EPISTEMIC_TOP_N]
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
    s = render_reverts(reverts, len(outcomes))
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
    epistemic_entries = collect_epistemic_blind_spots()
    if sections or epistemic_entries:
        sections.append(render_epistemic(epistemic_entries))

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
    parsed = parse_epistemic_output(canned)
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

    # 19c. Empty states / garbage yield [] (the fail-soft path)
    assert_true(
        "empty-state report yields no entries",
        parse_epistemic_output("  no snapshots yet — run `yoyo risk snapshot` first\n") == [],
    )
    assert_true("garbage yields no entries", parse_epistemic_output("total junk\n42\n") == [])

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
    clamped = parse_epistemic_output(f"   1. {long_path}   5.0\n")
    assert_true(
        "oversized entry clamped to 90 chars",
        len(clamped) == 1 and len(clamped[0]) <= EPISTEMIC_ENTRY_MAX_CHARS,
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
    synth = (
        synth_header
        + "\n"
        + "\n\n".join(
            [outcomes_sec, tasks_sec, reverts_sec, ci_sec, provider_sec, epistemic_sec]
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

    # 22. Oversized input still gets the marker and stays under the cap
    huge = synth + ("\n## Padding\n" + "z" * 4000)
    huge_capped = cap_output(huge)
    assert_true(
        "oversized input capped with marker",
        len(huge_capped.encode("utf-8")) <= TOTAL_BYTE_CAP
        and "(truncated to fit token budget)" in huge_capped,
    )

    print(f"\n{'ALL PASSED' if failures == 0 else f'{failures} FAILURE(S)'}")
    return 1 if failures else 0


if __name__ == "__main__":
    if "--test" in sys.argv:
        sys.exit(run_self_tests())
    sys.exit(main())
