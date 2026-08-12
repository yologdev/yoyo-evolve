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
    [ "${GITHUB_ACTIONS:-}" = "true" ] && echo "::warning::GASP state stream failure ($n recorded failures) — check the [gasp] log lines" 2>/dev/null
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
    # DO NOT export YOYO_GASP_STATE_DIR / YOYO_GASP_GOAL_ID here.
    #
    # src/gasp.rs reads those two vars, and bridging them to this clone looks
    # harmless — the in-process recorder records nothing until #683 box 4/5
    # lands. It is not harmless: `GaspRecorder::open` MUTATES the store. A GASP
    # repo is single-writer, and this shim is already the writer, holding a run
    # open from session-start until session-end.
    #
    # Measured end-to-end against a real yoyo-gasp clone, not reasoned about:
    #   within  600s of a sidecar call (git_store.rs lease_ttl):
    #       open fails, "lease held by worker evolve-shim-<pid>" -> recorder
    #       disabled. Noisy, harmless, and useless.
    #   after  600s — i.e. any invocation following a 30-min impl task, which
    #   is most of them:
    #       the in-process worker STEALS the expired lease, writes
    #       run.finished{outcome:"interrupted"} against the sidecar's live run,
    #       and never releases the lease (release happens only in
    #       record_stream's close path, which never runs — main.rs opens the
    #       recorder and drops it). Then the sidecar's own session-end fails:
    #           Error: Validation("cannot finish <run>: no run is open")
    #       -> _gasp_off -> no boundary commit, no push. The ENTIRE session's
    #       GASP record is lost, and the failure counter blames GH_PAT.
    #
    # This is the two-writer interim #683 explicitly exists to avoid
    # ("replacement avoids ever operating the awkward two-writer interim").
    # The bridge is correct only AFTER tools/gasp-emit is retired and this shim
    # stops opening runs — same commit, not before. Until then the in-process
    # recorder must stay unreachable, which is exactly what an unset
    # YOYO_GASP_STATE_DIR gives us (src/gasp.rs -> RecorderPlan::Disabled).
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
# current (called automatically from gasp_session_end):
#   - new learnings lines (session + social) convert to the GASP fact envelope
#     and APPEND to memory/*facts.jsonl; dreams/dream_log.jsonl appends
#     verbatim. Progress is tracked by an explicit cursor
#     (memory/.gasp_mirror_cursor.json in the state repo: consumed line count
#     + hash of the last consumed line, verified before every append) — never
#     inferred from destination line counts, so blank lines, unicode line
#     separators, or an upstream rewrite of a source file can stall a stream
#     loudly but can never append duplicated or misaligned facts.
#   - new journal entries (yoyo PREPENDS; the state copy is append-only) are
#     matched by strict "## Day N — " headers with multiset counting
#     (duplicate headers append their surplus) and appended oldest-first.
#   - regenerable projections (active syntheses, DREAM.md, dream arc,
#     DAY_COUNT) are overwritten; notes/cursors JSON copies verbatim (no
#     rebind — a substitution inside a seen-state key could reset it).
# Failure isolation: each stream degrades independently with its own warning;
# a broken stream NEVER disables event recording or the boundary commit.
# Generation stays in the executor; this copies outputs only.
gasp_mirror_memory() {
    [ "$GASP_ENABLED" = true ] || return 0
    local out root
    root=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
    out=$(GASP_RUN_ID="$GASP_RUN_ID" GASP_STATE_DIR="$GASP_STATE_DIR" GASP_SRC_ROOT="$root" python3 - << 'PYEOF' 2>&1
import hashlib, json, os, re, sys, time, uuid
from datetime import datetime

state = os.environ["GASP_STATE_DIR"]
run_id = os.environ["GASP_RUN_ID"]
root = os.environ["GASP_SRC_ROOT"]
CURSOR_PATH = os.path.join(state, "memory/.gasp_mirror_cursor.json")

def warn(msg):
    print(f"WARN {msg}")

def lines_of(path):
    """Physical \\n-split lines (never splitlines(): U+2028/NEL must not count)."""
    try:
        with open(path, encoding="utf-8", errors="strict") as f:
            raw = f.read()
    except FileNotFoundError:
        return None
    except UnicodeDecodeError as e:
        warn(f"{path}: undecodable ({e}) — stream skipped")
        return None
    lines = raw.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    return lines

def line_hash(line):
    return hashlib.sha256(line.encode("utf-8")).hexdigest()[:16]

def ensure_trailing_newline(path):
    """Appending after a file that lacks a final newline would merge lines."""
    try:
        with open(path, "rb") as f:
            f.seek(-1, os.SEEK_END)
            if f.read(1) != b"\n":
                with open(path, "a", encoding="utf-8") as g:
                    g.write("\n")
    except OSError:
        pass  # empty file or unseekable: append is safe

try:
    cursors = json.load(open(CURSOR_PATH, encoding="utf-8"))
except (FileNotFoundError, json.JSONDecodeError):
    cursors = {}

def sync_stream(src_rel, dst_rel, convert):
    """Append src lines beyond the verified cursor; each stream fails alone."""
    src = os.path.join(root, src_rel)
    dst = os.path.join(state, dst_rel)
    src_lines = lines_of(src)
    if src_lines is None:
        return
    cur = cursors.get(src_rel)
    if cur is None:
        # first run with a cursor: trust the seed alignment (dst length),
        # exactly the old semantics, then never infer again
        dst_lines = lines_of(dst) or []
        consumed = min(len(dst_lines), len(src_lines))
        cur = {"consumed": consumed,
               "last": line_hash(src_lines[consumed - 1]) if consumed else ""}
    consumed = int(cur.get("consumed", 0))
    if consumed > len(src_lines):
        warn(f"{src_rel}: source shrank below cursor ({len(src_lines)} < {consumed}) — stream stalled, reconcile manually")
        return
    if consumed and line_hash(src_lines[consumed - 1]) != cur.get("last"):
        warn(f"{src_rel}: content at cursor changed (source rewritten?) — stream stalled, reconcile manually")
        return
    new = src_lines[consumed:]
    if new:
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        if os.path.exists(dst):
            ensure_trailing_newline(dst)
        written = 0
        with open(dst, "a", encoding="utf-8") as f:
            for line in new:
                out_line = convert(line)
                if out_line is not None:
                    f.write(out_line + "\n")
                    written += 1
        if written:
            print(f"{dst_rel}: +{written} from {src_rel}")
    cursors[src_rel] = {"consumed": len(src_lines),
                        "last": line_hash(src_lines[-1]) if src_lines else ""}

def to_fact(line):
    line = line.strip()
    if not line:
        return None
    try:
        legacy = json.loads(line)
        title = legacy.get("title", "")
        context = legacy.get("context", "")
        text = (f"{title} — {context}" if title and context
                else title or context or json.dumps(legacy, ensure_ascii=True))
        ts = legacy.get("ts", "")
        try:
            ts_ms = int(datetime.fromisoformat(ts.replace("Z", "+00:00")).timestamp() * 1000)
        except (ValueError, AttributeError):
            ts_ms = int(time.time() * 1000)
    except json.JSONDecodeError:
        text, ts_ms = line, int(time.time() * 1000)
    fact = {"id": f"fact_{uuid.uuid4().hex}", "ts_ms": ts_ms, "text": text,
            "derived_from": [run_id], "supersedes": None}
    # ensure_ascii=True: raw U+2028/U+2029 in output would break line counts
    return json.dumps(fact, ensure_ascii=True)

def verbatim(line):
    return line if line.strip() else None

for args in [("memory/learnings.jsonl", "memory/facts.jsonl", to_fact),
             ("memory/social_learnings.jsonl", "memory/social_facts.jsonl", to_fact),
             ("dreams/dream_log.jsonl", "dreams/dream_log.jsonl", verbatim)]:
    try:
        sync_stream(*args)
    except Exception as e:  # one broken stream must not stop the others
        warn(f"{args[0]}: mirror error: {type(e).__name__}: {e}")

# journal: strict headers, multiset dedup, append surplus oldest-first
try:
    HEADER = re.compile(r"^## Day \d+ ")
    src_j = lines_of(os.path.join(root, "journals/JOURNAL.md"))
    dst_j_path = os.path.join(state, "journal/JOURNAL.md")
    dst_j = lines_of(dst_j_path)
    if src_j is None:
        warn("journals/JOURNAL.md: not found — journal skipped")
    elif dst_j is None:
        warn("state journal/JOURNAL.md: not found — journal skipped")
    else:
        from collections import Counter
        have = Counter(l for l in dst_j if HEADER.match(l))
        entries, cur_e = [], None
        for l in src_j:
            if HEADER.match(l):
                cur_e = [l]
                entries.append(cur_e)
            elif cur_e is not None:
                cur_e.append(l)
        missing, skipped = [], 0
        for e in entries:
            if have[e[0]] > 0:
                have[e[0]] -= 1
                skipped += 1
            else:
                missing.append(e)
        if missing:
            ensure_trailing_newline(dst_j_path)
            with open(dst_j_path, "a", encoding="utf-8") as f:
                for e in reversed(missing):  # executor is newest-first
                    f.write("\n" + "\n".join(e).rstrip("\n") + "\n")
            print(f"journal: +{len(missing)} entrie(s) ({skipped} already present)")
except Exception as e:
    warn(f"journal: mirror error: {type(e).__name__}: {e}")

os.makedirs(os.path.dirname(CURSOR_PATH), exist_ok=True)
with open(CURSOR_PATH, "w", encoding="utf-8") as f:
    json.dump(cursors, f, indent=1, sort_keys=True)
PYEOF
    ) || warn_rc=$?
    if [ -n "$out" ]; then
        printf '%s\n' "$out" | sed 's/^WARN /  [gasp] WARNING: /; s/^\([^ ]\)/  [gasp] \1/' | sed 's/^  \[gasp\]   \[gasp\]/  [gasp]/'
        if printf '%s' "$out" | grep -q '^WARN '; then
            _gasp_note_failure
        fi
    fi
    if [ "${warn_rc:-0}" != "0" ]; then
        echo "  [gasp] WARNING: memory mirror crashed (rc=$warn_rc) — events still record" >&2
        _gasp_note_failure
    fi

    # regenerable projections: rebind-copy with the mirror_skills guard
    # pattern (never leave a truncated file behind silently); JSON runtime
    # state copies verbatim — no rebind inside opaque data
    local src dst err
    while IFS=: read -r src dst; do
        src="$root/$src"
        [ -f "$src" ] || continue
        mkdir -p "$GASP_STATE_DIR/$(dirname "$dst")" 2>/dev/null || true
        case "$dst" in
            *.json)
                cp "$src" "$GASP_STATE_DIR/$dst" 2>/dev/null \
                    || echo "  [gasp] WARNING: copy failed for $dst" >&2 ;;
            *)
                if ! err=$(_gasp_rebind 2>&1 < "$src" > "$GASP_STATE_DIR/$dst"); then
                    echo "  [gasp] WARNING: projection rebind failed for $dst: $(printf '%s' "$err" | head -n 1)" >&2
                elif [ -s "$src" ] && [ ! -s "$GASP_STATE_DIR/$dst" ]; then
                    echo "  [gasp] WARNING: projection rebind emptied $dst — restoring verbatim copy" >&2
                    cp "$src" "$GASP_STATE_DIR/$dst" 2>/dev/null || true
                fi ;;
        esac
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
    # NUL-parsed porcelain (verbatim paths — spaces/quotes safe), renames off
    GASP_EXTRA_PATHS=$(git -C "$GASP_STATE_DIR" -c status.renames=false status --porcelain=v1 -z 2>/dev/null \
        | tr '\0' '\n' | cut -c4- | grep -v '^state/' | grep -v '^$' | cut -d/ -f1 | sort -u | paste -sd, - || true)
    [ -n "$GASP_EXTRA_PATHS" ] && echo "  [gasp] state streams synced: ${GASP_EXTRA_PATHS}"

    if ! out=$("$GASP_EMIT_BIN" session-end --state-dir "$GASP_STATE_DIR" \
        --run-id "$GASP_RUN_ID" --worker "evolve-shim-$$" --goal "$GASP_GOAL_ID" \
        --extra "$GASP_EXTRA_PATHS" --outcome "$outcome" 2>&1); then
        printf '%s\n' "$(_gasp_scrub "$out")" | sed 's/^/  [gasp] /' >&2
        _gasp_off "session-end failed"
        return 0
    fi
    printf '%s\n' "$out" | sed 's/^/  [gasp] /' || true
    # The clone is hours old by session end; any other loop (social runs
    # ~4x/day) that pushed its own record in the meantime makes our push a
    # non-fast-forward reject. Day 160 lost a full session record this way:
    # "preserved at /tmp/..." on an EPHEMERAL runner is deletion with extra
    # steps. A rebase retry recovers DISJOINT-file races only — concurrent
    # gasp pushes both append to the shared state/events.jsonl (yoagent-state
    # DEFAULT_EVENTS_PATH) and the memory mirrors rewrite shared files, so
    # same-file races still conflict unless the state repo's .gitattributes
    # marks *.jsonl merge=union (added alongside this fix). Best-effort; the
    # structural fix is #683's single in-process writer.
    local push_ok rb
    push_ok=false
    rb=""
    if out=$(git -C "$GASP_STATE_DIR" push --quiet "$GASP_PUSH_URL" HEAD:main 2>&1); then
        push_ok=true
    elif rb=$(git -C "$GASP_STATE_DIR" pull --rebase --quiet "$GASP_PUSH_URL" main 2>&1) \
        && out=$(git -C "$GASP_STATE_DIR" push --quiet "$GASP_PUSH_URL" HEAD:main 2>&1); then
        echo "  [gasp] state pushed after rebase (another session pushed mid-run)"
        push_ok=true
    else
        # A failed rebase leaves the clone mid-rebase with conflict markers —
        # abort so the "preserved" boundary commit is actually readable.
        git -C "$GASP_STATE_DIR" rebase --abort 2>/dev/null || true
    fi
    if [ "$push_ok" = true ]; then
        echo "  [gasp] state pushed to ${GASP_STATE_REPO}"
        echo 0 > "$GASP_FAIL_COUNTER" 2>/dev/null || true
        rm -rf "$GASP_STATE_DIR" 2>/dev/null || true
        GASP_ENABLED=false  # terminal: a later abort-trap call is a no-op
    else
        echo "  [gasp] WARNING: state push failed — boundary commit preserved at ${GASP_STATE_DIR}" >&2
        # Label the two errors separately: ${rb}${out} concatenation let the
        # stale pre-rebase push error shadow the actual rebase failure
        # (review finding — the message named the wrong cause).
        if [ -n "$rb" ]; then
            echo "  [gasp]   rebase failed: $(_gasp_scrub "$(printf '%s' "$rb" | tail -n 2 | tr '\n' '; ')")" >&2
        fi
        echo "  [gasp]   push: $(_gasp_scrub "$(printf '%s' "$out" | tail -n 2 | tr '\n' '; ')")" >&2
        _gasp_note_failure
    fi
    return 0
}
