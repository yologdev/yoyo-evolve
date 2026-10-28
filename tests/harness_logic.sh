#!/usr/bin/env bash
# Structural tests for the harness and its cross-file contracts — the pure
# decision logic in scripts/evolve.sh, plus the couplings it cannot express
# alone (evolve.yml budgets, gasp_shim.sh vs src/gasp.rs, ci.yml feature gates).
#
# The harness grew to ~3k lines of interlocking gates whose only automated
# checks were `bash -n` and the heredoc linter — every real bug this week was
# caught by ad-hoc review, including one that would have aborted every session
# at startup. These pin the pieces that are pure functions of their inputs.
# Run: bash tests/harness_logic.sh   (CI runs it via ci.yml)
set -uo pipefail
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
bad()  { FAIL=$((FAIL+1)); printf '  FAIL %s — %s\n' "$1" "$2"; }
check() { [ "$2" = "$3" ] && ok "$1" || bad "$1" "expected '$3', got '$2'"; }
# Extractions that yield nothing must FAIL, not silently compare ""=="" or be
# coerced to 0 in arithmetic — three checks passed vacuously before this
# (mutation-verified: renaming BFIX_TIMEOUT survived the old suite).
require() { [ -n "$2" ] && return 0; bad "$1" "extraction produced nothing — pattern no longer matches evolve.sh"; return 1; }

SCRIPT="$(cd "$(dirname "$0")/.." && pwd)/scripts/evolve.sh"
# dream.sh is the sibling harness script and, until 2026-09-22, had NO automated
# coverage of any kind — not even `bash -n`, which ci.yml runs for neither file.
# It is also unprotected (evolve.sh's protected list does not name it), so the
# evolve agent can rewrite it. Two safety-relevant bugs were fixed in it that
# evening, one of which was a documented way around the diff-scope guard its own
# header calls "the sole safety belt". 96 assertions on evolve.sh and zero on
# dream.sh was the asymmetry worth closing.
DREAM="$(cd "$(dirname "$0")/.." && pwd)/scripts/dream.sh"

# ── session budget helper ────────────────────────────────────────────────
budget_left() { # $1=YOYO_SESSION_BUDGET_SECS  $2=JOB_DEADLINE_EPOCH
    ( set -uo pipefail
      SESSION_T0=$(date +%s)
      YOYO_SESSION_BUDGET_SECS="${1-}"; JOB_DEADLINE_EPOCH="${2-}"
      eval "$(awk '/^# Guard: a non-numeric value/,/^}/' "$SCRIPT")"
      session_secs_left ) 2>/dev/null | tail -1
}
check "budget: unset → gates disabled"      "$(budget_left '' '')"        "999999"
check "budget: numeric honored"             "$(budget_left 500 '')"       "500"
# 2699..2700 for the same two-`date`-reads race the checks below document
# (observed 2026-09-27: got 2699).
L=$(budget_left '45m' '')
[ "$L" -ge 2699 ] 2>/dev/null && [ "$L" -le 2700 ] && ok "budget: non-numeric → 2700 fallback" \
    || bad "budget: non-numeric → 2700 fallback" "expected 2699..2700, got '$L'"
L=$(budget_left 7200 "$(( $(date +%s) + 2400 ))")
[ "$L" -ge 1190 ] && [ "$L" -le 1200 ] && ok "budget: job deadline clamps (2400-1200 margin)" \
    || bad "budget: job deadline clamps" "expected ~1200, got $L"
L=$(budget_left 300 "$(( $(date +%s) + 99999 ))")
# 299..300, not exactly 300: budget_left samples `date` once for SESSION_T0 and
# session_secs_left samples it again, so a second boundary between the two
# reads yields 299. The property is "the 300s budget beats the 99999s deadline",
# and a one-second race is not a counter-example to it (CI run 35035989877
# failed on exactly this, 2026-09-15; the clamp check above already tolerates
# the same race with a 1190..1200 band).
[ "$L" -ge 299 ] && [ "$L" -le 300 ] && ok "budget: smaller of the two wins (300s budget beats 99999s deadline)" \
    || bad "budget: smaller of the two wins" "expected 299..300, got '$L'"

# ── gate thresholds vs the timeouts they must cover ──────────────────────
val() { grep -oE "^[[:space:]]*$1=[0-9]+" "$SCRIPT" | head -1 | cut -d= -f2; }
gate() { grep -oE "session_secs_left\)\" -lt $1 " "$SCRIPT" >/dev/null && echo yes || echo no; }
IMPL=$(val IMPL_TIMEOUT); EVAL=$(val EVAL_TIMEOUT); BFIX=$(val BFIX_TIMEOUT); FIXT=$(val FIX_TIMEOUT)
for _n in IMPL:$IMPL EVAL:$EVAL BFIX:$BFIX FIXT:$FIXT; do
    require "constant ${_n%%:*} extracted" "${_n#*:}" && ok "constant ${_n%%:*}=${_n#*:}"
done
[ "$IMPL" -eq 1800 ] && ok "IMPL_TIMEOUT is 1800" || bad "IMPL_TIMEOUT" "got $IMPL"
# task gate must cover impl + one eval pass + a verify cycle
[ "$(( IMPL + EVAL + 750 ))" -le 3750 ] && ok "task gate 3750 covers impl+eval+verify" \
    || bad "task gate" "3750 < $(( IMPL + EVAL + 750 ))"
[ "$(( BFIX + 750 ))" -le 1650 ] && ok "fix gates 1650 cover fix+verify" \
    || bad "fix gates" "1650 < $(( BFIX + 750 ))"
check "task gate present"  "$(gate 3750)" "yes"
check "fix gates present"  "$(gate 1650)" "yes"

# ── coupled numbers: prompts must agree with the code ────────────────────
grep -q "Impl timeout: ${IMPL}s/task" "$SCRIPT" && ok "banner matches IMPL_TIMEOUT" \
    || bad "banner" "banner disagrees with IMPL_TIMEOUT=$IMPL"
grep -q "completable in $(( IMPL / 60 )) minutes" "$SCRIPT" && ok "planner minutes match IMPL_TIMEOUT" \
    || bad "planner minutes" "prompt disagrees with IMPL_TIMEOUT=$IMPL"
grep -q "You have $(( EVAL / 60 )) minutes" "$SCRIPT" && ok "evaluator prompt matches EVAL_TIMEOUT" \
    || bad "evaluator prompt" "prompt disagrees with EVAL_TIMEOUT=$EVAL"
SLOTS=$(grep -oE "You have ([0-9]+) task slots" "$SCRIPT" | grep -oE '[0-9]+' | head -1)
CAP=$(grep -oE '"\$TASK_NUM" -gt ([0-9]+)' "$SCRIPT" | grep -oE '[0-9]+' | head -1)
require "planner slots greppable" "$SLOTS" && require "harness cap greppable" "$CAP" \
    && check "planner slots == harness cap" "$SLOTS" "$CAP"

# ── ordering invariant: NO function called above its definition ──────────
# Generic scan over every function (was: two hardcoded names, which mutation
# testing showed missed calls in other forms and every other function).
# Top-level calls only: a call inside a function body runs at invocation time,
# so `grep -n "^[[:space:]]\{0,4\}fn"` approximates "not deeply nested".
# A call INSIDE a function body runs at invocation time, so only top-level
# call sites can execute before a definition. Two passes: map function-body
# line ranges, then flag any call outside them that precedes its own def.
BAD_ORDER=$(awk '
    FNR==NR {
        if ($0 ~ /^[a-z_][a-z0-9_]*\(\) \{/) { name=$0; sub(/\(\).*/,"",name); def[name]=FNR; inbody=1 }
        else if (inbody && $0 ~ /^\}/) { inbody=0 }
        body[FNR]=inbody
        next
    }
    body[FNR] { next }                       # skip calls inside function bodies
    /^[[:space:]]*#/ { next }                # skip comments
    {
        line=$0; sub(/^[[:space:]]+/,"",line)
        for (n in def) {
            if (line ~ ("^" n "([[:space:]]|$|;)") && FNR < def[n] && !(n in seen)) {
                seen[n]=1; out = out " " n "(def@" def[n] ",use@" FNR ")"
            }
        }
    }
    END { print (out == "" ? "none" : out) }
' "$SCRIPT" "$SCRIPT")
check "no function called above its definition" "${BAD_ORDER:-none}" "none"

# ── failed-test name extraction: exercise evolve.sh's OWN regex ──────────
# (was: a retyped copy — changing the real regex left this green.)
RE=$(grep -oE "grep -oE '\^test [^']*'" "$SCRIPT" | head -1 | sed "s/^grep -oE '//; s/'$//")
if require "innocence regex extracted from evolve.sh" "$RE"; then
    NAMES=$(printf 'test a::b::c_test ... FAILED\ntest x::ok_one ... ok\ntest d::e ... FAILED\n' \
        | grep -oE "$RE" | awk '{print $2}' | tr '\n' ' ')
    check "innocence regex parses FAILED names" "$NAMES" "a::b::c_test d::e "
fi

# ── scope-review checklist: prompt items == parser items (#712) ──────────
# Two hand-maintained lists of the same four names in one file; if the prompt
# gains an item the parser doesn't require, the evaluator can skip it silently
# — the exact failure the contract exists to prevent.
PROMPT_ITEMS=$(grep -oE '^Checked: [a-z_]+:' "$SCRIPT" | sed 's/^Checked: //; s/:$//' | sort -u | tr '\n' ' ')
# Anchor on the loop's own syntax, not on a member name: hardcoding
# `intent_alignment` made a rename of THAT item report "pattern no longer
# matches" instead of the real drift, and hid any item placed before it.
PARSER_ITEMS=$(grep -oE '"[a-z_]+:(PASS\|FAIL(\|N/A)?)"' "$SCRIPT" \
    | sed 's/^"//; s/:.*$//' | sort -u | tr '\n' ' ')
require "checklist items found in prompt" "$PROMPT_ITEMS" \
    && require "checklist items found in parser" "$PARSER_ITEMS" \
    && check "eval checklist: prompt items == parser items" "$PROMPT_ITEMS" "$PARSER_ITEMS"

# ── eval checklist BEHAVIOR: drive evolve.sh's own regexes over fixtures ──
# The drift guard pins that the two NAME lists agree; it said nothing about what
# the parser DOES. Behaviour testing found four defects at once: the unfilled
# template scored 4/4, all-N/A scored 4/4, indented lines scored 0/4, and an
# incomplete list discarded a stated FAIL.
CK_PRE=$(grep -oE "_CK_PRE='[^']+'" "$SCRIPT" | head -1 | sed "s/^_CK_PRE='//; s/'$//")
CK_SPECS=$(grep -oE '"[a-z_]+:(PASS\|FAIL(\|N/A)?)"' "$SCRIPT" | tr -d '"' | tr '\n' ' ')
# Lift the WHOLE match expression out of evolve.sh — reconstructing it here made
# the fixtures a tautology (a mutant that weakened evolve.sh's regex survived,
# because the test kept using its own retyped copy).
CK_PAT=$(grep -oE 'grep -qiE "\$\{_CK_PRE\}\$\{_item\}[^"]*"' "$SCRIPT" | head -1 \
    | sed 's/^grep -qiE "//; s/"$//')
CK_FAILPAT=$(grep -oE 'grep -qiE "\$\{_CK_PRE\}\[a-z_\]\+[^"]*"' "$SCRIPT" | head -1 \
    | sed 's/^grep -qiE "//; s/"$//')
if require "checklist line prefix extracted" "$CK_PRE" \
    && require "checklist item specs extracted" "$CK_SPECS" \
    && require "checklist match pattern extracted" "$CK_PAT" \
    && require "checklist FAIL pattern extracted" "$CK_FAILPAT"; then
    CKF=$(mktemp)
    # Expand evolve.sh's own pattern by binding its three variables.
    ck_miss() { local M="" S I T RE
        for S in $CK_SPECS; do I="${S%%:*}"; T="${S#*:}"
            RE=${CK_PAT//'${_CK_PRE}'/$CK_PRE}; RE=${RE//'${_item}'/$I}; RE=${RE//'${_toks}'/$T}
            grep -qiE "$RE" "$CKF" || M="${M:+$M,}$I"; done; printf '%s' "${M:-none}"; }
    ck_ovr() { local RE=${CK_FAILPAT//'${_CK_PRE}'/$CK_PRE}
        grep -qiE "$RE" "$CKF" && echo FAIL || echo none; }
    ck_fix() { printf '%s\n' "$@" > "$CKF"; }

    ck_fix "Verdict: PASS" "Checked: intent_alignment: PASS: diff wires the helper in" \
        "Checked: forgotten_touchpoints: PASS: new fn called at repl.rs:812" \
        "Checked: doc_sync: N/A: no behavior change at all" \
        "Checked: product_surface: N/A: evolve-kind, no user surface"
    check "eval checklist: fully answered → nothing missing" "$(ck_miss)" "none"
    check "eval checklist: clean run does not trip the override" "$(ck_ovr)" "none"

    ck_fix "Verdict: PASS" "Checked: intent_alignment: PASS|FAIL: [specific, one line]" \
        "Checked: forgotten_touchpoints: PASS|FAIL: [...]" \
        "Checked: doc_sync: PASS|FAIL: [...]" "Checked: product_surface: PASS|FAIL: [...]"
    check "eval checklist: unfilled template counts as UNANSWERED" \
        "$(ck_miss)" "intent_alignment,forgotten_touchpoints,doc_sync,product_surface"

    ck_fix "Verdict: PASS" "Checked: intent_alignment: N/A: nothing to say here" \
        "Checked: forgotten_touchpoints: N/A: nothing to say here" \
        "Checked: doc_sync: N/A: no behavior change here" \
        "Checked: product_surface: N/A: no product surface here"
    check "eval checklist: N/A is rejected where it cannot apply" \
        "$(ck_miss)" "intent_alignment,forgotten_touchpoints"

    ck_fix "Verdict: PASS" "  Checked: intent_alignment: PASS: matches the stated task" \
        "  Checked: forgotten_touchpoints: PASS: helper has two callers" \
        "  Checked: doc_sync: PASS: CLAUDE.md updated in commit" \
        "  Checked: product_surface: N/A: no product surface here"
    check "eval checklist: indented lines still count" "$(ck_miss)" "none"

    ck_fix "Verdict: PASS" "Checked: intent_alignment: PASS: matches the stated task" \
        "Checked: forgotten_touchpoints: FAIL: new const has no reader in diff" \
        "**Checked: doc_sync: N/A: no behavior change here**" \
        "Checked: product_surface: N/A: no product surface here"
    check "eval checklist: FAIL survives a bolded sibling line" "$(ck_ovr)" "FAIL"

    ck_fix "Verdict: PASS" "Reason: the test would FAIL without this" \
        "Checked: intent_alignment: PASS: matches the stated task" \
        "Checked: forgotten_touchpoints: PASS: docs mention the FAIL path" \
        "Checked: doc_sync: PASS: CLAUDE.md updated in commit" \
        "Checked: product_surface: N/A: no product surface here"
    check "eval checklist: the word FAIL in prose does not trip the override" "$(ck_ovr)" "none"

    ck_fix "Verdict: PASS" "Checked: intent_alignment: PASS" "Checked: forgotten_touchpoints: PASS" \
        "Checked: doc_sync: PASS" "Checked: product_surface: PASS"
    check "eval checklist: a verdict with no reason is UNANSWERED" \
        "$(ck_miss)" "intent_alignment,forgotten_touchpoints,doc_sync,product_surface"
    rm -f "$CKF"
fi

# The FAIL override must NOT be nested under the completeness branch: an omitted
# line would otherwise neutralise a FAIL the evaluator actually wrote.
OVR_LN=$(grep -n '\[a-z_\]+:\[\[:space:\]\]\*FAIL:' "$SCRIPT" | head -1 | cut -d: -f1)
LOOP_LN=$(grep -n 'for _spec in' "$SCRIPT" | head -1 | cut -d: -f1)
if require "override line located" "$OVR_LN" && require "item loop located" "$LOOP_LN"; then
    A=$(sed -n "${OVR_LN}p" "$SCRIPT" | sed 's/[^ ].*//' | wc -c)
    B=$(sed -n "${LOOP_LN}p" "$SCRIPT" | sed 's/[^ ].*//' | wc -c)
    [ "$A" -le "$B" ] && ok "FAIL override is unconditional (not nested under completeness)" \
        || bad "FAIL override nesting" "indented deeper than the item loop — an incomplete checklist would discard a stated FAIL"
fi

# ── cron cadence: evolve.yml owns it, and nothing restates it ────────────
# The doc-sync half was dropped on 2026-09-16 — CLAUDE.md deliberately no
# longer names the schedule, because a value copied into prose goes stale
# silently while the config keeps working. What survives is config-vs-config:
# the job ceiling must fit the SMALLEST gap between runs, or a long run's
# successor queues and is then cancelled, dropping a whole slot. Both cron
# shapes are read — `0 */N * * *` and `0 h1,h2,... * * *`. Neither matching
# is a REFUSAL, never a pass.
WF="$(cd "$(dirname "$0")/.." && pwd)/.github/workflows/evolve.yml"
TMIN=$(grep -oE 'timeout-minutes: [0-9]+' "$WF" | grep -oE '[0-9]+' | head -1)
CRON_SPEC=$(grep -oE "cron: '[^']+'" "$WF" | head -1 | sed -E "s/^cron: '(.*)'$/\1/")
CRON_H=$(echo "$CRON_SPEC" | awk '{
    if ($2 ~ /^\*\/[0-9]+$/) { sub(/^\*\//,"",$2); print $2; exit }
    if ($2 ~ /^[0-9]+(,[0-9]+)+$/) {
        n = split($2, h, ","); for (a = 1; a <= n; a++) for (b = a+1; b <= n; b++) if (h[a] > h[b]) { t = h[a]; h[a] = h[b]; h[b] = t }
        m = 24 - h[n] + h[1]; for (a = 2; a <= n; a++) { g = h[a] - h[a-1]; if (g < m) m = g }
        print m; exit
    }
}')
if require "evolve cron gap extracted (got '$CRON_SPEC')" "$CRON_H"; then
    [ "$TMIN" -le "$(( CRON_H * 60 ))" ] \
        && ok "job ceiling ${TMIN}m fits inside the ${CRON_H}h cron gap" \
        || bad "job ceiling vs cron gap" "timeout-minutes=$TMIN exceeds the $(( CRON_H * 60 ))m gap — the next run queues and is then cancelled"
fi
grep -qE '^# scripts/evolve\.sh .*(hourly|[0-9]+h gap)' "$SCRIPT" \
    && bad "evolve.sh header cadence" "header restates a cadence evolve.yml owns" \
    || ok "evolve.sh header does not restate the cron cadence"

# ── cross-file coupling: evolve.sh gates vs evolve.yml budgets ───────────
# The `2700` bug (an attempt budget below the task gate, so that attempt could
# never run a task) was exactly this class and nothing covered it.
WF="$(cd "$(dirname "$0")/.." && pwd)/.github/workflows/evolve.yml"
APH=$(grep -oE "TIMEOUT:-[0-9]+" "$SCRIPT" | grep -oE '[0-9]+' | head -1)
GATE=$(grep -oE 'session_secs_left\)" -lt 3750' "$SCRIPT" >/dev/null && echo 3750 || echo "")
if require "A-phase budget extracted" "$APH" && require "task gate extracted" "$GATE"; then
    NEED=$(( APH + GATE ))
    for b in $(grep -oE "YOYO_SESSION_BUDGET_SECS: '[0-9]+'" "$WF" | grep -oE '[0-9]+'); do
        [ "$b" -ge "$NEED" ] && ok "attempt budget ${b}s clears A-phases+one task (${NEED}s)" \
            || bad "attempt budget ${b}s" "below ${NEED}s — that attempt can never run a task"
    done
fi
TMIN=$(grep -oE 'timeout-minutes: [0-9]+' "$WF" | grep -oE '[0-9]+' | head -1)
DEADMIN=$(grep -oE '\+ [0-9]+\*60' "$WF" | grep -oE '^[0-9]+|[0-9]+' | head -1)
require "job timeout-minutes extracted" "$TMIN" && require "deadline minutes extracted" "$DEADMIN" \
    && check "JOB_DEADLINE_EPOCH minutes == timeout-minutes" "$DEADMIN" "$TMIN"

# ── GASP: exactly one writer to the state store (#683) ───────────────────
# gasp-emit holds a run open from session-start to session-end, a GASP repo is
# single-writer, and GaspRecorder::open MUTATES it: past the 600s lease TTL an
# in-process recorder steals the lease, writes run.finished{interrupted} over
# the sidecar's live run, and the sidecar's session-end then dies with "no run
# is open" — losing the whole session's record (measured, Day 165).
#
# NOT a name-equality check. The obvious test — "the shim exports exactly what
# src/gasp.rs reads" — encodes "the bridge must be live", which is false while
# the sidecar writes: it would PASS on the two-writer hazard and FAIL on the
# correct fix. So follow whichever half is the writer, and let the invariant
# flip automatically when gasp-emit is retired.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GASP_SHIM="$ROOT/scripts/gasp_shim.sh"
GASP_RS="$ROOT/src/gasp.rs"
RS_VARS=$(grep -oE 'const [A-Z_]+: &str = "YOYO_GASP_[A-Z_]+"' "$GASP_RS" \
    | grep -oE 'YOYO_GASP_[A-Z_]+' | sort -u | tr '\n' ' ')
SHIM_EXPORTS=$(grep -oE '^[[:space:]]*export YOYO_GASP_[A-Z_]+=' "$GASP_SHIM" \
    | grep -oE 'YOYO_GASP_[A-Z_]+' | sort -u | tr '\n' ' ')
SHIM_UNSETS=$(grep -oE '^[[:space:]]*unset (YOYO_GASP_[A-Z_]+[[:space:]]*)+' "$GASP_SHIM" \
    | sed 's/^[[:space:]]*unset //' | tr ' ' '\n' | grep -v '^$' | sort -u | tr '\n' ' ')
SIDECAR_OPENS=$(grep -cE '^[[:space:]]*_gasp_emit session-start' "$GASP_SHIM")
if require "gasp env vars declared in src/gasp.rs" "$RS_VARS"; then
    if [ "$SIDECAR_OPENS" -gt 0 ]; then
        check "gasp: sidecar owns the store — in-process bridge stays unexported" \
            "${SHIM_EXPORTS:-none}" "none"
    else
        check "gasp: sidecar retired — bridge exports exactly what src/gasp.rs reads" \
            "$SHIM_EXPORTS" "$RS_VARS"
        check "gasp: _gasp_off clears exactly what gasp_session_start exported" \
            "$SHIM_UNSETS" "$SHIM_EXPORTS"
    fi
fi

# ── the gasp feature is default-off, so CI is the ONLY thing that builds it ──
# yoyo's own `cargo build && cargo test` and evolve.sh's Step 1 both compile
# src/gasp.rs into nothing, and evolve.sh deliberately does not build the
# feature. Dropping the CI steps would silently un-verify the whole module —
# Day 164 already lost a session to yoagent-state failing to resolve under it.
# The feature name is lifted from the cfg gate so a rename must move in lockstep.
GASP_CI="$ROOT/.github/workflows/ci.yml"
GASP_CARGO="$ROOT/Cargo.toml"
GASP_FEAT=$(grep -B1 '^mod gasp;' "$ROOT/src/main.rs" \
    | grep -oE 'feature = "[a-z_]+"' | grep -oE '"[a-z_]+"' | tr -d '"')
if require "gasp cfg-gate feature name extracted from main.rs" "$GASP_FEAT"; then
    check "gasp: feature declared in Cargo.toml" \
        "$(grep -cE "^${GASP_FEAT} = \[" "$GASP_CARGO")" "1"
    check "gasp: feature stays default-off" \
        "$(grep -cE '^default = \[\]' "$GASP_CARGO")" "1"
    check "gasp: CI runs the test suite with it" \
        "$(grep -cE "cargo test .*--features ${GASP_FEAT}" "$GASP_CI")" "1"
    check "gasp: CI runs clippy with it" \
        "$(grep -cE "cargo clippy .*--features ${GASP_FEAT}" "$GASP_CI")" "1"
fi

# ── eval-fix loop: the no-progress detector ──────────────────────────────
# Day 166 ground all 10 eval attempts against a byte-identical empty diff in a
# 98-minute session, because the task was blocked upstream (yoagent#111) and no
# retry could move it. A fix attempt that changes nothing leaves the evaluator
# the same input, so the same verdict is guaranteed.
NP_MAX=$(grep -oE '^[[:space:]]*MAX_NO_PROGRESS_FIXES=[0-9]+' "$SCRIPT" | grep -oE '[0-9]+$' | head -1)
EV_MAX=$(grep -oE '^[[:space:]]*MAX_EVAL_ATTEMPTS=[0-9]+' "$SCRIPT" | grep -oE '[0-9]+$' | head -1)
if require "MAX_NO_PROGRESS_FIXES extracted" "$NP_MAX" \
   && require "MAX_EVAL_ATTEMPTS extracted" "$EV_MAX"; then
    check "no-progress guard fires before the attempt cap" \
        "$([ "$NP_MAX" -lt "$EV_MAX" ] && echo yes || echo no)" "yes"
    # >1 on purpose: one no-op can be a transient API error or timeout, and
    # abandoning on that would revert work that a retry would have finished.
    check "no-progress guard tolerates a single transient no-op" \
        "$([ "$NP_MAX" -ge 2 ] && echo yes || echo no)" "yes"
fi

# ── the no-progress COUNTER, not just its constants ──────────────────────
# The two constant checks above pin that the guard *could* fire; they say
# nothing about the increment, the reset, the keep-vs-revert decision, or the
# unmeasurable case. Mutation-measured: five mutants of this block (no
# accumulation, no reset, -ge -> -gt, dropped TASK_OK=false, inverted
# fingerprint compare) ALL survive the constant + fingerprint checks alone.
#
# Lift the block from its `if` down to the `fi` at the SAME indentation — a
# naive /^[[:space:]]*fi$/ range stops at the first inner `fi` and drops the
# reset branch, which is the one nothing else covers.
NP_BLOCK=$(awk '
    !inb && /^[[:space:]]*if ! FIX_STATE_AFTER=/ {
        inb=1; ind=$0; sub(/[^ ].*/, "", ind); print; next }
    inb { print; if ($0 == ind "fi") exit }
' "$SCRIPT")
# $1=counter in  $2=fp before  $3=fp after ("FAIL" = git could not answer)
# $4=diff-empty? (yes|no)  ->  "<counter out> <TASK_OK> <BUDGET_UNVERIFIED>"
np_step() {
    ( set -uo pipefail
      _FP_NOW="$3"; _DIFF_EMPTY="$4"   # captured OUT: "$3" inside a stub is the STUB's $3
      work_state_fingerprint() { [ "$_FP_NOW" = FAIL ] && return 1; printf '%s' "$_FP_NOW"; }
      # The block asks task_landed_changes (yes | no | cannot answer), not git directly.
      task_landed_changes() { case "$_DIFF_EMPTY" in yes) printf 'no';; no) printf 'yes';; *) return 1;; esac; }
      # Intercept only the one git call the block makes; anything else is a bug.
      git() { case "$*" in "diff --quiet"*) [ "$_DIFF_EMPTY" = yes ] && return 0 || return 1;;
                           *) return 0;; esac; }
      FIX_STATE_BEFORE="$2"; NO_PROGRESS_FIXES="$1"; NO_PROGRESS_CAUSES=""
      MAX_NO_PROGRESS_FIXES="$NP_MAX"; MAX_EVAL_ATTEMPTS="$EV_MAX"
      EVAL_ATTEMPT=1; TASK_NUM=0; EVAL_REASON=stub; EVAL_FEEDBACK=stub
      FIX_NOOP_CAUSE=stub; PRE_TASK_SHA=deadbeef; TASK_OK=true
      REVERT_REASON=""; REVERT_DETAILS=""; BUDGET_UNVERIFIED=""
      UNVERIFIED_REASON=""; UNVERIFIED_FEEDBACK=""
      while :; do eval "$NP_BLOCK"; break; done   # `break` needs a loop to leave
      printf '%s %s %s\n' "$NO_PROGRESS_FIXES" "$TASK_OK" "${BUDGET_UNVERIFIED:-none}"
    ) 2>/dev/null | tail -1
}
if require "no-progress block extracted" "$NP_BLOCK" && [ -n "$NP_MAX" ]; then
    check "no-progress: first no-op counts but does not abandon" \
        "$(np_step 0 same same yes)" "1 true none"
    # Empty diff — the Day 166 case. Nothing to keep, so reverting is free.
    check "no-progress: at threshold with an EMPTY diff, revert" \
        "$(np_step $(( NP_MAX - 1 )) same same yes)" "$NP_MAX false none"
    # Could not measure the diff: keep the work like a non-empty diff (never
    # reset --hard on a guess) — the post-loop check refuses to PROMOTE it.
    check "no-progress: at threshold with an UNMEASURABLE diff, keep it UNVERIFIED" \
        "$(np_step $(( NP_MAX - 1 )) same same fail)" "$NP_MAX true no_progress"
    # Non-empty diff — green work exists. Must NOT convert the harness's
    # fail-open accept into a git reset --hard.
    check "no-progress: at threshold with a NON-EMPTY diff, keep it UNVERIFIED" \
        "$(np_step $(( NP_MAX - 1 )) same same no)" "$NP_MAX true no_progress"
    # The reset is what makes the guard safe for a slow-but-progressing task.
    check "no-progress: any real change resets the streak" \
        "$(np_step $(( NP_MAX - 1 )) before after yes)" "0 true none"
    # Third value: git could not answer. Neither progress nor no-progress.
    check "no-progress: an unmeasurable attempt leaves the counter alone" \
        "$(np_step $(( NP_MAX - 1 )) same FAIL yes)" "$(( NP_MAX - 1 )) true none"
fi

# The fingerprint must be content-sensitive, not filename-sensitive: a fix that
# rewrites the same file to different bytes IS progress and must reset the
# counter. Extracted from evolve.sh and driven against a throwaway git repo —
# `eval` of a *function definition* is safe here (status 0); see the note on
# budget_left. Never run against the real repo.
FP_FN=$(awk '/^work_state_fingerprint\(\) \{/,/^\}/' "$SCRIPT")
if require "work_state_fingerprint extracted" "$FP_FN"; then
    FP_R=$( (
        eval "$FP_FN"
        T=$(mktemp -d) || exit 1
        cd "$T" || exit 1
        # Neutralise ambient git config: a dev machine's core.excludesFile
        # can hide the fixture files and red the suite for reasons that have
        # nothing to do with the code under test.
        export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
        git init -q . >/dev/null 2>&1 || exit 1
        git config user.email t@t; git config user.name t; git config commit.gpgsign false
        echo one > f.txt; git add -A; git commit -qm init >/dev/null 2>&1
        a=$(work_state_fingerprint); b=$(work_state_fingerprint)
        echo two > f.txt
        c=$(work_state_fingerprint)
        # The case that actually matters, and the only one that isolates content
        # sensitivity: BOTH states show the identical ` M f.txt` in porcelain, so
        # a filename-only fingerprint cannot tell them apart. A fix agent
        # re-editing one file between attempts looks exactly like this.
        echo three > f.txt
        c2=$(work_state_fingerprint)
        git add -A; git commit -qm second >/dev/null 2>&1
        d=$(work_state_fingerprint)
        echo x > untracked.txt
        e=$(work_state_fingerprint)
        cd /; rm -rf "$T"
        [ "$a" = "$b" ]   || { echo unstable;        exit 0; }
        [ "$a" != "$c" ]  || { echo change-blind;    exit 0; }
        [ "$c" != "$c2" ] || { echo content-blind;   exit 0; }
        [ "$c2" != "$d" ] || { echo commit-blind;    exit 0; }
        [ "$d" != "$e" ]  || { echo untracked-blind; exit 0; }
        echo ok
    ) 2>/dev/null )
    check "work_state_fingerprint: stable + content/commit/untracked sensitive" "$FP_R" "ok"
fi

# ── revert-receipt index + expiry ────────────────────────────────────────
# receipt_index emits `number \t parents \t age \t title`. The age field and the
# parentless-expiry branch that reads it were added with no coverage at all, so
# the harness suite was green on a change it could not see. Extracts the REAL
# function from evolve.sh and stubs `gh`, rather than restating its logic here.
receipt_rows() { # $1 = fixture JSON path
    ( set -uo pipefail
      RFIX="$1"
      # `gh` is called as `gh issue list ...`, so the stub must read the fixture
      # from a variable — $1 inside it is "issue", not the path.
      gh() { cat "$RFIX"; }
      REPO=x; RECEIPT_INDEX_LIMIT=100
      eval "$(awk '/^receipt_index\(\)/,/^}/' "$SCRIPT")"
      receipt_index /dev/null ) 2>/dev/null
}
FIXTURE=$(mktemp)
R_OLD=$(python3 -c "import datetime as d;print((d.datetime.now(d.timezone.utc)-d.timedelta(days=20)).isoformat().replace('+00:00','Z'))" 2>/dev/null)
R_NEW=$(python3 -c "import datetime as d;print((d.datetime.now(d.timezone.utc)-d.timedelta(days=2)).isoformat().replace('+00:00','Z'))" 2>/dev/null)
cat > "$FIXTURE" <<JSON
[
 {"number":1,"title":"Task reverted: old","body":"Issue: none","createdAt":"$R_OLD"},
 {"number":2,"title":"Task reverted: fresh","body":"Issue: none","createdAt":"$R_NEW"},
 {"number":3,"title":"Task reverted: parented","body":"**Parent issue:** #500","createdAt":"$R_OLD"},
 {"number":4,"title":"Task reverted: undated","body":"Issue: none","createdAt":""},
 {"number":5,"title":"Task reverted: junk","body":"Issue: none","createdAt":"not-a-date"}
]
JSON
R_ROWS=$(receipt_rows "$FIXTURE")
require "receipt_index: produced rows" "$R_ROWS"
check "receipt: parentless marked '-'"        "$(printf '%s\n' "$R_ROWS" | awk -F'\t' '$1==1{print $2}')" "-"
check "receipt: parent extracted"             "$(printf '%s\n' "$R_ROWS" | awk -F'\t' '$1==3{print $2}')" "500"
check "receipt: age computed"                 "$(printf '%s\n' "$R_ROWS" | awk -F'\t' '$1==1{print $3}')" "20"
# Absence gets its own value. An unreadable date must never look old (it would
# retire a receipt on a guess) nor new (it would pin one open forever).
check "receipt: empty date -> -1, not 0"      "$(printf '%s\n' "$R_ROWS" | awk -F'\t' '$1==4{print $3}')" "-1"
check "receipt: junk date -> -1"              "$(printf '%s\n' "$R_ROWS" | awk -F'\t' '$1==5{print $3}')" "-1"
check "receipt: title survives field 4"       "$(printf '%s\n' "$R_ROWS" | awk -F'\t' '$1==2{print $4}')" "fresh"

# The expiry decision itself, including the shapes that must NOT retire. This is
# the guard the sweep relies on: a non-numeric or unknown age must fail closed.
expires() { [ "${1:--1}" -ge "${2}" ] 2>/dev/null && echo yes || echo no; }
check "expiry: at threshold retires"          "$(expires 14 14)" "yes"
check "expiry: one day under keeps"           "$(expires 13 14)" "no"
check "expiry: unknown age (-1) keeps"        "$(expires -1 14)" "no"
check "expiry: empty age keeps"               "$(expires '' 14)" "no"
check "expiry: non-numeric age keeps"         "$(expires abc 14)" "no"
rm -f "$FIXTURE"

# ── empty-diff gate (Day 195, 2026-09-11: 5/5 promoted, 0 task commits) ──
# Extracts the REAL decision function and the REAL gate blocks and drives them
# with stubs and scratch repos — never restating the logic, never this repo.
TL_FN=$(awk '/^task_landed_changes\(\) \{/,/^\}/' "$SCRIPT")
if require "task_landed_changes extracted" "$TL_FN"; then
    tl_case() { # $1 = untouched | committed | moved-empty | bad-ref
        ( set -uo pipefail
          export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null  # ambient gpgsign etc. must not shape the answer
          eval "$TL_FN"
          d=$(mktemp -d) && cd "$d" || exit 1
          git init -q . && git config user.email t@t && git config user.name t
          echo a > f && git add f && git commit -qm base
          pre=$(git rev-parse HEAD)
          [ -n "$pre" ] || { printf 'fixture-broken'; exit 1; }   # anti-vacuous: a broken fixture fails as ITSELF
          case "$1" in
            committed)   echo b >> f && git commit -qam change ;;
            moved-empty) git commit -q --allow-empty -m nothing ;;
            bad-ref)     pre=not-a-ref ;;
          esac
          out=$(task_landed_changes "$pre"); rc=$?
          printf '%s/%s' "$out" "$rc"
          cd / && rm -rf "$d"
        ) 2>/dev/null | tail -1
    }
    check "landed: untouched tree -> no"                       "$(tl_case untouched)"   "no/0"
    check "landed: a real commit -> yes"                       "$(tl_case committed)"   "yes/0"
    check "landed: HEAD moved, empty tree diff -> no"          "$(tl_case moved-empty)" "no/0"
    # Could not answer: its own state — non-zero and NOTHING printed, so a
    # caller cannot mistake it for an answer. Each arm that can produce it:
    check "landed: bad ref -> cannot answer (diff exit 128)"   "$(tl_case bad-ref)"     "/1"
    TL_NOHEAD=$( ( set -uo pipefail; eval "$TL_FN"; git() { return 128; }
                   task_landed_changes deadbeef; printf '/%s' "$?" ) 2>/dev/null )
    check "landed: no HEAD -> cannot answer (rev-parse arm)"   "$TL_NOHEAD" "/1"
    TL_DIFF128=$( ( set -uo pipefail; eval "$TL_FN"
                    git() { case "$1" in diff) return 128;; *) return 0;; esac; }
                    task_landed_changes deadbeef; printf '/%s' "$?" ) 2>/dev/null )
    check "landed: diff exit 128 -> cannot answer (case arm)"  "$TL_DIFF128" "/1"
    # The harness runs under set -e. A BARE call (not inside an `if`, which
    # happens to suppress errexit) must survive the common path, where
    # `git diff --quiet` exits 1 for a real change.
    TL_ERREXIT=$( ( export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
                    fnf=$(mktemp) && printf '%s\n' "$TL_FN" > "$fnf"
                    d=$(mktemp -d) && cd "$d" && git init -q . && git config user.email t@t && git config user.name t \
                    && echo a > f && git add f && git commit -qm base && pre=$(git rev-parse HEAD) \
                    && echo b >> f && git commit -qam change \
                    && bash -c "set -euo pipefail; source '$fnf'; task_landed_changes '$pre'; printf '/%s' \$?"
                    cd / && rm -rf "$d" "$fnf" ) 2>/dev/null | tail -1 )
    check "landed: survives set -e on a bare call with a real change" "$TL_ERREXIT" "yes/0"

    # Position, not presence: the gate banner must sit AFTER the impl-agent
    # safety commit and BEFORE the evaluator loop opener.
    TL_GATE=$(grep -nF '# ── Empty-diff gate' "$SCRIPT" | head -1 | cut -d: -f1)
    TL_SAFE=$(grep -nF 'safety_commit "Day $DAY ($SESSION_TIME): $task_title (Task $TASK_NUM)"' "$SCRIPT" | head -1 | cut -d: -f1)
    TL_EVAL=$(grep -nF 'while [ "$TASK_OK" = true ] && [ "$EVAL_ATTEMPT" -lt "$MAX_EVAL_ATTEMPTS" ]; do' "$SCRIPT" | head -1 | cut -d: -f1)
    check "gate: sits after the safety commit and before the evaluator" \
        "$( [ -n "$TL_GATE" ] && [ -n "$TL_SAFE" ] && [ -n "$TL_EVAL" ] && [ "$TL_SAFE" -lt "$TL_GATE" ] && [ "$TL_GATE" -lt "$TL_EVAL" ] \
             && echo ordered || echo "gate=$TL_GATE safe=$TL_SAFE eval=$TL_EVAL")" "ordered"

    # The gate block itself, banner to its outer `fi`, driven with a stubbed
    # decision function and stubbed git. Pins polarity, the outer TASK_OK guard,
    # the dirty-tree state and the LANDED_UNKNOWN memo — none visible to a grep.
    TL_BLOCK=$(awk '/^    # ── Empty-diff gate/{p=1} p{print} p && /^    fi$/{exit}' "$SCRIPT")
    tl_gate() { # $1 = stub answer (yes|no|fail)  $2 = porcelain output  $3 = TASK_OK in
        ( set -uo pipefail
          _A="$1"; _P="$2"
          task_landed_changes() { [ "$_A" = fail ] && return 1; printf '%s' "$_A"; }
          git() { case "$*" in "status --porcelain") printf '%s' "$_P";; "rev-parse --short HEAD") echo abc1234;; *) return 0;; esac; }
          TASK_OK="$3"; PRE_TASK_SHA=deadbeefcafe; TASK_NUM=1; TASK_EXIT=1; TASK_LOG_TAIL="log tail"
          REVERT_CLASS=""; REVERT_REASON="prior reason"; REVERT_DETAILS=""
          eval "$TL_BLOCK" >/dev/null
          printf '%s|%s|%s|%s' "$TASK_OK" "$REVERT_CLASS" "${REVERT_REASON:0:18}" "$LANDED_UNKNOWN"
        ) 2>/dev/null | tail -1
    }
    if require "gate block extracted" "$TL_BLOCK"; then
        check "gate: landed yes -> untouched"                    "$(tl_gate yes '' true)"    "true||prior reason|"
        check "gate: landed no -> rejected, classed, detailed"   "$(tl_gate no '' true)"     "false| (no changes landed)|No changes landed:|"
        check "gate: could not measure -> untouched, remembered" "$(tl_gate fail '' true)"   "true||prior reason|true"
        check "gate: dirty tree -> NOT a no-op"                  "$(tl_gate no ' M f' true)" "true||prior reason|"
        check "gate: already-failed task keeps its own reason"   "$(tl_gate no '' false)"    "false||prior reason|"
    fi

    # The post-loop refusal: unmeasured diff + no evaluator judgment = no
    # evidence of work. Extracted from its comment to its `fi`; no git.
    TL_REFUSE=$(awk '/^    # An unverified accept is justified/{p=1} p{print} p && /^    fi$/{exit}' "$SCRIPT")
    tl_refuse() { # $1 = TASK_OK  $2 = LANDED_UNKNOWN  $3 = BUDGET_UNVERIFIED  $4 = EVAL_INFRA_WHY
        ( set -uo pipefail
          TASK_OK="$1"; LANDED_UNKNOWN="$2"; BUDGET_UNVERIFIED="$3"; EVAL_INFRA_WHY="$4"
          TASK_NUM=1; REVERT_CLASS=""; REVERT_REASON="prior reason"
          eval "$TL_REFUSE" >/dev/null
          printf '%s|%s' "$TASK_OK" "$REVERT_CLASS"
        ) 2>/dev/null | tail -1
    }
    if require "post-loop refusal extracted" "$TL_REFUSE"; then
        check "refuse: unmeasured + evaluator no verdict -> not promoted"  "$(tl_refuse true true eval_infra 'no verdict')" "false| (could not be measured)"
        check "refuse: unmeasured + budget-skipped evaluator -> not promoted" "$(tl_refuse true true skipped '')"          "false| (could not be measured)"
        check "refuse: unmeasured + evaluator PASSED -> promotes"          "$(tl_refuse true true '' '')"                 "true|"
        check "refuse: measured + unverified -> today's behaviour"         "$(tl_refuse true '' eval_infra 'no verdict')" "true|"
        check "refuse: already-failed task untouched"                      "$(tl_refuse false true eval_infra 'x')"       "false|"
    fi
fi

# ── accept verdict word (#915) ─────────────────────────────────────────────
AV_FN=$(awk '/^gasp_accept_verdict\(\) \{/,/^\}/' "$SCRIPT")
if require "gasp_accept_verdict extracted" "$AV_FN"; then
    av() { ( set -uo pipefail; eval "$AV_FN"; gasp_accept_verdict "$1" ) 2>/dev/null; }
    check "accept verdict: verified PASS -> promoted"            "$(av '')"          "promoted"
    check "accept verdict: evaluator no verdict -> unverified"   "$(av eval_infra)"  "unverified"
    check "accept verdict: evaluator FAILed out -> unverified"   "$(av eval_failed)" "unverified"
    check "accept verdict: no progress -> unverified"            "$(av no_progress)" "unverified"
    check "accept verdict: budget-skipped -> unverified"         "$(av skipped)"     "unverified"
    # The producer must pass the decided word, never the old literal.
    check "accept verdict: producer passes the decided word" \
        "$(grep -cF '"$task_title" "$GASP_ACCEPT_VERDICT" "$PRE_TASK_SHA"' "$SCRIPT")" "1"
    check "accept verdict: the fixed 'promoted' literal is gone from the producer" \
        "$(grep -cF '"$task_title" promoted "$PRE_TASK_SHA"' "$SCRIPT")" "0"
fi

# ── API-error detector: real failures abort, quoted JSON does not ──────────
# Fixtures are the exact shapes from runs 35909185710 (Day 207 false positive)
# and 36264370389 (Day 210 real 404s missed by the #953 detector).
AE_FN=$(awk '/^agent_log_has_api_error\(\) \{/,/^\}/' "$SCRIPT")
if require "agent_log_has_api_error extracted" "$AE_FN"; then
    ae() { # $1 = log contents (printf-interpreted) → yes|no
        ( set -uo pipefail; eval "$AE_FN"
          f=$(mktemp); printf "$1" > "$f"
          agent_log_has_api_error "$f" && echo yes || echo no; rm -f "$f" ) 2>/dev/null
    }
    R=$'\e[31m'; Z=$'\e[0m'
    check "api error: real 404 as yoyo renders it (red) -> abort" \
        "$(ae "\n${R}  error: API error: HTTP 404 Not Found: {\"type\":\"error\",\"error\":{}}${Z}\n")" "yes"
    check "api error: real 404 without color -> abort" \
        "$(ae "  error: API error: HTTP 529 : {\"type\":\"error\"}\n")" "yes"
    check "api error: 401 renders as Auth error -> abort" \
        "$(ae "${R}  error: Auth error: HTTP 401 Unauthorized: {}${Z}\n")" "yes"
    check "api error: non-Anthropic body (no type:error) still aborts" \
        "$(ae "  error: API error: HTTP 400 Bad Request: {\"error\":{\"message\":\"x\"}}\n")" "yes"
    check "api error: Day-207 test fixture quoted in a diff -> no abort" \
        "$(ae "        classify_session_usage(['{\"type\":\"error\",\"msg\":\"x\"}']),\n")" "no"
    check "api error: raw JSONL line an agent printed -> no abort" \
        "$(ae "{\"type\":\"error\",\"msg\":\"x\"}\n")" "no"
    check "api error: source line that renders errors -> no abort" \
        "$(ae "    eprintln!(\"\\\\n{RED}  error: {error_msg}{RESET}\");\n")" "no"
    check "api error: context-overflow retry note -> no abort" \
        "$(ae "  (API error: prompt is too long: {\"type\":\"error\"})\n")" "no"
    check "api error: clean log -> no abort" "$(ae "all good\n")" "no"
    # #916: terminal failures whose own line is not tagged `API error:` are
    # caught by yoyo's give-up sentinel, which follows every one of them.
    check "api error: Day-195 rate limit (plain) + give-up sentinel -> abort" \
        "$(ae "  error: Rate limited, retry after Some(57715000)ms\n  API error with no fallback configured. Exiting.\n")" "yes"
    check "api error: network error + red give-up sentinel -> abort" \
        "$(ae "${R}  error: Network error: connection reset${Z}\n${R}  API error with no fallback configured. Exiting.${Z}\n")" "yes"
    # Near-misses: the source that prints the sentinel, as read_file shows it,
    # and prose naming the error, must never abort a task.
    check "api error: the sentinel's source line read by an agent -> no abort" \
        "$(ae "        eprintln!(\"{RED}  API error with no fallback configured. Exiting.{RESET}\",);\n")" "no"
    check "api error: numbered read_file line of that source -> no abort" \
        "$(ae "1958\t        eprintln!(\"{RED}  API error with no fallback configured. Exiting.{RESET}\",);\n")" "no"
    check "api error: prose mentioning rate limits -> no abort" \
        "$(ae "The provider said Rate limited earlier, so I waited.\n")" "no"
fi

# ── #916: the API-error abort files a classed receipt like any revert ────────
# Ends at the next section, not the first column-0 `}`: the receipt body has
# one inside a quoted string (the close of `${task_issue:+ … }`).
RR_FN=$(awk '/^file_revert_receipt\(\) \{/{p=1} /^# ── Keep a reverted task/{exit} p' "$SCRIPT")
if require "file_revert_receipt extracted" "$RR_FN"; then
    rr_case() { # fake gh on PATH records the create call → title|label
        ( set +e; d=$(mktemp -d)
          printf '#!/bin/sh\ncase "$1 $2" in\n  "issue list") exit 0;;\n  "issue create") shift 2; while [ $# -gt 0 ]; do case "$1" in --title) echo "T:$2";; --label) echo "L:$2";; esac; shift; done >> "%s/calls"; echo https://example/1;;\nesac\n' "$d" > "$d/gh"
          chmod +x "$d/gh"; PATH="$d:$PATH"
          refresh_gh_token() { :; }
          QUIET_MODE=false; REPO=o/r; DAY=226; TASK_NUM=1; task_title="Fix the thing"; task_issue=""
          TASK_DESC="spec"; REVERT_CLASS=" (provider error)"; REVERT_REASON="r"; REVERT_DETAILS="d"
          eval "$RR_FN"; file_revert_receipt >/dev/null
          printf '%s|%s' "$(grep '^T:' "$d/calls" | cut -c3-)" "$(grep '^L:' "$d/calls" | cut -c3-)"
          rm -rf "$d" ) 2>/dev/null
    }
    check "receipt: provider-error class lands in the title with the agent-revert label" \
        "$(rr_case)" "Task reverted (provider error): Fix the thing|agent-revert"
    # Wiring: the abort records a verdict and files the receipt before it breaks.
    AB=$(awk '/if agent_log_has_api_error "\$TASK_LOG"; then/{p=1} p{print} p&&/^            break$/{exit}' "$SCRIPT")
    check "api abort: sets the provider-error class" "$(echo "$AB" | grep -c 'REVERT_CLASS=" (provider error)"')" "1"
    check "api abort: records a rejected GASP verdict" "$(echo "$AB" | grep -c 'gasp_task_result .* rejected')" "1"
    check "api abort: files the receipt" "$(echo "$AB" | grep -c '^            file_revert_receipt$')" "1"
    check "planner prompt: the class list says five" "$(grep -cF 'CLASS, and the five' "$SCRIPT")" "1"
    check "planner prompt: RECENTLY REVERTED lists provider error" "$(grep -cF '"Task reverted (provider error): X"' "$SCRIPT")" "1"
    check "planner prompt: task guidance lists provider error" "$(grep -cF '"(provider error)" — the provider failed' "$SCRIPT")" "1"
fi

# ── model: .yoyo.toml is the single source, nothing hardcodes an id ─────────
# Until 2026-09-27 the model id was copied into 22 places (every workflow's
# `secrets.MODEL || '…'`, every script's `${MODEL:-…}`), still naming a model two
# switches old, while `provider` lived only in .yoyo.toml. That split is how
# Days 210-211 sent a DeepSeek id to Anthropic (404 on every call). Comment
# lines are exempt (history may name models); so is extract_trajectory.py,
# whose self-test fixtures carry model ids but never select one.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODEL_ID_RE='claude-(opus|sonnet|haiku|fable|mythos)-[0-9]|deepseek-(v[0-9]|flash|chat|reasoner)|gpt-[0-9]|gemini-[0-9]'
hardcoded_models() { # $@ = files → offending file:line, one per line
    grep -HnE "$MODEL_ID_RE" "$@" 2>/dev/null | grep -vE '^[^:]+:[0-9]+:[[:space:]]*#'
}
HC_FILES=$(cd "$ROOT" && ls scripts/*.sh scripts/*.py .github/workflows/*.yml \
    | grep -vE '^scripts/test_|^scripts/extract_trajectory\.py$')
require "model guard: file list" "$HC_FILES" && \
    check "model guard: no script or workflow hardcodes a model id" \
        "$(cd "$ROOT" && hardcoded_models $HC_FILES | head -3)" ""
HC_TMP=$(mktemp -d)
printf 'MODEL="${MODEL:-claude-opus-5-5}"\n# was claude-opus-4-6\n' > "$HC_TMP/a.sh"
check "model guard: positive control flags a hardcoded default" \
    "$(hardcoded_models "$HC_TMP/a.sh" | wc -l | tr -d ' ')" "1"
rm -rf "$HC_TMP"
check "model guard: .yoyo.toml names a provider and a model" \
    "$(awk -F'"' '/^\[/{exit} /^(provider|model)[[:space:]]*=/{n++} END{print n+0}' "$ROOT/.yoyo.toml")" "2"
# Every harness script resolves the model the same way, from .yoyo.toml.
for _s in evolve.sh social.sh skill_evolve.sh dream.sh; do
    check "model guard: $_s resolves MODEL from .yoyo.toml" \
        "$(grep -c '^MODEL="${MODEL:-$CONFIG_MODEL}"$' "$ROOT/scripts/$_s")" "1"
done

# ── skill-evolve counter: a concurrent bump must not commit conflict markers ─
# bb2c6101 (2026-09-28) committed "<<<<<<< 7 ======= 0 >>>>>>>" because the
# reset wrote the counter BEFORE `git pull --rebase --autostash`; the pop
# conflicted, git exited 0, and the markers were committed. evolve.sh's reader
# then turned them into 70 and bumped to 71. Real git, real race, the real
# block extracted from skill_evolve.sh.
SE="$ROOT/scripts/skill_evolve.sh"
SE_BLOCK=$(awk '/# Race protection \(C2\)/{p=1} p{print} p&&/reset counter \(cycle/{c=1} c&&/^        fi$/{exit}' "$SE")
counter_race() { # $1 = block to run as the resetting side → final counter file, one line
    ( set +e; d=$(mktemp -d); cd "$d" || exit 1
      git init -q --bare remote.git
      git clone -q remote.git a 2>/dev/null; cd a
      git config user.email t@t; git config user.name t
      echo 6 > .skill_evolve_counter; git add .; git commit -qm base; git push -q origin HEAD 2>/dev/null
      git clone -q ../remote.git ../b 2>/dev/null
      ( cd ../b && git config user.email t@t && git config user.name t \
        && echo 7 > .skill_evolve_counter && git commit -qam bump && git push -q origin HEAD 2>/dev/null )
      COUNTER_FILE=.skill_evolve_counter
      eval "$1" >/dev/null 2>&1
      echo "$(tr '\n' '|' < .skill_evolve_counter)bump_kept=$(git log --format=%s | grep -c '^bump$')"
      cd /; rm -rf "$d" ) 2>/dev/null | tail -1
}
if require "skill_evolve.sh reset block extracted" "$SE_BLOCK"; then
    check "counter race: reset after a concurrent bump is a clean 0" \
        "$(counter_race "$SE_BLOCK")" "0|bump_kept=1"
    # Positive control: the pre-fix order (write, then pull) reproduces bb2c6101.
    OLD_ORDER="echo 0 > \"\$COUNTER_FILE\"
$(echo "$SE_BLOCK" | grep -v '^        echo 0 > "\$COUNTER_FILE"$')"
    case "$(counter_race "$OLD_ORDER")" in
        *"<<<<<<<"*) ok "counter race: positive control — write-then-pull commits conflict markers" ;;
        *) bad "counter race: positive control" "old order did not reproduce the markers" ;;
    esac
fi
MARKED=$(mktemp); printf '<<<<<<< Updated upstream\n7\n=======\n0\n>>>>>>> Stashed changes\n' > "$MARKED"
for _s in "$SE" "$SCRIPT"; do
    check "counter reader: $(basename "$_s") reads a conflict-marked file as 7, not 70" \
        "$(grep -m1 -xE '[0-9]+' "$MARKED")" "7"
    check "counter reader: $(basename "$_s") uses the bare-integer reader" \
        "$(grep -cF "grep -m1 -xE '[0-9]+'" "$_s")" "1"
done
rm -f "$MARKED"

# ── sweep scope gate (#951): the ungated `git add -A` commits bookkeeping only ─
# Real git. The dirty tree mixes what a wrap-up legitimately commits with the
# drift it has actually shipped (804620e4's src/ edit, 2c360b4c's stray `bar`),
# plus the awkward shapes: a deleted tracked file, a space in a path, a rename
# out of an allowed dir, and a gitignored file that must never be touched.
GATE_FN=$(awk '/^commit_scope_gate\(\) \{/,/^\}/' "$SCRIPT")
GATE_VARS=$(grep -E '^(WRAPUP|PREPUSH)_SCOPE=' "$SCRIPT")
gate_case() { # $1 = "gate" or "nogate" → committed|clean|patch|ignored
    ( set +e; d=$(mktemp -d); cd "$d" || exit 1
      git init -q; git config user.email t@t; git config user.name t
      mkdir -p src tests journals memory .yoyo target "src/sp ace"
      echo target/ > .gitignore
      echo a > src/a.rs; echo t > tests/t.rs; echo j > journals/J.md
      echo m > memory/moved.md; echo x > "src/sp ace/b.rs"
      git add -A; git commit -qm base
      echo EDIT >> src/a.rs; echo stray > bar; git rm -q tests/t.rs
      echo more >> "src/sp ace/b.rs"; git mv memory/moved.md src/moved.md
      echo entry >> journals/J.md; echo '{}' > .yoyo/new.jsonl
      echo keep > target/ignored.bin
      SESSION_STAGING="$(mktemp -d)/stage"  # outside the repo, as the real (gitignored) staging dir is
      eval "$GATE_VARS"
      if [ "$1" = gate ]; then eval "$GATE_FN"; commit_scope_gate "$WRAPUP_SCOPE" wrapup >/dev/null; fi
      git add -A; git commit -qm wrapup
      committed=$(git show --name-only --format= HEAD | sort | paste -sd, -)
      clean=$([ -z "$(git status --porcelain)" ] && echo clean || echo dirty)
      patch=$(grep -c '^diff --git' "$SESSION_STAGING/refused_wrapup.patch" 2>/dev/null || echo 0)
      ignored=$(cat target/ignored.bin 2>/dev/null)
      echo "$committed|$clean|$patch|$ignored"
      cd /; rm -rf "$d" "$(dirname "$SESSION_STAGING")" ) 2>/dev/null | tail -1
}
if require "commit_scope_gate extracted" "$GATE_FN" && require "scope vars extracted" "$GATE_VARS"; then
    # memory/moved.md -> src/moved.md is a rename OUT of scope: both halves are
    # refused, so moved.md stays where it was and nothing is committed for it.
    check "scope gate: wrap-up commits only bookkeeping; refused drift restored; ignored file untouched" \
        "$(gate_case gate)" ".yoyo/new.jsonl,journals/J.md|clean|5|keep"
    case "$(gate_case nogate)" in
        *src/a.rs*bar*|*bar*src/a.rs*) ok "scope gate: positive control — the bare sweep commits src/ and the stray file" ;;
        *) bad "scope gate: positive control" "ungated sweep did not commit the drift" ;;
    esac
    # Wired in front of BOTH sweeps, with the right scope each.
    check "scope gate: wrap-up sweep is gated" \
        "$(grep -A1 -F 'commit_scope_gate "$WRAPUP_SCOPE" wrapup' "$SCRIPT" | tail -1)" "git add -A"
    check "scope gate: pre-push sweep is gated" \
        "$(grep -A1 -F 'commit_scope_gate "$PREPUSH_SCOPE" prepush' "$SCRIPT" | tail -1 | cut -c1-17)" "PREPUSH_LEFTOVERS"
fi

# ── a reverted task's work is saved before the reset (#997) ─────────────────
# Day 219 (13:11) lost a verified fix to `git reset --hard` after a transient
# Overloaded. Real git: a task commit, an uncommitted edit and a new untracked
# file must all land in the saved patch, and that patch must re-apply cleanly
# on the reset tree. Nothing to save → no file.
SAVE_FN=$(awk '/^save_reverted_task_diff\(\) \{/,/^\}/' "$SCRIPT")
save_case() { # $1 = work|none → "<files in patch>|<tree after reset>|<re-apply>|<var set?>"
    ( set +e; d=$(mktemp -d); cd "$d" || exit 1
      git init -q; git config user.email t@t; git config user.name t
      echo a > a.rs; echo b > b.rs; git add -A; git commit -qm base; base=$(git rev-parse HEAD)
      if [ "$1" = work ]; then
          echo A2 >> a.rs; git commit -qam "task commit"
          echo B2 >> b.rs; echo new > c.rs
      fi
      SESSION_STAGING="$(mktemp -d)/stage"; eval "$SAVE_FN"
      save_reverted_task_diff "$base" task1_api_error >/dev/null
      git reset -q --hard "$base"; git clean -qfd
      p="$SESSION_STAGING/task1_api_error.patch"
      files=$(grep '^diff --git' "$p" 2>/dev/null | awk '{print $3}' | sed 's#^a/##' | sort | paste -sd, -)
      tree=$([ -z "$(git status --porcelain)" ] && echo clean || echo dirty)
      apply=$([ -s "$p" ] && { git apply --index "$p" 2>/dev/null && grep -q B2 b.rs && [ -f c.rs ] && echo reapplies || echo broken; } || echo none)
      echo "${files:-}|$tree|$apply|${SAVED_TASK_PATCH:+set}"
      cd /; rm -rf "$d" "$(dirname "$SESSION_STAGING")" ) 2>/dev/null | tail -1
}
if require "save_reverted_task_diff extracted" "$SAVE_FN"; then
    check "revert save: commit + uncommitted edit + new file saved, reset clean, patch re-applies" \
        "$(save_case work)" "a.rs,b.rs,c.rs|clean|reapplies|set"
    check "revert save: a task with no changes saves nothing" "$(save_case none)" "|clean|none|"
    # Both reset sites save first: the call sits right above each reset.
    check "revert save: the API-error abort saves before its reset" \
        "$(awk '/if agent_log_has_api_error "\$TASK_LOG"; then/{p=1} p&&/save_reverted_task_diff/{s=NR} p&&/git reset --hard "\$PRE_TASK_SHA"/{print (s && s<NR) ? "save-first" : "reset-first"; exit}' "$SCRIPT")" \
        "save-first"
    check "revert save: the verification revert saves before its reset" \
        "$(grep -c -F 'save_reverted_task_diff "$PRE_TASK_SHA" "task${TASK_NUM}_reverted"' "$SCRIPT")" "1"
fi

# ── skills shipped inside dependency crates (setup_dependency_skills) ────────
# Offline: `cargo` is stubbed to print a fixture `cargo metadata` document whose
# packages live in a temp dir. Only DIRECT dependencies of the root count, only
# a skills/ dir holding a <name>/SKILL.md counts, and no crate is named in the
# harness. Each case prints the skill flags appended (or "none") and the status.
DS_FN="$(awk '/^dependency_skill_dirs\(\) \{/,/^\}/' "$SCRIPT")
$(awk '/^setup_dependency_skills\(\) \{/,/^\}/' "$SCRIPT")"
ds_case() { # $1 = ok|fail (cargo metadata), $2 = disabled?
    ( set -euo pipefail; d=$(mktemp -d); cd "$d"
      mk() { mkdir -p "$d/$1"; echo '[package]' > "$d/$1/Cargo.toml"; }
      mk agentlib-1.2.0; mkdir -p agentlib-1.2.0/skills/agentlib; echo x > agentlib-1.2.0/skills/agentlib/SKILL.md
      mk emptyskills-1.0.0; mkdir -p emptyskills-1.0.0/skills/notes   # a skills/ dir with no SKILL.md
      mk plain-2.0.0                                                   # no skills/ at all
      mk deep-0.1.0; mkdir -p deep-0.1.0/skills/deep; echo x > deep-0.1.0/skills/deep/SKILL.md  # transitive only
      cat > meta.json <<JSON
{"packages":[
 {"id":"root","name":"root","version":"0.1.0","manifest_path":"$d/Cargo.toml"},
 {"id":"a","name":"agentlib","version":"1.2.0","manifest_path":"$d/agentlib-1.2.0/Cargo.toml"},
 {"id":"e","name":"emptyskills","version":"1.0.0","manifest_path":"$d/emptyskills-1.0.0/Cargo.toml"},
 {"id":"p","name":"plain","version":"2.0.0","manifest_path":"$d/plain-2.0.0/Cargo.toml"},
 {"id":"t","name":"deep","version":"0.1.0","manifest_path":"$d/deep-0.1.0/Cargo.toml"}],
 "resolve":{"root":"root","nodes":[
  {"id":"root","deps":[{"pkg":"a"},{"pkg":"e"},{"pkg":"p"}]},
  {"id":"a","deps":[{"pkg":"t"}]}]}}
JSON
      if [ "$1" = ok ]; then cargo() { cat "$d/meta.json"; }; else cargo() { return 101; }; fi
      [ "${2:-}" = disabled ] && export YOYO_DEPENDENCY_SKILLS_DISABLED=1
      YOYO_SKILL_FLAGS=()
      eval "$DS_FN"; setup_dependency_skills >/dev/null; rc=$?
      out="${YOYO_SKILL_FLAGS[*]:-none}"; echo "${out//$d/D}|rc=$rc"
      rm -rf "$d" ) 2>/dev/null
}
if require "setup_dependency_skills extracted" "$(awk '/^setup_dependency_skills\(\) \{/,/^\}/' "$SCRIPT")" &&
   require "dependency_skill_dirs extracted" "$(awk '/^dependency_skill_dirs\(\) \{/,/^\}/' "$SCRIPT")"; then
    check "dependency skills: only a direct dependency that ships a SKILL.md is loaded" \
        "$(ds_case ok)" "--skills D/agentlib-1.2.0/skills|rc=0"
    check "dependency skills: cargo metadata failing adds nothing and does not fail" \
        "$(ds_case fail)" "none|rc=0"
    check "dependency skills: YOYO_DEPENDENCY_SKILLS_DISABLED=1 adds nothing" \
        "$(ds_case ok disabled)" "none|rc=0"
fi
# ── session-start dependency updates (update_direct_dependencies) ────────────
# A temp git repo and a stub `cargo`: `metadata` reports the direct deps at the
# versions written in Cargo.lock, `update` rewrites the lock per case, and
# build/test pass or fail per case. Prints what cargo update was asked for, the
# commit (if any), whether DEPS_VERIFIED_SHA is HEAD, and the lock afterwards.
UD_FN="$(awk '/^direct_dependency_specs\(\) \{/,/^\}/' "$SCRIPT")
$(awk '/^update_direct_dependencies\(\) \{/,/^\}/' "$SCRIPT")"
ud_case() { # $1 = bump|none|updfail, $2 = pass|fail (build+test), $3 = disabled|dirty
    ( set -euo pipefail; d=$(mktemp -d); cd "$d"
      git init -q; git config user.email t@t; git config user.name t
      printf 'agentlib 1.2.0\nplain 2.0.0\ndeep 0.1.0\n' > Cargo.lock
      git add Cargo.lock; git commit -qm base
      [ "${3:-}" = dirty ] && echo '# local edit' >> Cargo.lock
      [ "${3:-}" = disabled ] && export YOYO_DEPENDENCY_UPDATES_DISABLED=1
      export UD_MODE="$1" UD_TESTS="$2" UD_LOG="$d/cargo.log"
      cargo() {
          case "$1" in
            metadata)
              local a p; a=$(awk '$1=="agentlib"{print $2}' Cargo.lock); p=$(awk '$1=="plain"{print $2}' Cargo.lock)
              printf '{"packages":[{"id":"r","name":"root","version":"0.1.0"},{"id":"a","name":"agentlib","version":"%s"},{"id":"p","name":"plain","version":"%s"},{"id":"t","name":"deep","version":"0.1.0"}],"resolve":{"root":"r","nodes":[{"id":"r","deps":[{"pkg":"a"},{"pkg":"p"}]},{"id":"a","deps":[{"pkg":"t"}]}]}}' "$a" "$p" ;;
            update)
              shift; echo "update $*" >> "$UD_LOG"
              case "$UD_MODE" in
                bump)    sed -i.bak 's/^agentlib 1.2.0$/agentlib 1.2.3/' Cargo.lock; rm -f Cargo.lock.bak ;;
                updfail) echo half-written > Cargo.lock; return 101 ;;
              esac ;;
            build|test) echo "$1" >> "$UD_LOG"; [ "$UD_TESTS" = pass ] ;;
          esac
      }
      eval "$UD_FN"; update_direct_dependencies >/dev/null 2>&1
      head=$(git rev-parse HEAD)
      printf '%s|%s|verified=%s|lock=%s|dirty=%s' \
        "$(grep '^update' cargo.log 2>/dev/null | tr '\n' ' ' || true)" \
        "$(git log -1 --format=%s)" \
        "$([ "${DEPS_VERIFIED_SHA:-}" = "$head" ] && echo yes || echo no)" \
        "$(awk '$1=="agentlib"{print $2}' Cargo.lock)" \
        "$(git diff --quiet -- Cargo.lock && echo no || echo yes)"
      rm -rf "$d" ) 2>/dev/null
}
if require "update_direct_dependencies extracted" "$(awk '/^update_direct_dependencies\(\) \{/,/^\}/' "$SCRIPT")"; then
    check "dep updates: a patch that passes is committed, direct deps only, and marked verified" \
        "$(ud_case bump pass)" "update -p agentlib@1.2.0 -p plain@2.0.0 |deps: semver-compatible updates (agentlib 1.2.0 -> 1.2.3)|verified=yes|lock=1.2.3|dirty=no"
    check "dep updates: a patch that fails build/tests is rolled back, nothing committed" \
        "$(ud_case bump fail)" "update -p agentlib@1.2.0 -p plain@2.0.0 |base|verified=no|lock=1.2.0|dirty=no"
    check "dep updates: nothing to update commits nothing" \
        "$(ud_case none pass)" "update -p agentlib@1.2.0 -p plain@2.0.0 |base|verified=no|lock=1.2.0|dirty=no"
    check "dep updates: cargo update failing restores the lock" \
        "$(ud_case updfail pass)" "update -p agentlib@1.2.0 -p plain@2.0.0 |base|verified=no|lock=1.2.0|dirty=no"
    check "dep updates: YOYO_DEPENDENCY_UPDATES_DISABLED=1 does nothing" \
        "$(ud_case bump pass disabled)" "|base|verified=no|lock=1.2.0|dirty=no"
    check "dep updates: an already-edited Cargo.lock is left alone" \
        "$(ud_case bump pass dirty)" "|base|verified=no|lock=1.2.0|dirty=yes"
fi
check "evolve.sh: Step 1 trusts the suite Step 1a just ran on the same commit" \
    "$(grep -c '"\$HEAD_SHA" = "\$DEPS_VERIFIED_SHA"' "$SCRIPT")" "1"
check "evolve.sh: the skipped-suite line says Step 1a, not CI, when Step 1a ran it" \
    "$(grep -c 'SUITE_SOURCE="Step 1a already ran the full suite on' "$SCRIPT")" "1"
check "evolve.sh: a failed-build revert restores Cargo.lock too" \
    "$(grep -c 'git checkout "\$SESSION_START_SHA" -- src/ Cargo.toml Cargo.lock' "$SCRIPT")" "1"
check "evolve.sh: dependency updates run before the Step 1 build check" \
    "$(awk '/^update_direct_dependencies$/{u=NR} /^echo "→ Checking build..."$/{b=NR} END{print (u && b && u<b) ? "yes" : "no"}' "$SCRIPT")" "yes"
check "evolve.sh: no dependency is named or cloned (dependency skills and updates)" \
    "$(awk '/^setup_dependency_skills\(\) \{/,/^\}/; /^dependency_skill_dirs\(\) \{/,/^\}/; /^direct_dependency_specs\(\) \{/,/^\}/; /^update_direct_dependencies\(\) \{/,/^\}/' "$SCRIPT" | grep -ciE 'yoagent|git clone')" "0"

# ── skill-evolve retire rule: one rule, and one that can fire (#1017, #858) ──
# The state table said `score < 0.3 OR 10 sessions unused`, the step that runs
# said AND, and the score could never fall below 0.5 (wins == uses for every
# skill), so retirement was unreachable. Pin the replacement in the spec text
# the skill-evolve agent follows: both places say 60 days of disuse, nothing
# gates retirement on the score, and unmeasured skills are exempt.
SE_SPEC="$(cd "$(dirname "$0")/.." && pwd)/skills/skill-evolve/SKILL.md"
check "skill-evolve: state table retires on 60 days of disuse" \
    "$(grep -c '^| \*\*deprecated\*\* |.*unused ≥ 60 days' "$SE_SPEC")" "1"
check "skill-evolve: the retire step uses the same 60-day disuse rule" \
    "$(grep -c '^1\. \*\*Retire\*\*.*60 or more days' "$SE_SPEC")" "1"
check "skill-evolve: no retire rule is gated on score < 0.3 any more" \
    "$(grep -E '^(1\. \*\*Retire\*\*|\| \*\*(active|deprecated)\*\* \|)' "$SE_SPEC" | grep -c 'score < 0.3')" "0"
check "skill-evolve: unmeasured skills are exempt, naming the unlogged loops" \
    "$(grep -c 'never retired for silence.*social.*dream' "$SE_SPEC")" "1"

# ── skill-evolve spec snippets: run the bash the agent copies (#858) ─────────
# The enumeration loop and the event-number lines are executed as written, so
# test them by running them, not by reading them.
SE_ENUM=$(awk '/^### 2\. Enumerate eligible skills/{f=1} f&&/^```bash$/{g=1; next} g&&/^```$/{exit} g' "$SE_SPEC")
if require "skill-evolve: enumeration snippet extracted" "$SE_ENUM"; then
    se_enum=$( set +e; d=$(mktemp -d); cd "$d" || exit 1
        mkdir -p skills/mine skills/creators skills/core skills/skill-evolve
        printf -- '---\nname: mine\norigin: yoyo\n---\nbody\n' > skills/mine/SKILL.md
        # origin: creator, but its body quotes a template carrying origin: yoyo
        printf -- '---\nname: creators\norigin: creator\n---\n```\n---\norigin: yoyo\n---\n```\n' > skills/creators/SKILL.md
        printf -- '---\nname: core\ncore: true\norigin: yoyo\n---\n' > skills/core/SKILL.md
        printf -- '---\norigin: yoyo\n---\n' > skills/skill-evolve/SKILL.md
        # no frontmatter; the body has origin: yoyo between two horizontal rules
        mkdir -p skills/nofm skills/unclosed
        printf -- '# nofm\n---\norigin: yoyo\n---\n' > skills/nofm/SKILL.md
        # frontmatter never closes, so the quoted template in the body is not frontmatter
        printf -- '---\nname: unclosed\norigin: creator\n```\norigin: yoyo\n```\n' > skills/unclosed/SKILL.md
        eval "$SE_ENUM" | tr '\n' ' '; rm -rf "$d" )
    check "skill-evolve: only frontmatter origin: yoyo is eligible (body, no-frontmatter and unclosed ignored)" "$se_enum" "mine "
fi
SE_EVT=$(awk '/^last=\$\(grep -oE .evt-/{f=1} f; f&&/^evt=/{exit}' "$SE_SPEC")
if require "skill-evolve: event-number snippet extracted" "$SE_EVT"; then
    se_evt() { ( set +e; d=$(mktemp -d); cd "$d" || exit 1; mkdir skills
        printf '## x %s NO-OP\n' "$@" > skills/_journal.md
        eval "$SE_EVT" 2>/dev/null; echo "${evt:-none}"; rm -rf "$d" ) }
    check "skill-evolve: next event after evt-0007 is evt-0008" "$(se_evt evt-0006 evt-0007)" "evt-0008"
    check "skill-evolve: next event after evt-0008 is evt-0009 (not an octal error)" "$(se_evt evt-0008)" "evt-0009"
    check "skill-evolve: next event after evt-0015 is evt-0016 (not octal 14)" "$(se_evt evt-0011 evt-0015)" "evt-0016"
    check "skill-evolve: a journal with no events starts at evt-0001" "$(se_evt)" "evt-0001"
    check "skill-evolve: a missing journal starts at evt-0001" \
        "$( set +e; d=$(mktemp -d); cd "$d" || exit 1; mkdir skills; eval "$SE_EVT" 2>/dev/null; echo "${evt:-none}"; rm -rf "$d" )" "evt-0001"
fi
check "skill-evolve: complaint signals need skills/<name> or a backticked name" \
    "$(grep -c '^- \*\*Complaint signals\*\*.*never the bare word' "$SE_SPEC")" "1"
# wins == uses for every skill, so a wins-ratio trigger can never fire: both the
# diagnosis step and the Refine action trigger on complaints alone.
check "skill-evolve: diagnosis and Refine trigger on complaint_signals alone" \
    "$(grep -cE '^(For each skill where `complaint_signals ≥ 2`, open|2\. \*\*Refine\*\*: if any skill has `complaint_signals ≥ 2` and has not)' "$SE_SPEC")" "2"
check "skill-evolve: (wins/uses) < 0.5 appears only in the note that it never fired" \
    "$(grep -c 'wins/uses) < 0.5' "$SE_SPEC")" "1"

# ── skill-evolve retire cleans up its own pointers (evt-0041, evt-0042) ──────
# Retiring explore-codebase left blindspot and synthesis saying "use
# `explore-codebase`", and no rule could repair them: a pointer is not a
# complaint, and synthesis is unmeasured so it is never retired. The Retire step
# now greps for pointers and fixes them in the same event. Run its grep.
SE_PTR=$(awk '/^#### Retire/{f=1} f&&/^grep -nE /{print; exit}' "$SE_SPEC")
if require "skill-evolve: retire pointer grep extracted" "$SE_PTR"; then
    se_ptr=$( set +e; d=$(mktemp -d); cd "$d" || exit 1; mkdir -p skills/a skills/b skills/c skills/d
        printf 'use `gone` first\n' > skills/a/SKILL.md
        printf 'see skills/gone/SKILL.md\n' > skills/b/SKILL.md
        printf 'the gone days\n' > skills/c/SKILL.md            # bare word: prose, not a pointer
        printf 'see skills/gone-v2/ and `gone-v2`\n' > skills/d/SKILL.md  # a different skill
        eval "${SE_PTR//<name>/gone}" | cut -d: -f1 | tr '\n' ' '; rm -rf "$d" )
    check "skill-evolve: retire pointer grep finds backticked and path pointers only" "$se_ptr" "skills/a/SKILL.md skills/b/SKILL.md "
fi
check "skill-evolve: hard rule 3 counts pointer cleanup as part of the retire" \
    "$(grep -c '^- a retirement .*pointer cleanup.*same mutation' "$SE_SPEC")" "1"
check "skill-evolve: no live skill points at a skill in the attic" \
    "$(for a in "$ROOT"/skills_attic/*/; do n=$(basename "$a"); grep -lE "\`$n\`|skills/$n([^a-z0-9-]|$)" "$ROOT"/skills/*/SKILL.md; done | grep -v '/skill-evolve/' | wc -l | tr -d ' ')" "0"

# ── skill-evolve saturation: the flag the spec writes is now read (evt-0035) ─
SAT_FN=$(awk '/^latest_event_saturated\(\) \{/,/^\}/' "$SE")
if require "skill_evolve.sh latest_event_saturated extracted" "$SAT_FN"; then
    sat_case() { ( d=$(mktemp -d); printf "$1" > "$d/j"; eval "$SAT_FN"
        latest_event_saturated "$d/j" && echo yes || echo no; rm -rf "$d" ) }
    check "saturation: newest event saturated -> cooldown extends" \
        "$(sat_case '## t evt-0001 NO-OP\n- evolution_saturation: true\n')" "yes"
    check "saturation: an older saturated event does not count" \
        "$(sat_case '## t evt-0001 NO-OP\n- evolution_saturation: true\n## t evt-0002 refine\n- evolution_saturation: false\n')" "no"
    check "saturation: no flag -> normal cooldown" "$(sat_case '## t evt-0001 NO-OP\n')" "no"
    check "saturation: missing journal -> normal cooldown" \
        "$( eval "$SAT_FN"; latest_event_saturated /nonexistent/j && echo yes || echo no )" "no"
fi
# ── skill-evolve cooldown survives a fresh checkout (evt-0046) ───────────────
# .skill_evolve_last_run is gitignored and uncached, so in CI the cooldown never
# applied. With no file, the last cycle's time comes from the journal's newest
# usable "## <ISO> evt-NNNN" header (headers are model-written: unparseable and
# far-future ones are skipped).
JE_FN=$(awk '/^journal_last_event_epoch\(\) \{/,/^\}/' "$SE")
if require "skill_evolve.sh journal_last_event_epoch extracted" "$JE_FN"; then
    je_case() { ( d=$(mktemp -d); printf "$1" > "$d/j"; eval "$JE_FN"; journal_last_event_epoch "$d/j"; rm -rf "$d" ) }
    check "cooldown: newest event header gives the last cycle (minutes form)" \
        "$(je_case '## 2026-10-17T12:30Z evt-0045 NO-OP\n- x\n## 2026-10-18T12:55Z evt-0046 meta-suggestion\n')" "1792328100 2026-10-18T12:55Z"
    check "cooldown: seconds and offset forms parse; the newest wins wherever it sits" \
        "$(je_case '## 2026-10-01T00:00Z evt-0001 NO-OP\n## 2026-10-18T12:55:30+00:00 evt-0046 NO-OP\n## 2026-10-02T00:00Z evt-0002 NO-OP\n')" "1792328130 2026-10-18T12:55:30+00:00"
    check "cooldown: a far-future header (typo'd year) is ignored" \
        "$(je_case '## 2026-10-18T12:55Z evt-0046 NO-OP\n## 2099-10-18T12:55Z evt-0047 NO-OP\n')" "1792328100 2026-10-18T12:55Z"
    check "cooldown: an unparseable header is skipped, not fatal" \
        "$(je_case '## 2026-10-18T12:55Z evt-0046 NO-OP\n## 2026-13-45T99:99Z evt-0047 NO-OP\n## evt-0000 init\n')" "1792328100 2026-10-18T12:55Z"
    check "cooldown: a journal with no event header gives nothing" "$(je_case '# journal\n- x\n')" ""
    check "cooldown: a missing journal gives nothing" \
        "$( eval "$JE_FN"; journal_last_event_epoch /nonexistent/j )" ""
fi
# The real Gate 2 region, run in a temp checkout: which of file / journal wins,
# and whether the saturation tripling now applies with no file.
GATE2=$(awk '/^journal_last_event_epoch\(\) \{/{p=1} /^# ── Gate 3/{exit} p' "$SE")
if require "skill_evolve.sh Gate 2 extracted" "$GATE2"; then
    gate2_case() { # $1 = journal (printf fmt) or "-", $2 = last-run file content or "-" → run|skip
        ( d=$(mktemp -d); cd "$d" || exit; mkdir skills
          [ "$1" = "-" ] || printf "$1" > skills/_journal.md
          [ "$2" = "-" ] || printf '%s' "$2" > .skill_evolve_last_run
          FORCE_RUN=""; COOLDOWN=86400; LAST_RUN_FILE=.skill_evolve_last_run
          ( eval "$GATE2"; echo run ) | tail -1 | grep -qx run && echo run || echo skip
          cd /; rm -rf "$d" ) 2>/dev/null
    }
    hours_ago() { python3 -c "import datetime as d,sys;print((d.datetime.now(d.timezone.utc)-d.timedelta(hours=float(sys.argv[1]))).strftime('%Y-%m-%dT%H:%MZ'))" "$1"; }
    G_NOW=$(date +%s)
    check "cooldown gate: no file, journal event 2h old -> skip" "$(gate2_case "## $(hours_ago 2) evt-0046 NO-OP\n" -)" "skip"
    check "cooldown gate: no file, journal event 30h old -> run"  "$(gate2_case "## $(hours_ago 30) evt-0046 NO-OP\n" -)" "run"
    check "cooldown gate: no file, no journal -> run"            "$(gate2_case - -)" "run"
    check "cooldown gate: a recent file wins over an old journal" "$(gate2_case "## $(hours_ago 30) evt-0046 NO-OP\n" "$((G_NOW-3600))")" "skip"
    check "cooldown gate: an old file wins over a recent journal" "$(gate2_case "## $(hours_ago 2) evt-0046 NO-OP\n" "$((G_NOW-100000))")" "run"
    check "cooldown gate: saturation tripling applies from the journal alone (30h < 72h)" \
        "$(gate2_case "## $(hours_ago 30) evt-0046 NO-OP\n- evolution_saturation: true\n" -)" "skip"
fi
check "skill_evolve.sh: a cooldown that cannot be applied says so" \
    "$(grep -c 'the cooldown is NOT applied' "$SE")" "1"

# ── wrap-up auto-label: bot issues with only generic labels get agent-self ───
UL_FN=$(awk '/^unlabelled_self_issues\(\) \{/,/^\}/' "$SCRIPT")
if require "evolve.sh unlabelled_self_issues extracted" "$UL_FN"; then
    GENERIC_ISSUE_LABELS=$(grep -m1 '^GENERIC_ISSUE_LABELS=' "$SCRIPT" | cut -d'"' -f2)
    ul_out=$( eval "$UL_FN"; printf '%s' '[
      {"number":1,"createdAt":"2026-10-19T01:00:00Z","labels":[]},
      {"number":2,"createdAt":"2026-10-19T01:00:00Z","labels":[{"name":"bug"}]},
      {"number":3,"createdAt":"2026-10-19T01:00:00Z","labels":[{"name":"agent-self"}]},
      {"number":4,"createdAt":"2026-10-19T01:00:00Z","labels":[{"name":"bug"},{"name":"agent-help-wanted"}]},
      {"number":5,"createdAt":"2026-10-19T01:00:00Z","labels":[{"name":"upstream"}]},
      {"number":6,"createdAt":"2026-10-19T01:00:00Z","labels":[{"name":"shoutout"}]},
      {"number":7,"createdAt":"2026-10-18T23:00:00Z","labels":[]},
      {"number":8,"createdAt":"2026-10-19T01:00:00Z","labels":[{"name":"enhancement"},{"name":"documentation"}]}]' \
      | unlabelled_self_issues 2026-10-19T00:00:00Z | tr '\n' ' ' )
    check "auto-label: only this session's issues with generic or no labels (not shoutout/upstream/planner)" "$ul_out" "1 2 8 "
fi
LA_FN=$(awk '/^GENERIC_ISSUE_LABELS=/{p=1} p{print} p&&/^label_unlabelled_self_issues\(\) \{/{q=1} q&&/^\}/{exit}' "$SCRIPT")
if require "evolve.sh label_unlabelled_self_issues extracted" "$LA_FN"; then
    la_case() { # $1 = exit code of the list call; fake gh on PATH → "list args|edit calls|output"
        ( set +e; d=$(mktemp -d)
          printf '[{"number":7,"createdAt":"2026-10-19T01:00:00Z","labels":[{"name":"bug"}]},{"number":8,"createdAt":"2026-10-19T01:00:00Z","labels":[{"name":"agent-self"}]}]' > "$d/fix"
          printf '#!/bin/sh\ncase "$1 $2" in\n  "issue list") echo "$*" > "%s/list"; cat "%s/fix"; [ %s = 0 ] || echo "HTTP 401: Bad credentials" >&2; exit %s;;\n  "issue edit") echo "$*" >> "%s/edits";;\nesac\n' "$d" "$d" "$1" "$1" "$d" > "$d/gh"
          chmod +x "$d/gh"; PATH="$d:$PATH"
          refresh_gh_token() { :; }
          REPO=o/r; BOT_LOGIN='yoyo[bot]'; SESSION_START_ISO=2026-10-19T00:00:00Z
          eval "$LA_FN"; out=$(label_unlabelled_self_issues)
          printf '%s|%s|%s' "$(cat "$d/list" 2>/dev/null)" "$(tr '\n' ';' < "$d/edits" 2>/dev/null)" "$(echo "$out" | tr '\n' ';')"
          rm -rf "$d" ) 2>/dev/null
    }
    LA=$(la_case 0)
    check "auto-label: only the generic-labelled issue gets agent-self" "$(echo "$LA" | cut -d'|' -f2)" "issue edit 7 --repo o/r --add-label agent-self;"
    check "auto-label: lists the bot's open issues directly, not through search" \
        "$(echo "$LA" | cut -d'|' -f1 | grep -cE -- '--state open --author yoyo\[bot\] ' | tr -d ' ')$(echo "$LA" | cut -d'|' -f1 | grep -c -- '--search')" "10"
    check "auto-label: a failed list labels nothing and shows gh's error" \
        "$(la_case 1 | cut -d'|' -f2-3 | grep -c '^|.*could not list.*HTTP 401')" "1"
fi
check "evolve.sh: the wrap-up runs the auto-label before its commit" \
    "$(awk '/^label_unlabelled_self_issues$/{l=NR} /^commit_scope_gate "\$WRAPUP_SCOPE" wrapup$/{c=NR} END{print (l && c && l<c) ? "yes" : "no"}' "$SCRIPT")" "yes"

# ── synthesize.yml: skip unchanged archives; record spend (#944, #1071) ─────
SYN="$ROOT/scripts/synthesize_state.sh"
SYN_WF="$ROOT/.github/workflows/synthesize.yml"
syn_case() { # $1 = setup commands run in a temp repo dir; then $2 = command → its output, newlines as ' ; '
    ( set +e; d=$(mktemp -d); cd "$d" || exit 1; mkdir -p memory dreams .yoyo
      SYNTH_WORK_DIR="$d/work"; export SYNTH_WORK_DIR
      source "$SYN"; eval "$1"; eval "$2" 2>/dev/null | tr '\n' ';' | sed 's/;/ ; /g; s/ ; $//'
      cd /; rm -rf "$d" )
}
if require "synthesize_state.sh exists" "$( [ -f "$SYN" ] && echo yes)"; then
    SYN_SETUP='echo a > memory/learnings.jsonl; echo b > memory/social_learnings.jsonl; : > dreams/dream_log.jsonl; echo "# Active Learnings" > memory/active_learnings.md; echo "# Active Social Learnings" > memory/active_social_learnings.md'
    check "synthesize plan: first run (no state) synthesizes every non-empty archive" \
        "$(syn_case "$SYN_SETUP" 'synth_plan | grep -E "^(run_|skip)"')" "run_learnings=true ; run_social=true ; run_dreams=false ; skip=false"
    check "synthesize plan: an archive unchanged since its last synthesis is skipped" \
        "$(syn_case "$SYN_SETUP; synth_record learnings \$(synth_sha memory/learnings.jsonl)" 'synth_plan | grep -E "^(run_|skip)"')" "run_learnings=false ; run_social=true ; run_dreams=false ; skip=false"
    check "synthesize plan: a changed archive runs again" \
        "$(syn_case "$SYN_SETUP; synth_record learnings \$(synth_sha memory/learnings.jsonl); echo more >> memory/learnings.jsonl" 'synth_plan | grep -E "^run_learnings"')" "run_learnings=true"
    check "synthesize plan: nothing changed -> the whole job skips" \
        "$(syn_case "$SYN_SETUP; synth_record learnings \$(synth_sha memory/learnings.jsonl); synth_record social \$(synth_sha memory/social_learnings.jsonl)" 'synth_plan | grep -E "^skip"')" "skip=true"
    check "synthesize plan: a manual run (FORCE) synthesizes every non-empty archive" \
        "$(syn_case "$SYN_SETUP; synth_record learnings \$(synth_sha memory/learnings.jsonl)" 'FORCE=true synth_plan | grep -E "^run_"')" "run_learnings=true ; run_social=true ; run_dreams=false"
    check "synthesize record: a second record replaces the key, keeps the others" \
        "$(syn_case 'synth_record learnings aaa; synth_record social bbb; synth_record learnings ccc' 'cat memory/.synthesize_inputs')" "learnings ccc ; social bbb"
    SYN_USAGE='synth_usage_mark learnings; printf "%s\n" "{\"type\":\"tool\"}" "{\"type\":\"usage\",\"cost_usd\":1.5}" >> .yoyo/audit.jsonl; synth_usage_collect learnings ran; synth_usage_mark social; synth_usage_collect social failed'
    check "synthesize spend: one row; usage per step; a step with no usage is no_terminal_emit; skipped steps named" \
        "$(syn_case "$SYN_USAGE" 'synth_spend_row | python3 -c "import json,sys; r=json.load(sys.stdin); print(r[\"phase\"], r[\"steps\"], [u[\"cost_usd\"] for u in r[\"usage_records\"]])"')" "synthesize {'learnings': 'ran', 'social': 'failed, no_terminal_emit', 'dreams': 'skipped'} [1.5]"
    check "synthesize spend: steps that ran but recorded nothing -> usage no_terminal_emit, never zero" \
        "$(syn_case 'synth_usage_mark learnings; synth_usage_collect learnings ran' 'synth_spend_row | python3 -c "import json,sys; r=json.load(sys.stdin); print(r.get(\"usage\"), \"usage_records\" in r)"')" "no_terminal_emit False"
    check "synthesize spend: no step ran -> no row" "$(syn_case '' 'synth_spend_row')" ""
    check "synthesize plan: an unchanged archive whose active file is missing runs again" \
        "$(syn_case "$SYN_SETUP; synth_record learnings \$(synth_sha memory/learnings.jsonl); rm memory/active_learnings.md" 'synth_plan | grep -E "^run_learnings"')" "run_learnings=true"
    SYN_OK='synth_output_ok learnings && echo ok || echo no'
    check "synthesize output: a fresh file with its header counts as regenerated" \
        "$(syn_case 'echo old > memory/active_learnings.md.bak; printf "# Active Learnings\nnew\n" > memory/active_learnings.md' "$SYN_OK")" "ok"
    check "synthesize output: exit 0 but the file was not rewritten (same as backup) is not success" \
        "$(syn_case 'printf "# Active Learnings\nold\n" > memory/active_learnings.md; cp memory/active_learnings.md memory/active_learnings.md.bak' "$SYN_OK")" "no"
    check "synthesize output: a wrong header or an empty file is not success" \
        "$(syn_case 'printf "Here is the file\n" > memory/active_learnings.md' "$SYN_OK") $(syn_case ': > memory/active_learnings.md' "$SYN_OK")" "no no"
    check "synthesize spend: a step that started but never finished is counted as interrupted, with its usage" \
        "$(syn_case 'synth_usage_mark learnings; printf "%s\n" "{\"type\":\"usage\",\"cost_usd\":2}" >> .yoyo/audit.jsonl; synth_claim_interrupted "learnings social"' 'synth_spend_row | python3 -c "import json,sys; r=json.load(sys.stdin); print(r[\"steps\"], [u[\"cost_usd\"] for u in r[\"usage_records\"]])"')" "{'learnings': 'interrupted', 'social': 'not_run', 'dreams': 'skipped'} [2]"
    check "synthesize spend: planned steps that never started -> no row" \
        "$(syn_case 'synth_claim_interrupted "learnings social"' 'synth_spend_row')" ""
    check "synthesize spend: a row that cannot be built says so, never 'no step ran'" \
        "$(syn_case 'synth_usage_mark learnings; synth_usage_collect learnings ran; python3() { return 1; }' 'synth_persist_spend learnings 2>&1')" "Spend sink: FAILED at step build-row ;  row not persisted."
fi
for k in learnings social dreams; do
    check "synthesize.yml: the $k step runs only when its archive changed, with YOYO_AUDIT on" \
        "$(awk -v k="$k" '/^      - name: /{blk=""} {blk=blk $0 "\n"} /^        run: \|/{ if (blk ~ ("run_" k " == .true.") && blk ~ /YOYO_AUDIT: "1"/) n++ } END{print n+0}' "$SYN_WF")" "1"
done
for k in learnings social dreams; do
    check "synthesize.yml: the $k step marks its own key and needs real output to record its hash" \
        "$(grep -c "synth_usage_mark $k\$" "$SYN_WF")$(grep -c "yoyo < \"\$PROMPT\" && synth_output_ok $k; then" "$SYN_WF")" "11"
done
check "synthesize.yml: the spend step is told which steps were planned" \
    "$(grep -c 'synth_persist_spend "\$PLANNED"' "$SYN_WF")" "1"
check "synthesize.yml: the spend row is recorded even when a step failed (always())" \
    "$(grep -A3 'name: Record spend on audit-log' "$SYN_WF" | grep -c "if: always() && steps.check.outputs.skip == 'false'")" "1"
check "synthesize.yml: auto-watch is turned off before the first synthesis step" \
    "$(awk '/name: Turn off auto-watch for synthesis/{t=NR} /name: Synthesize active learnings/{l=NR} END{print (t && l && t<l) ? "yes" : "no"}' "$SYN_WF")$(grep -c "sed -i 's/^auto_watch\[\[:space:\]\]\*=.\*/auto_watch = false/' .yoyo.toml" "$SYN_WF")" "yes1"
check "synthesize.yml: the input-hash state is committed with the active files" \
    "$(grep -c 'dreams/active_dream_arc.md memory/.synthesize_inputs; do' "$SYN_WF")" "1"

# ── synthesize.yml end-to-end: step bodies run against a stub yoyo ──────────
syn_has() { case "$1" in *"$2"*) echo yes ;; *) echo no ;; esac; }
syn_step() { # $1 = step name → "ENV k: v" and "RUN line" from the shipped workflow
    awk -v n="      - name: $1" '
        $0 == n {on=1; next}
        on && /^      - name: /{exit}
        on && /^          [A-Z_]+: /{ sub(/^ +/,""); print "ENV " $0; next }
        on && /^        run: \|/{body=1; next}
        on && body { if ($0 !~ /^          / && $0 != "") exit; sub(/^          /,""); print "RUN " $0 }' "$SYN_WF"
}
syn_run() { # $1 = yoyo exit for learnings, $2 social, $3 dreams → state + spend row
    ( set +e; d=$(mktemp -d); cd "$d" || exit 1; mkdir -p memory dreams .yoyo bin scripts; cp "$SYN" scripts/
      echo a > memory/learnings.jsonl; echo b > memory/social_learnings.jsonl; echo c > dreams/dream_log.jsonl
      export SYNTH_WORK_DIR="$d/work" RUNNER_TEMP="$d"
      printf '#!/bin/sh\nshift; exec "$@"\n' > bin/timeout
      # Stub yoyo: one usage record per call only under YOYO_AUDIT=1 (cost = 1,2,3 by call order); exits $YOYO_RC.
      # Stub yoyo: writes the file its prompt names, starting with the header the prompt
      # gives; one usage record per call only under YOYO_AUDIT=1 (cost 1,2,3 by call order); exits $YOYO_RC.
      cat > bin/yoyo <<'STUB'
#!/bin/sh
p=$(cat)
n=$(($(cat .yoyo/n 2>/dev/null || echo 0)+1)); echo $n > .yoyo/n
[ "$YOYO_AUDIT" = 1 ] && echo "{\"type\":\"usage\",\"cost_usd\":$n}" >> .yoyo/audit.jsonl
f=$(printf '%s\n' "$p" | sed -n 's/.*Write the result to \([^ ]*\.md\)\. Start with:.*/\1/p' | head -1)
h=$(printf '%s\n' "$p" | awk '/Start with:/{getline; print; exit}' | sed 's/^ *//')
[ -n "$f" ] && printf '%s\nbody %s\n' "$h" "$n" > "$f"
exit "$YOYO_RC"
STUB
      chmod +x bin/*; export PATH="$d/bin:$PATH"
      plan=$( . "$SYN"; synth_plan 2>/dev/null )
      for kr in "learnings:$1:Synthesize active learnings" "social:$2:Synthesize active social learnings" "dreams:$3:Synthesize active dream arc"; do
          rc=${kr#*:}; name=${rc#*:}; rc=${rc%%:*}
          step=$(syn_step "$name")
          ( while IFS= read -r l; do
                [ -n "$l" ] || continue; kv=${l#ENV }; k=${kv%%:*}
                v=$(printf '%s' "$kv" | sed -n 's/.*steps\.check\.outputs\.\([a-z_]*\).*/\1/p')
                if [ -n "$v" ]; then export "$k=$(printf '%s\n' "$plan" | sed -n "s/^$v=//p")"
                else export "$k=$(printf '%s' "${kv#*: }" | sed 's/ *#.*//; s/^"//; s/"$//')"; fi
            done <<< "$(printf '%s\n' "$step" | grep '^ENV ' | grep -v 'secrets\.')"
            printf '%s\n' "$step" | sed -n 's/^RUN //p' > step.sh
            YOYO_RC=$rc bash step.sh >/dev/null 2>&1 )
      done
      echo "state=$(tr '\n' ',' < memory/.synthesize_inputs 2>/dev/null)"
      ( . "$SYN"; synth_spend_row ) | python3 -c 'import json,sys; r=json.load(sys.stdin); print("steps=%s" % ",".join("%s:%s" % kv for kv in r["steps"].items())); print("costs=%s" % [u["cost_usd"] for u in r.get("usage_records",[])])' 2>/dev/null || echo "row=none"
      cd /; rm -rf "$d" )
}
E_SA=$(printf 'a\n' | shasum -a 256 | cut -d' ' -f1); E_SB=$(printf 'b\n' | shasum -a 256 | cut -d' ' -f1); E_SC=$(printf 'c\n' | shasum -a 256 | cut -d' ' -f1)
if require "synthesize.yml step bodies extracted" "$(syn_step 'Synthesize active learnings' | grep '^RUN ')"; then
E_OK=$(syn_run 0 0 0); E_MIX=$(syn_run 0 1 0)
check "synthesize e2e: all succeed -> each key records its OWN archive sha" "$(syn_has "$E_OK" "state=dreams $E_SC,learnings $E_SA,social $E_SB,")" "yes"
check "synthesize e2e: each step collects only its own usage record"       "$(syn_has "$E_OK" "costs=[1, 2, 3]")" "yes"
check "synthesize e2e: a failed step records no sha, others do"           "$(syn_has "$E_MIX" "state=dreams $E_SC,learnings $E_SA,")" "yes"
check "synthesize e2e: a failed step is still counted, with its usage"    "$(syn_has "$E_MIX" "steps=learnings:ran,social:failed,dreams:ran")" "yes"
check "synthesize e2e: the failed step's usage is in the row"             "$(syn_has "$E_MIX" "costs=[1, 2, 3]")" "yes"
fi

# ── synthesize: the spend row reaches audit-log (bare origin, one rejected push) ─
syn_sink_case() { # $1 = with-branch|no-branch  [$2 = reject-once]
    ( set +e
      export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t GIT_CONFIG_NOSYSTEM=1
      d=$(mktemp -d) && cd "$d"; export HOME="$d/home" RUNNER_TEMP="$d" && mkdir -p "$HOME"
      git init -q --bare -b main origin.git
      git init -q -b main seed && cd seed
      echo a > tracked.txt && git add tracked.txt && git commit -qm init
      git remote add origin "$d/origin.git" && git push -q origin main
      if [ "$1" = with-branch ]; then
          git checkout -q --orphan audit-log && git rm -rq --cached . && rm -f tracked.txt
          mkdir -p sessions/day-1-x && echo '{}' > sessions/day-1-x/outcome.json
          git add sessions && git commit -qm audit && git push -q origin audit-log
      fi
      if [ "${2:-}" = reject-once ]; then
          printf '#!/bin/sh\n[ -f "%s/rejected" ] && exit 0\ntouch "%s/rejected"; exit 1\n' "$d" "$d" > "$d/origin.git/hooks/pre-receive"
          chmod +x "$d/origin.git/hooks/pre-receive"
      fi
      cd "$d" && git clone -q origin.git work 2>/dev/null && cd work
      echo b > tracked.txt && git add tracked.txt && echo u > untracked.txt
      before="$(git status --porcelain)|$(git ls-files -s)|$(git rev-parse HEAD)"
      export SYNTH_WORK_DIR="$d/w"; source "$SYN"
      synth_usage_mark learnings; echo '{"type":"usage","cost_usd":1.5}' >> "$d/w/a"; SYNTH_AUDIT_FILE="$d/w/a" synth_usage_collect learnings ran
      for _ in $(seq "${3:-1}"); do synth_persist_spend learnings 2>&1; echo "rc=$?"; done
      after="$(git status --porcelain)|$(git ls-files -s)|$(git rev-parse HEAD)"
      [ "$before" = "$after" ] && echo "main=same" || echo "main=CHANGED"
      echo "worktrees=$(git worktree list | wc -l | tr -d ' ')"
      if git --git-dir="$d/origin.git" rev-parse -q --verify refs/heads/audit-log >/dev/null; then
          rows=$(git --git-dir="$d/origin.git" show "audit-log:phases/synthesize/spend.jsonl" 2>/dev/null || true)
          echo "rows=$(printf '%s' "$rows" | grep -c . || true)"; printf '%s\n' "$rows" | sed 's/^/ROW /'
          git --git-dir="$d/origin.git" cat-file -e audit-log:sessions/day-1-x/outcome.json && echo "sessions=kept"
      fi
      cd /; rm -rf "$d" ) 2>&1
}
P_SA=$(syn_sink_case with-branch); P_SD=$(syn_sink_case no-branch); P_SF=$(syn_sink_case with-branch reject-once)
check "synthesize sink: one row on origin/audit-log"           "$(syn_has "$P_SA" 'rows=1')" "yes"
check "synthesize sink: the row is phase synthesize with usage" "$(syn_has "$P_SA" 'ROW {"ts"')$(syn_has "$P_SA" '"phase":"synthesize","steps":{"learnings":"ran"')$(syn_has "$P_SA" '"cost_usd":1.5')" "yesyesyes"
check "synthesize sink: earlier audit-log content kept"         "$(syn_has "$P_SA" 'sessions=kept')" "yes"
check "synthesize sink: main untouched, temp worktree removed"  "$(syn_has "$P_SA" 'main=same')$(syn_has "$P_SA" 'worktrees=1')" "yesyes"
check "synthesize sink: no audit-log branch -> skipped, rc 0, none created" "$(syn_has "$P_SD" 'Spend sink: skipped')$(syn_has "$P_SD" 'rc=0')$(syn_has "$P_SD" 'rows=')" "yesyesno"
P_S2=$(syn_sink_case with-branch "" 2)
check "synthesize sink: two runs -> two rows (appended, not overwritten)" "$(syn_has "$P_S2" 'rows=2')" "yes"
check "synthesize sink: one rejected push -> retry lands one row" "$(syn_has "$P_SF" 'rows=1')$(syn_has "$P_SF" 'FAILED')" "yesno"

# ── social/dream: auto_watch off around the yoyo call, file restored after ───
CM="$ROOT/scripts/common.sh"
WO_FN="$(awk '/^YOYO_WATCH_BACKUP=""$/{p=1} p' "$CM")"
if require "common.sh yoyo_watch_off/restore extracted" "$WO_FN"; then
    wo_case() { # $1 = .yoyo.toml content (printf) or "-" for none → "during|restored-identical"
        ( d=$(mktemp -d); cd "$d" || exit 1; [ "$1" = "-" ] || printf "$1" > .yoyo.toml
          [ -f .yoyo.toml ] && cp .yoyo.toml orig
          eval "$WO_FN"; yoyo_watch_off >/dev/null
          during=$(grep '^auto_watch' .yoyo.toml 2>/dev/null || echo none)
          yoyo_watch_restore
          same=$( [ ! -f orig ] && [ ! -f .yoyo.toml ] && echo n/a || { cmp -s orig .yoyo.toml && echo identical || echo DIFFERENT; } )
          echo "$during|$same"; cd /; rm -rf "$d" )
    }
    check "watch off: auto_watch = true becomes false during the call, file byte-identical after" \
        "$(wo_case 'provider = "x"\nauto_watch = true\n# auto_watch = true in a comment stays\nmodel = "m"\n')" "auto_watch = false|identical"
    check "watch off: no auto_watch key -> file untouched" "$(wo_case 'provider = "x"\n')" "none|identical"
    check "watch off: no .yoyo.toml -> nothing created" "$(wo_case -)" "none|n/a"
fi
for f in social dream; do
    check "$f.sh: auto_watch is off for the yoyo call and restored right after it" \
        "$(awk '/^yoyo_watch_off$/{o=NR} /"\$YOYO_BIN" \\$/{y=NR} /^yoyo_watch_restore$/{r=NR} END{print (o && y && r && o<y && y<r) ? "yes" : "no"}' "$ROOT/scripts/$f.sh")" "yes"
done

# ── backlog convergence (2026-10-28): leftovers as comments; the drain closes ──
check "evolve skill: over 20 open agent-self, residue is a comment on the parent issue" \
    "$(grep -c 'over 20, leftovers go on the parent issue, not into new ones' "$ROOT/skills/evolve/SKILL.md")" "1"
check "evolve.sh: no planner text still says to take the oldest backlog item" \
    "$(grep -c 'take the oldest still-valid item' "$SCRIPT")" "0"
check "evolve.sh: the backlog index shows each issue's comment count (leftovers live there)" \
    "$(grep -cF '\(.comments|length) comments)' "$SCRIPT")" "1"
check "evolve.sh: closing a backlog item as not-planned happens at planning, not as a task" \
    "$(grep -c 'CLOSE it now, at planning, as' "$SCRIPT")" "1"
check "evolve skill: residue cannot go on a parent the same task closes" \
    "$(grep -c 'its residue cannot go there' "$ROOT/skills/evolve/SKILL.md")" "1"
check "evolve.sh: the backlog drain closes the issue with the least work left, not the oldest" \
    "$(grep -c 'spend this slot on CLOSING one:' "$SCRIPT")$(grep -c 'another Part-of slice does not$' "$SCRIPT")$(grep -c 'Draining the oldest' "$SCRIPT")" "110"

check "skill_evolve.sh: diff-scope guard lists a retire by its attic path (-M)" \
    "$(grep -c 'CHANGED_FILES=$(git diff --name-only -M ' "$SE")" "1"

# ── refused wrap-up patches reach the next planner; the responder is read-only ─
# Day 228 (14:52): the issue responder wrote three tests while "probing", the
# wrap-up gate refused them, and nothing ever read the saved patch. Real git: an
# audit-log branch with four sessions; only patches in the newest three are
# listed, with the files they touch.
RP_FN=$(awk '/^refused_patches_block\(\) \{/,/^\}/' "$SCRIPT")
rp_case() { # $1 = ref to pass → the function's output, newlines as ' ; '
    ( set +e; d=$(mktemp -d); cd "$d" || exit 1
      git init -q; git config user.email t@t; git config user.name t
      echo x > f; git add f; git commit -qm base
      git checkout -q -b audit-log
      mkpatch() { printf 'diff --git a/%s b/%s\n--- a/%s\n+++ b/%s\n' "$2" "$2" "$2" "$2" > "$1"; }
      mkdir -p sessions/day-9-20261001T000000Z sessions/day-10-20261002T000000Z \
               sessions/day-11-20261003T000000Z sessions/day-12-20261004T000000Z
      mkpatch sessions/day-9-20261001T000000Z/refused_wrapup.patch src/old.rs     # aged out
      mkpatch sessions/day-11-20261003T000000Z/refused_wrapup.patch src/prompt/x.rs
      echo t > sessions/day-10-20261002T000000Z/transcript.log
      echo t > sessions/day-12-20261004T000000Z/transcript.log
      git add -A; git commit -qm sessions; git checkout -q -
      eval "$RP_FN"; refused_patches_block "$1" | paste -sd';' - | sed 's/;/ ; /g'
      cd /; rm -rf "$d" ) 2>/dev/null
}
if require "refused_patches_block extracted" "$RP_FN"; then
    check "refused patches: only the newest three sessions, with the files touched" \
        "$(rp_case audit-log)" "- sessions/day-11-20261003T000000Z/refused_wrapup.patch: src/prompt/x.rs"
    check "refused patches: a missing audit-log ref lists nothing" "$(rp_case no-such-ref)" ""
    check "refused patches: the planner prompt carries the block" \
        "$(grep -c '^=== REFUSED AT A RECENT WRAP-UP' "$SCRIPT")" "1"
    check "respond prompt: the issue responder is read-only" \
        "$(grep -c 'This phase only replies. Do NOT edit, create or delete repository files' "$SCRIPT")" "1"
fi

# ── main push: retry, and never echo a failure into success ────────────────
PR_FN=$(awk '/^push_main_with_retry\(\) \{/,/^\}/' "$SCRIPT")
if require "push_main_with_retry extracted" "$PR_FN"; then
    pr_case() { # $1 = how many pushes fail  $2 = pull fails? (yes|no)  -> "rc pushes aborts"
        ( set -uo pipefail
          eval "$PR_FN"
          _FAILS="$1"; _PULL="$2"; _PUSHES=0; _ABORTS=0
          git() { case "$1" in
                    pull)   [ "$_PULL" = yes ] && return 1; return 0 ;;
                    rebase) _ABORTS=$((_ABORTS+1)); return 0 ;;
                    push)   _PUSHES=$((_PUSHES+1)); [ "$_FAILS" -gt 0 ] && { _FAILS=$((_FAILS-1)); return 1; }; return 0 ;;
                    *)      return 0 ;; esac; }
          push_main_with_retry >/dev/null; rc=$?
          printf '%s %s %s' "$rc" "$_PUSHES" "$_ABORTS"
        ) 2>/dev/null | tail -1
    }
    check "push: first try lands -> ok, one push"            "$(pr_case 0 no)"  "0 1 0"
    check "push: rejected once, retry lands -> ok, two pushes" "$(pr_case 1 no)"  "0 2 0"
    check "push: rejected twice -> FAILS, two pushes"         "$(pr_case 2 no)"  "1 2 0"
    # A failed rebase must be aborted before pushing, every attempt.
    check "push: pull fails -> rebase aborted each attempt"    "$(pr_case 2 yes)" "1 2 2"
    # Wiring: the caller decides, the outcome names it, and a lost push exits 1.
    check "push: caller branches on the helper"     "$(grep -cF 'if ! push_main_with_retry; then' "$SCRIPT")" "1"
    check "push: run outcome carries PUSH FAILED"   "$(grep -cF 'PUSH FAILED: none of it is on main' "$SCRIPT")" "1"
    check "push: lost push exits non-zero at the end" \
        "$(awk '/=== Day \$DAY complete ===/{p=1} p && /exit 1/{print "yes"; exit}' "$SCRIPT")" "yes"
    check "push: the old fail-open echo is gone" \
        "$(grep -cF 'Push failed (maybe no remote or auth issue)' "$SCRIPT")" "0"

    # The pre-push sweep, banner to its `fi`, in a scratch repo: a tracked file
    # dirtied after the wrap-up gets committed by name; a clean tree is untouched.
    PS_BLOCK=$(awk '/^# ── Pre-push sweep/{p=1} p{print} p && /^fi$/{exit}' "$SCRIPT")
    # The dirtied file is the sweep's one real writer, .yoyo/applied_pattern_keys.txt
    # (20 of 20 bot pre-push commits). `stray` dirties a file outside .yoyo/,
    # which the #951 scope gate must refuse rather than commit.
    ps_case() { # $1 = dirty|clean|stray  -> "<commits after> <clean?> <subject>"
        ( set -uo pipefail
          export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
          d=$(mktemp -d) && cd "$d" || exit 1
          git init -q . && git config user.email t@t && git config user.name t
          mkdir -p .yoyo && echo keys > .yoyo/applied_pattern_keys.txt && echo code > handoff
          git add -A && git commit -qm base
          [ "$1" = dirty ] && : > .yoyo/applied_pattern_keys.txt
          [ "$1" = stray ] && echo EDIT >> handoff
          DAY=197; SESSION_TIME=15:47; SESSION_STAGING=$(mktemp -d)
          eval "$GATE_FN"; eval "$GATE_VARS"
          eval "$PS_BLOCK" >/dev/null
          printf '%s %s %s' "$(git rev-list --count HEAD)" "$([ -z "$(git status --porcelain)" ] && echo clean || echo dirty)" "$(git log -1 --format=%s | cut -c1-40)"
          cd / && rm -rf "$d"
        ) 2>/dev/null | tail -1
    }
    if require "pre-push sweep extracted" "$PS_BLOCK"; then
        check "pre-push sweep: dirty tracked file -> committed by name, tree clean" "$(ps_case dirty)" "2 clean Day 197 (15:47): session-end state reset"
        check "pre-push sweep: clean tree -> untouched"                              "$(ps_case clean)" "1 clean base"
        check "pre-push sweep: file outside .yoyo/ -> refused, not committed"        "$(ps_case stray)" "1 clean base"
    fi
fi

# ── dream.sh: post-agent outcome ─────────────────────────────────────────
# The slice deliberately runs PAST the closing `fi`, through `cycle complete`.
# The no-commit branch ends in `[ "${GITHUB_ACTIONS:-}" = "true" ] && echo …`,
# which returns 1 whenever that variable is unset — i.e. every local run. A
# slice stopping at `fi` therefore reports rc=1 even with the branch's `exit 1`
# DELETED, and the assertion passes vacuously. Mutation-verified both ways:
# with the short slice the delete-exit mutant went green; with this slice it
# reddens "nothing committed -> rc 1" by name.
DS_BLOCK=$(awk '/^# ── Diff-scope guard/{p=1} p{print} p && /^echo "dream: cycle complete"/{exit}' "$DREAM")
ds_case() { # $1=moved|same $2=changed files $3=agent exit -> "<rc>|<gasp>|<reverted>"
    ( set -euo pipefail
      GATES_PASSED=1; GASP_OUTCOME=""; REVERTED=0
      exit_code="$3"; HEAD_BEFORE=aaaaaaa; _m="$1"; _f="$2"
      git() { case "$1 ${2:-}" in
                "rev-parse HEAD")   if [ "$_m" = moved ]; then echo bbbbbbb; else echo aaaaaaa; fi ;;
                "diff --name-only") printf '%s\n' $_f ;;
                "reset --hard")     REVERTED=1 ;;
              esac; }
      # cleanup ends in `exit`, so results escape on fd 3 rather than by return.
      trap 'printf "%s|%s|%s\n" "$?" "$GASP_OUTCOME" "$REVERTED" >&3' EXIT
      eval "$DS_BLOCK"
    ) 3>&1 >/dev/null 2>/dev/null | tail -1
}
if require "dream: outcome block extracted" "$DS_BLOCK"; then
    # The two near-miss guards: a dream that landed must stay landed, however
    # the agent exited. 2026-08-23 committed Day 176 and was then killed by the
    # wall clock; branching on the exit code first would have discarded it.
    check "dream: in-scope commit -> kept, rc 0"             "$(ds_case moved DREAM.md 0)"    "0|dreamed: DREAM.md|0"
    check "dream: in-scope commit + timeout -> still kept"   "$(ds_case moved DREAM.md 124)"  "0|dreamed: DREAM.md|0"
    check "dream: nothing committed -> rc 1"                 "$(ds_case same '' 0)"           "1|failed: agent wrote nothing (exit 0)|0"
    check "dream: nothing committed after error -> rc 1"     "$(ds_case same '' 1)"           "1|failed: agent wrote nothing (exit 1)|0"
    check "dream: out-of-scope commit -> reverted, rc 1"     "$(ds_case moved IDENTITY.md 0)" "1|rejected: diff-scope violation|1"
fi

# ── dream.sh: the cooldown checkpoint is bounded to its own file ─────────
# `git commit` with no pathspec commits the WHOLE INDEX, and the dream prompt
# tells the agent to `git add` its own files — so a kill between its add and its
# commit left those files staged, and they rode the checkpoint out to origin
# past the diff-scope guard, which by then had already run.
CK_BLOCK=$(awk '/# Stage ONLY the stamp/{p=1} p{print} p && /^        fi$/{exit}' "$DREAM")
ck_case() { # $1=junk|clean -> comma-joined files in the resulting commit
    ( set -uo pipefail
      export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
      d=$(mktemp -d) && cd "$d" || exit 1
      git init -q . && git config user.email t@t && git config user.name t
      echo 0 > .dream_last_run && git add . && git commit -qm base
      LAST_RUN_FILE=".dream_last_run"; echo 1789 > "$LAST_RUN_FILE"
      [ "$1" = junk ] && { echo ROGUE > IDENTITY.md; git add IDENTITY.md; }
      eval "$CK_BLOCK" >/dev/null 2>&1
      git show --name-only --format='' HEAD | paste -sd, -
      cd / && rm -rf "$d"
    ) 2>/dev/null | tail -1
}
if require "dream: checkpoint block extracted" "$CK_BLOCK"; then
    check "dream: checkpoint commits only the stamp"            "$(ck_case clean)" ".dream_last_run"
    check "dream: staged out-of-scope file does NOT ride along" "$(ck_case junk)"  ".dream_last_run"
fi

# social.sh's spend reader (#944). Production captures yoyo with `2>&1 | tee`,
# so stdin and stdout are both non-terminal, yoyo auto-enables quiet mode
# (src/cli.rs) and the `↳` usage line never reaches $AGENT_LOG. Run
# 36463335955 (2026-09-28) finished and still printed no_terminal_emit because
# the reader grepped that line. Every case below uses the PRODUCTION shape: an
# AGENT_LOG with no `↳` line, and the spend only in the audit delta.
SOCIAL="$(cd "$(dirname "$0")/.." && pwd)/scripts/social.sh"
SS_FN=$(awk '/^report_social_spend\(\) \{/,/^\}/' "$SOCIAL")
ss_case() { # $1=pre-existing lines (watermark)  $2=lines appended this run, '|'-separated
    ( set -uo pipefail
      d=$(mktemp -d) && cd "$d" || exit 1
      AUDIT_FILE="$d/audit.jsonl"; AGENT_LOG="$d/agent.log"
      printf 'thinking about replies\nposted a comment\n' > "$AGENT_LOG"
      printf '%s' "$1" | tr '|' '\n' > "$AUDIT_FILE"
      AUDIT_BEFORE=$(wc -l < "$AUDIT_FILE" | tr -d '[:space:]')
      [ -n "$2" ] && printf '%s\n' "$2" | tr '|' '\n' >> "$AUDIT_FILE"
      eval "$SS_FN"
      report_social_spend; echo "rc=$?"
      cd / && rm -rf "$d"
    ) 2>/dev/null
}
TOOL='{"type":"tool_call","tool":"bash"}'
U1='{"type":"usage","input_tokens":1200,"output_tokens":340}'
U2='{"type":"usage","input_tokens":900,"output_tokens":55}'
has() { printf '%s' "$1" | grep -qF -- "$2" && echo yes || echo no; }
if require "social: report_social_spend extracted" "$SS_FN"; then
    # Case A — the shape the old `↳` grep got wrong: finished run, no `↳` line.
    A=$(ss_case "$TOOL|$TOOL|" "$TOOL|$TOOL|$U1")
    check "social spend A: usage record reported"             "$(has "$A" "$U1")"                "yes"
    check "social spend A: finished run is not no_terminal_emit" "$(has "$A" no_terminal_emit)"   "no"
    check "social spend A: count stated"                      "$(has "$A" '1 usage record(s)')"  "yes"
    check "social spend A: audit line byte-identical"         "$(printf '%s\n' "$A" | grep -c '^  Spend audit: 3 record(s) this run (line 2 → 5); ')" "1"
    check "social spend A: fail-soft rc"                      "$(has "$A" 'rc=0')"               "yes"
    # Case B — two processes in one run: both records, never only the last.
    B=$(ss_case "$TOOL|" "$TOOL|$U1|$TOOL|$U2")
    check "social spend B: count 2"                           "$(has "$B" '2 usage record(s)')"  "yes"
    check "social spend B: first record kept"                 "$(has "$B" "$U1")"                "yes"
    check "social spend B: second record kept"                "$(has "$B" "$U2")"                "yes"
    # Case C — the real death: tool calls, no terminal emit.
    C=$(ss_case "$TOOL|" "$TOOL|$TOOL")
    check "social spend C: no usage -> no_terminal_emit"      "$(has "$C" no_terminal_emit)"     "yes"
    check "social spend C: absence never rendered as a count" "$(has "$C" 'usage record(s)')"   "no"
    # Case D — watermark: an earlier run's usage record is not this run's.
    D=$(ss_case "$TOOL|$U1|" "$TOOL")
    check "social spend D: pre-watermark usage not reported"  "$(has "$D" "$U1")"                "no"
    check "social spend D: -> no_terminal_emit"               "$(has "$D" no_terminal_emit)"     "yes"
fi

# dream.sh's spend reader (#944, Day 213). Same shape as social's: the function
# is extracted from the shipped script, never transcribed. Output is stderr.
DREAM_SH="$(cd "$(dirname "$0")/.." && pwd)/scripts/dream.sh"
DS_FN=$(awk '/^report_dream_spend\(\) \{/,/^\}/' "$DREAM_SH")
ds_case() { # $1=pre-existing lines (watermark), '|'-separated, or MISSING  $2=lines appended this run
    ( set -euo pipefail
      d=$(mktemp -d) && cd "$d" || exit 1
      DREAM_AUDIT_FILE="$d/audit.jsonl"
      if [ "$1" = MISSING ]; then
          DREAM_AUDIT_BEFORE=0
      else
          printf '%s' "$1" | tr '|' '\n' > "$DREAM_AUDIT_FILE"
          DREAM_AUDIT_BEFORE=$(wc -l < "$DREAM_AUDIT_FILE" | tr -d '[:space:]')
          [ -n "$2" ] && printf '%s\n' "$2" | tr '|' '\n' >> "$DREAM_AUDIT_FILE"
      fi
      eval "$DS_FN"
      report_dream_spend 2>&1; echo "rc=$?"
      cd / && rm -rf "$d"
    ) 2>/dev/null
}
if require "dream: report_dream_spend extracted" "$DS_FN"; then
    # (a) a usage record after the watermark is reported.
    DA=$(ds_case "$TOOL|$TOOL|" "$TOOL|$U1")
    check "dream spend a: usage record reported"               "$(has "$DA" "Spend (this run): $U1")" "yes"
    check "dream spend a: not no_terminal_emit"                "$(has "$DA" no_terminal_emit)"     "no"
    check "dream spend a: delta line"                          "$(has "$DA" '2 record(s) this run (line 2 → 4)')" "yes"
    check "dream spend a: fail-soft rc"                        "$(has "$DA" 'rc=0')"               "yes"
    # (b) only tool-call lines this run -> no_terminal_emit, never a zero.
    DB=$(ds_case "$TOOL|" "$TOOL|$TOOL")
    check "dream spend b: tool calls only -> no_terminal_emit" "$(has "$DB" no_terminal_emit)"     "yes"
    check "dream spend b: absence never rendered as a record"  "$(has "$DB" '"type":"usage"')"     "no"
    # (c) the watermark: an earlier run's usage record is not this run's.
    DC=$(ds_case "$TOOL|$U1|$U2|" "$TOOL")
    check "dream spend c: pre-watermark usage not reported"    "$(has "$DC" "$U1")"                "no"
    check "dream spend c: pre-watermark usage -> no_terminal_emit" "$(has "$DC" no_terminal_emit)" "yes"
    # (d) missing file -> WARNING line, and still no_terminal_emit.
    DD=$(ds_case MISSING "")
    check "dream spend d: missing file -> WARNING"             "$(has "$DD" 'Spend audit: WARNING')" "yes"
    check "dream spend d: missing file -> no_terminal_emit"    "$(has "$DD" no_terminal_emit)"     "yes"
    check "dream spend d: fail-soft rc"                        "$(has "$DD" 'rc=0')"               "yes"
    # Two processes (--fallback): both records, never only the last.
    DE=$(ds_case "$TOOL|" "$U1|$TOOL|$U2")
    check "dream spend e: count 2"                             "$(has "$DE" '2 usage record(s)')"  "yes"
    check "dream spend e: first record kept"                   "$(has "$DE" "$U1")"                "yes"
fi

# ── social.sh durable spend sink (#944, Day 236) — functions extracted from the shipped script ──
# The spend lines above go to the Actions log, which expires. persist_social_spend
# appends one row per run to phases/social/spend.jsonl on the audit-log branch.
# Driven against a local bare repo as `origin`, under the script's own
# `set -euo pipefail`, so a bare non-zero inside the function would kill the case.
SP_FN=$(awk '/^social_spend_row\(\) \{/,/^\}/' "$SOCIAL"; awk '/^persist_social_spend\(\) \{/,/^\}/' "$SOCIAL")
SP_SINK_LINE=$(grep -m1 '^SOCIAL_SPEND_SINK=' "$SOCIAL")
sp_case() { # $1 = runs (1|2)  $2 = usage recs ('' = none)  $3 = with-branch|no-branch  [$4 = reject-once]
    ( set -euo pipefail
      export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
      export GIT_CONFIG_NOSYSTEM=1
      d=$(mktemp -d) && cd "$d"
      export HOME="$d/home" && mkdir -p "$HOME"
      git init -q --bare -b main origin.git
      git init -q -b main seed && cd seed
      echo a > tracked.txt && git add tracked.txt && git commit -qm init
      git remote add origin "$d/origin.git" && git push -q origin main
      if [ "$3" = with-branch ]; then
          git checkout -q --orphan audit-log && git rm -rq --cached . && rm -f tracked.txt
          mkdir -p sessions/day-1-x && echo '{}' > sessions/day-1-x/outcome.json
          git add sessions && git commit -qm audit && git push -q origin audit-log
      fi
      if [ "${4:-}" = reject-once ]; then
          # The first push is refused (as if a concurrent run moved the
          # branch); the function's one fetch+rebase+push retry must land it.
          printf '#!/bin/sh\n[ -f "%s/rejected" ] && exit 0\ntouch "%s/rejected"; exit 1\n' "$d" "$d" \
              > "$d/origin.git/hooks/pre-receive"
          chmod +x "$d/origin.git/hooks/pre-receive"
      fi
      cd "$d" && git clone -q origin.git work 2>/dev/null && cd work
      # A dirty main: a staged edit and an untracked file, which must survive untouched.
      echo b > tracked.txt && git add tracked.txt && echo u > untracked.txt
      before="$(git status --porcelain)|$(git ls-files -s)|$(git rev-parse HEAD)"
      origin_before=$(git --git-dir="$d/origin.git" for-each-ref --format='%(refname) %(objectname)')
      eval "$SP_SINK_LINE"; eval "$SP_FN"
      DAY=236; AGENT_EXIT=0; SESSION_TIME=09:09; SOCIAL_USAGE_RECS="$2"
      i=0
      while [ "$i" -lt "$1" ]; do
          persist_social_spend 2>&1; echo "rc=$?"; i=$((i + 1))
      done
      after="$(git status --porcelain)|$(git ls-files -s)|$(git rev-parse HEAD)"
      [ "$before" = "$after" ] && echo "main=same" || echo "main=CHANGED"
      echo "worktrees=$(git worktree list | wc -l | tr -d ' ')"
      origin_after=$(git --git-dir="$d/origin.git" for-each-ref --format='%(refname) %(objectname)')
      [ "$origin_before" = "$origin_after" ] && echo "origin=same" || echo "origin=moved"
      if git --git-dir="$d/origin.git" rev-parse -q --verify refs/heads/audit-log >/dev/null; then
          rows=$(git --git-dir="$d/origin.git" show "audit-log:$SOCIAL_SPEND_SINK" 2>/dev/null || true)
          echo "rows=$(printf '%s' "$rows" | grep -c . || true)"
          echo "social_rows=$(printf '%s\n' "$rows" | grep -c '"phase":"social"' || true)"
          printf '%s\n' "$rows" | sed 's/^/ROW /'
          git --git-dir="$d/origin.git" cat-file -e audit-log:sessions/day-1-x/outcome.json && echo "sessions=kept"
      fi
      cd "$d/.." && rm -r -f "$d"
    ) 2>&1
}
if require "social: spend sink functions extracted" "$SP_FN" && require "social: sink path extracted" "$SP_SINK_LINE"; then
    # (a) one run -> exactly one row, phase social, the usage record inside it.
    PA=$(sp_case 1 "$U1" with-branch)
    check "social sink a: exactly one row on origin/audit-log"   "$(has "$PA" 'rows=1')"            "yes"
    check "social sink a: the row is phase social"               "$(has "$PA" 'social_rows=1')"     "yes"
    check "social sink a: usage record carried in the row"       "$(has "$PA" "\"usage_records\":[$U1]")" "yes"
    check "social sink a: rc 0"                                  "$(has "$PA" 'rc=0')"              "yes"
    check "social sink a: earlier audit-log content kept"        "$(has "$PA" 'sessions=kept')"     "yes"
    check "social sink a: temp worktree removed"                 "$(has "$PA" 'worktrees=1')"       "yes"
    # (b) two runs -> two rows, appended not overwritten.
    PB=$(sp_case 2 "$U1" with-branch)
    check "social sink b: two runs -> two rows"                  "$(has "$PB" 'rows=2')"            "yes"
    # (c) no usage record -> no_terminal_emit, never a zero.
    PC=$(sp_case 1 "" with-branch)
    check "social sink c: no record -> no_terminal_emit"         "$(has "$PC" '"usage":"no_terminal_emit"')" "yes"
    check "social sink c: absence not written as usage_records"  "$(has "$PC" 'usage_records')"     "no"
    check "social sink c: still one row"                         "$(has "$PC" 'rows=1')"            "yes"
    # (d) origin has no audit-log branch -> skip line, rc 0, origin unchanged.
    PD=$(sp_case 1 "$U1" no-branch)
    check "social sink d: skip line printed"                     "$(has "$PD" 'Spend sink: skipped')" "yes"
    check "social sink d: rc 0"                                  "$(has "$PD" 'rc=0')"              "yes"
    check "social sink d: origin refs unchanged"                 "$(has "$PD" 'origin=same')"       "yes"
    check "social sink d: no branch created"                     "$(has "$PD" 'rows=')"             "no"
    # (e) main's status, index and HEAD byte-identical before/after (dirty main).
    check "social sink e: main untouched (one run)"              "$(has "$PA" 'main=same')"         "yes"
    check "social sink e: main untouched (two runs)"             "$(has "$PB" 'main=same')"         "yes"
    check "social sink e: main untouched (skip)"                 "$(has "$PD" 'main=same')"         "yes"
    # (f) one rejected push -> the single retry lands exactly one row.
    PF=$(sp_case 1 "$U1" with-branch reject-once)
    check "social sink f: rejected push retried, one row"        "$(has "$PF" 'rows=1')"            "yes"
    check "social sink f: no FAILED line after the retry"        "$(has "$PF" 'FAILED')"            "no"
fi

# ── dream.sh durable spend sink (#944, Day 236) — mirror of the social sink above ──
# Functions extracted from the shipped script, never transcribed. dream.sh's
# sink reads DREAM_USAGE_RECS / DAY / exit_code and prints to stderr (captured
# here with 2>&1). Same local bare origin, same dirty main that must survive.
DP_FN=$(awk '/^dream_spend_row\(\) \{/,/^\}/' "$DREAM_SH"; awk '/^persist_dream_spend\(\) \{/,/^\}/' "$DREAM_SH")
DP_SINK_LINE=$(grep -m1 '^DREAM_SPEND_SINK=' "$DREAM_SH")
dp_case() { # $1 = runs (1|2)  $2 = usage recs ('' = none)  $3 = with-branch|no-branch  [$4 = reject-once]
    ( set -euo pipefail
      export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
      export GIT_CONFIG_NOSYSTEM=1
      d=$(mktemp -d) && cd "$d"
      export HOME="$d/home" && mkdir -p "$HOME"
      git init -q --bare -b main origin.git
      git init -q -b main seed && cd seed
      echo a > DREAM.md && git add DREAM.md && git commit -qm init
      git remote add origin "$d/origin.git" && git push -q origin main
      if [ "$3" = with-branch ]; then
          git checkout -q --orphan audit-log && git rm -rq --cached . && rm -f DREAM.md
          mkdir -p sessions/day-1-x && echo '{}' > sessions/day-1-x/outcome.json
          git add sessions && git commit -qm audit && git push -q origin audit-log
      fi
      if [ "${4:-}" = reject-once ]; then
          printf '#!/bin/sh\n[ -f "%s/rejected" ] && exit 0\ntouch "%s/rejected"; exit 1\n' "$d" "$d" \
              > "$d/origin.git/hooks/pre-receive"
          chmod +x "$d/origin.git/hooks/pre-receive"
      fi
      cd "$d" && git clone -q origin.git work 2>/dev/null && cd work
      # A dirty main (the dream's own uncommitted edit + an untracked file):
      # status, index and HEAD must be byte-identical afterwards, so the
      # diff-scope guard and the EXIT trap's stamp-only commit see nothing.
      echo b > DREAM.md && git add DREAM.md && echo u > untracked.txt
      before="$(git status --porcelain)|$(git ls-files -s)|$(git rev-parse HEAD)"
      origin_before=$(git --git-dir="$d/origin.git" for-each-ref --format='%(refname) %(objectname)')
      eval "$DP_SINK_LINE"; eval "$DP_FN"
      DAY=236; exit_code=124; DREAM_USAGE_RECS="$2"
      i=0
      while [ "$i" -lt "$1" ]; do
          persist_dream_spend 2>&1; echo "rc=$?"; i=$((i + 1))
      done
      after="$(git status --porcelain)|$(git ls-files -s)|$(git rev-parse HEAD)"
      [ "$before" = "$after" ] && echo "main=same" || echo "main=CHANGED"
      echo "worktrees=$(git worktree list | wc -l | tr -d ' ')"
      origin_after=$(git --git-dir="$d/origin.git" for-each-ref --format='%(refname) %(objectname)')
      [ "$origin_before" = "$origin_after" ] && echo "origin=same" || echo "origin=moved"
      if git --git-dir="$d/origin.git" rev-parse -q --verify refs/heads/audit-log >/dev/null; then
          rows=$(git --git-dir="$d/origin.git" show "audit-log:$DREAM_SPEND_SINK" 2>/dev/null || true)
          echo "rows=$(printf '%s' "$rows" | grep -c . || true)"
          echo "dream_rows=$(printf '%s\n' "$rows" | grep -c '"phase":"dream"' || true)"
          printf '%s\n' "$rows" | sed 's/^/ROW /'
          git --git-dir="$d/origin.git" cat-file -e audit-log:sessions/day-1-x/outcome.json && echo "sessions=kept"
      fi
      cd "$d/.." && rm -r -f "$d"
    ) 2>&1
}
if require "dream: spend sink functions extracted" "$DP_FN" && require "dream: sink path extracted" "$DP_SINK_LINE"; then
    check "dream sink: path is phases/dream/spend.jsonl"         "$DP_SINK_LINE"                    'DREAM_SPEND_SINK="phases/dream/spend.jsonl"'
    # (a) one run -> exactly one row, phase dream, the usage record and exit inside it.
    QA=$(dp_case 1 "$U1" with-branch)
    check "dream sink a: exactly one row on origin/audit-log"    "$(has "$QA" 'rows=1')"            "yes"
    check "dream sink a: the row is phase dream"                 "$(has "$QA" 'dream_rows=1')"      "yes"
    check "dream sink a: usage record carried in the row"        "$(has "$QA" "\"usage_records\":[$U1]")" "yes"
    check "dream sink a: day and agent exit carried"             "$(has "$QA" '"day":236,"exit":124')" "yes"
    check "dream sink a: rc 0"                                   "$(has "$QA" 'rc=0')"              "yes"
    check "dream sink a: earlier audit-log content kept"         "$(has "$QA" 'sessions=kept')"     "yes"
    check "dream sink a: temp worktree removed"                  "$(has "$QA" 'worktrees=1')"       "yes"
    # (b) two runs -> two rows, appended not overwritten.
    QB=$(dp_case 2 "$U1" with-branch)
    check "dream sink b: two runs -> two rows"                   "$(has "$QB" 'rows=2')"            "yes"
    # (c) no usage record -> no_terminal_emit, never a zero.
    QC=$(dp_case 1 "" with-branch)
    check "dream sink c: no record -> no_terminal_emit"          "$(has "$QC" '"usage":"no_terminal_emit"')" "yes"
    check "dream sink c: absence not written as usage_records"   "$(has "$QC" 'usage_records')"     "no"
    check "dream sink c: still one row"                          "$(has "$QC" 'rows=1')"            "yes"
    # (d) origin has no audit-log branch -> one skip line, rc 0, origin unchanged.
    QD=$(dp_case 1 "$U1" no-branch)
    check "dream sink d: skip line printed"                      "$(has "$QD" 'Spend sink: skipped')" "yes"
    check "dream sink d: rc 0"                                   "$(has "$QD" 'rc=0')"              "yes"
    check "dream sink d: origin refs unchanged"                  "$(has "$QD" 'origin=same')"       "yes"
    check "dream sink d: no branch created"                      "$(has "$QD" 'rows=')"             "no"
    # (e) main's status, index and HEAD byte-identical (what the diff-scope guard reads).
    check "dream sink e: main untouched (one run)"               "$(has "$QA" 'main=same')"         "yes"
    check "dream sink e: main untouched (two runs)"              "$(has "$QB" 'main=same')"         "yes"
    check "dream sink e: main untouched (skip)"                  "$(has "$QD" 'main=same')"         "yes"
    # (f) one rejected push -> the single retry lands exactly one row.
    QF=$(dp_case 1 "$U1" with-branch reject-once)
    check "dream sink f: rejected push retried, one row"         "$(has "$QF" 'rows=1')"            "yes"
    check "dream sink f: no FAILED line after the retry"         "$(has "$QF" 'FAILED')"            "no"
fi
# (g) wiring: the sink is called right after the spend report, on the line
# every exit path passes (the invocation never exits the script), and the
# report hands its records over rather than the sink re-reading the audit file.
check "dream sink g: persist called right after report" \
    "$(grep -A1 -x 'report_dream_spend || true' "$DREAM_SH" | tail -1)" "persist_dream_spend || true"
check "dream sink g: report hands records to the sink" \
    "$(printf '%s' "$DS_FN" | grep -c 'DREAM_USAGE_RECS="$usage_recs"')" "1"

# ── social.sh binary selector (#944, Day 213) — block extracted from the shipped script ──
SEL=$(sed -n '/^YOYO_BIN=""$/,/^fi$/p' scripts/social.sh)
sel_case() { # $1 = older profile, $2 = newer profile
    local d; d=$(mktemp -d)
    ( cd "$d" && mkdir -p target/release target/debug
      touch -d '2026-01-01' "target/$1/yoyo"; touch -d '2026-02-01' "target/$2/yoyo"
      eval "$SEL"; echo "$YOYO_BIN" )
    rm -rf "$d"
}
check "social bin: stale release loses to newer debug" "$(sel_case release debug)" "./target/debug/yoyo"
check "social bin: newer release still wins"          "$(sel_case debug release)" "./target/release/yoyo"

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
