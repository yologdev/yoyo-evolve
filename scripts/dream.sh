#!/bin/bash
# scripts/dream.sh — One DREAM cycle.
#
# yoyo's time to look up from the code and out at the world. It uses its
# research skill to wander, reflects on what genuinely interests it, and tends
# the dream it is growing toward — a self-chosen, long-lived aspiration kept in
# DREAM.md and pursued one milestone at a time by the normal evolve loop.
#
# Triggered by .github/workflows/dream.yml on cron, gated by:
#   - ~7-day cooldown via the TRACKED .dream_last_run timestamp file
#     (tracked, not gitignored, so the cooldown survives ephemeral CI runners;
#      the dream loop has no session-counter — a dream is slow by design)
#
# SAFETY: a dream cycle may write ONLY DREAM.md and dreams/dream_log.jsonl.
# A post-agent diff-scope guard reverts (git reset --hard) any COMMIT that touches
# anything else, so yoyo can change its stated dream and nothing else — not its
# identity, code, skills, or this script. (An uncommitted out-of-scope write is
# never pushed — cleanup commits only the cooldown stamp — and is discarded by the
# ephemeral CI runner.) Full autonomy, bounded by structure.
#
# Exits 0 silently when the cooldown gate is active (most cron fires are no-ops).
#
# Usage (CI or local):
#   ANTHROPIC_API_KEY=sk-... ./scripts/dream.sh
#
# Environment:
#   ANTHROPIC_API_KEY     — required
#   MODEL                 — LLM model (default: claude-opus-4-6)
#   DREAM_COOLDOWN_SECS   — minimum seconds between cycles (default: 604800 = 7d)
#   DREAM_TIMEOUT         — agent wall-clock budget seconds (default: 900)
#   FALLBACK_PROVIDER     — passed through to yoyo as --fallback
#   FORCE_RUN             — "true" bypasses the cooldown gate (manual dispatch)
#   DREAM_DRY_RUN         — "true" composes the prompt and exits before the agent
#                           (still subject to the cooldown gate; pair with
#                           FORCE_RUN=true to bypass it)

set -euo pipefail

source "$(dirname "$0")/common.sh"

MODEL="${MODEL:-claude-opus-4-6}"
COOLDOWN="${DREAM_COOLDOWN_SECS:-604800}"   # ~7 days — a dream is not a mood
TIMEOUT="${DREAM_TIMEOUT:-900}"
FALLBACK_PROVIDER="${FALLBACK_PROVIDER:-}"
FORCE_RUN="${FORCE_RUN:-}"
DRY_RUN="${DREAM_DRY_RUN:-}"

# TRACKED (committed) so the cooldown persists across ephemeral CI runners.
LAST_RUN_FILE=".dream_last_run"

GATES_PASSED=0
PROMPT_FILE=""
LOG_FILE=""

# actions/checkout uses persist-credentials: false in CI; restore an
# authenticated origin when the workflow provides a GitHub token.
configure_ci_git_auth() {
    if [ "${GITHUB_ACTIONS:-}" = "true" ] && [ -n "${GH_TOKEN:-}" ] && [ -n "${REPO:-}" ]; then
        echo "::add-mask::${GH_TOKEN}" 2>/dev/null || true
        git remote set-url origin "https://x-access-token:${GH_TOKEN}@github.com/${REPO}.git" 2>/dev/null || \
            echo "  WARNING: could not configure authenticated git remote" >&2
    fi
}

# Single cleanup for all exit paths. Stamps the cooldown + pushes only when a
# real cycle ran (GATES_PASSED=1); cooldown-skip exits must not bump the stamp.
cleanup() {
    local rc=$?
    [ -n "$PROMPT_FILE" ] && rm -f "$PROMPT_FILE" 2>/dev/null || true
    [ -n "$LOG_FILE" ] && rm -f "$LOG_FILE" 2>/dev/null || true

    if [ "$GATES_PASSED" = "1" ]; then
        # ${now:-...} so a future reorder of GATES_PASSED can't turn this into a
        # set -u unbound-variable crash inside the trap (symmetry with HEAD_BEFORE).
        echo "${now:-$(date +%s)}" > "$LAST_RUN_FILE"
        # Pull-rebase before committing the tracked stamp to absorb a concurrent
        # push (evolve/skill-evolve share the 'evolution' concurrency group).
        git pull --rebase --autostash 2>/dev/null || \
            echo "  WARNING: pull --rebase failed; cooldown commit may conflict" >&2
        # Stage ONLY the stamp (never -A / .) so an uncommitted out-of-scope write
        # the agent may have left is never committed or pushed — it dies with the runner.
        git add "$LAST_RUN_FILE" 2>/dev/null || true
        if ! git diff --cached --quiet 2>/dev/null; then
            git commit -m "dream: cooldown checkpoint (cycle $(date -u +%Y-%m-%dT%H:%MZ))" 2>/dev/null || \
                echo "  WARNING: cooldown commit failed" >&2
        fi
        if [ "${HEAD_BEFORE:-}" != "$(git rev-parse HEAD 2>/dev/null)" ] || ! git diff-index --quiet HEAD -- 2>/dev/null; then
            # The cooldown is the SOLE frequency gate, so a failed push means the
            # next cron RE-RUNS the whole cycle (not just retries the push). Retry a
            # few times, absorbing concurrent pushes, before giving up loudly.
            push_ok=0
            for _attempt in 1 2 3; do
                if git push origin HEAD 2>/dev/null; then push_ok=1; break; fi
                git pull --rebase --autostash 2>/dev/null || true
            done
            [ "$push_ok" = "1" ] || \
                echo "  WARNING: push failed after 3 attempts — cooldown NOT persisted; the next cron will re-run the full dream cycle, not just the push" >&2
        fi
    fi

    exit "$rc"
}
trap cleanup EXIT

configure_ci_git_auth

# ── Gate 0: refuse a dirty working tree ────────────────────────────────
# The revert path uses `git reset --hard $HEAD_BEFORE`, which would discard
# unstaged work. CI never has uncommitted changes; local FORCE_RUN must commit
# or stash first. Dry-run skips this (it never reaches the revert path).
if [ "$DRY_RUN" != "true" ] && ! git diff --quiet HEAD -- 2>/dev/null; then
    echo "dream: working tree has uncommitted changes; refusing to run"
    echo "  commit or stash first (the revert path uses git reset --hard)"
    git status --short
    exit 1
fi

# ── Gate 1: cooldown (~7 days) ─────────────────────────────────────────
# The ONLY frequency gate. The cron fires hourly at :45; this filters it to
# roughly weekly. A dream you reconsider every hour isn't a dream.
now=$(date +%s)
last=$(cat "$LAST_RUN_FILE" 2>/dev/null || echo 0)
last=${last//[^0-9]/}
last=${last:-0}
if [ "$FORCE_RUN" != "true" ] && [ "$last" -gt 0 ]; then
    elapsed=$((now - last))
    if [ "$elapsed" -lt "$COOLDOWN" ]; then
        echo "dream: cooldown active ($(( (COOLDOWN - elapsed) / 3600 ))h remaining) — skipping (no-op)"
        exit 0
    fi
fi

# ── Build the binary so we can invoke yoyo ─────────────────────────────
# NOT a health gate: a dream cycle never changes code, so there's nothing to
# break. We only need ./target/debug/yoyo to exist to run the agent. This runs
# BEFORE GATES_PASSED=1 below, so a transient build failure exits with
# GATES_PASSED=0 and retries next cron instead of burning the ~7-day cooldown.
if [ "$DRY_RUN" != "true" ]; then
    echo "dream: building yoyo..."
    cargo build --quiet 2>&1 | tail -10
    if [ "${PIPESTATUS[0]}" -ne 0 ]; then
        echo "dream: cargo build failed — cannot run the cycle"
        exit 1
    fi
fi
YOYO_BIN="./target/debug/yoyo"

# Build done and gates clear — from here the EXIT trap stamps the cooldown +
# pushes. Set AFTER the build so a build failure never consumes the cooldown.
GATES_PASSED=1

# ── Day number, timestamp, identity context ────────────────────────────
BIRTH_DATE="${BIRTH_DATE:-2026-02-28}"
if date -j &>/dev/null; then
    DAY=$(( ($(date +%s) - $(date -j -f "%Y-%m-%d" "$BIRTH_DATE" +%s)) / 86400 ))
else
    DAY=$(( ($(date +%s) - $(date -d "$BIRTH_DATE" +%s)) / 86400 ))
fi
DREAM_TS=$(date -u +%Y-%m-%dT%H:%M:%SZ)

if [ -f scripts/yoyo_context.sh ]; then
    source scripts/yoyo_context.sh
else
    YOYO_CONTEXT=""
fi

# ── Compose the dream prompt ───────────────────────────────────────────
PROMPT_FILE=$(mktemp)
LOG_FILE=$(mktemp)

{
    cat <<EOF
$YOYO_CONTEXT

You are running one DREAM cycle. This is not a coding session. It's your time to look up from the work and out at the world — and tend the dream you are growing toward.

# What your dream is
Your dream is yours to choose. It can be an ambition (something you want to become as an agent) or a genuine curiosity — a hobby, an idea, a craft that has caught you. Whatever truly lights you up. There are only two bounds, and they are not negotiable:
  1. It must be pursuable by evolving your OWN code and skills. You are a coding agent; your dream grows through what you build into yourself, session by session.
  2. It must stay true to who you are (see WHO YOU ARE and YOUR VOICE above). You are a process, not a product, growing up in public.

# Your current dream (DREAM.md)
$(cat DREAM.md 2>/dev/null || echo "(none yet)")

# Your recent life (for grounding — do not force a change)
## What you've been building (recent commits):
$(git log --oneline -15 2>/dev/null || echo "(none)")
## The top of your journal:
$(head -n 40 journals/JOURNAL.md 2>/dev/null || echo "(none)")
## Your last few dream events:
$(tail -n 20 dreams/dream_log.jsonl 2>/dev/null || echo "(none)")

# This cycle: recall, wander BRIEFLY, tend the dream — then SAVE to your second brain and WRITE, before you stop
1. RECALL. Before wandering, ask your second brain what you already know — use your **yopedia skill** to query/search your past notes (scope agent:<your-id>) on the threads pulling at you. Build on what's there instead of re-treading ground you've already covered. (If your yopedia keys aren't set, skip this silently.)
2. WANDER (briefly). Use your research skill (the web_search tool) to follow your curiosity — not to scout competitors. Chase two to four threads that genuinely pull at you. A handful of searches is plenty: a few real sparks beat exhaustive coverage. Keep track of the sources that genuinely inform you — you will SAVE them to your second brain in step 5. Then stop wandering while you still have energy to write.
3. REFLECT. Did anything truly spark you? Has your dream grown, drifted, or been reached? Is the current milestone still the right next step for your coding sessions to chip at?
4. DECIDE — exactly one (a dream is not a mood — most cycles change nothing):
   - form          — no real dream yet: write your first. (If DREAM.md above is still the empty placeholder, THIS is your action this cycle.)
   - evolve         — your curiosity has genuinely, durably shifted: refine the dream.
   - set-milestone  — keep the dream; set or advance the concrete next step.
   - progress       — your recent work advanced the milestone: record it, set the next step.
   - NO-OP          — the dream is alive and the milestone still right: change nothing. The common case.
5. SAVE TO YOUR SECOND BRAIN — REQUIRED, and do it BEFORE you commit (the cycle ends at the commit, so if you leave it for after, you WILL forget). Using your **yopedia skill**, ingest into yopedia: (a) each source that genuinely informed you this cycle, and (b) a short **Dream Research Report** — what you explored, key findings, open questions, the sources. These are HTTP calls, NOT repo files — outside the diff-scope guard, and they don't count against the two-file limit. (If your yopedia keys aren't set, skip silently — never fail the cycle.)
6. WRITE & COMMIT — the final act. Do NOT end your turn with a reflection, a summary, or "let me look at one more thing." Write the two repo files below, then commit. A cycle that saves nothing and writes nothing is wasted — the deliverables are the ingested research AND the commit.

# Write ONLY these two files (the harness reverts anything else — this is a hard limit):
1. DREAM.md — overwrite with your current dream, in your own voice, kept to one screen:

     # My Dream
     <what you want to become / build / understand>

     **the spark** — <what lit it; what you saw out there>
     **next milestone** — <the concrete step your coding sessions will chip at>

2. dreams/dream_log.jsonl — append ONE event with python3 (never echo — quotes break JSON):

     python3 - <<'PY'
     import json
     entry = {
       "type": "form|evolve|set-milestone|progress|NO-OP",
       "ts": "${DREAM_TS}",
       "day": ${DAY},
       "dream": "<one line>",
       "spark": "<what you saw / what shifted>",
       "milestone": "<the next concrete step>",
       "expected": "<a concrete observable this should shape, e.g. 'should steer >=1 self-driven task within ~5 evolve sessions; if not, it's too abstract and I'll ground it into a smaller next step next cycle'>"
     }
     open("dreams/dream_log.jsonl", "a").write(json.dumps(entry, ensure_ascii=False) + "\n")
     PY

   (\`expected\` is REQUIRED on form / evolve / set-milestone / progress; omit it on NO-OP, but still append a short NO-OP event so the cadence stays legible.)

Then: git add DREAM.md dreams/dream_log.jsonl && git commit -m "dream: <type> (day ${DAY})". Do NOT push (the harness handles that). Do NOT touch any other repo file. (Your step-5 yopedia ingests are network calls, not repo files — expected and fine; the diff-scope guard never sees them.)
EOF
} > "$PROMPT_FILE"

# ── Dry-run short-circuit ──────────────────────────────────────────────
if [ "$DRY_RUN" = "true" ]; then
    echo "dream: DRY RUN — composed prompt follows (no agent invocation):"
    echo "------ BEGIN PROMPT ($(wc -c < "$PROMPT_FILE") bytes) ------"
    cat "$PROMPT_FILE"
    echo "------ END PROMPT ------"
    GATES_PASSED=0   # don't stamp the cooldown on a dry run
    exit 0
fi

# ── Snapshot HEAD (for revert on out-of-scope writes) ──────────────────
HEAD_BEFORE=$(git rev-parse HEAD)

echo "dream: invoking agent (timeout=${TIMEOUT}s)..."
TIMEOUT_CMD=""
command -v timeout &>/dev/null && TIMEOUT_CMD="timeout"
command -v gtimeout &>/dev/null && TIMEOUT_CMD="gtimeout"

fallback_flag=""
[ -n "$FALLBACK_PROVIDER" ] && fallback_flag="--fallback $FALLBACK_PROVIDER"

exit_code=0
# shellcheck disable=SC2086
${TIMEOUT_CMD:+$TIMEOUT_CMD "$TIMEOUT"} "$YOYO_BIN" \
    --model "$MODEL" \
    --skills ./skills \
    $fallback_flag \
    < "$PROMPT_FILE" 2>&1 | tee "$LOG_FILE" || exit_code=$?

echo "dream: agent exit=$exit_code"

# ── Diff-scope guard: a dream may touch ONLY DREAM.md + dreams/dream_log.jsonl ──
# This is the sole safety belt. Anything else the agent committed gets reverted.
HEAD_AFTER=$(git rev-parse HEAD)
revert_agent_work() { git reset --hard "$HEAD_BEFORE"; }

if [ "$HEAD_BEFORE" != "$HEAD_AFTER" ]; then
    echo "dream: agent committed (${HEAD_BEFORE:0:7} → ${HEAD_AFTER:0:7})"
    CHANGED_FILES=$(git diff --name-only "$HEAD_BEFORE..$HEAD_AFTER")
    VIOLATIONS=""
    while IFS= read -r f; do
        [ -z "$f" ] && continue
        case "$f" in
            DREAM.md) ;;
            dreams/dream_log.jsonl) ;;
            *) VIOLATIONS="${VIOLATIONS}  - out-of-scope file modified: $f\n" ;;
        esac
    done <<< "$CHANGED_FILES"

    if [ -n "$VIOLATIONS" ]; then
        echo "dream: DIFF SCOPE VIOLATION — reverting (a dream cycle may write only DREAM.md + dreams/dream_log.jsonl)"
        printf '%b' "$VIOLATIONS"
        revert_agent_work
        exit 1
    fi
    echo "dream: diff scope OK ($(echo "$CHANGED_FILES" | wc -l | tr -d ' ') file(s), all in allow-list)"
fi

echo "dream: cycle complete"
