#!/usr/bin/env bash
# Helpers for .github/workflows/synthesize.yml (sourced, not run).
#
# 1. Skip-when-unchanged. Synthesis re-reads each whole archive and rewrites
#    its active file every day, even when the archive has not changed since
#    the last synthesis. memory/.synthesize_inputs records the sha256 of each
#    archive as it was when its active file was last regenerated; a step whose
#    archive still has that hash is skipped. A manual dispatch (FORCE=true)
#    runs every non-empty archive.
# 2. Spend row (#944, #1071). Each step runs yoyo with YOYO_AUDIT=1; the usage
#    records it appends to .yoyo/audit.jsonl are collected per step, and one
#    row per run goes to phases/synthesize/spend.jsonl on the audit-log
#    branch, the shape scripts/extract_trajectory.py reads for social and
#    dream. A step that ran but left no usage record is kept as
#    "no_terminal_emit", never as zero.
#
# Every function is fail-soft: synthesis must never fail because of its own
# bookkeeping.

SYNTH_STATE_FILE="${SYNTH_STATE_FILE:-memory/.synthesize_inputs}"
SYNTH_AUDIT_FILE="${SYNTH_AUDIT_FILE:-.yoyo/audit.jsonl}"
SYNTH_WORK_DIR="${SYNTH_WORK_DIR:-${RUNNER_TEMP:-/tmp}/synthesize}"
SYNTH_SPEND_SINK="phases/synthesize/spend.jsonl"
# key → archive. Order is the order the workflow runs the steps in.
SYNTH_KEYS="learnings social dreams"

synth_archive() { # $1 = key → archive path
    case "$1" in
        learnings) echo "memory/learnings.jsonl" ;;
        social) echo "memory/social_learnings.jsonl" ;;
        dreams) echo "dreams/dream_log.jsonl" ;;
    esac
}

synth_active() { # $1 = key → the active file its synthesis writes
    case "$1" in
        learnings) echo "memory/active_learnings.md" ;;
        social) echo "memory/active_social_learnings.md" ;;
        dreams) echo "dreams/active_dream_arc.md" ;;
    esac
}

synth_header() { # $1 = key → the first line a valid active file starts with
    case "$1" in
        learnings) echo "# Active Learnings" ;;
        social) echo "# Active Social Learnings" ;;
        dreams) echo "# Active Dream Arc" ;;
    esac
}

# $1 = key: true when this run really regenerated the active file. yoyo
# exits 0 in piped mode even when the model never wrote the file (turn limit,
# an answer in chat, a stub), so the exit code alone is not success: the file
# must be non-empty, start with its header, and differ from the backup taken
# before the step. A step that fails this records no hash and runs next time.
synth_output_ok() {
    local active header bak
    active=$(synth_active "$1"); header=$(synth_header "$1"); bak="${active}.bak"
    [ -s "$active" ] || return 1
    [ "$(head -1 "$active")" = "$header" ] || return 1
    if [ -f "$bak" ] && cmp -s "$active" "$bak"; then
        return 1
    fi
    return 0
}

synth_sha() { # $1 = file → sha256, or nothing when unreadable
    [ -r "$1" ] || return 0
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

synth_recorded_sha() { # $1 = key → the sha recorded at the last synthesis, or nothing
    [ -r "$SYNTH_STATE_FILE" ] || return 0
    awk -v k="$1" '$1 == k { print $2; exit }' "$SYNTH_STATE_FILE"
}

# Prints, for each key, "run_<key>=true|false", "sha_<key>=<sha>" and a
# reason line on stderr; then "skip=true|false" (true when no step runs).
# FORCE=true runs every non-empty archive.
synth_plan() {
    local key archive sha recorded run any=false
    for key in $SYNTH_KEYS; do
        archive=$(synth_archive "$key")
        sha=$(synth_sha "$archive")
        recorded=$(synth_recorded_sha "$key")
        run=false
        if [ ! -s "$archive" ]; then
            echo "  $key: $archive is empty or missing; skipped." >&2
        elif [ "${FORCE:-}" = "true" ]; then
            run=true
            echo "  $key: manual run; synthesizing." >&2
        elif [ -n "$recorded" ] && [ "$recorded" = "$sha" ] && [ -s "$(synth_active "$key")" ]; then
            echo "  $key: $archive unchanged since the last synthesis; skipped." >&2
        elif [ -n "$recorded" ] && [ "$recorded" = "$sha" ]; then
            run=true
            echo "  $key: $archive unchanged, but $(synth_active "$key") is missing or empty; synthesizing." >&2
        else
            run=true
            echo "  $key: $archive changed since the last synthesis; synthesizing." >&2
        fi
        echo "run_${key}=${run}"
        echo "sha_${key}=${sha}"
        [ "$run" = true ] && any=true
    done
    if [ "$any" = true ]; then echo "skip=false"; else echo "skip=true"; fi
}

synth_record() { # $1 = key, $2 = sha: record that $1's active file now reflects $2
    local key="$1" sha="$2" tmp
    [ -n "$key" ] && [ -n "$sha" ] || return 0
    mkdir -p "$(dirname "$SYNTH_STATE_FILE")" 2>/dev/null || return 0
    tmp="${SYNTH_STATE_FILE}.tmp.$$"
    { [ -r "$SYNTH_STATE_FILE" ] && awk -v k="$key" '$1 != k' "$SYNTH_STATE_FILE"; echo "$key $sha"; } \
        | sort > "$tmp" 2>/dev/null && mv "$tmp" "$SYNTH_STATE_FILE" || rm -f "$tmp"
    return 0
}

synth_usage_mark() { # $1 = key, before its yoyo call: note the step started and where the audit file ends
    mkdir -p "$SYNTH_WORK_DIR" 2>/dev/null || return 0
    local n=0
    [ -f "$SYNTH_AUDIT_FILE" ] && n=$(wc -l < "$SYNTH_AUDIT_FILE" | tr -d '[:space:]')
    echo "${n:-0}" > "$SYNTH_WORK_DIR/mark" 2>/dev/null || true
    : > "$SYNTH_WORK_DIR/started_$1" 2>/dev/null || true
    return 0
}

# Before the spend row: a planned step that started but never recorded an
# outcome (the job was cancelled or timed out mid-step, or the step died under
# -e) still spent tokens. Steps run one at a time, so its records are the ones
# after the last mark. A planned step that never started is "not_run".
# $1 = space-separated planned keys.
synth_claim_interrupted() {
    local key
    for key in $1; do
        [ -f "$SYNTH_WORK_DIR/outcome_$key" ] && continue
        if [ -f "$SYNTH_WORK_DIR/started_$key" ]; then
            synth_usage_collect "$key" interrupted
        else
            mkdir -p "$SYNTH_WORK_DIR" 2>/dev/null && echo not_run > "$SYNTH_WORK_DIR/outcome_$key"
        fi
    done
    return 0
}

# $1 = key, $2 = ran|failed: after a step's yoyo call, keep the usage records
# it appended and the step's outcome.
synth_usage_collect() {
    local key="$1" outcome="$2" mark=0 now=0
    mkdir -p "$SYNTH_WORK_DIR" 2>/dev/null || return 0
    [ -r "$SYNTH_WORK_DIR/mark" ] && mark=$(cat "$SYNTH_WORK_DIR/mark")
    case "$mark" in '' | *[!0-9]*) mark=0 ;; esac
    [ -f "$SYNTH_AUDIT_FILE" ] && now=$(wc -l < "$SYNTH_AUDIT_FILE" | tr -d '[:space:]')
    case "$now" in '' | *[!0-9]*) now=0 ;; esac
    : > "$SYNTH_WORK_DIR/usage_$key"
    if [ "$now" -gt "$mark" ]; then
        tail -n "$((now - mark))" "$SYNTH_AUDIT_FILE" | grep -a '"type":"usage"' > "$SYNTH_WORK_DIR/usage_$key" || true
    fi
    echo "$outcome" > "$SYNTH_WORK_DIR/outcome_$key"
    return 0
}

# Prints one JSON row for this run, or nothing when no step ran.
synth_spend_row() {
    python3 - "$SYNTH_WORK_DIR" "$SYNTH_KEYS" <<'PY'
import datetime, json, os, sys
work, keys = sys.argv[1], sys.argv[2].split()
steps, records, ran = {}, [], False
for key in keys:
    outcome_f = os.path.join(work, "outcome_" + key)
    if not os.path.exists(outcome_f):
        steps[key] = "skipped"
        continue
    outcome = open(outcome_f).read().strip() or "ran"
    if outcome == "not_run":
        steps[key] = outcome
        continue
    ran = True
    recs = []
    usage_f = os.path.join(work, "usage_" + key)
    if os.path.exists(usage_f):
        for line in open(usage_f, errors="replace"):
            line = line.strip()
            if not line:
                continue
            try:
                recs.append(json.loads(line))
            except ValueError:
                pass
    steps[key] = outcome if recs else outcome + ", no_terminal_emit"
    records.extend(recs)
if ran:
    row = {"ts": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
           "phase": "synthesize", "steps": steps}
    if records:
        row["usage_records"] = records
    else:
        row["usage"] = "no_terminal_emit"
    print(json.dumps(row, separators=(",", ":")))
PY
}

# Appends this run's row to audit-log:phases/synthesize/spend.jsonl, the way
# scripts/social.sh's persist_social_spend does: a detached temp worktree, one
# retry on a rejected push, main's checkout and index untouched.
synth_persist_spend() { # $1 = space-separated keys the plan said to run
    local row wt rc step=""
    synth_claim_interrupted "${1:-}"
    if ! row=$(synth_spend_row); then
        echo "Spend sink: FAILED at step build-row; row not persisted." >&2
        return 0
    fi
    if [ -z "$row" ]; then
        echo "Spend sink: no synthesis step ran; nothing to record."
        return 0
    fi
    echo "Spend (this run): $row"
    rc=0
    git ls-remote --exit-code --heads origin audit-log >/dev/null 2>&1 || rc=$?
    if [ "$rc" -eq 2 ]; then
        echo "Spend sink: skipped; origin has no audit-log branch."
        return 0
    elif [ "$rc" -ne 0 ]; then
        echo "Spend sink: FAILED at step ls-remote (rc=$rc); row not persisted." >&2
        return 0
    fi
    if ! git fetch -q --depth 50 origin "+refs/heads/audit-log:refs/remotes/origin/audit-log" 2>/dev/null; then
        echo "Spend sink: FAILED at step fetch; row not persisted." >&2
        return 0
    fi
    wt="${RUNNER_TEMP:-/tmp}/synth-spend-wt-$$-${RANDOM}"
    if ! git worktree add -q --detach "$wt" refs/remotes/origin/audit-log >/dev/null 2>&1; then
        echo "Spend sink: FAILED at step worktree-add; row not persisted." >&2
        rm -rf "$wt" 2>/dev/null || true
        git worktree prune 2>/dev/null || true
        return 0
    fi
    step=$(
        cd "$wt" || { echo cd; exit 0; }
        mkdir -p "$(dirname "$SYNTH_SPEND_SINK")" || { echo mkdir; exit 0; }
        printf '%s\n' "$row" >> "$SYNTH_SPEND_SINK" || { echo append; exit 0; }
        git add -- "$SYNTH_SPEND_SINK" >/dev/null 2>&1 || { echo add; exit 0; }
        git commit -q -m "audit: synthesize spend ($(date -u +%Y-%m-%d))" >/dev/null 2>&1 || { echo commit; exit 0; }
        if git push -q --no-verify origin HEAD:refs/heads/audit-log >/dev/null 2>&1; then
            exit 0
        fi
        git fetch -q --depth 50 origin "+refs/heads/audit-log:refs/remotes/origin/audit-log" >/dev/null 2>&1 || { echo retry-fetch; exit 0; }
        git rebase -q refs/remotes/origin/audit-log >/dev/null 2>&1 || { git rebase --abort >/dev/null 2>&1; echo retry-rebase; exit 0; }
        git push -q --no-verify origin HEAD:refs/heads/audit-log >/dev/null 2>&1 || { echo retry-push; exit 0; }
    )
    git worktree remove --force "$wt" >/dev/null 2>&1 || true
    rm -rf "$wt" 2>/dev/null || true
    git worktree prune 2>/dev/null || true
    if [ -n "$step" ]; then
        echo "Spend sink: FAILED at step ${step}; row not persisted." >&2
    else
        echo "Spend sink: appended this run's row to audit-log:${SYNTH_SPEND_SINK}"
    fi
    return 0
}
