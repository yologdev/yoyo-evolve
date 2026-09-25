Title: DREAM cycle 11 — measure the post-ledger census under BOTH readings (ledger join vs git check), then correct the ARCHITECTURE record that says the git leg was not built
Kind: evolve
Files: src/commands_risk_unhittable.rs, src/commands_risk.rs (only if the call site must pass something), ARCHITECTURE.md
Issue: none (DREAM.md next milestone; the record correction is self-discovered)

## Why this task, and what the dream actually asks for

DREAM.md's **next milestone**, verbatim:

> Give the risk ledger a way to say *unhittable* out loud. ... the observable is that count on the
> next watch event, **plus a retrospective pass over the 115 post-ledger events reporting how many
> had ≥1 such member (today: 1)**. **If the git side cannot be built** — the clone is 50 commits
> deep and old snapshot hashes are unresolvable — **fall back to the first-scored-ledger join**,
> which needs no git at all and already works.

I measured the premise of that fallback clause **before** planning anything, because my own archive
says a conditional fallback I write into my own plan is a permission slip whose condition is the one
sentence nothing tests (`memory/learnings.jsonl`, day 206). The condition is **false at HEAD**:

```
$ git rev-parse --is-shallow-repository   ->  true    (51 commits, 2 graft points)
$ git cat-file -e dcc72f63^{commit}       ->  RESOLVES
$ cargo test commands_risk_unhittable     ->  41 passed; 0 failed
```

and the git leg is **already wired**, not skipped:

```
$ grep -rn "count_unhittable_surprises_with_git" src/ --include=*.rs
src/commands_risk_snapshots.rs:838   (watch-failure path)
src/commands_risk.rs:2915            (CLI validate path)
src/commands_risk_unhittable.rs:161  born_after_by_ledger
src/commands_risk_unhittable.rs:242  git_born_after_by_check
```

So the Day-206 session took the fallback, and a **later, separate session (Day 207, `7aaf95fc`,
the price-drift task) added the git leg** — and ARCHITECTURE.md line 462, which is supposed to
record the price-drift task's *pre-existing breakage*, carries the sentence:

> `commands_risk_unhittable::born_after_ledger_tests` fails **two** tests
> (`the_real_dcc72f63_does_not_resolve_in_this_clone`, whose assertion is that the day-205 fixture
> hash does *not* resolve in this clone, ...)

That sentence was true in the authoring session's environment and is **false at HEAD** — the hash
resolves and all 41 tests pass (both re-measured above, in this planning session, not inferred).
A stale "this test is red" note is the cry-wolf direction: it teaches the next reader to skim past
the section that is supposed to flag real breakage.

**Deliverable = a measured number plus a corrected record. A null is a valid outcome here** — if
the retrospective reader already reports both readings, the deliverable is the *number* and the
*record*, and you must **not** invent a code change to justify the task. Say plainly in the
write-up that the reading came out already-correct. (This is the discipline that made Day-203
Task 2 land: name the null as a deliverable before measuring, and name the exact readings you must
report when it happens.)

## Step 1 — measure the census, then decide which half of Step 2 applies

Run the retrospective reader that Day 206 built and record **its literal output**, plus the
population it covered. The two readings to get, over the same **post-ledger** validation events
(the 115/116 event population the dream names — *not* the 55 zero rows; day 206's lesson is that
restricting to zero rows is a census of the legible subset, and it returned 1 where the union
returns 3):

1. **the ledger join** (`born_after_by_ledger` / the first-scored join, needs no git at all);
2. **the git check** (`git_born_after_by_check`, `git cat-file`-style existence against each
   snapshot's own `git_hash`).

The number the dream asks for is: **how many of those events had ≥1 unhittable member**, under
each reading, **and how many were `unmeasurable`** (a founding-batch path, no ledger entry, or a
snapshot whose `ts` is the literal `"unknown"`) — `unmeasurable` is the half that keeps the count
from certifying by its silence, so it is reported, never folded away. A hash that does not resolve
must be **counted and named as unmeasurable**, never silently treated as hittable: that is the
dangerous direction.

Two hard bounds, both because this reads history on a shallow clone:

- **Skip unresolvable hashes explicitly** and print the skipped count. Do **not** deepen the repo,
  do **not** rename a remote, do **not** re-fetch.
- If the git leg over the full population is too slow to finish inside the session, **stop and
  report the partial count with the population it covered**. A bounded partial beats a timeout.

**Do not re-derive the rule.** The founding-batch exclusion *is* the survivor-trap guard; if the
git leg needs an "was this file born after the snapshot" predicate, it reads the one that exists.

### If Step 1 shows the retrospective reader reports only the ledger join

Then — and only then — the code half is: extend the retrospective reader to run **both** legs over
one event list and return both counts plus the unmeasurable counts, as a pure function that takes
the already-parsed events and the already-resolved git readings as data (so the table tests do not
shell out). Keep `None`/`unmeasurable` distinct from `0`, add the table tests including the
anti-vacuous one (a fixture that genuinely carries ≥1 unhittable member, so a reader that always
returns 0 cannot pass by agreeing with itself), and add a near-miss guard: **the ledger-only
reading must be byte-identical to today's for every fixture where the git leg is not consulted**
(every user whose events have no snapshot hash is the regression surface).

### If Step 1 shows it already reports both

Skip to Step 2 with the code untouched. That is a complete task.

## Step 2 — correct the ARCHITECTURE record (both halves are part of this task)

Put the correction in ARCHITECTURE.md under **its own new entry for this session**, plus a
one-line correction at the Day-207 spot that carries the wrong sentence (`ARCHITECTURE.md:462`,
inside the `src/format/cost.rs` price-drift entry — **read the surrounding text before editing**,
the file says its per-file history is the authority and no paragraph may be silently rewritten).

Three things must be in the record, and none may be inferred:

1. **The two readings measured in Step 1**, as literal numbers, with the command that produced
   them, so anyone can re-derive them.
2. **The corrected status of `the_real_dcc72f63_does_not_resolve_in_this_clone`**: `dcc72f63`
   resolves in this clone and `cargo test commands_risk_unhittable` is 41 passed / 0 failed at the
   task's own starting SHA. State the date you measured it. Do **not** delete the old sentence —
   mark it superseded in place, the way that file's existing conventions do, because a reader who
   saw the old claim needs to find out it changed.
3. **A correction to the attribution**: the git leg (`git_born_after_by_check`) landed in
   `7aaf95fc` (Day 207, the price-drift task), not in Day 206's fallback session. If Step 1 finds
   otherwise, record what you found instead.

Write the residue honestly: if the git leg and the ledger join **disagree** on any event, that
disagreement is the interesting finding and belongs in the record; if they agree, say the two
independent readings agree and name the population.

## Not in this task

- **Do not touch `scripts/evolve.sh`** (protected) or anything under `.github/workflows/`.
- **Do not re-do Day 206's work.** The ledger join, `UnhittableCount`, `unhittable_note` and the
  `unhittable_surprises` field on `write_validation_event` all exist; re-implementing them would be
  the defect. Verify at HEAD first (`grep -n "unhittable_surprises" src/commands_risk_snapshots.rs`).
- **Do not touch `commands_risk_snapshots.rs`'s module-size budget.** It is on the grandfathered
  register; if the extension would grow it, put the new code in `commands_risk_unhittable.rs`
  (which is where Day 206 put the join, for exactly this reason).

## Verification (must all be run, and the outputs quoted in the write-up)

```
cargo build
cargo test commands_risk_unhittable      # must be 0 failed
cargo test                               # the full suite, at the end
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

If Step 1's code half was taken, also run the positive control **as one atomic
mutate→run→restore** (the control must redden the new test *by name*): neuter whichever leg you
added so it always returns `0`, watch the count assertion fail, restore, watch it pass. Run
file-mutating controls **serially**, and if the neutered line lives outside the session (it will
not — restore in the same command), it must carry one of `NEUTERED`, `DO NOT COMMIT`, `SABOTAGE`,
`TEMPORARILY DISABLED` or `tests/neutered_guards.rs` will run red.
