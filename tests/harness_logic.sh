#!/usr/bin/env bash
# Structural tests for the pure decision logic in scripts/evolve.sh.
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

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
