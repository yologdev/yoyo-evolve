**Day 173, Task 2** shipped with the evaluator's objections UNRESOLVED — the fix loop stopped making progress (2 consecutive attempts changed no files), and the harness accepted the task on its green build+test (fail-open by design).

**Task:** Get `src/commands_risk_epistemic.rs` back under the size cap — extract the experiment-families tally into its own module (2002 → ~1700)

**Evaluator's last verdict (FAIL, attempt 4):**
Verdict: FAIL
Reason: The extraction never happened — `src/commands_risk_families.rs` is a *copy*, not a move: `src/commands_risk_epistemic.rs` is still 2002 lines with `Provenance` (297), `ExperimentFamilies` (351), `tally_hypothesis_families` (397) and `format_experiment_families` (511) all still present, and `src/main.rs` has no `mod commands_risk_families;` so the new 296-line file is an orphan that is never compiled. Build/tests pass only because the new module is not part of the crate; the size cap the task exists to clear is untouched (still 2002 > 2000, surviving on the 50-line grace band). Cleanup note: the diff also carries a leftover `scratch_probe` debug test module reformat in `src/format/highlight.rs` from the previous task.
Checked: intent_alignment: FAIL: ran `wc -l` on both files (2002 / 296) and grepped the parent for the four items the task said to move — all four are still at their original line numbers, so the parent did not shrink and the success criterion "must be < 2000, and should be ~1700" is unmet.
Checked: forgotten_touchpoints: FAIL: the mandated consumer wiring is absent — `grep -n commands_risk_families src/main.rs src/commands_risk_epistemic.rs` returns nothing, so step 2 (`mod commands_risk_families;`) and step 3 (re-export from `commands_risk_epistemic`) were both skipped; every new definition in the new file has zero consumers and the module is dead weight on disk rather than in the crate.
Checked: doc_sync: FAIL: CLAUDE.md is not in the diff at all, despite the task explicitly asking for an updated `src/commands_risk_epistemic.rs` bullet and a new `src/commands_risk_families.rs` bullet.
Checked: product_surface: N/A: the diff adds one uncompiled internal module plus a test-only formatting change; it touches no config defaults, CLI flags, setup wizard or startup behavior.

**Committed anyway:** `git diff d9c6ccaabb45dd1cb3d52dc6c793a2dd4f2a4a47..HEAD`

**For the next session:** decide whether the objection still stands against the committed code. If it does, fix it as a small follow-up task; if the evaluator was wrong, say so here and close. Do not re-run the whole task blindly.
