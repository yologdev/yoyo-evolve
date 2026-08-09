#!/bin/bash
# scripts/skill_evolve.sh — One skill-evolution cycle.
# Triggered by .github/workflows/skill-evolve.yml on cron, gated by:
#   - .skill_evolve_counter ≥ SKILL_EVOLVE_THRESHOLD (default 5 sessions)
#   - 24h cooldown via .skill_evolve_last_run timestamp file
#   - cargo build && cargo test pass on current main
#
# Exits 0 silently when gates fail (this is normal — most cron fires are no-ops).
# Auto-commits and pushes any change the meta-skill produced; reverts on build break.
#
# Usage (CI or local):
#   ANTHROPIC_API_KEY=sk-... ./scripts/skill_evolve.sh
#
# Environment:
#   ANTHROPIC_API_KEY            — required
#   MODEL                        — LLM model (default: claude-opus-4-6)
#   SKILL_EVOLVE_THRESHOLD       — sessions required before a cycle runs (default: 5)
#   SKILL_EVOLVE_COOLDOWN_SECS   — minimum seconds between cycles (default: 86400)
#   SKILL_EVOLVE_TIMEOUT         — agent wall-clock budget seconds (default: 1500)
#   FALLBACK_PROVIDER            — passed through to yoyo as --fallback
#   FORCE_RUN                    — "true" bypasses both counter and cooldown gates
#   SKILL_EVOLVE_DRY_RUN         — "true" composes the prompt and exits before
#                                  invoking the agent. Useful for verifying gate
#                                  logic and prompt content without spending tokens.

set -euo pipefail

source "$(dirname "$0")/common.sh"

# GASP state instrumentation (fail-soft; see scripts/gasp_shim.sh). Skill
# cycles serve their own standing goal; skill patches pin the skill commit —
# the spec's canonical self-improvement case.
export GASP_GOAL_ID="goal_skill_quality"
export GASP_GOAL_TITLE="Keep yoyo's skills sharp: refine, create, retire"
export GASP_GOAL_SUMMARY="the standing goal skill-evolve cycles serve; each skill change is a patch pinning the skill commit"
# the eval fact must name THIS harness's actual oracle (no clippy, no
# evaluator agent here) — the record never claims a stronger gate than ran
export GASP_EVAL_COMMAND="diff-scope hard rules; cargo build+test"
if [ -r "$(dirname "$0")/gasp_shim.sh" ] && . "$(dirname "$0")/gasp_shim.sh"; then
    :
else
    echo "  [gasp] shim missing or failed to load — GASP instrumentation disabled" >&2
    gasp_session_start() { :; }; gasp_task_planned() { :; }
    gasp_task_result()  { :; }; gasp_mirror_skills() { :; }; gasp_session_end() { :; }
fi
GASP_OUTCOME=""

MODEL="${MODEL:-claude-opus-4-6}"
THRESHOLD="${SKILL_EVOLVE_THRESHOLD:-5}"
COOLDOWN="${SKILL_EVOLVE_COOLDOWN_SECS:-86400}"
TIMEOUT="${SKILL_EVOLVE_TIMEOUT:-1500}"
FALLBACK_PROVIDER="${FALLBACK_PROVIDER:-}"
FORCE_RUN="${FORCE_RUN:-}"
DRY_RUN="${SKILL_EVOLVE_DRY_RUN:-}"

COUNTER_FILE=".skill_evolve_counter"
LAST_RUN_FILE=".skill_evolve_last_run"

# Cleanup state. GATES_PASSED stays 0 until every gate clears; gate-skip
# exits must not reset the counter/cooldown.
GATES_PASSED=0
AUDIT_WT=""
PROMPT_FILE=""
LOG_FILE=""

# actions/checkout uses persist-credentials: false in CI, so restore an
# authenticated origin explicitly when the workflow provides a GitHub token.
configure_ci_git_auth() {
    if [ "${GITHUB_ACTIONS:-}" = "true" ] && [ -n "${GH_TOKEN:-}" ] && [ -n "${REPO:-}" ]; then
        echo "::add-mask::${GH_TOKEN}" 2>/dev/null || true
        git remote set-url origin "https://x-access-token:${GH_TOKEN}@github.com/${REPO}.git" 2>/dev/null || \
            echo "  WARNING: could not configure authenticated git remote" >&2
    fi
}

# Single cleanup function for all exit paths (success, gate skip, revert, kill).
# Order matters: worktree first (so .git/worktrees/ is cleaned), then dir.
# Reversing this leaves a stale worktree registration that breaks the next
# cycle with "worktree already exists".
cleanup() {
    local rc=$?
    [ -n "$AUDIT_WT" ] && git worktree remove --force "$AUDIT_WT" 2>/dev/null || true
    [ -n "$AUDIT_WT" ] && rm -rf "$AUDIT_WT" 2>/dev/null || true
    git worktree prune 2>/dev/null || true
    [ -n "$PROMPT_FILE" ] && rm -f "$PROMPT_FILE" 2>/dev/null || true
    [ -n "$LOG_FILE" ] && rm -f "$LOG_FILE" 2>/dev/null || true

    # Gate state reset: only when a real cycle ran. NO-OP gate-skip exits do
    # not bump the cooldown timestamp (otherwise gate skips would gate themselves).
    if [ "$GATES_PASSED" = "1" ]; then
        # Reset counter on every completed cycle, including NO-OP and refused —
        # cooldown gates frequency, not outcome. The counter file is tracked;
        # the timestamp file is gitignored.
        echo 0 > "$COUNTER_FILE"
        echo "$now" > "$LAST_RUN_FILE"

        # Race protection (C2): evolve.sh and skill_evolve.sh both touch the
        # counter on different cron offsets. Pull-rebase before committing so
        # a concurrent bump from evolve.sh doesn't get swallowed by a
        # non-fast-forward rejection on push.
        git pull --rebase --autostash 2>/dev/null || \
            echo "  WARNING: pull --rebase failed; counter commit may conflict" >&2

        git add "$COUNTER_FILE" 2>/dev/null || true
        if ! git diff --cached --quiet 2>/dev/null; then
            git commit -m "skill-evolve: reset counter (cycle $(date -u +%Y-%m-%dT%H:%MZ))" 2>/dev/null || \
                echo "  WARNING: counter commit failed" >&2
        fi

        if [ "${HEAD_BEFORE:-}" != "$(git rev-parse HEAD 2>/dev/null)" ] || ! git diff-index --quiet HEAD -- 2>/dev/null; then
            git push origin HEAD 2>/dev/null || \
                echo "  WARNING: push failed (next cron will retry)" >&2
        fi

        # GASP: close the run and push state AFTER the code push (code first,
        # state second). Fail-soft; no-ops when the session never opened.
        # If the pre-push rebase rewrote the agent's commit, the recorded
        # patch artifact pins a SHA that never becomes public — say so.
        if [ -n "${HEAD_AFTER:-}" ] && [ "${HEAD_BEFORE:-}" != "${HEAD_AFTER:-}" ] \
           && ! git merge-base --is-ancestor "$HEAD_AFTER" HEAD 2>/dev/null; then
            echo "  WARNING: rebase rewrote ${HEAD_AFTER:0:7}; GASP patch artifact pins a non-pushed SHA" >&2
            GASP_OUTCOME="${GASP_OUTCOME:-done} (artifact sha rewritten by rebase; now $(git rev-parse --short HEAD 2>/dev/null || echo unknown))"
        fi
        gasp_session_end "${GASP_OUTCOME:-aborted rc=$rc}"
    fi

    exit "$rc"
}
trap cleanup EXIT

configure_ci_git_auth

# ── Gate 0: refuse to run with a dirty working tree ────────────────────
# The revert path below uses `git reset --hard $HEAD_BEFORE` which would
# discard unstaged work. CI never has uncommitted changes; for local
# FORCE_RUN, the operator must commit/stash first.
# Dry-run skips this gate because it never invokes the revert path.
if [ "$DRY_RUN" != "true" ] && ! git diff --quiet HEAD -- 2>/dev/null; then
    echo "skill-evolve: working tree has uncommitted changes; refusing to run"
    echo "  commit or stash first (the revert path uses git reset --hard)"
    git status --short
    exit 1
fi

# ── Gate 1: session counter ────────────────────────────────────────────
counter=$(cat "$COUNTER_FILE" 2>/dev/null || echo 0)
counter=${counter//[^0-9]/}
counter=${counter:-0}

if [ "$FORCE_RUN" != "true" ] && [ "$counter" -lt "$THRESHOLD" ]; then
    echo "skill-evolve: counter=$counter < $THRESHOLD — skipping (no-op)"
    exit 0
fi

# ── Gate 2: 24h cooldown ───────────────────────────────────────────────
now=$(date +%s)
last=$(cat "$LAST_RUN_FILE" 2>/dev/null || echo 0)
last=${last//[^0-9]/}
last=${last:-0}

if [ "$FORCE_RUN" != "true" ] && [ "$last" -gt 0 ]; then
    elapsed=$((now - last))
    if [ "$elapsed" -lt "$COOLDOWN" ]; then
        remaining=$((COOLDOWN - elapsed))
        echo "skill-evolve: cooldown active (${remaining}s remaining) — skipping"
        exit 0
    fi
fi

# ── Gate 3: build is green ─────────────────────────────────────────────
# Use debug build to share cache with evolve.sh (which also uses debug).
# Capture exit explicitly via PIPESTATUS instead of relying on `set -o pipefail`,
# so a future edit that drops pipefail doesn't silently turn build gates into no-ops.
# Dry-run skips this gate (no agent invocation → no need to gate the codebase).
if [ "$DRY_RUN" != "true" ]; then
    echo "skill-evolve: verifying build/test on current HEAD..."
    cargo build --quiet 2>&1 | tail -10
    if [ "${PIPESTATUS[0]}" -ne 0 ]; then
        echo "skill-evolve: cargo build failed before cycle — refusing to run"
        exit 1
    fi
    cargo test --quiet 2>&1 | tail -10
    if [ "${PIPESTATUS[0]}" -ne 0 ]; then
        echo "skill-evolve: cargo test failed before cycle — refusing to run"
        exit 1
    fi

    YOYO_BIN="./target/debug/yoyo"
    [ -x "$YOYO_BIN" ] || { echo "skill-evolve: $YOYO_BIN missing"; exit 1; }
else
    YOYO_BIN="./target/debug/yoyo"  # set anyway for downstream env consistency
fi

# All gates passed — from here on, the EXIT trap will reset counter + cooldown.
GATES_PASSED=1

# ── Identity context ───────────────────────────────────────────────────
if [ -f scripts/yoyo_context.sh ]; then
    source scripts/yoyo_context.sh
else
    YOYO_CONTEXT=""
fi

# ── Fetch audit-log worktree (evidence; treat as read-only by convention) ──
# Nothing in this cycle should write into $AUDIT_WT — it's the meta-skill's
# evidence corpus. Writes belong on `audit-log` branch via the session-end
# push in evolve.sh (Step 7c2), not from skill-evolve.
AUDIT_WT="/tmp/skill-evolve-audit-$$"

if git fetch --depth 100 origin audit-log:audit-log 2>/dev/null; then
    if git worktree add "$AUDIT_WT" audit-log 2>/dev/null; then
        export YOYO_AUDIT_DIR="$AUDIT_WT/sessions"
        echo "skill-evolve: audit evidence at $YOYO_AUDIT_DIR ($(ls "$YOYO_AUDIT_DIR" 2>/dev/null | wc -l) sessions)"
    fi
fi

# ── Compose prompt ─────────────────────────────────────────────────────
PROMPT_FILE=$(mktemp)
LOG_FILE=$(mktemp)

{
    cat <<EOF
$YOYO_CONTEXT

You are running one skill-evolve cycle. Read skills/skill-evolve/SKILL.md for the full procedure — that skill is your spec.

# Recent evidence

## Last 200 lines of skills/_journal.md (skill-evolution events):
$(tail -n 200 skills/_journal.md 2>/dev/null || echo "(empty)")

## Last 50 entries of memory/learnings.jsonl (self-reflection):
$(tail -n 50 memory/learnings.jsonl 2>/dev/null || echo "(empty)")

## Top of journals/JOURNAL.md (most recent sessions):
$(head -n 200 journals/JOURNAL.md 2>/dev/null || echo "(empty)")

## Recent GH Action runs:
$(gh run list --json url,conclusion,createdAt,name -L 10 2>/dev/null || echo "[]")

## Audit evidence pointer:
\$YOYO_AUDIT_DIR = ${YOYO_AUDIT_DIR:-(unavailable — no audit-log branch yet)}
Run \`ls "\$YOYO_AUDIT_DIR" | tail -30\` and read individual session files there for fine-grained tool-call evidence.

# Your task

Run exactly one skill-evolve cycle per skills/skill-evolve/SKILL.md. Honor all three hard rules. Produce exactly one of: refine | create | retire | meta-suggestion | refused | NO-OP.

Append the resulting event to skills/_journal.md, commit any changes (do not push — the harness handles that), and stop.
EOF
} > "$PROMPT_FILE"

# ── Dry-run short-circuit ──────────────────────────────────────────────
# Print the composed prompt and exit before invoking the agent. Useful for:
# verifying gate logic, inspecting evidence-stitching, debugging prompt size.
if [ "$DRY_RUN" = "true" ]; then
    echo "skill-evolve: DRY RUN — composed prompt follows (no agent invocation):"
    echo "------ BEGIN PROMPT ($(wc -c < "$PROMPT_FILE") bytes) ------"
    cat "$PROMPT_FILE"
    echo "------ END PROMPT ------"
    # Don't reset gate state on dry-run — operator may want to keep testing
    # without consuming the gate.
    GATES_PASSED=0
    exit 0
fi

# ── Snapshot HEAD (for revert on build break) ──────────────────────────
HEAD_BEFORE=$(git rev-parse HEAD)

# Skill cycles intentionally emit no task node — the chain is
# run -> patch -> eval -> decision under goal_skill_quality (the spec's
# spine is "a backbone, not a forced linear workflow").
GASP_DAY=$(cat DAY_COUNT 2>/dev/null || echo 0); GASP_DAY=${GASP_DAY//[^0-9]/}
gasp_session_start "${GASP_DAY:-0}" "skill_day" \
    "skill-evolve cycle (refine|create|retire one skill)"

# ── Invoke yoyo ────────────────────────────────────────────────────────
echo "skill-evolve: invoking agent (timeout=${TIMEOUT}s)..."

TIMEOUT_CMD=""
command -v timeout &>/dev/null && TIMEOUT_CMD="timeout"
command -v gtimeout &>/dev/null && TIMEOUT_CMD="gtimeout"

fallback_flag=""
# Explicit default at the use site. NOTE: line ~52 already defaults this var,
# so the bare form was never an abort risk — the commit that added this guard
# (2b7e832c) claimed otherwise and was wrong (dream.sh still uses the bare form
# and does not abort). Kept because a use-site default is cheap and correct.
[ -n "${FALLBACK_PROVIDER:-}" ] && fallback_flag="--fallback ${FALLBACK_PROVIDER}"

exit_code=0
# shellcheck disable=SC2086
${TIMEOUT_CMD:+$TIMEOUT_CMD "$TIMEOUT"} "$YOYO_BIN" \
    --model "$MODEL" \
    --skills ./skills \
    $fallback_flag \
    < "$PROMPT_FILE" 2>&1 | tee "$LOG_FILE" || exit_code=$?

echo "skill-evolve: agent exit=$exit_code"

# ── Verify diff scope, then build, then revert if anything is wrong ────
HEAD_AFTER=$(git rev-parse HEAD)

# Helper: revert anything the agent did. Safe because Gate 0 verified the
# pre-agent working tree was clean; only the agent's commits get dropped.
revert_agent_work() {
    git reset --hard "$HEAD_BEFORE"
    git clean -fd skills/skill-evolve-* 2>/dev/null || true
}

if [ "$HEAD_BEFORE" != "$HEAD_AFTER" ]; then
    echo "skill-evolve: agent committed (${HEAD_BEFORE:0:7} → ${HEAD_AFTER:0:7})"

    # ── Diff-scope guard: enforce HARD RULES from skills/skill-evolve/SKILL.md ──
    # The meta-skill's three hard rules are LLM-compliance only; this is the
    # harness-side belt that turns them into actual constraints.
    CHANGED_FILES=$(git diff --name-only "$HEAD_BEFORE..$HEAD_AFTER")
    VIOLATIONS=""

    while IFS= read -r f; do
        [ -z "$f" ] && continue
        case "$f" in
            # Whole-tree allow-list: the only paths skill-evolve may legitimately touch.
            skills/_journal.md) ;;
            memory/learnings.jsonl) ;;
            skills_attic/*) ;;  # retirement: git mv into attic
            skills/*/SKILL.md)
                # Per-file check: must be a yoyo-origin skill, not core, not skill-evolve itself.
                skill_name=$(echo "$f" | awk -F/ '{print $2}')
                if [ "$skill_name" = "skill-evolve" ]; then
                    VIOLATIONS="${VIOLATIONS}  - HARD RULE #2 violation: skill-evolve modified itself ($f)\n"
                    continue
                fi
                # Use the post-agent file content for the origin check (the agent may have just created it).
                if grep -q "^core: true" "$f" 2>/dev/null; then
                    VIOLATIONS="${VIOLATIONS}  - HARD RULE #1 violation: $f carries core: true\n"
                    continue
                fi
                if ! grep -q "^origin: yoyo$" "$f" 2>/dev/null; then
                    VIOLATIONS="${VIOLATIONS}  - HARD RULE #1 violation: $f lacks 'origin: yoyo' (not eligible)\n"
                    continue
                fi
                ;;
            *)
                # Anything outside the allow-list is a violation, no exceptions.
                VIOLATIONS="${VIOLATIONS}  - out-of-scope file modified: $f\n"
                ;;
        esac
    done <<< "$CHANGED_FILES"

    if [ -n "$VIOLATIONS" ]; then
        echo "skill-evolve: DIFF SCOPE VIOLATION — reverting agent commits"
        printf '%b' "$VIOLATIONS"
        gasp_task_result 1 "skill cycle: $(echo "$CHANGED_FILES" | paste -sd ', ' - | cut -c1-160)" \
            rejected "$HEAD_BEFORE" "$HEAD_AFTER" "diff-scope violation: $(printf '%b' "$VIOLATIONS" | paste -sd '; ' -)"
        GASP_OUTCOME="rejected: diff-scope violation"
        revert_agent_work
        exit 1
    fi
    echo "skill-evolve: diff scope OK ($(echo "$CHANGED_FILES" | wc -l | tr -d ' ') files changed, all in allow-list)"

    # ── Build/test verify. PIPESTATUS makes this independent of `set -o pipefail`. ──
    cargo build --quiet 2>&1 | tail -10
    if [ "${PIPESTATUS[0]}" -ne 0 ]; then
        echo "skill-evolve: build broken after agent commit — reverting"
        gasp_task_result 1 "skill cycle: $(echo "$CHANGED_FILES" | paste -sd ', ' - | cut -c1-160)" \
            rejected "$HEAD_BEFORE" "$HEAD_AFTER" "build broken after skill change"
        GASP_OUTCOME="rejected: build broken"
        revert_agent_work
        exit 1
    fi
    cargo test --quiet 2>&1 | tail -10
    if [ "${PIPESTATUS[0]}" -ne 0 ]; then
        echo "skill-evolve: tests broken after agent commit — reverting"
        gasp_task_result 1 "skill cycle: $(echo "$CHANGED_FILES" | paste -sd ', ' - | cut -c1-160)" \
            rejected "$HEAD_BEFORE" "$HEAD_AFTER" "tests broken after skill change"
        GASP_OUTCOME="rejected: tests broken"
        revert_agent_work
        exit 1
    fi
    echo "skill-evolve: build/test still green"

    # GASP: a journal-only commit (refused / NO-OP / meta-suggestion — the
    # meta-skill journals every cycle) is NOT a skill change: no patch is
    # recorded. A real change records the promoted patch pinning the skill
    # commit; the state repo's skill tree is synced either way.
    NON_JOURNAL=$(echo "$CHANGED_FILES" | grep -v -e '^skills/_journal\.md$' -e '^memory/learnings\.jsonl$' || true)
    if [ -n "$NON_JOURNAL" ]; then
        gasp_task_result 1 "skill cycle: $(echo "$NON_JOURNAL" | paste -sd ', ' - | cut -c1-160)" \
            promoted "$HEAD_BEFORE" "$HEAD_AFTER"
        GASP_OUTCOME="promoted skill change"
    else
        GASP_OUTCOME="journal-only cycle (refused, no-op, or meta-suggestion — no skill change)"
    fi
    gasp_mirror_skills
fi
if [ -z "$GASP_OUTCOME" ] && [ "${exit_code:-0}" -ne 0 ]; then
    # agent timeout / API error with no commit is an abort, not a verdict
    GASP_OUTCOME="aborted: agent exit=$exit_code (no commit)"
fi
[ -z "$GASP_OUTCOME" ] && GASP_OUTCOME="no-op (no skill change this cycle)"

# Cycle complete. Gate state reset, push, and temp cleanup all happen in the
# EXIT trap (cleanup() near the top). This ensures revert paths reach them too.
echo "skill-evolve: cycle complete"
