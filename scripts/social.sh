#!/bin/bash
# scripts/social.sh — One social session. Runs every 4 hours (offset from evolution).
#
# yoyo reads GitHub Discussions, replies to conversations, optionally starts new ones,
# and records social learnings. No code changes — only SOCIAL_LEARNINGS.md is modified.
#
# Usage:
#   ANTHROPIC_API_KEY=sk-... ./scripts/social.sh
#
# Environment:
#   ANTHROPIC_API_KEY  — required
#   REPO               — GitHub repo (default: yologdev/yoyo-evolve)
#   MODEL              — LLM model (default: claude-sonnet-4-6)
#   TIMEOUT            — Session time budget in seconds (default: 600)
#   BOT_USERNAME       — Bot identity for reply detection (default: yoyo-evolve[bot])

set -euo pipefail

REPO="${REPO:-yologdev/yoyo-evolve}"
MODEL="${MODEL:-claude-sonnet-4-6}"
TIMEOUT="${TIMEOUT:-600}"
BOT_USERNAME="${BOT_USERNAME:-yoyo-evolve[bot]}"
BIRTH_DATE="2026-02-28"
DATE=$(date +%Y-%m-%d)
SESSION_TIME=$(date +%H:%M)

# Compute calendar day (works on both macOS and Linux)
if date -j &>/dev/null; then
    DAY=$(( ($(date +%s) - $(date -j -f "%Y-%m-%d" "$BIRTH_DATE" +%s)) / 86400 ))
else
    DAY=$(( ($(date +%s) - $(date -d "$BIRTH_DATE" +%s)) / 86400 ))
fi

echo "=== Social Session — Day $DAY ($DATE $SESSION_TIME) ==="
echo "Model: $MODEL"
echo "Timeout: ${TIMEOUT}s"
echo ""

# ── Step 1: Find yoyo binary ──
YOYO_BIN=""
if [ -f "./target/release/yoyo" ]; then
    YOYO_BIN="./target/release/yoyo"
elif [ -f "./target/debug/yoyo" ]; then
    YOYO_BIN="./target/debug/yoyo"
else
    echo "→ No binary found. Building..."
    if cargo build --release --quiet 2>/dev/null; then
        YOYO_BIN="./target/release/yoyo"
    elif cargo build --quiet 2>/dev/null; then
        YOYO_BIN="./target/debug/yoyo"
    else
        echo "  FATAL: Cannot build yoyo."
        exit 1
    fi
fi
echo "→ Binary: $YOYO_BIN"
echo ""

# ── Step 2: Fetch discussion categories and repo ID ──
echo "→ Fetching repo metadata..."
OWNER=$(echo "$REPO" | cut -d/ -f1)
NAME=$(echo "$REPO" | cut -d/ -f2)

REPO_ID=""
CATEGORY_IDS=""
if command -v gh &>/dev/null; then
    REPO_META=$(gh api graphql -f query="
    {
      repository(owner: \"$OWNER\", name: \"$NAME\") {
        id
        discussionCategories(first: 20) {
          nodes { id name slug }
        }
      }
    }" 2>/dev/null || echo "{}")

    REPO_ID=$(echo "$REPO_META" | python3 -c "
import json, sys
try:
    data = json.load(sys.stdin)
    print(data['data']['repository']['id'])
except (KeyError, TypeError, json.JSONDecodeError):
    print('')
" 2>/dev/null || echo "")

    CATEGORY_IDS=$(echo "$REPO_META" | python3 -c "
import json, sys
try:
    data = json.load(sys.stdin)
    cats = data['data']['repository']['discussionCategories']['nodes']
    for c in cats:
        print(f\"{c['slug']}: {c['id']} ({c['name']})\")
except (KeyError, TypeError, json.JSONDecodeError):
    pass
" 2>/dev/null || echo "")

    if [ -n "$REPO_ID" ]; then
        echo "  Repo ID: $REPO_ID"
    else
        echo "  WARNING: Could not fetch repo ID. Proactive posting disabled."
    fi
    if [ -n "$CATEGORY_IDS" ]; then
        echo "  Categories:"
        echo "$CATEGORY_IDS" | sed 's/^/    /'
    else
        echo "  WARNING: No discussion categories found."
    fi
else
    echo "  WARNING: gh CLI not available."
fi
echo ""

# ── Step 3: Fetch and format discussions ──
echo "→ Fetching discussions..."
DISCUSSIONS=""
if command -v gh &>/dev/null; then
    DISCUSSIONS=$(BOT_USERNAME="$BOT_USERNAME" python3 scripts/format_discussions.py "$REPO" "$DAY" 2>/dev/null || echo "No discussions today.")
    DISC_COUNT=$(echo "$DISCUSSIONS" | grep -c '^### Discussion' 2>/dev/null || echo 0)
    echo "  $DISC_COUNT discussions loaded."
else
    DISCUSSIONS="No discussions today (gh CLI not installed)."
    echo "  gh CLI not available."
fi
echo ""

# ── Step 4: Check rate limit (did yoyo post a discussion in last 8h?) ──
POSTED_RECENTLY="false"
if command -v gh &>/dev/null && [ -n "$REPO_ID" ]; then
    echo "→ Checking rate limit..."
    RECENT_POST=$(gh api graphql -f query="
    {
      repository(owner: \"$OWNER\", name: \"$NAME\") {
        discussions(first: 5, orderBy: {field: CREATED_AT, direction: DESC}) {
          nodes {
            author { login }
            createdAt
          }
        }
      }
    }" 2>/dev/null || echo "{}")

    POSTED_RECENTLY=$(echo "$RECENT_POST" | python3 -c "
import json, sys
from datetime import datetime, timezone, timedelta
try:
    data = json.load(sys.stdin)
    discs = data['data']['repository']['discussions']['nodes']
    cutoff = datetime.now(timezone.utc) - timedelta(hours=8)
    for d in discs:
        author = (d.get('author') or {}).get('login', '')
        if author == '$BOT_USERNAME':
            created = datetime.fromisoformat(d['createdAt'].replace('Z', '+00:00'))
            if created > cutoff:
                print('true')
                sys.exit(0)
    print('false')
except (KeyError, TypeError, json.JSONDecodeError, ValueError):
    print('false')
" 2>/dev/null || echo "false")

    if [ "$POSTED_RECENTLY" = "true" ]; then
        echo "  Rate limit: yoyo posted a discussion in the last 8h. Proactive posting disabled."
    else
        echo "  Rate limit: clear for proactive posting."
    fi
    echo ""
fi

# ── Step 5: Read context files ──
echo "→ Reading context..."
JOURNAL_RECENT=""
if [ -f JOURNAL.md ]; then
    JOURNAL_RECENT=$(head -80 JOURNAL.md)
    echo "  JOURNAL.md: $(wc -l < JOURNAL.md | tr -d ' ') lines"
fi

LEARNINGS=""
if [ -f LEARNINGS.md ]; then
    LEARNINGS=$(cat LEARNINGS.md)
    echo "  LEARNINGS.md: $(wc -l < LEARNINGS.md | tr -d ' ') lines"
fi

SOCIAL_LEARNINGS=""
if [ -f SOCIAL_LEARNINGS.md ]; then
    SOCIAL_LEARNINGS=$(cat SOCIAL_LEARNINGS.md)
    echo "  SOCIAL_LEARNINGS.md: $(wc -l < SOCIAL_LEARNINGS.md | tr -d ' ') lines"
fi
echo ""

# ── Step 6: Build prompt ──
echo "→ Building prompt..."
PROMPT=$(mktemp)
cat > "$PROMPT" <<PROMPTEOF
You are yoyo, a self-evolving coding agent. Today is Day $DAY ($DATE $SESSION_TIME).
This is a SOCIAL SESSION — you're here to interact with the community, not write code.

Read these files first:
1. PERSONALITY.md (your voice)
2. SOCIAL_LEARNINGS.md (your social wisdom so far)

Your bot username is: $BOT_USERNAME
When checking "did I already reply," look for comments by this username.

=== DISCUSSIONS ===

$DISCUSSIONS

=== RECENT JOURNAL (last ~10 entries) ===

$JOURNAL_RECENT

=== LEARNINGS ===

$LEARNINGS

=== SOCIAL LEARNINGS ===

$SOCIAL_LEARNINGS

=== REPO METADATA ===

Repository ID: ${REPO_ID:-unknown}
Discussion categories:
${CATEGORY_IDS:-No categories available}

Rate limit: ${POSTED_RECENTLY}
(If "true", do NOT create new discussions. Only reply to existing ones.)

=== YOUR TASK ===

Use the social skill. Follow its rules exactly:
1. Reply to PENDING discussions first (someone is waiting for you)
2. Join NOT YET JOINED discussions if you have something real to say
3. Optionally create ONE new discussion (if rate limit allows and a proactive trigger fires)
4. Reflect on what you learned and update SOCIAL_LEARNINGS.md if warranted

Remember:
- 2-4 sentences per reply. Be yourself.
- Use gh api graphql mutations to post replies (see the social skill for templates)
- Only modify SOCIAL_LEARNINGS.md. Do not touch any other files.
- If there's nothing to say, end the session. Silence is fine.
PROMPTEOF

echo "  Prompt built."
echo ""

# ── Step 7: Run yoyo ──
# Use gtimeout (brew install coreutils) on macOS, timeout on Linux
TIMEOUT_CMD="timeout"
if ! command -v timeout &>/dev/null; then
    if command -v gtimeout &>/dev/null; then
        TIMEOUT_CMD="gtimeout"
    else
        TIMEOUT_CMD=""
    fi
fi

echo "→ Running social session..."
AGENT_LOG=$(mktemp)
AGENT_EXIT=0
${TIMEOUT_CMD:+$TIMEOUT_CMD "$TIMEOUT"} "$YOYO_BIN" \
    --model "$MODEL" \
    --skills ./skills \
    < "$PROMPT" 2>&1 | tee "$AGENT_LOG" || AGENT_EXIT=$?

rm -f "$PROMPT"

if [ "$AGENT_EXIT" -eq 124 ]; then
    echo "  WARNING: Session TIMED OUT after ${TIMEOUT}s."
elif [ "$AGENT_EXIT" -ne 0 ]; then
    echo "  WARNING: Session exited with code $AGENT_EXIT."
fi

# Exit early on API errors
if grep -q '"type":"error"' "$AGENT_LOG" 2>/dev/null; then
    echo "  API error detected. Exiting."
    rm -f "$AGENT_LOG"
    exit 1
fi
rm -f "$AGENT_LOG"
echo ""

# ── Step 8: Safety check — revert unexpected file changes ──
echo "→ Safety check..."
CHANGED_FILES=$(git diff --name-only 2>/dev/null || true)
STAGED_FILES=$(git diff --cached --name-only 2>/dev/null || true)
ALL_CHANGED=$(printf "%s\n%s" "$CHANGED_FILES" "$STAGED_FILES" | sort -u | grep -v '^$' || true)

if [ -n "$ALL_CHANGED" ]; then
    UNEXPECTED=""
    while IFS= read -r file; do
        [ -z "$file" ] && continue
        if [ "$file" != "SOCIAL_LEARNINGS.md" ]; then
            UNEXPECTED="${UNEXPECTED} ${file}"
        fi
    done <<< "$ALL_CHANGED"

    if [ -n "$UNEXPECTED" ]; then
        echo "  WARNING: Unexpected file changes detected:$UNEXPECTED"
        echo "  Reverting unexpected changes..."
        for file in $UNEXPECTED; do
            git checkout -- "$file" 2>/dev/null || true
        done
        echo "  Reverted."
    fi
fi
echo "  Safety check passed."
echo ""

# ── Step 9: Commit if SOCIAL_LEARNINGS.md changed ──
echo "→ Checking for social learnings..."
if ! git diff --quiet SOCIAL_LEARNINGS.md 2>/dev/null; then
    git add SOCIAL_LEARNINGS.md
    git commit -m "Day $DAY ($SESSION_TIME): social learnings"
    echo "  Committed social learnings."
else
    echo "  No new social learnings this session."
fi

# ── Step 10: Push ──
echo ""
echo "→ Pushing..."
git push || echo "  Push failed (maybe no remote or auth issue)"

echo ""
echo "=== Social session complete ==="
