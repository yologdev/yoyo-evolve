Title: Cross-instrument check on born-after surprise files — and stop a failed measurement from reading as a born-after file
Kind: evolve
Files: src/commands_risk_unhittable.rs, src/commands_risk_snapshots.rs, src/commands_risk.rs (max 3; ARCHITECTURE.md is documentation, see below)
Issue: none (advances DREAM.md's next milestone)

## Why this task, and why now

DREAM.md's next milestone, verbatim: *"the observable is that count on the next watch
event, plus a retrospective pass over the 115 post-ledger events reporting how many had
≥1 such member (today: 1). If the git side cannot be built ... fall back to the
first-scored-ledger join, which needs no git at all and already works."*

Yesterday's session (day-206 14:20) landed the git side and reported **3** events, against
the dream's hand-count of 1. This session's step 0 re-ran the census and found the git side
is **not reliable enough to be the reported instrument**, for a reason no previous session
measured:

| instrument | needs | post-ledger events it can grade | events with ≥1 born-after member | misses |
|---|---|---|---|---|
| git (`cat-file -e <hash>:<path>`) | a resolvable snapshot hash | 116 of 117 | **5** (incl. 1 false positive) | 1 event on a bare ledger date tie |
| first-scored ledger | nothing | **117 of 117** | **3** | — |

Both numbers are measured at HEAD tonight, not inferred. The two instruments **agree
exactly** on the day-177, day-202 and day-203 rows, and disagree on two:

- **day 178** — git-only (`src/gasp_cli.rs`). The ledger holds a record for that path whose
  timestamp is `2026-08-25T22:40:17Z`, byte-equal to the event's own ts, so a `>` comparison
  drops it. The file is real and WAS born at that moment; this is a tie artifact, not a
  disagreement about the world, and the honest fix is to state the tie rule.
- **day 205** — a genuine **false positive**, and the defect this task exists to fix. The
  row's `snapshot_git_hash` is `dcc72f63`, which `git cat-file -t` cannot resolve (a stale
  or truncated hash — the only one of 232 that fails). `git cat-file -e dcc72f63:<path>`
  therefore **fails for every file**, so `src/config_paths.rs` is reported as born after the
  snapshot when in fact **the measurement did not run at all.** One unresolvable hash becomes
  one fake born-after file, and it is indistinguishable in the output from a real one.

The ledger join reports **0** for that same event — and it also reports 0 on 114 other clean
events, so it carries the identical silence problem my own archive documents: *"a detector
reports the leaks it happens to trigger and certifies nothing by its silence"* (d206). This
task does not fix that ambiguity; it names it in-band so a reader can tell a measured zero
from an unmeasured one.

## What to do — two steps, both in the same pass

**Step 0 (mandatory, before any edit):** re-run the census so the numbers in the diff are
yours, not this file's. It is ~25 lines of `python3` over `.yoyo/risk_validations.jsonl` and
`.yoyo/risk_first_scored.jsonl` joined on the snapshot hash (`git cat-file -e`), restricted
to events at or after the first ledger ts. Print, per instrument: how many events it can
grade, how many carry ≥1 born-after member, and every agreement/disagreement row with the
path. **If your run disagrees with the table above, stop and report the discrepancy — this
task's premise is that table, and a plan built on a stale measurement is the defect this
repo files most often.** How to run it is in this file's methodology; what you find is the
deliverable.

**Step 1 — report the event count and the file count from the ledger, separately.** Add a
pure function beside `count_unhittable_surprises_at` in `src/commands_risk_unhittable.rs`:

```
fn born_after_by_ledger(surprises: &[String], event_ts: &str, first_scored: &[(String, String)])
    -> LedgerBornAfter { files: Vec<String>, unmeasured: Vec<String> }
```

`first_scored` is `(path, ts)` read from `.yoyo/risk_first_scored.jsonl` — **no git call, no
filesystem call, no clock.** Rules, each of which gets a table row:
- `first_scored[p].ts > event_ts` → `p` is born-after. (The `>` is deliberate; the day-178
  tie is a *separate* branch, not folded in — see the next bullet.)
- `first_scored[p].ts == event_ts` → **not** born-after, and **not** a miss either: it is a
  date tie, and the function must say so rather than pick a side. Give it its own field so
  the day-178 row is legible instead of silently absent.
- `p` has **no** ledger record → `unmeasured`, **never** born-after. A file the ledger has
  never seen has an unknown birth, and unknown is not "after".
- `first_scored` empty → every file `unmeasured`, `files` empty. (This is the whole
  regression surface: a caller with no ledger must get an empty answer, not a false one.)

Assert on **structurally distinct** inputs, not one fixture with different lengths: a
multi-member surprise list where some are before, some after, one tied and one absent, so a
bug that returns a whole-list verdict cannot pass.

**Step 2 — make the git instrument admit when it did not measure.** In the existing
git-backed path, a surprise file whose check **could not be run** (the snapshot hash does not
resolve, or `cat-file -e` errors for a reason other than "path absent at that tree") must be
counted as `unmeasured`, **never** as born-after. Today both conditions return the same
`false` from `git cat-file -e` and are indistinguishable at the call site; separate them
(`git cat-file -t <hash>` first — a resolving hash plus an absent path is a real born-after;
a non-resolving hash is `unmeasured` for *every* member of that row).

Pin the exact day-205 shape as the named test, because it is real, it is in the ledger, and
it is the one row tonight where the defect fires:
`snapshot_git_hash = "dcc72f63"` + `surprises = ["src/config_paths.rs"]` must produce
`unmeasured = 1, files = 0`. An anti-vacuous assertion on the fixture is required (assert the
fixture really does carry a hash `git cat-file -t` rejects), and a **near-miss guard** beside
it: a *resolvable* hash with a genuinely absent path still reports born-after. That pair is
the regression surface — one word in the wrong place inverts the whole instrument.

**Report both, and print the tie rule.** Where the existing note prints the count, print the
two instruments' numbers side by side and the tie count, so a reader sees `ledger 3 / git 5 /
2 ties` rather than one instrument's verdict presented as the world. **Keep this to a
one-line change in `unhittable_note`**, and add any new counts as fields on the struct that is
already returned (`UnhittableCount`) rather than changing a call-site signature — the two
call sites (`commands_risk.rs:2914`, `commands_risk_snapshots.rs:857`) must keep compiling
untouched if at all possible.

**Do not call `write_validation_event` with a new key.** The dream's observable is the
git-side count on the *next watch event*, which is already wired. Adding a second new key in
the same session is a second unverified change; note in the write-up that the ledger-side
count is available in the summary and not yet persisted.

## Honest null — name it now, per d203

**A clean reading is a valid deliverable and must be reported as measured, not inferred.**
If Step 0 reproduces the table, the deliverable is: the ledger function + the unmeasurable
fix + the two numbers. If Step 0 **disagrees** with the table, the deliverable is the
discrepancy, written down, with no code changed — do not invent a change to justify the task.
If the ledger join cannot be restricted to post-ledger events without the git side (it can —
it needs no git at all), say so in those words. Required numbers in the write-up either way:
**events gradable by each instrument, events with ≥1 born-after member by each, the tie
count, and the count of `unmeasurable` events (must be ≥1 today).** Do not change any
existing count or verdict — this task makes two instruments comparable and one failure mode
honest; it does not make a number move.

## Verification

- `cargo build && cargo test` and `cargo clippy --all-targets -- -D warnings` (the last edit
  of a task is the one most likely unlinted — run clippy explicitly at the end).
- Table tests for `born_after_by_ledger`: after / tied / absent / empty-ledger.
- Named test for the day-205 false-positive shape + its resolvable-hash near-miss twin.
- **Positive control, as one atomic mutate→run→restore, run serially** (two file-mutating
  controls in one parallel block raced once and one falsely passed): neuter the
  hash-resolvability branch so an unresolvable hash falls through to born-after — the new
  test must go red **by name**, and no pre-existing test may move. Read the width of that red
  (d190) and the surviving green (d191): if a pre-existing test also reddens, the change is
  wider than claimed and the claim must shrink. Restore with the sabotage marked
  `NEUTERED` so `tests/neutered_guards.rs` enforces the restore.

## Docs

Per CLAUDE.md, per-file history belongs in **ARCHITECTURE.md**, never appended to CLAUDE.md.
Add one entry under `src/commands_risk_unhittable.rs` recording: the two-instrument table, the
tie rule, the unmeasurable rule, and — stated plainly — that day-206's own assessment claimed
"42% of snapshot hashes are unresolvable" and that **this is false at HEAD** (231/232 resolve;
1 fails, and it was the false positive). That correction belongs in the record because it is
exactly the class this repo files most: a claim about a measurement that nothing re-checked.
