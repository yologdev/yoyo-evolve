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
            // Nothing in this fixture's ledger sits at a row's own second.
            tied_rows: ts_set(&[]),
            // The ledger-only shape: no git reading at all, which is what this
            // fixture has always been about.
            git: None,
            dropped: 0,
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
                // The ledger has this path strictly before the event: a plain
                // birthday, never a tie.
                tied_rows: ts_set(&[]),
                // Neither fixture event carries a snapshot hash — the shape of
                // every ledger written before Day 165, pinned here as still
                // reading as `None` rather than as an empty census.
                git: None,
                dropped: 0,
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
        assert_eq!(
            got.uncheckable_rows,
            ts_set(&["2026-09-03T17:23:00Z"]),
            "the could-not-check row is named, not just counted — it is the cause \
             `retrospective_note` prints for a ledger-only row: {got:?}"
        );
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
        assert!(
            got2.uncheckable_rows.is_empty(),
            "a check that RAN names no could-not-check row — the two states must \
             not bleed into each other: {got2:?}"
        );
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

    /// **The tie cause, measured by the join itself rather than hand-set on a
    /// fixture.** A test that only ever hands [`RetrospectiveCount::tied_rows`]
    /// in as data cannot see the join stop filling it — a positive control
    /// proved exactly that (neutering the `==` arm left every test green). So
    /// this one drives the real ledger join with the day-178 shape: a path whose
    /// first-scored `ts` is *exactly* the row's own, which is neither the strict
    /// `>` above (born-after) nor a miss.
    #[test]
    fn the_join_fills_tied_rows_for_a_same_second_birthday_and_not_for_a_strict_one() {
        let ts = "2026-08-25T22:40:17Z";
        let map = scored(&[
            ("src/seed.rs", "2026-08-22T15:40:02Z"),
            // The real day-178 row: the ledger holds this path at exactly the
            // event's own second.
            ("src/tied.rs", ts),
            // The near-miss, one second EARLIER: a plain birthday that existed
            // when the prediction was made, which is neither bucket.
            ("src/before.rs", "2026-08-25T22:40:16Z"),
        ]);
        let founding = founding_ts(&map);
        // Anti-vacuous, and it has to be here rather than assumed: the fixture
        // only means anything if the birthday really is the event's own second.
        assert_eq!(map.get("src/tied.rs").map(String::as_str), Some(ts));

        let rows = [row(ts, &["src/tied.rs"]), row(ts, &["src/before.rs"])];
        let got = retrospective_unhittable(&rows, &map, founding);
        assert_eq!(
            got.tied_rows,
            ts_set(&[ts]),
            "exactly the row with a same-second birthday, and never the strict-before one: {got:?}"
        );
        assert_eq!(
            got.with_unhittable, 0,
            "a tie is not born-after — the strict `>` cannot credit it: {got:?}"
        );
        // And no git reading is needed to fill it: this is a ledger-side
        // observable, which is the whole reason it can serve as the *cause* for a
        // git-only row.
        assert_eq!(got.git, None, "{got:?}");
    }

    /// **The `None` fallback for the tie cause.** A row with no tie at all must
    /// leave `tied_rows` empty rather than collecting every row, so the weaker
    /// "cause not observable" sentence stays reachable.
    #[test]
    fn the_join_leaves_tied_rows_empty_when_no_birthday_lands_on_the_event_second() {
        let map = ledger();
        let founding = founding_ts(&map);
        let rows = [row("2026-09-03T17:23:00Z", &["src/born.rs"])];
        let got = retrospective_unhittable(&rows, &map, founding);
        assert!(
            got.tied_rows.is_empty(),
            "a born-after row is not a tie: {got:?}"
        );
        assert_eq!(got.with_unhittable, 1, "{got:?}");
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
            tied_rows: ts_set(&[]),
            git: None,
            dropped: 0,
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
            tied_rows: ts_set(&[]),
            git: Some(GitCensus {
                population: 3,
                with_unhittable: 1,
                unhittable_rows: ts_set(&["2026-09-04T17:23:00Z"]),
                with_unmeasurable: 1,
                unresolvable: 1,
                // The row the git check could not evaluate at all: the ledger-only
                // direction's second cause (day 206's real shape).
                uncheckable_rows: ts_set(&["2026-09-03T17:23:00Z"]),
                skipped_surprises: 2,
            }),
            dropped: 0,
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

    /// **The day-209 shape, at the emission point, with the day-210 bracket and
    /// per-row causes.** Two instruments reading the SAME total over member sets
    /// that differ. The fixture is the real measurement at a smaller scale: one
    /// row named by both, one git-only row whose cause the ledger can observe
    /// (day 178's same-second tie), and one ledger-only row the git check could
    /// not evaluate at all (day 206's unresolvable snapshot hash).
    ///
    /// A matching total is not evidence of agreement, so the clause prints the
    /// bracket and then one line per divergent row naming the instrument and the
    /// cause — and the two causes must never render identically, because
    /// *checked and found present* and *could not check* are different readings.
    #[test]
    fn the_intersection_clause_names_the_members_the_totals_cannot() {
        let count = RetrospectiveCount {
            population: 4,
            with_unhittable: 2,
            // Day 203's row (shared with git) and day 206's (the git check
            // cannot evaluate it).
            unhittable_rows: ts_set(&["2026-09-19T07:34:00Z", "2026-09-26T07:34:00Z"]),
            with_unmeasurable: 0,
            undated: 0,
            // Day 178's row: the ledger join reads a same-second tie, so the
            // strict `>` cannot call it born-after.
            tied_rows: ts_set(&["2026-08-25T22:40:17Z"]),
            git: Some(GitCensus {
                population: 4,
                with_unhittable: 2,
                // The SAME total over different members: day 203 shared, plus
                // the tie the ledger join cannot credit.
                unhittable_rows: ts_set(&["2026-09-19T07:34:00Z", "2026-08-25T22:40:17Z"]),
                with_unmeasurable: 1,
                unresolvable: 1,
                uncheckable_rows: ts_set(&["2026-09-26T07:34:00Z"]),
                skipped_surprises: 1,
            }),
            dropped: 0,
        };
        // Anti-vacuous, and it has to be here rather than assumed: the fixture
        // only means anything if the two member sets really do diverge in both
        // directions. A transcription slip that made them equal would let the
        // whole test pass while exercising nothing.
        let git = count.git.as_ref().expect("fixture carries a git reading");
        assert!(
            count
                .unhittable_rows
                .difference(&git.unhittable_rows)
                .count()
                == 1
                && git
                    .unhittable_rows
                    .difference(&count.unhittable_rows)
                    .count()
                    == 1,
            "anti-vacuous: the fixture must differ in both directions"
        );

        let note =
            retrospective_note(&RetrospectiveReading::Counted(count.clone()), true).expect("note");
        // The totals really do match — that is the trap this clause closes, and
        // asserting it here keeps the fixture from passing vacuously.
        assert!(
            note.contains("2 of 4 post-ledger grading events")
                && note.contains("the git check reads 2 of 4 rows"),
            "the fixture must be the matching-totals case: {note}"
        );
        // The bracket: (intersection, union), never one number — and the two
        // numbers are different, so a disagreement cannot render as agreement.
        assert!(
            note.contains(
                "; the two instruments name 1..3 row(s) (in common .. union; ledger 2, git 2)"
            ),
            "the disagreement is a bracket, with each instrument's own count: {note}"
        );
        assert!(
            !note.contains("both instruments name the same"),
            "a matching total must never be rendered as agreement: {note}"
        );
        // One line per divergent row, each naming its own instrument and cause —
        // and the two causes are different sentences.
        assert!(
            note.contains(
                "\n  row 2026-08-25T22:40:17Z: only the git check - the ledger join \
                 reads a same-second timestamp tie here"
            ),
            "the observable cause is named rather than guessed: {note}"
        );
        assert!(
            note.contains(
                "\n  row 2026-09-26T07:34:00Z: only the ledger - the git check \
                 COULD NOT CHECK it (its snapshot hash does not resolve in this clone)"
            ),
            "a check that could not run must say so, in those words: {note}"
        );
        assert!(
            !note.contains(
                "2026-09-26T07:34:00Z: only the ledger - the git check evaluated this snapshot"
            ),
            "could-not-check must never read as checked-and-absent: {note}"
        );
        assert!(note.is_ascii(), "plain mode stays glyph-free: {note}");
        assert!(!note.as_bytes().contains(&0x1b), "no ANSI in the note");

        // The near-miss: equal member sets are the one case that may read as
        // agreement, so the differing branch is not the only one exercised.
        let mut agreed = count;
        let shared = ts_set(&[
            "2026-08-25T22:40:17Z",
            "2026-09-19T07:34:00Z",
            "2026-09-26T07:34:00Z",
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

    /// **The bracket shape, asserted as a pair.** The reported numbers really are
    /// (intersection, union) and really are not two equal numbers — the whole
    /// point of printing a bracket rather than a total.
    #[test]
    fn the_bracket_reports_the_intersection_and_the_union_never_one_number() {
        // Four distinct rows, two shared and one singleton each way:
        // intersection 2 (A, B), union 4 (A, B, C, D).
        let count = RetrospectiveCount {
            population: 4,
            with_unhittable: 3,
            unhittable_rows: ts_set(&[
                "2026-09-19T07:34:00Z",
                "2026-09-26T07:34:00Z",
                "2026-07-01T00:00:00Z",
            ]),
            with_unmeasurable: 0,
            undated: 0,
            // The ledger can observe no cause for this row, so its per-row line
            // is the weaker true sentence.
            tied_rows: ts_set(&[]),
            git: Some(GitCensus {
                population: 4,
                with_unhittable: 3,
                unhittable_rows: ts_set(&[
                    "2026-09-19T07:34:00Z",
                    "2026-09-26T07:34:00Z",
                    "2026-07-02T00:00:00Z",
                ]),
                with_unmeasurable: 0,
                unresolvable: 0,
                uncheckable_rows: ts_set(&[]),
                skipped_surprises: 0,
            }),
            dropped: 0,
        };
        let note = retrospective_note(&RetrospectiveReading::Counted(count), true).expect("note");
        assert!(
            note.contains(
                "; the two instruments name 2..4 row(s) (in common .. union; ledger 3, git 3)"
            ),
            "the bracket is (intersection, union) with each side's own count: {note}"
        );
        assert!(
            !note.contains("name 4 row(s) in common"),
            "the intersection must not be reported as the union: {note}"
        );
        assert!(
            !note.contains("name 2 row(s) in common"),
            "and the union must not be dropped either: {note}"
        );
    }

    /// **The two causes must not collide.** One fixture carrying a tie-shaped
    /// singleton AND a cannot-evaluate singleton: these are the day-178 and
    /// day-206 rows, they are different rows, and they must not produce the same
    /// sentence. Byte-level, at the emission point.
    #[test]
    fn a_tie_cause_and_a_could_not_check_cause_do_not_render_identically() {
        let tie_row = "2026-08-25T22:40:17Z";
        let unchecked_row = "2026-09-26T07:34:00Z";
        let count = RetrospectiveCount {
            population: 4,
            with_unhittable: 1,
            unhittable_rows: ts_set(&[unchecked_row]),
            with_unmeasurable: 0,
            undated: 0,
            tied_rows: ts_set(&[tie_row]),
            git: Some(GitCensus {
                population: 4,
                with_unhittable: 1,
                unhittable_rows: ts_set(&[tie_row]),
                with_unmeasurable: 1,
                unresolvable: 1,
                uncheckable_rows: ts_set(&[unchecked_row]),
                skipped_surprises: 1,
            }),
            dropped: 0,
        };
        let note = retrospective_note(&RetrospectiveReading::Counted(count), true).expect("note");
        let tie_line = format!("\n  row {tie_row}: only the git check");
        let unchecked_line = format!("\n  row {unchecked_row}: only the ledger");
        assert!(
            note.contains(&tie_line),
            "the tie row is attributed: {note}"
        );
        assert!(
            note.contains(&unchecked_line),
            "the uncheckable row is attributed: {note}"
        );
        // Anti-vacuous: the two rows really are distinct members of the union,
        // so a note that collapsed them into one sentence could not pass.
        assert!(
            note.contains(tie_row) && note.contains(unchecked_row) && tie_row != unchecked_row,
            "{note}"
        );
        // The sentences differ, and each names its own cause in its own words.
        assert!(
            note.contains("reads a same-second timestamp tie here"),
            "{note}"
        );
        assert!(
            note.contains("COULD NOT CHECK it (its snapshot hash does not resolve in this clone)"),
            "a check that could not run says exactly that, never 'checked; clean': {note}"
        );
        assert!(
            !note.contains("only the ledger - the git check evaluated this snapshot and found"),
            "the uncheckable row must not get the ran-and-found-present sentence: {note}"
        );
    }

    /// **The checked-and-absent direction, its own sentence.** A ledger-only row
    /// whose snapshot hash DOES resolve: the git check ran and found the path
    /// inside that tree. This is the reading that must never be conflated with
    /// could-not-check, so it is pinned on its own fixture.
    #[test]
    fn a_ledger_only_row_the_git_check_ran_and_cleared_says_so() {
        let row_ts = "2026-09-19T07:34:00Z";
        let count = RetrospectiveCount {
            population: 2,
            with_unhittable: 1,
            unhittable_rows: ts_set(&[row_ts]),
            with_unmeasurable: 0,
            undated: 0,
            tied_rows: ts_set(&[]),
            git: Some(GitCensus {
                population: 2,
                with_unhittable: 0,
                unhittable_rows: ts_set(&[]),
                with_unmeasurable: 0,
                unresolvable: 0,
                uncheckable_rows: ts_set(&[]),
                skipped_surprises: 0,
            }),
            dropped: 0,
        };
        let note = retrospective_note(&RetrospectiveReading::Counted(count), true).expect("note");
        assert!(
            note.contains(&format!(
                "\n  row {row_ts}: only the ledger - the git check evaluated this snapshot \
                 and found every surprise present inside it"
            )),
            "{note}"
        );
        assert!(!note.contains("COULD NOT CHECK"), "{note}");
        assert!(note.is_ascii(), "{note}");
    }

    /// **The regression surface for agreeing rulers, asserted on the full
    /// string.** Identical member sets print ONE line and no per-row causes —
    /// byte-identical to what this pass printed before the bracket existed.
    /// `assert_eq!` on the whole note, never a `contains`: a stray per-row line
    /// or a changed separator would otherwise slip through.
    #[test]
    fn identical_member_sets_print_one_line_and_no_per_row_causes() {
        let shared = ts_set(&["2026-08-25T22:40:17Z", "2026-09-19T07:34:00Z"]);
        let count = RetrospectiveCount {
            population: 6,
            with_unhittable: 2,
            unhittable_rows: shared.clone(),
            with_unmeasurable: 0,
            undated: 0,
            // A tie row exists in the ledger, and because the sets agree the
            // per-row causes are — deliberately — not printed at all.
            tied_rows: ts_set(&["2026-08-25T22:40:17Z"]),
            git: Some(GitCensus {
                population: 6,
                with_unhittable: 2,
                unhittable_rows: shared,
                with_unmeasurable: 0,
                unresolvable: 0,
                uncheckable_rows: ts_set(&[]),
                skipped_surprises: 0,
            }),
            dropped: 0,
        };
        let note = retrospective_note(&RetrospectiveReading::Counted(count), true).expect("note");
        assert_eq!(
            note,
            "unhittable: 2 of 6 post-ledger grading events carried a file first scored \
             after the event; the git check reads 2 of 6 rows with a snapshot hash; \
             both instruments name the same 2 row(s)"
        );
        assert!(
            !note.contains("\n  row"),
            "an agreeing pair prints no per-row causes: {note}"
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
            tied_rows: ts_set(&[]),
            git: Some(GitCensus {
                population: 3,
                with_unhittable: 0,
                unhittable_rows: ts_set(&[]),
                with_unmeasurable: 0,
                unresolvable: 0,
                uncheckable_rows: ts_set(&[]),
                skipped_surprises: 0,
            }),
            dropped: 0,
        };
        let note = retrospective_note(&RetrospectiveReading::Counted(count), true).expect("note");
        assert_eq!(
            note,
            "unhittable: 0 of 3 post-ledger grading events carried a file first scored \
             after the event; the git check reads 0 of 3 rows with a snapshot hash"
        );
    }

    // -----------------------------------------------------------------------
    // Day 210: the first-scored ledger's own dropped-line count.
    //
    // `read_first_scored` has always returned `(map, dropped)` and both call
    // sites in this module bound that second element to `_dropped`. A malformed
    // line names no path, so the path it would have dated reads exactly like a
    // path the scorer has never seen — a corrupted ledger and a clean one
    // printed identical output, and the count that breaks the tie was already
    // in the caller's hand. Same defect class as #764, one ledger over.
    // -----------------------------------------------------------------------

    /// The whole-string regression: `dropped == 0` is every clean project and
    /// this whole repo, and the note prints on every `/risk`. `assert_eq!`, not
    /// `contains` — the clause must be *absent*, not merely unasserted.
    #[test]
    fn a_clean_first_scored_ledger_is_byte_identical_and_a_dirty_one_says_so() {
        let clean = RetrospectiveCount {
            population: 3,
            with_unhittable: 1,
            unhittable_rows: ts_set(&["2026-09-03T17:23:00Z"]),
            with_unmeasurable: 1,
            tied_rows: ts_set(&[]),
            undated: 1,
            git: None,
            dropped: 0,
        };
        let zero =
            retrospective_note(&RetrospectiveReading::Counted(clean.clone()), true).expect("note");
        // The pinned pre-Day-210 output, whole-string. Nothing about this
        // session's change may move a byte of it.
        assert_eq!(
            zero,
            "unhittable: 1 of 3 post-ledger grading events carried a file first scored after \
             the event; 1 carried at least one undecidable surprise; 1 undated row(s)"
        );

        let mut dirty = clean.clone();
        dirty.dropped = 3;
        let note = retrospective_note(&RetrospectiveReading::Counted(dirty), true).expect("note");
        // The clause is APPENDED, on the note's own `"; "` separator: every
        // existing clause keeps its place and its bytes.
        assert_eq!(
            note,
            format!(
                "{zero}; 3 malformed lines in the first-scored ledger - those paths read as \
                 unmeasured, not as having no record"
            )
        );
        assert!(
            note.contains("read as unmeasured, not as having no record"),
            "the clause must separate a corrupted ledger from an unmeasured path: {note}"
        );
        assert!(note.is_ascii(), "plain mode stays glyph-free: {note}");
        assert!(!note.as_bytes().contains(&0x1b), "no ANSI: {note}");
        // Not decorative: the rich form carries the same clause behind exactly
        // one leading glyph, so the two modes cannot disagree about the fact.
        let rich =
            retrospective_note(&RetrospectiveReading::Counted(clean.clone()), false).unwrap();
        assert_eq!(rich, format!("📊 {zero}"));

        // Singular agreement, the same rule the sibling clauses follow.
        let mut one = clean.clone();
        one.dropped = 1;
        let note = retrospective_note(&RetrospectiveReading::Counted(one), true).expect("note");
        assert!(
            note.contains("; 1 malformed line in the first-scored ledger"),
            "{note}"
        );
        assert!(!note.contains("1 malformed lines"), "{note}");
    }

    /// The count is a **reader**'s reading, so the fixture has to be a ledger
    /// file: setting `count.dropped = 1` by hand would prove only that the
    /// struct has a field (Day 201).
    #[test]
    fn a_malformed_first_scored_line_reaches_the_note_through_the_real_reader() {
        let dir = tempfile::tempdir().expect("temp dir");
        let val = dir.path().join("risk_validations.jsonl");
        let led = dir.path().join("risk_first_scored.jsonl");
        let content = concat!(
            "{\"path\":\"src/seed.rs\",\"ts\":\"2026-08-22T15:40:00Z\"}\n",
            "not-json\n",
        );
        // Anti-vacuous: the hostile row is really in the bytes this test hands
        // the reader, so a transcription slip cannot make the test agree with
        // itself.
        assert!(
            content.contains("not-json"),
            "the fixture must contain the malformed line"
        );
        std::fs::write(&led, content).expect("seed ledger");
        std::fs::write(
            &val,
            "{\"ts\":\"2026-09-03T17:23:00Z\",\"hits\":[],\"surprises\":[\"src/born.rs\"],\
             \"accuracy_pct\":0.0}\n",
        )
        .expect("seed validations");

        let reading = retrospective_unhittable_at(&val, &led);
        let RetrospectiveReading::Counted(count) = &reading else {
            panic!("expected a counted reading, got {reading:?}")
        };
        assert_eq!(
            count.dropped, 1,
            "the malformed line is counted, not swallowed: {count:?}"
        );
        let note = retrospective_note(&reading, true).expect("note");
        assert!(
            note.contains("; 1 malformed line in the first-scored ledger"),
            "{note}"
        );
    }

    /// The tie this whole field exists to break: a malformed line is the ONLY
    /// reason a surprise is undecidable, and the clean-ledger control beside it
    /// proves the fixture's other member is decidable. A later change that
    /// drops the count again reddens here by name.
    #[test]
    fn a_malformed_line_can_be_the_only_reason_a_surprise_is_undecidable() {
        let dir = tempfile::tempdir().expect("temp dir");
        let val = dir.path().join("risk_validations.jsonl");
        std::fs::write(
            &val,
            "{\"ts\":\"2026-09-03T17:23:00Z\",\"hits\":[],\"surprises\":[\"src/ghost.rs\"],\
             \"accuracy_pct\":0.0}\n",
        )
        .expect("seed validations");

        // The truncated JSON is the line that *would* have dated `src/ghost.rs`
        // before the event — the readable form of a corrupted record.
        let hostile = "{\"path\":\"src/ghost.rs\",\"ts\":\"2026-09-03T18:03:00Z\"\n";
        assert!(
            serde_json::from_str::<serde_json::Value>(hostile).is_err(),
            "anti-vacuous: the fixture line must really fail to parse"
        );
        let dirty = dir.path().join("dirty.jsonl");
        std::fs::write(
            &dirty,
            format!("{{\"path\":\"src/seed.rs\",\"ts\":\"2026-08-22T15:40:00Z\"}}\n{hostile}"),
        )
        .expect("seed dirty ledger");

        let reading = retrospective_unhittable_at(&val, &dirty);
        let RetrospectiveReading::Counted(count) = &reading else {
            panic!("expected a counted reading, got {reading:?}")
        };
        assert!(
            count.with_unmeasurable > 0,
            "the ghost path has no record at all, so it is undecidable: {count:?}"
        );
        assert!(
            count.dropped > 0,
            "the reason it has no record is the malformed line, and that must be \
             visible rather than folded into the undecidable count: {count:?}"
        );
        let note = retrospective_note(&reading, true).expect("note");
        assert!(
            note.contains("malformed line") && note.contains("unmeasured"),
            "the note must distinguish corrupt from unseen: {note}"
        );

        // The control: the same event and the same path, on a ledger whose
        // record for it parses. The path is then decidable and the dropped
        // clause is gone — so the dirty arm's reading is the malformed line's
        // doing and not the fixture's shape.
        let clean = dir.path().join("clean.jsonl");
        std::fs::write(
            &clean,
            concat!(
                "{\"path\":\"src/seed.rs\",\"ts\":\"2026-08-22T15:40:00Z\"}\n",
                "{\"path\":\"src/ghost.rs\",\"ts\":\"2026-09-03T18:03:00Z\"}\n",
            ),
        )
        .expect("seed clean ledger");
        let reading = retrospective_unhittable_at(&val, &clean);
        let RetrospectiveReading::Counted(count) = &reading else {
            panic!("expected a counted reading, got {reading:?}")
        };
        assert_eq!(
            count.dropped, 0,
            "a parsing ledger drops nothing: {count:?}"
        );
        assert_eq!(
            count.with_unmeasurable, 0,
            "the same surprise is decidable once its record parses: {count:?}"
        );
        assert_eq!(count.with_unhittable, 1, "born after the event: {count:?}");
        let note = retrospective_note(&reading, true).expect("note");
        assert!(!note.contains("malformed"), "{note}");
    }

    /// One ledger, two seams: the live/watch line must carry the identical
    /// clause, because a reader who sees different numbers from `/risk` and
    /// from the watch event has no way to tell which one to believe.
    #[test]
    fn both_seams_speak_one_clause_about_one_ledger() {
        let dropped = dropped_ledger_clause(2).expect("2 malformed lines is not clean");
        assert_eq!(
            dropped,
            "; 2 malformed lines in the first-scored ledger - those paths read as \
             unmeasured, not as having no record"
        );
        assert_eq!(dropped_ledger_clause(0), None, "a clean ledger is silence");
    }
}
