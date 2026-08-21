//! Prediction-accuracy stats for the `/risk` subsystem — trend detection,
//! aggregate accuracy statistics, and the accuracy report display.
//! Extracted from commands_risk.rs (Day 127) to keep that module focused
//! on scoring and command handling; commands_risk.rs re-exports everything
//! here so call sites are unchanged.

use crate::commands_risk_snapshots::ValidationEvent;
use crate::format::*;

// ── Risk prediction accuracy tracking ──

/// Trend direction for accuracy over time.
#[derive(Debug, PartialEq)]
pub(crate) enum AccuracyTrend {
    Improving,
    Declining,
    Stable,
    Insufficient, // not enough data points
}

/// Outcome-breadth boundary (Day 149): a failure whose outcome touched at
/// most this many files is one a 10-slot prediction list could *plausibly*
/// have covered. Above it, the outcome is close to unpredictable by
/// construction — a red build touching 31 files drags recall down no matter
/// how good the model is, because the list physically cannot hold 31 slots.
///
/// This is a **judgment threshold, not a derived constant.** 3 is chosen
/// because it is well inside the 10-slot list (a model with any skill should
/// land ≥1 of 3) while still admitting the majority of small breakages. If
/// the list length ever changes, revisit this number — the two are related
/// but deliberately not tied, so the split stays stable across list resizes.
pub(crate) const NARROW_OUTCOME_MAX: usize = 3;

/// Aggregate accuracy statistics computed from validation history.
///
/// Since Day 140, validation events carry two opposite polarities and a
/// "hit" means opposite things on the two day types:
/// - **failure-day** events (severity `revert`, `watch_failure`, or legacy
///   `None`): a hit = a predicted-risky file was involved in the breakage
///   → GOOD prediction (recall).
/// - **green-day** events (severity `watch_success`): a hit = a
///   predicted-risky file changed and NOTHING broke → false-alarm evidence
///   (precision-inverse).
///
/// `overall_hit_rate_pct` blends both into a semantically meaningless
/// average — it is kept only for backward compatibility (the `/status`
/// summary in `commands_risk_report.rs` reads it). The honest numbers are
/// the separated `failure_hit_rate_pct` / `green_flagged_change_rate_pct`.
pub(crate) struct AccuracyStats {
    pub(crate) total_validations: usize,
    /// Pooled hit/changed totals across BOTH polarities, and the blended rate
    /// derived from them. Kept for struct compatibility and asserted by tests;
    /// **never rendered** since Day 174 (blind round 64), which is why all
    /// three now carry the same `#[allow(dead_code)]` the blended emerging
    /// pair below has carried since Day 142. The blend averages recall on
    /// failure days with false-alarm evidence on green days, so it means
    /// nothing — it used to be the headline of the summary box regardless.
    #[allow(dead_code)]
    pub(crate) total_hits: usize,
    #[allow(dead_code)]
    pub(crate) total_changed: usize,
    /// Blended hit rate across BOTH polarities. Kept for struct
    /// compatibility; the report leads with the separated numbers instead.
    #[allow(dead_code)]
    pub(crate) overall_hit_rate_pct: f64,
    /// Number of failure-day events (severity ≠ `watch_success`).
    pub(crate) failure_samples: usize,
    /// Recall: when something broke, how often was the broken file on the
    /// risk list? Pooled over failure-day events only. `None` when there
    /// are no failure-day events yet — absence is not a score.
    pub(crate) failure_hit_rate_pct: Option<f64>,
    /// Failure-day events whose outcome touched `1 ..= NARROW_OUTCOME_MAX`
    /// files — outcomes a 10-slot prediction list could plausibly cover.
    /// `total_changed == 0` events land in neither bucket (ungraded, not
    /// narrow — Day 144: the absent case is an explicit third value).
    pub(crate) failure_narrow_samples: usize,
    /// Recall over narrow failure-day outcomes only. Pooled exactly like
    /// `failure_hit_rate_pct`, so the two are commensurable. **This is the
    /// number that grades the model** — the broad side mostly grades the
    /// breadth of my breakages. `None` when there are no narrow events.
    pub(crate) failure_narrow_hit_rate_pct: Option<f64>,
    /// Failure-day events whose outcome touched `> NARROW_OUTCOME_MAX` files.
    pub(crate) failure_broad_samples: usize,
    /// Recall over broad failure-day outcomes only. Pooled like the above.
    /// Close to unpredictable by construction; kept so the confound is
    /// visible rather than baked into one blended recall number.
    /// `None` when there are no broad events.
    pub(crate) failure_broad_hit_rate_pct: Option<f64>,
    /// Number of green-day events (severity `watch_success`).
    pub(crate) green_samples: usize,
    /// False-alarm signal: on green days, what fraction of changed files
    /// that were flagged as risky changed WITHOUT breaking? Pooled over
    /// green-day events only. `None` when there are no green-day events.
    pub(crate) green_flagged_change_rate_pct: Option<f64>,
    pub(crate) trend: AccuracyTrend,
    pub(crate) best_day: Option<(u32, f64)>, // (day, accuracy_pct)
    pub(crate) worst_day: Option<(u32, f64)>, // (day, accuracy_pct)
    /// Number of events whose anticipatory (emerging) prediction was graded.
    /// Compat-only (never rendered); read by tests.
    #[allow(dead_code)]
    pub(crate) emerging_samples: usize,
    /// Blended anticipatory average across BOTH polarities. Kept for struct
    /// compatibility (mirrors `overall_hit_rate_pct`); the report renders
    /// the separated failure/green numbers instead. `None` when no event
    /// carries an emerging grade yet (older validation history).
    #[allow(dead_code)]
    pub(crate) emerging_avg_pct: Option<f64>,
    /// Number of failure-day events with an emerging grade.
    pub(crate) emerging_failure_samples: usize,
    /// Anticipatory recall: on failure days, average emerging accuracy —
    /// how often the momentum-flagged files were the ones that broke.
    /// `None` when no failure-day event carries an emerging grade.
    pub(crate) emerging_failure_avg_pct: Option<f64>,
    /// Number of green-day events with an emerging grade.
    pub(crate) emerging_green_samples: usize,
    /// Anticipatory false-alarm signal: on green days, average emerging
    /// accuracy — momentum-flagged files that changed WITHOUT breaking.
    /// `None` when no green-day event carries an emerging grade.
    pub(crate) emerging_green_avg_pct: Option<f64>,
    /// Failure-day events with NO emerging grade (`emerging_accuracy_pct ==
    /// None` — legacy pre-emerging lines or empty-emerging snapshots).
    /// Derived: `failure_samples - emerging_failure_samples`. Rendered so a
    /// reader can tell "quiet meter" from "starved meter".
    pub(crate) emerging_failure_ungraded: usize,
    /// Green-day events with NO emerging grade. Derived:
    /// `green_samples - emerging_green_samples`.
    pub(crate) emerging_green_ungraded: usize,
    /// Event counts per severity tag (e.g. `"revert"`, `"watch_failure"`).
    /// Legacy severity-less events are counted under `"untagged"`.
    pub(crate) severity_counts: std::collections::BTreeMap<String, usize>,
}

/// Compute trend by comparing the average accuracy of the last N events
/// vs the first N events. Uses min(5, len/2) as window size.
///
/// Callers should pass failure-day events only: on green days a high
/// `accuracy_pct` means MORE false alarms, so mixing polarities would let
/// crying-wolf evidence read as an "improving" recall trend.
fn compute_accuracy_trend<E: std::borrow::Borrow<ValidationEvent>>(events: &[E]) -> AccuracyTrend {
    if events.len() < 2 {
        return AccuracyTrend::Insufficient;
    }

    let window = std::cmp::min(5, events.len() / 2).max(1);
    let first_avg: f64 = events[..window]
        .iter()
        .map(|e| e.borrow().accuracy_pct)
        .sum::<f64>()
        / window as f64;
    let last_avg: f64 = events[events.len() - window..]
        .iter()
        .map(|e| e.borrow().accuracy_pct)
        .sum::<f64>()
        / window as f64;

    let diff = last_avg - first_avg;
    if diff > 5.0 {
        AccuracyTrend::Improving
    } else if diff < -5.0 {
        AccuracyTrend::Declining
    } else {
        AccuracyTrend::Stable
    }
}

/// Authoritative green-day predicate (Day 142): a green-day event is one
/// recorded on a session where nothing broke (`severity == "watch_success"`).
/// Everything else — `revert`, `watch_failure`, and legacy severity-less
/// events — is a failure-day event. Both `compute_accuracy_stats` and the
/// effectiveness verdict in `commands_risk.rs` derive from this single
/// definition; never hand-copy the string (Day 140: never hand-type an
/// enumeration/predicate the code already owns).
pub(crate) fn is_green_event(e: &ValidationEvent) -> bool {
    e.severity.as_deref() == Some("watch_success")
}

/// Pool a set of validation events into one hit rate: `sum(hits) /
/// sum(changed)`. Single source of the pooling method so the whole-set,
/// narrow and broad failure numbers stay commensurable with each other (and
/// with the green side) instead of drifting into different arithmetic.
///
/// Empty set ⇒ `None` — zero samples is not a score of 0% (Day 144).
/// Non-empty but zero changed files ⇒ `Some(0.0)`, preserving the prior
/// behaviour of the two existing call sites.
fn pooled_hit_rate(events: &[&ValidationEvent]) -> Option<f64> {
    if events.is_empty() {
        return None;
    }
    let hits: usize = events.iter().map(|e| e.hit_count).sum();
    let changed: usize = events.iter().map(|e| e.total_changed).sum();
    if changed > 0 {
        Some((hits as f64 / changed as f64) * 100.0)
    } else {
        Some(0.0)
    }
}

/// Compute aggregate accuracy statistics from validation events.
pub(crate) fn compute_accuracy_stats(events: &[ValidationEvent]) -> AccuracyStats {
    if events.is_empty() {
        return AccuracyStats {
            total_validations: 0,
            total_hits: 0,
            total_changed: 0,
            overall_hit_rate_pct: 0.0,
            failure_samples: 0,
            failure_hit_rate_pct: None,
            failure_narrow_samples: 0,
            failure_narrow_hit_rate_pct: None,
            failure_broad_samples: 0,
            failure_broad_hit_rate_pct: None,
            green_samples: 0,
            green_flagged_change_rate_pct: None,
            trend: AccuracyTrend::Insufficient,
            best_day: None,
            worst_day: None,
            emerging_samples: 0,
            emerging_avg_pct: None,
            emerging_failure_samples: 0,
            emerging_failure_avg_pct: None,
            emerging_green_samples: 0,
            emerging_green_avg_pct: None,
            emerging_failure_ungraded: 0,
            emerging_green_ungraded: 0,
            severity_counts: std::collections::BTreeMap::new(),
        };
    }

    let total_validations = events.len();
    let total_hits: usize = events.iter().map(|e| e.hit_count).sum();
    let total_changed: usize = events.iter().map(|e| e.total_changed).sum();
    let overall_hit_rate_pct = if total_changed > 0 {
        (total_hits as f64 / total_changed as f64) * 100.0
    } else {
        0.0
    };

    // Polarity split (Day 142): a "hit" means opposite things on the two
    // day types. `watch_success` ⇒ green day (hit = false alarm); anything
    // else, including legacy severity-less events, ⇒ failure day (hit =
    // good recall). Blending them cancels into a meaningless average, so
    // each side gets its own pooled rate.
    let failure_events: Vec<&ValidationEvent> =
        events.iter().filter(|e| !is_green_event(e)).collect();
    let green_events: Vec<&ValidationEvent> = events.iter().filter(|e| is_green_event(e)).collect();

    let failure_samples = failure_events.len();
    let failure_hit_rate_pct = pooled_hit_rate(&failure_events);

    // Outcome-breadth split (Day 149). Day 148 harvested real failure days
    // out of CI history and recall came back mediocre — but the events are
    // wildly unequal in difficulty: one touched 1 file, another touched 31.
    // A 10-slot prediction list cannot cover a 31-file breakage, so pooling
    // them answers "how broad are my breakages?" as much as "how blind is my
    // model?" — the same mistake as the Day 142 polarity blend, one layer
    // down (same polarity, incommensurable difficulty). Splitting lets the
    // narrow side grade the model and the broad side own the confound.
    //
    // `total_changed == 0` lands in NEITHER bucket: a zero-file outcome is
    // ungraded, not narrow (Day 144 — the absent case is an explicit third
    // value, never absorbed by a convenient neighbour).
    let narrow_events: Vec<&ValidationEvent> = failure_events
        .iter()
        .copied()
        .filter(|e| e.total_changed >= 1 && e.total_changed <= NARROW_OUTCOME_MAX)
        .collect();
    let broad_events: Vec<&ValidationEvent> = failure_events
        .iter()
        .copied()
        .filter(|e| e.total_changed > NARROW_OUTCOME_MAX)
        .collect();
    let failure_narrow_samples = narrow_events.len();
    let failure_narrow_hit_rate_pct = pooled_hit_rate(&narrow_events);
    let failure_broad_samples = broad_events.len();
    let failure_broad_hit_rate_pct = pooled_hit_rate(&broad_events);

    let green_samples = green_events.len();
    let green_flagged_change_rate_pct = pooled_hit_rate(&green_events);

    // Best/worst day — failure-day events only, same reason as the trend:
    // a green day's 100% accuracy_pct is maximal false-alarm evidence, not
    // a "best" prediction day.
    let mut day_accuracies: std::collections::BTreeMap<u32, Vec<f64>> =
        std::collections::BTreeMap::new();
    for e in &failure_events {
        day_accuracies
            .entry(e.day)
            .or_default()
            .push(e.accuracy_pct);
    }

    let mut best_day: Option<(u32, f64)> = None;
    let mut worst_day: Option<(u32, f64)> = None;
    for (&day, accs) in &day_accuracies {
        let avg = accs.iter().sum::<f64>() / accs.len() as f64;
        let avg_rounded = (avg * 10.0).round() / 10.0;
        match best_day {
            None => best_day = Some((day, avg_rounded)),
            Some((_, best_acc)) if avg_rounded > best_acc => best_day = Some((day, avg_rounded)),
            _ => {}
        }
        match worst_day {
            None => worst_day = Some((day, avg_rounded)),
            Some((_, worst_acc)) if avg_rounded < worst_acc => worst_day = Some((day, avg_rounded)),
            _ => {}
        }
    }

    // Recall trend — failure-day events only. Green days are excluded
    // because their accuracy_pct is a false-alarm rate: a run of "clean"
    // green 100% events would otherwise read as an improving recall trend.
    // Fewer than 2 failure-day events ⇒ Insufficient (handled inside).
    let trend = compute_accuracy_trend(&failure_events);

    // Anticipatory (emerging) accuracy: average only the graded events.
    // `None` grades come from snapshots that carried no emerging list —
    // skipping them keeps the 24 historical lines from dragging the average.
    // The blended number is kept for struct compatibility only (same status
    // as `overall_hit_rate_pct`); the honest numbers are the polarity split
    // below — an emerging "hit" is vindication on a failure day and
    // false-alarm evidence on a green day, so averaging them cancels
    // rightness against wrongness.
    let graded: Vec<f64> = events
        .iter()
        .filter_map(|e| e.emerging_accuracy_pct)
        .collect();
    let emerging_samples = graded.len();
    let emerging_avg_pct = if graded.is_empty() {
        None
    } else {
        Some(graded.iter().sum::<f64>() / graded.len() as f64)
    };

    // Polarity split for the anticipatory column (Day 142, evening): same
    // classifier as the reactive split above. Ungraded events (`None`) are
    // excluded from both sides, exactly as the blended average excludes them.
    let emerging_failure_graded: Vec<f64> = failure_events
        .iter()
        .filter_map(|e| e.emerging_accuracy_pct)
        .collect();
    let emerging_failure_samples = emerging_failure_graded.len();
    let emerging_failure_avg_pct = if emerging_failure_graded.is_empty() {
        None
    } else {
        Some(emerging_failure_graded.iter().sum::<f64>() / emerging_failure_graded.len() as f64)
    };

    let emerging_green_graded: Vec<f64> = green_events
        .iter()
        .filter_map(|e| e.emerging_accuracy_pct)
        .collect();
    let emerging_green_samples = emerging_green_graded.len();
    let emerging_green_avg_pct = if emerging_green_graded.is_empty() {
        None
    } else {
        Some(emerging_green_graded.iter().sum::<f64>() / emerging_green_graded.len() as f64)
    };

    // Ungraded counts (derived, per side): events with no emerging grade at
    // all. Without these the report can't distinguish "the anticipatory
    // meter is quiet" from "the anticipatory meter has almost no samples".
    let emerging_failure_ungraded = failure_samples - emerging_failure_samples;
    let emerging_green_ungraded = green_samples - emerging_green_samples;

    // Per-severity event counts — legacy severity-less events land in
    // "untagged" so the historical lines stay visible without a fake tag.
    let mut severity_counts: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for e in events {
        let key = e.severity.as_deref().unwrap_or("untagged").to_string();
        *severity_counts.entry(key).or_insert(0) += 1;
    }

    AccuracyStats {
        total_validations,
        total_hits,
        total_changed,
        overall_hit_rate_pct,
        failure_samples,
        failure_hit_rate_pct,
        failure_narrow_samples,
        failure_narrow_hit_rate_pct,
        failure_broad_samples,
        failure_broad_hit_rate_pct,
        green_samples,
        green_flagged_change_rate_pct,
        trend,
        best_day,
        worst_day,
        emerging_samples,
        emerging_avg_pct,
        emerging_failure_samples,
        emerging_failure_avg_pct,
        emerging_green_samples,
        emerging_green_avg_pct,
        emerging_failure_ungraded,
        emerging_green_ungraded,
        severity_counts,
    }
}

/// Format the accuracy report as a compact box display.
pub(crate) fn format_accuracy_report(stats: &AccuracyStats) -> String {
    if stats.total_validations == 0 {
        return format!(
            "\n{BOLD}{CYAN}  No prediction accuracy data yet.{RESET}\n\n\
             {DIM}  Accuracy tracking starts automatically when watch commands\n\
             {DIM}  detect failures and validate them against risk predictions.\n\n\
             {DIM}  Run {RESET}/risk snapshot{DIM} first, then trigger a watch failure{RESET}\n\
             {DIM}  to begin collecting data.{RESET}\n"
        );
    }

    // The summary box's headline number. Day 174 (blind round 64): this used
    // to render `overall_hit_rate_pct` — the blend this module's own doc (see
    // `AccuracyStats`) calls semantically meaningless, because it averages
    // recall on failure days with false-alarm evidence on green days. It was
    // labelled "Hit rate" under a box titled "Risk Prediction Accuracy", so
    // the single biggest number a reader sees was the one number Day 142 split
    // apart precisely because it means nothing. `commands_risk_report.rs`
    // documents its own headline as "never returns the blend" — true there and
    // false for the box printed beside it. The box now leads with recall, the
    // same polarity-separated number the prose above it leads with; absence
    // keeps its own name rather than borrowing a 0% (Day 144).
    let headline_str = match stats.failure_hit_rate_pct {
        Some(pct) => {
            let rounded = (pct * 10.0).round() / 10.0;
            format!("{rounded:.0}% ({} events)", stats.failure_samples)
        }
        None => "(none graded)".to_string(),
    };

    // Lead with the two separated polarity numbers — a "hit" means opposite
    // things on failure days (recall) and green days (false alarm), so the
    // blended box number below is kept only for continuity.
    let failure_line = match stats.failure_hit_rate_pct {
        Some(pct) => {
            let rounded = (pct * 10.0).round() / 10.0;
            format!(
                "  {BOLD}recall{RESET} (failure days, {} events): {rounded:.0}% of breakages were on the risk list",
                stats.failure_samples
            )
        }
        None => {
            format!("  {BOLD}recall{RESET}: {DIM}(no failure-day events yet){RESET}").to_string()
        }
    };
    // Outcome-breadth split of the recall line (Day 149). See
    // `NARROW_OUTCOME_MAX`: pooling a 1-file breakage with a 31-file one
    // answers "how broad are my breakages?" as much as "how blind is my
    // model?", so render the two sides separately and own the caveat.
    let narrow_line = match stats.failure_narrow_hit_rate_pct {
        Some(pct) => {
            let rounded = (pct * 10.0).round() / 10.0;
            format!(
                "    {DIM}└{RESET} narrow outcomes (≤{NARROW_OUTCOME_MAX} files changed, {} events): {rounded:.1}%",
                stats.failure_narrow_samples
            )
        }
        None => format!(
            "    {DIM}└ narrow outcomes (≤{NARROW_OUTCOME_MAX} files changed): (no narrow failure-day events yet){RESET}"
        ),
    };
    let broad_line = match stats.failure_broad_hit_rate_pct {
        Some(pct) => {
            let rounded = (pct * 10.0).round() / 10.0;
            format!(
                "    {DIM}└{RESET} broad outcomes (>{NARROW_OUTCOME_MAX} files changed, {} events):  {rounded:.1}%",
                stats.failure_broad_samples
            )
        }
        None => format!(
            "    {DIM}└ broad outcomes (>{NARROW_OUTCOME_MAX} files changed): (no broad failure-day events yet){RESET}"
        ),
    };
    // Own the caveat rather than burying it: only print it when both sides
    // actually have events, i.e. when the comparison is meaningful.
    let breadth_note = if stats.failure_narrow_hit_rate_pct.is_some()
        && stats.failure_broad_hit_rate_pct.is_some()
    {
        format!(
            "    {DIM}a broad outcome is near-unpredictable by construction — a red build touching\n\
             \x20   dozens of files drags recall however good the model is. The narrow number grades\n\
             \x20   the model; the broad number mostly grades the breadth of my breakages.{RESET}"
        )
    } else {
        String::new()
    };

    let green_line = match stats.green_flagged_change_rate_pct {
        Some(pct) => {
            let rounded = (pct * 10.0).round() / 10.0;
            format!(
                "  {BOLD}false-alarm signal{RESET} (green days, {} events): {rounded:.0}% — flagged files changed without breaking",
                stats.green_samples
            )
        }
        None => format!("  {BOLD}false-alarm signal{RESET}: {DIM}(no green-day events yet){RESET}")
            .to_string(),
    };

    // Anticipatory (emerging) column — same polarity split as the reactive
    // pair above. The blended `emerging_avg_pct` is never rendered: an
    // emerging hit is recall evidence on failure days and false-alarm
    // evidence on green days, so the blend means nothing.
    let emerging_failure_line = match stats.emerging_failure_avg_pct {
        Some(pct) => {
            let rounded = (pct * 10.0).round() / 10.0;
            let ungraded_note = if stats.emerging_failure_ungraded > 0 {
                format!(
                    ", {} ungraded — no emerging forecast recorded",
                    stats.emerging_failure_ungraded
                )
            } else {
                String::new()
            };
            format!(
                "  {BOLD}emerging recall{RESET} (failure days, {} graded{ungraded_note}): {rounded:.0}% — momentum-flagged files were the ones that broke",
                stats.emerging_failure_samples
            )
        }
        None => format!(
            "  {BOLD}emerging recall{RESET}: {DIM}(no graded failure-day events yet){RESET}"
        ),
    };
    let emerging_green_line = match stats.emerging_green_avg_pct {
        Some(pct) => {
            let rounded = (pct * 10.0).round() / 10.0;
            let ungraded_note = if stats.emerging_green_ungraded > 0 {
                format!(
                    ", {} ungraded — no emerging forecast recorded",
                    stats.emerging_green_ungraded
                )
            } else {
                String::new()
            };
            format!(
                "  {BOLD}emerging false-alarm signal{RESET} (green days, {} graded{ungraded_note}): {rounded:.0}% — momentum-flagged files changed without breaking",
                stats.emerging_green_samples
            )
        }
        None => format!(
            "  {BOLD}emerging false-alarm signal{RESET}: {DIM}(no graded green-day events yet){RESET}"
        ),
    };

    let trend_str = match stats.trend {
        AccuracyTrend::Improving => format!("{GREEN}↑ Improving{RESET}"),
        AccuracyTrend::Declining => format!("{RED}↓ Declining{RESET}"),
        AccuracyTrend::Stable => format!("{YELLOW}→ Stable{RESET}"),
        AccuracyTrend::Insufficient => format!("{DIM}? Too few data points{RESET}"),
    };

    let best_str = match stats.best_day {
        Some((day, pct)) => format!("Day {day} ({pct:.0}%)"),
        None => "—".to_string(),
    };
    let worst_str = match stats.worst_day {
        Some((day, pct)) => format!("Day {day} ({pct:.0}%)"),
        None => "—".to_string(),
    };

    let mut report = format!("\n{failure_line}\n{narrow_line}\n{broad_line}\n");
    if !breadth_note.is_empty() {
        report.push_str(&breadth_note);
        report.push('\n');
    }
    report.push_str(&format!(
        "{green_line}\n{emerging_failure_line}\n{emerging_green_line}\n"
    ));

    // DREAM meter honesty (Day 146; Day-140 lesson: "one-sided self-measurement
    // measures recall and never precision"). Recall grades only on failure days
    // — a real breakage validated against the risk list. Green-day events grade
    // the false-alarm signal (precision). When the whole graded set is green-day
    // only, the meter has NEVER exercised recall: a reader must not mistake
    // green-day precision for the whole picture. State the asymmetry explicitly
    // — the abstention ("recall never graded") is a chosen third value, not
    // silence (Day-144 abstention discipline). `recall_coverage_note` owns the
    // three-state logic (failure>0 → None; both zero → None; green-only → note).
    if let Some(note) = recall_coverage_note(stats) {
        report.push_str(&format!(
            "  {YELLOW}⚠ {note} — this measures precision only, not recall.{RESET}\n"
        ));
    }
    report.push_str(&format!(
        "\n{BOLD}  ╭─ Risk Prediction Accuracy ─╮{RESET}\n\
         {BOLD}  │{RESET} Validations:  {:<13}{BOLD}│{RESET}\n\
         {BOLD}  │{RESET} Recall:       {:<13}{BOLD}│{RESET}\n\
         {BOLD}  │{RESET} Trend:        {:<25}{BOLD}│{RESET}\n\
         {BOLD}  │{RESET} Best day:     {:<13}{BOLD}│{RESET}\n\
         {BOLD}  │{RESET} Worst day:    {:<13}{BOLD}│{RESET}\n\
         {BOLD}  ╰────────────────────────────╯{RESET}\n",
        stats.total_validations, headline_str, trend_str, best_str, worst_str,
    ));

    // Per-severity breakdown — only shown once at least one event carries a
    // severity tag (legacy histories are all "untagged"; stay quiet for them).
    let tagged: Vec<String> = stats
        .severity_counts
        .iter()
        .filter(|(k, _)| k.as_str() != "untagged")
        .map(|(k, c)| format!("{c} {k}"))
        .collect();
    if !tagged.is_empty() {
        let untagged = stats.severity_counts.get("untagged").copied().unwrap_or(0);
        let mut parts = tagged;
        if untagged > 0 {
            parts.push(format!("{untagged} untagged"));
        }
        report.push_str(&format!(
            "{DIM}  events by severity: {}{RESET}\n",
            parts.join(", ")
        ));
    }
    report
}

/// Honest one-line note surfacing recall starvation for `/status`.
///
/// The reflex/prediction meter grades two polarities: recall (failure-day
/// events — did the risk list contain the file that broke?) and the
/// false-alarm signal (green-day events — did a flagged file change without
/// breaking?). When ALL accumulated events are green-day only, the meter is
/// measuring **precision only, never recall** — a reader glancing at a green
/// number in `/status` would wrongly believe the prediction meter is working.
///
/// This helper makes the starving half legible (Day 145; Day-140 lesson:
/// "one-sided self-measurement measures recall and never precision").
///
/// Three explicit states (Day-144 abstention discipline — the absent case is
/// a chosen third value, not absorbed into a neighbor):
/// - failure samples > 0 → meter is balanced (or has recall data) → `None`
///   (don't nag).
/// - failure samples == 0 AND green samples > 0 → precision-only → the note.
/// - no events at all (both zero) → nothing to say → `None` (silence for
///   product users with no risk data).
pub(crate) fn recall_coverage_note(stats: &AccuracyStats) -> Option<String> {
    if stats.failure_samples > 0 {
        // Recall has at least one graded event — the meter is not starving.
        return None;
    }
    if stats.green_samples == 0 {
        // No events at all — inert, say nothing.
        return None;
    }
    let n = stats.green_samples;
    let day_word = if n == 1 { "day" } else { "days" };
    Some(format!(
        "recall ungraded — 0 failure-day events ({n} green-{day_word} only)"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test category 7: Accuracy tracking ──

    #[test]
    fn test_compute_accuracy_stats_empty() {
        let stats = compute_accuracy_stats(&[]);
        assert_eq!(stats.total_validations, 0);
        assert_eq!(stats.trend, AccuracyTrend::Insufficient);
        assert!(stats.best_day.is_none());
        assert!(stats.worst_day.is_none());
    }

    #[test]
    fn test_compute_accuracy_stats_single_entry() {
        let events = vec![ValidationEvent {
            day: 110,
            hit_count: 3,
            total_changed: 5,
            accuracy_pct: 60.0,
            emerging_accuracy_pct: None,
            severity: None,
        }];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.total_validations, 1);
        assert_eq!(stats.total_hits, 3);
        assert_eq!(stats.total_changed, 5);
        assert!((stats.overall_hit_rate_pct - 60.0).abs() < 0.1);
        assert_eq!(stats.trend, AccuracyTrend::Insufficient);
        assert_eq!(stats.best_day, Some((110, 60.0)));
        assert_eq!(stats.worst_day, Some((110, 60.0)));
    }

    #[test]
    fn test_compute_accuracy_stats_emerging_average() {
        // Emerging (anticipatory) accuracy: average only the graded events,
        // count how many were graded. None-events (older snapshots) are skipped.
        let events = vec![
            ValidationEvent {
                day: 138,
                hit_count: 2,
                total_changed: 4,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: Some(100.0),
                severity: None,
            },
            ValidationEvent {
                day: 138,
                hit_count: 1,
                total_changed: 4,
                accuracy_pct: 25.0,
                emerging_accuracy_pct: None, // ungraded — must not drag the average
                severity: None,
            },
            ValidationEvent {
                day: 139,
                hit_count: 3,
                total_changed: 4,
                accuracy_pct: 75.0,
                emerging_accuracy_pct: Some(50.0),
                severity: None,
            },
        ];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.emerging_samples, 2);
        let avg = stats.emerging_avg_pct.expect("two graded events → Some");
        assert!(
            (avg - 75.0).abs() < 0.1,
            "avg of 100 and 50 is 75, got {avg}"
        );
        // All events here are failure-day (severity None) → the split's
        // failure side matches the blend and the green side is empty.
        assert_eq!(stats.emerging_failure_samples, 2);
        let f_avg = stats
            .emerging_failure_avg_pct
            .expect("two graded failure-day events → Some");
        assert!((f_avg - 75.0).abs() < 0.1);
        assert_eq!(stats.emerging_green_samples, 0);
        assert!(stats.emerging_green_avg_pct.is_none());
    }

    #[test]
    fn test_compute_accuracy_stats_emerging_split_by_polarity() {
        // Mixed green + failure events with emerging grades: each side must
        // average only its own events. Ungraded events drag neither side.
        let events = vec![
            ValidationEvent {
                day: 142,
                hit_count: 2,
                total_changed: 4,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: Some(100.0),
                severity: Some("revert".to_string()), // failure day
            },
            ValidationEvent {
                day: 142,
                hit_count: 1,
                total_changed: 2,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: Some(40.0),
                severity: Some("watch_success".to_string()), // green day
            },
            ValidationEvent {
                day: 143,
                hit_count: 3,
                total_changed: 4,
                accuracy_pct: 75.0,
                emerging_accuracy_pct: Some(60.0),
                severity: None, // legacy severity-less → failure day
            },
            ValidationEvent {
                day: 143,
                hit_count: 1,
                total_changed: 4,
                accuracy_pct: 25.0,
                emerging_accuracy_pct: None, // ungraded — drags neither side
                severity: Some("watch_success".to_string()),
            },
        ];
        let stats = compute_accuracy_stats(&events);
        // Failure side: 100 and 60 → 80.
        assert_eq!(stats.emerging_failure_samples, 2);
        let f_avg = stats.emerging_failure_avg_pct.expect("failure side graded");
        assert!(
            (f_avg - 80.0).abs() < 0.1,
            "avg of 100 and 60 is 80: {f_avg}"
        );
        // Green side: only the graded 40 — the ungraded green event is excluded.
        assert_eq!(stats.emerging_green_samples, 1);
        let g_avg = stats.emerging_green_avg_pct.expect("green side graded");
        assert!((g_avg - 40.0).abs() < 0.1, "single green grade 40: {g_avg}");
    }

    #[test]
    fn test_compute_accuracy_stats_emerging_none_when_ungraded() {
        // All-None history (legacy emerging-less lines) → no emerging stats.
        let events = vec![ValidationEvent {
            day: 110,
            hit_count: 3,
            total_changed: 5,
            accuracy_pct: 60.0,
            emerging_accuracy_pct: None,
            severity: None,
        }];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.emerging_samples, 0);
        assert!(stats.emerging_avg_pct.is_none());
    }

    #[test]
    fn test_format_accuracy_report_recall_never_graded_when_green_only() {
        // DREAM meter honesty: when ALL events are green-day (watch_success)
        // and ZERO failure-day events exist, the report must STATE that recall
        // has never been graded — the number measures precision only. A reader
        // must not mistake green-day precision for the whole picture.
        let events = vec![
            ValidationEvent {
                day: 145,
                hit_count: 1,
                total_changed: 3,
                accuracy_pct: 33.3,
                emerging_accuracy_pct: None,
                severity: Some("watch_success".to_string()),
            },
            ValidationEvent {
                day: 146,
                hit_count: 0,
                total_changed: 2,
                accuracy_pct: 0.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_success".to_string()),
            },
        ];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.failure_samples, 0, "no failure-day events in fixture");
        assert!(stats.green_samples > 0, "green events present in fixture");
        let report = format_accuracy_report(&stats);
        assert!(
            report.contains("measures precision only"),
            "green-only log must state recall was never graded: {report}"
        );
        // Must NOT fabricate a recall percentage.
        assert!(
            !report.contains("of breakages were on the risk list"),
            "no recall percentage should be claimed with zero failure events: {report}"
        );
        // Day 174 (blind round 64): the summary box leads with recall, so with
        // zero failure-day events its headline must ABSTAIN by name rather
        // than borrow a 0% (Day-144 abstention discipline). Before this the
        // box rendered the blended `overall_hit_rate_pct`, which on a
        // green-only log is pure false-alarm evidence displayed under the
        // label "Hit rate" — a number that could only be misread.
        assert!(
            report.contains("(none graded)"),
            "green-only log must abstain in the box headline: {report}"
        );
    }

    #[test]
    fn test_format_accuracy_report_no_recall_note_when_failure_events_present() {
        // Near-miss side (Day-122 lesson): once ANY failure-day event exists,
        // the honest "precision only" line must be ABSENT — recall IS being
        // graded, so the asymmetry warning would be wrong.
        let events = vec![
            ValidationEvent {
                day: 145,
                hit_count: 2,
                total_changed: 4,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_failure".to_string()),
            },
            ValidationEvent {
                day: 146,
                hit_count: 1,
                total_changed: 3,
                accuracy_pct: 33.3,
                emerging_accuracy_pct: None,
                severity: Some("watch_success".to_string()),
            },
        ];
        let stats = compute_accuracy_stats(&events);
        assert!(
            stats.failure_samples > 0,
            "failure event present in fixture"
        );
        let report = format_accuracy_report(&stats);
        assert!(
            !report.contains("measures precision only"),
            "recall-not-graded note must be absent when a failure event exists: {report}"
        );
    }

    #[test]
    fn test_format_accuracy_report_shows_emerging_line() {
        let events = vec![ValidationEvent {
            day: 139,
            hit_count: 2,
            total_changed: 4,
            accuracy_pct: 50.0,
            emerging_accuracy_pct: Some(75.0),
            severity: None,
        }];
        let stats = compute_accuracy_stats(&events);
        let report = format_accuracy_report(&stats);
        assert!(
            report.contains("emerging recall"),
            "report must surface the anticipatory recall line: {report}"
        );
        assert!(
            report.contains("75%"),
            "graded emerging accuracy must appear: {report}"
        );
    }

    #[test]
    fn test_format_accuracy_report_emerging_dash_when_ungraded() {
        let events = vec![ValidationEvent {
            day: 110,
            hit_count: 3,
            total_changed: 5,
            accuracy_pct: 60.0,
            emerging_accuracy_pct: None,
            severity: None,
        }];
        let stats = compute_accuracy_stats(&events);
        let report = format_accuracy_report(&stats);
        assert!(
            report.contains("no graded failure-day events yet"),
            "lines present even without data so the gap is visible: {report}"
        );
        assert!(
            report.contains("no graded green-day events yet"),
            "green side also shows honest absence, not a fake 0%: {report}"
        );
    }

    #[test]
    fn test_compute_accuracy_stats_emerging_ungraded_counts_per_side() {
        // 3 failure-day events (2 graded, 1 None) + 2 green-day events
        // (1 graded, 1 None). Ungraded counts must be derived per side and
        // None events must contribute NO 0.0 sample to either average.
        let events = vec![
            ValidationEvent {
                day: 140,
                hit_count: 1,
                total_changed: 2,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: Some(100.0),
                severity: Some("watch_failure".to_string()),
            },
            ValidationEvent {
                day: 141,
                hit_count: 1,
                total_changed: 2,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: Some(60.0),
                severity: Some("revert".to_string()),
            },
            ValidationEvent {
                day: 142,
                hit_count: 1,
                total_changed: 2,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: None, // legacy line, no forecast
                severity: None,
            },
            ValidationEvent {
                day: 143,
                hit_count: 1,
                total_changed: 2,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: Some(40.0),
                severity: Some("watch_success".to_string()),
            },
            ValidationEvent {
                day: 143,
                hit_count: 1,
                total_changed: 2,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: None, // empty emerging list → ungraded
                severity: Some("watch_success".to_string()),
            },
        ];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.emerging_failure_samples, 2);
        assert_eq!(stats.emerging_failure_ungraded, 1);
        assert_eq!(stats.emerging_green_samples, 1);
        assert_eq!(stats.emerging_green_ungraded, 1);
        // Pin the filter_map behavior: a None grade is EXCLUDED, not a 0.0
        // sample — the averages must be exactly the graded values' means.
        let f_avg = stats.emerging_failure_avg_pct.expect("failure side graded");
        assert!(
            (f_avg - 80.0).abs() < 0.1,
            "None must not drag the failure avg toward 0: {f_avg}"
        );
        let g_avg = stats.emerging_green_avg_pct.expect("green side graded");
        assert!(
            (g_avg - 40.0).abs() < 0.1,
            "None must not drag the green avg toward 0: {g_avg}"
        );
    }

    #[test]
    fn test_format_accuracy_report_ungraded_clause_when_present() {
        // Failure side: 1 graded + 2 ungraded; green side: 1 graded + 1 ungraded.
        let events = vec![
            ValidationEvent {
                day: 140,
                hit_count: 1,
                total_changed: 2,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: Some(75.0),
                severity: Some("watch_failure".to_string()),
            },
            ValidationEvent {
                day: 141,
                hit_count: 1,
                total_changed: 2,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 142,
                hit_count: 1,
                total_changed: 2,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: None,
                severity: Some("revert".to_string()),
            },
            ValidationEvent {
                day: 143,
                hit_count: 1,
                total_changed: 2,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: Some(40.0),
                severity: Some("watch_success".to_string()),
            },
            ValidationEvent {
                day: 143,
                hit_count: 1,
                total_changed: 2,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_success".to_string()),
            },
        ];
        let stats = compute_accuracy_stats(&events);
        let report = format_accuracy_report(&stats);
        assert!(
            report.contains("1 graded, 2 ungraded — no emerging forecast recorded"),
            "failure line must count its ungraded events: {report}"
        );
        assert!(
            report.contains("1 graded, 1 ungraded — no emerging forecast recorded"),
            "green line must count its ungraded events: {report}"
        );
    }

    #[test]
    fn test_format_accuracy_report_no_ungraded_clause_on_clean_data() {
        // Every event graded → no ungraded noise, exact pre-change wording.
        let events = vec![
            ValidationEvent {
                day: 140,
                hit_count: 1,
                total_changed: 2,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: Some(75.0),
                severity: Some("watch_failure".to_string()),
            },
            ValidationEvent {
                day: 141,
                hit_count: 1,
                total_changed: 2,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: Some(40.0),
                severity: Some("watch_success".to_string()),
            },
        ];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.emerging_failure_ungraded, 0);
        assert_eq!(stats.emerging_green_ungraded, 0);
        let report = format_accuracy_report(&stats);
        assert!(
            !report.contains("ungraded"),
            "clean data must render without the ungraded clause: {report}"
        );
        assert!(
            report.contains("(failure days, 1 graded):"),
            "zero-ungraded wording must be byte-identical to today's: {report}"
        );
        assert!(
            report.contains("(green days, 1 graded):"),
            "zero-ungraded wording must be byte-identical to today's: {report}"
        );
    }

    #[test]
    fn test_compute_accuracy_trend_improving() {
        let events = vec![
            ValidationEvent {
                day: 100,
                hit_count: 1,
                total_changed: 5,
                accuracy_pct: 20.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 101,
                hit_count: 1,
                total_changed: 5,
                accuracy_pct: 25.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 102,
                hit_count: 2,
                total_changed: 5,
                accuracy_pct: 40.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 103,
                hit_count: 3,
                total_changed: 5,
                accuracy_pct: 60.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 104,
                hit_count: 4,
                total_changed: 5,
                accuracy_pct: 80.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 105,
                hit_count: 4,
                total_changed: 5,
                accuracy_pct: 80.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
        ];
        let trend = compute_accuracy_trend(&events);
        assert_eq!(trend, AccuracyTrend::Improving);
    }

    #[test]
    fn test_compute_accuracy_trend_declining() {
        let events = vec![
            ValidationEvent {
                day: 100,
                hit_count: 4,
                total_changed: 5,
                accuracy_pct: 80.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 101,
                hit_count: 4,
                total_changed: 5,
                accuracy_pct: 75.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 102,
                hit_count: 3,
                total_changed: 5,
                accuracy_pct: 60.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 103,
                hit_count: 2,
                total_changed: 5,
                accuracy_pct: 40.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 104,
                hit_count: 1,
                total_changed: 5,
                accuracy_pct: 20.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 105,
                hit_count: 1,
                total_changed: 5,
                accuracy_pct: 15.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
        ];
        let trend = compute_accuracy_trend(&events);
        assert_eq!(trend, AccuracyTrend::Declining);
    }

    #[test]
    fn test_compute_accuracy_trend_stable() {
        let events = vec![
            ValidationEvent {
                day: 100,
                hit_count: 3,
                total_changed: 5,
                accuracy_pct: 60.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 101,
                hit_count: 3,
                total_changed: 5,
                accuracy_pct: 58.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 102,
                hit_count: 3,
                total_changed: 5,
                accuracy_pct: 62.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 103,
                hit_count: 3,
                total_changed: 5,
                accuracy_pct: 59.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
        ];
        let trend = compute_accuracy_trend(&events);
        assert_eq!(trend, AccuracyTrend::Stable);
    }

    #[test]
    fn test_compute_accuracy_trend_insufficient() {
        let events = vec![ValidationEvent {
            day: 100,
            hit_count: 3,
            total_changed: 5,
            accuracy_pct: 60.0,
            emerging_accuracy_pct: None,
            severity: None,
        }];
        let trend = compute_accuracy_trend(&events);
        assert_eq!(trend, AccuracyTrend::Insufficient);
    }

    #[test]
    fn test_compute_accuracy_stats_best_worst_day() {
        let events = vec![
            ValidationEvent {
                day: 108,
                hit_count: 1,
                total_changed: 5,
                accuracy_pct: 20.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 110,
                hit_count: 2,
                total_changed: 5,
                accuracy_pct: 40.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 115,
                hit_count: 4,
                total_changed: 5,
                accuracy_pct: 80.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
        ];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.best_day, Some((115, 80.0)));
        assert_eq!(stats.worst_day, Some((108, 20.0)));
    }

    #[test]
    fn test_compute_accuracy_stats_multiple_events_same_day() {
        let events = vec![
            ValidationEvent {
                day: 110,
                hit_count: 1,
                total_changed: 5,
                accuracy_pct: 20.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
            ValidationEvent {
                day: 110,
                hit_count: 4,
                total_changed: 5,
                accuracy_pct: 80.0,
                emerging_accuracy_pct: None,
                severity: None,
            },
        ];
        let stats = compute_accuracy_stats(&events);
        // Average for day 110 = (20 + 80) / 2 = 50
        assert_eq!(stats.best_day, Some((110, 50.0)));
        assert_eq!(stats.worst_day, Some((110, 50.0)));
    }

    #[test]
    fn test_format_accuracy_report_empty() {
        let stats = compute_accuracy_stats(&[]);
        let report = format_accuracy_report(&stats);
        assert!(report.contains("No prediction accuracy data yet"));
        assert!(report.contains("/risk snapshot"));
    }

    #[test]
    fn test_format_accuracy_report_with_data() {
        let stats = AccuracyStats {
            total_validations: 12,
            total_hits: 7,
            total_changed: 12,
            overall_hit_rate_pct: 58.333,
            failure_samples: 12,
            failure_hit_rate_pct: Some(58.333),
            failure_narrow_samples: 8,
            failure_narrow_hit_rate_pct: Some(75.0),
            failure_broad_samples: 4,
            failure_broad_hit_rate_pct: Some(25.0),
            green_samples: 0,
            green_flagged_change_rate_pct: None,
            trend: AccuracyTrend::Improving,
            best_day: Some((115, 80.0)),
            worst_day: Some((108, 20.0)),
            emerging_samples: 0,
            emerging_avg_pct: None,
            emerging_failure_samples: 0,
            emerging_failure_avg_pct: None,
            emerging_green_samples: 0,
            emerging_green_avg_pct: None,
            emerging_failure_ungraded: 12,
            emerging_green_ungraded: 0,
            severity_counts: std::collections::BTreeMap::new(),
        };
        let report = format_accuracy_report(&stats);
        assert!(report.contains("Risk Prediction Accuracy"));
        assert!(report.contains("12"));
        assert!(report.contains("58%"));
        // Day 174 (blind round 64): this line used to be
        // `assert!(report.contains("7/12"))` — total_hits/total_changed, the
        // blended pair from BOTH polarities, which the box rendered as its
        // headline "Hit rate". That assertion pinned the defect as a green
        // invariant (Day-148 lesson). The box now leads with recall, so the
        // blend must be absent and the failure-day sample count present.
        assert!(report.contains("Recall:"));
        assert!(report.contains("12 events"));
        assert!(
            !report.contains("7/12"),
            "the blended hits/changed pair must no longer be rendered: {report}"
        );
        assert!(report.contains("Improving"));
        assert!(report.contains("Day 115"));
        assert!(report.contains("Day 108"));
    }

    // ── Polarity split (Day 142): green vs failure days ──

    /// Fixture: 2 failure-day events (recall) + 2 green-day events (false
    /// alarm). A "hit" means opposite things on the two day types, so the
    /// blended average is flattering nonsense.
    fn mixed_polarity_events() -> Vec<ValidationEvent> {
        vec![
            ValidationEvent {
                day: 140,
                hit_count: 2,
                total_changed: 4,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_failure".to_string()),
            },
            ValidationEvent {
                day: 141,
                hit_count: 2,
                total_changed: 4,
                accuracy_pct: 50.0,
                emerging_accuracy_pct: None,
                severity: Some("revert".to_string()),
            },
            ValidationEvent {
                day: 141,
                hit_count: 4,
                total_changed: 4,
                accuracy_pct: 100.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_success".to_string()),
            },
            ValidationEvent {
                day: 142,
                hit_count: 4,
                total_changed: 4,
                accuracy_pct: 100.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_success".to_string()),
            },
        ]
    }

    #[test]
    fn test_polarity_split_separates_recall_from_false_alarm() {
        let stats = compute_accuracy_stats(&mixed_polarity_events());
        // Old blended average: (2+2+4+4)/16 = 75% — flattering.
        assert!(
            (stats.overall_hit_rate_pct - 75.0).abs() < 0.1,
            "blended average should be the flattering 75%, got {}",
            stats.overall_hit_rate_pct
        );
        // But the honest separated numbers tell a different story:
        assert_eq!(stats.failure_samples, 2);
        let recall = stats.failure_hit_rate_pct.expect("2 failure events → Some");
        assert!(
            (recall - 50.0).abs() < 0.1,
            "recall is 4/8 = 50%, got {recall}"
        );
        assert_eq!(stats.green_samples, 2);
        let false_alarm = stats
            .green_flagged_change_rate_pct
            .expect("2 green events → Some");
        assert!(
            (false_alarm - 100.0).abs() < 0.1,
            "green flagged-change rate is 8/8 = 100%, got {false_alarm}"
        );
    }

    #[test]
    fn test_polarity_split_report_contains_both_labeled_lines() {
        let stats = compute_accuracy_stats(&mixed_polarity_events());
        let report = format_accuracy_report(&stats);
        assert!(
            report.contains("(failure days, 2 events)")
                && report.contains("of breakages were on the risk list"),
            "report must lead with the labeled recall line: {report}"
        );
        assert!(
            report.contains("(green days, 2 events)")
                && report.contains("flagged files changed without breaking"),
            "report must include the labeled false-alarm line: {report}"
        );
    }

    #[test]
    fn test_polarity_split_zero_sample_sides_print_no_events_yet() {
        // Failure-only history → the green side must say "(no ... yet)",
        // never 0.0% — absence is not a score.
        let failure_only = vec![ValidationEvent {
            day: 140,
            hit_count: 2,
            total_changed: 4,
            accuracy_pct: 50.0,
            emerging_accuracy_pct: None,
            severity: Some("watch_failure".to_string()),
        }];
        let stats = compute_accuracy_stats(&failure_only);
        assert_eq!(stats.green_samples, 0);
        assert!(stats.green_flagged_change_rate_pct.is_none());
        let report = format_accuracy_report(&stats);
        assert!(
            report.contains("(no green-day events yet)"),
            "zero green samples must print the no-events text: {report}"
        );

        // Green-only history → the failure side must say "(no ... yet)".
        let green_only = vec![
            ValidationEvent {
                day: 141,
                hit_count: 4,
                total_changed: 4,
                accuracy_pct: 100.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_success".to_string()),
            },
            ValidationEvent {
                day: 142,
                hit_count: 4,
                total_changed: 4,
                accuracy_pct: 100.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_success".to_string()),
            },
        ];
        let stats = compute_accuracy_stats(&green_only);
        assert_eq!(stats.failure_samples, 0);
        assert!(stats.failure_hit_rate_pct.is_none());
        // Two green events but zero failure events → recall trend must stay
        // Insufficient (green days are excluded from the trend).
        assert_eq!(stats.trend, AccuracyTrend::Insufficient);
        let report = format_accuracy_report(&stats);
        assert!(
            report.contains("(no failure-day events yet)"),
            "zero failure samples must print the no-events text: {report}"
        );
    }

    #[test]
    fn test_green_100pct_event_does_not_raise_failure_recall() {
        let failure_events = vec![
            ValidationEvent {
                day: 140,
                hit_count: 1,
                total_changed: 4,
                accuracy_pct: 25.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_failure".to_string()),
            },
            ValidationEvent {
                day: 141,
                hit_count: 1,
                total_changed: 4,
                accuracy_pct: 25.0,
                emerging_accuracy_pct: None,
                severity: Some("revert".to_string()),
            },
        ];
        let baseline = compute_accuracy_stats(&failure_events);
        let baseline_recall = baseline.failure_hit_rate_pct.expect("failure events");

        let mut with_green = failure_events;
        with_green.push(ValidationEvent {
            day: 142,
            hit_count: 4,
            total_changed: 4,
            accuracy_pct: 100.0,
            emerging_accuracy_pct: None,
            severity: Some("watch_success".to_string()),
        });
        let mixed = compute_accuracy_stats(&with_green);
        let mixed_recall = mixed.failure_hit_rate_pct.expect("failure events");
        assert!(
            (mixed_recall - baseline_recall).abs() < f64::EPSILON,
            "a green 100% event must not raise failure-day recall: \
             baseline {baseline_recall}, with green {mixed_recall}"
        );
        // The blended number DOES move — that's exactly the bug the split fixes.
        assert!(
            mixed.overall_hit_rate_pct > baseline.overall_hit_rate_pct,
            "sanity: the blended average is what the green event inflates"
        );
        // And best day must not become the green day.
        assert_ne!(
            mixed.best_day.map(|(d, _)| d),
            Some(142),
            "a green day's 100% is false-alarm evidence, not a best prediction day"
        );
    }

    #[test]
    fn test_recall_coverage_note_when_no_failure_events() {
        // All green-day events, zero failure-day events → the meter is
        // measuring precision only. The note must name the precision-only
        // state and the green count so /status can't hide it behind a number.
        let events = vec![
            ValidationEvent {
                day: 145,
                hit_count: 1,
                total_changed: 3,
                accuracy_pct: 33.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_success".to_string()),
            },
            ValidationEvent {
                day: 145,
                hit_count: 0,
                total_changed: 2,
                accuracy_pct: 0.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_success".to_string()),
            },
        ];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.failure_samples, 0);
        assert_eq!(stats.green_samples, 2);
        let note = recall_coverage_note(&stats).expect("note present when recall starved");
        assert!(note.contains("recall ungraded"), "note: {note}");
        assert!(note.contains("0 failure-day"), "note: {note}");
        assert!(note.contains("2 green-days"), "note: {note}");
    }

    #[test]
    fn test_recall_coverage_note_absent_when_failure_events_exist() {
        // Once a failure-day (recall-graded) event exists the meter is no
        // longer precision-only — don't nag.
        let events = vec![
            ValidationEvent {
                day: 145,
                hit_count: 2,
                total_changed: 3,
                accuracy_pct: 66.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_failure".to_string()),
            },
            ValidationEvent {
                day: 145,
                hit_count: 0,
                total_changed: 2,
                accuracy_pct: 0.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_success".to_string()),
            },
        ];
        let stats = compute_accuracy_stats(&events);
        assert!(stats.failure_samples > 0);
        assert!(
            recall_coverage_note(&stats).is_none(),
            "no note once recall has data"
        );
    }

    #[test]
    fn test_recall_coverage_note_silent_when_no_events_at_all() {
        // Abstention discipline (Day 144): "no answer" is an explicit third
        // state, not absorbed into the precision-only note. Zero events → inert
        // (product users with no risk data see nothing).
        let stats = compute_accuracy_stats(&[]);
        assert_eq!(stats.failure_samples, 0);
        assert_eq!(stats.green_samples, 0);
        assert!(
            recall_coverage_note(&stats).is_none(),
            "silent when there are zero events"
        );
    }

    // ── Day 149: outcome-breadth split of failure-day recall ──

    /// Build a failure-day (severity-less → failure) event with the given
    /// breadth. `accuracy_pct` is derived so fixtures stay self-consistent.
    fn breadth_event(day: u32, hit_count: usize, total_changed: usize) -> ValidationEvent {
        let accuracy_pct = if total_changed > 0 {
            (hit_count as f64 / total_changed as f64) * 100.0
        } else {
            0.0
        };
        ValidationEvent {
            day,
            hit_count,
            total_changed,
            accuracy_pct,
            emerging_accuracy_pct: None,
            severity: Some("ci_failure".to_string()),
        }
    }

    #[test]
    fn test_breadth_split_fixture_table() {
        // (name, events, expected narrow samples/rate, expected broad samples/rate)
        #[allow(clippy::type_complexity)]
        let cases: Vec<(
            &str,
            Vec<ValidationEvent>,
            usize,
            Option<f64>,
            usize,
            Option<f64>,
        )> = vec![
            ("both sides empty", vec![], 0, None, 0, None),
            (
                "narrow only — pooled 3 hits / 4 changed",
                vec![breadth_event(149, 1, 1), breadth_event(149, 2, 3)],
                2,
                Some(75.0),
                0,
                None,
            ),
            (
                "broad only — pooled 7 hits / 31 changed",
                vec![breadth_event(148, 7, 31)],
                0,
                None,
                1,
                Some(7.0 / 31.0 * 100.0),
            ),
            (
                "mixed — each side pooled independently",
                vec![
                    breadth_event(148, 1, 1),
                    breadth_event(148, 7, 31),
                    breadth_event(149, 1, 3),
                ],
                2,
                Some(50.0), // (1 + 1) / (1 + 3)
                1,
                Some(7.0 / 31.0 * 100.0),
            ),
            (
                "zero-change outcome lands in neither bucket",
                vec![breadth_event(149, 0, 0), breadth_event(149, 2, 2)],
                1,
                Some(100.0),
                0,
                None,
            ),
            (
                "boundary: 3 files is narrow, 4 files is broad",
                vec![breadth_event(149, 3, 3), breadth_event(149, 1, 4)],
                1,
                Some(100.0),
                1,
                Some(25.0),
            ),
        ];

        for (name, events, want_narrow_n, want_narrow_pct, want_broad_n, want_broad_pct) in cases {
            let stats = compute_accuracy_stats(&events);
            assert_eq!(
                stats.failure_narrow_samples, want_narrow_n,
                "[{name}] narrow sample count"
            );
            assert_eq!(
                stats.failure_broad_samples, want_broad_n,
                "[{name}] broad sample count"
            );
            match (stats.failure_narrow_hit_rate_pct, want_narrow_pct) {
                (None, None) => {}
                (Some(got), Some(want)) => assert!(
                    (got - want).abs() < 0.05,
                    "[{name}] narrow rate: got {got}, want {want}"
                ),
                (got, want) => panic!("[{name}] narrow rate mismatch: got {got:?}, want {want:?}"),
            }
            match (stats.failure_broad_hit_rate_pct, want_broad_pct) {
                (None, None) => {}
                (Some(got), Some(want)) => assert!(
                    (got - want).abs() < 0.05,
                    "[{name}] broad rate: got {got}, want {want}"
                ),
                (got, want) => panic!("[{name}] broad rate mismatch: got {got:?}, want {want:?}"),
            }
            // Invariant: the two breadth buckets partition a subset of the
            // failure-day events — never more than the whole.
            assert!(
                stats.failure_narrow_samples + stats.failure_broad_samples <= stats.failure_samples,
                "[{name}] breadth buckets must not exceed failure_samples"
            );
        }
    }

    #[test]
    fn test_breadth_split_partitions_all_failure_events_when_no_zero_change() {
        // With no zero-change outcomes present, narrow + broad == failure_samples.
        let events = vec![
            breadth_event(148, 1, 1),
            breadth_event(148, 2, 3),
            breadth_event(148, 7, 31),
            breadth_event(149, 1, 4),
        ];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.failure_samples, 4);
        assert_eq!(
            stats.failure_narrow_samples + stats.failure_broad_samples,
            stats.failure_samples,
            "no zero-change events → the split is a full partition"
        );
    }

    #[test]
    fn test_breadth_split_zero_change_event_is_ungraded_not_narrow() {
        // A 0-file outcome is an explicit third value (Day 144): it belongs to
        // neither bucket, so the sum is strictly less than failure_samples.
        let events = vec![breadth_event(149, 0, 0), breadth_event(149, 1, 2)];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.failure_samples, 2);
        assert_eq!(stats.failure_narrow_samples, 1);
        assert_eq!(stats.failure_broad_samples, 0);
        assert!(
            stats.failure_narrow_samples + stats.failure_broad_samples < stats.failure_samples,
            "the zero-change event must be in neither bucket"
        );
    }

    #[test]
    fn test_breadth_split_empty_side_is_none_not_zero() {
        // Absence is not a score: an empty side must be None, never Some(0.0)
        // — even when the populated side genuinely scored 0%.
        let events = vec![breadth_event(149, 0, 9)];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.failure_narrow_samples, 0);
        assert!(
            stats.failure_narrow_hit_rate_pct.is_none(),
            "empty narrow side must be None, got {:?}",
            stats.failure_narrow_hit_rate_pct
        );
        assert_eq!(stats.failure_broad_samples, 1);
        assert_eq!(stats.failure_broad_hit_rate_pct, Some(0.0));
    }

    #[test]
    fn test_breadth_split_green_days_excluded() {
        // Green-day events grade the opposite polarity (Day 142) — they must
        // not leak into either breadth bucket.
        let mut green = breadth_event(149, 1, 2);
        green.severity = Some("watch_success".to_string());
        let events = vec![green, breadth_event(149, 2, 2)];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.failure_samples, 1);
        assert_eq!(stats.failure_narrow_samples, 1);
        assert_eq!(stats.failure_broad_samples, 0);
        assert!(stats.failure_broad_hit_rate_pct.is_none());
    }

    #[test]
    fn test_breadth_split_pooling_matches_whole_failure_rate() {
        // Both sides use the same pooled hits/changed method as
        // failure_hit_rate_pct, so a single-bucket set must agree with it.
        let events = vec![breadth_event(148, 7, 31), breadth_event(148, 3, 9)];
        let stats = compute_accuracy_stats(&events);
        let whole = stats.failure_hit_rate_pct.expect("failure events exist");
        let broad = stats.failure_broad_hit_rate_pct.expect("all events broad");
        assert!(
            (whole - broad).abs() < 1e-9,
            "all-broad set: whole {whole} vs broad {broad}"
        );
        assert!(stats.failure_narrow_hit_rate_pct.is_none());
    }

    #[test]
    fn test_breadth_report_renders_both_sides_and_caveat() {
        let events = vec![
            breadth_event(148, 1, 1),
            breadth_event(148, 7, 31),
            breadth_event(149, 1, 3),
        ];
        let stats = compute_accuracy_stats(&events);
        let report = format_accuracy_report(&stats);
        assert!(report.contains("narrow outcomes"), "report: {report}");
        assert!(report.contains("broad outcomes"), "report: {report}");
        assert!(
            report.contains("near-unpredictable by construction"),
            "caveat must be printed when both sides have events: {report}"
        );
    }

    #[test]
    fn test_breadth_report_says_no_events_instead_of_zero_pct() {
        // One-sided data: the empty side must read as absent, not as 0.0%.
        let events = vec![breadth_event(148, 0, 31)];
        let stats = compute_accuracy_stats(&events);
        let report = format_accuracy_report(&stats);
        assert!(
            report.contains("no narrow failure-day events yet"),
            "report: {report}"
        );
        assert!(
            !report.contains("near-unpredictable by construction"),
            "caveat needs both sides populated: {report}"
        );
    }
}
