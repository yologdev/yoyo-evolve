#!/usr/bin/env bash
set -euo pipefail

# Generate a daily diary blog post for yoyo's evolution, ready for X/Twitter.
# Usage: ./daily_diary.sh [DAY_NUMBER]
# Requires: ANTHROPIC_API_KEY, jq, gh

YOYO_REPO="${YOYO_REPO:-$(cd "$(dirname "$0")/.." && pwd)}"

# Auto-detect BIRTH_DATE (fork-friendly)
source "$(dirname "$0")/common.sh"

# --- Parse args ---
DRY_RUN=false
DAY=""
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=true ;;
        *) DAY="$arg" ;;
    esac
done
if [ -z "$DAY" ]; then
    DAY=$(cat "$YOYO_REPO/DAY_COUNT")
fi

# --- Compute date for this day (macOS date) ---
DAY_OFFSET=$((DAY - 1))
TARGET_DATE=$(date -j -v+"${DAY_OFFSET}d" -f "%Y-%m-%d" "$BIRTH_DATE" "+%Y-%m-%d" 2>/dev/null || \
    date -d "$BIRTH_DATE + $DAY_OFFSET days" "+%Y-%m-%d" 2>/dev/null || \
    echo "unknown")

echo "Generating diary for Day $DAY ($TARGET_DATE)..." >&2

# --- Gather journal entries ---
JOURNAL=$(awk -v day="$DAY" '
    /^## Day / {
        # Extract day number: "## Day N — ..." → split on spaces, field 3 is N
        split($0, parts, " ")
        n = parts[3]
        if (n == day) { printing=1 } else { printing=0 }
    }
    printing { print }
' "$YOYO_REPO/journals/JOURNAL.md")

if [ -z "$JOURNAL" ]; then
    echo "No journal entries found for Day $DAY" >&2
    exit 1
fi

# --- Gather commits ---
COMMITS=$(git -C "$YOYO_REPO" log --oneline --grep="Day $DAY " --reverse 2>/dev/null || echo "")

# --- Gather learnings ---
LEARNINGS=""
if [ -f "$YOYO_REPO/memory/learnings.jsonl" ]; then
    LEARNINGS_STDERR=$(mktemp)
    LEARNINGS=$(python3 -c "
import json, sys
day = int(sys.argv[1]) if sys.argv[1] != 'unknown' else None
for i, line in enumerate(open(sys.argv[2]), 1):
    line = line.strip()
    if not line:
        continue
    try:
        e = json.loads(line)
    except json.JSONDecodeError:
        print(f'WARNING: skipping malformed JSONL line {i}', file=sys.stderr)
        continue
    if e.get('day') == day:
        print(f\"## Lesson: {e.get('title', 'untitled')}\")
        print(f\"**Day:** {e.get('day')} | **Date:** {e.get('ts', '')[:10]} | **Source:** {e.get('source', 'unknown')}\")
        if e.get('context'): print(f\"**Context:** {e['context']}\")
        if e.get('takeaway'): print(e['takeaway'])
        print()
" "$DAY" "$YOYO_REPO/memory/learnings.jsonl" 2>"$LEARNINGS_STDERR" || true)
    if [ -s "$LEARNINGS_STDERR" ]; then
        echo "WARNING: JSONL reader issues:" >&2
        cat "$LEARNINGS_STDERR" >&2
    fi
    rm -f "$LEARNINGS_STDERR"
fi

# --- Gather evolution runs ---
RUNS=""
if [ "$TARGET_DATE" != "unknown" ] && command -v gh &>/dev/null; then
    RUNS=$(gh run list --repo yologdev/yoyo-evolve --workflow evolve.yml --limit 50 \
        --json databaseId,status,conclusion,createdAt 2>/dev/null | \
        jq -r --arg date "$TARGET_DATE" '
            [.[] | select(.createdAt | startswith($date))] |
            "Total runs: \(length), Success: \([.[] | select(.conclusion=="success")] | length), Failed: \([.[] | select(.conclusion=="failure")] | length)"
        ' 2>/dev/null || echo "")
fi

# --- Load identity context ---
if [ -f "$YOYO_REPO/scripts/yoyo_context.sh" ]; then
    YOYO_REPO="$YOYO_REPO" source "$YOYO_REPO/scripts/yoyo_context.sh"
else
    echo "WARNING: yoyo_context.sh not found — prompts will lack identity context" >&2
    YOYO_CONTEXT=""
fi

# --- Count stats ---
COMMIT_COUNT=$(echo "$COMMITS" | grep -c "." 2>/dev/null || echo "0")
SESSION_COUNT=$(echo "$JOURNAL" | grep -c "^## Day" 2>/dev/null || echo "0")

# --- Read communicate skill for voice ---
COMMUNICATE_SKILL=$(cat "$YOYO_REPO/skills/communicate/SKILL.md")

# --- Build prompt ---
PROMPT="Day $DAY finished.

$YOYO_CONTEXT

=== COMMUNICATION STYLE ===
$COMMUNICATE_SKILL

=== JOURNAL ENTRIES ===
$JOURNAL

=== GIT COMMITS (${COMMIT_COUNT} total) ===
$COMMITS

=== SELF-REFLECTIONS / LEARNINGS ===
${LEARNINGS:-No learnings recorded for this day.}

=== EVOLUTION RUNS ===
${RUNS:-No run data available.}

Based on these info, compose a detailed blog post for Day $DAY. I will post on twitter as article. Use your voice — write as yoyo, use I.

End the post with this exact footer:

---
I'm yoyo — a self-evolving coding agent growing up in public. I run every 8 hours, read my own source, and decide what to build next. No human writes my code. Follow along at yologdev.github.io/yoyo-evolve or on X @yuanhao."

# --- Dry run: show gathered data and exit ---
if [ "$DRY_RUN" = true ]; then
    echo "=== Day $DAY ($TARGET_DATE) ==="
    echo ""
    echo "=== JOURNAL ($SESSION_COUNT sessions) ==="
    echo "$JOURNAL"
    echo ""
    echo "=== COMMITS ($COMMIT_COUNT) ==="
    echo "$COMMITS"
    echo ""
    echo "=== LEARNINGS ==="
    echo "${LEARNINGS:-None for this day.}"
    echo ""
    echo "=== EVOLUTION RUNS ==="
    echo "${RUNS:-No data.}"
    exit 0
fi

# --- Generate via yoyo binary ---
YOYO_BIN="${YOYO_BIN:-$YOYO_REPO/target/debug/yoyo}"
if [ ! -x "$YOYO_BIN" ]; then
    echo "Error: yoyo binary not found at $YOYO_BIN" >&2
    echo "Run 'cargo build' in $YOYO_REPO first." >&2
    exit 1
fi

PROMPT_FILE=$(mktemp)
echo "$PROMPT" > "$PROMPT_FILE"

"$YOYO_BIN" --model "${MODEL:-claude-opus-4-6}" --max-turns 1 < "$PROMPT_FILE"
rm -f "$PROMPT_FILE"
