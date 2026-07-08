#!/usr/bin/env bash
# scripts/bench/humaneval_one.sh — single-case HumanEval-style benchmark adapter.
#
# WHAT THIS IS (run → score, parameterized by problem — issue #156):
#   The smallest useful benchmark artifact toward giving yoyo an external,
#   comparable capability number. It feeds ONE HumanEval-style Python problem
#   (function signature + docstring) — selected by a problem-ID argument — into
#   yoyo non-interactively, captures the completion, and SCORES it against that
#   problem's canonical HumanEval unit tests — printing a clear PASS/FAIL verdict
#   and setting the exit code (0 = pass, 1 = fail). Prompt IN, scored verdict OUT.
#
#   The problem is a real *adapter* now (issue #156, Day 130): pass a problem name
#   as the first positional arg to select which problem to run. With no args it
#   defaults to `has_close_elements` (HumanEval/0), so old invocations are
#   unchanged. A small in-script lookup holds each problem's prompt AND its
#   canonical checks — no network fetch, so the runner stays offline/deterministic
#   for the scoring half. Adding a problem = adding one case branch here.
#
# WHAT THIS IS NOT (the explicit FOLLOW-UP — a small set, not a full run):
#   - No dataset loading (the full HumanEval jsonl, 164 problems).
#   - No multi-problem batch running / aggregate pass@1 in ONE invocation.
#   - No SWE-bench / terminal-bench (those need repo checkout — later tasks).
#   There is NO published benchmark number yet. This scores ONE problem per run.
#
# USAGE:
#   ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh                 # default: has_close_elements
#   ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh has_close_elements
#   ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh add
#   YOYO_BIN=./target/release/yoyo ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh add
#
# PROBLEMS (each self-contained; add one by extending the case blocks below):
#   has_close_elements  — HumanEval/0  (any two numbers closer than a threshold?)
#   add                 — HumanEval/53 (return x + y)
#
# ENV:
#   ANTHROPIC_API_KEY  — required (read from env, never hardcoded).
#   YOYO_BIN           — optional path to a prebuilt yoyo binary. If unset, the
#                        script builds/uses `cargo run --release --`.
#
# NOTE: this is a Kind:evolve script — it assumes this repo's Rust build and
# lives under scripts/bench/ purely for yoyo's own capability measurement. It is
# not shipped to product users. Kept POSIX-flavored bash + dependency-light (no
# python package installs) so a human can run it in CI or locally without setup.
# The scoring step needs only python3 (stdlib); if python3 is absent it skips
# scoring gracefully (exit 0) rather than hard-failing on a missing tool.

set -euo pipefail

# --- problem selection ------------------------------------------------------
#
# First positional arg selects the problem; default keeps old behavior exactly.
PROBLEM="${1:-has_close_elements}"

# Available problem names (kept in sync with the case blocks below). Used for the
# error message when an unknown name is passed — fail loudly, never silently pass.
AVAILABLE_PROBLEMS="has_close_elements add"

# The per-problem PROMPT_BODY (function signature + docstring) and ENTRY_POINT
# (the function name the scorer looks up) are set here. The canonical checks live
# in the Python scorer, keyed by the same ENTRY_POINT.
case "$PROBLEM" in
    has_close_elements)
        ENTRY_POINT="has_close_elements"
        read -r -d '' PROBLEM_BODY <<'PROBLEM_EOF' || true
from typing import List


def has_close_elements(numbers: List[float], threshold: float) -> bool:
    """ Check if in given list of numbers, are any two numbers closer to each other than
    given threshold.
    >>> has_close_elements([1.0, 2.0, 3.0], 0.5)
    False
    >>> has_close_elements([1.0, 2.8, 3.0, 4.0, 5.0, 2.0], 0.3)
    True
    """
PROBLEM_EOF
        ;;
    add)
        ENTRY_POINT="add"
        read -r -d '' PROBLEM_BODY <<'PROBLEM_EOF' || true
def add(x: int, y: int) -> int:
    """Add two numbers x and y
    >>> add(2, 3)
    5
    >>> add(5, 7)
    12
    """
PROBLEM_EOF
        ;;
    *)
        echo "error: unknown problem '$PROBLEM'." >&2
        echo "  available problems: $AVAILABLE_PROBLEMS" >&2
        echo "  usage: $0 [problem-name]" >&2
        exit 1
        ;;
esac

# --- 0. preconditions -------------------------------------------------------

if [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo "error: ANTHROPIC_API_KEY is not set." >&2
    echo "  Set it in your environment before running this benchmark adapter:" >&2
    echo "  ANTHROPIC_API_KEY=sk-... $0 $PROBLEM" >&2
    exit 1
fi

# Resolve repo root from this script's location so it runs from anywhere.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

# Scratch dir for anything we need to write; cleaned up on exit.
WORK_DIR="$(mktemp -d)"
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

# --- 1. build the prompt for the selected problem ---------------------------

echo "=== problem: $PROBLEM (entry point: $ENTRY_POINT) ===" >&2

PROMPT="Complete this Python function. Respond with ONLY the code (the full function including its signature and body), no explanation, no markdown fences.

$PROBLEM_BODY"

# --- 2. resolve the yoyo invocation -----------------------------------------

if [ -n "${YOYO_BIN:-}" ]; then
    if [ ! -x "$YOYO_BIN" ]; then
        echo "error: YOYO_BIN='$YOYO_BIN' is not an executable file." >&2
        exit 1
    fi
    echo "=== using prebuilt binary: $YOYO_BIN ===" >&2
    run_yoyo() { "$YOYO_BIN" "$@"; }
else
    echo "=== building yoyo (cargo build --release) ===" >&2
    cargo build --release >&2
    run_yoyo() { ./target/release/yoyo "$@"; }
fi

# --- 3. invoke yoyo non-interactively on the problem ------------------------
#
# Uses the existing --prompt / --output-format text piped path. Completion may
# be multi-line, so capture the whole thing (do not assume a single line). We
# tee it into a file so we can both show it and score it.

RAW_COMPLETION="$WORK_DIR/completion_raw.py"
COMPLETION="$WORK_DIR/completion.py"

echo "=== yoyo completion ==="
run_yoyo --prompt "$PROMPT" --output-format text | tee "$RAW_COMPLETION"
echo ""
echo "=== end ==="

# --- 4. defensively strip markdown fences -----------------------------------
#
# The prompt asks for no fences, but models sometimes wrap code in
# ```python ... ```. Drop any line that is a bare triple-backtick fence
# (optionally with a language tag) before scoring. We don't assume clean output.

grep -v -E '^[[:space:]]*```' "$RAW_COMPLETION" > "$COMPLETION" || cp "$RAW_COMPLETION" "$COMPLETION"

# --- 5. score against the selected problem's canonical tests -----------------
#
# Gate on python3: if it's missing, skip scoring gracefully (exit 0) rather than
# hard-failing on a missing optional tool (product-safe posture).

if ! command -v python3 >/dev/null 2>&1; then
    echo "=== scoring skipped: python3 not found ==="
    echo "  (completion captured above; install python3 to enable PASS/FAIL scoring)"
    exit 0
fi

# The scorer execs the (fence-stripped) completion, then runs the canonical
# checks for the selected ENTRY_POINT. A syntax error, missing function, or
# failed assert all yield a non-zero exit → FAIL, never a crashed script. The
# heredoc is quoted (<<'PY') so bash does not expand $ inside the Python; the
# entry point is passed as an argv rather than interpolated into the source.

echo "=== scoring against canonical tests for '$ENTRY_POINT' ==="

# Temporarily disable -e so a scorer failure becomes a FAIL verdict, not a crash.
set +e
python3 - "$COMPLETION" "$ENTRY_POINT" <<'PY'
import sys

completion_path = sys.argv[1]
entry_point = sys.argv[2]

with open(completion_path, "r", encoding="utf-8") as fh:
    source = fh.read()

ns = {}
try:
    exec(compile(source, completion_path, "exec"), ns)
except Exception as exc:  # syntax error, import error, etc.
    print(f"  completion failed to load: {type(exc).__name__}: {exc}")
    sys.exit(1)

fn = ns.get(entry_point)
if not callable(fn):
    print(f"  completion did not define {entry_point}()")
    sys.exit(1)

# Canonical checks per problem, keyed by entry point. Each entry is a list of
# (args_tuple, expected) pairs — args are splatted into the function call.
CHECKS = {
    # HumanEval/0
    "has_close_elements": [
        (([1.0, 2.0, 3.9, 4.0, 5.0, 2.2], 0.3), True),
        (([1.0, 2.0, 3.9, 4.0, 5.0, 2.2], 0.05), False),
        (([1.0, 2.0, 3.0], 0.5), False),
        (([1.0, 2.8, 3.0, 4.0, 5.0, 2.0], 0.3), True),
        (([1.1, 2.2, 3.1, 4.1, 5.1], 1.0), True),
        (([1.1, 2.2, 3.1, 4.1, 5.1], 0.5), False),
    ],
    # HumanEval/53
    "add": [
        ((0, 1), 1),
        ((1, 0), 1),
        ((2, 3), 5),
        ((5, 7), 12),
        ((7, 5), 12),
        ((-1, 1), 0),
        ((100, 200), 300),
    ],
}

checks = CHECKS.get(entry_point)
if checks is None:
    print(f"  no canonical checks registered for {entry_point}")
    sys.exit(1)

for args, expected in checks:
    try:
        got = fn(*args)
    except Exception as exc:
        print(f"  raised on {args}: {type(exc).__name__}: {exc}")
        sys.exit(1)
    if got != expected:
        print(f"  wrong: {entry_point}{args} == {got!r}, expected {expected!r}")
        sys.exit(1)

print(f"  all {len(checks)} canonical checks passed")
sys.exit(0)
PY
SCORE_RC=$?
set -e

if [ "$SCORE_RC" -eq 0 ]; then
    echo "=== RESULT: PASS ==="
    exit 0
else
    echo "=== RESULT: FAIL ==="
    exit 1
fi

# --- FOLLOW-UP (not done here) ----------------------------------------------
# The next step toward a real benchmark number: run a *set* of problems in one
# invocation (batch), then load the full HumanEval dataset (jsonl, 164 problems)
# and compute aggregate pass@1. SWE-bench / terminal-bench adapters (which need
# repo checkout) come after that. See docs/src/contributing/benchmarks.md and
# issue #156 for the plan.
