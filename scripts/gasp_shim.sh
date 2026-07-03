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
# GASP_TASK_KIND (product|evolve, optional) routes the task/patch to the
# matching standing goal: product work advances goal_product_value, evolve
# work advances the session goal — so the lineage graph separates what yoyo
# ships for users from what it invests in itself.
gasp_task_planned() {
    _gasp_emit task --state-dir "$GASP_STATE_DIR" --run-id "$GASP_RUN_ID" \
        --worker "evolve-shim-$$" --goal "$GASP_GOAL_ID" \
        --kind "${GASP_TASK_KIND:-}" --num "$1" --title "$2"
}

# gasp_task_result <num> <title> <promoted|rejected> <pre-sha> <post-sha> [reason]
# GASP_EVAL_COMMAND names the oracle recorded on the eval fact — callers with
# a different gate (skill_evolve.sh) override it so the record never claims a
# stronger oracle than actually ran.
gasp_task_result() {
    _gasp_emit task-result --state-dir "$GASP_STATE_DIR" --run-id "$GASP_RUN_ID" \
        --worker "evolve-shim-$$" --goal "$GASP_GOAL_ID" \
        --kind "${GASP_TASK_KIND:-}" \
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
    return 0
}

# The perl rebind shared by every mirror: executor-layout paths -> state layout.
_gasp_rebind() {
    perl -pe '
        s/active_social_learnings\.md/active_social_memory.md/g;
        s/social_learnings\.jsonl/social_facts.jsonl/g;
        s/active_learnings\.md/active_memory.md/g;
        s/learnings\.jsonl/facts.jsonl/g;
        s/journals\//journal\//g;
    '
}

# gasp_mirror_memory — keeps the state repo's memory/journal/dream streams
# current (called automatically from gasp_session_end, fail-soft):
#   - new learnings lines (session + social) are converted to the GASP fact
#     envelope {id, ts_ms, text, derived_from: [run], supersedes} and APPENDED
#     to memory/*facts.jsonl (append-only paths — never rewritten). Since the
#     seed was a verbatim copy, "new" = executor lines beyond the state file's
#     line count, so a missed session self-heals on the next one.
#   - new journal entries (yoyo PREPENDS to its journal; the state copy is
#     append-only) are extracted by their "## Day N — ..." headers and
#     appended oldest-first.
#   - dreams/dream_log.jsonl is appended by line count; regenerable
#     projections (active syntheses, DREAM.md, dream arc, notes, cursors,
#     DAY_COUNT) are overwritten — the spec's projection tier allows it.
# Generation stays in the executor; this copies outputs only.
gasp_mirror_memory() {
    [ "$GASP_ENABLED" = true ] || return 0
    local out
    if ! out=$(GASP_RUN_ID="$GASP_RUN_ID" GASP_STATE_DIR="$GASP_STATE_DIR" python3 - << 'PYEOF' 2>&1
import json, os, sys, time, uuid
from datetime import datetime, timezone

state = os.environ["GASP_STATE_DIR"]
run_id = os.environ["GASP_RUN_ID"]

def lines_of(path):
    try:
        with open(path, encoding="utf-8") as f:
            return f.read().splitlines()
    except FileNotFoundError:
        return None

# 1. learnings -> facts (append-only, envelope-converted)
for src, dst in [("memory/learnings.jsonl", "memory/facts.jsonl"),
                 ("memory/social_learnings.jsonl", "memory/social_facts.jsonl")]:
    src_lines = lines_of(src)
    if src_lines is None:
        continue
    dst_path = os.path.join(state, dst)
    dst_lines = lines_of(dst_path) or []
    if len(src_lines) < len(dst_lines):
        print(f"WARNING: {src} has fewer lines than {dst} — skipping (append-only source shrank?)")
        continue
    new = src_lines[len(dst_lines):]
    if not new:
        continue
    with open(dst_path, "a", encoding="utf-8") as f:
        for line in new:
            line = line.strip()
            if not line:
                continue
            try:
                legacy = json.loads(line)
                title = legacy.get("title", "")
                context = legacy.get("context", "")
                text = (f"{title} — {context}" if title and context
                        else title or context or json.dumps(legacy, ensure_ascii=False))
                ts = legacy.get("ts", "")
                try:
                    ts_ms = int(datetime.fromisoformat(ts.replace("Z", "+00:00")).timestamp() * 1000)
                except (ValueError, AttributeError):
                    ts_ms = int(time.time() * 1000)
            except json.JSONDecodeError:
                text, ts_ms = line, int(time.time() * 1000)
            fact = {"id": f"fact_{uuid.uuid4().hex}", "ts_ms": ts_ms, "text": text,
                    "derived_from": [run_id], "supersedes": None}
            f.write(json.dumps(fact, ensure_ascii=False) + "\n")
    print(f"facts: +{len(new)} from {src}")

# 2. journal: append entries whose header the state copy lacks (oldest first)
src_j = lines_of("journals/JOURNAL.md")
dst_j_path = os.path.join(state, "journal/JOURNAL.md")
dst_j = lines_of(dst_j_path)
if src_j is not None and dst_j is not None:
    have = {l for l in dst_j if l.startswith("## ")}
    entries, cur = [], None
    for l in src_j:
        if l.startswith("## "):
            cur = [l]
            entries.append(cur)
        elif cur is not None:
            cur.append(l)
    missing = [e for e in entries if e[0] not in have]
    if missing:
        with open(dst_j_path, "a", encoding="utf-8") as f:
            for e in reversed(missing):  # executor is newest-first; append chronologically
                block = "\n".join(e).rstrip("\n")
                f.write("\n" + block + "\n")
        print(f"journal: +{len(missing)} entrie(s)")

# 3. dream log: verbatim append by line count
src_d = lines_of("dreams/dream_log.jsonl")
if src_d is not None:
    dst_d_path = os.path.join(state, "dreams/dream_log.jsonl")
    os.makedirs(os.path.dirname(dst_d_path), exist_ok=True)
    dst_d = lines_of(dst_d_path) or []
    if len(src_d) > len(dst_d):
        with open(dst_d_path, "a", encoding="utf-8") as f:
            for line in src_d[len(dst_d):]:
                f.write(line + "\n")
        print(f"dream log: +{len(src_d) - len(dst_d)}")
PYEOF
    ); then
        _gasp_off "memory mirror failed: $(printf '%s' "$out" | tail -n 2 | tr '\n' '; ')"
        return 0
    fi
    [ -n "$out" ] && printf '%s\n' "$out" | sed 's/^/  [gasp] /'

    # regenerable projections + runtime state: plain overwrite copies
    local src dst
    while IFS=: read -r src dst; do
        [ -f "$src" ] || continue
        mkdir -p "$GASP_STATE_DIR/$(dirname "$dst")" 2>/dev/null || true
        _gasp_rebind < "$src" > "$GASP_STATE_DIR/$dst" 2>/dev/null || true
    done << 'MAP'
memory/active_learnings.md:memory/active_memory.md
memory/active_social_learnings.md:memory/active_social_memory.md
.yoyo/memory.json:memory/notes.json
.yoyo/social-state.json:memory/social_cursors.json
DREAM.md:DREAM.md
dreams/active_dream_arc.md:dreams/active_dream_arc.md
DAY_COUNT:DAY_COUNT
MAP
    return 0
}

# Call after Step 8's code push. Closes the run, makes the boundary commit,
# pushes the state repo. The local clone is removed ONLY after a successful
# push — on failure it is preserved and named, because it holds the session's
# only complete record.
gasp_session_end() {
    [ "$GASP_ENABLED" = true ] || return 0
    local outcome="${1:-done}" out

    # memory/journal/dream streams sync on every session close; everything
    # any mirror changed rides the boundary commit (state/ is always staged
    # by commit_run itself)
    gasp_mirror_memory
    GASP_EXTRA_PATHS=$(git -C "$GASP_STATE_DIR" status --porcelain 2>/dev/null \
        | awk '{print $NF}' | grep -v '^state/' | cut -d/ -f1 | sort -u | paste -sd, - || true)
    [ -n "$GASP_EXTRA_PATHS" ] && echo "  [gasp] state streams synced: ${GASP_EXTRA_PATHS}"

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
