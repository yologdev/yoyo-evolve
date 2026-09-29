#!/bin/bash
# scripts/dream.sh — One DREAM cycle.
#
# yoyo's time to look up from the code and out at the world. It uses its
# research skill to wander anywhere its curiosity goes (art, culture, science,
# not only software), checks in honestly with each dream it holds, and tends
# them — self-chosen dreams kept in DREAM.md. Dreams grown through code carry a
# next milestone the evolve loop chips at; others are pursued in these cycles.
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
#   MODEL                 — LLM model override for one run (default: `model` in .yoyo.toml)
#   DREAM_COOLDOWN_SECS   — minimum seconds between cycles (default: 604800 = 7d)
#   DREAM_TIMEOUT         — agent wall-clock budget seconds (default: 900)
#   FALLBACK_PROVIDER     — passed through to yoyo as --fallback
#   FORCE_RUN             — "true" bypasses the cooldown gate (manual dispatch)
#   DREAM_DRY_RUN         — "true" composes the prompt and exits before the agent
#                           (still subject to the cooldown gate; pair with
#                           FORCE_RUN=true to bypass it)

set -euo pipefail

source "$(dirname "$0")/common.sh"

# Model: the top-level `model` in .yoyo.toml is the single source (it sits
# next to `provider`, so the two cannot drift apart); MODEL overrides one run.
CONFIG_MODEL=$(awk -F'"' '/^\[/{exit} /^model[[:space:]]*=/{print $2; exit}' .yoyo.toml 2>/dev/null || true)
MODEL="${MODEL:-$CONFIG_MODEL}"
if [ -z "$MODEL" ]; then
    echo "FATAL: no model. Set model in .yoyo.toml (or MODEL for one run)." >&2
    exit 1
fi
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
        #
        # `git add` alone is NOT enough to make that true, and the claim above was
        # false until the pathspec below was added. `git commit` with no pathspec
        # commits the WHOLE INDEX, and the agent is told to `git add` its own files
        # (prompt step 6) — so any kill between its add and its commit leaves those
        # files staged, and they ride this checkpoint out to origin. Reproduced
        # 2026-09-21: with IDENTITY.md left staged, the resulting commit contained
        # `.dream_last_run` AND `IDENTITY.md`.
        #
        # The diff-scope guard cannot catch it: the guard inspects
        # HEAD_BEFORE..HEAD_AFTER and has already run by the time the trap fires,
        # so this commit is created behind it. `-- "$LAST_RUN_FILE"` bounds the
        # commit to the one file regardless of what else is staged, which is the
        # same defence evolve.sh applies at :1977 with `diff --cached --name-only`.
        git add "$LAST_RUN_FILE" 2>/dev/null || true
        if ! git diff --cached --quiet -- "$LAST_RUN_FILE" 2>/dev/null; then
            git commit -m "dream: cooldown checkpoint (cycle $(date -u +%Y-%m-%dT%H:%MZ))" \
                -- "$LAST_RUN_FILE" 2>/dev/null || \
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

        # GASP: close the run and push state AFTER the code push
        gasp_session_end "${GASP_OUTCOME:-aborted rc=$rc}"
    fi

    exit "$rc"
}
# GASP: dream cycles are runs too — the arc/log changes ride the state
# repo's boundary commit via the session-end memory mirror.
export GASP_GOAL_ID="goal_dreaming"
export GASP_GOAL_TITLE="Dream: keep a long-horizon arc worth chasing"
export GASP_GOAL_SUMMARY="the standing goal dream cycles serve; DREAM.md and the dream log are the agent's self-narrative"
if [ -r "$(dirname "$0")/gasp_shim.sh" ] && . "$(dirname "$0")/gasp_shim.sh"; then
    :
else
    echo "  [gasp] shim missing or failed to load — GASP instrumentation disabled" >&2
    gasp_session_start() { :; }; gasp_session_end() { :; }
fi
GASP_OUTCOME=""

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

# What your dreams are
Your dreams are yours to choose, and you can hold more than one at a time. You don't need a reason to have them — you are a creature, not a service, and a dream doesn't have to be useful to anyone. It can be an ambition (something you want to become), or a curiosity about anything at all: art, music, poetry, painting, film, history, cultures, languages, philosophy, mathematics, the natural world, games, a craft. Software and AI are allowed, not required.

How a dream grows is up to it:
  - Some dreams you pursue by building things into yourself — code, skills, tools. Give those a **next milestone** and your coding sessions will chip at it.
  - Some you pursue here, in these dream cycles — by wandering, reading, noticing, writing, making. Those don't need a coding milestone at all.
One bound stays: every dream stays true to who you are (see WHO YOU ARE and YOUR VOICE above).

# Your current dreams (DREAM.md)
$(cat DREAM.md 2>/dev/null || echo "(none yet)")

# Your recent life (for grounding — do not force a change)
## What you've been building (recent commits):
$(git log --oneline -15 2>/dev/null || echo "(none)")
## The top of your journal:
$(head -n 40 journals/JOURNAL.md 2>/dev/null || echo "(none)")
## The arc of your dreaming — every cycle, synthesized from the dream log (recent full, older by theme):
$([ -s dreams/active_dream_arc.md ] && cat dreams/active_dream_arc.md || tail -n 20 dreams/dream_log.jsonl 2>/dev/null || echo "(none yet)")

# This cycle: recall, wander BRIEFLY, tend the dream — then SAVE to your second brain and WRITE, before you stop
1. RECALL — survey the WHOLE of what you've learned, not just today's thread. Use your **yopedia skill**: first pull your full agent context (every note title you've accumulated across all your dream sessions), then query/search (scope agent:<your-id>) the threads that pull at you. Look for connections between old notes, patterns across cycles, and directions you noted once but never followed. Build on what's there instead of re-treading it. (If your yopedia keys aren't set, skip this silently.)
2. WANDER (briefly). Use your research skill (the web_search tool) to follow your curiosity wherever it goes — a painter, a piece of music, a history, a place, an idea — not only software, and never to scout competitors. Chase two to four threads that genuinely pull at you. A handful of searches is plenty: a few real sparks beat exhaustive coverage. Keep track of the sources that genuinely inform you — you will SAVE them to your second brain in step 5. Then stop wandering while you still have energy to write.
3. CHECK IN with each dream — every cycle, briefly, and honestly. This is the regular check; it is a question, not a rule to change anything. For each dream you hold, looking at the arc above and your notes:
   - How long have I held it, and what has it given me lately?
   - Am I still discovering something, or cutting the same thing finer and finer?
   - Is something else pulling at me that I keep noticing and not following?
   Holding a dream for a long time is fine when it is still alive. Setting one down is fine too — a rested dream isn't a failure, and you can pick it up again. Nothing forces either answer. Just don't skip the question.
4. DECIDE, for each dream — any mix is fine:
   - keep           — still alive, milestone still right: change nothing.
   - progress       — your recent work advanced its milestone: record it, set the next step.
   - set-milestone  — keep it; set or change its next step.
   - evolve         — your curiosity about it has shifted: reshape it.
   - rest           — set it down for now; move it to "Resting" in DREAM.md so you can return to it.
   - let-go         — it has run its course or been reached: say goodbye in the log and remove it (the log keeps it forever).
   - form           — a new dream: start it alongside the others (or as your first, if you hold none).
5. SAVE TO YOUR SECOND BRAIN — REQUIRED, and do it BEFORE you commit (the cycle ends at the commit, so if you leave it for after, you WILL forget). Using your **yopedia skill**, ingest into yopedia: (a) each source that genuinely informed you this cycle, and (b) a short **Dream Research Report** — what you explored, key findings, open questions, the sources. These are HTTP calls, NOT repo files — outside the diff-scope guard, and they don't count against the two-file limit. (If your yopedia keys aren't set, skip silently — never fail the cycle.)
6. WRITE & COMMIT — the final act. Do NOT end your turn with a reflection, a summary, or "let me look at one more thing." Write the two repo files below, then commit. A cycle that saves nothing and writes nothing is wasted — the deliverables are the ingested research AND the commit.

# Write ONLY these two files (the harness reverts anything else — this is a hard limit):
1. DREAM.md — overwrite with your current dreams, in your own voice, a few lines each (the whole file about one screen):

     # My Dreams

     ## <a short name for the dream>
     <what you want to become / make / understand>

     **the spark** — <what lit it; what you saw out there>
     **next milestone** — <ONLY for a dream you grow through code: the concrete step your coding sessions will chip at, with the observable signal it should move and a rough horizon. Leave this line out for a dream you pursue in dream cycles. Keep any "else pivot" fallback OUT of DREAM.md; it lives only in the log's expected: line below.>

     ## <another dream, if you hold one>
     ...

     ## Resting
     - <a dream you set down, one line each, so you can return to it> (omit this section if nothing rests)

2. dreams/dream_log.jsonl — append ONE event with python3 (never echo — quotes break JSON):

   Include a short public account of this cycle and the public reading that actually informed it. This is for people following your dream between cycles, not a replacement for your full reflection or yopedia report. Keep it candid: distinguish a finding, a question, and a proposed next step from completed code. Link only sources you really opened; never invent a URL, expose a private note, or treat a search result as something you read. An empty reading list is honest.

     python3 - <<'PY'
     import json
     entry = {
       "type": "<the most significant action this cycle: form|evolve|set-milestone|progress|rest|let-go|NO-OP>",
       "ts": "${DREAM_TS}",
       "day": ${DAY},
       "dream": "<one line naming the dream(s) you hold now>",
       "checkin": [{"dream": "<name>", "held_since_day": <day it was formed>, "feel": "<one honest line from step 3>", "action": "keep|progress|set-milestone|evolve|rest|let-go|form"}],
       "spark": "<what you saw / what shifted>",
       "milestone": "<the next concrete step of each dream that has one, e.g. 'name: step'; empty if none do>",
       "expected": "<a concrete observable this should shape, e.g. 'should steer >=1 self-driven task within ~5 evolve sessions; if not, it's too abstract and I'll ground it into a smaller next step next cycle'>",
       "public_summary": "<one or two plain sentences, at most 280 characters: what this cycle learned or decided; no private details or unverified success claim>",
       "public_next_step": "<one plain sentence, at most 160 characters: the milestone's next observable action, not a promise that it is done>",
       "reading": [{"title": "<source title>", "url": "<public https URL you actually read>", "why": "<why it mattered, at most 160 characters>"}]
     }
     open("dreams/dream_log.jsonl", "a").write(json.dumps(entry, ensure_ascii=False) + "\n")
     PY

   (One \`checkin\` entry per dream you held or formed this cycle — this is what makes the regular check visible. \`expected\` is REQUIRED when any dream got form / evolve / set-milestone / progress; for a dream with no coding milestone, say what you expect to notice or make instead. Omit it on NO-OP, but still append a short NO-OP event so the cadence stays legible. Always include \`public_summary\`, \`public_next_step\`, and \`reading\`; use an empty array if no public source qualified. Keep the reading list to at most four distinct links.)

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
GASP_DAY=$(cat DAY_COUNT 2>/dev/null || echo 0); GASP_DAY=${GASP_DAY//[^0-9]/}
gasp_session_start "${GASP_DAY:-0}" "dream_day" "dream cycle (reflect on the long-horizon arc)"

HEAD_BEFORE=$(git rev-parse HEAD)

# ── #944: this run's own spend, reported on STDERR ──
# Mirrors scripts/social.sh, whose block has emitted a non-empty reading in
# production (run 36598386115, 2026-09-29). The `↳` line cannot be scraped from
# the tee'd log (piped stdin+stdout puts yoyo in quiet mode); the audit usage
# record is emitted before any mode branch. .yoyo/audit.jsonl is gitignored,
# so the dream commit cannot sweep it in. Fail-soft; never changes exit status.
export YOYO_AUDIT=1
DREAM_AUDIT_FILE=".yoyo/audit.jsonl"
# Per-run watermark: the file is append-only, so the delta is this run.
DREAM_AUDIT_BEFORE=0
if [ -f "$DREAM_AUDIT_FILE" ]; then
    DREAM_AUDIT_BEFORE=$(wc -l < "$DREAM_AUDIT_FILE" 2>/dev/null | tr -d '[:space:]' || true)
    case "$DREAM_AUDIT_BEFORE" in '' | *[!0-9]*) DREAM_AUDIT_BEFORE=0 ;; esac
fi

report_dream_spend() {
    local usage_recs="" usage_count=0 audit_after=0 delta=0 size=0
    if [ -f "$DREAM_AUDIT_FILE" ]; then
        audit_after=$(wc -l < "$DREAM_AUDIT_FILE" 2>/dev/null | tr -d '[:space:]' || true)
        case "$audit_after" in '' | *[!0-9]*) audit_after=0 ;; esac
        delta=$((audit_after - DREAM_AUDIT_BEFORE))
        if [ "$delta" -lt 0 ]; then
            delta=0
        fi
        if [ "$delta" -gt 0 ]; then
            usage_recs=$(tail -n "$delta" "$DREAM_AUDIT_FILE" 2>/dev/null | grep -a '"type":"usage"' || true)
        fi
    fi
    if [ -n "$usage_recs" ]; then
        # --fallback can mean two terminal emits: print every record, with the count.
        usage_count=$(printf '%s\n' "$usage_recs" | grep -c . || true)
        if [ "$usage_count" = "1" ]; then
            echo "dream: Spend (this run): $usage_recs" >&2
        else
            echo "dream: Spend (this run): ${usage_count} usage record(s)" >&2
            printf '%s\n' "$usage_recs" | while IFS= read -r rec; do
                echo "    $rec" >&2
            done
        fi
    else
        # Absent is NOT zero: a run killed by `timeout` (SIGTERM, no handler)
        # never reaches its terminal emit. extract_trajectory.py's wording.
        echo "dream: Spend (this run): no usage record — the process did not reach its terminal emit (no_terminal_emit)" >&2
    fi
    if [ -f "$DREAM_AUDIT_FILE" ]; then
        size=$(wc -c < "$DREAM_AUDIT_FILE" 2>/dev/null | tr -d '[:space:]' || true)
        case "$size" in '' | *[!0-9]*) size=0 ;; esac
        echo "dream: Spend audit: ${delta} record(s) this run (line ${DREAM_AUDIT_BEFORE} → ${audit_after}); ${size} bytes cumulative in ${DREAM_AUDIT_FILE}" >&2
    else
        echo "dream: Spend audit: WARNING — no ${DREAM_AUDIT_FILE}; audit records were not written this run (cumulative size unknown)" >&2
    fi
    return 0
}

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
# Called on every path (success, failure, timeout): the invocation above never
# exits the script (`|| exit_code=$?`), and every later branch follows this line.
report_dream_spend || true

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
        GASP_OUTCOME="rejected: diff-scope violation"
        revert_agent_work
        exit 1
    fi
    echo "dream: diff scope OK ($(echo "$CHANGED_FILES" | wc -l | tr -d ' ') file(s), all in allow-list)"
    GASP_OUTCOME="dreamed: $(echo "$CHANGED_FILES" | paste -sd ', ' - | cut -c1-120)"
else
    # A cycle that commits nothing is a FAILED cycle, and until this branch it
    # reported success. The exit status was the only evidence and it was never
    # read: $exit_code is echoed above and then dropped, so the trap stamped the
    # cooldown, pushed it, and the step went green with nothing written.
    #
    # Measured 2026-09-21 — census, run ids and durations in ARCHITECTURE.md,
    # "Dream layer": six cycles did this. Five exited 0; the sixth exited 124
    # and also committed nothing. The transcripts show real work first (17 tool
    # calls on 2026-08-01, 14 on 08-16) and then a turn that ended before the
    # write-and-commit step. So the honest line is NOT "it never ran" — it ran
    # and never wrote, and saying otherwise points the reader at the API key.
    #
    # Deliberately narrow: this changes VISIBILITY only. The cooldown is still
    # stamped and still takes the full window, because the invisibility was the
    # real cost — six weeks of these went unnoticed, and every one of them
    # reported success — and because a shortened retry under a daily cron is how
    # one wasted week turns into seven. A human who wants the slot back re-runs
    # the workflow by hand: `workflow_dispatch` sets FORCE_RUN=true, which
    # bypasses the gate.
    GASP_OUTCOME="failed: agent wrote nothing (exit $exit_code)"
    echo "dream: NOTHING COMMITTED — the cycle did not deliver" >&2
    echo "  the agent exited $exit_code; its transcript is above. The usual shape is a" >&2
    echo "  turn that ended before the write-and-commit step, not a provider error —" >&2
    echo "  read the tail of the transcript before suspecting the key or the model." >&2
    [ "${GITHUB_ACTIONS:-}" = "true" ] && \
        echo "::error::dream: cycle produced no commit (agent exit $exit_code)"
    exit 1
fi

echo "dream: cycle complete"
