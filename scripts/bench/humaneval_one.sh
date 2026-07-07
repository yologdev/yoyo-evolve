#!/usr/bin/env bash
# scripts/bench/humaneval_one.sh — single-case HumanEval-style benchmark adapter.
#
# WHAT THIS IS (retreat size — issue #156):
#   The smallest useful first step toward giving yoyo an external, comparable
#   capability number. It feeds ONE hardcoded HumanEval-style Python problem
#   (function signature + docstring) into yoyo non-interactively and prints the
#   raw completion. That's the whole job: benchmark-shaped prompt IN, model
#   completion OUT. It proves the yoyo→benchmark I/O boundary works end to end.
#
# WHAT THIS IS NOT (the explicit FOLLOW-UP — do not confuse this with a score):
#   - No dataset loading (the full HumanEval jsonl).
#   - No sandboxed test execution / pass@1 scoring.
#   - No multi-problem batch running.
#   - No SWE-bench / terminal-bench (those need repo checkout — later tasks).
#   There is NO published benchmark number yet. This only captures a completion.
#
# USAGE:
#   ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh
#   YOYO_BIN=./target/release/yoyo ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh
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

set -euo pipefail

# --- 0. preconditions -------------------------------------------------------

if [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo "error: ANTHROPIC_API_KEY is not set." >&2
    echo "  Set it in your environment before running this benchmark adapter:" >&2
    echo "  ANTHROPIC_API_KEY=sk-... $0" >&2
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

# --- 1. the one hardcoded HumanEval-style problem ---------------------------
#
# Canonical HumanEval/0 (has_close_elements): short, unambiguous, self-contained.
read -r -d '' PROBLEM <<'PROBLEM_EOF' || true
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

PROMPT="Complete this Python function. Respond with ONLY the code (the full function including its signature and body), no explanation, no markdown fences.

$PROBLEM"

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
# be multi-line, so capture the whole thing (do not assume a single line).

echo "=== yoyo completion ===" 
run_yoyo --prompt "$PROMPT" --output-format text
echo ""
echo "=== end ==="

# --- FOLLOW-UP (not done here) ----------------------------------------------
# The next step toward an actual benchmark number: load the full HumanEval
# dataset (jsonl), run each problem's provided unit tests against the captured
# completion inside a sandbox, and compute pass@1. SWE-bench / terminal-bench
# adapters (which need repo checkout) come after that. See
# docs/src/contributing/benchmarks.md and issue #156 for the plan.
