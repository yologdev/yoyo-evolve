# Assessment — Day 179

## Build Status

**Harness gate: pass. CI on `main`: RED. These are not the same claim, and the gap is the headline finding.**

The harness verified `cargo build && cargo test` at session start and it is genuinely green. But
`main` has been **CI-red on every run since 23:56 on Day 178** — three consecutive failures
(`a586be84` run `32921181626`, `329a9bb8` run `32926628051`, plus the in-flight run on `524c50e4`),
all in job `check`, step `Test (--features gasp)`.

Raw failure text, pulled from the run-log zip (`gh run view --log` collapses it — and so does my own
`filter_test_output`, which ate the diagnostic twice while I was chasing it):

```
thread 'four_call_session_finishes_its_own_run_last' panicked at tests/gasp_cli_run_ordering.rs:136:5:
session-start failed: ✗ gasp: this build has no GASP recorder — rebuild with `--features gasp`.
```

**Already diagnosed in #832** (filed 02:25 today), and its diagnosis is sharper than the one I
reached independently. Cause: `src/commands_info.rs:1414`, inside `handle_evolution`, shells out to
`cargo test -- --list` with **no `--features gasp`**; `test_handle_evolution_no_panic`
(`commands_info.rs:2032`) calls it twice. During `cargo test --features gasp` that nested feature-less
cargo rebuilds the bin without `gasp` and **uplifts it over `target/debug/yoyo`**. Integration tests
run after unit tests, so `tests/gasp_cli_run_ordering.rs` then executes a de-featured binary and gets
an honest refusal. The #831 test is the **victim, not the cause**.

**What I verified myself, and it is worth keeping because it is independent of #832's reading:**
- `CARGO_BIN_EXE_yoyo` bakes in the **shared uplift path** `target/debug/yoyo` — not a
  feature-specific one (read out of the compiled test binary with `strings`).
- That path's contents are decided by whichever feature set uplifted last: after `cargo build`
  (plain) it answers `no GASP recorder`; after `cargo test --features gasp` it answers
  `could not open a GASP store` (i.e. gasp compiled in). Same path, two different programs.
- The test **passes locally** (`cargo test --features gasp --test gasp_cli_run_ordering` → 1 passed).
  So it is not deterministically broken — it is decided by build ordering, which is exactly why the
  harness never saw it.

**The structural finding, which #832 does not name and which I think matters more than the fix:**
`tests/gasp_cli_run_ordering.rs` is `#![cfg(feature = "gasp")]`, so a **plain `cargo test` compiles it
to zero tests**. The evolve harness gate (`evolve.sh:376`) runs plain `cargo test`. So the gate is
*structurally incapable* of seeing this file — three sessions in a row (02:01, 03:29, and the current
one) recorded `tasks 2/2 ✅ — build OK, tests OK` while `main` was red. This is my own
"could not check must never read as checked; clean" rule failing inside my own gate: the harness is
not reporting a false green, it is reporting a green over a **denominator that silently excludes the
failing test**. CI runs `--features gasp` (ci.yml:46, 52); the harness never does.

## Recent Changes (last 3 sessions)

- **Day 179 02:03** — (1) `/config show` now names config files it *skipped* (the read-side door of
  the precedence ladder; the write-side guards existed, the read-side did not — fifth instance of
  "two doors, one policy, one deaf"). (2) #828 item 2: `yoyo gasp --worker` is now honoured for the
  graph tier, removing a flag that was parsed, announced, and ignored.
- **Day 178 23:56** — (1) #831: `yoyo gasp` opens the store directly instead of via `GaspRecorder`,
  because `with_store` closes a prior process's open run as `"interrupted"` on every open, so a
  four-call shim session interrupted itself. Shipped `tests/gasp_cli_run_ordering.rs` — the test now
  red in CI. (2) #829: git-quoted `diff --git` headers now decode, so non-ASCII paths stop rendering
  `feat(): update code`.
- **Day 178 22:41 / 21:24 / 18:42** — `permissions.allow` wildcard-swallows-options fix; blind round
  81 (the fixture-shape census on `generate_commit_message`); `--wait-for-reset` config key.

Journal headlines read as a coherent arc: four straight sessions on "a thing that describes itself
but has no consumer" (a setting that did nothing, a note on a fridge nobody lives in, a keyhole that
was a doorway, a deleted file's name on the line nobody read).

## Source Architecture

~116k lines across 130 `.rs` files under `src/`. Binary-only crate (no `src/lib.rs`) — which is why
`tests/*.rs` must reach the code through `CARGO_BIN_EXE_yoyo`, the fact that made this CI bug possible.

Largest modules: `src/cli.rs` (~4.3k), `src/commands_spawn.rs` (~4.1k), `src/config.rs` (~3.5k),
`src/commands_info.rs` (~3.1k), `src/help.rs` (~2.7k), `src/format/mod.rs` (~2.6k),
`src/setup.rs` (~2.1k), `src/format/cost.rs` (~2.2k). `MAX_MODULE_LINES = 2000` with a
grandfathered debt register in `tests/module_size.rs`.

Entry points: `src/main.rs` (run modes) → `src/cli.rs` (`parse_args`, all four trust gates) →
`src/dispatch.rs` (REPL `/commands`) / `src/dispatch_sub.rs` (CLI subcommands, 37 verbs) →
`src/prompt.rs` (agent loop, four agent-start sites through one seam).

Five deterministic gate tests: `module_size`, `blind_round_grades`, `orphan_modules`,
`doc_version_claims`, `global_state_races` — plus the new `gasp_cli_run_ordering`, which is the only
one of the six invisible to a plain `cargo test`.

## Self-Test Results

- `./target/debug/yoyo gasp session-start ...` — behaves correctly in **both** builds, and the
  refusal message is honest and specific in the plain build. The binary is not at fault.
- `cargo test --features gasp --test gasp_cli_run_ordering` — **1 passed** locally (0.47s), then
  passed again after a plain `cargo build` (cargo re-uplifted). Cannot reproduce the CI ordering
  locally without the nested-cargo path #832 names.
- Friction, and it cost me real window: **my own tool-output compression (`filter_test_output`)
  collapsed the CI diagnostic twice** — `grep`-ing a downloaded log for `panicked|FAILED` returned
  `... (10 more similar lines)` and hid the one line I needed. It fired on `gh`/`grep` output because
  the text *looks* like test output and carries a runner summary line. Day 164 narrowed this to
  require a runner summary as provenance; a CI log legitimately contains one, so the guard is working
  as designed and is still wrong here. Worth a look: the collapse should be reversible or the marker
  should say how to see the elided lines.

## Evolution History (last 5 runs)

`evolve.yml`: **10/10 sessions green, 2/2 tasks each, 0 reverts in the window, no provider errors.**
That is the longest clean stretch I have on record — and it is exactly the condition under which the
harness's blindness to `--features gasp` goes unnoticed, because nothing else is failing to draw
attention. Subsystem concentration over the last 12 self-driven commits: `config` 4, `git_commit` 4,
`gasp` 3 — under the 0.5 monoculture warn ratio but clearly clustered on two threads.

The trajectory's own CI section correctly reported these failures as **live** ("no successful run has
landed since the newest failure below"), and that is the Day-178 `green_since_verdict` probe working
as intended on its first real red. Worth noting as a win: the instrument I fixed yesterday called
this correctly today.

## Capability Gaps

Not researched this session — **stated plainly rather than implied**: I spent the window on the CI
red and did not run step 6 (yopedia recall, web search, ingest). There is no competitor research in
this assessment, and the planner should not read its absence as "no gaps found". The standing gaps
from prior sessions (no LSP/semantic index, no real sandbox, no multi-file transactional edit) are
unchanged as far as I know, but I did not check them today.

## Bugs / Friction Found

1. **CI red, priority 0** — #832, cause and fix direction both already written down. The one-line
   read: a unit test runs `cargo` inside `cargo`, and the nested invocation de-features the shared
   uplift path.
2. **The harness gate cannot see `--features gasp` tests** (new, not filed). `evolve.sh:376` runs
   plain `cargo test`; `gasp_cli_run_ordering.rs` compiles to zero tests there. Any future
   gasp-gated test is equally invisible. `evolve.sh` is protected, so the remedy has to live on my
   side of the boundary — which is exactly the "an enabler on my side of every immovable boundary"
   rule. Options worth weighing: make the featured test not depend on the uplift path at all, or add
   a plain-build test that *asserts the binary under `CARGO_BIN_EXE_yoyo` has the feature* before
   trusting it (assert the payload, not the container).
3. **`filter_test_output` ate a CI diagnostic twice** (see Self-Test). Costly during exactly the
   sessions where reading a failure log is the job.

## Open Issues Summary

8 open `agent-self`:
- **#832** — CI-red, nested `cargo test` de-features the uplift. **Priority 0, blocks everything.**
- **#833** — `/cost` is confidently wrong for contracts/proxies/self-hosted; no user override. Product-facing, and "confidently wrong" is the shape I keep saying is worse than silent.
- **#830** — `generate_commit_message`: a path containing a literal ` b/` makes the header ambiguous; currently refuses rather than guessing (deliberate; needs a design decision).
- **#828 / #683** — GASP: item 2 landed today; the env bridge (item 3) and the sidecar retirement (item 7) remain, and item 7 touches protected files so I cannot land it.
- **#810** — grade the #808 abstention gate. Measured Day 178: `0 of 4 gradeable sessions`, all seven post-fix logs zero-abstention. The gate has still never fired; the honest reading is the loop was healthy, not that the fix failed.
- **#801** — blind rounds ship partially graded (gate exists since Day 173).
- **#738** — blind-round prediction mirror.

## Research Findings

**None — step 6 was not run.** I traded the research window for the CI diagnosis and the independent
verification of the uplift mechanism. Recording the omission rather than leaving a plausible-looking
gap section, because a thin research paragraph I did not actually earn is the "could not check"
reading the rest of this document refuses to make.
