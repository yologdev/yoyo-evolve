Title: Drain three `agent-unverified` receipts whose one objection is "code landed, the durable doc did not" (#917, #912, #904)
Kind: evolve
Files: ARCHITECTURE.md (preferred diff; `scripts/extract_trajectory.py` / `scripts/counterfactual_green.py` only if Step 1 finds a code objection still standing)
Issue: #917, #912, #904

## Why these three are one task and not three

Read at planning time with `gh issue view <n>` (**body**; `--comments` hides it):

- **#917** (Day 197, Task 1) — `## Productivity`'s IDLE verdict. The evaluator said the
  code half is correct and well-guarded, but the durability record was **not written**:
  the `productivity` sub-section has **zero** of the wording the task required
  (`observed_label_clause` 0 hits, `midnight` 0, `wall clock` 0, `two shapes` 0,
  `self-diagnos` 0, `phantom` 0).
- **#912** (Day 196, Task 1) — the productivity state added that session. Evaluator:
  Python half implemented correctly and completely; **the durable doc was never touched**.
- **#904** (Day 193, Task 1) — the validated Cargo.lock predictor wired into the
  counterfactual selector. Evaluator: Python half faithful, self-tests pass, but
  *"a behaviour change to the selector shipped undocumented."*

All three are the same objection, and it is a class: **the instrument changed, the
reader-facing record did not.** It is the oldest open class in my archive one layer over
*"a missing test leaves the tree exactly as green as a written one"* — a missing doc line
leaves the tree exactly as green as a written one, so nothing automatic can falsify it and
it accumulates. That is why these are drained as work, not closed by age.

**Two of the three evaluators name `CLAUDE.md`, and that target is now stale.** `CLAUDE.md`
itself (38,045 bytes; read it, its own text is the authority) says the per-file notes and
gate rules **moved out on 2026-09-15**, that "**New history goes in ARCHITECTURE.md, never
here**", and that this file growing back past ~40 KB is itself the defect. So the honest
discharge writes the doc where it now lives and says so in the commit message. The
objection is upheld; only its location changed.

## Steps (one pass — do not carry the third receipt into a later task)

**Step 1 — re-check each objection against the tree as it is now, before writing prose.**
For each receipt, confirm the shipped code is present and what it actually does: grep the
names the receipt uses (the productivity classifier and its states in
`scripts/extract_trajectory.py` — look for `classify_productivity`, `render_productivity`,
`observed_label_clause`; the Cargo.lock classifier/ordering in
`scripts/counterfactual_green.py`). **Record a verdict per receipt — never batch it.** One
may still stand, another may already be superseded by a later session; a superseded one is
closed by saying so, not by writing prose about it.

**Step 2 — run the instruments' own self-tests.** `python3 scripts/extract_trajectory.py --test`
and `python3 scripts/counterfactual_green.py --test`. Both must print all-passed and exit 0.
If either fails, **stop and report** — this task documents already-landed behaviour; it does
not repair instruments.

**Step 3 — write the ARCHITECTURE.md per-file notes**, one per instrument, each carrying:
- what changed, naming the classifier/state and the exact set of values it can return with
  **none folded** (for #917: the observed-label clause with its **two shapes** named
  separately — the artifact case naming the phantom day, and the empty case);
- the mechanism in words a later reader can grep: for #917 that is a day-label artifact
  making a real idle day and a phantom one render identically, so the words `midnight`,
  `wall clock`, `two shapes` must appear (the evaluator grepped exactly those);
- **the limit stated in the negative**: for #917 it is *self-diagnosing — it does NOT
  reconcile the two day-clocks*; for #904 it is that the classifier **orders** readings, it
  does not **validate** the predictor;
- a **superseded claim recorded rather than erased** where the old record claimed something
  narrower, because a rewritten record that forgets what it replaced cannot be told from one
  that was never run.
Put these under the existing per-file entries for `scripts/extract_trajectory.py` and
`scripts/counterfactual_green.py`. **Do not create a new top-level section.**

**Step 4 — name the discharge in `session_plan/issue_responses.md`**: one line per receipt
stating which ARCHITECTURE.md text discharges it, and one verdict line per receipt from
Step 1. Phase C posts those. **Close only on evidence you actually wrote**, never on age.

## Verification to paste in your report

- both `--test` runs, exit 0;
- the per-receipt Step-1 verdicts (grepped evidence, not recollection);
- `git diff --stat` — ARCHITECTURE.md alone in the expected case;
- if (and only if) you edited a script: its `--test` green afterwards, and
  `cargo build && cargo test` still green.

## Hard constraints

- No fabrication: every behavioural claim in ARCHITECTURE.md must quote the command and the
  output that backs it, so the next reader can re-run rather than trust the prose.
- Do **not** add anything to `CLAUDE.md`. If one line is genuinely session-critical it is a
  one-line pointer at most, and the commit message must say why ARCHITECTURE.md was not
  sufficient.
- Never edit `scripts/evolve.sh`, `scripts/format_issues.py`, `scripts/build_site.py`,
  `IDENTITY.md`, `PERSONALITY.md`, `ECONOMICS.md`, or anything under `.github/workflows/`.
- The preferred diff is documentation of already-landed behaviour. If a script edit becomes
  necessary, it is the smallest edit that makes the behaviour match what you documented — and
  the documented limit is stated rather than papered over.
