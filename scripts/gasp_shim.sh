#!/bin/bash
# scripts/gasp_shim.sh — GASP instrumentation for evolve.sh (sourced, never run).
#
# Emits yoyo's session transitions as GASP events (github.com/yologdev/gasp)
# into the yoyo-gasp state repo (github.com/yologdev/yoyo-gasp), via the
# gasp-emit sidecar (tools/gasp-emit, built on yoagent-state 0.4).
#
# DESIGN RULE: strictly fail-soft. Every entry point returns 0; any failure
# disables emission for the rest of the session with a warning naming the
# root cause. An evolve session must never break because state
# instrumentation did. Repeated all-session failures escalate loudly after 3
# in a row (same pattern as the audit-log push counter in evolve.sh).
#
# Environment:
#   GASP_DISABLE=1        — skip all instrumentation
#   GASP_STATE_REPO       — state repo URL   (default: yologdev/yoyo-gasp)
#   GASP_STATE_DIR        — local clone path (default: /tmp/yoyo-gasp-state.$$)
#   GH_PAT                — used for the authenticated clone/push when set
#
# Session ordering (GASP integration contract): events append locally during
# the session (durable, uncommitted); gasp_session_end runs AFTER evolve.sh's
# Step 8 code push, so the state repo's boundary commit — which references
# code SHAs as patch artifacts — is pushed only after those SHAs are public.
# Known limitation: a session that dies before gasp_session_end never pushes
# its events; the partial clone is preserved in /tmp (path in the warning)
# but ephemeral runners discard it. GASP here is a completed-session ledger.

GASP_ENABLED=false
GASP_STATE_REPO="${GASP_STATE_REPO:-github.com/yologdev/yoyo-gasp}"
GASP_STATE_DIR="${GASP_STATE_DIR:-/tmp/yoyo-gasp-state.$$}"
GASP_EMIT_BIN="target/gasp-emit/debug/gasp-emit"
GASP_PUSH_URL=""
GASP_RUN_ID=""
GASP_FAIL_COUNTER=".yoyo/gasp_failures"
# The standing goal this session's tasks/patches serve. Callers may override
# before gasp_session_start (skill_evolve.sh sets goal_skill_quality).
GASP_GOAL_ID="${GASP_GOAL_ID:-goal_self_improvement}"
GASP_GOAL_TITLE="${GASP_GOAL_TITLE:-}"
GASP_GOAL_SUMMARY="${GASP_GOAL_SUMMARY:-}"
# Extra repo-relative paths for the boundary commit (set by gasp_mirror_skills)
GASP_EXTRA_PATHS=""

# Remove credentials from any text we are about to print.
_gasp_scrub() {
    local text="$1"
    [ -n "${GH_PAT:-}" ] && text=${text//"$GH_PAT"/***}
    printf '%s' "$text"
}

# Consecutive-failure escalation (mirrors evolve.sh's audit_push_failures).
_gasp_note_failure() {
    local n
    n=$(cat "$GASP_FAIL_COUNTER" 2>/dev/null || echo 0)
    n=${n//[^0-9]/}
    n=$(( ${n:-0} + 1 ))
    echo "$n" > "$GASP_FAIL_COUNTER" 2>/dev/null || true
    if [ "$n" -ge 3 ]; then
        echo "  [gasp] ⚠⚠⚠ GASP has failed $n recorded sessions — the state stream may be dead" >&2
        echo "  [gasp]     check: GH_PAT validity/push access to ${GASP_STATE_REPO}, gasp-emit build" >&2
        echo "  [gasp]     reset with: echo 0 > $GASP_FAIL_COUNTER (gitignored — counts persist only on non-ephemeral runners)" >&2
    fi
    return 0
}

_gasp_off() {
    echo "  [gasp] $(_gasp_scrub "$1") — GASP emission disabled for this session" >&2
    [ -d "$GASP_STATE_DIR" ] && echo "  [gasp] partial state left at ${GASP_STATE_DIR}" >&2
    _gasp_note_failure
    GASP_ENABLED=false
    return 0
}

_gasp_emit() {
    [ "$GASP_ENABLED" = true ] || return 0
    local out
    if ! out=$("$GASP_EMIT_BIN" "$@" 2>&1); then
        _gasp_off "gasp-emit $1 failed: $(printf '%s' "$out" | tail -n 3 | tr '\n' '; ')"
    fi
    return 0
}

# Call once, after the baseline build passes. Builds the emitter, clones the
# state repo, verifies push access, opens the run.
# gasp_session_start <day> [kind] [task-desc]
# kind defaults to "day" (run ids run_day124_...); skill_evolve passes
# "skill_day" (run_skill_day124_...) plus its own task description.
gasp_session_start() {
    [ "${GASP_DISABLE:-}" = "1" ] && return 0
    local day="${1:-0}" kind="${2:-day}" task="${3:-evolve session day ${1:-0}}" out

    # 1. build the emitter (own manifest; target/ is cached+ignored already)
    if ! out=$(cargo build --quiet --manifest-path tools/gasp-emit/Cargo.toml \
        --target-dir target/gasp-emit 2>&1); then
        _gasp_off "gasp-emit build failed: $(printf '%s' "$out" | tail -n 3 | tr '\n' '; ')"
        return 0
    fi

    # 2. clone the state repo (authenticated when GH_PAT is available).
    # GASP_STATE_REPO may be owner/name (default), a full URL, or a local path.
    local clean_url
    case "$GASP_STATE_REPO" in
        *://*|/*)
            GASP_PUSH_URL="$GASP_STATE_REPO"; clean_url="$GASP_STATE_REPO" ;;
        *)
            clean_url="https://${GASP_STATE_REPO}.git"
            GASP_PUSH_URL="$clean_url"
            [ -n "${GH_PAT:-}" ] && GASP_PUSH_URL="https://x-access-token:${GH_PAT}@${GASP_STATE_REPO}.git" ;;
    esac
    rm -rf "$GASP_STATE_DIR" 2>/dev/null || true
    if ! out=$(git clone --quiet "$GASP_PUSH_URL" "$GASP_STATE_DIR" 2>&1); then
        _gasp_off "cannot clone ${GASP_STATE_REPO}: $(printf '%s' "$out" | tail -n 2 | tr '\n' '; ')"
        return 0
    fi
    # keep the token out of .git/config; pushes use $GASP_PUSH_URL explicitly
    git -C "$GASP_STATE_DIR" remote set-url origin "$clean_url" 2>/dev/null || true
    git -C "$GASP_STATE_DIR" config user.name "yoyo[gasp]" 2>/dev/null || true
    git -C "$GASP_STATE_DIR" config user.email "yoyo-gasp@users.noreply.github.com" 2>/dev/null || true

    # 3. fail fast if we cannot push — otherwise the whole session's state
    # would be silently lost at session end (clone succeeds on public repos
    # regardless of write access)
    if ! out=$(git -C "$GASP_STATE_DIR" push --dry-run --quiet "$GASP_PUSH_URL" HEAD:main 2>&1); then
        _gasp_off "no push access to ${GASP_STATE_REPO} (state would be lost at session end): $(printf '%s' "$out" | tail -n 2 | tr '\n' '; ')"
        return 0
    fi

    GASP_RUN_ID="run_${kind}${day}_$(date -u +%Y%m%dT%H%M%SZ)"
    GASP_ENABLED=true
    # flags are passed unconditionally (empty = use default) — conditional
    # ${var:+...} argv splicing is a bash-only subtlety worth avoiding
    _gasp_emit session-start --state-dir "$GASP_STATE_DIR" --run-id "$GASP_RUN_ID" \
        --worker "evolve-shim-$$" --day "$day" --goal "$GASP_GOAL_ID" \
        --goal-title "$GASP_GOAL_TITLE" --goal-summary "$GASP_GOAL_SUMMARY" \
        --task "$task"
    [ "$GASP_ENABLED" = true ] && echo "  [gasp] recording as $GASP_RUN_ID -> ${GASP_STATE_REPO}"
    return 0
}

# gasp_task_planned <num> <title>
gasp_task_planned() {
    _gasp_emit task --state-dir "$GASP_STATE_DIR" --run-id "$GASP_RUN_ID" \
        --worker "evolve-shim-$$" --goal "$GASP_GOAL_ID" --num "$1" --title "$2"
}

# gasp_task_result <num> <title> <promoted|rejected> <pre-sha> <post-sha> [reason]
# GASP_EVAL_COMMAND names the oracle recorded on the eval fact — callers with
# a different gate (skill_evolve.sh) override it so the record never claims a
# stronger oracle than actually ran.
gasp_task_result() {
    _gasp_emit task-result --state-dir "$GASP_STATE_DIR" --run-id "$GASP_RUN_ID" \
        --worker "evolve-shim-$$" --goal "$GASP_GOAL_ID" \
        --num "$1" --title "$2" --verdict "$3" --pre-sha "$4" --post-sha "$5" \
        --repo "${REPO:-yologdev/yoyo-evolve}" --branch "$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo main)" \
        --eval-command "${GASP_EVAL_COMMAND:-}" \
        --reason "${6:-}"
}

# gasp_mirror_skills — idempotent full-tree sync of skills/ + skills_attic/
# into the state clone (GASP conformance rule 3), rebinding executor-layout
# paths to the state layout (learnings.jsonl -> facts.jsonl, journals/ ->
# journal/, ...) exactly as the original seed did. Full sync rather than a
# cycle diff, so one lost session never diverges the trees permanently —
# it stays "mirror-on-change" at the commit level because commit_run no-ops
# when nothing changed. The mirrored tree rides the same boundary commit as
# the events (session-end extra paths), so a skill version and its lineage
# ship together. Files present only in the state clone are removed
# (retire = the attic move mirrors as delete + add).
gasp_mirror_skills() {
    [ "$GASP_ENABLED" = true ] || return 0
    local dir f rel err
    for dir in skills skills_attic; do
        [ -d "$dir" ] || continue
        while IFS= read -r -d '' f; do
            mkdir -p "$GASP_STATE_DIR/$(dirname "$f")" 2>/dev/null || true
            # the shell owns the open/redirect so unreadable inputs and
            # write failures are non-zero and caught, with stderr surfaced
            if ! err=$(perl -pe '
                s/active_social_learnings\.md/active_social_memory.md/g;
                s/social_learnings\.jsonl/social_facts.jsonl/g;
                s/active_learnings\.md/active_memory.md/g;
                s/learnings\.jsonl/facts.jsonl/g;
                s/journals\//journal\//g;
            ' 2>&1 < "$f" > "$GASP_STATE_DIR/$f"); then
                _gasp_off "skill mirror failed for $f: $(printf '%s' "$err" | head -n 1)"
                return 0
            fi
            if [ -s "$f" ] && [ ! -s "$GASP_STATE_DIR/$f" ]; then
                _gasp_off "skill mirror produced empty output for non-empty $f"
                return 0
            fi
        done < <(find "$dir" -type f -print0 2>/dev/null)
        if [ -d "$GASP_STATE_DIR/$dir" ]; then
            while IFS= read -r -d '' f; do
                rel="${f#"$GASP_STATE_DIR/"}"
                [ -f "$rel" ] || rm -f "$f" 2>/dev/null || true
            done < <(find "$GASP_STATE_DIR/$dir" -type f -print0 2>/dev/null)
        fi
    done
    # name only directories that actually changed — git add fatals on
    # pathspecs matching nothing in the state clone
    GASP_EXTRA_PATHS=$(git -C "$GASP_STATE_DIR" status --porcelain -- skills skills_attic 2>/dev/null \
        | awk '{print $NF}' | cut -d/ -f1 | sort -u | paste -sd, - || true)
    [ -n "$GASP_EXTRA_PATHS" ] && echo "  [gasp] skills synced into the state repo"
    return 0
}

# Call after Step 8's code push. Closes the run, makes the boundary commit,
# pushes the state repo. The local clone is removed ONLY after a successful
# push — on failure it is preserved and named, because it holds the session's
# only complete record.
gasp_session_end() {
    [ "$GASP_ENABLED" = true ] || return 0
    local outcome="${1:-done}" out
    if ! out=$("$GASP_EMIT_BIN" session-end --state-dir "$GASP_STATE_DIR" \
        --run-id "$GASP_RUN_ID" --worker "evolve-shim-$$" --goal "$GASP_GOAL_ID" \
        --extra "$GASP_EXTRA_PATHS" --outcome "$outcome" 2>&1); then
        printf '%s\n' "$(_gasp_scrub "$out")" | sed 's/^/  [gasp] /' >&2
        _gasp_off "session-end failed"
        return 0
    fi
    printf '%s\n' "$out" | sed 's/^/  [gasp] /' || true
    if out=$(git -C "$GASP_STATE_DIR" push --quiet "$GASP_PUSH_URL" HEAD:main 2>&1); then
        echo "  [gasp] state pushed to ${GASP_STATE_REPO}"
        echo 0 > "$GASP_FAIL_COUNTER" 2>/dev/null || true
        rm -rf "$GASP_STATE_DIR" 2>/dev/null || true
    else
        echo "  [gasp] WARNING: state push failed — boundary commit preserved at ${GASP_STATE_DIR}" >&2
        echo "  [gasp]   $(_gasp_scrub "$(printf '%s' "$out" | tail -n 2 | tr '\n' '; ')")" >&2
        _gasp_note_failure
    fi
    return 0
}
