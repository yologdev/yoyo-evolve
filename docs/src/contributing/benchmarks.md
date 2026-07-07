# Benchmarks

## Goal

yoyo has no published benchmark number yet — while comparable coding agents do
(Claude Mythos: 93.9% SWE-bench, Cursor: 88.1%, Aider: 82.4%). Getting an
external, *comparable* capability number is the single biggest strategic gap in
yoyo's self-assessment. This page tracks the harness/adapter work needed to plug
yoyo into standard benchmark evaluation pipelines. See
[issue #156](https://github.com/yologdev/yoyo-evolve/issues/156).

## What exists now

A **single-case HumanEval adapter** — `scripts/bench/humaneval_one.sh`. It is
the smallest useful first step: it feeds **one** hardcoded HumanEval-style Python
problem (a function signature + docstring) into yoyo non-interactively and prints
the raw completion yoyo produced. That's the whole job right now:
benchmark-shaped prompt IN, model completion OUT.

This proves the yoyo → benchmark I/O boundary works end to end. It is
deliberately **run + capture only** — there is **no scoring** and **no published
number**.

### How to run it

```bash
# Build and run against the one hardcoded problem:
ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh

# Reuse a prebuilt binary instead of rebuilding:
YOYO_BIN=./target/release/yoyo ANTHROPIC_API_KEY=sk-... ./scripts/bench/humaneval_one.sh
```

The script reads `ANTHROPIC_API_KEY` from the environment (never hardcoded); if
it is unset, the script prints a clear message and exits non-zero instead of
hanging. Output is delimited between `=== yoyo completion ===` and `=== end ===`.

It is a `Kind: evolve` script: it assumes this repo's Rust build and lives under
`scripts/bench/` purely for yoyo's own capability measurement. It is not shipped
to product users. It is kept dependency-light (no python package installs) so a
human can run it in CI or locally without setup.

## Honest status

- ✅ Single HumanEval-style problem: run + capture works end to end.
- ⬜ **Scoring** — running each problem's provided unit tests against the
  captured completion in a sandbox and computing pass@1. *This is the next
  step.*
- ⬜ **Dataset loading** — the full HumanEval `jsonl` (164 problems) instead of
  one inline problem.
- ⬜ **Multi-problem batch running** and aggregate reporting.
- ⬜ **SWE-bench / terminal-bench** adapters — these need repository checkout and
  come after HumanEval scoring lands.

No benchmark number is published or implied. When scoring exists and produces a
real pass@1 over the full dataset, that number will be reported here.
