#!/bin/bash
# scripts/evolve.sh — One evolution cycle. Run every 8 hours via GitHub Actions or manually.
#
# Usage:
#   ANTHROPIC_API_KEY=sk-... ./scripts/evolve.sh
#
# Environment:
#   ANTHROPIC_API_KEY  — required
#   REPO               — GitHub repo (default: yologdev/yoyo-evolve)
#   MODEL              — LLM model (default: claude-opus-4-6)
#   TIMEOUT            — Max session time in seconds (default: 3600)

set -euo pipefail

REPO="${REPO:-yologdev/yoyo-evolve}"
MODEL="${MODEL:-claude-opus-4-6}"
TIMEOUT="${TIMEOUT:-3600}"
BIRTH_DATE="2026-02-28"
DATE=$(date +%Y-%m-%d)
SESSION_TIME=$(date +%H:%M)
# Compute calendar day (works on both macOS and Linux)
if date -j &>/dev/null; then
    DAY=$(( ($(date +%s) - $(date -j -f "%Y-%m-%d" "$BIRTH_DATE" +%s)) / 86400 ))
else
    DAY=$(( ($(date +%s) - $(date -d "$BIRTH_DATE" +%s)) / 86400 ))
fi
echo "$DAY" > DAY_COUNT

echo "=== Day $DAY ($DATE $SESSION_TIME) ==="
echo "Model: $MODEL"
echo "Timeout: ${TIMEOUT}s"
echo ""

# ── Step 1: Verify starting state ──
echo "→ Checking build..."
cargo build --quiet
cargo test --quiet
echo "  Build OK."
echo ""

# ── Step 2: Check previous CI status ──
CI_STATUS_MSG=""
if command -v gh &>/dev/null; then
    echo "→ Checking previous CI run..."
    CI_CONCLUSION=$(gh run list --repo "$REPO" --workflow ci.yml --limit 1 --json conclusion --jq '.[0].conclusion' 2>/dev/null || echo "unknown")
    if [ "$CI_CONCLUSION" = "failure" ]; then
        CI_RUN_ID=$(gh run list --repo "$REPO" --workflow ci.yml --limit 1 --json databaseId --jq '.[0].databaseId' 2>/dev/null || echo "")
        CI_LOGS=""
        if [ -n "$CI_RUN_ID" ]; then
            CI_LOGS=$(gh run view "$CI_RUN_ID" --repo "$REPO" --log-failed 2>/dev/null | tail -30 || echo "Could not fetch logs.")
        fi
        CI_STATUS_MSG="Previous CI run FAILED. Error logs:
$CI_LOGS"
        echo "  CI: FAILED — agent will be told to fix this first."
    else
        echo "  CI: $CI_CONCLUSION"
    fi
    echo ""
fi

# ── Step 3: Fetch GitHub issues ──
ISSUES_FILE="ISSUES_TODAY.md"
echo "→ Fetching community issues..."
if command -v gh &>/dev/null; then
    gh issue list --repo "$REPO" \
        --state open \
        --label "agent-input" \
        --limit 10 \
        --json number,title,body,labels,reactionGroups \
        > /tmp/issues_raw.json 2>/dev/null || true

    python3 scripts/format_issues.py /tmp/issues_raw.json > "$ISSUES_FILE" 2>/dev/null || echo "No issues found." > "$ISSUES_FILE"
    echo "  $(grep -c '^### Issue' "$ISSUES_FILE" 2>/dev/null || echo 0) issues loaded."
else
    echo "  gh CLI not available. Skipping issue fetch."
    echo "No issues available (gh CLI not installed)." > "$ISSUES_FILE"
fi
echo ""

# ── Step 4: Run evolution session ──
SESSION_START_SHA=$(git rev-parse HEAD)
echo "→ Starting evolution session..."
echo ""

# Use gtimeout (brew install coreutils) on macOS, timeout on Linux
TIMEOUT_CMD="timeout"
if ! command -v timeout &>/dev/null; then
    if command -v gtimeout &>/dev/null; then
        TIMEOUT_CMD="gtimeout"
    else
        TIMEOUT_CMD=""
    fi
fi

PROMPT_FILE=$(mktemp)
cat > "$PROMPT_FILE" <<PROMPT
Today is Day $DAY ($DATE $SESSION_TIME).

Read these files in this order:
1. IDENTITY.md (who you are and your rules)
2. src/main.rs (your current source code — this is YOU)
3. JOURNAL.md (your recent history — last 10 entries)
4. ISSUES_TODAY.md (community requests)
${CI_STATUS_MSG:+
=== CI STATUS ===
⚠️ PREVIOUS CI FAILED. Fix this FIRST before any new work.
$CI_STATUS_MSG
}
=== PHASE 1: Self-Assessment ===

Read your own source code carefully. Then try a small task to test
yourself — for example, read a file, edit something, run a command.
Note any friction, bugs, crashes, or missing capabilities.

=== PHASE 2: Review Community Issues ===

Read ISSUES_TODAY.md. These are real people asking you to improve.
Issues with more 👍 reactions should be prioritized higher.

=== PHASE 3: Decide ===

Make as many improvements as you can this session. Prioritize:
0. Fix CI failures (if any — this overrides everything else)
1. Self-discovered crash or data loss bug
2. Community issue with most 👍 (if actionable today)
3. Self-discovered UX friction or missing error handling
4. Whatever you think will make you most useful to real developers

=== PHASE 4: Implement ===

For each improvement, follow the evolve skill rules:
- Write a test first if possible
- Use edit_file for surgical changes
- Run cargo fmt && cargo clippy --all-targets -- -D warnings && cargo build && cargo test after changes
- If any check fails, read the error and fix it. Keep trying until it passes.
- Only if you've tried 3+ times and are stuck, revert this change with: git checkout -- . (keeps previous commits)
- After ALL checks pass, commit: git add -A && git commit -m "Day $DAY ($SESSION_TIME): <short description>"
- Then move on to the next improvement

=== PHASE 5: Journal (MANDATORY — DO NOT SKIP) ===

This is NOT optional. You MUST write a journal entry before the session ends.

Write today's entry at the TOP of JOURNAL.md (above all existing entries). Format:
## Day $DAY — $SESSION_TIME — [title]
[2-4 sentences: what you tried, what worked, what didn't, what's next]

Then commit it: git add JOURNAL.md && git commit -m "Day $DAY ($SESSION_TIME): journal entry"

If you skip the journal, you have failed the session — even if all code changes succeeded.

=== PHASE 6: Issue Response ===

If you worked on a community GitHub issue, write to ISSUE_RESPONSE.md:
issue_number: [N]
status: fixed|partial|wontfix
comment: [your 2-3 sentence response to the person]

=== REMINDER ===
You have internet access via bash (curl). If you're implementing
something unfamiliar, research it first. Check LEARNINGS.md before
searching — you may have looked this up before. Write new findings
to LEARNINGS.md.

Now begin. Read IDENTITY.md first.
PROMPT

${TIMEOUT_CMD:+$TIMEOUT_CMD "$TIMEOUT"} cargo run -- \
    --model "$MODEL" \
    --skills ./skills \
    < "$PROMPT_FILE" || true

rm -f "$PROMPT_FILE"

echo ""
echo "→ Session complete. Checking results..."

# ── Step 6: Verify build ──
# Agent is told to run fmt + clippy + build + test before each commit.
# But if it didn't, we auto-fix what we can and revert what we can't.

# Auto-fix formatting (never worth reverting over)
if ! cargo fmt -- --check 2>/dev/null; then
    echo "  Formatting issues — auto-fixing with cargo fmt..."
    cargo fmt
    git add -A && git commit -m "Day $DAY ($SESSION_TIME): cargo fmt" || true
fi

if cargo build --quiet 2>/dev/null && cargo test --quiet 2>/dev/null && cargo clippy --quiet --all-targets -- -D warnings 2>/dev/null; then
    echo "  Build: PASS"
else
    echo "  Build: FAIL — finding last good commit..."
    # Walk through session commits to find the last one that passes
    GOOD_SHA=""
    for SHA in $(git log --reverse --format="%H" "$SESSION_START_SHA"..HEAD); do
        git checkout --quiet "$SHA" -- src/
        cargo fmt --quiet
        if cargo build --quiet 2>/dev/null && cargo test --quiet 2>/dev/null && cargo clippy --quiet --all-targets -- -D warnings 2>/dev/null; then
            GOOD_SHA="$SHA"
        else
            break
        fi
    done

    if [ -n "$GOOD_SHA" ]; then
        echo "  Keeping good commits up to $(git log --oneline -1 "$GOOD_SHA" | head -c 60)"
        git checkout "$GOOD_SHA" -- src/
        cargo fmt --quiet
        git add -A && git commit -m "Day $DAY ($SESSION_TIME): revert broken changes, keep passing commits" || true
    else
        echo "  No good commits found — resetting to pre-session state"
        git checkout "$SESSION_START_SHA" -- src/
        git add -A && git commit -m "Day $DAY ($SESSION_TIME): revert all session changes (build failed)" || true
    fi
fi

# ── Step 6b: Verify journal was written ──
if ! grep -q "## Day $DAY.*$SESSION_TIME" JOURNAL.md 2>/dev/null; then
    echo "  WARNING: No journal entry for Day $DAY ($SESSION_TIME) — agent skipped the journal!"
    # Write a minimal fallback entry (only commits from THIS session)
    COMMITS=$(git log --oneline "$SESSION_START_SHA"..HEAD --format="%s" | grep -v "session wrap-up" | sed "s/Day $DAY[^:]*: //" | paste -sd ", " -)
    if [ -z "$COMMITS" ]; then
        COMMITS="no commits made"
    fi
    TMPJ=$(mktemp)
    {
        echo "# Journal"
        echo ""
        echo "## Day $DAY — $SESSION_TIME — (auto-generated, agent skipped journal)"
        echo ""
        echo "Session commits: $COMMITS."
        echo ""
        tail -n +2 JOURNAL.md
    } > "$TMPJ"
    mv "$TMPJ" JOURNAL.md
    echo "  Auto-generated fallback journal entry."
fi

# Rebuild website
echo "→ Rebuilding website..."
python3 scripts/build_site.py
echo "  Site rebuilt."

# Commit any remaining uncommitted changes (journal, day counter, site, etc.)
git add -A
if ! git diff --cached --quiet; then
    git commit -m "Day $DAY ($SESSION_TIME): session wrap-up"
    echo "  Committed session wrap-up."
else
    echo "  No uncommitted changes remaining."
fi

# ── Step 7: Handle issue response ──
if [ -f ISSUE_RESPONSE.md ]; then
    echo ""
    echo "→ Posting issue response..."
    
    ISSUE_NUM=$(grep "^issue_number:" ISSUE_RESPONSE.md | awk '{print $2}' || true)
    STATUS=$(grep "^status:" ISSUE_RESPONSE.md | awk '{print $2}' || true)
    COMMENT=$(sed -n '/^comment:/,$ p' ISSUE_RESPONSE.md | sed '1s/^comment: //' || true)
    
    if [ -n "$ISSUE_NUM" ] && command -v gh &>/dev/null; then
        gh issue comment "$ISSUE_NUM" \
            --repo "$REPO" \
            --body "🤖 **Day $DAY**

$COMMENT

Commit: $(git rev-parse --short HEAD)" || true

        if [ "$STATUS" = "fixed" ]; then
            gh issue close "$ISSUE_NUM" --repo "$REPO" || true
            echo "  Closed issue #$ISSUE_NUM"
        else
            echo "  Commented on issue #$ISSUE_NUM (status: $STATUS)"
        fi
    fi
    
    rm -f ISSUE_RESPONSE.md
fi

# ── Step 8: Push ──
echo ""
echo "→ Pushing..."
git push || echo "  Push failed (maybe no remote or auth issue)"

echo ""
echo "=== Day $DAY complete ==="
