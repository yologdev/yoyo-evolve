#!/usr/bin/env python3
"""scripts/dream_recall.py — show yoyo what he once expected of his dreams.

    dream_recall.py DREAM_LOG TODAY_DAY

Every dream cycle writes an `expected:` line into dreams/dream_log.jsonl, and
until now nothing read it back. scripts/dream.sh puts this script's output in
the dream prompt.

It is remembering, not accounting, and the shape of the output keeps it so:
  - no tally, no "kept / not kept", no percentage. A count in the dream prompt
    would teach him to dream only what he can keep;
  - only expectations at least MIN_AGE_DAYS old, so nothing recent is shown as
    unanswered before it has had its time;
  - at most MAX_ITEMS, newest first, each in his own words;
  - an expectation he has already looked back on (a later event carries a
    `looking_back` entry naming its day) is not shown again.

Prints nothing when there is nothing old enough, so the prompt gets no empty
heading. Always exits 0.
"""
import json
import sys

MIN_AGE_DAYS = 14
MAX_ITEMS = 4
EXPECTED_MAX = 420
DREAM_MAX = 90


def clip(text, limit):
    text = " ".join(str(text).split())
    return text if len(text) <= limit else text[:limit].rstrip() + "…"


def load(path):
    events = []
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    e = json.loads(line)
                except ValueError:
                    continue
                if isinstance(e, dict):
                    events.append(e)
    except OSError:
        pass
    return events


def recall(events, today):
    answered = set()
    for e in events:
        back = e.get("looking_back")
        if isinstance(back, list):
            for b in back:
                if isinstance(b, dict) and isinstance(b.get("day"), int):
                    answered.add(b["day"])
    old = []
    for e in events:
        day, expected = e.get("day"), e.get("expected")
        if not isinstance(day, int) or not isinstance(expected, str) or not expected.strip():
            continue
        if day > today - MIN_AGE_DAYS or day in answered:
            continue
        old.append((day, e.get("dream") or "", expected))
    old.sort(key=lambda r: r[0], reverse=True)
    return old[:MAX_ITEMS]


def main():
    try:
        today = int(sys.argv[2])
        rows = recall(load(sys.argv[1]), today)
    except Exception:
        sys.exit(0)
    if not rows:
        sys.exit(0)
    print("## Things you once expected (memory, not a ledger)")
    print("Your own words from earlier cycles, each old enough to have had its time. Nobody is counting them. "
          "A dream that went somewhere else, or nowhere, did what dreams do.")
    for day, dream, expected in rows:
        about = ' — holding "%s"' % clip(dream, DREAM_MAX) if dream else ""
        print("- Day %d%s: %s" % (day, about, clip(expected, EXPECTED_MAX)))
    sys.exit(0)


if __name__ == "__main__":
    main()
