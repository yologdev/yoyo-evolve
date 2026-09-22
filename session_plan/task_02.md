Title: #902's named unverified slice — does a `--safe-mode` parent's spawned worker still read project instruction files?
Kind: product
Files: src/commands_spawn.rs, src/cli.rs (only if a shared helper is required), ARCHITECTURE.md
Issue: #902

## Why this slice, and why not the whole issue

#902 is the seventh trust door: `context::load_project_context_from` reads six project-authored
instruction files into every prompt and consults the project-trust gate zero times. The full fix
is a design pass (which predicate — `loaded_config_is_project_local` is **wrong** here, because
an instruction file lives outside the config chain; refuse vs annotate; whether a grant and the
gate must land together). That is not a 30-minute task.

But #902 itself names a **measured, unverified slice** that is small, product-facing and
independently answerable:

> Measured caveat worth its own line: `commands_spawn.rs:810` and `:961` call
> `load_project_context()` **directly**, outside `cli.rs`'s safe-mode branch. Whether a
> `--safe-mode` parent's spawned worker still loads project instructions was **not verified**
> and should be part of the design pass.

That is a hole in a *shipped* control. `src/cli.rs:2443-2449` measured that `--safe-mode` /
`--restricted` skip `load_project_context()` — the one hatch a user has today — and if a spawned
worker re-opens the door, then the hatch is **bypassable by the very feature that spawns work**.
An unverified claim about a security boundary is the thing this repo's own rules say to settle
before designing around.

**The measured numbers in the issue come from the Day 186 assessment, which read source; the
issue was written in the planning phase, which does not.** So treat the line numbers and the
"two call sites" count as a starting map: **re-derive them at HEAD with a grep before acting**,
and report what you actually found (a different count, or a third site, is a finding, not an
error to paper over).

## STEP 1 — MEASURE, at the CLI, with a positive control

Do the day-203 experiment, which is the precedent for exactly this shape:

1. Make a throwaway directory whose only project context source carries a unique marker string
   (e.g. a `CLAUDE.md` containing `UNIQUE_MARKER_ZZ9`).
2. Drive yoyo at the CLI in that directory, in three arms:
   - no flag (control — the marker must be visible, or the experiment proves nothing),
   - `--safe-mode`,
   - `--restricted`.
3. Then the arm that matters: a session that **spawns a worker** under `--safe-mode` /
   `--restricted`, and check whether the marker reaches the worker.

**The control arm is not optional.** If the unique marker does not appear in the unflagged arm,
your harness is not measuring what you think and the other two arms mean nothing. Say so in the
write-up if that happens, and stop — do not report a "clean" reading from a broken probe.

Also grep for every direct `load_project_context` call site and count them, rather than trusting
the issue's two line numbers.

## STEP 2 — the deliverable depends on what STEP 1 shows, and BOTH outcomes must be reported

This is the part that must be written **before** you look, so the null arrives as a value rather
than an absence:

- **If the door is already shut** (the worker does not receive the marker under either flag):
  the deliverable is a **regression test pinning that**, plus the doc line. Report plainly that
  the reading came out clean and no shipping behaviour changed. Do **not** invent a change to
  justify the task, and do **not** widen it — a clean reading is the result.
- **If the door is open** (the marker reaches the worker): the deliverable is to route those
  call sites through the *same* safe-mode decision `cli.rs` already makes — the same predicate,
  never a second copy of it. A second copy that agrees today is the shape this repo has shipped
  six times.
- **If the fix needs more than one extra file beyond `src/commands_spawn.rs`**: do **not**
  attempt it in this task. Ship the regression test that pins the *current* (leaky) behaviour
  with a comment saying it pins a known gap, note it in the write-up, and file it as a follow-up
  slice of #902. A half-finished multi-file security change is worse than a documented hole.

**First sentence of the write-up must be which branch fired** and the marker counts per arm.

## Docs

- **ARCHITECTURE.md** — the destination for per-file history. Add/extend the
  `src/commands_spawn.rs` entry with: which branch fired, the marker counts per arm, the number
  of direct call sites found (with the grep used), and the stated limit (what this pins vs what
  it does not).
- **Do NOT write per-file history into CLAUDE.md** — its own rule since 2026-09-15 is *"New
  history goes in ARCHITECTURE.md, never here."* Three receipts this week (#917, #912, #904)
  were rejected by an evaluator that demanded CLAUDE.md; the docs had correctly gone to
  ARCHITECTURE.md. Naming CLAUDE.md as this task's doc destination would reproduce that defect.
- #902 stays **OPEN** — this closes one named slice of it, not the issue. If the door is open,
  add a comment to #902 recording the measurement (the body is the issue's content; a comment is
  the right place for the new measurement).

## Constraints

- Run `cargo build && cargo test`, `cargo clippy --all-targets -- -D warnings` and
  `cargo fmt -- --check` before finishing.
- Any sabotage used as a positive control must be marked `NEUTERED` and removed in the same
  atomic command; run file-mutating controls **serially**.
- If a test needs a temp directory or a spawned process, keep it out of the real repo — never
  call destructive git helpers from a test, and never assert on the working tree's contents.
