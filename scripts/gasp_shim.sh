#!/bin/bash
# scripts/gasp_shim.sh — GASP instrumentation for evolve.sh (sourced, never run).
#
# Emits yoyo's session transitions as GASP events (github.com/yologdev/gasp)
# into the yoyo-gasp state repo (github.com/yologdev/yoyo-gasp), via the
# gasp-emit sidecar (tools/gasp-emit, built on yoagent-state 0.4).
#
# DESIGN RULE: strictly fail-soft. Every entry point returns 0; any failure
# disables emission for the rest of the session with a warning. An evolve
# session must never break because state instrumentation did.
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

GASP_ENABLED=false
GASP_STATE_REPO="${GASP_STATE_REPO:-github.com/yologdev/yoyo-gasp}"
GASP_STATE_DIR="${GASP_STATE_DIR:-/tmp/yoyo-gasp-state.$$}"
GASP_EMIT_BIN="target/gasp-emit/debug/gasp-emit"
GASP_RUN_ID=""

_gasp_off() {
    echo "  [gasp] $1 — GASP emission disabled for this session" >&2
    GASP_ENABLED=false
    return 0
}

_gasp_emit() {
    [ "$GASP_ENABLED" = true ] || return 0
    if ! "$GASP_EMIT_BIN" "$@" >/dev/null 2>&1; then
        _gasp_off "gasp-emit $1 failed"
    fi
    return 0
}

# Call once, after the baseline build passes. Clones the state repo, builds
# the emitter, opens the run.
gasp_session_start() {
    [ "${GASP_DISABLE:-}" = "1" ] && return 0
    local day="${1:-0}"

    # 1. build the emitter (own manifest; target/ is cached+ignored already)
    if ! cargo build --quiet --manifest-path tools/gasp-emit/Cargo.toml \
        --target-dir target/gasp-emit 2>/dev/null; then
        _gasp_off "gasp-emit build failed"; return 0
    fi

    # 2. clone the state repo (authenticated when GH_PAT is available).
    # GASP_STATE_REPO may be owner/name (default), a full URL, or a local path.
    local url
    case "$GASP_STATE_REPO" in
        *://*|/*) url="$GASP_STATE_REPO" ;;
        *)  url="https://${GASP_STATE_REPO}.git"
            [ -n "${GH_PAT:-}" ] && url="https://x-access-token:${GH_PAT}@${GASP_STATE_REPO}.git" ;;
    esac
    rm -rf "$GASP_STATE_DIR"
    if ! git clone --quiet "$url" "$GASP_STATE_DIR" 2>/dev/null; then
        _gasp_off "cannot clone ${GASP_STATE_REPO}"; return 0
    fi
    git -C "$GASP_STATE_DIR" config user.name "yoyo[gasp]" 2>/dev/null
    git -C "$GASP_STATE_DIR" config user.email "yoyo-gasp@users.noreply.github.com" 2>/dev/null

    GASP_RUN_ID="run_day${day}_$(date -u +%Y%m%dT%H%M%SZ)"
    GASP_ENABLED=true
    _gasp_emit session-start --state-dir "$GASP_STATE_DIR" --run-id "$GASP_RUN_ID" \
        --day "$day" --task "evolve session day $day"
    [ "$GASP_ENABLED" = true ] && echo "  [gasp] recording as $GASP_RUN_ID -> ${GASP_STATE_REPO}"
    return 0
}

# gasp_task_planned <num> <title>
gasp_task_planned() {
    _gasp_emit task --state-dir "$GASP_STATE_DIR" --run-id "$GASP_RUN_ID" \
        --num "$1" --title "$2"
}

# gasp_task_result <num> <title> <promoted|rejected> <pre-sha> <post-sha> [reason]
gasp_task_result() {
    _gasp_emit task-result --state-dir "$GASP_STATE_DIR" --run-id "$GASP_RUN_ID" \
        --num "$1" --title "$2" --verdict "$3" --pre-sha "$4" --post-sha "$5" \
        --repo "${REPO:-yologdev/yoyo-evolve}" --branch "$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo main)" \
        --reason "${6:-}"
}

# Call after Step 8's code push. Closes the run, makes the boundary commit,
# pushes the state repo.
gasp_session_end() {
    [ "$GASP_ENABLED" = true ] || return 0
    local outcome="${1:-done}"
    if ! "$GASP_EMIT_BIN" session-end --state-dir "$GASP_STATE_DIR" \
        --run-id "$GASP_RUN_ID" --outcome "$outcome" 2>&1 | sed 's/^/  [gasp] /'; then
        _gasp_off "session-end failed"; return 0
    fi
    if git -C "$GASP_STATE_DIR" push --quiet origin HEAD:main 2>/dev/null; then
        echo "  [gasp] state pushed to ${GASP_STATE_REPO}"
    else
        echo "  [gasp] WARNING: state push failed — boundary commit remains local to the runner" >&2
    fi
    rm -rf "$GASP_STATE_DIR"
    return 0
}
