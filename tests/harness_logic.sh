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
val() { grep -oE "(^|[[:space:]])$1=[0-9]+" "$SCRIPT" | head -1 | cut -d= -f2; }
gate() { grep -oE "session_secs_left\)\" -lt $1 " "$SCRIPT" >/dev/null && echo yes || echo no; }
IMPL=$(val IMPL_TIMEOUT); EVAL=$(val EVAL_TIMEOUT); BFIX=$(val BFIX_TIMEOUT)
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
check "planner slots == harness cap" "$SLOTS" "$CAP"

# ── ordering invariant: functions defined before first use ───────────────
DEF=$(grep -n "^refresh_gh_token()" "$SCRIPT" | cut -d: -f1)
USE=$(grep -n "^\s*refresh_gh_token$" "$SCRIPT" | head -1 | cut -d: -f1)
[ -n "$DEF" ] && [ -n "$USE" ] && [ "$DEF" -lt "$USE" ] \
    && ok "refresh_gh_token defined before first call" \
    || bad "refresh_gh_token order" "def@$DEF use@$USE"
DEF=$(grep -n "^session_filed_issues_section()" "$SCRIPT" | cut -d: -f1)
USE=$(grep -n "FILED_SECTION=\$(session_filed_issues_section)" "$SCRIPT" | head -1 | cut -d: -f1)
[ -n "$DEF" ] && [ -n "$USE" ] && [ "$DEF" -lt "$USE" ] \
    && ok "session_filed_issues_section defined before first call" \
    || bad "session_filed_issues_section order" "def@$DEF use@$USE"

# ── failed-test name extraction (innocence check) ────────────────────────
NAMES=$(printf 'test a::b::c_test ... FAILED\ntest x::ok_one ... ok\ntest d::e ... FAILED\n' \
    | grep -oE '^test [a-zA-Z0-9_:]+ \.\.\. FAILED' | awk '{print $2}' | tr '\n' ' ')
check "innocence check parses FAILED names" "$NAMES" "a::b::c_test d::e "

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
