//! Detection of blind rounds that were *started and never graded* — the
//! `dreams/experiments.jsonl` ledger's own slip detector.
//!
//! Extracted from `commands_risk_epistemic.rs` (which was at the fatal
//! 2000-line module cap) for the same reason `commands_risk_neverforecast.rs`
//! was: to make room without raising the ceiling. Re-exported by that module,
//! so call sites are unchanged.
//!
//! **Detection only.** Nothing here writes, renumbers or back-fills the
//! ledger. Past ledger lines are never rewritten — the drift is data about my
//! own discipline, and rewriting it would manufacture evidence.

use crate::format::{DIM, RESET, YELLOW};

/// A blind round that was *started* and never graded — an `experiment` line
/// with no `experiment_result` line under the same composite key.
///
/// `round` is `Option` on purpose: the seven earliest rounds (days 150–153)
/// predate the `round` field entirely and carry `None`, which is a real key
/// component here, not a parse failure.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct UngradedRound {
    /// Hand-assigned round number, or `None` for the pre-`round`-field rounds.
    pub(crate) round: Option<i64>,
    /// Day the round was started. Display only — never part of the key, see
    /// [`ungraded_rounds`].
    pub(crate) day: Option<i64>,
    /// The file the round studied. Half of the composite key.
    pub(crate) target: String,
}

/// The result of scanning the ledger for started-but-never-graded rounds.
/// The excluded count is carried beside the findings so an unkeyable line is
/// *counted* rather than silently dropped (Day 144: absence gets its own name).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct UngradedScan {
    pub(crate) ungraded: Vec<UngradedRound>,
    /// `experiment` lines with no usable `target`, so no key could be formed.
    /// Excluded from the pairing and reported as a count. Live ledger: 0.
    pub(crate) unkeyed_excluded: usize,
    /// Non-blank lines that did not parse as JSON at all. Counted, never
    /// silently dropped — a shrinking denominator inside my own meter is the
    /// defect this module exists to catch, and it flatters me in the dangerous
    /// direction: a dropped `experiment_result` raises a loud false alarm, but
    /// a dropped `experiment` makes a genuinely ungraded round *vanish* from
    /// the scan. Their `type` is unknowable (the line could not be read), so
    /// this is deliberately a count of unreadable **lines**, not of rounds.
    pub(crate) unparseable_excluded: usize,
}

/// Find blind rounds that were started and never graded.
///
/// **The key is `(round, target)`, not `round` alone and not `(round, day)`** —
/// and that choice is measured, not stylistic. Round ids are *hand-assigned*
/// (nothing derives `max(round)+1`), so they have collided: over the live
/// ledger, `{57: 2, 58: 2}` — round 57 was used on day 169 for `src/docs.rs`
/// (graded) and again on day 171 for `src/commands_plan.rs` (ungraded), round
/// 58 on day 169 for `src/update.rs` (graded) and again on day 172 for
/// `src/config_paths.rs` (ungraded). Keyed on `round` alone, the graded twin
/// answers for its ungraded namesake and the query returns a clean sheet while
/// two rounds are owed. Keyed on `(round, day)` the collision is resolved but
/// round 43 becomes a **false positive**: it was started on day 165 and graded
/// on day 166, and a round graded the next morning is not an ungraded round.
/// `target` is the component that separates the collided pairs *and* survives
/// a grade written on a later day. Verified against the live ledger: exactly
/// the two genuinely ungraded rounds, no false positives.
///
/// Absence rule, stated rather than absorbed: a **missing or null `round`** is
/// kept as an explicit `None` key component, not treated as unkeyable — that is
/// what makes the seven pre-`round`-field rounds of days 150–153 pair with
/// their results instead of being reported as seven false alarms. A **missing
/// or empty `target`** cannot be keyed at all, so the line is excluded and
/// counted in [`UngradedScan::unkeyed_excluded`]; it never vanishes.
///
/// Detection only. Nothing here writes, renumbers or back-fills the ledger.
/// Parses defensively like its neighbours: malformed or truncated lines
/// contribute no rounds, an empty ledger yields an empty scan, no panics. They
/// are, however, **counted** in [`UngradedScan::unparseable_excluded`] rather
/// than dropped in silence — the ledger is hand-authored, which makes me a
/// second implementation of its format with no compiler between us.
pub(crate) fn ungraded_rounds(ledger_text: &str) -> UngradedScan {
    use std::collections::HashSet;

    // Key half shared by both passes: (round, target).
    fn key(val: &serde_json::Value) -> Option<(Option<i64>, String)> {
        let target = val["target"].as_str().map(str::trim)?;
        if target.is_empty() {
            return None;
        }
        Some((val["round"].as_i64(), target.to_string()))
    }

    // Parsed ONCE, not once per kind. A single unreadable line is a single
    // real event, and parsing per pass would let it emit two counted records —
    // the "one act, a quorum of itself" defect, here inside my own meter.
    let mut values: Vec<serde_json::Value> = Vec::new();
    let mut unparseable_excluded = 0usize;
    for line in ledger_text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => values.push(v),
            Err(_) => unparseable_excluded += 1,
        }
    }
    let of_kind = |kind: &'static str| {
        values
            .iter()
            .filter(move |v| v["type"].as_str() == Some(kind))
    };

    let graded: HashSet<(Option<i64>, String)> =
        of_kind("experiment_result").filter_map(key).collect();

    let mut out = UngradedScan {
        unparseable_excluded,
        ..UngradedScan::default()
    };
    let mut seen: HashSet<(Option<i64>, String)> = HashSet::new();
    for val in of_kind("experiment") {
        let Some(k) = key(val) else {
            out.unkeyed_excluded += 1;
            continue;
        };
        if graded.contains(&k) || !seen.insert(k.clone()) {
            continue;
        }
        out.ungraded.push(UngradedRound {
            round: k.0,
            day: val["day"].as_i64(),
            target: k.1,
        });
    }
    out
}

/// One line naming the rounds that were started and never graded, or `None`
/// when the ledger is clean *and was fully read* — a clean ledger prints
/// nothing, so the common path stays byte-identical.
///
/// The qualifier is the point: with no owed rounds but unreadable lines this
/// returns a line saying so, because "could not check" must never read as
/// "checked; clean" — the unreadable line may be the very `experiment` whose
/// absence makes the scan look empty.
///
/// Detection only: this says a round is *owed*, it never grades one and never
/// writes to the ledger. The wording deliberately contains no `never forecast`
/// substring and matches neither of `scripts/extract_trajectory.py`'s row
/// regexes, so the planner's collection (which has already hard-stopped above
/// this block) is unaffected.
pub(crate) fn format_ungraded_rounds(scan: &UngradedScan) -> Option<String> {
    // Two different reasons a line was not checked, kept distinct rather than
    // summed: one had no key, the other could not be read at all. When only
    // the first fires the clause is byte-identical to before this counter
    // existed, so the common path is unchanged.
    let mut notes: Vec<String> = Vec::new();
    if scan.unkeyed_excluded > 0 {
        notes.push(format!(
            "{} line(s) had no target and could not be checked",
            scan.unkeyed_excluded
        ));
    }
    if scan.unparseable_excluded > 0 {
        notes.push(format!(
            "{} line(s) could not be parsed and were not checked",
            scan.unparseable_excluded
        ));
    }
    let excluded = if notes.is_empty() {
        String::new()
    } else {
        format!(" {DIM}({}){RESET}", notes.join("; "))
    };

    if scan.ungraded.is_empty() {
        if excluded.is_empty() {
            // Genuinely clean: prints nothing, byte-identical to before.
            return None;
        }
        // Not clean — *unread*. The dangerous case is exactly here: the line
        // that could not be parsed may have been the `experiment` whose
        // absence makes this scan look empty, so "could not check" must never
        // be allowed to read as "checked; clean".
        return Some(format!(
            "    {YELLOW}⚠{RESET} no ungraded rounds found, but the ledger was not fully read:{excluded}\n"
        ));
    }
    let listed = scan
        .ungraded
        .iter()
        .map(|r| {
            let round = match r.round {
                Some(n) => n.to_string(),
                // Absence keeps its own name rather than borrowing a number.
                None => "unnumbered".to_string(),
            };
            match r.day {
                Some(d) => format!("{round} (day {d}, {})", r.target),
                None => format!("{round} ({})", r.target),
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    // Two different reasons a line was not checked, kept distinct rather than
    // summed: one had no key, the other could not be read at all. When only
    // the first fires the clause is byte-identical to before this counter
    // existed, so the common path is unchanged.
    let mut notes: Vec<String> = Vec::new();
    if scan.unkeyed_excluded > 0 {
        notes.push(format!(
            "{} line(s) had no target and could not be checked",
            scan.unkeyed_excluded
        ));
    }
    if scan.unparseable_excluded > 0 {
        notes.push(format!(
            "{} line(s) could not be parsed and were not checked",
            scan.unparseable_excluded
        ));
    }
    let excluded = if notes.is_empty() {
        String::new()
    } else {
        format!(" {DIM}({}){RESET}", notes.join("; "))
    };
    Some(format!(
        "    {YELLOW}⚠{RESET} {} round(s) started but never graded: {listed}{excluded}\n",
        scan.ungraded.len()
    ))
}
