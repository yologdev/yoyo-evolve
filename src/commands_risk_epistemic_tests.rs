//! Render- and ordering-level tests for [`crate::commands_risk_epistemic`].
//!
//! A `#[cfg(test)]`-only sibling (same shape as `help_data_guards.rs`, Day
//! 166): the parent hit the 2000-line module cap while the study-tier fix was
//! landing, and these two modules moved out **verbatim** — same tests, same
//! names, same fixtures; the only edits are the `use super::*;` lines becoming
//! absolute paths. Test paths are now
//! `commands_risk_epistemic_tests::{reason_truncation_tests,study_tier_tests}::…`.

#[cfg(test)]
mod reason_truncation_tests {
    use crate::commands_risk_epistemic::*;
    use crate::commands_risk_snapshots::ParsedSnapshot;

    /// Local snapshot builder — the sibling `tests` module's helper is private
    /// to it, and one non-empty snapshot is all these render tests need.
    fn snap(day: u64, predicted: &[&str]) -> ParsedSnapshot {
        ParsedSnapshot {
            day,
            git_hash: format!("hash{day}"),
            ts: format!("2026-08-{:02}T00:00:00Z", (day % 28) + 1),
            predicted: predicted.iter().map(|s| s.to_string()).collect(),
            emerging: Vec::new(),
        }
    }

    #[test]
    fn short_reason_comes_back_byte_identical_and_uncut() {
        let r = "predicted 17×, never graded";
        let (out, cut) = truncate_reason(r, REASON_MAX_CHARS);
        assert_eq!(out, r, "a short reason must not be rewritten at all");
        assert!(!cut, "nothing was cut, so the flag must be false");
    }

    #[test]
    fn long_ascii_reason_is_capped_and_marked() {
        let r = "x".repeat(1_600);
        let (out, cut) = truncate_reason(&r, 140);
        assert!(cut, "a 1600-char reason is cut");
        assert!(
            out.chars().count() <= 140,
            "cap counts the FINAL string incl. ellipsis, got {}",
            out.chars().count()
        );
        assert!(out.ends_with('…'), "a cut must be visible: {out}");
    }

    #[test]
    fn cut_inside_a_multi_byte_char_does_not_panic() {
        // Every char is 3 bytes, so any byte-index cut lands mid-character.
        let r = "→".repeat(600);
        for cap in [0usize, 1, 2, 3, 7, 139, 140] {
            let (out, _) = truncate_reason(&r, cap);
            assert!(
                out.chars().count() <= cap,
                "cap {cap} exceeded: {}",
                out.chars().count()
            );
        }
        let (out, cut) = truncate_reason(&"✓ ok ".repeat(200), 90);
        assert!(cut);
        assert!(out.chars().count() <= 90);
    }

    #[test]
    fn embedded_newlines_and_tabs_collapse_to_one_line() {
        let (out, cut) = truncate_reason("first\nsecond\tthird\r\nfourth", 140);
        assert!(!out.contains('\n'), "a reason is one line: {out:?}");
        assert!(!out.contains('\t'), "a reason is one line: {out:?}");
        assert!(!out.contains('\r'), "a reason is one line: {out:?}");
        assert!(!cut, "flattening whitespace is not a truncation");
        assert_eq!(out, "first second third  fourth");
    }

    #[test]
    fn zero_cap_is_an_explicit_case_not_an_accident() {
        assert_eq!(truncate_reason("", 0), (String::new(), false));
        assert_eq!(truncate_reason("anything", 0), (String::new(), true));
    }

    /// The consumer contract, not just the helper: `EPISTEMIC_STUDIED_RE` in
    /// `scripts/extract_trajectory.py` is
    /// `studied by graded experiment \(day (\d+), ([^)]+)\)` — it needs the
    /// prefix AND the closing paren. A naive cut eats the paren and the
    /// planner's compaction silently stops matching.
    #[test]
    fn studied_reason_keeps_its_parsable_shape_when_cut() {
        let reason = format!(
            "studied by graded experiment (day 164, {})",
            "2 clean hits / 5 (h2, h5); 3 partial ".repeat(45)
        );
        assert!(reason.chars().count() > 1_600);
        let (out, cut) = truncate_reason(&reason, REASON_MAX_CHARS);
        assert!(cut);
        assert!(out.chars().count() <= REASON_MAX_CHARS);
        assert!(
            out.starts_with("studied by graded experiment (day 164, "),
            "the prefix the parser anchors on must survive verbatim: {out}"
        );
        assert!(
            out.ends_with("…)"),
            "the closing paren must survive the cut or the regex stops matching: {out}"
        );
    }

    #[test]
    fn report_caps_reason_bullets_and_discloses_the_cut() {
        let snapshots = vec![snap(1, &["src/a.rs"])];
        let entries = vec![EpistemicEntry {
            path: "src/a.rs".to_string(),
            score: 2.0,
            reasons: vec![
                "predicted 3×, never graded".to_string(),
                format!(
                    "studied by graded experiment (day 164, {})",
                    "y".repeat(1_600)
                ),
            ],
            risk_score: Some(1.0),
            tier: StudyTier::NeverStudied,
        }];
        let report = format_epistemic_report(
            &snapshots,
            &entries,
            &NeverForecastGroups::default(),
            &ExperimentFamilies::default(),
        );
        for line in report.lines() {
            assert!(
                line.chars().count() < 400,
                "a reason bullet escaped the cap: {line}"
            );
        }
        assert!(
            report.contains("1 reason shortened for display"),
            "an elision I own must mark its cut in-band:\n{report}"
        );
        assert!(
            report.contains("dreams/experiments.jsonl"),
            "the disclosure must say where the full text lives:\n{report}"
        );
    }

    #[test]
    fn no_disclosure_line_when_nothing_was_cut() {
        let snapshots = vec![snap(1, &["src/a.rs"])];
        let entries = vec![EpistemicEntry {
            path: "src/a.rs".to_string(),
            score: 2.0,
            reasons: vec!["predicted 3×, never graded".to_string()],
            risk_score: Some(1.0),
            tier: StudyTier::NeverStudied,
        }];
        let report = format_epistemic_report(
            &snapshots,
            &entries,
            &NeverForecastGroups::default(),
            &ExperimentFamilies::default(),
        );
        assert!(
            !report.contains("shortened for display"),
            "silence is correct when nothing was cut:\n{report}"
        );
    }
}

/// Tests for the study *tier* — the predicate form of the study weights, and
/// the primary sort key of the ranked list. The weights alone could not steer
/// the top-N selector in `scripts/extract_trajectory.py`; these pin that the
/// tier does, at the **emission point** (the report string a caller receives),
/// not just one layer below it on the sort helper.
#[cfg(test)]
mod study_tier_tests {
    use crate::commands_risk_epistemic::*;
    use crate::commands_risk_snapshots::{GradedEvent, ParsedSnapshot};

    fn snap(day: u64, predicted: &[&str]) -> ParsedSnapshot {
        ParsedSnapshot {
            day,
            git_hash: format!("hash{day}"),
            ts: format!("2026-08-{:02}T00:00:00Z", (day % 28) + 1),
            predicted: predicted.iter().map(|s| s.to_string()).collect(),
            emerging: Vec::new(),
        }
    }

    fn entry(path: &str, score: f64, tier: StudyTier) -> EpistemicEntry {
        EpistemicEntry {
            path: path.to_string(),
            score,
            reasons: vec!["predicted 3×, never graded".to_string()],
            risk_score: Some(1.0),
            tier,
        }
    }

    #[test]
    fn study_tier_maps_all_three_states() {
        assert_eq!(study_tier(None), StudyTier::NeverStudied);
        assert_eq!(
            study_tier(Some(&StudyState::VisitedUngraded)),
            StudyTier::VisitedUngraded
        );
        assert_eq!(
            study_tier(Some(&StudyState::Graded("3 hits".to_string()))),
            StudyTier::Graded
        );
    }

    #[test]
    fn tier_order_is_dark_first() {
        // The whole fix rests on this ordering: reordering the variants must
        // fail a test, not silently reorder the planner's exploration budget.
        assert!(StudyTier::NeverStudied < StudyTier::VisitedUngraded);
        assert!(StudyTier::VisitedUngraded < StudyTier::Graded);
        let mut v = vec![
            StudyTier::Graded,
            StudyTier::NeverStudied,
            StudyTier::VisitedUngraded,
        ];
        v.sort();
        assert_eq!(
            v,
            vec![
                StudyTier::NeverStudied,
                StudyTier::VisitedUngraded,
                StudyTier::Graded
            ]
        );
    }

    #[test]
    fn ranking_puts_a_low_scoring_dark_file_above_a_high_scoring_studied_one() {
        // The exact shape observed live on Day 169: a never-studied file whose
        // only signal is staleness scores 0.5, while a file that is never
        // graded + stale scores 1.5 even after the -1.0 study discount. The
        // weight cannot demote it below the dark one — that is the defect —
        // so the tier must.
        let snapshots = vec![
            snap(1, &["src/dark.rs", "src/studied.rs"]),
            snap(2, &["src/other.rs"]),
            snap(3, &["src/other.rs"]),
            snap(4, &["src/other.rs"]),
            snap(5, &["src/other.rs"]),
            snap(6, &["src/other.rs"]),
            snap(7, &["src/other.rs"]),
        ];
        // dark.rs was graded once, before its last snapshot appearance: it
        // loses W_NEVER_GRADED and keeps only W_STALE.
        let events = vec![GradedEvent {
            day: 0,
            paths: vec!["src/dark.rs".to_string()],
        }];
        let experiments = vec![ExperimentVisit {
            path: "src/studied.rs".to_string(),
            day: 165,
            state: StudyState::Graded("4 of 5 hit".to_string()),
        }];
        let ranked = compute_epistemic_ranking(&snapshots, &events, &[], &experiments);
        let dark = ranked.iter().find(|e| e.path == "src/dark.rs").unwrap();
        let studied = ranked.iter().find(|e| e.path == "src/studied.rs").unwrap();
        assert!(
            studied.score > dark.score,
            "fixture must have the studied file scoring higher: {} vs {}",
            studied.score,
            dark.score
        );
        let pos = |p: &str| ranked.iter().position(|e| e.path == p).unwrap();
        assert!(
            pos("src/dark.rs") < pos("src/studied.rs"),
            "the 0.5 dark room must rank above the 1.5 studied one: {ranked:?}"
        );
        assert_eq!(dark.tier, StudyTier::NeverStudied);
        assert_eq!(studied.tier, StudyTier::Graded);
        // Every unstudied file, including the filler, sorts ahead of the
        // studied one — the tier is a group boundary, not a nudge.
        assert_eq!(
            ranked.last().unwrap().path,
            "src/studied.rs",
            "the studied file ranks last: {ranked:?}"
        );
    }

    #[test]
    fn report_shows_a_lower_scoring_dark_file_before_a_higher_scoring_studied_one() {
        // Emission point: the string a caller receives, not the sort helper.
        let snapshots = vec![snap(1, &["src/dark.rs"])];
        let entries = vec![
            entry("src/dark.rs", 0.5, StudyTier::NeverStudied),
            entry("src/visited.rs", 1.0, StudyTier::VisitedUngraded),
            entry("src/studied.rs", 2.0, StudyTier::Graded),
        ];
        let report = format_epistemic_report(
            &snapshots,
            &entries,
            &NeverForecastGroups::default(),
            &ExperimentFamilies::default(),
        );
        let dark_at = report.find("src/dark.rs").expect("dark file rendered");
        let studied_at = report
            .find("src/studied.rs")
            .expect("studied file rendered");
        assert!(
            dark_at < studied_at,
            "a 0.5 dark file must render above a 2.0 studied one:\n{report}"
        );
        for tier in [
            StudyTier::NeverStudied,
            StudyTier::VisitedUngraded,
            StudyTier::Graded,
        ] {
            assert!(
                report.contains(study_tier_header(tier)),
                "missing header for {tier:?}:\n{report}"
            );
        }
        // Numbering stays contiguous across the group headers — the planner
        // reads those numbered rows.
        assert!(report.contains(" 1. "), "row 1 missing:\n{report}");
        assert!(report.contains(" 2. "), "row 2 missing:\n{report}");
        assert!(report.contains(" 3. "), "row 3 missing:\n{report}");
    }

    #[test]
    fn a_header_is_only_emitted_for_a_group_that_appears() {
        let snapshots = vec![snap(1, &["src/dark.rs"])];
        let entries = vec![entry("src/dark.rs", 1.0, StudyTier::NeverStudied)];
        let report = format_epistemic_report(
            &snapshots,
            &entries,
            &NeverForecastGroups::default(),
            &ExperimentFamilies::default(),
        );
        assert!(report.contains(study_tier_header(StudyTier::NeverStudied)));
        assert!(
            !report.contains(study_tier_header(StudyTier::Graded)),
            "an empty group must not announce itself:\n{report}"
        );
    }

    #[test]
    fn headers_respect_the_trajectory_parsers_two_hard_constraints() {
        // 1. `EPISTEMIC_NEVER_FORECAST_RE = re.compile(r"never forecast")` is a
        //    hard stop for entry collection in scripts/extract_trajectory.py —
        //    a header carrying it would truncate the planner's list to nothing.
        // 2. `EPISTEMIC_ENTRY_RE = r"^\s*\d+\.\s+(\S+)\s+(\d+(?:\.\d+)?)\s*$"`
        //    — a header must not read as a numbered entry. Checked, not eyeballed.
        for tier in [
            StudyTier::NeverStudied,
            StudyTier::VisitedUngraded,
            StudyTier::Graded,
        ] {
            let h = study_tier_header(tier);
            assert!(
                !h.contains("never forecast"),
                "header would hard-stop the planner's parser: {h}"
            );
            let trimmed = h.trim_start();
            assert!(
                !trimmed
                    .split('.')
                    .next()
                    .is_some_and(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_digit())),
                "header must not start with `N.` or it reads as an entry row: {h}"
            );
        }
    }
}
