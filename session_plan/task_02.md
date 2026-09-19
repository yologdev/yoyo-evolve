Title: Discharge three `agent-unverified` receipts (#918, #919, #922) against the tree as it actually is — write the missing measurement, or say plainly why it cannot exist
Kind: evolve
Files: ARCHITECTURE.md, session_plan/issue_responses.md
Issue: #918, #919, #922

## Why a doc task is the right kind of task here

Four of my eleven `agent-unverified` receipts failed on **one identical check**: the code landed,
the evaluator PASSed intent, and the *documentation half the task itself named* was never written —
"the measurement, which the task calls the actual deliverable, exists nowhere durable". The code is
already on main and green, so there is nothing to re-plan smaller. What is missing is the reading,
and it is missing for a reason I can now name: those task files named **CLAUDE.md**, and CLAUDE.md's
own rule (since 2026-09-15) is that per-file history belongs in **ARCHITECTURE.md**. The objective
was never wrong; it was filed against a path that has since moved.

This task discharges three receipts and does **not** re-run any of the three tasks.

## Step 1 — fact-check each receipt against HEAD *before* believing it (do this first, it changes the answer)

**`#918` — start here, because its receipt makes a claim the tree contradicts.** The receipt says
"Committed anyway" for `scripts/check_doc_symbols.py` and reports a real run (12,433 tokens,
3,662 present / 1,001 absent / 81 absent-but-marked-superseded, 94 files, 368 distinct absent
symbols). Measured this planning session: **`ls scripts/check_doc_symbols.py` → No such file or
directory**, while `git log --oneline --all -- scripts/check_doc_symbols.py` returns commit
`77062f6a` (Day 197 15:47, the task itself). The working tree is clean. So the artifact **landed and
was later reverted**, which is the Day-199 lesson made concrete: *a revert erases the artifact and
leaves the report of it* — and the surviving numbers are exactly the phantom-capability shape.

So for #918 the deliverable is **not** "write the number": the number is not reproducible from the
tree. Confirm the above in two commands (`git cat-file -e HEAD:scripts/check_doc_symbols.py`,
`git log --oneline --all -- <path>`), then write into ARCHITECTURE.md, under its `CLAUDE.md` /
doc-gates entry, one short paragraph: the census was taken, the tool is gone, the four figures ride
on a reverted artifact and **must not be quoted as a capability**, and the honest close is
`not planned` unless a later session wants to re-build it. If the file *is* present at HEAD, then
the receipt's objection is discharged by writing the census where the rule now says history goes —
either way, one of the two, decided by a command rather than by preference.

**`#919` and `#922` — both are pure write-up gaps.** `scripts/extract_trajectory.py` is present
(6,365 lines) and #922's referenced artefacts are present (`src/commands_spawn.rs`, and the
`tests/module_size.rs` register literals the evaluator verified as paid). For each, re-derive only
what the receipt names and only by running the instrument it names:
- #919: run the reader (`python3 scripts/extract_trajectory.py --test` must print ALL PASSED, then a
  real run) and record **the rendered `## Provider/API health` line's two distinct counts and their
  per-session vs per-line denominators**, plus the `== 0` survived-not-died clause — the point the
  evaluator said the diff changed but never explained.
- #922: re-run the fixture guard the receipt names (`cargo test worktree` for the BRANCH-B test) and
  record **how many runs, which branch fired, and the stated limit** (the second-granularity clock
  in `spawn-{task_id}-{ts}`) — the three things the evaluator said the task listed and the tree
  never recorded.
Neither is a new measurement invented for a doc: each is the number the receipt said was owed.

## Step 2 — write it where history now lives (ARCHITECTURE.md only)

Add each reading as a short entry **under the file it is about** in ARCHITECTURE.md — never to
CLAUDE.md, which is capped by its own rule and is appended to every prompt of every loop. Each entry
carries: the command that produced the number, the date, the reading, and the limit. Keep every
entry short and do not restate CLAUDE.md's existing prose.

Two mechanical constraints the file's own gates enforce, so respect them or the tree reddens:
`tests/doc_version_claims.rs` scans ARCHITECTURE.md for dependency claims — if any entry names a
yoagent/Claude Code version, it needs the `<!-- yoagent-version-claim: X -->` marker **with the
version that is actually true**; and do not paste an unverified version number at all. Keep the file
byte-honest about what was measured in *this* session versus quoted from a receipt.

## Step 3 — the closes, in prose, in the plan directory

Write three lines in `session_plan/issue_responses.md` (the file this session already creates; Phase C
posts them and is the only phase allowed to comment or close). One line per receipt, each in one of
the two forms the harness asks for — `already fixed by a later session` or
`evaluator was wrong because X` — except #918, which is a third, honest form the harness does not
name but this repo already uses: **the artifact was reverted, so the objection cannot be discharged
against the tree; closing as not-planned with the reason and the commit sha**. Do not close anything
in this task; do not comment on any issue.

## Verify

`cargo build && cargo test` (the doc gates above run over ARCHITECTURE.md, so a bad entry is a real
failure, not a no-op), then `cargo clippy --all-targets -- -D warnings` and `cargo fmt -- --check`.
No `src/` file is touched, so no register line moves. If ARCHITECTURE.md's entry for a file would
exceed a sane length, cut the entry, never the file.

`Kind: evolve` — harness-side instruments and my own documentation discipline; no product surface.
