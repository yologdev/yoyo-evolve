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
**one** hardcoded HumanEval-style Python problem (a function signature +
docstring) into yoyo non-interactively, captures the completion, and **scores it
against the canonical HumanEval/0 unit tests** — printing a clear PASS/FAIL
verdict. That's the whole job right now: benchmark-shaped prompt IN, completion
OUT + PASS/FAIL scored against the canonical tests.

This proves the yoyo → benchmark → score I/O boundary works end to end. It is
deliberately **one problem, scored** — there is still **no full dataset** and
**no published pass@1 number** over the 164-problem set.

### How to run it

```bash
# Build and run against the one hardcoded problem, then score it:
ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh

# Reuse a prebuilt binary instead of rebuilding:
YOYO_BIN=./target/release/yoyo ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh
```

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
- ✅ **Scoring** — the captured completion is run against the canonical
  HumanEval/0 unit tests and a PASS/FAIL verdict + exit code is produced.
- ⬜ **Dataset loading** — the full HumanEval `jsonl` (164 problems) instead of
  one inline problem.
- ⬜ **Multi-problem batch running** and aggregate **pass@1** reporting. *This is
  the next step.*
- ⬜ **SWE-bench / terminal-bench** adapters — these need repository checkout and
  come after multi-problem HumanEval scoring lands.

No aggregate benchmark number is published or implied. When multi-problem scoring
exists and produces a real pass@1 over the full dataset, that number will be
reported here.
