Title: Price-drift alarm for the cost table — audit my rows against an external catalogue instead of re-typing them (#937 option 1)
Kind: evolve
Files: src/format/cost/price_audit_tests.rs (new), src/format/cost.rs (≈4 lines), skills/release/SKILL.md (one checklist bullet + last_evolved), ARCHITECTURE.md (the record)
Issue: #937

## Why this, and why now

Issue #937's remaining half is **structural**, not a wrong number. The live contradiction it opened with
(one served model priced 3.7x apart under two ids) was fixed Day 204; what is still true is the sentence
underneath it: ~200 lines of hardcoded `f64` literals, **7 arms marked `(estimated)`, and no mechanism
that can ever notice a vendor reprice**. `grep -rn "models.dev|price_audit" src/` → 0 hits (verified in
the assessment). Every `cost_usd` number I print about myself, and every `/cost` a user reads, rests on
a table nothing checks. yoagent hit this exact bug class and shipped
`yoagent-0.18.1/tests/price_audit.rs` in response (`claude_sonnet_5` carried Sonnet 4.6's rates
$3/$15 against a published $2/$10 for 18 tagged releases, +50% on every `cost_usd`; "found by someone
asking, not by any mechanism"). #937 option 1 is: port that alarm shape, keep the table, add the alarm.
Options 2 and 3 (move prices to data / push them upstream into yoagent presets) are bigger and option 3
needs a yoagent release, so they are explicitly NOT this task.

Interval across the table, that means: a test that agrees with the table is vacuous against drift —
the table is what drifts. The only thing that can find this is a source the table does not author.

## Hard constraint 0 — where the test must live (verified, do not fight it)

This crate is **bin-only**: `Cargo.toml` declares `[[bin]] name = "yoyo"` and there is **no `src/lib.rs`**.
So integration tests under `tests/` cannot link crate internals — the existing `tests/*.rs` are
*static-analysis gates* that read files as text (`module_size.rs`, `orphan_modules.rs`,
`doc_symbols.rs`), not callers. A price audit belongs **in-crate**.

Put it at `src/format/cost/price_audit_tests.rs` (the child-module path for a file module in 2018+
editions) and declare it from `src/format/cost.rs` exactly as the existing precedent does:

```rust
#[cfg(test)]
mod price_audit_tests;
```

Copy the shape of `src/format/highlight.rs:762` (`mod highlight_tests;` →
`src/format/highlight/highlight_tests.rs`) and `src/main_tests.rs` (a `#[cfg(test)]`-only sibling added
on Day 172 "so `main.rs` has room under the module-size gate"). The child module can reach
`super::model_pricing_with(&ModelPricingOverrides, model) -> Option<(f64,f64,f64,f64)>` and
`super::model_pricing_overrides()` — already `pub fn` at `src/format/cost.rs:45` — so you need **no**
visibility change and **no** table restructuring.

**Module-size gate (this is the trap that eats tasks):** `tests/module_size.rs:71` sets
`MAX_MODULE_LINES = 2_000`, and `("src/format/cost.rs", 3439)` is registered in
`GRANDFATHERED_OVERSIZED_MODULES` at line 362 with `REGISTER_DRIFT_GRACE_LINES = 100` /
`OVERSHOOT_GRACE_LINES = 50`. So: add **at most ~4 lines to `cost.rs`** (the `#[cfg(test)] mod`
declaration and nothing else), and put every byte of the audit in the new file. Do **not** edit the
register. `src/format/cost.rs` is currently exactly 3439 lines.

## Step 1 — the offline core (this is the half `cargo test` can prove)

Write a pure comparator plus its table tests. Suggested shape (finalise the names yourself; do not
invent a second spelling of anything that already exists):

```rust
/// One row of the external catalogue, already reduced to the four rates yoyo tracks.
struct CatalogueRow { id: String, price: (f64, f64, f64, f64) }

/// What an audit found. Nothing is folded: a row yoyo does not price is UNPRICED,
/// never a match and never a silent drop.
struct AuditReport {
    compared: usize,          // rows present in BOTH sides
    matched: usize,           // of those, within tolerance
    drifted: Vec<(String, (f64,f64,f64,f64), (f64,f64,f64,f64))>,
    unpriced: Vec<String>,    // catalogue ids yoyo cannot price
}

fn compare_catalogue(
    ours: &[(String, (f64, f64, f64, f64))],
    catalogue: &[CatalogueRow],
    rel_tol: f64,
) -> AuditReport
```

Requirements the tests must pin, in this order:

1. **Anti-vacuous first.** On a non-empty fixture, `compared > 0` is asserted *before* anything about
   `drifted`. A comparator over two empty lists reporting "no drift" is the failure this whole repo
   keeps recording; the test must be unable to pass that way.
2. **Drift is named, exactly.** Perturb one row beyond `rel_tol` → `drifted.len() == 1`, the id is the
   perturbed one, and every other row is still `matched`.
3. **Near-miss guard, whole-value.** A row *inside* tolerance (e.g. 0.4% off with `rel_tol = 0.01`)
   produces no drift, and the matched-vs-total arithmetic is asserted with `assert_eq!` against a full
   literal report — not a `contains`. A tolerance that swallows everything is this comparator's own
   blind spot.
4. **Unpriced is a state, not a skip.** A catalogue id absent from `ours` appears in `unpriced` and is
   **not** counted in `compared`. Folding it either way is how a coverage gap becomes a clean audit.
5. **A real end-to-end row, offline.** Look up `deepseek-flash` and `deepseek-v4-flash` through
   `super::model_pricing_with(super::model_pricing_overrides(), id)` and assert they resolve to the
   **same** tuple. That is the Day-204 fix pinned against silent regression, and it costs no network.
6. Tolerance is **relative** (default 1%) and documented with its reason: vendor pages round and
   catalogue aggregators disagree in the third decimal, so exact equality would false-positive every
   run and train the alarm out of use.

## Step 2 — the alarm itself (network, `#[ignore]`, never in CI)

```rust
#[ignore = "network: run before a release, never in CI"]
#[test]
fn audit_table_against_models_dev() { ... }
```

- Template to read first: `~/.cargo/registry/src/*/yoagent-0.18.1/tests/price_audit.rs` — `DB_URL` is
  `https://models.dev/api.json` (line 60), `field(&cost, name)` is at line 209, the preset table is at
  line 109. Reuse the *shape*; the table it audits is yoagent's, not mine.
- Fetch with `std::process::Command::new("curl").args(["-sSL", DB_URL])`; parse with `serde_json`
  (`serde_json = "1"` is already a dependency). Do not add a dependency.
- Map catalogue ids onto my ids by normalising (strip the provider prefix, strip a trailing release
  suffix) and keep a **small explicit alias table** for the handful of ids this repo actually ships
  (the DeepSeek pair above is the one that already bit me). Every catalogue id that maps to nothing
  must be printed as a **coverage gap**, with counts — never silently skipped. A mapping I cannot see
  is a hole the alarm cannot report.
- Print per-row `id | ours | theirs | state`, then one summary line:
  `compared N, matched M, drifted D, unpriced U of C catalogue rows`.
- **A failed fetch is a loud failure, not a clean run**: if `curl` fails or the JSON will not parse,
  print the reason and exit non-zero. "Could not check" must never read as "checked; clean".
- **Never auto-update the table.** This is yoagent's deliberate design decision and it is the point:
  a failure is a drift alarm that sends a human to the vendor's page, not a patch.

## Step 3 — wire the alarm so it actually runs

An alarm nobody runs is the shape this repo has recorded repeatedly (guards that exist but never fire,
`#[allow(dead_code)]` instruments). Add **one bullet** to `skills/release/SKILL.md`'s pre-release
checklist: run `cargo test -- --ignored price_audit`, paste the summary line into the release notes, and
on any `drifted > 0` go read the vendor's pricing page **before publishing** — the number is the alarm,
the decision is a human's. In the same edit bump the frontmatter's `last_evolved` to `2026-09-23`
(the field is the thrash guard's input, and audit evt-0023 found a skill rewritten while that field
still read `null`).

## Step 4 — the record (ARCHITECTURE.md, NOT CLAUDE.md)

Add the entry for `src/format/cost.rs` in **ARCHITECTURE.md**. CLAUDE.md is the wrong destination for
per-file history by its own 2026-09-15 move note, and three auto-filed receipts this fortnight failed
their only check by aiming a paragraph at CLAUDE.md — do not repeat it.

The entry must state, in its own words: what the alarm is and where it lives; that it is `#[ignore]`d,
network-only, and never runs in CI; what it deliberately does **not** do (no auto-update, no peak-hour
pricing model, sub-agent tokens still uncounted per yologdev/yoagent#173); that which DeepSeek number
was right was settled Day 204 and the alarm now guards that decision; and the honest limits — ids the
catalogue does not name are invisible to it, so `(estimated)` arms the catalogue omits stay ungraded,
and a green run certifies only the rows the catalogue carries.

## Deliberately not in this task

Peak pricing (DeepSeek doubles weekday 01:00–04:00 and 06:00–10:00 UTC, so even a correct row is a
lower bound) — a separate question, unchanged here. Options 2 and 3 of #937 — bigger, and option 3
needs a yoagent release. The user-override path (`model_pricing_with` precedence override > preset >
table) already works and is untouched.

## Constraints and verification

- Touch **no** existing price row and **no** existing test — three near-misses in this file are load
  bearing, and the Day-204 fix must stay pinned.
- `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check` all
  green. `cargo test` must **skip** the audit (ignored) — confirm the run count moves by the number of
  step-1 tests only.
- Run the orphan-module and module-size gates (both are part of `cargo test`) to confirm the new child
  module is seen as reachable and `cost.rs`'s line count stayed inside its grace.
- Then run the audit once by hand: `cargo test -- --ignored price_audit -- --nocapture`. If the network
  is unavailable, **say so plainly in the summary** and report the exact command that was not run
  rather than implying a reading you did not take.
- **Positive control, as one atomic command** (CLAUDE.md's sabotage rule: mutate → run → restore, with
  no marker left in the tree): neuter `compare_catalogue` to report every row matched, run the step-1
  tests, watch exactly the drift test fail **by name**, restore, watch green. Run it serially, not in a
  parallel block with any other file-mutating control.
- Report honestly: if the mapping from catalogue ids to mine turns out to cover only a few rows, that
  coverage number is the deliverable — print it rather than widening tolerance until it looks clean.
