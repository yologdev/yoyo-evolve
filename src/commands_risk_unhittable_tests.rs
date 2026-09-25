//! Render- and census-level tests for [`crate::commands_risk_unhittable`]'s
//! **retrospective** half, in their own file for the module-size gate.
//!
//! Extracted on Day 209 rather than registered as an oversized module: the
//! retrospective pass plus its table tests took `commands_risk_unhittable.rs`
//! to 2175 lines, 175 past `MAX_MODULE_LINES`, and the gate's own preferred
//! remedy is a split. Only this test module moved — the production code stayed
//! where the task put it — and the sibling-file shape is the one this repo
//! already uses for exactly this reason (`commands_risk_epistemic_tests.rs`).

/// Tests for the retrospective pass — the dream's second observable: how many
/// **post-ledger grading events** carried an unhittable member, over the whole
/// history rather than the one row in front of me.
#[cfg(test)]
mod retrospective_tests {
    use crate::commands_risk_snapshots::founding_ts;
    use crate::commands_risk_unhittable::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn scored(rows: &[(&str, &str)]) -> BTreeMap<String, String> {
        rows.iter()
            .map(|(p, t)| ((*p).to_string(), (*t).to_string()))
            .collect()
    }

    /// The member identity behind a `with_unhittable` count, spelled the way the
    /// instruments spell it (the row's own `ts`).
    fn ts_set(rows: &[&str]) -> BTreeSet<String> {
        rows.iter().map(|s| (*s).to_string()).collect()
    }

    fn row(ts: &str, surprises: &[&str]) -> SurpriseRow {
        SurpriseRow {
            ts: ts.to_string(),
            surprises: surprises.iter().map(|s| (*s).to_string()).collect(),
            // No snapshot hash: the shape every event written before Day 165
            // has, and the regression surface of the git pass.
            git_hash: String::new(),
        }
    }

    /// The same row, carrying the snapshot hash the git instrument reads.
    fn row_h(ts: &str, git_hash: &str, surprises: &[&str]) -> SurpriseRow {
        SurpriseRow {
            git_hash: git_hash.to_string(),
            ..row(ts, surprises)
        }
    }

    fn paths(rows: &[&str]) -> Vec<String> {
        rows.iter().map(|s| (*s).to_string()).collect()
    }

    /// The ledger most rows below join against: one founding-batch path, one
    /// long-scored path, one born two days before the graded event below.
    fn ledger() -> BTreeMap<String, String> {
        scored(&[
            ("src/seed.rs", "2026-08-22T15:40:00Z"),
            ("src/old.rs", "2026-08-25T10:00:00Z"),
            ("src/born.rs", "2026-09-03T18:03:00Z"),
        ])
    }

    #[test]
    fn a_post_ledger_row_with_a_later_birthday_fires_and_its_twin_does_not() {
        let map = ledger();
        let founding = founding_ts(&map);
        let fired_ts = "2026-09-03T17:23:00Z";
        // Anti-vacuous, and it has to be here rather than assumed: the fixture
        // only means anything if the path really is in the ledger AND really is
        // first scored *after* the event that missed it. A transcription slip in
        // either direction would otherwise let this test pass by agreeing with
        // itself.
        let born = map
            .get("src/born.rs")
            .expect("fixture really contains the born path");
        assert!(
            born.as_str() > fired_ts,
            "anti-vacuous: the fixture's birthday must postdate the event, got {born}"
        );

        let rows = [
            row(fired_ts, &["src/born.rs", "src/old.rs"]),
            // The near-miss: same ledger, a member first scored BEFORE the
            // event that missed it — a real miss, and it must not fire.
            row("2026-08-26T00:00:00Z", &["src/old.rs"]),
        ];
        let got = retrospective_unhittable(&rows, &map, founding);
        assert_eq!(
            got.population, 2,
            "both rows are after the founding instant"
        );
        assert_eq!(
            got.with_unhittable, 1,
            "only the row whose member was born after it"
        );
        assert_eq!(got.with_unmeasurable, 0);
        assert_eq!(got.undated, 0);

        let note = retrospective_note(&RetrospectiveReading::Counted(got), true).expect("note");
        assert!(
            note.contains("unhittable: 1 of 2 post-ledger grading events carried a file first scored after the event"),
            "{note}"
        );
    }

    #[test]
    fn a_row_at_or_before_the_founding_instant_is_out_of_the_population() {
        // The survivor trap, in one row: a first-scored date from before the
        // ledger's own founding is backfill, so a row graded then cannot be
        // graded this way. The note must then be state (b), not a zero.
        let map = ledger();
        let founding = founding_ts(&map);
        let rows = [
            row("2026-08-22T15:40:00Z", &["src/born.rs"]), // exactly the founding instant
            row("2026-08-01T00:00:00Z", &["src/born.rs"]), // before it
        ];
        let got = retrospective_unhittable(&rows, &map, founding);
        assert_eq!(got.population, 0, "neither row is gradeable by this join");
        assert_eq!(got.with_unhittable, 0);
        assert_eq!(
            got.undated, 0,
            "both rows are dated; they are just too early"
        );

        let note = retrospective_note(&RetrospectiveReading::Counted(got), true).expect("note");
        assert!(note.contains("no post-ledger grading events yet"), "{note}");
        assert!(
            note.contains("this is not the same as zero unhittable"),
            "{note}"
        );
    }

    #[test]
    fn a_surprise_with_no_ledger_record_is_counted_out_loud() {
        let map = ledger();
        let founding = founding_ts(&map);
        assert!(
            !map.contains_key("src/never-scored.rs"),
            "anti-vacuous: the path really has no birthday in the ledger"
        );
        let rows = [row("2026-09-03T17:23:00Z", &["src/never-scored.rs"])];
        let got = retrospective_unhittable(&rows, &map, founding);
        assert_eq!(got.population, 1);
        assert_eq!(got.with_unhittable, 0, "unmeasurable is never unhittable");
        assert_eq!(got.with_unmeasurable, 1);

        let note = retrospective_note(&RetrospectiveReading::Counted(got), true).expect("note");
        assert!(note.contains("unhittable: 0 of 1"), "{note}");
        assert!(note.contains("at least one undecidable surprise"), "{note}");
    }

    #[test]
    fn a_ts_outside_the_ledger_shape_has_no_placeable_moment() {
        // The join's string compare is chronological *only* for the one
        // fixed-width shape, so a row that is not in it must be unorderable
        // rather than silently participating in a compare it cannot support.
        assert!(is_placeable_ledger_ts("2026-09-03T17:23:00Z"));
        for bad in [
            "",
            "unknown",
            "; not a date",
            "2026-9-03T17:23:00Z",  // 19 bytes: the shape the compare breaks on
            "2026-09-03T17:23:00",  // no `Z`
            "2026-09-03 17:23:00Z", // space where the `T` belongs
            "2026-09-03T17:23:00Z\n", // trailing byte
            "2026-09-03T17:23:00\u{2713}", // non-ASCII
        ] {
            assert!(!is_placeable_ledger_ts(bad), "must not place {bad:?}");
        }
    }

    #[test]
    fn a_founding_batch_surprise_is_unmeasurable_not_unhittable() {
        // Inherited from `first_scored_age` rather than re-derived: a path
        // stamped at the founding instant has an *unknown* birthday, and
        // relabelling the whole backlog as unhittable would be the survivor trap
        // read in the flattering direction.
        let map = ledger();
        let founding = founding_ts(&map);
        let rows = [row("2026-09-03T17:23:00Z", &["src/seed.rs"])];
        let got = retrospective_unhittable(&rows, &map, founding);
        assert_eq!(got.with_unhittable, 0);
        assert_eq!(got.with_unmeasurable, 1);
    }

    #[test]
    fn an_undated_row_leaves_the_population_and_says_so() {
        // No placeable moment ⇒ nothing to compare a birthday against. It must
        // not be absorbed into the population as a hit, and it must not be
        // silently dropped either.
        let map = ledger();
        let founding = founding_ts(&map);
        let rows = [
            row("", &["src/born.rs"]),
            row("; not a date", &["src/born.rs"]),
            row("2026-09-03T17:23:00Z", &["src/born.rs"]),
        ];
        let got = retrospective_unhittable(&rows, &map, founding);
        assert_eq!(got.population, 1, "only the dated row can be graded");
        assert_eq!(got.undated, 2);
        let note = retrospective_note(&RetrospectiveReading::Counted(got), true).expect("note");
        assert!(note.contains("2 undated row(s)"), "{note}");
    }

    #[test]
    fn no_ledger_prints_nothing_at_all() {
        // The whole regression surface for a project that has never recorded a
        // risk grade: `None`, in both output modes, so `yoyo risk` stays
        // byte-identical.
        assert_eq!(
            retrospective_note(&RetrospectiveReading::NoLedger, true),
            None
        );
        assert_eq!(
            retrospective_note(&RetrospectiveReading::NoLedger, false),
            None
        );
    }

    #[test]
    fn an_unreadable_ledger_is_unknown_not_zero() {
        let reading = RetrospectiveReading::Unreadable(
            "could not read .yoyo/risk_validations.jsonl: Is a directory (os error 21)".to_string(),
        );
        let note = retrospective_note(&reading, true).expect("an unreadable ledger still speaks");
        assert!(
            note.contains("unknown, not zero"),
            "an unreadable ledger must never render as a zero: {note}"
        );
        assert!(note.contains("Is a directory"), "{note}");
        assert!(!note.contains("0 of"), "{note}");
    }

    #[test]
    fn the_retrospective_note_is_glyph_free_and_mark_rich_in_the_two_modes() {
        let reading = RetrospectiveReading::Counted(RetrospectiveCount {
            population: 116,
            with_unhittable: 3,
            // Present for consistency with the count, never read: with
            // `git: None` the intersection clause has no second instrument and
            // stays unreachable.
            unhittable_rows: ts_set(&[
                "2026-09-03T17:23:00Z",
                "2026-09-04T17:23:00Z",
                "2026-09-05T17:23:00Z",
            ]),
            with_unmeasurable: 86,
            undated: 0,
            // The ledger-only shape: no git reading at all, which is what this
            // fixture has always been about.
            git: None,
        });
        let rich = retrospective_note(&reading, false).expect("note");
        assert!(rich.starts_with("📊"), "{rich}");
        let plain = retrospective_note(&reading, true).expect("note");
        assert!(
            plain.is_ascii(),
            "plain mode must be glyph-free (no em dash either), got: {plain}"
        );
        assert!(!plain.as_bytes().contains(&0x1b), "no ANSI in the note");
        // Not decorative: the two strings really differ, so the plain branch is
        // exercised rather than agreeing with itself.
        assert_ne!(plain, rich);
        assert!(
            plain.contains("3 of 116 post-ledger grading events"),
            "a count with its denominator, never a percentage: {plain}"
        );
        assert!(
            plain.contains("86 carried at least one undecidable surprise"),
            "{plain}"
        );
    }

    #[test]
    fn parse_surprise_rows_reads_ts_and_surprises_and_skips_non_events() {
        let content = concat!(
            "\n",
            "{\"ts\":\"2026-09-03T17:23:00Z\",\"day\":165,\"hits\":[],",
            "\"surprises\":[\"src/a.rs\",\"src/b.rs\"],\"accuracy_pct\":0.0}\n",
            "{\"not\":\"an event\"}\n", // gradable = false → not a row
            "{ not json at all\n",      // malformed → not a row
            "{\"ts\":\"2026-09-04T00:00:00Z\",\"surprises\":[]}\n",
        );
        let rows = parse_surprise_rows(content);
        assert_eq!(rows.len(), 2, "only gradable lines are rows");
        assert_eq!(rows[0].ts, "2026-09-03T17:23:00Z");
        assert_eq!(rows[0].surprises, paths(&["src/a.rs", "src/b.rs"]));
        assert_eq!(rows[1].ts, "2026-09-04T00:00:00Z");
        assert!(rows[1].surprises.is_empty());
    }

    #[test]
    fn the_io_wrapper_reads_both_ledgers_and_keeps_the_three_states_apart() {
        let dir = tempfile::tempdir().expect("temp dir");
        let val = dir.path().join("risk_validations.jsonl");
        let led = dir.path().join("risk_first_scored.jsonl");

        // (1) Nothing on disk: no ledger.
        assert_eq!(
            retrospective_unhittable_at(&val, &led),
            RetrospectiveReading::NoLedger
        );

        std::fs::write(
            &led,
            concat!(
                "{\"path\":\"src/seed.rs\",\"ts\":\"2026-08-22T15:40:00Z\"}\n",
                "{\"path\":\"src/born.rs\",\"ts\":\"2026-09-03T18:03:00Z\"}\n",
            ),
        )
        .expect("seed ledger");
        std::fs::write(
            &val,
            concat!(
                "{\"ts\":\"2026-09-03T17:23:00Z\",\"hits\":[],",
                "\"surprises\":[\"src/born.rs\"],\"accuracy_pct\":0.0}\n",
                "{\"ts\":\"2026-08-22T15:40:00Z\",\"hits\":[],",
                "\"surprises\":[\"src/born.rs\"],\"accuracy_pct\":0.0}\n",
            ),
        )
        .expect("seed validations");

        // (2) Present: the same numbers the pure pass gives, end to end.
        assert_eq!(
            retrospective_unhittable_at(&val, &led),
            RetrospectiveReading::Counted(RetrospectiveCount {
                population: 1,
                with_unhittable: 1,
                // The row the join actually named, spelled the way it spells it.
                unhittable_rows: ts_set(&["2026-09-03T17:23:00Z"]),
                with_unmeasurable: 0,
                undated: 0,
                // Neither fixture event carries a snapshot hash — the shape of
                // every ledger written before Day 165, pinned here as still
                // reading as `None` rather than as an empty census.
                git: None,
            })
        );

        // (3) A path that exists but cannot be read is `Unreadable`, never a
        // missing ledger and never a zero. A directory is the portable way to
        // build that on every platform this runs on.
        let unreadable = dir.path().join("a-directory");
        std::fs::create_dir(&unreadable).expect("create dir");
        match retrospective_unhittable_at(&unreadable, &led) {
            RetrospectiveReading::Unreadable(msg) => {
                assert!(msg.contains("a-directory"), "{msg}")
            }
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }

    /// Probes for the git leg, built as data — the whole reason that pass takes
    /// its readings as a parameter rather than shelling out.
    fn probes(resolved: &[(&str, bool)], present: &[(&str, bool)]) -> GitProbes {
        GitProbes {
            resolves: resolved
                .iter()
                .map(|(h, r)| ((*h).to_string(), *r))
                .collect(),
            present: present
                .iter()
                .map(|(k, v)| ((*k).to_string(), *v))
                .collect(),
        }
    }

    /// **The anti-vacuous row.** The git census really does fire on this
    /// fixture — a reader that always returned `0` cannot pass by agreeing with
    /// itself — and its twin, one second of birthday change, does not.
    #[test]
    fn the_git_census_fires_on_a_row_whose_file_did_not_exist_at_the_snapshot() {
        let rows = [
            row_h(
                "2026-09-03T17:23:00Z",
                "aaaaaaa1",
                &["src/born.rs", "src/old.rs"],
            ),
            // The near-miss, and it has to be a *different tree*: the same
            // members under a snapshot taken after the file was added. (A twin
            // sharing the first row's hash would hit the identical probe and
            // prove nothing — that mistake was made and caught by this test.)
            row_h(
                "2026-09-04T17:23:00Z",
                "aaaaaaa2",
                &["src/born.rs", "src/old.rs"],
            ),
        ];
        let p = probes(
            &[("aaaaaaa1", true), ("aaaaaaa2", true)],
            &[
                // Absent at the first row's tree, present at the second — which
                // is exactly the day-205/206 shape, a file that did not exist
                // when the snapshot was taken.
                ("aaaaaaa1:src/born.rs", false),
                ("aaaaaaa1:src/old.rs", true),
                ("aaaaaaa2:src/born.rs", true),
                ("aaaaaaa2:src/old.rs", true),
            ],
        );
        let got = retrospective_git_census(&rows, Some("2026-08-22T15:40:02Z"), &p);
        assert_eq!(got.population, 2, "{got:?}");
        assert_eq!(
            got.with_unhittable, 1,
            "only the row whose tree lacked a surprise member: {got:?}"
        );
        assert_eq!(got.with_unmeasurable, 0, "{got:?}");
        assert_eq!(got.unresolvable, 0, "{got:?}");
        assert_eq!(got.skipped_surprises, 0, "{got:?}");

        // And the count reads the MEMBERS, not the row count: make the second
        // member absent at the first tree too and the same one row still fires,
        // while clearing both trees' absences drives it to zero.
        let p_first_absent = probes(
            &[("aaaaaaa1", true), ("aaaaaaa2", true)],
            &[
                ("aaaaaaa1:src/born.rs", false),
                ("aaaaaaa1:src/old.rs", false),
                ("aaaaaaa2:src/born.rs", true),
                ("aaaaaaa2:src/old.rs", true),
            ],
        );
        let both = retrospective_git_census(&rows, Some("2026-08-22T15:40:02Z"), &p_first_absent);
        assert_eq!(
            both.with_unhittable, 1,
            "one row, two absent members — still one row: {both:?}"
        );

        let p_none = probes(
            &[("aaaaaaa1", true), ("aaaaaaa2", true)],
            &[
                ("aaaaaaa1:src/born.rs", true),
                ("aaaaaaa1:src/old.rs", true),
                ("aaaaaaa2:src/born.rs", true),
                ("aaaaaaa2:src/old.rs", true),
            ],
        );
        let none = retrospective_git_census(&rows, Some("2026-08-22T15:40:02Z"), &p_none);
        assert_eq!(
            none.with_unhittable, 0,
            "and it really does depend on the probes: {none:?}"
        );
    }

    /// **The dangerous direction, pinned.** A snapshot hash that does not
    /// resolve must be counted and named, never read as "nothing was born
    /// late" — that is the day-205 shape, and the reason the dream itself wrote
    /// a fallback clause.
    #[test]
    fn an_unresolvable_hash_is_unresolvable_and_never_unhittable() {
        let rows = [row_h(
            "2026-09-03T17:23:00Z",
            "0ec456e0",
            &["src/format/cost.rs", "src/format/cost/price_audit_tests.rs"],
        )];
        // The hash does not resolve, and no member probe exists for it: the
        // run had nothing to ask.
        let p = probes(&[("0ec456e0", false)], &[]);
        let got = retrospective_git_census(&rows, Some("2026-08-22T15:40:02Z"), &p);
        assert_eq!(got.population, 1, "{got:?}");
        assert_eq!(
            got.with_unhittable, 0,
            "a check that did not run is not a finding: {got:?}"
        );
        assert_eq!(got.unresolvable, 1, "{got:?}");
        assert_eq!(got.with_unmeasurable, 1, "{got:?}");
        assert_eq!(
            got.skipped_surprises, 2,
            "the unprobed members are counted, not dropped: {got:?}"
        );

        // The near-miss on the same fixture: the identical members under a hash
        // that DOES resolve read as a real unhittable, so the flag is
        // load-bearing rather than decorative.
        let resolving = probes(
            &[("0ec456e0", true)],
            &[
                ("0ec456e0:src/format/cost.rs", true),
                ("0ec456e0:src/format/cost/price_audit_tests.rs", false),
            ],
        );
        let got2 = retrospective_git_census(&rows, Some("2026-08-22T15:40:02Z"), &resolving);
        assert_eq!(got2.with_unhittable, 1, "{got2:?}");
        assert_eq!(got2.unresolvable, 0, "{got2:?}");
        assert_ne!(got, got2, "the flag changes the answer for one member set");
    }

    /// **A row with no snapshot hash is out of this population entirely** — not
    /// an error, not an unmeasurable: there is nothing to ask. This is the
    /// whole regression surface for a pre-Day-165 ledger.
    #[test]
    fn a_row_without_a_snapshot_hash_is_out_of_the_git_population() {
        let rows = [
            row("2026-09-03T17:23:00Z", &["src/absent.rs"]),
            row_h("2026-09-04T17:23:00Z", "aaaaaaa2", &["src/absent.rs"]),
        ];
        let p = probes(&[("aaaaaaa2", true)], &[("aaaaaaa2:src/absent.rs", true)]);
        let got = retrospective_git_census(&rows, Some("2026-08-22T15:40:02Z"), &p);
        assert_eq!(
            got.population, 1,
            "the hashless row is not counted, and not as unmeasurable: {got:?}"
        );
        assert_eq!(got.with_unmeasurable, 0, "{got:?}");
    }

    /// **The disagreement that is the interesting finding.** The day-178 row's
    /// ledger birthday is exactly its own event `ts`, so the join calls it a
    /// tie — not born-after. Git says the other half of the story: the path did
    /// not exist at that snapshot's tree at all. Two independent readings of
    /// one row, and the note prints both rather than picking one.
    #[test]
    fn the_two_instruments_can_disagree_and_both_readings_are_printed() {
        let ts = "2026-08-25T22:40:17Z";
        let map = scored(&[
            ("src/seed.rs", "2026-08-22T15:40:02Z"),
            // The real day-178 row: the ledger holds this path at exactly the
            // event's own ts, i.e. a tie.
            ("src/gasp_cli.rs", ts),
        ]);
        let founding = founding_ts(&map);
        let rows = [row_h(ts, "13a3c6f8", &["src/gasp_cli.rs"])];

        let ledger = retrospective_unhittable(&rows, &map, founding);
        assert_eq!(ledger.population, 1, "{ledger:?}");
        assert_eq!(
            ledger.with_unhittable, 0,
            "the ledger join reads a tie, not a born-after: {ledger:?}"
        );

        // git: the file did not exist at that tree.
        let p = probes(
            &[("13a3c6f8", true)],
            &[("13a3c6f8:src/gasp_cli.rs", false)],
        );
        let git = retrospective_git_census(&rows, founding, &p);
        assert_eq!(
            git.with_unhittable, 1,
            "the second instrument sees the missing file: {git:?}"
        );

        // Both numbers reach the note, named as which is which.
        let mut count = ledger;
        count.git = Some(git);
        let note = retrospective_note(&RetrospectiveReading::Counted(count), true).expect("note");
        assert!(
            note.contains("0 of 1 post-ledger grading events"),
            "the ledger join's own reading is unchanged: {note}"
        );
        assert!(
            note.contains("the git check reads 1 of 1 rows with a snapshot hash"),
            "the disagreement is printed, not resolved by picking one: {note}"
        );
        assert!(note.is_ascii(), "plain mode stays glyph-free: {note}");
    }

    /// **The `None` regression surface, at the emission point.** With no git
    /// reading the note is byte-identical to the ledger-only note this pass has
    /// always printed — which is every user whose events carry no snapshot hash
    /// and every project with no ledger at all.
    #[test]
    fn the_note_is_byte_identical_when_the_git_leg_was_not_consulted() {
        let count = RetrospectiveCount {
            population: 3,
            with_unhittable: 1,
            // The join names a row — and the clause is still absent, because
            // there is no second instrument to intersect with. Full-string
            // equality against the pre-change sentence, never a `contains`:
            // this is the regression surface for every ledger with no hash.
            unhittable_rows: ts_set(&["2026-09-03T17:23:00Z"]),
            with_unmeasurable: 0,
            undated: 0,
            git: None,
        };
        let note = retrospective_note(&RetrospectiveReading::Counted(count), true).expect("note");
        assert_eq!(
            note,
            "unhittable: 1 of 3 post-ledger grading events carried a file first scored \
             after the event"
        );

        // And an unresolvable hash does speak, with the skipped count in band.
        let with_skip = RetrospectiveCount {
            population: 3,
            with_unhittable: 1,
            unhittable_rows: ts_set(&["2026-09-03T17:23:00Z"]),
            with_unmeasurable: 1,
            undated: 0,
            git: Some(GitCensus {
                population: 3,
                with_unhittable: 1,
                unhittable_rows: ts_set(&["2026-09-04T17:23:00Z"]),
                with_unmeasurable: 1,
                unresolvable: 1,
                skipped_surprises: 2,
            }),
        };
        let note =
            retrospective_note(&RetrospectiveReading::Counted(with_skip), true).expect("note");
        assert!(
            note.contains("1 row(s) skip: their snapshot hash does not resolve in this clone"),
            "{note}"
        );
        assert!(note.contains("2 surprise(s) unprobed"), "{note}");
        assert!(note.is_ascii(), "{note}");
        assert!(!note.as_bytes().contains(&0x1b), "no ANSI in the note");
    }

    /// **The day-209 shape, at the emission point.** Two instruments reading the
    /// SAME total over member sets that differ. The day-178 row is the
    /// ledger-only member (a timestamp tie: the join cannot call it born-after,
    /// the tree says the file was not there), one row is named by both, and one
    /// is git-only. A matching total is not evidence of agreement, so the clause
    /// states the intersection rather than deriving one from the sum.
    #[test]
    fn the_intersection_clause_names_the_members_the_totals_cannot() {
        let count = RetrospectiveCount {
            population: 4,
            with_unhittable: 2,
            unhittable_rows: ts_set(&["2026-08-25T22:40:17Z", "2026-09-19T07:34:00Z"]),
            with_unmeasurable: 0,
            undated: 0,
            git: Some(GitCensus {
                population: 4,
                with_unhittable: 2,
                // Same total, different members: day 203 shared, plus one the
                // ledger join reads as a tie.
                unhittable_rows: ts_set(&["2026-09-19T07:34:00Z", "2026-09-03T10:00:00Z"]),
                with_unmeasurable: 0,
                unresolvable: 0,
                skipped_surprises: 0,
            }),
        };
        let note =
            retrospective_note(&RetrospectiveReading::Counted(count.clone()), true).expect("note");
        // The totals really do match — that is the trap this clause closes, and
        // asserting it here keeps the fixture from passing vacuously.
        assert!(
            note.contains("2 of 4 post-ledger grading events")
                && note.contains("the git check reads 2 of 4 rows"),
            "the fixture must be the matching-totals case: {note}"
        );
        assert!(
            note.contains(
                "; the two instruments name 1 row(s) in common, 1 only the ledger and 1 only git"
            ),
            "the members are the finding, and the total cannot carry them: {note}"
        );
        assert!(
            !note.contains("both instruments name the same"),
            "a matching total must never be rendered as agreement: {note}"
        );
        assert!(note.is_ascii(), "plain mode stays glyph-free: {note}");
        assert!(!note.as_bytes().contains(&0x1b), "no ANSI in the note");

        // The near-miss: equal member sets are the one case that may read as
        // agreement, so the differing branch is not the only one exercised.
        let mut agreed = count;
        let shared = ts_set(&[
            "2026-08-25T22:40:17Z",
            "2026-09-19T07:34:00Z",
            "2026-09-03T10:00:00Z",
        ]);
        agreed.with_unhittable = 3;
        agreed.unhittable_rows = shared.clone();
        if let Some(git) = agreed.git.as_mut() {
            git.with_unhittable = 3;
            git.unhittable_rows = shared;
        }
        let note = retrospective_note(&RetrospectiveReading::Counted(agreed), true).expect("note");
        assert!(
            note.contains("; both instruments name the same 3 row(s)"),
            "equal member sets are the one case that may read as agreement: {note}"
        );
        assert!(
            !note.contains("only git"),
            "no leftover branch text when the sets are equal: {note}"
        );
    }

    /// **Neither instrument names a row.** The clause is absent — byte-for-byte
    /// the pre-change sentence — which is the shape of a ledger where nothing
    /// was unhittable at all, and the case a reader could mistake for silence
    /// about a disagreement.
    #[test]
    fn the_clause_is_absent_when_neither_instrument_names_a_row() {
        let count = RetrospectiveCount {
            population: 3,
            with_unhittable: 0,
            unhittable_rows: ts_set(&[]),
            with_unmeasurable: 0,
            undated: 0,
            git: Some(GitCensus {
                population: 3,
                with_unhittable: 0,
                unhittable_rows: ts_set(&[]),
                with_unmeasurable: 0,
                unresolvable: 0,
                skipped_surprises: 0,
            }),
        };
        let note = retrospective_note(&RetrospectiveReading::Counted(count), true).expect("note");
        assert_eq!(
            note,
            "unhittable: 0 of 3 post-ledger grading events carried a file first scored \
             after the event; the git check reads 0 of 3 rows with a snapshot hash"
        );
    }
}
