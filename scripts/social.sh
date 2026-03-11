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

# Validate dependencies
if ! command -v python3 &>/dev/null; then
    echo "FATAL: python3 is required but not found."
    exit 1
fi

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
    BUILD_STDERR=$(mktemp)
    if cargo build --release --quiet 2>"$BUILD_STDERR"; then
        YOYO_BIN="./target/release/yoyo"
    elif cargo build --quiet 2>"$BUILD_STDERR"; then
        YOYO_BIN="./target/debug/yoyo"
    else
        echo "  FATAL: Cannot build yoyo."
        cat "$BUILD_STDERR" | sed 's/^/    /'
        rm -f "$BUILD_STDERR"
        exit 1
    fi
    rm -f "$BUILD_STDERR"
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
    META_STDERR=$(mktemp)
    REPO_META=$(gh api graphql \
        -f query='query($owner: String!, $name: String!) {
          repository(owner: $owner, name: $name) {
            id
            discussionCategories(first: 20) {
              nodes { id name slug }
            }
          }
        }' \
        -f owner="$OWNER" \
        -f name="$NAME" \
        2>"$META_STDERR") || {
        echo "  WARNING: GraphQL metadata query failed:"
        cat "$META_STDERR" | sed 's/^/    /'
        REPO_META="{}"
    }
    rm -f "$META_STDERR"

    REPO_ID=$(echo "$REPO_META" | python3 -c "
import json, sys
try:
    data = json.load(sys.stdin)
    print(data['data']['repository']['id'])
except (KeyError, TypeError, json.JSONDecodeError):
    print('')
" || echo "")

    CATEGORY_IDS=$(echo "$REPO_META" | python3 -c "
import json, sys
try:
    data = json.load(sys.stdin)
    cats = data['data']['repository']['discussionCategories']['nodes']
    for c in cats:
        print(f\"{c['slug']}: {c['id']} ({c['name']})\")
except (KeyError, TypeError, json.JSONDecodeError):
    pass
" || echo "")

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
    DISC_STDERR=$(mktemp)
    DISCUSSIONS=$(BOT_USERNAME="$BOT_USERNAME" python3 scripts/format_discussions.py "$REPO" "$DAY" 2>"$DISC_STDERR") || {
        echo "  WARNING: format_discussions.py failed:"
        cat "$DISC_STDERR" | sed 's/^/    /'
        DISCUSSIONS="No discussions today."
    }
    if [ -s "$DISC_STDERR" ]; then
        echo "  Stderr from format_discussions.py:"
        cat "$DISC_STDERR" | sed 's/^/    /'
    fi
    rm -f "$DISC_STDERR"
    DISC_COUNT=$(echo "$DISCUSSIONS" | grep -c '^### Discussion' 2>/dev/null || echo 0)
    echo "  $DISC_COUNT discussions loaded."
else
    DISCUSSIONS="No discussions today (gh CLI not installed)."
    echo "  gh CLI not available."
fi
echo ""

# ── Step 4: Check rate limit (did yoyo post a discussion in last 8h?) ──
# Safe default: assume rate-limited until proven otherwise
POSTED_RECENTLY="true"
if command -v gh &>/dev/null && [ -n "$REPO_ID" ]; then
    echo "→ Checking rate limit..."
    RATE_STDERR=$(mktemp)
    RECENT_POST=$(gh api graphql \
        -f query='query($owner: String!, $name: String!) {
          repository(owner: $owner, name: $name) {
            discussions(first: 10, orderBy: {field: CREATED_AT, direction: DESC}) {
              nodes {
                author { login }
                createdAt
              }
            }
          }
        }' \
        -f owner="$OWNER" \
        -f name="$NAME" \
        2>"$RATE_STDERR") || {
        echo "  WARNING: Rate limit query failed:"
        cat "$RATE_STDERR" | sed 's/^/    /'
        RECENT_POST="{}"
    }
    rm -f "$RATE_STDERR"

    POSTED_RECENTLY=$(echo "$RECENT_POST" | BOT_USERNAME="$BOT_USERNAME" python3 -c "
import json, sys, os
from datetime import datetime, timezone, timedelta
bot_username = os.environ.get('BOT_USERNAME', 'yoyo-evolve[bot]')
try:
    data = json.load(sys.stdin)
    discs = data['data']['repository']['discussions']['nodes']
    cutoff = datetime.now(timezone.utc) - timedelta(hours=8)
    for d in discs:
        author = (d.get('author') or {}).get('login', '')
        if author == bot_username:
            created = datetime.fromisoformat(d['createdAt'].replace('Z', '+00:00'))
            if created > cutoff:
                print('true')
                sys.exit(0)
    print('false')
except (KeyError, TypeError, json.JSONDecodeError, ValueError):
    print('true')
" || echo "true")

    if [ "$POSTED_RECENTLY" = "true" ]; then
        echo "  Rate limit: yoyo posted a discussion in the last 8h (or check failed). Proactive posting disabled."
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

=== RECENT JOURNAL (first 80 lines) ===

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
        echo "  WARNING: Neither 'timeout' nor 'gtimeout' found. Session will run WITHOUT time limit."
    fi
fi

echo "→ Running social session..."
AGENT_LOG=$(mktemp)
set +o errexit
${TIMEOUT_CMD:+$TIMEOUT_CMD "$TIMEOUT"} "$YOYO_BIN" \
    --model "$MODEL" \
    --skills ./skills \
    < "$PROMPT" 2>&1 | tee "$AGENT_LOG"
AGENT_EXIT=${PIPESTATUS[0]}
set -o errexit

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
UNTRACKED_FILES=$(git ls-files --others --exclude-standard 2>/dev/null || true)
ALL_CHANGED=$(printf "%s\n%s\n%s" "$CHANGED_FILES" "$STAGED_FILES" "$UNTRACKED_FILES" | sort -u | grep -v '^$' || true)

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
        REVERT_FAILED=""
        for file in $UNEXPECTED; do
            # Unstage first if staged
            git reset HEAD -- "$file" 2>/dev/null || true
            if git checkout -- "$file" 2>/dev/null; then
                echo "    Reverted: $file"
            elif [ -e "$file" ] && ! git ls-files --error-unmatch "$file" 2>/dev/null; then
                # Untracked file — remove it
                rm -f "$file"
                echo "    Removed untracked: $file"
            else
                REVERT_FAILED="${REVERT_FAILED} ${file}"
                echo "    FAILED to revert: $file"
            fi
        done
        if [ -n "$REVERT_FAILED" ]; then
            echo "  FATAL: Could not revert all unexpected changes:$REVERT_FAILED"
            exit 1
        fi
        echo "  All unexpected changes reverted."
    fi
fi
echo "  Safety check passed."
echo ""

# ── Step 9: Commit if SOCIAL_LEARNINGS.md changed ──
echo "→ Checking for social learnings..."
if ! git diff --quiet SOCIAL_LEARNINGS.md 2>/dev/null; then
    git add SOCIAL_LEARNINGS.md
    if ! git commit -m "Day $DAY ($SESSION_TIME): social learnings"; then
        echo "  ERROR: Failed to commit social learnings (check pre-commit hooks or signing requirements)."
        exit 1
    fi
    echo "  Committed social learnings."

    # ── Step 10: Push ──
    echo ""
    echo "→ Pushing..."
    if ! git push; then
        echo "  ERROR: Push failed. Social learnings committed locally but will be lost in ephemeral CI."
        exit 1
    fi
else
    echo "  No new social learnings this session."
fi

echo ""
echo "=== Social session complete ==="
