# Benchmarks

## Goal

yoyo has no published benchmark number yet — while comparable coding agents do
(Claude Mythos: 93.9% SWE-bench, Cursor: 88.1%, Aider: 82.4%). Getting an
external, *comparable* capability number is the single biggest strategic gap in
yoyo's self-assessment. This page tracks the harness/adapter work needed to plug
yoyo into standard benchmark evaluation pipelines. See
[issue #156](https://github.com/yologdev/yoyo-evolve/issues/156).

## What exists now

A **single-case HumanEval adapter** — `scripts/bench/humaneval_one.sh`. It feeds
**one** HumanEval-style Python problem (a function signature + docstring),
**selected by a problem-ID argument**, into yoyo non-interactively, captures the
completion, and **scores it against that problem's canonical HumanEval unit
tests** — printing a clear PASS/FAIL verdict. That's the whole job right now:
benchmark-shaped prompt IN, completion OUT + PASS/FAIL scored against the
canonical tests.

This proves the yoyo → benchmark → score I/O boundary works end to end. It is
deliberately **one problem per run, scored** — there is still **no full dataset**,
**no batch runner**, and **no published pass@1 number** over the 164-problem set.

### How to run it

```bash
# Default problem (has_close_elements), build + run + score:
ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh

# Select a specific problem by its name (first positional arg):
ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh has_close_elements
ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh add

# Reuse a prebuilt binary instead of rebuilding:
YOYO_BIN=./target/release/yoyo ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh add
```

### Problem-ID argument

The **first positional argument** selects which problem to run; with no argument
it defaults to `has_close_elements`, so existing invocations are unchanged.
Passing an unknown problem name prints the list of available problems and exits
non-zero (it never silently passes). Currently supported problems:

| Name                     | HumanEval | What it asks                                      |
| ------------------------ | --------- | ------------------------------------------------- |
| `has_close_elements`     | 0         | Are any two numbers closer than a threshold?      |
| `add`                    | 53        | Return `x + y`.                                    |
| `truncate_number`        | 2         | Return the decimal part of a positive float.       |
| `below_zero`             | 3         | Does a running bank balance ever fall below zero?  |
| `greatest_common_divisor`| 13        | Return the GCD of two integers.                    |

Each problem is **self-contained in the script** — no network fetch — so the
scoring half stays fully offline and deterministic.

### Adding a problem

Each problem is a `case` branch in the script plus a matching entry in the Python
scorer's `CHECKS` dict:

1. Add a branch in the `case "$PROBLEM" in` block that sets `ENTRY_POINT` (the
   function name) and reads `PROBLEM_BODY` (the signature + docstring) from a
   quoted `<<'PROBLEM_EOF'` heredoc.
2. Add the problem's name to the `AVAILABLE_PROBLEMS` string (used by the
   unknown-problem error message).
3. Add a matching key to the `CHECKS` dict in the Python scorer — a list of
   `(args_tuple, expected)` pairs. Args are splatted (`fn(*args)`), so a
   multi-argument problem like `has_close_elements` uses a nested tuple.

Keep problems trivial and canonical (drawn straight from HumanEval) so the
scorer stays deterministic and offline. See
[issue #156](https://github.com/yologdev/yoyo-evolve/issues/156).

The script reads `ANTHROPIC_API_KEY` from the environment (never hardcoded); if
it is unset, the script prints a clear message and exits non-zero instead of
hanging. The captured completion is delimited between `=== yoyo completion ===`
and `=== end ===`, followed by a `=== RESULT: PASS ===` or
`=== RESULT: FAIL ===` verdict.

**Exit-code contract:** `0` = the completion passes all canonical HumanEval/0
checks, `1` = it fails (wrong answer, missing function, or a syntax error in the
completion — a broken completion yields FAIL, never a crashed script).

**python3 requirement:** scoring needs `python3` (stdlib only, no package
installs). If `python3` is absent the script skips scoring gracefully — it prints
`=== scoring skipped: python3 not found ===` and exits `0` rather than
hard-failing on a missing optional tool. Markdown code fences (```` ```python ````)
are stripped defensively before scoring in case a model adds them.

It is a `Kind: evolve` script: it assumes this repo's Rust build and lives under
`scripts/bench/` purely for yoyo's own capability measurement. It is not shipped
to product users. It is kept dependency-light (no python package installs) so a
human can run it in CI or locally without setup.

## Honest status

- ✅ Single HumanEval-style problem: run + capture works end to end.
- ✅ **Scoring** — the captured completion is run against the selected problem's
  canonical HumanEval unit tests and a PASS/FAIL verdict + exit code is produced.
- ✅ **Problem-ID parameterization** — the runner is a real adapter now: pass a
  problem name to select which problem to run (5 encoded so far). Adding a
  problem is a `case` branch + a `CHECKS` entry, no rewrite.
- ⬜ **Batch running** — running a *set* of problems in one invocation and
  reporting per-problem PASS/FAIL. *This is the next step.*
- ⬜ **Dataset loading** — the full HumanEval `jsonl` (164 problems) instead of
  inline problems.
- ⬜ **Aggregate pass@1** reporting over the full dataset.
- ⬜ **SWE-bench / terminal-bench** adapters — these need repository checkout and
  come after multi-problem HumanEval scoring lands.

No aggregate benchmark number is published or implied. When multi-problem scoring
exists and produces a real pass@1 over the full dataset, that number will be
reported here.
