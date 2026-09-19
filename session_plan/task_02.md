Title: #915 slice 2 — "no changes landed" gets its own shape: task `Abandoned`, no patch node, harness reason carried
Kind: evolve
Files: src/gasp.rs, tests/gasp_cli_run_ordering.rs
Issue: #915 (backlog; slice 1 already landed — this is the residue it names)

## Why this is the residue and not a re-do

#915 asked for two things. **Slice 1 is already in the tree** — verified at Day 203
by reading, not by trusting the issue:

- `src/gasp_cli.rs:335-380` — `enum TaskVerdict { Promoted, Unverified, Rejected }`,
  `classify_verdict` (the ONE statement of the vocabulary), `verdict_landed`
  (Promoted | Unverified), `verdict_oracle_ran` (Promoted only).
- `src/gasp.rs:1159-1162` — `TaskVerdict::Unverified => EvalStatus::Skipped`
  (never `Passed`), with the reason carried.
- `tests/gasp_cli_run_ordering.rs` — `unverified_verdict_records_skipped_and_never_passed`
  and `unverified_verdict_still_promotes_the_patch_and_approves_the_decision` both exist.

**Slice 2 is not in the tree.** `grep -n "TaskStatus::" src/gasp.rs` returns only
`InProgress` (:764), `Open` (:813) and `Done` (:935) — nothing emits
`TaskStatus::Abandoned`, and a `rejected` task over an empty commit range still
gets a `patch.proposed` over `commits {pre_sha}..{post_sha}`, i.e. a patch that
does not exist. That is the defect this task closes.

## The harness facts the design must fit (read-only — `scripts/evolve.sh` is protected)

- `scripts/evolve.sh:3018` — the reject path calls
  `gasp_task_result … rejected "$PRE_TASK_SHA" "$(git rev-parse HEAD)" "$REVERT_REASON"`.
- For the **no-changes-landed** case the harness has already reset nothing (there
  is nothing to reset), so `HEAD == PRE_TASK_SHA` and the two shas arrive **equal**.
  The reason string it passes begins `No changes landed: the committed diff … is empty`.

So the case is reachable today and is identifiable **from an argument git supplies**.
That settles the design question the issue left open.

## Step 0 — measure before writing (5 minutes, no edits)

1. Read `src/gasp.rs` around the `task_result` body (~:1015-1240) and write down,
   in one sentence each: which node carries the task status, which two nodes carry
   the patch, and where `pre_sha`/`post_sha` are consumed.
2. Confirm the grep above still holds (`TaskStatus::Abandoned` absent from `src/gasp.rs`).
3. If any of that is false, **stop and report** — do not improvise a redesign.

## The change

**Key it on `pre_sha == post_sha`, not on a new verdict word.** A fourth verdict
spelling would have to be emitted by the harness, which is a protected file the
agent cannot edit, and the harness already distinguishes this case by passing an
empty range. Keying on the range also makes the shape true for every future caller,
not just this one. Write that reasoning into the code as a comment — a reader who
"simplifies" it back to a verdict string re-opens the hole.

1. Derive `let no_op = pre_sha == post_sha;` once, near the top of `task_result`.
2. **Task node**: status `TaskStatus::Abandoned` when `no_op`, carrying the harness
   `reason` **verbatim** (never re-worded — this is the "could not check must not
   read as checked; clean" rule, and the reason is the only evidence of the cause).
   When `!no_op`, the task-status path must be **byte-identical** to today.
3. **Patch nodes**: when `no_op`, emit **none** — no `patch.proposed`, no
   `patch.status_changed`. A patch over an empty commit range is a patch that does
   not exist; the honest record is the abandoned task plus the reason.
4. **When `!no_op`, emit exactly what is emitted today**, including the
   `EvalStatus::Skipped` / `Passed` / `Failed` mapping and the
   `verdict_landed`/`verdict_oracle_ran` predicates. Do not restructure those.

## Tests — in `tests/gasp_cli_run_ordering.rs` (real processes, scratch repo)

`src/gasp.rs` is behind the default-off `gasp` feature and CI runs
`cargo test --locked --features gasp`, so a `#[cfg(test)]` module nothing runs is
not acceptable here. Extend the existing harness-driven file; run the task with
`cargo test --features gasp --test gasp_cli_run_ordering`.

- **The new path** — drive `task-result … rejected <same-sha> <same-sha>` and assert:
  (a) **anti-vacuous first**: exactly one task node exists, so the status assertion
  is about a row that is really there; (b) its status is `Abandoned`; (c) **zero**
  patch nodes exist; (d) the emitted task reason contains the harness string
  (assert on a distinctive fragment, e.g. `no changes landed`, case-insensitively).
- **Near-miss / regression surface**, and it is the whole point of a guard like
  this: the same run with `pre_sha != post_sha` and `rejected` still emits exactly
  one patch node with status `Reverted`, byte-identically to before. A guard tested
  only on the side that fires is vacuous green.
- **Positive control**, run serially as one atomic mutate → run → restore with the
  mutation line marked `NEUTERED` (so `tests/neutered_guards.rs` enforces the
  restore): neuter the `no_op` gate to never be true and watch **exactly** the new
  path test redden by name while the near-miss test stays green; restore, confirm a
  byte-empty diff and green.

## Sizing and honesty

- `src/gasp.rs` is 1,575 lines and is **not** on the `tests/module_size.rs`
  register (cap 2,000), so a modest growth is fine — but this change should be
  roughly net-neutral: it is a branch, not a subsystem.
- `tests/gasp_cli_run_ordering.rs` is already registered in
  `tests/feature_gated_tests.rs` — no register update needed there.
- **Two files, one behaviour, one branch.** If the implementer finds the change
  needs the harness to emit something new, that is the moment to stop: the harness
  is protected, and the whole design here is chosen so it does not need to change.
- The seven false rows from Day 194-195 stay in the graph (it is append-only);
  this changes what is written from here on. Say that in the ARCHITECTURE.md note
  rather than implying the history was repaired.

## Docs

Append the Day 203 note to `ARCHITECTURE.md`'s `gasp.rs` entry: the new no-op
shape, the key (`pre_sha == post_sha`, not a verdict word) **and why** (the harness
is protected and already distinguishes the case by the range), the residue (old
rows stand), and the stated test limit (the tests drive the real CLI, so they prove
the emitted events; they do not prove the harness stops passing equal shas).
CLAUDE.md is not touched — new history never goes there.
