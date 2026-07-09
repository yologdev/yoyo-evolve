#!/usr/bin/env bash
# scripts/bench/humaneval_run.sh — HumanEval-lite AGGREGATE runner (issue #156).
#
# WHAT THIS IS (the aggregation layer — the smallest real advance toward a
# reportable score):
#   It COMPOSES the single-case harness `humaneval_one.sh` (do NOT reinvent the
#   prompt/scoring logic — this runner calls it as a black box), loops over the
#   known problem set, tallies pass/fail, and prints ONE greppable summary line:
#
#       HumanEval-lite: 5/5 passed (100.0%)
#
#   Exit 0 iff every problem passed, non-zero otherwise — so CI/humans get a
#   clean signal.
#
# WHAT THIS IS NOT:
#   - It does NOT re-implement scoring. Each problem's PASS/FAIL is decided by
#     `humaneval_one.sh`'s exit code (0 = pass, non-zero = fail).
#   - It is NOT the full 164-problem HumanEval dataset. This is HumanEval-*lite*
#     over the small inline problem set the single-case harness encodes.
#
# USAGE:
#   ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_run.sh            # all problems
#   ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_run.sh add below_zero  # a subset
#   YOYO_BIN=./target/release/yoyo ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_run.sh
#
# The problem list is read from the SINGLE SOURCE OF TRUTH — the
# `AVAILABLE_PROBLEMS=` line in `humaneval_one.sh` — so the runner never drifts
# from the single-case harness. Do NOT hardcode the list independently.
#
# NOTE: this is a Kind:evolve script — a bench harness for yoyo's own capability
# measurement, run manually with an API key. It is NOT wired into any product
# default, CI gate, or startup path.

set -euo pipefail

# Resolve SCRIPT_DIR the same way humaneval_one.sh does (from $0), so it runs
# anywhere. The single-case harness resolves its own REPO_ROOT/cwd, so we only
# need SCRIPT_DIR to locate it.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

ONE="$SCRIPT_DIR/humaneval_one.sh"
if [ ! -f "$ONE" ]; then
    echo "error: single-case harness not found at $ONE" >&2
    exit 1
fi

# --- resolve the problem set from the single source of truth ----------------
#
# Grep the AVAILABLE_PROBLEMS= line out of humaneval_one.sh and parse the
# space-separated names from between the quotes. This keeps the two scripts in
# lockstep — adding a problem there automatically enrolls it here.
AVAILABLE_LINE="$(grep -E '^AVAILABLE_PROBLEMS=' "$ONE" | head -n1 || true)"
if [ -z "$AVAILABLE_LINE" ]; then
    echo "error: could not find AVAILABLE_PROBLEMS= in $ONE" >&2
    exit 1
fi
# Strip everything up to the first quote and the trailing quote, leaving the names.
AVAILABLE_PROBLEMS="${AVAILABLE_LINE#*\"}"
AVAILABLE_PROBLEMS="${AVAILABLE_PROBLEMS%\"*}"
if [ -z "${AVAILABLE_PROBLEMS// /}" ]; then
    echo "error: parsed an empty problem list from $ONE" >&2
    exit 1
fi

# --- select which problems to run -------------------------------------------
#
# With no args, run all available problems. With positional args, run that
# subset (validated against the available set so a typo fails loudly).
if [ "$#" -gt 0 ]; then
    PROBLEMS=("$@")
    for p in "${PROBLEMS[@]}"; do
        case " $AVAILABLE_PROBLEMS " in
            *" $p "*) : ;;
            *)
                echo "error: unknown problem '$p'." >&2
                echo "  available problems: $AVAILABLE_PROBLEMS" >&2
                exit 1
                ;;
        esac
    done
else
    # shellcheck disable=SC2206  # deliberate word-splitting of the space list
    PROBLEMS=($AVAILABLE_PROBLEMS)
fi

TOTAL="${#PROBLEMS[@]}"
if [ "$TOTAL" -eq 0 ]; then
    echo "error: no problems selected." >&2
    exit 1
fi

echo "=== HumanEval-lite aggregate runner: $TOTAL problem(s) ===" >&2
echo "    problems: ${PROBLEMS[*]}" >&2

# --- run each problem, tallying pass/fail -----------------------------------
#
# Each problem's verdict comes solely from humaneval_one.sh's exit code:
#   0        = PASS
#   1        = FAIL (wrong answer / broken completion — a real answer verdict)
#   other    = the harness itself errored (crash, missing key, build failure).
# We count both 1 and other as "not passed", but track harness errors distinctly
# so a harness bug isn't silently scored as a wrong answer.
PASSED=0
FAILED=0
ERRORED=0
FAILED_NAMES=()
ERRORED_NAMES=()

for problem in "${PROBLEMS[@]}"; do
    echo "" >&2
    echo "--- running: $problem ---" >&2
    # Stream the single-case output to stderr so it's visible, keep stdout for
    # the tally. Disable -e around the call so a non-zero exit is a verdict, not
    # a crash of the aggregate runner.
    set +e
    bash "$ONE" "$problem" >&2
    rc=$?
    set -e

    if [ "$rc" -eq 0 ]; then
        PASSED=$((PASSED + 1))
        echo "--- $problem: PASS ---" >&2
    elif [ "$rc" -eq 1 ]; then
        FAILED=$((FAILED + 1))
        FAILED_NAMES+=("$problem")
        echo "--- $problem: FAIL (wrong answer) ---" >&2
    else
        ERRORED=$((ERRORED + 1))
        ERRORED_NAMES+=("$problem")
        echo "--- $problem: HARNESS ERROR (exit $rc) ---" >&2
    fi
done

# --- percentage (float, product-safe) ---------------------------------------
#
# Use python3 for a clean one-decimal percentage; degrade to integer bash
# arithmetic if python3 is absent (matches the single-case harness's posture).
if command -v python3 >/dev/null 2>&1; then
    PCT="$(python3 -c "print(f'{100.0 * $PASSED / $TOTAL:.1f}')")"
else
    # Integer percent fallback (no float division available).
    PCT="$(( 100 * PASSED / TOTAL ))"
fi

# --- summary line (greppable, on stdout) ------------------------------------
echo "HumanEval-lite: ${PASSED}/${TOTAL} passed (${PCT}%)"

# Surface non-passing detail on stdout too, but keep it distinct from the
# headline so a harness bug is never silently scored as a wrong answer.
if [ "$FAILED" -gt 0 ]; then
    echo "  failed (wrong answer): ${FAILED_NAMES[*]}"
fi
if [ "$ERRORED" -gt 0 ]; then
    echo "  harness errored (NOT scored as wrong answer): ${ERRORED_NAMES[*]}"
fi

# Exit 0 only if every problem passed; otherwise non-zero for a clean CI signal.
if [ "$PASSED" -eq "$TOTAL" ]; then
    exit 0
else
    exit 1
fi
