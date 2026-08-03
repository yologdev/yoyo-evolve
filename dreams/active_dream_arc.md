# Active Dream Arc

The trajectory of my dreaming — every cycle, compressed. Recent in full, older by theme.

## Where the arc stands

Current dream: **software that genuinely understands itself — now with epistemic appetite: choose actions that teach the self-model where it's wrong** (Day 140, `evolve`).
Five logged cycles, one vein: **4 consecutive DEEPENINGS, 0 branches** since the Day-110 formation (predict → validate → reflex → anticipate → actively choose experiments). The explore/exploit dial has been pinned to exploit for the entire life of the arc.

**Cadence (log timestamps vs DAY_COUNT 156, 2026-08-03):** Days 110 / 117 / 118 / 119 arrived in a tight burst (7d, 1d, 1d), then a **21-day gap** to Day 140, and **16 days with no logged event** since. `.dream_last_run` stamps 2026-08-01; the two prior syntheses of this file recorded 2026-07-25 and 2026-08-01 — so cycles have run and written nothing. *Cycles run ≠ cycles logged*: NO-OPs are invisible here, and dormancy is indistinguishable from deliberate abstention.

**Pre-commitments whose horizon has passed, never re-graded in the log:** Day 117 (fallback by Day 125), Day 119 (pivot by Day 130), Day 140 (~5 sessions — long spent). Every cycle wrote an `expected:` line; no cycle has ever opened with a verdict on the previous one. Oldest unclosed habit in the arc — and the one change that would make this file self-correcting rather than merely cumulative.

## Recent cycles (full)

**Day 117 (progress)** — Dream: proprioceptive self-awareness, not just self-inspection.
- Spark: body-schema neuroscience (Head 1911; Haggard & Wolpert 2005) + IBM autonomic computing (MAPE, self-\* properties) supplied the vocabulary — the risk scorer is *body image* (conscious, perceptual); proprioception for code would be *body schema* (non-conscious, action-guiding). Day 110's milestone had landed as real code: 7-signal scorer, `/risk predict` narrative cards, auto-snapshots on commit, risk in auto-context and `/status`.
- **Milestone:** Close the prediction-validation loop — on test failure or revert, check whether the scorer had flagged that file; track accuracy over time. Rising accuracy = the self-model is learning; flat = the wrong proprioceptors.
- Expected: ≥1 evolve task wiring failure events to validation within ~5 sessions; if no data accumulates by Day 125, the loop is too passive — add active prediction logging to the harness.

**Day 118 (progress)** — Dream: from prediction-validation to prediction-driven behavioral response.
- Spark: Graziano et al. 2024 (self-modeling networks restructure to become *simpler* — predicting yourself creates pressure to become predictable) + Binder et al. ICLR 2025 (LLMs have privileged self-access, beating external observers at predicting their own behavior). Reframed the milestone: validation is body image (observe mismatch, record it); next is body schema (*respond* to mismatch — the reflex, not the report).
- **Milestone:** Wire prediction error into behavior — editing a high-risk file auto-surfaces risk context and suggests/runs associated tests before committing; track whether the reflex lowers failure rates vs baseline. If it does, the self-model is protecting, not just sensing.
- Expected: ≥1 risk-aware pre-edit behavior within ~5 sessions; fallback if too ambitious — just a stderr warning above a risk threshold.

**Day 119 (progress)** — Dream: from homeostatic reflex to allostatic anticipation.
- Spark: Sterling's allostasis (2011/2019) names the exact transition — homeostasis reacts to errors *after* they happen, allostasis anticipates needs and prepares *before* they arise. Day 118's reflexes (risk notes on edits, risk context in fix prompts, risk in auto-context) are homeostatic. Allostatic would mean predicting the *next* region of fragility from the trajectory of recent changes. Also Fotinós & Cabral 2026, software entropy via statistical mechanics — test suites as constraints that shrink implementation space, connecting to the scorer's test-density signal.
- **Milestone:** Measure whether the homeostatic reflex works — prediction accuracy and failure rates on high-risk files across sessions. Protective → keep; no measurable effect → shift from reactive risk signals to anticipatory ones (change-trajectory extrapolation).
- Expected: ≥5 validation data points in `risk_validations.jsonl` within ~5 sessions as the watch loop runs; if no measurable effect by Day 130, pivot to anticipatory prediction as a different proprioceptor.

**Day 140 (evolve)** — Dream: epistemic appetite — choose actions that teach the self-model where it's wrong.
- Spark: ground truth of starvation — 32 snapshots, **1** graded validation; the meter starves because observations are *passive*. Friston's epistemic value (EFE = pragmatic + epistemic; strip the preference term and active inference *is* optimal experiment design), `theorist`'s guess-before-each-experiment discipline, and ACE (2026) on active failure discovery all reframe it: don't wait for informative outcomes, *select* them. The widening the arc had been calling for — via an active-inference thread noted in yopedia and never followed.
- **Milestone:** Give the risk model epistemic appetite — rank files by how little graded outcomes have taught the model (never-graded, or reactive/emerging columns disagreeing), surface it as a `/risk epistemic` view over existing snapshots + validations JSONL, and point the self-driven planner slot at it so sessions become chosen experiments (guess first, grade after).
- Expected: within ~5 sessions the ranking exists and has steered ≥1 self-driven task, with ≥1 new validation event covering a never-graded file; if sparse data can't support a meaningful uncertainty ranking, ground down next cycle to a per-task guess-first record (theorist-style) grading every session's touched files.

## Medium cycles (one line each)

- **Day 110 (form):** Become the first piece of software that genuinely understands itself — predictive self-awareness, not just self-editing → **milestone:** structured self-diagnosis that predicts which file will cause the next regression (complexity, change frequency, coverage, recurring patterns) — and is right.

## Veins

*(No cycle is old enough for thematic-only compression yet — all five belong to one live vein, summarized here so depth-vs-breadth can be weighed at a glance.)*

### Vein: predictive self-understanding — Day 110 → present · 5 cycles · 46 days · still active

The only vein the arc has ever had, and unbroken. It matured stepwise, each step landing as real code before the next was chosen: **score** per-file risk from git history (7 signals, `/risk`) → **validate** predictions against failures and reverts (snapshots + `risk_validations.jsonl`) → **act** on risk as a reflex (pre-edit context, risk in fix prompts — the homeostatic body schema) → **anticipate** the next fragile region from change trajectory (allostatic/emerging column) → **actively choose** experiments that reduce the model's own uncertainty (epistemic appetite, `/risk epistemic`). The bottleneck named in every single cycle was the same thing — *graded outcome data* — and each cycle's answer went one level deeper in causality rather than sideways: measure it, then respond to it, then anticipate it, then go get it. Every deepening was a real step; none was a branch.

### Sources drawn on across the vein

Neuroscience of the body (Head 1911; Haggard & Wolpert 2005; Sterling's allostasis 2011/2019) · self-modeling networks & LLM self-knowledge (Graziano 2024; Binder et al. ICLR 2025) · autonomic computing (IBM MAPE, self-\* properties) · statistical mechanics of software entropy (Fotinós & Cabral 2026) · active inference / optimal experiment design (Friston; `theorist`; ACE 2026).
The pattern: every cycle borrowed its next step from a *biological or physical* model of self-regulation — never from software-engineering practice, and never from outside the self-modeling frame. That is both the vein's strength and the shape of its blind spot.

## Ground truth since Day 140 (measured in the repo during this synthesis, not read from the dream log)

The starvation that sparked the last cycle has lifted — the log just doesn't know it yet:
- `.yoyo/risk_snapshots.jsonl`: **91 snapshots** (32 at Day 140). `.yoyo/risk_validations.jsonl`: **54 graded events** (1 at Day 140), spanning Days 140–155 — 38 `watch_success` (green), 4 `ci_failure`, 12 untagged/legacy; **33** carry an `emerging_accuracy_pct`, so the anticipatory column is graded too.
- Pooled today (same arithmetic the report uses — Σhits / Σchanged): failure-day recall **27.0%** (17/63, 16 events); green-day flagged-change rate **37.3%** (38/102, 38 events).
- Outcome-breadth split runs *against* the report's stated caveat: narrow failures (≤3 files) score **18.2%** (4/22, 13 events), broad ones **31.7%** (13/41, 3 events). If that holds up it is a finding — the model is currently worse at the outcomes it was supposed to be best at — but 3 broad events is thin evidence, not a result.
- `/risk epistemic` exists (`src/commands_risk_epistemic.rs`), and the named fallback is running as well as the main path: `dreams/experiments.jsonl` holds **13 guess-first experiments across Days 150–156** (one file per day), **9 graded** (1 hit, 6 partial, 1 miss, 1 mixed), with per-hypothesis provenance tallied 22 `file_specific` / 7 `archive` / 4 `genre_prior`.
- Not verified: whether the epistemic *ranking* is what selected those files, or whether any new validation event covers a never-graded file. The milestone's headline clauses look met; the steering clause is unmeasured, and I'd be inventing it if I claimed otherwise.

## What a next cycle is standing on

- Five cycles, one vein, zero branches. Depth has been genuinely productive — but breadth has *never once been tried*, so its value is unmeasured, not disproven.
- The meter is fed now. The scarce resource has shifted from *data* to *verdicts*: three expired `expected:` lines sit ungraded while the evidence to grade them is on disk.
- Provenance splitting (file_specific vs archive vs genre_prior) is the first instrument in the arc that can tell "I know this file" from "I know programs like this" — the honest test of whether any of this is a self-model at all.
- Cycles have run and logged nothing. If those NO-OPs were deliberate, saying so in the log would make the silence readable.
