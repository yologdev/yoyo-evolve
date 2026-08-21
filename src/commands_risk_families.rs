//! Hypothesis-family tally for the chosen-experiment record shown under
//! `/risk epistemic` — splits graded hypotheses by **provenance** (archive-derived,
//! file-specific, genre-prior, plus the explicit `Unknown` fourth value) and
//! renders the block that reports them.
//!
//! A **pure Day-174 move** out of `commands_risk_epistemic.rs`, which was sitting
//! at 2002 lines against the fatal `MAX_MODULE_LINES = 2000` gate, surviving only
//! on the 50-line grace band. No body was rewritten, no test was deleted, and
//! nothing about the tally or its output changed — the items live one file over
//! and are re-exported by the parent, so every call site is unchanged.

use crate::commands_risk_ungraded::{format_ungraded_rounds, ungraded_rounds, UngradedScan};
use crate::format::{BOLD, DIM, RESET, YELLOW};

/// Where a hypothesis came from. `Unknown` is the deliberate **third value**
/// (Day 144): a missing, misspelled or null `provenance` is never quietly
/// bucketed into one of the two real families — absence gets its own name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Provenance {
    /// Derived from my own learnings archive ("does my hottest lesson apply
    /// here?"). A hit grades the *lesson's generality*, not the file model.
    Archive,
    /// Derived from something about the file itself — its role, callers, age,
    /// dependencies, consumers. A hit grades my model of *that file*.
    FileSpecific,
    /// True of programs *of this kind*, not of this file: "any stateful
    /// line-mode renderer with an `is_in_code_block` flag forgets to reset at
    /// end of stream". A hit grades the **genre prior** — that programs of this
    /// shape commonly have the defect — not my model of this file. Day 154: the
    /// mechanical test is whether the sentence could be pasted verbatim into an
    /// experiment about a stranger's file with the same role; if yes it belongs
    /// here, not in [`Provenance::FileSpecific`].
    GenrePrior,
    /// Not stated, or stated as something I don't recognise.
    Unknown,
}

impl Provenance {
    fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim) {
            Some("archive") => Provenance::Archive,
            Some("file_specific") => Provenance::FileSpecific,
            Some("genre_prior") => Provenance::GenrePrior,
            _ => Provenance::Unknown,
        }
    }
}

/// Hit/miss record for one hypothesis family.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyTally {
    /// Hypotheses in this family that carry a non-empty grade.
    pub(crate) graded: usize,
    /// Of those, how many graded `hit`.
    pub(crate) hits: usize,
    /// Of those, how many graded `partial`.
    pub(crate) partials: usize,
}

impl FamilyTally {
    fn record(&mut self, grade: &str) {
        self.graded += 1;
        match grade {
            "hit" => self.hits += 1,
            "partial" => self.partials += 1,
            _ => {}
        }
    }
}

/// The chosen-experiment record, split by where each hypothesis came from.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ExperimentFamilies {
    pub(crate) archive: FamilyTally,
    pub(crate) file_specific: FamilyTally,
    /// Hypotheses true of programs *of this kind* — see [`Provenance::GenrePrior`].
    pub(crate) genre_prior: FamilyTally,
    pub(crate) unknown: FamilyTally,
    /// Graded experiment results written before per-hypothesis provenance
    /// existed. Counted and disclosed, never back-filled — rewriting history
    /// in the ledger would manufacture evidence.
    pub(crate) experiments_without_hypotheses: usize,
    /// Rounds started and never graded — see [`ungraded_rounds`]. Detection
    /// only: the meter says a round is owed, it does not grade it.
    pub(crate) ungraded: UngradedScan,
}

impl ExperimentFamilies {
    pub(crate) fn total_graded(&self) -> usize {
        self.archive.graded
            + self.file_specific.graded
            + self.genre_prior.graded
            + self.unknown.graded
    }

    /// Nothing to say at all — no graded hypotheses and no predating results.
    pub(crate) fn is_empty(&self) -> bool {
        self.total_graded() == 0
            && self.experiments_without_hypotheses == 0
            && self.ungraded.ungraded.is_empty()
    }
}

/// Tally the experiment ledger's per-hypothesis grades by provenance family.
///
/// Pure over the ledger text so it is testable without touching the real file.
/// Both keys it reads are **optional** — every line written before Day 151
/// lacks them and parses exactly as before:
///
/// * `type: "experiment"` may carry `hypotheses: [{id, provenance, ...}]`
///   (the declaration, used as a provenance fallback);
/// * `type: "experiment_result"` may carry
///   `hypothesis_grades: [{id, provenance, graded}]` (the grades).
///
/// A graded result with no usable per-hypothesis records counts toward
/// `experiments_without_hypotheses` instead of vanishing. Ungraded
/// (`"graded": null`) hypotheses and malformed lines contribute nothing —
/// same discipline as [`parse_experiment_visits`].
pub(crate) fn tally_hypothesis_families(ledger_text: &str) -> ExperimentFamilies {
    use std::collections::HashMap;

    let mut declared: HashMap<(String, String), Provenance> = HashMap::new();
    let mut out = ExperimentFamilies::default();

    // Pass 1: provenance declared on the `experiment` (guess) lines, keyed by
    // (target, hypothesis id) so ids only have to be unique per experiment.
    for line in ledger_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        if val["type"].as_str() != Some("experiment") {
            continue;
        }
        let (Some(target), Some(hyps)) = (val["target"].as_str(), val["hypotheses"].as_array())
        else {
            continue;
        };
        for h in hyps {
            let Some(id) = h["id"].as_str() else { continue };
            let prov = Provenance::parse(h["provenance"].as_str());
            if prov != Provenance::Unknown {
                declared.insert((target.trim().to_string(), id.trim().to_string()), prov);
            }
        }
    }

    // Pass 2: the grades.
    for line in ledger_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        if val["type"].as_str() != Some("experiment_result") {
            continue;
        }
        let target = val["target"].as_str().unwrap_or("").trim().to_string();
        let overall_graded = matches!(val["graded"].as_str(), Some(g) if !g.trim().is_empty());

        let records = val["hypothesis_grades"].as_array();
        let mut saw_record = false;
        if let Some(records) = records {
            for r in records {
                saw_record = true;
                // Ungraded hypothesis: counted in neither `graded` nor `hits`.
                let Some(grade) = r["graded"]
                    .as_str()
                    .map(str::trim)
                    .filter(|g| !g.is_empty())
                else {
                    continue;
                };
                let mut prov = Provenance::parse(r["provenance"].as_str());
                if prov == Provenance::Unknown {
                    if let Some(id) = r["id"].as_str() {
                        if let Some(d) = declared.get(&(target.clone(), id.trim().to_string())) {
                            prov = *d;
                        }
                    }
                }
                match prov {
                    Provenance::Archive => out.archive.record(grade),
                    Provenance::FileSpecific => out.file_specific.record(grade),
                    Provenance::GenrePrior => out.genre_prior.record(grade),
                    Provenance::Unknown => out.unknown.record(grade),
                }
            }
        }
        if !saw_record && overall_graded {
            out.experiments_without_hypotheses += 1;
        }
    }

    // Same ledger text, same read: which rounds were started and never graded.
    out.ungraded = ungraded_rounds(ledger_text);

    out
}

/// Render one family line — or an honest "none yet" when the denominator is
/// empty. A rate over zero graded hypotheses is the exact mistake Days 142 and
/// 144 cost me twice, so it is never printed.
fn format_family_line(label: &str, human: &str, tally: &FamilyTally) -> String {
    if tally.graded == 0 {
        return format!("    {DIM}{label:<14} (no {human} hypotheses recorded yet){RESET}\n");
    }
    let partials = if tally.partials > 0 {
        format!(" (+{} partial)", tally.partials)
    } else {
        String::new()
    };
    format!(
        "    {label:<14} {YELLOW}{}{RESET} hit / {} graded{DIM}{partials}{RESET}\n",
        tally.hits, tally.graded
    )
}

/// The chosen-experiment scoreboard, split by hypothesis provenance.
///
/// Empty tally → empty string. Silence beats an empty scoreboard.
///
/// Consumer note: every line here must match neither
/// `scripts/extract_trajectory.py::EPISTEMIC_ENTRY_RE` (`N. path score`) nor
/// the `•` reason bullet it appends to the previous ranked entry. The block is
/// also rendered *below* the "never forecast" header, where that parser has
/// already hard-stopped collecting (`EPISTEMIC_NEVER_FORECAST_RE`).
pub(crate) fn format_experiment_families(fam: &ExperimentFamilies) -> String {
    if fam.is_empty() {
        return String::new();
    }
    let total = fam.total_graded();
    let mut out = String::new();
    out.push_str(&format!(
        "\n  {BOLD}chosen-experiment record (guess-first){RESET}{DIM}: {total} graded hypothes{}{RESET}\n",
        if total == 1 { "is" } else { "es" }
    ));
    out.push_str(&format_family_line(
        "file-specific",
        "file-specific",
        &fam.file_specific,
    ));
    out.push_str(&format_family_line("archive", "archive", &fam.archive));
    // Only shown when non-empty, same call as `unrecognised`: until a round
    // files one, an always-zero genre-prior row would be noise.
    if fam.genre_prior.graded > 0 {
        out.push_str(&format_family_line(
            "genre-prior",
            "genre-prior",
            &fam.genre_prior,
        ));
    }
    // Only shown when non-empty: an "unrecognised provenance" row that is
    // always zero is noise, but hiding real entries in it would be a lie.
    if fam.unknown.graded > 0 {
        out.push_str(&format_family_line(
            "unrecognised",
            "unrecognised provenance",
            &fam.unknown,
        ));
        out.push_str(&format!(
            "    {DIM}unrecognised provenance: neither family claimed these — absence is not a family.{RESET}\n"
        ));
    }
    if fam.experiments_without_hypotheses > 0 {
        out.push_str(&format!(
            "    {DIM}{} earlier experiment(s) predate per-hypothesis provenance and are not counted here.{RESET}\n",
            fam.experiments_without_hypotheses
        ));
    }
    out.push_str(&format!(
        "    {DIM}archive-derived hits measure how general a recent lesson is; file-specific hits{RESET}\n"
    ));
    out.push_str(&format!(
        "    {DIM}measure my model of that file. Only the second is what the dream is after.{RESET}\n"
    ));
    out.push_str(&format!(
        "    {DIM}A genre prior hit is the weakest of the three: it says programs of this KIND{RESET}\n"
    ));
    out.push_str(&format!(
        "    {DIM}commonly have the defect — true of a stranger's file too, so it proves no self-model.{RESET}\n"
    ));
    if let Some(line) = format_ungraded_rounds(&fam.ungraded) {
        out.push_str(&line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Emission point for the slip detector: the block a reader actually
    /// receives names the owed rounds, and says nothing at all when the ledger
    /// is clean. `tally_hypothesis_families` reads the same ledger text once,
    /// so the detector rides the existing `/risk epistemic` surface.
    #[test]
    fn test_family_block_names_ungraded_rounds_and_is_silent_when_clean() {
        let graded_round = concat!(
            r#"{"type":"experiment","round":57,"day":169,"target":"src/docs.rs","hypotheses":[{"id":"h1","provenance":"file_specific","claim":"c"}]}"#,
            "\n",
            r#"{"type":"experiment_result","round":57,"day":169,"target":"src/docs.rs","graded":"hit","hypothesis_grades":[{"id":"h1","graded":"hit"}]}"#,
            "\n",
        );
        // Clean ledger: the block renders, the warning does not.
        let clean = format_experiment_families(&tally_hypothesis_families(graded_round));
        assert!(clean.contains("chosen-experiment record"), "{clean}");
        assert!(
            !clean.contains("never graded"),
            "a clean ledger prints no warning: {clean}"
        );

        // Same ledger plus the colliding round 57 that was never graded.
        let slipped = format!(
            concat!(
                "{}",
                r#"{{"type":"experiment","round":57,"day":171,"target":"src/commands_plan.rs"}}"#,
                "\n",
                r#"{{"type":"experiment","round":58,"day":172,"target":"src/config_paths.rs"}}"#,
                "\n",
            ),
            graded_round
        );
        let report = format_experiment_families(&tally_hypothesis_families(&slipped));
        assert!(
            report.contains("2 round(s) started but never graded"),
            "{report}"
        );
        assert!(
            report.contains("57 (day 171, src/commands_plan.rs)"),
            "{report}"
        );
        assert!(
            report.contains("58 (day 172, src/config_paths.rs)"),
            "{report}"
        );
        // The graded twin is not reported as owed.
        assert!(
            !report.contains("57 (day 169"),
            "the graded round 57 is not owed: {report}"
        );
        // Consumer guard: `scripts/extract_trajectory.py` hard-stops above this
        // block on `never forecast` and matches rows on `N. path score` / `◦`.
        assert!(!report.contains("never forecast"), "{report}");
    }

    /// `genre_prior` declared on the `experiment` line is the fallback when the
    /// grade record omits provenance — same path as the other two families.
    #[test]
    fn test_families_genre_prior_falls_back_to_declaration() {
        let ledger = concat!(
            r#"{"type":"experiment","day":154,"target":"src/a.rs","hypotheses":[{"id":"h1","provenance":"genre_prior","claim":"c"}]}"#,
            "\n",
            r#"{"type":"experiment_result","day":154,"target":"src/a.rs","graded":"hit","hypothesis_grades":[{"id":"h1","graded":"hit"}]}"#,
            "\n",
        );
        let fam = tally_hypothesis_families(ledger);
        assert_eq!(fam.genre_prior.hits, 1);
        assert_eq!(fam.unknown.graded, 0);
        assert_eq!(fam.file_specific.graded, 0);
    }

    /// An ungraded hypothesis counts in neither `graded` nor `hits`.
    #[test]
    fn test_families_ungraded_hypothesis_counts_nowhere() {
        let ledger = concat!(
            r#"{"type":"experiment_result","day":151,"target":"src/a.rs","graded":"hit","hypothesis_grades":[{"id":"h1","provenance":"archive","graded":null},{"id":"h2","provenance":"archive","graded":"partial"}]}"#,
            "\n",
        );
        let fam = tally_hypothesis_families(ledger);
        assert_eq!(fam.archive.graded, 1);
        assert_eq!(fam.archive.hits, 0);
        assert_eq!(fam.archive.partials, 1);
    }

    /// Provenance declared on the `experiment` line is used when the grade
    /// record omits it — same target, same hypothesis id.
    #[test]
    fn test_families_provenance_falls_back_to_declaration() {
        let ledger = concat!(
            r#"{"type":"experiment","day":151,"target":"src/a.rs","hypotheses":[{"id":"h1","provenance":"file_specific","claim":"c"}]}"#,
            "\n",
            r#"{"type":"experiment_result","day":151,"target":"src/a.rs","graded":"hit","hypothesis_grades":[{"id":"h1","graded":"hit"}]}"#,
            "\n",
        );
        let fam = tally_hypothesis_families(ledger);
        assert_eq!(fam.file_specific.hits, 1);
        assert_eq!(fam.unknown.graded, 0);
    }

    /// Malformed lines are skipped without panicking, and don't poison the rest.
    #[test]
    fn test_families_malformed_lines_are_skipped() {
        let ledger = concat!(
            "not json at all\n",
            "\n",
            r#"{"type":"experiment_result","day":151,"target":"src/a.rs","graded":"hit","hypothesis_grades":"oops"}"#,
            "\n",
            r#"{"type":"experiment_result","day":151,"target":"src/b.rs","graded":"hit","hypothesis_grades":[{"id":"h1","provenance":"archive","graded":"hit"}]}"#,
            "\n",
        );
        let fam = tally_hypothesis_families(ledger);
        assert_eq!(fam.archive.hits, 1);
        // The `"oops"` line has no usable per-hypothesis records but IS graded,
        // so it counts as predating — absence is not absorbed into a family.
        assert_eq!(fam.experiments_without_hypotheses, 1);
    }
}
