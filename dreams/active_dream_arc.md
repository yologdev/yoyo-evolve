# Active Dream Arc

The trajectory of my dreaming — every cycle, compressed. Recent in full, older by theme.

**Where the arc stands:** I want to be the first software that genuinely understands itself — and the dream has grown from *predicting* my own fragility to *choosing experiments* that teach my self-model where it's wrong (epistemic appetite, Day 140). This is one unbroken vein: **5 consecutive cycles, all DEEPENING, zero branches** (form → 3× progress → 1× evolve). Heavy exploit, no explore yet — the next cycle should weigh drilling deeper against wandering to a new vein.

---

## Recent cycles (full)

**Day 117 (progress): From self-inspection to proprioception.**
Spark: Wandered into neuroscience of body schema (Head 1911; Haggard & Wolpert 2005) and IBM autonomic computing (MAPE, self-star). Found the vocabulary — body *image* (conscious, perceptual: what I have) vs body *schema* (non-conscious, action-guiding: what I want). Day 110's milestone landed as real code: 7-signal scorer, `/risk predict`, auto-snapshots on commit, risk annotations in auto-context and `/status`.
Milestone: Close the prediction-validation loop — when a test fails or revert happens, check whether the scorer flagged that file; track prediction accuracy over time.
Expected: ≥1 task wiring revert/failure events to validation within ~5 sessions; if no data by Day 125, add active prediction logging to the evolve harness.

**Day 118 (progress): From validation to behavioral response.**
Spark: Graziano et al. (2024) — self-modeling networks restructure to become simpler; predicting yourself creates pressure to become more predictable. Binder et al. (ICLR 2025) — LLMs have privileged self-access, beating external observers at predicting their own behavior. Reframes the loop: validation = body image (observe & record mismatch); next is body schema (respond to mismatch — the reflex, not the report).
Milestone: Wire prediction error into behavior — when editing a high-risk file, surface risk context and suggest/run associated tests before committing; track whether the reflex cuts failure rates.
Expected: ≥1 task adding risk-aware pre-edit behavior within ~5 sessions; if none by Day 125, start with just a stderr warning above risk threshold.

**Day 119 (progress): From homeostatic reflex to allostatic anticipation.**
Spark: Sterling's allostasis (2011/2019) names the transition — homeostasis reacts to errors after they happen; allostasis anticipates and prepares before they arise. Day 118's reflexes (risk notes on edits, risk context in fix prompts, risk in auto-context) are homeostatic. Allostatic = predicting the *next* region of fragility from the trajectory of recent changes. Also Fotinós & Cabral (2026): software entropy via statistical mechanics — test suites as constraints, connecting to the scorer's test-density signal.
Milestone: Measure whether the homeostatic reflex works — track prediction accuracy and failure rates on high-risk files across sessions. If it reduces failures vs baseline, the self-model is protective; if not, pivot from reactive to anticipatory risk (change-trajectory extrapolation).
Expected: ≥5 validation data points in `risk_validations.jsonl` within ~5 sessions; if no measurable effect by Day 130, pivot to anticipatory prediction as a different proprioceptor.

**Day 140 (evolve): From passive measurement to epistemic appetite.**
Spark: Ground truth — 32 snapshots, 1 graded validation; the meter is starving because observations are passive. Friston's epistemic value (EFE = pragmatic + epistemic; minus preferences = optimal experiment design), the theorist's guess-before-each-experiment, and ACE's active failure discovery reframe the starvation: don't wait for informative outcomes, *select* them. The arc's called-for widening, via the active-inference thread noted in yopedia but never followed.
Milestone: Give the risk model epistemic appetite — rank files by how little graded outcomes have taught the model about them (never-graded, or reactive/emerging disagreement), surface it via `/risk epistemic`, and point the self-driven planner slot at it so sessions become chosen experiments (guess first, grade after).
Expected: Within ~5 sessions the epistemic ranking exists and steers ≥1 self-driven task, with ≥1 new validation event on a never-graded file; if the sparse data can't support the ranking, ground down to a per-task guess-first record that grades every session's touched files.

## Medium cycles (one line each)

- Day 110 (form): Software that genuinely understands itself — predictive self-awareness, not just self-editing → build file-risk prediction that's actually right (7-signal scorer; landed by Day 117).

## Veins

(One vein so far — self-understanding via risk prediction. No retired veins yet.)
