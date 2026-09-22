//! The unhittable-surprise join: let a validation event say "unhittable" out
//! loud (DREAM.md milestone, cycles 9 → 10).
//!
//! A surprise the model could not have predicted — because the file did not
//! exist when the snapshot was taken — is scored exactly like a real miss. Both
//! render as a lower `accuracy_pct` with the file named under `surprises`, so
//! the number alone cannot tell whether a `0%` is foresight failing or my own
//! change restating itself. This module gives that zero an honest neighbour; it
//! never moves the number.
//!
//! **The proxy, chose deliberately.** The dream's first choice was a `git
//! cat-file` existence check against each snapshot's own `git_hash`; the dream
//! itself names the fallback for a shallow clone whose old snapshot hashes are
//! unresolvable, and the fallback is what is built here: a join against
//! `.yoyo/risk_first_scored.jsonl`, needs no git at all. It is a **proxy for
//! existence and only as complete as the ledger is** — a path the scorer never
//! saw has no birthday here and lands in `unmeasurable`, never in `unhittable`.
//!
//! Extracted into its own module (Day 206) rather than grown inside
//! `commands_risk_snapshots.rs`: that file is grandfathered at its recorded
//! 2047-line ceiling precisely so growth is acknowledged rather than absorbed,
//! and this code is a self-contained pure join that needs nothing from it but
//! `read_first_scored`/`founding_ts`/`first_scored_age`.

use crate::commands_risk_snapshots::{first_scored_age, founding_ts, read_first_scored};

/// Split a surprise list into files that were *hittable* and files that were
/// not, using the first-scored ledger as the proxy for "did this exist at the
/// snapshot".
///
/// Why this exists: a surprise the model could not have predicted — because the
/// file did not exist yet when the snapshot was taken — is scored exactly like a
/// real miss. Both render as `0%` with the file named under `surprises`, so the
/// number alone cannot tell me whether that zero is my foresight failing or my
/// own change restating itself. This is the honest neighbour for that zero.
///
/// The proxy, stated rather than implied: the ledger join is only as complete as
/// the ledger is — a path the scorer never saw has no birthday here and lands in
/// `unmeasurable`, **never** in `unhittable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct UnhittableCount {
    /// Surprises whose ledger birthday is strictly after the snapshot's `ts`.
    pub(crate) unhittable: u32,
    /// Surprises the join could actually decide (`unhittable + hittable`).
    pub(crate) measured: u32,
    /// Surprises the join could **not** decide: no ledger entry at all, a
    /// founding-batch path (unknown birthday — see [`first_scored_age`]), or a
    /// snapshot whose own `ts` is `"unknown"`.
    pub(crate) unmeasurable: u32,
}

/// Decide each surprise against the first-scored ledger.
///
/// A surprise is **unhittable** when [`first_scored_age`] is `Some(ts)` and
/// `ts > snapshot_ts`. That is a plain string compare, which is chronological
/// here because every `ts` in both files is written by one fixed-width
/// ISO-8601 UTC shape (`%Y-%m-%dT%H:%M:%SZ`), so lexicographic order is time
/// order.
///
/// It is **unmeasurable** when the join cannot decide: no ledger entry, a
/// founding-batch birthday (the survivor-trap guard — [`first_scored_age`] is
/// reused rather than re-derived), or `snapshot_ts == "unknown"`. The last case
/// is load-bearing: an unorderable snapshot must not let every surprise read as
/// *hittable*, which is the direction silence would be read in.
///
/// Invariant, pinned by tests: `measured + unmeasurable == surprises.len()`.
pub(crate) fn count_unhittable_surprises(
    surprises: &[String],
    snapshot_ts: &str,
    first_scored: &std::collections::BTreeMap<String, String>,
    founding: Option<&str>,
) -> UnhittableCount {
    let mut out = UnhittableCount::default();
    let snapshot_orderable = snapshot_ts != "unknown";
    for path in surprises {
        let Some(birthday) = first_scored_age(path, first_scored, founding) else {
            out.unmeasurable += 1;
            continue;
        };
        if !snapshot_orderable {
            // "Born after an unknown moment" is not a measurement — say so
            // rather than letting the compare default to all-hittable.
            out.unmeasurable += 1;
            continue;
        }
        out.measured += 1;
        if birthday > snapshot_ts {
            out.unhittable += 1;
        }
    }
    out
}

/// I/O wrapper for [`count_unhittable_surprises`]: read the ledger at
/// `ledger_path` and run the join. One composition for every call site — never
/// two copies that agree today.
pub(crate) fn count_unhittable_surprises_at(
    surprises: &[String],
    snapshot_ts: &str,
    ledger_path: &std::path::Path,
) -> UnhittableCount {
    let (map, _dropped) = read_first_scored(ledger_path);
    let founding = founding_ts(&map);
    count_unhittable_surprises(surprises, snapshot_ts, &map, founding)
}

/// The one clause a validation summary adds when some surprises could not have
/// been hit — `None` when there is nothing to say.
///
/// `None` is the whole regression surface: an ordinary session (every surprise
/// measured and hittable) prints nothing new, so its stderr stays byte-identical.
/// The denominator is derived from the count itself (`measured + unmeasurable`,
/// which equals the surprise-list length by construction) so there is no second
/// number to keep in sync.
pub(crate) fn unhittable_note(count: UnhittableCount, plain: bool) -> Option<String> {
    if count.unhittable == 0 && count.unmeasurable == 0 {
        return None;
    }
    let total = count.measured + count.unmeasurable;
    let verb = if count.unhittable == 1 { "was" } else { "were" };
    let mut note = format!(
        "{} of {total} surprises {verb} unhittable (first scored after this snapshot)",
        count.unhittable
    );
    if count.unmeasurable > 0 {
        note.push_str(&format!("; {} undecidable", count.unmeasurable));
    }
    // Glyph-free under plain output, matching the sibling refusal/notice
    // messages; the glyph lives at the call site so the note stays a string.
    Some(if plain { note } else { format!("📊 {note}") })
}

/// The end-to-end half: the watch-failure path measures the count from the
/// ledger beside its own snapshots and records it — plus the near-miss guard
/// that the pre-existing `accuracy_pct` did not move.
#[cfg(test)]
mod end_to_end_tests {
    use crate::commands_risk_parse::parse_validation_events;
    use crate::commands_risk_snapshots::{auto_validate_after_failure_to, RISK_FIRST_SCORED_FILE};

    /// End-to-end: the watch-failure path measures the unhittable count from
    /// the ledger beside its own snapshots, records it, and leaves the
    /// pre-existing `accuracy_pct` exactly where it was. The second half is the
    /// near-miss guard — this task adds a neighbour to the number, it does not
    /// move the number.
    #[test]
    fn test_watch_failure_records_unhittable_count_without_moving_accuracy() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let snap_path = dir.path().join("risk_snapshots.jsonl");
        let val_path = dir.path().join("risk_validations.jsonl");

        let snapshot = serde_json::json!({
            "ts": "2026-09-03T17:23:00Z", "day": 165, "git_hash": "feedfac",
            "top_10": [{"path": "src/main.rs", "score": 0.9, "signals": ["churn"]}],
        });
        std::fs::write(&snap_path, serde_json::to_string(&snapshot).unwrap()).expect("write snap");

        // The ledger beside it: one long-scored path and one born after the
        // snapshot (the shape a brand-new test file makes).
        std::fs::write(
            dir.path().join(RISK_FIRST_SCORED_FILE),
            concat!(
                "{\"path\":\"src/old.rs\",\"ts\":\"2026-08-25T10:00:00Z\"}\n",
                "{\"path\":\"src/born.rs\",\"ts\":\"2026-09-03T18:03:00Z\"}\n",
            ),
        )
        .expect("seed ledger");

        let changed = [
            "src/main.rs".to_string(),
            "src/old.rs".to_string(),
            "src/born.rs".to_string(),
        ];
        auto_validate_after_failure_to(&changed, "watch_failure", &snap_path, &val_path);

        let contents = std::fs::read_to_string(&val_path).expect("read validation file");
        let raw: serde_json::Value =
            serde_json::from_str(contents.lines().next().unwrap()).expect("valid JSON");
        assert_eq!(
            raw["unhittable_surprises"].as_u64(),
            Some(1),
            "src/born.rs was first scored after this snapshot: {contents}"
        );
        // The number this task must NOT move.
        let pct = raw["accuracy_pct"].as_f64().expect("accuracy_pct present");
        assert!(
            (pct - 33.3).abs() < 0.05,
            "1 of 3 changed files was predicted — unchanged by this diff, got {pct}"
        );
        let events = parse_validation_events(&contents);
        assert_eq!(events.len(), 1);
        assert!((events[0].accuracy_pct - 33.3).abs() < 0.1);
    }

    /// The near-miss on the record itself: an ordinary session (every surprise
    /// long-scored) writes `Some(0)` — a measurement — and *not* the absent
    /// key a legacy line carries.
    #[test]
    fn test_watch_failure_records_a_zero_when_nothing_is_unhittable() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let snap_path = dir.path().join("risk_snapshots.jsonl");
        let val_path = dir.path().join("risk_validations.jsonl");

        let snapshot = serde_json::json!({
            "ts": "2026-09-03T17:23:00Z", "day": 165, "git_hash": "feedfac",
            "top_10": [{"path": "src/main.rs", "score": 0.9, "signals": ["churn"]}],
        });
        std::fs::write(&snap_path, serde_json::to_string(&snapshot).unwrap()).expect("write snap");
        std::fs::write(
            dir.path().join(RISK_FIRST_SCORED_FILE),
            "{\"path\":\"src/old.rs\",\"ts\":\"2026-08-25T10:00:00Z\"}\n",
        )
        .expect("seed ledger");

        let changed = ["src/old.rs".to_string()];
        auto_validate_after_failure_to(&changed, "watch_failure", &snap_path, &val_path);

        let contents = std::fs::read_to_string(&val_path).expect("read validation file");
        let raw: serde_json::Value =
            serde_json::from_str(contents.lines().next().unwrap()).expect("valid JSON");
        assert_eq!(
            raw["unhittable_surprises"].as_u64(),
            Some(0),
            "measured-and-zero is written, because absent means 'not measured': {contents}"
        );
    }

    /// A snapshot with no `ts` (a legacy line) must land in `unmeasurable` — the
    /// absent/unknown distinction, end to end.
    #[test]
    fn test_watch_failure_with_an_unknown_snapshot_ts_records_no_unhittable() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let snap_path = dir.path().join("risk_snapshots.jsonl");
        let val_path = dir.path().join("risk_validations.jsonl");

        let snapshot = serde_json::json!({
            "ts": "unknown", "day": 165, "git_hash": "feedfac",
            "top_10": [{"path": "src/main.rs", "score": 0.9, "signals": ["churn"]}],
        });
        std::fs::write(&snap_path, serde_json::to_string(&snapshot).unwrap()).expect("write snap");
        std::fs::write(
            dir.path().join(RISK_FIRST_SCORED_FILE),
            "{\"path\":\"src/born.rs\",\"ts\":\"2026-09-03T18:03:00Z\"}\n",
        )
        .expect("seed ledger");

        let changed = ["src/born.rs".to_string()];
        auto_validate_after_failure_to(&changed, "watch_failure", &snap_path, &val_path);

        let contents = std::fs::read_to_string(&val_path).expect("read validation file");
        let raw: serde_json::Value =
            serde_json::from_str(contents.lines().next().unwrap()).expect("valid JSON");
        assert_eq!(
            raw["unhittable_surprises"].as_u64(),
            Some(0),
            "an unorderable snapshot decides nothing: {contents}"
        );
    }
}

/// Tests for the unhittable-surprise join — the counter that lets a `0%` say
/// whether it was a miss or a file that did not exist yet.
///
/// The whole regression surface is the near-miss row: an ordinary surprise list
/// (every path long-scored) must yield `unhittable == 0` — so the join adds a
/// neighbour to the accuracy line and never moves it.
#[cfg(test)]
mod unhittable_tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A ledger as `parse_first_scored` would hand it over: `path -> first ts`.
    fn scored(rows: &[(&str, &str)]) -> BTreeMap<String, String> {
        rows.iter()
            .map(|(p, t)| ((*p).to_string(), (*t).to_string()))
            .collect()
    }

    fn paths(rows: &[&str]) -> Vec<String> {
        rows.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_surprise_first_scored_after_the_snapshot_is_unhittable() {
        // The load-bearing row: the file was born 40 minutes after the
        // prediction was made, so no foresight could have hit it.
        let map = scored(&[
            ("src/old.rs", "2026-08-22T15:40:00Z"),
            ("src/born.rs", "2026-09-03T18:03:00Z"),
        ]);
        let founding = founding_ts(&map);
        let got = count_unhittable_surprises(
            &paths(&["src/born.rs"]),
            "2026-09-03T17:23:00Z",
            &map,
            founding,
        );
        assert_eq!(got.unhittable, 1);
        assert_eq!(got.measured, 1);
        assert_eq!(got.unmeasurable, 0);
    }

    #[test]
    fn a_surprise_first_scored_before_the_snapshot_is_hittable() {
        let map = scored(&[
            // A founding row that is NOT a surprise — without it the earliest
            // *surprise* would itself be the batch and read as unknowable.
            ("src/seed.rs", "2026-08-22T15:40:00Z"),
            ("src/old.rs", "2026-08-25T10:00:00Z"),
            ("src/later.rs", "2026-09-10T00:00:00Z"),
        ]);
        let founding = founding_ts(&map);
        let got = count_unhittable_surprises(
            &paths(&["src/old.rs"]),
            "2026-09-03T17:23:00Z",
            &map,
            founding,
        );
        assert_eq!(
            got.unhittable, 0,
            "long-scored: a real miss, not unhittable"
        );
        assert_eq!(got.measured, 1);
        assert_eq!(got.unmeasurable, 0);
    }

    #[test]
    fn a_founding_batch_path_is_unmeasurable_never_unhittable() {
        // The survivor-trap guard, inherited from `first_scored_age`: the
        // founding batch stamps every path at one instant, so those birthdays
        // are unknowns, NOT "born after the snapshot" — reading them the other
        // way would relabel the whole long-standing backlog as unhittable.
        let map = scored(&[("src/ancient.rs", "2026-08-22T15:40:00Z")]);
        let founding = founding_ts(&map);
        assert_eq!(founding, Some("2026-08-22T15:40:00Z"));
        let got = count_unhittable_surprises(
            &paths(&["src/ancient.rs"]),
            "2026-09-03T17:23:00Z",
            &map,
            founding,
        );
        assert_eq!(
            got.unhittable, 0,
            "founding batch is unknowable, not unhittable"
        );
        assert_eq!(got.measured, 0);
        assert_eq!(got.unmeasurable, 1);
    }

    #[test]
    fn a_path_absent_from_the_ledger_is_unmeasurable() {
        // The honest limit: the join is a proxy for existence and is only as
        // complete as the ledger is. A path the scorer never saw has no
        // birthday here.
        let map = scored(&[("src/known.rs", "2026-08-22T15:40:00Z")]);
        let got = count_unhittable_surprises(
            &paths(&["src/never-scored.rs"]),
            "2026-09-03T17:23:00Z",
            &map,
            None,
        );
        assert_eq!(got.unhittable, 0);
        assert_eq!(got.measured, 0);
        assert_eq!(got.unmeasurable, 1);
    }

    #[test]
    fn an_unknown_snapshot_ts_is_all_unmeasurable_never_all_hittable() {
        // Legacy snapshot lines carry the literal "unknown" (see
        // `ParsedSnapshot::ts`). "Born after an unknown moment" is not a
        // measurement — and it must not silently default to hittable either.
        let map = scored(&[
            ("src/a.rs", "2026-08-22T15:40:00Z"),
            ("src/b.rs", "2026-09-03T18:03:00Z"),
        ]);
        let founding = founding_ts(&map);
        let got = count_unhittable_surprises(
            &paths(&["src/a.rs", "src/b.rs"]),
            "unknown",
            &map,
            founding,
        );
        assert_eq!(got.unhittable, 0);
        assert_eq!(got.measured, 0);
        assert_eq!(got.unmeasurable, 2);
    }

    #[test]
    fn an_empty_surprise_list_counts_nothing() {
        let map = scored(&[("src/a.rs", "2026-08-22T15:40:00Z")]);
        let got = count_unhittable_surprises(&[], "2026-09-03T17:23:00Z", &map, None);
        assert_eq!(got, UnhittableCount::default());
        assert_eq!((got.unhittable, got.measured, got.unmeasurable), (0, 0, 0));
    }

    #[test]
    fn a_normal_surprise_list_is_all_measured_and_hittable() {
        // The whole regression surface, in one row: an ordinary session scores
        // `0` unhittable and measures every path, so nothing new prints.
        let map = scored(&[
            ("src/seed.rs", "2026-08-22T15:40:00Z"),
            ("src/a.rs", "2026-08-25T10:00:00Z"),
            ("src/b.rs", "2026-08-27T10:00:00Z"),
            ("src/c.rs", "2026-08-30T10:00:00Z"),
        ]);
        let founding = Some("2026-08-22T15:40:00Z");
        let surprises = paths(&["src/a.rs", "src/b.rs", "src/c.rs"]);
        let got = count_unhittable_surprises(&surprises, "2026-09-03T17:23:00Z", &map, founding);
        assert_eq!(got.unhittable, 0, "every path predates the snapshot");
        assert_eq!(got.measured, 3);
        assert_eq!(got.unmeasurable, 0);
        assert_eq!(got.measured + got.unmeasurable, surprises.len() as u32);
        assert_eq!(
            unhittable_note(got, false),
            None,
            "an ordinary session prints nothing new"
        );
        assert_eq!(unhittable_note(got, true), None);
    }

    #[test]
    fn the_mixed_row_partitions_every_surprise() {
        // Invariant, over the messiest input: measured + unmeasurable is the
        // list length. A surprise that falls out of both buckets would be the
        // silence this task exists to remove.
        let map = scored(&[
            ("src/founding.rs", "2026-08-22T15:40:00Z"),
            ("src/old.rs", "2026-08-25T10:00:00Z"),
            ("src/born.rs", "2026-09-03T18:03:00Z"),
        ]);
        let founding = founding_ts(&map);
        let surprises = paths(&[
            "src/founding.rs",
            "src/old.rs",
            "src/born.rs",
            "src/absent.rs",
        ]);
        let got = count_unhittable_surprises(&surprises, "2026-09-03T17:23:00Z", &map, founding);
        assert_eq!(got.unhittable, 1, "only src/born.rs postdates the snapshot");
        assert_eq!(got.measured, 2);
        assert_eq!(got.unmeasurable, 2);
        assert_eq!(got.measured + got.unmeasurable, surprises.len() as u32);
    }

    /// The note's own lexicographic claim, pinned: the compare is chronological
    /// only because both sides are the one fixed-width `...Z` shape. If that
    /// ever stops holding, this row reddens rather than the join going quietly
    /// wrong on an ordinary date pair.
    #[test]
    fn the_string_compare_is_chronological_for_the_ledger_shape() {
        let map = scored(&[
            ("src/seed.rs", "2026-08-22T15:40:00Z"),
            ("src/x.rs", "2026-09-03T18:03:00Z"),
        ]);
        // Same minute, one second later — the tightest boundary the format has.
        let just_after = count_unhittable_surprises(
            &paths(&["src/x.rs"]),
            "2026-09-03T18:02:59Z",
            &map,
            founding_ts(&map),
        );
        assert_eq!(just_after.unhittable, 1);
        // Equal timestamps are NOT "after" — the snapshot's own universe.
        let same_instant = count_unhittable_surprises(
            &paths(&["src/x.rs"]),
            "2026-09-03T18:03:00Z",
            &map,
            founding_ts(&map),
        );
        assert_eq!(same_instant.unhittable, 0);
        assert_eq!(same_instant.measured, 1);
    }

    #[test]
    fn the_note_names_the_counts_and_reads_glyph_free_when_plain() {
        let count = UnhittableCount {
            unhittable: 2,
            measured: 5,
            unmeasurable: 1,
        };
        let rich = unhittable_note(count, false).expect("a note when something is unhittable");
        assert!(
            rich.contains("2 of 6 surprises were unhittable"),
            "denominator is the whole surprise list: {rich}"
        );
        assert!(rich.contains("1 undecidable"), "{rich}");
        assert!(rich.starts_with("📊"), "{rich}");

        let plain = unhittable_note(count, true).expect("note still fires in plain mode");
        assert!(
            !plain.as_bytes().contains(&0x1b),
            "no ANSI in the note itself"
        );
        assert!(
            plain.is_ascii(),
            "plain mode must be glyph-free, got: {plain}"
        );
        // Not decorative: the two strings really do differ, so the plain branch
        // is exercised rather than agreeing with itself.
        assert_ne!(plain, rich);
    }

    #[test]
    fn the_note_agrees_with_itself_on_singular_and_plural() {
        let one = unhittable_note(
            UnhittableCount {
                unhittable: 1,
                measured: 1,
                unmeasurable: 0,
            },
            true,
        )
        .expect("note");
        assert!(one.contains("1 of 1 surprises was unhittable"), "{one}");

        let many = unhittable_note(
            UnhittableCount {
                unhittable: 3,
                measured: 3,
                unmeasurable: 0,
            },
            true,
        )
        .expect("note");
        assert!(many.contains("3 of 3 surprises were unhittable"), "{many}");
    }

    #[test]
    fn an_unmeasurable_only_row_still_speaks() {
        // The dream's own sentence: a detector that certifies nothing by its
        // silence. A row it could not decide is exactly the thing that must not
        // be absorbed into a zero.
        let count = UnhittableCount {
            unhittable: 0,
            measured: 0,
            unmeasurable: 2,
        };
        let note = unhittable_note(count, true).expect("an undecidable row still prints");
        assert!(note.contains("0 of 2 surprises were unhittable"), "{note}");
        assert!(note.contains("2 undecidable"), "{note}");
    }
}
