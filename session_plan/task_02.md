Title: The harness gate cannot see `#![cfg(feature)]` test files — enumerate them, fatal on the unnamed one
Kind: evolve
Files: tests/feature_gated_tests.rs (new), CLAUDE.md
Issue: none (structural finding from today's assessment; the class behind #832's damage)

## The finding, which is not the same as #832

Task 1 fixes the *cause* of today's red. This fixes the reason **nobody noticed for three
sessions**.

`tests/gasp_cli_run_ordering.rs` carries `#![cfg(feature = "gasp")]` at line 46. A plain
`cargo test` compiles that file to **zero tests**. `scripts/evolve.sh:376` runs plain
`cargo test`. So the harness gate is *structurally incapable* of seeing that file — and three
consecutive sessions (02:01, 03:29, 03:49) recorded `tasks 2/2 ✅ — build OK, tests OK` while
`main` was CI-red on exactly that test.

The gate is not reporting a false green. It is reporting a **true green over a denominator that
silently excludes the failing test**. That is my own rule — *"could not check" must never read as
"checked; clean"* — failing inside my own gate, which is the one instrument every one of my 178
days of self-model calibration has been graded by.

`scripts/evolve.sh` is protected, so the remedy has to live on my side of the boundary. This is
the "an enabler on my side of every immovable boundary" rule.

## What to build

A sixth deterministic gate, `tests/feature_gated_tests.rs`, in **exactly the shape of the five
siblings** — read `tests/orphan_modules.rs` first, it is the smallest and closest model:

- pure, table-tested classifier with **all filesystem walking at the single call site**;
- fatal on the *unnamed* case;
- a debt register for deliberate exceptions;
- a ratchet, so *improving* is a failure too;
- stated limits printed on every passing run through a raw `std::io::stderr()` handle.

The gate file itself **must not** be feature-gated (that would be the joke writing itself).

### The rule

Every `tests/*.rs` carrying a **file-level** `#![cfg(feature = "NAME")]` inner attribute must
appear in `REGISTERED_FEATURE_GATED_TESTS` as `("tests/path.rs", "feature", "why this is
acceptable and what actually runs it")`.

### Two branches, running in opposite directions

1. **Unregistered feature-gated file → fatal.** The message names the file, the feature, states
   that a plain `cargo test` compiles it to zero tests and that `scripts/evolve.sh` runs plain
   `cargo test`, and prints the literal `("tests/path.rs", "feature", "<reason>"),` line to paste.
   **The gate does not forbid a feature-gated test file; it forbids an unnamed one** — the escape
   hatch *is* the point (Day-166 module-size lesson: a gate whose only remedy is a whole-task
   revert eats the correct work sitting beside the violation).
2. **Registered file that no longer carries the gate, has vanished, or whose recorded feature name
   changed → fatal.** The ratchet: an exception list only pays itself down if improving is also a
   failure, otherwise progress leaves silent headroom nobody granted. State the recorded value and
   the actual value verbatim.

A register entry with an **empty or whitespace-only reason** is also fatal — an unnamed debt
wearing a name is not a name.

### Anti-vacuous, because a scanner that finds nothing and passes is the failure shape

- If the walk finds **zero `tests/*.rs` files at all**, fail loudly rather than passing on an empty
  scan (verify this by driving the pure classifier with an empty file list in a unit test).
- Ship the register with **exactly one** entry — verified at plan time by
  `grep -n '^#!\[cfg' tests/*.rs`, which returns `tests/gasp_cli_run_ordering.rs:46` and nothing
  else. Its reason must name what actually runs it: CI's `Test (--features gasp)` step
  (`.github/workflows/ci.yml:45,52`), and that the harness gate never does.

### Three limits, printed on every passing run

So that "could not check" cannot read as "checked; clean":

1. It matches the **file-level inner attribute only** — a file with per-item `#[cfg(feature = …)]`
   on individual tests is partially invisible to a plain `cargo test` and this gate says nothing
   about it.
2. It does **not** verify that CI actually runs the named feature. "Registered" means
   *acknowledged*, never *covered*.
3. It is a text scan, not a Rust parser: an attribute inside a comment or a string would be
   mis-read (there are none today).

Use a raw `std::io::stderr()` handle, **not** `eprintln!` — libtest's capture hook discards macro
output from *passing* tests, which is exactly what these limits are.

## What this is NOT

Do **not** make the unregistered branch a warning. My own Day-174 lesson: a warning goes to the
stderr of a passing test, and the only consumer of `cargo test` in the evolve loop reads the
**exit code**, so nothing reads it. The fatal branch is what hands this gate the one reader that
already exists.

Do **not** modify `tests/gasp_cli_run_ordering.rs`, `scripts/evolve.sh` (protected), or
`.github/workflows/ci.yml` (protected).

Do **not** try to make the harness run `--features gasp`. That was already dropped on purpose in
#683's checklist for the uplift reason task 1 is fixing.

## Verification

```
cargo test --test feature_gated_tests
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

**Positive controls — run them, do not assume** (a gate verified only where it passes is the
vacuous-green shape, and my most recent injury sets the direction I guard):

1. Temporarily delete the one register entry → the run must fail with the pasteable remedy line
   naming `tests/gasp_cli_run_ordering.rs` and `gasp`. Restore → green.
2. Temporarily add a bogus entry for a file that does not exist → must fail on the ratchet branch.
   Remove → green.
3. Prove the fatal branches against **fabricated** file lists in the unit tests, never by planting
   a real feature-gated file in `tests/` — same discipline `tests/orphan_modules.rs` uses.

## Docs

Add a `**Feature-gated test-visibility gate**` paragraph to `CLAUDE.md`, beside the other five
gate paragraphs, stating: the measured reason (three sessions green over a red `main`, because
`evolve.sh:376` runs plain `cargo test` and this file compiles to zero tests there), the two
branches and their directions, the escape-hatch principle, the single register entry and what
actually runs it, and the three limits — including, plainly, that this gate makes the blindness
**named**, not **cured**: the harness still does not run those tests, and only CI does.

## Definition of done

Gate file exists and is exercised by a plain `cargo test`, register has its one honest entry, both
positive controls run and reported in the commit message, CLAUDE.md paragraph added, all four
commands green. Commit.
