//! Render- and ordering-level tests for [`crate::commands_risk_epistemic`].
//!
//! A `#[cfg(test)]`-only sibling (same shape as `help_data_guards.rs`, Day
//! 166): the parent hit the 2000-line module cap while the study-tier fix was
//! landing, and these two modules moved out **verbatim** — same tests, same
//! names, same fixtures; the only edits are the `use super::*;` lines becoming
//! absolute paths. Test paths are now
//! `commands_risk_epistemic_tests::{reason_truncation_tests,study_tier_tests}::…`.
//!
//! Day 171 added a third module, `stale_weight_tests`, here rather than in the
//! parent for the same reason: the parent is un-grandfathered against
//! `MAX_MODULE_LINES = 2000`, so only the const, the function and its doc
//! comment landed there.

#[cfg(test)]
mod stale_weight_tests {
    use crate::commands_risk_epistemic::*;

    /// Below the threshold the contribution is 0.0 — byte-identical to the
    /// pre-Day-171 binary step, which is the half of the old behaviour that
    /// was correct: a recently-seen file is not stale at all.
    #[test]
    fn below_the_threshold_contributes_nothing() {
        for gap in 0..STALE_SNAPSHOT_GAP {
            assert_eq!(
                stale_weight(gap),
                0.0,
                "gap {gap} is below STALE_SNAPSHOT_GAP and must not be stale"
            );
        }
    }

    /// The backward-compatibility anchor: *at* the threshold the function
    /// returns exactly `W_STALE`, so the old floor is preserved and the
    /// `test_score_is_sum_of_signals` fixture in the parent still holds.
    #[test]
    fn at_the_threshold_returns_exactly_w_stale() {
        assert_eq!(stale_weight(STALE_SNAPSHOT_GAP), W_STALE);
    }

    /// Monotone non-decreasing over a wide sweep, including the boundary and
    /// the saturating tail.
    #[test]
    fn is_monotone_non_decreasing() {
        let mut prev = stale_weight(0);
        for gap in [
            0usize,
            1,
            4,
            5,
            6,
            7,
            10,
            25,
            49,
            50,
            55,
            100,
            161,
            169,
            500,
            5_000,
            1_000_000,
            usize::MAX,
        ] {
            let w = stale_weight(gap);
            assert!(
                w >= prev,
                "stale_weight({gap}) = {w} decreased below the previous value {prev}"
            );
            prev = w;
        }
    }

    /// Bounded by `W_STALE_MAX` for every input, including `usize::MAX`, and
    /// never NaN. The bound is what keeps `W_NEVER_GRADED = 2.0` worth at
    /// least as much as maximal staleness: "never graded at all" and "graded
    /// long ago" are different facts and the first is the darker one.
    #[test]
    fn is_bounded_by_w_stale_max_and_never_nan() {
        for gap in [
            0usize,
            STALE_SNAPSHOT_GAP,
            STALE_SNAPSHOT_GAP + 1,
            1_000,
            usize::MAX / 2,
            usize::MAX - 1,
            usize::MAX,
        ] {
            let w = stale_weight(gap);
            assert!(!w.is_nan(), "stale_weight({gap}) produced NaN");
            assert!(
                w <= W_STALE_MAX,
                "stale_weight({gap}) = {w} exceeded W_STALE_MAX {W_STALE_MAX}"
            );
            assert!(
                w <= W_NEVER_GRADED,
                "maximal staleness outranked never-graded"
            );
        }
    }

    /// Regression guard tied to the defect this landed for: the three gaps
    /// observed live in `yoyo risk epistemic` at 06:47 on Day 171 all scored
    /// an identical 0.5 (W_STALE-only, after #726 removed the last
    /// discriminating signal). They must now be strictly ordered.
    #[test]
    fn the_live_day_171_gaps_are_strictly_discriminated() {
        let a = stale_weight(6);
        let b = stale_weight(49);
        let c = stale_weight(161);
        assert!(
            a < b,
            "stale_weight(6) = {a} must be below stale_weight(49) = {b}"
        );
        assert!(
            b < c,
            "stale_weight(49) = {b} must be below stale_weight(161) = {c}"
        );
        // And all three sit inside the documented band.
        for (gap, w) in [(6, a), (49, b), (161, c)] {
            assert!(
                (W_STALE..=W_STALE_MAX).contains(&w),
                "stale_weight({gap}) = {w} left the [W_STALE, W_STALE_MAX] band"
            );
        }
    }

    /// The other half of the same defect: `src/git.rs` (169 snapshots ago)
    /// must out-score `src/commands_risk.rs` (6 snapshots ago). That is the
    /// success criterion the task stated in advance, expressed at the level
    /// of the function that decides it.
    #[test]
    fn an_older_file_outscores_a_recent_one() {
        assert!(stale_weight(169) > stale_weight(6));
    }
}

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

/// The ledger's own slip detector — [`crate::commands_risk_ungraded`].
///
/// Round ids in `dreams/experiments.jsonl` are hand-assigned and have collided
/// (measured over the live ledger: `{57: 2, 58: 2}`), so a "which rounds lack a
/// result" query keyed on `round` alone comes back empty while rounds are
/// genuinely owed. These tests pin the composite key that fixes it.
#[cfg(test)]
mod ungraded_round_tests {
    use crate::commands_risk_ungraded::{
        format_ungraded_rounds, ungraded_rounds, UngradedRound, UngradedScan,
    };

    fn exp(round: &str, day: i64, target: &str) -> String {
        format!(r#"{{"type":"experiment","round":{round},"day":{day},"target":"{target}"}}"#)
    }
    fn res(round: &str, day: i64, target: &str) -> String {
        format!(
            r#"{{"type":"experiment_result","round":{round},"day":{day},"target":"{target}","graded":"h1 HIT"}}"#
        )
    }

    /// **The regression guard tied to the defect.** Round 57 appears twice —
    /// day 169 on `src/docs.rs` (graded) and day 171 on `src/commands_plan.rs`
    /// (not graded). Keyed on `round` alone the graded twin answers for its
    /// namesake and this comes back empty; keyed on `(round, target)` exactly
    /// the ungraded one is reported, and it is the *right* one.
    #[test]
    fn collided_round_id_reports_only_the_ungraded_twin() {
        let ledger = [
            exp("57", 169, "src/docs.rs"),
            res("57", 169, "src/docs.rs"),
            exp("57", 171, "src/commands_plan.rs"),
        ]
        .join("\n");
        let scan = ungraded_rounds(&ledger);
        assert_eq!(
            scan.ungraded,
            vec![UngradedRound {
                round: Some(57),
                day: Some(171),
                target: "src/commands_plan.rs".to_string(),
            }],
            "the graded twin must not answer for its ungraded namesake"
        );
        assert_eq!(scan.unkeyed_excluded, 0);
    }

    /// A grade written the *next morning* is still a grade. This is why the key
    /// is `(round, target)` and not `(round, day)`: live round 43 started on day
    /// 165 and was graded on day 166, and `(round, day)` calls that ungraded.
    #[test]
    fn grade_written_on_a_later_day_still_counts() {
        let ledger = [
            exp("43", 165, "src/commands_lint.rs"),
            res("43", 166, "src/commands_lint.rs"),
        ]
        .join("\n");
        assert!(ungraded_rounds(&ledger).ungraded.is_empty());
    }

    #[test]
    fn fully_graded_ledger_is_empty() {
        let ledger = [
            exp("1", 100, "src/a.rs"),
            res("1", 100, "src/a.rs"),
            exp("2", 101, "src/b.rs"),
            res("2", 101, "src/b.rs"),
        ]
        .join("\n");
        assert_eq!(ungraded_rounds(&ledger), UngradedScan::default());
    }

    /// A null `round` is a real key component, not a parse failure — the seven
    /// pre-`round`-field rounds of days 150–153 pair with their results this
    /// way instead of being reported as seven false alarms.
    #[test]
    fn null_round_pairs_by_target_and_is_not_a_false_alarm() {
        let ledger = [
            exp("null", 150, "src/commands_fork.rs"),
            res("null", 150, "src/commands_fork.rs"),
            exp("null", 152, "src/banner.rs"),
        ]
        .join("\n");
        let scan = ungraded_rounds(&ledger);
        assert_eq!(
            scan.ungraded,
            vec![UngradedRound {
                round: None,
                day: Some(152),
                target: "src/banner.rs".to_string(),
            }]
        );
    }

    /// A line with no usable target cannot be keyed at all, so it is excluded
    /// and *counted* — it never vanishes (Day 144).
    #[test]
    fn targetless_experiment_is_excluded_and_counted() {
        let ledger = [
            r#"{"type":"experiment","round":9,"day":170}"#.to_string(),
            r#"{"type":"experiment","round":10,"day":170,"target":"   "}"#.to_string(),
            exp("11", 170, "src/kept.rs"),
        ]
        .join("\n");
        let scan = ungraded_rounds(&ledger);
        assert_eq!(scan.unkeyed_excluded, 2);
        assert_eq!(scan.ungraded.len(), 1);
        assert_eq!(scan.ungraded[0].target, "src/kept.rs");
    }

    #[test]
    fn malformed_and_empty_lines_contribute_nothing() {
        let ledger = format!(
            "not json\n\n   \n{{\"type\":\"experiment\",\"round\":1,\n{}\n{}",
            exp("5", 160, "src/x.rs"),
            res("5", 160, "src/x.rs")
        );
        assert!(ungraded_rounds(&ledger).ungraded.is_empty());
        assert!(ungraded_rounds("").ungraded.is_empty());
    }

    // --- emission point: the string a reader actually receives ---------------

    #[test]
    fn rendered_line_names_the_rounds_and_their_targets() {
        let scan = ungraded_rounds(
            &[
                exp("57", 169, "src/docs.rs"),
                res("57", 169, "src/docs.rs"),
                exp("57", 171, "src/commands_plan.rs"),
                exp("58", 172, "src/config_paths.rs"),
            ]
            .join("\n"),
        );
        let line = format_ungraded_rounds(&scan).expect("two rounds are owed");
        assert!(
            line.contains("2 round(s) started but never graded"),
            "{line}"
        );
        assert!(
            line.contains("57 (day 171, src/commands_plan.rs)"),
            "{line}"
        );
        assert!(line.contains("58 (day 172, src/config_paths.rs)"), "{line}");
        // The graded twin's target must not be named as owed.
        assert!(!line.contains("src/docs.rs"), "{line}");
        // Consumer guard: invisible to `scripts/extract_trajectory.py`.
        assert!(!line.contains("never forecast"), "{line}");
    }

    /// A clean ledger prints nothing at all, so the common path stays
    /// byte-identical to before this detector existed.
    #[test]
    fn clean_ledger_renders_nothing() {
        let scan =
            ungraded_rounds(&[exp("1", 100, "src/a.rs"), res("1", 100, "src/a.rs")].join("\n"));
        assert!(format_ungraded_rounds(&scan).is_none());
    }

    // --- round 68: unreadable lines are counted, and they speak --------------

    /// A line that does not parse at all is *counted*, not dropped in silence.
    /// The ledger is hand-authored, so a truncated or badly escaped line is a
    /// live possibility, and a shrinking denominator inside my own meter is the
    /// defect this module exists to catch.
    #[test]
    fn unparseable_lines_are_counted_not_silently_dropped() {
        let ledger = [
            "{\"type\":\"experiment\",\"round\":1,",
            "not json at all",
            &exp("5", 160, "src/x.rs"),
        ]
        .join("\n");
        let scan = ungraded_rounds(&ledger);
        assert_eq!(scan.unparseable_excluded, 2, "{scan:?}");
        // The readable round is still found — counting must not cost detection.
        assert_eq!(scan.ungraded.len(), 1, "{scan:?}");
        assert_eq!(scan.ungraded[0].target, "src/x.rs");
    }

    /// One unreadable line is one real event. The scan parses the ledger once
    /// rather than once per line-kind, so a bad line cannot emit two counted
    /// records — the "one act, a quorum of itself" defect, here inside my own
    /// meter.
    #[test]
    fn one_unreadable_line_is_counted_exactly_once() {
        let scan = ungraded_rounds("{\"type\":\"experiment\",\"round\":1,");
        assert_eq!(scan.unparseable_excluded, 1, "{scan:?}");
    }

    /// Emission point, and the sharp case: with no owed rounds but a line that
    /// could not be read, the reader must not be told the ledger is clean —
    /// the unreadable line may be the `experiment` whose absence empties the
    /// scan.
    #[test]
    fn unread_ledger_with_no_owed_rounds_still_speaks() {
        let ledger = [
            exp("1", 100, "src/a.rs"),
            res("1", 100, "src/a.rs"),
            "{\"type\":\"experiment\",\"round\":9,".to_string(),
        ]
        .join("\n");
        let scan = ungraded_rounds(&ledger);
        assert!(scan.ungraded.is_empty(), "{scan:?}");
        let line = format_ungraded_rounds(&scan).expect("an unread ledger must not stay silent");
        assert!(line.contains("was not fully read"), "{line}");
        assert!(
            line.contains("1 line(s) could not be parsed and were not checked"),
            "{line}"
        );
        // It must not claim rounds are owed when none were found.
        assert!(
            !line.contains("round(s) started but never graded"),
            "{line}"
        );
        // Consumer guard: still invisible to `scripts/extract_trajectory.py`.
        assert!(!line.contains("never forecast"), "{line}");
    }

    /// Near-miss guard for the branch above: a ledger that is clean *and* fully
    /// read stays silent. A discriminator tested only on the side that fires is
    /// vacuous green.
    #[test]
    fn clean_and_fully_read_ledger_stays_silent() {
        let scan =
            ungraded_rounds(&[exp("1", 100, "src/a.rs"), res("1", 100, "src/a.rs")].join("\n"));
        assert_eq!(scan.unparseable_excluded, 0, "{scan:?}");
        assert!(format_ungraded_rounds(&scan).is_none());
    }

    /// Emission point: the two exclusion reasons are reported as *distinct*
    /// facts, never summed into one number — a line with no target and a line
    /// that could not be read are different failures with different remedies.
    #[test]
    fn both_exclusion_reasons_are_named_separately() {
        let ledger = [
            "{\"type\":\"experiment\",\"round\":3,\"day\":170}".to_string(),
            "{\"type\":\"experiment\",\"round\":4,".to_string(),
            exp("11", 170, "src/owed.rs"),
        ]
        .join("\n");
        let scan = ungraded_rounds(&ledger);
        assert_eq!(scan.unkeyed_excluded, 1, "{scan:?}");
        assert_eq!(scan.unparseable_excluded, 1, "{scan:?}");
        let line = format_ungraded_rounds(&scan).expect("one round is owed");
        assert!(
            line.contains("1 line(s) had no target and could not be checked"),
            "{line}"
        );
        assert!(
            line.contains("1 line(s) could not be parsed and were not checked"),
            "{line}"
        );
        assert!(line.contains("11 (day 170, src/owed.rs)"), "{line}");
    }
}
