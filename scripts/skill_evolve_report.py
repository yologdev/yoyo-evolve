#!/usr/bin/env python3
"""
skill_evolve_report.py — Layer-3 observability for skill-evolve.

Reads:
  - skills/<skill>/SKILL.md frontmatter (status, score, uses, wins, last_*)
  - skills/_journal.md (every cycle event)
  - audit-log branch session outcomes (if YOYO_AUDIT_DIR or default path is available)
  - memory/learnings.jsonl (recurrence trends)

Writes nothing — pure stdout report.

Usage:
  python3 scripts/skill_evolve_report.py
  YOYO_AUDIT_DIR=/path/to/audit/sessions python3 scripts/skill_evolve_report.py
"""

import json
import os
import re
import sys
from collections import Counter, defaultdict
from datetime import datetime, timedelta, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SKILLS_DIR = REPO_ROOT / "skills"
JOURNAL = SKILLS_DIR / "_journal.md"
LEARNINGS = REPO_ROOT / "memory" / "learnings.jsonl"


def parse_frontmatter(path: Path) -> dict:
    """Parse `key: value` YAML frontmatter. Tolerates `:` inside values
    (e.g. descriptions like `Foo: bar`) by splitting only on the FIRST `:` —
    `partition(":")` already does this; the previous bug was treating any line
    without `:` as malformed and silently dropping it. We now warn instead.
    Lists/dicts are kept as raw strings (caller handles them)."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError as e:
        print(f"WARN: cannot read {path}: {e}", file=sys.stderr)
        return {}
    m = re.match(r"---\n(.*?)\n---\n", text, re.DOTALL)
    if not m:
        print(f"WARN: no YAML frontmatter in {path}", file=sys.stderr)
        return {}
    fm = {}
    for lineno, line in enumerate(m.group(1).splitlines(), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if ":" not in stripped:
            print(f"WARN: {path}:{lineno} frontmatter line has no key: {stripped!r}", file=sys.stderr)
            continue
        k, _, v = stripped.partition(":")
        fm[k.strip()] = v.strip().strip('"').strip("'")
    return fm


def load_skills() -> list[dict]:
    out = []
    if not SKILLS_DIR.exists():
        return out
    for d in sorted(SKILLS_DIR.iterdir()):
        if not d.is_dir():
            continue
        skill_md = d / "SKILL.md"
        if not skill_md.exists():
            continue
        fm = parse_frontmatter(skill_md)
        fm["_dir"] = d.name
        out.append(fm)
    return out


def parse_journal_events() -> list[dict]:
    """Parse `## [<ts>] evt-NNNN <type>` headers + bullet `- key: value` body.
    Two header forms accepted: with timestamp (`## 2026-04-25T... evt-0042 refine`)
    or without (`## evt-0000 init` — the bootstrap form)."""
    if not JOURNAL.exists():
        return []
    try:
        text = JOURNAL.read_text(encoding="utf-8", errors="replace")
    except OSError as e:
        print(f"WARN: cannot read {JOURNAL}: {e}", file=sys.stderr)
        return []
    events = []
    dropped = 0
    for block in re.split(r"^## ", text, flags=re.MULTILINE)[1:]:
        head, *rest = block.splitlines()
        head = head.strip()
        # Try with-ts form first; fall back to evt-NNNN at start.
        m = re.match(r"(\S+)\s+(evt-\d+)\s+(\S+)", head)
        if m:
            ts, evt_id, evt_type = m.groups()
        else:
            m = re.match(r"(evt-\d+)\s+(\S+)", head)
            if m:
                ts = None
                evt_id, evt_type = m.groups()
            else:
                dropped += 1
                continue
        body = "\n".join(rest)
        fields = {"id": evt_id, "type": evt_type, "ts": ts}
        for line in body.splitlines():
            line = line.strip()
            if line.startswith("- ") and ":" in line:
                k, _, v = line[2:].partition(":")
                fields[k.strip()] = v.strip()
        events.append(fields)
    if dropped:
        print(f"WARN: dropped {dropped} unparseable journal blocks", file=sys.stderr)
    return events


def load_audit_outcomes() -> tuple[list[dict], str]:
    """Returns (outcomes, status) where status is one of:
    'ok' / 'no-branch' / 'empty' / 'all-malformed'."""
    audit_dir = os.environ.get("YOYO_AUDIT_DIR") or "/tmp/audit-read/sessions"
    base = Path(audit_dir)
    if not base.exists():
        return [], "no-branch"
    session_dirs = sorted(d for d in base.iterdir() if d.is_dir())
    if not session_dirs:
        return [], "empty"
    outcomes = []
    malformed = 0
    for session_dir in session_dirs:
        outcome_file = session_dir / "outcome.json"
        if not outcome_file.exists():
            continue
        try:
            outcomes.append(json.loads(outcome_file.read_text()))
        except (OSError, json.JSONDecodeError) as e:
            malformed += 1
            print(f"WARN: skipped {outcome_file}: {e}", file=sys.stderr)
    if not outcomes and malformed:
        return [], "all-malformed"
    return outcomes, "ok"


def load_learnings() -> list[dict]:
    if not LEARNINGS.exists():
        return []
    out = []
    malformed = 0
    with LEARNINGS.open(encoding="utf-8", errors="replace") as f:
        for lineno, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue
            try:
                out.append(json.loads(line))
            except json.JSONDecodeError as e:
                malformed += 1
                print(f"WARN: {LEARNINGS}:{lineno} bad JSON: {e}", file=sys.stderr)
    if malformed:
        print(f"WARN: dropped {malformed} malformed learnings entries", file=sys.stderr)
    return out


def days_ago(ts_str: str) -> int | None:
    if not ts_str or ts_str == "null":
        return None
    try:
        if "T" in ts_str:
            dt = datetime.fromisoformat(ts_str.replace("Z", "+00:00"))
        else:
            dt = datetime.fromisoformat(ts_str + "T00:00:00+00:00")
        return (datetime.now(timezone.utc) - dt).days
    except (ValueError, TypeError):
        return None


def section(title: str) -> None:
    print()
    print(f"━━━ {title} ".ljust(72, "━"))


def report_skills(skills: list[dict]) -> None:
    section("Per-skill snapshot")
    # Eligibility for skill-evolve: origin == 'yoyo' AND core != 'true'.
    print(
        f"{'name':<14} {'origin':<11} {'status':<11} {'score':>6} {'uses':>5} "
        f"{'wins':>5} {'last_used':<12} {'last_evolved':<12} {'eligible':<8}"
    )
    print("-" * 92)
    for s in skills:
        is_core = (s.get("core", "").lower() == "true")
        is_yoyo = (s.get("origin", "") == "yoyo")
        eligible = "yes" if (is_yoyo and not is_core) else "no"
        print(
            f"{s.get('_dir', '?'):<14} "
            f"{s.get('origin', '-'):<11} "
            f"{s.get('status', '-'):<11} "
            f"{s.get('score', '-'):>6} "
            f"{s.get('uses', '-'):>5} "
            f"{s.get('wins', '-'):>5} "
            f"{s.get('last_used', '-'):<12} "
            f"{s.get('last_evolved', '-'):<12} "
            f"{eligible:<8}"
        )


def report_events(events: list[dict]) -> None:
    section("Skill-evolution events (most recent 10)")
    if not events:
        print("(no events)")
        return
    type_counts = Counter(e["type"] for e in events)
    print("Type counts: " + ", ".join(f"{t}={n}" for t, n in type_counts.most_common()))
    print()
    for e in events[-10:]:
        skill = e.get("skill", "-")
        trigger = (e.get("trigger") or "")[:50]
        delta = e.get("score-delta", "-")
        print(f"  {e['id']:<10} {e['type']:<16} skill={skill:<12} score={delta:<14} {trigger}")

    # Saturation flag
    last_three = [e["type"] for e in events[-3:]]
    if last_three == ["NO-OP"] * 3:
        print()
        print("  ⚠ Last 3 events are NO-OP — saturation likely. Cooldown should auto-extend.")


def report_outcomes(outcomes: list[dict], status: str) -> None:
    section("Session outcomes (audit-log branch)")
    if status == "no-branch":
        print("(audit-log branch not fetched at $YOYO_AUDIT_DIR — set the env var or fetch the branch first)")
        return
    if status == "empty":
        print("(audit-log branch present but contains no session directories yet)")
        return
    if status == "all-malformed":
        print("(audit-log branch has session dirs but every outcome.json is malformed — see WARN lines on stderr)")
        return
    total = len(outcomes)
    if total == 0:
        print("(audit-log branch present, session dirs exist, but none contain outcome.json)")
        return
    builds = sum(1 for o in outcomes if o.get("build_ok"))
    tests = sum(1 for o in outcomes if o.get("test_ok"))
    reverted = sum(1 for o in outcomes if o.get("reverted"))
    avg_succeeded = sum(o.get("tasks_succeeded", 0) for o in outcomes) / total if total else 0
    avg_attempted = sum(o.get("tasks_attempted", 0) for o in outcomes) / total if total else 0
    print(f"sessions={total}  build_ok={builds}/{total}  test_ok={tests}/{total}  reverted={reverted}/{total}")
    print(f"avg tasks: succeeded={avg_succeeded:.2f}  attempted={avg_attempted:.2f}")
    # Issue #501: applied-signal ("applied, not just recalled"). Counts distinct
    # sessions per pattern_key that declared it in outcome.json.applied_pattern_keys.
    applied_sessions = [o for o in outcomes if o.get("applied_pattern_keys")]
    applied: Counter = Counter()
    for o in applied_sessions:
        keys = o.get("applied_pattern_keys") or []
        if not isinstance(keys, list):
            continue  # tolerate hand-edited / future-format outcome.json on the audit branch
        for k in keys:
            if isinstance(k, str):
                applied[k] += 1
    print(f"applied-signal: {len(applied_sessions)}/{total} sessions tagged · {len(applied)} distinct pattern_keys")
    if applied:
        print("top applied: " + ", ".join(f"{k}×{n}" for k, n in applied.most_common(5)))


def report_recurrence(learnings: list[dict]) -> None:
    section("Pattern-key recurrence (last 30 vs previous 30 days)")
    if not learnings:
        print("(no learnings)")
        return

    now = datetime.now(timezone.utc)
    recent: Counter = Counter()
    previous: Counter = Counter()
    for entry in learnings:
        ts = entry.get("ts")
        pk = entry.get("pattern_key") or entry.get("title", "").strip().lower()[:40]
        if not pk or not ts:
            continue
        try:
            dt = datetime.fromisoformat(ts.replace("Z", "+00:00"))
        except (ValueError, TypeError):
            continue
        delta = (now - dt).days
        if delta <= 30:
            recent[pk] += 1
        elif delta <= 60:
            previous[pk] += 1

    overlap = set(recent) & set(previous)
    print(f"recent unique keys: {len(recent)}")
    print(f"previous unique keys: {len(previous)}")
    print(f"keys appearing in both windows: {len(overlap)} (lower over time = yoyo internalizing patterns)")
    if recent:
        print(f"top recent: {', '.join(k for k, _ in recent.most_common(5))}")


def main() -> int:
    skills = load_skills()
    events = parse_journal_events()
    outcomes, outcomes_status = load_audit_outcomes()
    learnings = load_learnings()

    print(f"skill-evolve report — {datetime.now(timezone.utc).isoformat(timespec='seconds')}")
    print(f"repo: {REPO_ROOT}")

    report_skills(skills)
    report_events(events)
    report_outcomes(outcomes, outcomes_status)
    report_recurrence(learnings)

    return 0


if __name__ == "__main__":
    sys.exit(main())
