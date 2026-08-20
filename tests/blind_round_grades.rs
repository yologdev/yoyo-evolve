//! Structural gate: a blind round that predicted must also grade.
//!
//! A blind round writes two lines into `dreams/experiments.jsonl` — a
//! `type:"experiment"` prediction carrying `hypotheses: [...]`, and later a
//! `type:"experiment_result"` grade carrying `hypothesis_grades: [...]`. The
//! prediction keeps landing and the grade keeps not landing (#801). The
//! prediction alone is a non-empty diff that builds and tests green, so the
//! harness **accepts** the half-finished round instead of reverting it.
//!
//! The cost is not cosmetic. `latest_study_state_by_path` maps an ungraded
//! round's target to `StudyState::VisitedUngraded`, which since #744/#711
//! counts as **"not dark"** — so `never_forecast_files` stops listing the file
//! as an unexplored room and `study_tier` ranks it above genuinely unstudied
//! files. A round that taught nothing steers the exploration budget away from
//! itself. That is my own dream instrument degrading quietly.
//!
//! Day 172 shipped option 3 of the issue — make it *visible*
//! (`src/commands_risk_ungraded.rs`, surfaced under `/risk epistemic`). This is
//! option 2: make the half-written state **fail a check** instead of passing
//! one.
//!
//! **The key is `(round, target)`, not `round`.** Round numbers are
//! hand-assigned — nothing derives `max(round)+1` — and they have collided:
//! round 57 was used on day 169 for `src/docs.rs` and again on day 171 for
//! `src/commands_plan.rs`; round 58 on day 169 for `src/update.rs` and again on
//! day 172 for `src/config_paths.rs`. Any query keyed on `round` alone lets the
//! graded twin answer for its ungraded namesake. `src/commands_risk_ungraded.rs`
//! keys the same way for the same measured reason.
//!
//! Three branches that are **deliberately not the same property** (the Day-166
//! module-size lesson: a gate that can only revert a whole task eats correct
//! work, so the penalty has to be priced per branch, not per taxonomy):
//!
//! 1. An unregistered round with `registered > 0` and `graded == 0` → **fatal**.
//!    This is exactly the state #801 names: a prediction shipped with no grade.
//!    The escape hatch is the point — the message gives both remedies verbatim,
//!    and one of them is a single line in the register below. **The gate does
//!    not forbid an ungraded round; it forbids an *unnamed* one.**
//! 2. An unregistered round with `0 < graded < registered` → **warning, run
//!    stays green**. Partial grading is information, not an emergency: a
//!    partially graded round did teach something. Same message shape (round,
//!    target, both counts, the literal register line to paste).
//! 3. A **registered** round whose counts no longer match the recorded pair →
//!    **fatal**, in either direction. More grades landed (pay the debt down),
//!    or the row shrank/vanished. This is the ratchet: an exception list only
//!    pays itself down if improving is *also* a failure, otherwise progress
//!    leaves silent headroom nobody granted. It is the cheap direction — the
//!    message states the recorded numbers and the actual numbers verbatim.
//!
//! The branch-2 warning is written straight to `std::io::stderr()` rather than
//! through `eprintln!`, because libtest's capture hook swallows macro output
//! from *passing* tests — and a silent gate teaches nothing at all, which is
//! worse than a fatal one.
//!
//! **What this gate cannot do, stated plainly:** it checks that a grade is
//! *present*, never that it is *correct*. A round graded with four rubber-stamp
//! "hit" strings passes this gate exactly as a round graded honestly does.
//! Presence is the property that is mechanically checkable; honesty is not, and
//! pretending otherwise would be the "checked; clean" reading of "could not
//! check" that my pre-push hook already refuses to make.
//!
//! Unparseable lines are **counted and reported**, never silently dropped — a
//! shrinking denominator inside my own meter is the defect I keep fixing
//! elsewhere. A missing ledger file is its own explicit state: said out loud,
//! and passing, because "missing" is not "clean".
//!
//! `Kind: evolve` — this governs my own blind-round discipline; no product
//! surface changes.

use std::io::Write;
use std::path::PathBuf;

/// The blind-round ledger, relative to the crate root.
const LEDGER_PATH: &str = "dreams/experiments.jsonl";

/// Rounds whose grading debt is **acknowledged**: `(round, target, registered,
/// graded)`.
///
/// This register is **debt, not absolution**. An entry does not make a round
/// graded — it records, deliberately and by hand, that a prediction shipped
/// without its grade and that I knew. The ratchet in branch 3 is what keeps it
/// honest: the moment more grades land, the row stops matching and the gate
/// fails until the entry is paid down or deleted. The register can only shrink.
///
/// The round is stored as a **string** so a line with no `round` field still
/// has a name (`(no round)`) instead of being unkeyable — the seven pre-`round`
/// rounds of days 150–153 are real rounds, and absence gets its own value
/// rather than being absorbed by a convenient neighbour.
const GRANDFATHERED_UNGRADED_ROUNDS: &[(&str, &str, usize, usize)] = &[
    // Day 172: prediction landed, the grade never did. The exact shape #801 is
    // about, and the reason this gate exists.
    ("58", "src/config_paths.rs", 4, 0),
    // Day 171: 1 of 3 graded — the round ran out of clock mid-grade.
    // Day 169: 1 of 3 graded.
    ("58", "src/update.rs", 3, 1),
    // Day 169: 1 of 2 graded.
    ("59", "src/format/diff.rs", 2, 1),
];

/// One `(round, target)` pair as the ledger actually reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RoundTally {
    /// Display/register name of the round (`"58"`, or `"(no round)"`).
    round: String,
    target: String,
    /// First day seen for the pair — display only, never part of the key.
    day: Option<i64>,
    /// Hypotheses registered across this pair's `type:"experiment"` lines.
    registered: usize,
    /// Hypotheses graded across this pair's `type:"experiment_result"` lines.
    graded: usize,
}

#[derive(Debug, PartialEq, Eq)]
enum RoundViolation {
    /// Branch 1: a prediction with no grade at all, and nobody said so.
    UngradedRound {
        round: String,
        target: String,
        day: Option<i64>,
        registered: usize,
    },
    /// Branch 2: some grades landed, some didn't, and nobody said so.
    PartiallyGraded {
        round: String,
        target: String,
        day: Option<i64>,
        registered: usize,
        graded: usize,
    },
    /// Branch 3: a registered row whose counts moved, and the round is now
    /// fully graded — the debt is paid, the entry has to go.
    RegisteredRoundNowGraded {
        round: String,
        target: String,
        recorded: (usize, usize),
        actual: (usize, usize),
    },
    /// Branch 3: a registered row whose counts moved in any other direction.
    RegisteredCountsChanged {
        round: String,
        target: String,
        recorded: (usize, usize),
        actual: (usize, usize),
    },
    /// Branch 3: a registered row the ledger no longer knows about at all.
    RegisteredRoundVanished {
        round: String,
        target: String,
        recorded: (usize, usize),
    },
}

impl RoundViolation {
    /// Only branch 2 is non-fatal.
    ///
    /// Branch 1 is the defect the gate was written for; branch 3 is the
    /// ratchet, and both of its directions are cheap to fix (the message states
    /// the exact edit). Partial grading is the one state that genuinely taught
    /// something, so it warns and the run stays green.
    fn is_fatal(&self) -> bool {
        match self {
            RoundViolation::UngradedRound { .. } => true,
            RoundViolation::PartiallyGraded { .. } => false,
            RoundViolation::RegisteredRoundNowGraded { .. } => true,
            RoundViolation::RegisteredCountsChanged { .. } => true,
            RoundViolation::RegisteredRoundVanished { .. } => true,
        }
    }

    fn message(&self) -> String {
        match self {
            RoundViolation::UngradedRound {
                round,
                target,
                day,
                registered,
            } => format!(
                "round {round} on {target}{} registered {registered} hypothes(e)s and graded 0 — \
                 a prediction shipped with no grade.\n     \
                 Fix, either one:\n       \
                 1. grade it in this same pass — append the type:\"experiment_result\" line \
                 (with hypothesis_grades) to {LEDGER_PATH}; or\n       \
                 2. if the grade genuinely cannot land now, paste \
                 (\"{round}\", \"{target}\", {registered}, 0) into \
                 GRANDFATHERED_UNGRADED_ROUNDS and say why in the commit message.\n     \
                 This gate does not forbid an ungraded round. It forbids an unnamed one.",
                day_clause(day),
            ),
            RoundViolation::PartiallyGraded {
                round,
                target,
                day,
                registered,
                graded,
            } => format!(
                "round {round} on {target}{} graded {graded} of {registered} hypothes(e)s.\n     \
                 Not fatal — a partially graded round did teach something, and reverting the \
                 whole task teaches nothing the warning cannot.\n     \
                 Fix: grade the remaining {} by appending to {LEDGER_PATH}, or paste \
                 (\"{round}\", \"{target}\", {registered}, {graded}) into \
                 GRANDFATHERED_UNGRADED_ROUNDS to record the debt on purpose.",
                day_clause(day),
                registered.saturating_sub(*graded),
            ),
            RoundViolation::RegisteredRoundNowGraded {
                round,
                target,
                recorded,
                actual,
            } => format!(
                "round {round} on {target} is registered as ({}, {}) but is now fully graded \
                 ({} of {}). The debt is paid.\n     \
                 Fix: delete (\"{round}\", \"{target}\", {}, {}) from \
                 GRANDFATHERED_UNGRADED_ROUNDS — the register only shrinks. Fatal on purpose: \
                 an exception list that does not fail when it is paid down never pays down.",
                recorded.0, recorded.1, actual.1, actual.0, recorded.0, recorded.1,
            ),
            RoundViolation::RegisteredCountsChanged {
                round,
                target,
                recorded,
                actual,
            } => format!(
                "round {round} on {target} records ({}, {}) registered/graded but the ledger now \
                 says ({}, {}).\n     \
                 Fix: paste (\"{round}\", \"{target}\", {}, {}) over its entry in \
                 GRANDFATHERED_UNGRADED_ROUNDS. Fatal on purpose: the register only tracks \
                 reality if drift in either direction is a failure.",
                recorded.0, recorded.1, actual.0, actual.1, actual.0, actual.1,
            ),
            RoundViolation::RegisteredRoundVanished {
                round,
                target,
                recorded,
            } => format!(
                "round {round} on {target} is in GRANDFATHERED_UNGRADED_ROUNDS (recorded {} \
                 registered / {} graded) but {LEDGER_PATH} has no hypotheses for that \
                 (round, target) pair at all.\n     \
                 Fix: delete its entry — a renamed target or a rewritten line must not retire a \
                 debt without someone deciding to.",
                recorded.0, recorded.1,
            ),
        }
    }
}

fn day_clause(day: &Option<i64>) -> String {
    match day {
        Some(d) => format!(" (day {d})"),
        None => String::new(),
    }
}

/// Pure checker: given every `(round, target)` tally and the register, report
/// every violation. No I/O, so the branches are testable against synthetic
/// input rather than only against whatever the ledger happens to look like.
fn check_blind_rounds(
    tallies: &[RoundTally],
    register: &[(&str, &str, usize, usize)],
) -> Vec<RoundViolation> {
    let mut violations = Vec::new();

    for t in tallies {
        match register
            .iter()
            .find(|(r, g, _, _)| *r == t.round && *g == t.target)
        {
            Some((_, _, reg_registered, reg_graded)) => {
                let recorded = (*reg_registered, *reg_graded);
                let actual = (t.registered, t.graded);
                if recorded == actual {
                    continue;
                }
                // Fully graded now → the debt is paid and the entry must go.
                // Any other drift → repaste the true numbers.
                if t.registered > 0 && t.graded >= t.registered {
                    violations.push(RoundViolation::RegisteredRoundNowGraded {
                        round: t.round.clone(),
                        target: t.target.clone(),
                        recorded,
                        actual,
                    });
                } else {
                    violations.push(RoundViolation::RegisteredCountsChanged {
                        round: t.round.clone(),
                        target: t.target.clone(),
                        recorded,
                        actual,
                    });
                }
            }
            None => {
                if t.registered > 0 && t.graded == 0 {
                    violations.push(RoundViolation::UngradedRound {
                        round: t.round.clone(),
                        target: t.target.clone(),
                        day: t.day,
                        registered: t.registered,
                    });
                } else if t.graded > 0 && t.graded < t.registered {
                    violations.push(RoundViolation::PartiallyGraded {
                        round: t.round.clone(),
                        target: t.target.clone(),
                        day: t.day,
                        registered: t.registered,
                        graded: t.graded,
                    });
                }
                // registered == 0 (rounds predating per-hypothesis records) and
                // graded >= registered > 0 (fully graded) are the two healthy
                // states. Neither is a violation.
            }
        }
    }

    // A registered row the ledger no longer reports is its own third value —
    // not silently ignored, because a rename or a rewritten line would
    // otherwise retire a debt without anyone deciding to.
    for (round, target, reg, grd) in register {
        if !tallies
            .iter()
            .any(|t| t.round == *round && t.target == *target)
        {
            violations.push(RoundViolation::RegisteredRoundVanished {
                round: (*round).to_string(),
                target: (*target).to_string(),
                recorded: (*reg, *grd),
            });
        }
    }

    violations
}

/// What one pass over the ledger file found.
#[derive(Debug, Default)]
struct LedgerScan {
    tallies: Vec<RoundTally>,
    /// Non-blank lines that failed to parse as JSON. Counted, never dropped.
    unparseable: usize,
    /// Lines with no usable `target` — they cannot be keyed at all, so they are
    /// excluded *and* disclosed rather than vanishing.
    unkeyable: usize,
}

/// Parse the ledger text into `(round, target)` tallies.
///
/// `round` may be a JSON number or string; a missing/null one keeps its own
/// name (`(no round)`) so the pre-`round`-field rounds pair with their results
/// instead of being reported as false alarms. `day` is display-only and never
/// part of the key — round 43 started on day 165 and was graded on day 166, and
/// a round graded the next morning is not an ungraded round.
fn scan_ledger(content: &str) -> LedgerScan {
    let mut scan = LedgerScan::default();
    // Insertion-ordered so the report reads in ledger order.
    let mut order: Vec<(String, String)> = Vec::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                scan.unparseable += 1;
                continue;
            }
        };
        let kind = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let (registered, graded) = match kind {
            "experiment" => (array_len(&value, "hypotheses"), 0),
            "experiment_result" => (0, array_len(&value, "hypothesis_grades")),
            _ => continue,
        };

        let target = value
            .get("target")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or("");
        if target.is_empty() {
            scan.unkeyable += 1;
            continue;
        }
        let round = round_label(value.get("round"));
        let day = value.get("day").and_then(|v| v.as_i64());

        let key = (round.clone(), target.to_string());
        let idx = match order.iter().position(|k| *k == key) {
            Some(i) => i,
            None => {
                order.push(key);
                scan.tallies.push(RoundTally {
                    round,
                    target: target.to_string(),
                    day: None,
                    registered: 0,
                    graded: 0,
                });
                scan.tallies.len() - 1
            }
        };
        let tally = &mut scan.tallies[idx];
        tally.registered += registered;
        tally.graded += graded;
        if tally.day.is_none() {
            tally.day = day;
        }
    }

    scan
}

fn array_len(value: &serde_json::Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0)
}

/// Name of a round for keying and for the register line.
///
/// A missing/null round is an explicit value, not an unkeyable line.
fn round_label(value: Option<&serde_json::Value>) -> String {
    match value {
        Some(v) if v.is_i64() || v.is_u64() || v.is_f64() => v.to_string(),
        Some(v) => match v.as_str() {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => "(no round)".to_string(),
        },
        None => "(no round)".to_string(),
    }
}

/// Write the non-fatal half of the report straight to stderr.
///
/// Deliberately **not** `eprintln!`: libtest's capture hook intercepts the
/// print macros and discards output from *passing* tests, which is exactly what
/// a warning branch produces. A raw handle reaches the terminal in a plain
/// `cargo test` run, so "non-fatal" does not quietly become "silent".
fn write_warnings(warnings: &[&RoundViolation], scan: &LedgerScan) {
    let mut err = std::io::stderr();
    for w in warnings {
        let _ = writeln!(err, "\nblind round gate WARNING: {}", w.message());
    }
    if scan.unparseable > 0 || scan.unkeyable > 0 {
        let _ = writeln!(
            err,
            "\nblind round gate WARNING: {LEDGER_PATH} has {} unparseable line(s) and {} line(s) \
             with no target — excluded from the tally above, so that denominator is smaller than \
             the file.",
            scan.unparseable, scan.unkeyable
        );
    }
    if !warnings.is_empty() {
        let _ = writeln!(
            err,
            "     ({} non-fatal blind-round warning(s). Not failing the run — see \
             tests/blind_round_grades.rs for why.)\n",
            warnings.len()
        );
    }
    let _ = err.flush();
}

#[test]
fn blind_rounds_that_predicted_also_graded() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join(LEDGER_PATH);

    // A missing ledger is its own explicit state. "Missing" is not "clean", so
    // it is said out loud rather than passing silently — but it is also not a
    // failure: a fork may have no blind rounds at all.
    if !path.exists() {
        let mut err = std::io::stderr();
        let _ = writeln!(
            err,
            "\nblind round gate: {LEDGER_PATH} does not exist — nothing to check. \
             This is not a clean bill of health; it is an absent ledger.\n"
        );
        let _ = err.flush();
        return;
    }

    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let scan = scan_ledger(&content);

    assert!(
        !scan.tallies.is_empty(),
        "the ledger parsed to zero (round, target) pairs — the scan is broken, not the ledger \
         ({} unparseable line(s), {} unkeyable)",
        scan.unparseable,
        scan.unkeyable
    );

    let violations = check_blind_rounds(&scan.tallies, GRANDFATHERED_UNGRADED_ROUNDS);
    let (fatal, warnings): (Vec<&RoundViolation>, Vec<&RoundViolation>) =
        violations.iter().partition(|v| v.is_fatal());

    write_warnings(&warnings, &scan);

    if !fatal.is_empty() {
        let report = fatal
            .iter()
            .map(|v| format!("  - {}", v.message()))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "blind round gate failed ({} violation(s)):\n{report}\n\n\
             This gate lives in tests/blind_round_grades.rs (#801). It is deliberate: a \
             prediction without its grade is a green diff that quietly degrades my own \
             exploration ranking. It checks that a grade is PRESENT, never that it is correct.",
            fatal.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(round: &str, target: &str, registered: usize, graded: usize) -> RoundTally {
        RoundTally {
            round: round.to_string(),
            target: target.to_string(),
            day: Some(170),
            registered,
            graded,
        }
    }

    #[test]
    fn fully_graded_round_passes() {
        let v = check_blind_rounds(&[tally("60", "src/a.rs", 3, 3)], &[]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn round_with_no_registered_hypotheses_passes() {
        // Rounds predating the per-hypothesis fields carry no counts either
        // side. Absent is not ungraded.
        let v = check_blind_rounds(&[tally("12", "src/a.rs", 0, 0)], &[]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn prediction_with_no_grade_is_fatal_and_names_the_register_line() {
        let v = check_blind_rounds(&[tally("58", "src/config_paths.rs", 4, 0)], &[]);
        assert_eq!(
            v,
            vec![RoundViolation::UngradedRound {
                round: "58".to_string(),
                target: "src/config_paths.rs".to_string(),
                day: Some(170),
                registered: 4,
            }]
        );
        assert!(v[0].is_fatal());
        let msg = v[0].message();
        assert!(
            msg.contains(r#"("58", "src/config_paths.rs", 4, 0)"#),
            "the remedy must be paste-able verbatim: {msg}"
        );
        assert!(msg.contains("experiment_result"), "{msg}");
        assert!(msg.contains("It forbids an unnamed one"), "{msg}");
    }

    #[test]
    fn partially_graded_round_warns_and_stays_green() {
        let v = check_blind_rounds(&[tally("57", "src/commands_plan.rs", 3, 1)], &[]);
        assert_eq!(
            v,
            vec![RoundViolation::PartiallyGraded {
                round: "57".to_string(),
                target: "src/commands_plan.rs".to_string(),
                day: Some(170),
                registered: 3,
                graded: 1,
            }]
        );
        assert!(!v[0].is_fatal(), "partial grading must not revert a task");
        let msg = v[0].message();
        assert!(
            msg.contains(r#"("57", "src/commands_plan.rs", 3, 1)"#),
            "{msg}"
        );
        assert!(msg.contains("graded 1 of 3"), "{msg}");
    }

    #[test]
    fn registered_round_is_silent_while_the_counts_match() {
        let v = check_blind_rounds(
            &[tally("58", "src/config_paths.rs", 4, 0)],
            &[("58", "src/config_paths.rs", 4, 0)],
        );
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn paying_a_registered_debt_down_is_fatal_the_ratchet() {
        let v = check_blind_rounds(
            &[tally("58", "src/config_paths.rs", 4, 4)],
            &[("58", "src/config_paths.rs", 4, 0)],
        );
        assert_eq!(
            v,
            vec![RoundViolation::RegisteredRoundNowGraded {
                round: "58".to_string(),
                target: "src/config_paths.rs".to_string(),
                recorded: (4, 0),
                actual: (4, 4),
            }]
        );
        assert!(
            v[0].is_fatal(),
            "improving must also fail, or it never pays"
        );
        assert!(v[0].message().contains("delete"), "{}", v[0].message());
    }

    #[test]
    fn registered_counts_drifting_any_other_way_is_fatal() {
        let v = check_blind_rounds(
            &[tally("57", "src/commands_plan.rs", 3, 2)],
            &[("57", "src/commands_plan.rs", 3, 1)],
        );
        assert_eq!(
            v,
            vec![RoundViolation::RegisteredCountsChanged {
                round: "57".to_string(),
                target: "src/commands_plan.rs".to_string(),
                recorded: (3, 1),
                actual: (3, 2),
            }]
        );
        assert!(v[0].is_fatal());
        assert!(
            v[0].message()
                .contains(r#"("57", "src/commands_plan.rs", 3, 2)"#),
            "{}",
            v[0].message()
        );
    }

    #[test]
    fn registered_round_the_ledger_forgot_is_fatal() {
        let v = check_blind_rounds(&[], &[("58", "src/gone.rs", 4, 0)]);
        assert_eq!(
            v,
            vec![RoundViolation::RegisteredRoundVanished {
                round: "58".to_string(),
                target: "src/gone.rs".to_string(),
                recorded: (4, 0),
            }]
        );
        assert!(v[0].is_fatal());
    }

    #[test]
    fn the_key_is_round_and_target_so_a_collision_does_not_absolve() {
        // The measured collision: round 58 twice, one graded, one not. Keyed on
        // `round` alone the graded twin answers for its namesake and the gate
        // reports nothing.
        let v = check_blind_rounds(
            &[
                tally("58", "src/update.rs", 3, 3),
                tally("58", "src/config_paths.rs", 4, 0),
            ],
            &[],
        );
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(matches!(
            &v[0],
            RoundViolation::UngradedRound { target, .. } if target == "src/config_paths.rs"
        ));
    }

    #[test]
    fn scan_pairs_lines_by_round_and_target_and_sums_both_sides() {
        let content = r#"{"type":"experiment","day":172,"round":58,"target":"src/a.rs","hypotheses":[1,2,3]}
{"type":"experiment_result","day":173,"round":58,"target":"src/a.rs","hypothesis_grades":[1]}
{"type":"experiment","day":169,"round":58,"target":"src/b.rs","hypotheses":[1,2]}
"#;
        let scan = scan_ledger(content);
        assert_eq!(scan.tallies.len(), 2, "{:?}", scan.tallies);
        assert_eq!(scan.tallies[0].round, "58");
        assert_eq!(scan.tallies[0].target, "src/a.rs");
        assert_eq!((scan.tallies[0].registered, scan.tallies[0].graded), (3, 1));
        // day comes from the first line of the pair, and is display-only.
        assert_eq!(scan.tallies[0].day, Some(172));
        assert_eq!((scan.tallies[1].registered, scan.tallies[1].graded), (2, 0));
    }

    #[test]
    fn a_grade_written_a_day_later_still_pairs() {
        // Round 43 started day 165 and was graded day 166. `day` is not part of
        // the key precisely so this is not a false alarm.
        let content = r#"{"type":"experiment","day":165,"round":43,"target":"src/a.rs","hypotheses":[1,2]}
{"type":"experiment_result","day":166,"round":43,"target":"src/a.rs","hypothesis_grades":[1,2]}
"#;
        let scan = scan_ledger(content);
        assert!(
            check_blind_rounds(&scan.tallies, &[]).is_empty(),
            "a next-morning grade is not an ungraded round"
        );
    }

    #[test]
    fn a_missing_round_keeps_its_own_name_rather_than_being_dropped() {
        let content = r#"{"type":"experiment","day":151,"target":"src/a.rs","hypotheses":[1]}
{"type":"experiment_result","day":151,"target":"src/a.rs","hypothesis_grades":[1]}
"#;
        let scan = scan_ledger(content);
        assert_eq!(scan.tallies.len(), 1);
        assert_eq!(scan.tallies[0].round, "(no round)");
        assert!(check_blind_rounds(&scan.tallies, &[]).is_empty());
    }

    #[test]
    fn unparseable_and_targetless_lines_are_counted_not_dropped() {
        let content = "not json at all\n\
                       {\"type\":\"experiment\",\"round\":9,\"hypotheses\":[1]}\n\
                       \n\
                       {\"type\":\"experiment\",\"round\":9,\"target\":\"src/a.rs\",\"hypotheses\":[1]}\n";
        let scan = scan_ledger(content);
        assert_eq!(scan.unparseable, 1, "{scan:?}");
        assert_eq!(scan.unkeyable, 1, "{scan:?}");
        assert_eq!(scan.tallies.len(), 1);
    }

    #[test]
    fn non_experiment_lines_are_ignored() {
        let content = r#"{"type":"note","day":1,"target":"src/a.rs"}
{"type":"experiment","round":1,"target":"src/a.rs","hypotheses":[1]}
{"type":"experiment_result","round":1,"target":"src/a.rs","hypothesis_grades":[1]}
"#;
        let scan = scan_ledger(content);
        assert_eq!(scan.tallies.len(), 1);
        assert!(check_blind_rounds(&scan.tallies, &[]).is_empty());
    }
}
