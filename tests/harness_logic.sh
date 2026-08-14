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
check "budget: non-numeric → 2700 fallback" "$(budget_left '45m' '')"     "2700"
L=$(budget_left 7200 "$(( $(date +%s) + 2400 ))")
[ "$L" -ge 1190 ] && [ "$L" -le 1200 ] && ok "budget: job deadline clamps (2400-1200 margin)" \
    || bad "budget: job deadline clamps" "expected ~1200, got $L"
L=$(budget_left 300 "$(( $(date +%s) + 99999 ))")
check "budget: smaller of the two wins"     "$L" "300"

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

# ── cron cadence: evolve.yml owns it; docs and the job ceiling must agree ──
# (WF/TMIN are re-derived here rather than reused: this block precedes the
# budget section that also defines them, and an unbound var under `set -u`
# aborted the check silently-ish.)
WF="$(cd "$(dirname "$0")/.." && pwd)/.github/workflows/evolve.yml"
TMIN=$(grep -oE 'timeout-minutes: [0-9]+' "$WF" | grep -oE '[0-9]+' | head -1)
DOC="$(cd "$(dirname "$0")/.." && pwd)/CLAUDE.md"
CRON_H=$(grep -oE "cron: '0 \*/[0-9]+ \* \* \*'" "$WF" | grep -oE '[0-9]+' | tail -1)
if require "evolve cron hour-step extracted" "$CRON_H"; then
    check "CLAUDE.md states the real gap"      "$(grep -cE "flat ${CRON_H}h gap" "$DOC")" "1"
    check "CLAUDE.md states the real runs/day" "$(grep -cE "~$(( 24 / CRON_H ))/day" "$DOC")" "1"
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

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
