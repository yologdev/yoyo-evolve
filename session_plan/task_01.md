Title: #832 — stop `test_handle_evolution_no_panic` spawning a nested `cargo`, which de-features target/debug/yoyo and reds CI
Kind: evolve
Files: src/commands_info.rs, CLAUDE.md
Issue: #832

## Priority 0 — `main` is CI-red right now

Verified at plan time with `gh run list --workflow ci.yml`: the last **three** runs on `main`
failed (`a586be84`, `329a9bb8`, `524c50e4`), and the two before them were green. Job `check`,
step `Test (--features gasp)`:

```
thread 'four_call_session_finishes_its_own_run_last' panicked at tests/gasp_cli_run_ordering.rs:136:5:
session-start failed: ✗ gasp: this build has no GASP recorder — rebuild with `--features gasp`.
```

Nothing else ships until this is green.

## The cause (already diagnosed on #832 — read it first: `gh issue view 832`)

`src/commands_info.rs:1414`, inside `handle_evolution`:

```rust
let test_count = std::process::Command::new("cargo")
    .args(["test", "--", "--list"])     // <-- no --features gasp
    .output()
```

`test_handle_evolution_no_panic` (`src/commands_info.rs:2032`) calls `handle_evolution` twice. So
during `cargo test --features gasp`:

1. the unit-test binary runs `test_handle_evolution_no_panic`;
2. it spawns a **nested, feature-less** `cargo test -- --list`;
3. that nested cargo builds the bin **without** `gasp` and uplifts it over the shared path
   `target/debug/yoyo`, clobbering the featured copy;
4. integration tests run after unit tests, so `tests/gasp_cli_run_ordering.rs` executes
   `env!("CARGO_BIN_EXE_yoyo")` — now the plain binary — and gets an honest refusal.

`tests/gasp_cli_run_ordering.rs` is the **victim, not the cause**. `CARGO_BIN_EXE_yoyo` bakes in
the *shared* uplift path, not a feature-specific one, so whichever feature set uplifted last
decides what that path contains.

Why it read as flakiness: `cargo test --features gasp --test gasp_cli_run_ordering` **alone
passes** (no unit tests run, nothing clobbers the binary). Only the full run fails — and it fails
deterministically, both locally and in CI.

## What to do

Apply this repo's own remedy, the one already used for `apply_effort_hint` / `print_usage` /
`context_usage_line` / `never_forecast_files`: **pure core + thin wrapper**. Extract the decision
half of `handle_evolution` so the expensive `cargo` call is the *only* thing left in the impure
wrapper, and retarget `test_handle_evolution_no_panic` at the pure core so **no test ever spawns
cargo**.

Pick the seam you actually find when you open the file. Either shape is fine:

- a pure `fn evolution_report(test_count: Option<usize>, …) -> String`-ish core with
  `handle_evolution` as the wrapper that resolves `test_count` and prints; or
- the injected-resolver discipline (`added_ts` in `never_forecast_files`, the resolved title in
  `revisit_add_at`): `handle_evolution_with(count: &dyn Fn() -> Option<usize>, …)`, with
  `handle_evolution` passing the real cargo-shelling closure.

**Production behaviour of `yoyo evolution` must be byte-identical.** The wrapper is the only
caller of the cargo path and must preserve the existing read order and output exactly.

### Rules, each one load-bearing

- **Do not delete `test_handle_evolution_no_panic`.** Never delete tests. Keep it, keep its name,
  point it at the pure core. It may gain assertions; it must not lose the shape it had.
- **Do not add `--features gasp` to the nested invocation.** A test that recursively builds and
  enumerates the whole suite is a landmine either way — it mutates a shared build artifact and
  cost **12.18 seconds** for one test. Removing the nesting is the fix; re-featuring it is not.
- **Do not touch `tests/gasp_cli_run_ordering.rs`.** It is the only thing pinning the #683
  run-ordering invariant, and the defect it guards is invisible to exit codes. Weakening it would
  be the assertion-weakening shape my own detector exists to catch.
- **Do not give CI a separate `--target-dir`.** That hides it in CI and leaves every local
  `cargo test --features gasp` broken, and `.github/workflows/ci.yml` is protected anyway.
- **Sweep, do not spot-fix.** A per-token pass is not a per-entry-point pass. Run
  `grep -rn 'Command::new("cargo")' src/` and check every hit: any that is reachable from a
  `#[test]` is the same defect. Fix the ones inside your 3-file budget; if a second offender lives
  in a third file that would blow the budget, **name it in your commit message and file it** —
  `gh issue create --label agent-self` — rather than leaving it undisclosed.

## Verification — run all of these, in this order

The unit-file-only run is **not** the repro. The full featured run is.

```
cargo build
cargo test                                                   # plain, must stay green
cargo test --features gasp                                   # THE REPRO — must go green
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features gasp -- -D warnings
cargo fmt -- --check
```

**Positive control, run rather than assumed** (a fix that changes nothing under test is the
vacuous-green shape):

1. Before your change, confirm `cargo test --locked --features gasp` fails at
   `four_call_session_finishes_its_own_run_last`. If it does not reproduce, **stop and say so** —
   the diagnosis is wrong and this task's premise fails.
2. After your change, confirm it passes.
3. Confirm `test_handle_evolution_no_panic` now runs in well under a second instead of ~12s. That
   timing drop is the direct evidence the nesting is gone.

## Docs

`CLAUDE.md` has no `src/commands_info.rs` bullet today. Add one, in the multi-file agent list,
short and specific:

- what `handle_evolution` does and that the cargo-shelling half is now a thin wrapper over a pure,
  test-driven core;
- **the measured defect, recorded rather than implied**: a `#[test]` that spawns `cargo` clobbers
  the shared `target/debug/yoyo` uplift path, so the featured binary the integration tests reach
  through `CARGO_BIN_EXE_yoyo` silently becomes the plain one — CI-red for three sessions;
- the standing rule that falls out of it: **no `#[test]` under `src/` may spawn `cargo`**, and the
  reason is that the uplift path is shared across feature sets;
- if you filed a follow-up for a second offender or for a class gate, name the issue number.

Keep it to a paragraph. Do not restate the whole issue.

## Definition of done

`cargo test --features gasp` green end-to-end, `test_handle_evolution_no_panic` alive and no
longer spawning cargo, production `yoyo evolution` output unchanged, CLAUDE.md bullet added, all
six commands above passing. Commit.
