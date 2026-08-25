# Mutation Testing

yoyo uses [cargo-mutants](https://github.com/sourcefrog/cargo-mutants) to assess test quality. Mutation testing works by making small changes (mutants) to the source code — flipping conditions, replacing return values, removing function bodies — and checking whether any test catches each change.

**If a mutant survives (no test fails), it means that line of code isn't actually tested.**

## Baseline

As of Day 9, yoyo has **1004 total mutants** across its source files. This number grows as features are added. The mutation testing setup uses a **20% maximum survival rate threshold** — if more than 20% of tested mutants survive, the check fails.

| Metric | Value |
|--------|-------|
| Total mutants | 1004 |
| Threshold | 20% max survival rate |
| Established | Day 9 (2026-03-09) |

## Install cargo-mutants

```bash
cargo install cargo-mutants
```

## Quick start with the threshold script

The easiest way to run mutation testing is with the threshold script:

```bash
# Run with default 20% threshold
./scripts/run_mutants.sh

# Run with a stricter threshold
./scripts/run_mutants.sh --threshold 10

# Just count mutants without running them
./scripts/run_mutants.sh --list

# Test mutants in a specific file only
./scripts/run_mutants.sh --file src/format.rs
```

The script:
1. Runs `cargo mutants` on the project
2. Counts caught vs survived mutants
3. Calculates the survival rate
4. Exits with code 1 if the rate exceeds the threshold
5. Prints surviving mutants on failure so you know what to fix

This makes it easy for maintainers to run locally and could be added to CI by the project owner.

## Run mutation testing directly

From the project root:

```bash
# Run all mutants (this takes a while — several minutes)
cargo mutants

# Show only the surviving mutants (uncaught mutations)
cargo mutants -- --survived

# Run mutants for a specific file
cargo mutants -f src/format.rs

# Run mutants for a specific function
cargo mutants -F "format_cost"
```

## Read the results

After a run, cargo-mutants creates a `mutants.out/` directory with detailed results:

```bash
# Summary
cat mutants.out/caught.txt     # mutants killed by tests ✓
cat mutants.out/survived.txt   # mutants NOT caught — test gaps!
cat mutants.out/timeout.txt    # mutants that caused infinite loops
cat mutants.out/unviable.txt   # mutants that didn't compile
```

Focus on `survived.txt` — each line is a mutation that no test catches. These are the weak spots.

## Configuration

The `.cargo/mutants.toml` file excludes known-acceptable mutants:

- **Cosmetic functions** — ANSI color codes, banner printing, help text
- **Interactive I/O** — functions that read stdin or require a terminal
- **Async API calls** — prompt execution that needs a live Anthropic API

These exclusions keep mutation testing focused on logic that *should* be tested. If you add a new feature with testable logic, make sure it's not excluded.

**The path matters.** cargo-mutants reads `.cargo/mutants.toml`, not a file at the repo root. This config sat at the root from Day 9 to Day 178 and was never read once in that time. If you move it, or start a fresh copy, verify it is actually being loaded — see "Proving the config is read" below.

Exclusions are written as `exclude_re`: a list of regexps matched against the whole line `--list` prints, i.e. `path:line:col: <mutant text>`. Two conventions in this file are load-bearing:

- **Anchor each regex to its source file** with `^src/…\.rs:`. This is what stops an exclusion reaching a module it was never meant to touch — in particular the five modules with mutation readings recorded in `CLAUDE.md`, whose denominators must stay stable or every recorded number becomes incomparable with its own future re-measure.
- **Keep the `\b` word boundaries.** Without them `run_prompt` also swallows `run_prompt_with_changes` and nine other live functions, and `compact_agent` swallows `compact_agent_with_keep`.

## Proving the config is read

Editing the file is the container; being loaded is the payload. `--list` runs no tests, so the check is fast and deterministic — compare a file holding an excluded function against the same file with `--no-config`:

```bash
cargo mutants --list -f src/commands_session.rs | wc -l              # 168
cargo mutants --list --no-config -f src/commands_session.rs | wc -l  # 191
```

If the two counts are equal, the config is **not** being read, whatever the file says. Check the mutant *text* too, not just the count: diff the two lists and confirm the functions that disappeared are the ones you excluded, and that near-miss neighbours (`compact_agent_with_keep`, `usage_print_line`) survived.

## Writing targeted tests

When you find a surviving mutant:

1. Read what the mutation does (e.g., "replace `<` with `<=` in format_cost")
2. Write a test that specifically catches that boundary condition
3. Re-run `cargo mutants -F "function_name"` to verify the mutant is now caught

Example workflow:

```bash
# Find surviving mutants
cargo mutants 2>&1 | grep "SURVIVED"

# Write a test to kill the mutant, then verify
cargo mutants -F "format_cost"
```

## Threshold script for CI

The `scripts/run_mutants.sh` script is designed to be CI-friendly:

```bash
# In a CI pipeline or pre-merge check:
./scripts/run_mutants.sh --threshold 20

# Exit codes:
#   0 = survival rate within threshold (PASS)
#   1 = survival rate exceeds threshold (FAIL)
```

The project owner can add this to CI workflows when ready. For now, contributors should run it locally before submitting PRs that add new logic.

## When to run

Mutation testing is slow — it builds and tests your code once per mutant. Run it:

- After adding a new feature, to verify test coverage
- Before a release, as a quality check
- When you suspect the test suite has gaps
- On specific files with `--file` to keep it fast during development

## Notes for CI integration

The `scripts/run_mutants.sh` script and `.cargo/mutants.toml` config are ready for a human maintainer to wire into CI. A few things to know:

- **Git-dependent tests**: Some tests (e.g. `test_git_branch_returns_something_in_repo`, `test_build_project_tree_runs`, `test_get_staged_diff_runs`) gracefully handle running outside a git repo. cargo-mutants copies source to a temp directory without `.git/`, so these tests skip git-specific assertions when not in a repo.
- **Exclusions were stale for ~169 days, and are now re-resolved**: the config excludes cosmetic/display functions (ANSI colors, banners), interactive I/O (stdin, terminal), and async API calls (needs live Anthropic key) — that policy is unchanged and still sound. What was wrong was everything else. Written on Day 9, the file sat at the repo root where cargo-mutants never looks, used a `[[exclude]] function = "…"` schema that current cargo-mutants rejects outright (`unknown field 'exclude'`), and named six functions that had since moved modules or never existed (`main::collect_multiline`, `main::run_shell_command`, `main::compact_agent`, `main::auto_compact_if_needed`, `cli::print_banner`, `format::Color::fmt`). On Day 178 it was moved to `.cargo/mutants.toml`, rewritten to the `exclude_re` schema, and every name re-resolved against the tree and verified to generate at least one real mutant. An exclusion that matches nothing is worse than no exclusion, because it reads as a considered policy.
- **No previously recorded reading is invalidated by this.** All of them passed `-f` on the command line, which bypasses the config entirely.
- **The script cannot be added to `.github/workflows/` by the agent** (safety rules), but it exits with code 0/1 and is designed for CI use.
