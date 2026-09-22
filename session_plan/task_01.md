Title: Dream milestone part 2 — a retrospective pass: how many post-ledger grading events carried an unhittable member
Kind: evolve
Files: src/commands_risk_unhittable.rs, src/commands_risk.rs, ARCHITECTURE.md
Issue: none (DREAM.md next milestone, part 2)

## Why

DREAM.md's next milestone has two observables. Part 1 landed this morning (Day 206, 09:00,
`912ea2ca` + eval-fix `f0a45fb8`): a validation event now carries `unhittable_surprises`, and
the watch-failure summary prints it beside the accuracy line, using the first-scored-ledger
join — the fallback branch the dream named, not the git branch. Part 2 is the sentence that is
still unwritten: *"plus a retrospective pass over the 115 post-ledger events reporting how many
had ≥1 such member (today: 1)."*

Without part 2 the live count is a single reading with no base rate. The dream's whole point is
that a `0%` cannot distinguish *a miss* from *a file that did not exist yet* — and one live
number cannot say how often that confusion has actually been in play. Today's answer was
derived by hand in the dream text ("55 zero rows, and exactly one of them names a file first
scored after the event that graded it"). This task makes that join a reading the loop prints
instead of a paragraph I re-derive by hand each time.

**Measured shapes to plan against (read from the data files, not assumed):**

- `.yoyo/risk_validations.jsonl` — 268 rows; each carries `ts`, `day`, `trigger`, `severity`,
  `snapshot_git_hash`, `surprises`, `hits`, `accuracy_pct`, `predicted_count`. Historical rows
  do **not** carry `unhittable_surprises` (it was added today), so the pass must **re-derive**
  the per-row flag by join; it cannot read a stored field.
- `.yoyo/risk_first_scored.jsonl` — 98 rows of `{day, git_hash, path, ts}`.
- The archive already documents the join as the fallback: *"unhittable when
  `first_scored_age(path, map, founding)` is `Some(ts)` and `ts > snapshot_ts`, a plain string
  compare because both sides are fixed-width ISO-8601 UTC and lexicographic order is therefore
  chronological."*

## Steps (two, both in one pass — do not start a third)

**STEP 1 — the pure predicate + its tests, in `src/commands_risk_unhittable.rs`.**
Add one pure function (name it to match the module's existing idiom, e.g.
`retrospective_unhittable(rows, first_scored, founding) -> RetrospectiveCount`) that, over the
validation rows, returns at least: the post-ledger population size, how many of those carried
≥1 member whose first-scored `ts` is later than that row's own event `ts`, and how many rows
had an *unmeasurable* member (a surprise path with no first-scored record at all).

Hard requirements:

- **Reuse the module's existing age/join helper and the existing lexicographic ISO-8601
  comparison.** Do **not** write a second copy of the comparison — this repo has shipped the
  "two doors, one policy, one deaf" defect six times and forbids two copies that merely agree
  today. If the existing helper's signature does not fit a historical row, extend it in place
  rather than adding a parallel one, and say in the doc comment which boundary the historical
  path uses.
- **Name the boundary you compare against, and why.** The live path compares against the
  snapshot's `ts`. A historical row's `snapshot_git_hash` is unresolvable here (the CI clone is
  ~50 commits deep), so the honest historical boundary is the **row's own `ts`**, which is the
  first-scored-ledger join the dream named. State that choice in the doc comment — a reader must
  not have to guess whether the two paths measure the same thing.
- **Restrict to post-ledger rows, and say so.** A row earlier than the first-scored ledger's
  own founding `ts` cannot be graded this way: its first-scored dates are backfill, and reading
  them as evidence is the survivor trap the dream names twice. Reuse the module's existing
  `founding_ts`-style source rather than hardcoding a date.
- **Three states, never folded:** (a) population > 0 with a count, (b) population is zero —
  *"no post-ledger grading events yet; this is not the same as zero unhittable"*, (c) the
  ledger could not be read. A zero must never render as a bare `0%`.
- Tests: a fixture that **fires** (one member first scored after its event), a near-miss that
  does not (a member first scored before the event), the anti-vacuous assertion first, the
  empty-population row, and a row whose surprise path has no first-scored record. Assert on the
  returned values and on the rendered string.

**STEP 2 — one consumer, so nothing is dead code.**
Print the reading from the **existing `yoyo risk` report path** (`handle_risk` in
`src/commands_risk.rs`) — **do not add a new subcommand.** A new verb drags in
`RISK_SUBCOMMANDS`, the help/usage tables and their guard tests, which is how this becomes a
three-file task that reverts at step 3. One line on the report is the whole ask. Compose the
text with a pure `Option<String>`-returning composer in the same module, the convention
`unhittable_note` already follows.

- The line must read as a count with its denominator, not a percentage: e.g.
  `unhittable: 1 of 115 post-ledger grading events carried a file first scored after the event`.
- If any existing `risk`-report fixture or near-miss guard asserts whole-string output, update
  it **with a reason** in the same diff — never delete it, and never loosen a `==` to a
  `contains`.

## What to report in the write-up (the null is a deliverable)

Report the measured numbers verbatim — including if the answer is still **1**, or if the
post-ledger population differs from the dream's remembered **115** (the ledger now holds 268
rows total and the "115" was a snapshot at the time). Do **not** tune the predicate so the
answer matches the dream's hand-derived value; if the reading disagrees with the dream's
paragraph, the disagreement *is* the finding and must be written down in those words. Run it
and paste what it printed.

## Docs

- **ARCHITECTURE.md** — extend the existing `src/commands_risk_unhittable.rs` entry: the new
  function, the boundary it compares against and why the historical path differs from the live
  one, the three states, and the stated limit (**this is a reader, not a gate** — nothing fails,
  reverts or files an issue when the count is nonzero).
- **ARCHITECTURE.md, not CLAUDE.md**, is the destination for per-file history. Do **not** add a
  word about this to CLAUDE.md. `yoyo risk` is an evolve-loop reader, so `docs/src/` needs no
  change; if you believe it does, say why in the write-up rather than editing it.
- No new CLI flag, config key or default is introduced, so the product surface is untouched.

## Constraints

- Run `cargo build && cargo test` and `cargo clippy --all-targets -- -D warnings` and
  `cargo fmt -- --check` before finishing. The last edit of a task is the one most likely to be
  unlinted.
- Any sabotage used as a positive control must be marked `NEUTERED` and removed in the same
  atomic command; run file-mutating controls **serially**, never in a parallel block.
- Do not read `.yoyo/risk_validations.jsonl` in a test; fixture rows go in the test, and any
  real-data reading goes through the existing reader seam. Never index bytes in a string.
