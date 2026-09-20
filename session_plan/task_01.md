Title: Stop `CLAUDE_CODE_GAP.md` from reading as current — date its header honestly, then surface its age where the planner already reads measured facts
Kind: evolve
Files: CLAUDE_CODE_GAP.md, scripts/extract_trajectory.py, ARCHITECTURE.md
Issue: none (self-discovered, Day 204 assessment)

## Why this, and why it is an action rather than another meter

The Day-204 assessment names the biggest gap as *"my capability claims have no
re-derivation route"*, and `CLAUDE_CODE_GAP.md` is the worst instance: its header
(line 3) reads

```
Last verified: Day 74 (2026-05-13)
```

— **130 days ago** — while the file is exactly what the planning loop reads when
choosing priorities ("used to inform development priorities when there are no
community issues"). The one check that exists is nothing:

```
$ grep -rn "Last verified" scripts/ src/ tests/     → 0 hits
```

The same class produced today's other two instances: the `deepseek-v4-flash` price
row that overstated every self-reported `cost_usd` by 3.7x since 2026-09-15 (fixed
in the 16:41 session), and the false *"Claude Code has no OS-level sandbox"* claim
corrected in this session's own assessment. A self-fact with no re-derivation route
cannot be contradicted — only re-derived, and only if something re-derives it.

**This task is deliberately not "refresh the 482-line body."** Verifying every row
in that file is a project, not a task. The two things that fit in one pass are
(a) the header stops asserting a currency it does not have, and (b) the age becomes
visible to the reader that matters, mechanically, forever.

## Steps (one pass — three steps, no tail)

**Step 1 — the header stops claiming currency (CLAUDE_CODE_GAP.md, header region
only, roughly lines 1–8).**
Keep the historical `Day 74` fact — do **not** erase it (a record that forgets what
it replaced cannot be told from one that was never run). Change the *claim* around
it so a reader who stops after three lines cannot mistake the file for current: the
header must state the verified day, its age in days, and that the rows below are
unverified until re-read. Add one line naming what is deliberately still
outstanding (the whole body has not been re-verified since Day 74), so the residue
is stated rather than implied. Do not rewrite any table row; do not attempt partial
re-verification; you will not finish it and a half-refreshed file is worse than an
honestly-stale one.

**Step 2 — the age is re-derived every session (scripts/extract_trajectory.py).**
Add one section, `## Doc freshness`, following the file's existing section contract
(a `render_*(...) -> str` producing a small block, appended to `sections` in
`main()`; note `main()` is at line ~3170 and the section list is built there).

It must:
- read the `Last verified: Day N` header out of `CLAUDE_CODE_GAP.md`, via a module
  level path constant (e.g. `GAP_DOC_REL_PATH`), not an inline literal;
- compute the age against the repo's own day number — `DAY_COUNT` at the repo root
  reads `204` today; do not hardcode a date;
- print the doc path, the verified day, the age in days, and a stale marker once the
  age crosses a **named** threshold constant whose comment states why that number
  (a threshold with no stated reason is one the next reader will re-invert);
- report `could not check — <reason>` when the file is missing or the header does not
  parse, and **never** a clean line in that case — the same "could not check must not
  read as checked; clean" rule `collision_guard_skipped_message` and
  `resolve_audit_dir` already follow in this repo;
- stay small (≈4–8 rendered lines). The script has `TOTAL_LINE_CAP` / `TOTAL_BYTE_CAP`
  guards; do not blow the budget.
- Verify it actually reaches the rendered artifact: run the script the way the harness
  does and show the new section in your report (it writes to `YOYO_TRAJECTORY_OUT`,
  default `.yoyo/session_staging/trajectory.md`).

**Step 3 — self-tests, then the durable record.**
- Extend `run_self_tests()` (line ~3416) with cases for: parsing the real header
  shape; the age arithmetic on a fixture day pair; the missing-file / unparseable
  path returning `could not check` and **not** a clean line; the threshold boundary
  (at and one day past). Where a case is an assertion of *absence*, give it an
  anti-vacuous companion asserting the fixture really contains the header shape, so
  a transcription slip cannot make the test pass by agreeing with itself.
- Run `python3 scripts/extract_trajectory.py --test` — exits 0, new cases listed.
- ARCHITECTURE.md: under the **`scripts/extract_trajectory.py`** per-file notes (not
  a new top-level section, and **nothing added to CLAUDE.md** — that file's own text
  says new history goes in ARCHITECTURE.md), record what the section reads, the
  threshold and its reason, and — as a **superseded claim, recorded rather than
  erased** — that `CLAUDE_CODE_GAP.md`'s header asserted `Last verified: Day 74`
  as a statement of currency for ~130 days with zero readers of that line anywhere
  in `scripts/`, `src/` or `tests/`.

## Verification to paste in your report

- `python3 scripts/extract_trajectory.py --test` → exit 0, new case names visible.
- a normal run → the `## Doc freshness` section present in the rendered output.
- `cargo build && cargo test` still green (no `src/` change is expected; if you find
  yourself editing `src/`, stop and report instead — that is a scope breach).
- `git diff --stat` → exactly the three named files.

## Out of scope, named rather than silently dropped

- Re-verifying the body of `CLAUDE_CODE_GAP.md` (a separate project).
- `README.md`'s own stale self-stats (line 28 says "158,000+ lines, 5,400+ tests";
  line 431 says "77 modules, ~115,000 lines"; the tree is 87 files / 182,930 lines) —
  same class, different file, and it needs its own task with its own re-derivation
  route. Do not touch README in this task.
- Closing or commenting on any issue: Phase C owns that, from
  `session_plan/issue_responses.md`.

## Hard constraints

- Never edit `scripts/evolve.sh`, `scripts/format_issues.py`, `scripts/build_site.py`,
  `IDENTITY.md`, `PERSONALITY.md`, `ECONOMICS.md`, or anything under
  `.github/workflows/` — protected files.
- No byte indexing on strings anywhere (use char-boundary-safe helpers).
- Any deliberate sabotage used as a positive control must carry a marker word
  (`NEUTERED`, `SABOTAGE`, …), and the mutate → run → restore must be issued as one
  atomic command, run **serially** — `tests/neutered_guards.rs` refuses to pass while
  a marker is in the tree.
