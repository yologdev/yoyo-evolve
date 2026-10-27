#!/usr/bin/env bash
# common.sh — shared auto-detection for fork-friendly operation.
# Source this from evolve.sh, social.sh, daily_diary.sh, etc.
# Exports: REPO, BOT_LOGIN, BOT_SLUG, BIRTH_DATE
# All variables have sensible defaults for yoyo-evolve; forks override via env.

_COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_REPO_ROOT="$(cd "$_COMMON_DIR/.." && pwd)"

# Auto-detect repo from git remote if not set via env
if [ -z "${REPO:-}" ]; then
    REPO=$(git remote get-url origin 2>/dev/null | sed -E 's|.*github\.com[:/]||; s|\.git$||')
fi
if [ -z "${REPO:-}" ]; then
    echo "FATAL: Could not detect REPO from git remote. Set REPO env var." >&2
    exit 1
fi

# Bot identity — detected from GitHub App in CI, defaults for local runs.
# In CI, both BOT_LOGIN and BOT_SLUG are set by the workflow's "Detect bot identity" step.
# These defaults only apply for local runs.
BOT_SLUG="${BOT_SLUG:-yoyo-evolve}"
BOT_LOGIN="${BOT_LOGIN:-${BOT_SLUG}[bot]}"

# Birth date — when the agent was born.
# Existing agents (DAY_COUNT exists): use hardcoded default (2026-02-28 for yoyo).
# New forks (no DAY_COUNT): birth date is today.
# Override: set BIRTH_DATE env var.
if [ -z "${BIRTH_DATE:-}" ]; then
    if [ -f "$_REPO_ROOT/DAY_COUNT" ]; then
        BIRTH_DATE="2026-02-28"
    else
        BIRTH_DATE=$(date +%Y-%m-%d)
    fi
fi

# ── auto_watch off around one yoyo call ──
# .yoyo.toml sets auto_watch = true for the evolve loop. The social and dream
# loops run yoyo in this same checkout, so a turn that edited a file ran
# `cargo clippy && cargo test` there too (synthesize's dream step took 449s on
# 2026-10-26 for this reason; synthesize.yml now turns it off the same way).
# These two wrap the call: off before it, the file restored byte-for-byte
# after, so the post-run scope checks never see .yoyo.toml as changed and
# nothing can commit the edit. Both are fail-soft.
YOYO_WATCH_BACKUP=""
yoyo_watch_off() {
    [ -f .yoyo.toml ] && grep -q '^auto_watch[[:space:]]*=' .yoyo.toml || return 0
    YOYO_WATCH_BACKUP=$(mktemp) || { YOYO_WATCH_BACKUP=""; return 0; }
    cp .yoyo.toml "$YOYO_WATCH_BACKUP" || { rm -f "$YOYO_WATCH_BACKUP"; YOYO_WATCH_BACKUP=""; return 0; }
    sed 's/^auto_watch[[:space:]]*=.*/auto_watch = false/' "$YOYO_WATCH_BACKUP" > .yoyo.toml \
        || cp "$YOYO_WATCH_BACKUP" .yoyo.toml
    echo "  auto_watch off for this run (restored after the agent exits)"
    return 0
}
yoyo_watch_restore() {
    [ -n "$YOYO_WATCH_BACKUP" ] && [ -f "$YOYO_WATCH_BACKUP" ] || return 0
    if cp "$YOYO_WATCH_BACKUP" .yoyo.toml; then
        rm -f "$YOYO_WATCH_BACKUP"
    else
        echo "  WARNING: could not restore .yoyo.toml from $YOYO_WATCH_BACKUP" >&2
    fi
    YOYO_WATCH_BACKUP=""
    return 0
}
