//! Epistemic ranking for `/risk` — rank files by how little the graded
//! validation outcomes have taught the risk model about them.
//!
//! This is the dream's epistemic-appetite milestone (DREAM.md, Day 140),
//! ranking half only: a file is high-epistemic-value (the model is blind
//! about it) when it has been *predicted* (reactive `top_10` or anticipatory
//! `emerging`) but never *graded* (never appeared in any validation event's
//! outcome, neither as hit nor surprise), or — at lower weight — when it went
//! stale (last seen many snapshots ago with no graded event since). Study
//! history from the experiment ledger discounts both.
//!
//! A third signal used to live here: "the reactive and anticipatory columns
//! disagree about this file". It was removed on Day 163 (#726) as the last
//! live consumer of the `emerging` column deleted in #724 — see that module's
//! ledger for the measurement.
//!
//! Steering the self-driven planner slot at this ranking is a named
//! follow-up, not part of this module.

use crate::commands_risk_snapshots::{GradedEvent, ParsedSnapshot};
use crate::format::{BOLD, CYAN, DIM, RESET, YELLOW};

// The never-forecast half of this view lives in its own module since Day 166
// (extracted to make room under the module-size gate; no behaviour change).
// Re-exported here so every existing call site — including the
// `commands_risk` chain — is unchanged.
pub(crate) use crate::commands_risk_neverforecast::{
    git_added_ts, never_forecast_files, NeverForecastGroups, MIN_FORECAST_OPPORTUNITIES,
    NEVER_FORECAST_SAMPLE,
};

/// Weight for "predicted but never graded" — the strongest blindness signal:
/// the model has made claims about this file that no outcome ever tested.
pub(crate) const W_NEVER_GRADED: f64 = 2.0;

/// Lower weight for staleness: last seen ≥ [`STALE_SNAPSHOT_GAP`] snapshots
/// ago with no graded event since.
pub(crate) const W_STALE: f64 = 0.5;

/// A file last seen this many snapshots ago (or more) counts as stale.
pub(crate) const STALE_SNAPSHOT_GAP: usize = 5;

/// Saturating ceiling of the staleness contribution — see [`stale_weight`].
///
/// **Judgment threshold, not a measurement**, in the same spirit as
/// `MIN_FORECAST_OPPORTUNITIES` and `NARROW_OUTCOME_MAX`. It is set equal to
/// [`W_NEVER_GRADED`] deliberately: maximal staleness may *match* but never
/// *outrank* "predicted and never graded at all", because those are different
/// facts and the second is the darker one — a file the model has claimed
/// things about that no outcome ever tested is blinder than one that was
/// graded once, long ago.
pub(crate) const W_STALE_MAX: f64 = 2.0;

/// Excess (snapshots past [`STALE_SNAPSHOT_GAP`]) at which the staleness
/// contribution has closed half the distance from [`W_STALE`] to
/// [`W_STALE_MAX`].
///
/// **Judgment threshold, not a measurement.** 50 was chosen so that the live
/// spread observed on Day 171 (files last seen 6, 49 and 161 snapshots ago)
/// lands on visibly different scores at the one-decimal precision the report
/// prints, without any single ancient file running away with the ranking.
const STALE_HALF_SATURATION: f64 = 50.0;

/// Staleness contribution for a file last seen `snapshots_ago` snapshots ago.
///
/// Why this is magnitude-scaled rather than a binary step (Day 171): once
/// #726 removed the `reactive/emerging` disagreement signal — correctly, it
/// was measured at 0% recall — `W_STALE` was the only signal still firing for
/// files that *had* been graded, and it fires identically for every one of
/// them. The live report returned ten entries all scoring exactly 0.5, so the
/// score column did no work and the entire visible order came from the
/// tie-break, which is the *reactive* risk model's ordering — precisely the
/// attention this ranking exists to correct. The information was already in
/// the data and already printed in the reason line: 6 snapshots ago and 161
/// snapshots ago are not the same amount of dark.
///
/// Shape: `0.0` below the threshold, exactly [`W_STALE`] *at* the threshold
/// (so the previous floor is preserved), then a hyperbolic approach to
/// [`W_STALE_MAX`] that saturates rather than running away. Monotone
/// non-decreasing and bounded for every input, including `usize::MAX`.
pub(crate) fn stale_weight(snapshots_ago: usize) -> f64 {
    if snapshots_ago < STALE_SNAPSHOT_GAP {
        return 0.0;
    }
    let excess = (snapshots_ago - STALE_SNAPSHOT_GAP) as f64;
    // 1 - 1/(1 + excess/half) is 0 at excess 0 and approaches 1 from below.
    let approach = 1.0 - 1.0 / (1.0 + excess / STALE_HALF_SATURATION);
    let w = W_STALE + (W_STALE_MAX - W_STALE) * approach;
    // Belt and braces: never exceed the documented ceiling, never emit NaN.
    if w.is_nan() {
        W_STALE
    } else {
        w.min(W_STALE_MAX)
    }
}

/// Negative weight applied when a file has ≥1 **graded** entry in the
/// experiment ledger (`dreams/experiments.jsonl`): I deliberately aimed a
/// session at this file, committed a guess about it, and graded that guess.
/// That is real study, so the file is less blind than an untouched one — but
/// it is *not* a graded validation event, so it only discounts (half of
/// [`W_NEVER_GRADED`]) instead of clearing the never-graded signal.
///
/// Without this, the ranking could never converge: it would keep pointing the
/// planner at files I had already studied, because study left no trace it
/// could read.
pub(crate) const W_RECENTLY_STUDIED: f64 = -1.0;

/// Negative weight applied when an experiment round *named* a file but
/// produced no grade of any kind — no summary `graded`, no per-hypothesis
/// grade. The round happened; it just taught nothing measurable.
///
/// Half of [`W_RECENTLY_STUDIED`], and the number is a judgment call, not a
/// measurement: a visit is weaker evidence that I learned something than a
/// graded round (nothing was scored, so nothing was falsifiable), but it is
/// strictly more than never having looked — I have read the file, its
/// neighbours, and formed whatever unrecorded impression a session leaves.
/// Ranking a visited file *above* a graded one but *below* a never-studied
/// one is the ordering this encodes; if grading discipline improves and these
/// rounds stop happening, the const stops mattering on its own.
pub(crate) const W_VISITED_UNGRADED: f64 = -0.5;

/// Negative weight applied when a graded round **declared its own scope** —
/// the round says in the ledger (`scope_limit`) that it studied only part of
/// the file.
///
/// A **judgment threshold, not a measurement**: nothing measured says a
/// self-declared partial round is worth exactly three quarters of a full one.
/// It sits between [`W_VISITED_UNGRADED`] (-0.5) and [`W_RECENTLY_STUDIED`]
/// (-1.0) because a partial round *did* grade something — more than a visit
/// that scored nothing — while leaving named regions of the file unread.
/// The steering is done by [`StudyTier::PartiallyStudied`]; this weight only
/// orders files *within* that group, exactly as its two siblings do.
pub(crate) const W_PARTIALLY_STUDIED: f64 = -0.75;

/// Two epistemic scores within this distance count as tied and fall through
/// to the tie-break: current risk score (higher first), then path.
pub(crate) const SCORE_EPSILON: f64 = 1e-6;

/// Append-only ledger of chosen experiments and their grades, written by the
/// dream/evolve loop. Read-only from here.
pub(crate) const EXPERIMENT_LEDGER_PATH: &str = "dreams/experiments.jsonl";

/// What an experiment round left behind about one file. Two *stated* values
/// plus a third that is the absence of any entry at all (never studied) —
/// #711: "visited" and "graded" used to be one fact, so a round whose summary
/// `graded` field was null looked exactly like a round that never happened,
/// and the ranking sent the planner back to files it had already spent a
/// session on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StudyState {
    /// The round has grade evidence: either a non-empty summary `graded`
    /// field, or ≥1 `hypothesis_grades` record carrying its own grade. The
    /// payload is the grade summary — verbatim when the summary field held
    /// one, derived from the per-hypothesis records when it did not.
    Graded(String),
    /// The round has grade evidence **and** declared its own `scope_limit` in
    /// the ledger: it says out loud that it studied only part of the file.
    ///
    /// #839: blind round 82 studied three named functions of `src/safety.rs`
    /// and moved that 3559-line file from the darkest tier to the lightest in
    /// one step — ~3400 unread lines bought credit. The round was complete and
    /// honest (it passes `tests/blind_round_grades.rs` exactly as a whole-file
    /// study would), so there is no half-written state to detect; the bias is
    /// simply that the bigger the file, the less of it a round covers.
    ///
    /// **A declared scope is a claim the round made about itself, not a
    /// measurement of coverage.** A round that declares no scope is still
    /// credited whole-file, so this makes the over-crediting *nameable*, not
    /// impossible.
    PartiallyGraded {
        /// Grade summary, same derivation as [`StudyState::Graded`].
        summary: String,
        /// The round's own `scope_limit` text, verbatim.
        scope: String,
    },
    /// A round named this file but recorded no grade anywhere. The expedition
    /// happened; it produced nothing falsifiable.
    VisitedUngraded,
}

/// Primary ordering key for the epistemic ranking. Lower = darker = ranks
/// first.
///
/// This is the **predicate form** of the two study weights
/// ([`W_RECENTLY_STUDIED`], [`W_VISITED_UNGRADED`]), which still exist and
/// still apply — they now only order files *within* a tier. The weights alone
/// could not do this job: the ranking's real consumer is a top-N *selector*
/// (`EPISTEMIC_TOP_N` in `scripts/extract_trajectory.py`), and a −1.0 discount
/// does not move a file scoring +1.0 (never graded) +1.0 (stale) off the top,
/// so the planner was handed the rooms I had most recently lit. A selector
/// needs a predicate, not a nudge.
///
/// The variant order **is** the sort order and is pinned by a test: reordering
/// these variants silently reorders the ranking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StudyTier {
    /// No experiment round has ever named this file.
    NeverStudied,
    /// A graded round named it and **declared its own scope**: it says in the
    /// ledger that it covered only part of the file.
    ///
    /// Ordered **darker than [`StudyTier::VisitedUngraded`]**, which is the
    /// load-bearing judgment here and is deliberate rather than incidental: a
    /// partial round has *proved* the file contains regions nobody entered,
    /// while an ungraded visit at least aimed at the whole file and merely
    /// failed to score anything. The partial round therefore leaves a *known*
    /// dark remainder; the visit leaves an unknown one that may be empty.
    /// (The opposite ordering is arguable — a graded partial did learn
    /// something measurable where a visit did not — but that argument is about
    /// what the round *taught*, and this tier ranks by what is still *unread*.)
    PartiallyStudied,
    /// A round named it but recorded no grade — the expedition happened and
    /// produced nothing falsifiable, so it is darker than a graded round but
    /// lighter than never having looked.
    VisitedUngraded,
    /// A round named it and graded something. Direct evidence about the file
    /// that the validation ledger cannot supply.
    Graded,
}

/// Map a file's latest study state onto its ordering tier. Absence
/// ([`None`]) is the darkest value and gets its own name rather than being
/// folded into the neighbouring one.
pub(crate) fn study_tier(state: Option<&StudyState>) -> StudyTier {
    match state {
        None => StudyTier::NeverStudied,
        Some(StudyState::VisitedUngraded) => StudyTier::VisitedUngraded,
        Some(StudyState::PartiallyGraded { .. }) => StudyTier::PartiallyStudied,
        Some(StudyState::Graded(_)) => StudyTier::Graded,
    }
}

/// Group header for one tier in the report.
///
/// Two constraints checked by tests, not by eye: no header may contain the
/// substring `never forecast` (`extract_trajectory.py` hard-stops entry
/// collection on it), and none may match that script's numbered-entry regex.
pub(crate) fn study_tier_header(tier: StudyTier) -> &'static str {
    match tier {
        StudyTier::NeverStudied => "dark — no deliberate study on record",
        StudyTier::PartiallyStudied => {
            "partially studied — a graded round covered only part of the file"
        }
        StudyTier::VisitedUngraded => "visited by an ungraded round",
        StudyTier::Graded => {
            "already studied by a graded round (shown for completeness, ranked last)"
        }
    }
}

/// One experiment-ledger visit to a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExperimentVisit {
    pub(crate) path: String,
    pub(crate) day: u32,
    pub(crate) state: StudyState,
}

/// Count the per-hypothesis records that carry a grade of their own.
///
/// A `hypothesis_grades` entry whose own `graded` field is null/absent is not
/// grade evidence — it is a hypothesis that was *listed*, not scored. Same
/// discipline as the summary field: absence is never counted as a zero.
fn graded_hypothesis_count(val: &serde_json::Value) -> usize {
    match val["hypothesis_grades"].as_array() {
        Some(items) => items
            .iter()
            .filter(|h| h["graded"].as_str().is_some_and(|g| !g.trim().is_empty()))
            .count(),
        None => 0,
    }
}

/// Parse the experiment ledger into per-line visits, each carrying its own
/// [`StudyState`].
///
/// Grade evidence is read from **both** places a round can record it:
/// - a non-empty summary `graded` string (the original source), or
/// - ≥1 `hypothesis_grades` record with its own grade — the per-hypothesis
///   records are the source of truth when the summary was left null. Measured
///   on Day 163 against `dreams/experiments.jsonl` (a snapshot of a growing
///   ledger, not an invariant — re-count before quoting it): 21
///   `experiment_result` lines, 9 with a null/empty summary `graded`, 8 of
///   those 9 carrying real per-hypothesis grades, so the round *was* graded
///   and only the summary field was empty; 1 genuinely ungraded. The same
///   9/8/1 split held at 20 result lines earlier the same day (#718 — the
///   figures first written here, "7 of 20, 6 of those 7", were wrong against
///   the very commit that introduced them).
///
/// Defensive, like every other JSONL reader here:
/// - a line with no grade evidence anywhere is [`StudyState::VisitedUngraded`],
///   not a skip: the round happened, it just taught nothing measurable.
/// - a line with no usable `target` is skipped — there is nothing to attribute
///   it to.
/// - malformed / non-JSON lines are skipped, never fatal.
/// - an absent file yields an empty vec at the call site — "I have run no
///   experiments" is an honest state, not a defect.
///
/// Both `experiment` (the guess) and `experiment_result` (the grading) lines
/// count as visits; the merge in [`compute_epistemic_ranking`] decides which
/// state wins for a file that has several.
pub(crate) fn parse_experiment_visits(contents: &str) -> Vec<ExperimentVisit> {
    let mut out = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let val: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match val["type"].as_str() {
            Some("experiment") | Some("experiment_result") => {}
            _ => continue,
        }
        let path = match val["target"].as_str() {
            Some(p) if !p.trim().is_empty() => p.trim().to_string(),
            _ => continue,
        };
        let day = val["day"].as_u64().unwrap_or(0) as u32;
        // A round's own declared scope. **`None` means UNSCOPED, never
        // "partial"**: every ledger line written before #839 carries no
        // `scope_limit`, and those lines must keep meaning exactly what they
        // meant before — credited whole-file. The ledger is never back-filled.
        let scope = match val["scope_limit"].as_str() {
            Some(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            _ => None,
        };
        let summary = match val["graded"].as_str() {
            Some(g) if !g.trim().is_empty() => Some(g.trim().to_string()),
            _ => match graded_hypothesis_count(&val) {
                0 => None,
                n => Some(format!("{n} hypotheses graded")),
            },
        };
        // Grade evidence with no declared scope stays `Graded`, byte-identical
        // to before. A scope on an *ungraded* line is deliberately dropped:
        // the round scored nothing, so its coverage claim credits nothing yet.
        let state = match (summary, scope) {
            (Some(summary), Some(scope)) => StudyState::PartiallyGraded { summary, scope },
            (Some(summary), None) => StudyState::Graded(summary),
            (None, _) => StudyState::VisitedUngraded,
        };
        out.push(ExperimentVisit { path, day, state });
    }
    out
}

// The hypothesis-family tally (archive vs file-specific vs genre-prior) lives in
// its own module since Day 174 — this file was 2 lines past the fatal
// `MAX_MODULE_LINES` gate. Re-exported so every existing call site keeps naming
// it through this module.
pub(crate) use crate::commands_risk_families::{
    format_experiment_families, tally_hypothesis_families, ExperimentFamilies,
};

/// Default number of entries shown in the report.
const REPORT_TOP_N: usize = 10;

/// Max characters of a single reason bullet in the human report, ellipsis
/// included. A **judgment threshold** for readability, not a measurement:
/// summaries have no natural length bound (one live reason ran ~1,600 chars).
/// Sibling cap: `EPISTEMIC_ENTRY_MAX_CHARS = 90` in
/// `scripts/extract_trajectory.py`, which clamps this same text for the
/// *planner* view — the two numbers are independent by construction and this
/// comment is the only link that will ever exist between them (Day 159).
/// Display-only: nothing here changes a score, a grade, or a persisted field.
pub(crate) const REASON_MAX_CHARS: usize = 140;

/// Shorten one reason bullet for display. Returns the text to print and
/// whether anything was **cut** (whitespace flattening is not a cut).
///
/// Rules, all deliberate:
/// - a reason is one line: embedded newlines/tabs/CRs collapse to spaces first;
/// - the cut is on a **character** boundary, never a byte index (`truncate`
///   panics mid-UTF-8 — CLAUDE.md rule);
/// - `max_chars` counts the FINAL string including the ellipsis, so the output
///   never exceeds the cap;
/// - if the reason ends with `)`, the closing paren is preserved after the
///   ellipsis. That is the consumer contract, not cosmetics:
///   `EPISTEMIC_STUDIED_RE` in `scripts/extract_trajectory.py` is
///   `studied by graded experiment \(day (\d+), ([^)]+)\)` and stops matching
///   the moment the paren disappears — the planner's blind-spot section would
///   silently degrade to raw text with nobody noticing.
pub(crate) fn truncate_reason(reason: &str, max_chars: usize) -> (String, bool) {
    // A reason is one line: any embedded newline/tab/other whitespace becomes
    // a plain space so a multi-line summary can't break the numbered list.
    let flat: String = reason
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    if flat.chars().count() <= max_chars {
        return (flat, false);
    }
    let suffix = if flat.ends_with(')') { "…)" } else { "…" };
    let suffix_len = suffix.chars().count();
    if max_chars <= suffix_len {
        // No room for content. `max_chars == 0` is an explicit case, not an
        // accident: it yields the empty string, still marked as cut.
        return (suffix.chars().take(max_chars).collect(), true);
    }
    let mut out: String = flat.chars().take(max_chars - suffix_len).collect();
    out.push_str(suffix);
    (out, true)
}

/// One file the graded outcomes have taught the model little about.
#[derive(Debug)]
pub(crate) struct EpistemicEntry {
    pub(crate) path: String,
    pub(crate) score: f64,
    /// Human-readable reasons, e.g. "predicted 5×, never graded".
    pub(crate) reasons: Vec<String>,
    /// Current risk score, used only to break epistemic-score ties.
    /// `None` is the explicit abstention case: the risk model has no score
    /// for this file — it sorts after scored files within a tie.
    pub(crate) risk_score: Option<f64>,
    /// Study tier — the **primary** sort key, ahead of `score`. See
    /// [`StudyTier`].
    pub(crate) tier: StudyTier,
}

/// Per-file aggregate built while scanning snapshots.
#[derive(Default)]
struct FileStats {
    predicted_count: usize,
    emerging_count: usize,
    /// Index into `snapshots` of the last snapshot mentioning this file.
    last_seen_index: usize,
    /// `day` of that snapshot (used to check "graded since").
    last_seen_day: u64,
}

/// Latest experiment visit per file, by the one precedence rule both halves of
/// this view share: a **graded** round outranks a bare visit whatever the days
/// say (a grade once earned is not erased by a later expedition that recorded
/// nothing), and within the same state the latest day wins.
///
/// Extracted Day 166 (#744) so the ranked half and the never-forecast half can
/// never disagree about what "already studied" means — one statement of the
/// rule, two consumers.
/// Rank a study state for the precedence rule in
/// [`latest_study_state_by_path`]. Higher wins outright, regardless of day.
///
/// The order is the one argued at [`StudyTier`], read the other way up: a
/// **full** (unscoped) graded round is the strongest evidence and is never
/// erased by a later narrow one; a **partial** graded round outranks a bare
/// visit because it scored something; a visit is the weakest.
fn study_state_rank(state: &StudyState) -> u8 {
    match state {
        StudyState::VisitedUngraded => 0,
        StudyState::PartiallyGraded { .. } => 1,
        StudyState::Graded(_) => 2,
    }
}

/// Latest experiment visit per file, by the one precedence rule both halves of
/// this view share:
///
/// - a **full** (unscoped) graded round outranks a **partial** one whatever
///   the days say — a whole-file study is not erased by a later narrow one;
/// - a **partial** graded round outranks a bare visit whatever the days say;
/// - within the *same* state the latest day wins, so a partial round is
///   replaced by a later partial rather than being downgraded;
/// - a grade once earned is not erased by a later expedition that recorded
///   nothing.
///
/// Extracted Day 166 (#744) so the ranked half and the never-forecast half can
/// never disagree about what "already studied" means — one statement of the
/// rule, two consumers. Do not re-derive it in `never_forecast_files`.
pub(crate) fn latest_study_state_by_path(
    experiments: &[ExperimentVisit],
) -> std::collections::HashMap<&str, (&StudyState, u32)> {
    use std::collections::HashMap;
    let mut studied: HashMap<&str, (&StudyState, u32)> = HashMap::new();
    for visit in experiments {
        let candidate = (&visit.state, visit.day);
        match studied.get(visit.path.as_str()) {
            None => {
                studied.insert(visit.path.as_str(), candidate);
            }
            Some((state, day)) => {
                let replace = match study_state_rank(&visit.state).cmp(&study_state_rank(state)) {
                    std::cmp::Ordering::Greater => true,
                    std::cmp::Ordering::Less => false,
                    std::cmp::Ordering::Equal => visit.day >= *day,
                };
                if replace {
                    studied.insert(visit.path.as_str(), candidate);
                }
            }
        }
    }
    studied
}

/// Rank files by epistemic value — how little the graded outcomes have
/// taught the model about them. Pure: reads only its arguments.
/// `risk_scores` (path → current risk score, as returned by
/// `commands_risk::top_risk_files`) is used only to break ties.
///
/// Files whose every signal is satisfied (graded, columns agree, fresh)
/// score 0 and are absent from the result — the model has nothing left to
/// learn about them from this view.
pub(crate) fn compute_epistemic_ranking(
    snapshots: &[ParsedSnapshot],
    events: &[GradedEvent],
    risk_scores: &[(String, f64)],
    experiments: &[ExperimentVisit],
) -> Vec<EpistemicEntry> {
    use std::collections::{HashMap, HashSet};

    if snapshots.is_empty() {
        return Vec::new();
    }

    // Tie-break lookup: path → current risk score.
    let score_by_path: HashMap<&str, f64> =
        risk_scores.iter().map(|(p, s)| (p.as_str(), *s)).collect();

    // Every path that ever appeared in a graded outcome, with the latest
    // day it was graded (hits and surprises both count — each taught the
    // model something).
    let mut graded_latest_day: HashMap<&str, u64> = HashMap::new();
    for ev in events {
        for p in &ev.paths {
            let entry = graded_latest_day.entry(p.as_str()).or_insert(0);
            *entry = (*entry).max(ev.day);
        }
    }

    // Aggregate per-file stats across all snapshots.
    let mut stats: HashMap<String, FileStats> = HashMap::new();
    for (idx, snap) in snapshots.iter().enumerate() {
        let mut seen: HashSet<&str> = HashSet::new();
        for p in &snap.predicted {
            let s = stats.entry(p.clone()).or_default();
            s.predicted_count += 1;
            seen.insert(p.as_str());
        }
        for p in &snap.emerging {
            let s = stats.entry(p.clone()).or_default();
            s.emerging_count += 1;
            seen.insert(p.as_str());
        }
        for p in seen {
            let s = stats.get_mut(p).expect("just inserted");
            s.last_seen_index = idx;
            s.last_seen_day = snap.day;
        }
    }

    let last_index = snapshots.len() - 1;
    // Latest experiment visit per file. This is study history, NOT grading
    // history: it says "I read this file on purpose", never "the risk model
    // was measured on it". Kept in its own map and rendered as its own reason
    // so the two can never be conflated.
    //
    // Merge rule (#711): a *graded* round outranks a *visited-ungraded* one
    // whatever the days say — a grade once earned is not erased by a later
    // expedition that recorded nothing. Within the same state, the latest day
    // wins.
    let studied = latest_study_state_by_path(experiments);

    let mut entries: Vec<EpistemicEntry> = Vec::new();
    for (path, s) in &stats {
        let mut score = 0.0;
        let mut reasons = Vec::new();

        let appearances = s.predicted_count + s.emerging_count;
        let graded_day = graded_latest_day.get(path.as_str()).copied();

        if graded_day.is_none() {
            score += W_NEVER_GRADED;
            reasons.push(format!("predicted {appearances}×, never graded"));
        }

        let snapshots_ago = last_index - s.last_seen_index;
        let graded_since = graded_day.is_some_and(|d| d >= s.last_seen_day);
        if snapshots_ago >= STALE_SNAPSHOT_GAP && !graded_since {
            // Magnitude-scaled since Day 171: the guard is unchanged (a file
            // with a graded event since is not stale), but *how* stale now
            // depends on how long it has been unobserved.
            score += stale_weight(snapshots_ago);
            reasons.push(format!(
                "last seen {snapshots_ago} snapshots ago, no graded event since"
            ));
        }

        // Study discount. Deliberately additive to the reason list, never a
        // replacement: if the validation ledger still says "never graded",
        // that line stays. Both facts are true at once and both get said.
        //
        // The two study states render as two *distinct* reasons (#711) — a
        // visit that produced no grade must never wear the word "graded".
        let study_state = studied.get(path.as_str()).map(|(state, _)| *state);
        if let Some((state, day)) = studied.get(path.as_str()) {
            match state {
                StudyState::Graded(grade) => {
                    score += W_RECENTLY_STUDIED;
                    reasons.push(format!("studied by graded experiment (day {day}, {grade})"));
                }
                StudyState::VisitedUngraded => {
                    score += W_VISITED_UNGRADED;
                    reasons.push(format!("visited by ungraded experiment (day {day})"));
                }
                // Deliberately worded so it does NOT contain the substring
                // `studied by graded experiment (day N, …)`:
                // `EPISTEMIC_STUDIED_RE` in `scripts/extract_trajectory.py`
                // uses `.search()`, so a reason carrying that phrase would be
                // compacted for the planner as a *full* study — dropping the
                // word "partial" and the scope, i.e. the exact over-crediting
                // this whole state exists to name. It falls through to that
                // script's `reason[:60]` raw path instead; giving it a proper
                // compaction prefix is a separate task (#839 leaves it
                // uncompacted on purpose). Day and grade are front-loaded so
                // they survive that 60-char cut.
                StudyState::PartiallyGraded { summary, scope } => {
                    score += W_PARTIALLY_STUDIED;
                    reasons.push(format!(
                        "partial study (day {day}, {summary}) — scope: {scope}"
                    ));
                }
            }
        }

        if score > 0.0 {
            entries.push(EpistemicEntry {
                path: path.clone(),
                score,
                reasons,
                risk_score: score_by_path.get(path.as_str()).copied(),
                tier: study_tier(study_state),
            });
        }
    }

    // Highest epistemic value first. Scores within SCORE_EPSILON are tied
    // and fall through to the current risk score (higher first) — a flat
    // ranking steers the planner arbitrarily; risk makes the choice
    // meaningful. A file the risk model has no score for (explicit
    // abstention, `risk_score: None`) sorts after scored files within a
    // tie; final fallback is path, for full determinism.
    entries.sort_by(|a, b| {
        use std::cmp::Ordering;
        // Study tier first — the predicate the top-N selector needs. A file
        // already studied by a graded round ranks below every unstudied one
        // whatever its score; the score orders within a tier.
        match a.tier.cmp(&b.tier) {
            Ordering::Equal => {}
            other => return other,
        }
        if (a.score - b.score).abs() > SCORE_EPSILON {
            return b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal);
        }
        match (a.risk_score, b.risk_score) {
            (Some(ra), Some(rb)) => rb
                .partial_cmp(&ra)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.path.cmp(&b.path)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a.path.cmp(&b.path),
        }
    });
    entries
}

/// Format the epistemic ranking as a report. Honest empty states — never a
/// silent nothing.
pub(crate) fn format_epistemic_report(
    snapshots: &[ParsedSnapshot],
    entries: &[EpistemicEntry],
    never: &NeverForecastGroups,
    families: &ExperimentFamilies,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "\n{BOLD}{CYAN}🔍 Epistemic view — where graded outcomes have taught the model least{RESET}\n\n"
    ));

    if snapshots.is_empty() {
        out.push_str(&format!(
            "  {DIM}no snapshots yet — run `yoyo risk snapshot` first{RESET}\n"
        ));
        // The experiment record doesn't depend on snapshots — it is my own
        // guess-first history, and it is worth saying even before the first
        // snapshot exists.
        out.push_str(&format_experiment_families(families));
        return out;
    }

    if entries.is_empty() {
        out.push_str(&format!(
            "  {DIM}no ungraded predictions — the model has been graded on everything it predicted{RESET}\n"
        ));
    } else {
        let mut truncated_reasons = 0usize;
        let mut last_tier: Option<StudyTier> = None;
        for (i, e) in entries.iter().take(REPORT_TOP_N).enumerate() {
            // Group header, emitted only when a group actually appears in the
            // shown slice. Deliberately carries no leading "N." so it cannot
            // match the planner's entry regex, and no "never forecast"
            // substring, which is that parser's hard stop for this half.
            if last_tier != Some(e.tier) {
                out.push_str(&format!("  {DIM}{}{RESET}\n", study_tier_header(e.tier)));
                last_tier = Some(e.tier);
            }
            out.push_str(&format!(
                "  {:>2}. {YELLOW}{:<40}{RESET} {:.1}\n",
                i + 1,
                e.path,
                e.score
            ));
            for r in &e.reasons {
                let (text, was_cut) = truncate_reason(r, REASON_MAX_CHARS);
                if was_cut {
                    truncated_reasons += 1;
                }
                out.push_str(&format!("      {DIM}• {text}{RESET}\n"));
            }
        }
        // Honest ordering note: if any displayed neighbours are tied (within
        // SCORE_EPSILON), say so — the order among them came from the tie-break,
        // not the epistemic score.
        let shown = &entries[..entries.len().min(REPORT_TOP_N)];
        let has_tie = shown
            .windows(2)
            .any(|w| (w[0].score - w[1].score).abs() <= SCORE_EPSILON);
        if has_tie {
            out.push_str(&format!(
                "\n  {DIM}note: tied scores are ordered by current risk score (higher first), then path{RESET}\n"
            ));
        }
        out.push_str(&format!(
            "\n  {DIM}ordered by (study tier, score): a deliberate study round is direct evidence\n  about a file that the validation ledger cannot supply, so a studied file ranks below\n  every unstudied one regardless of score. That is a judgment call, not a measurement —\n  within a group, higher score still means the model is blinder.{RESET}\n"
        ));
        // Mark my own elisions in-band (Day 162): a silent cut is a lie of
        // omission. Absent entirely when nothing was shortened.
        if truncated_reasons > 0 {
            out.push_str(&format!(
                "  {DIM}({} reason{} shortened for display — full experiment summaries live in dreams/experiments.jsonl){RESET}\n",
                truncated_reasons,
                if truncated_reasons == 1 { "" } else { "s" }
            ));
        }
    }

    // The unknown unknowns, kept as an explicit separate section — merging
    // them into the ranked list above would flatten it (a list where
    // everything is equally unknown ranks nothing).
    //
    // Consumer note: rows deliberately do NOT take the `N. path score` shape
    // that `scripts/extract_trajectory.py::EPISTEMIC_ENTRY_RE` matches, and
    // use the `◦` glyph rather than the `•` that parser appends to the
    // previous ranked entry's reasons. The chosen guard is on the parser
    // side: it stops collecting at the "never forecast" header line (see
    // `EPISTEMIC_NEVER_FORECAST_RE` there). The distinct glyph is belt-and-
    // braces for any other reader.
    let dark = &never.dark;
    if !dark.is_empty() {
        out.push_str(&format!(
            "\n  {YELLOW}⚠ never forecast{RESET} {DIM}— {} scored file{} {} never appeared in any prediction{RESET}\n",
            dark.len(),
            if dark.len() == 1 { "" } else { "s" },
            if dark.len() == 1 { "has" } else { "have" },
        ));
        for n in dark.iter().take(NEVER_FORECAST_SAMPLE) {
            out.push_str(&format!(
                "  ◦ {YELLOW}{}{RESET} {DIM}(risk {:.1}){RESET}\n",
                n.path, n.risk_score
            ));
        }
        if dark.len() > NEVER_FORECAST_SAMPLE {
            out.push_str(&format!(
                "    {DIM}... (+{} more){RESET}\n",
                dark.len() - NEVER_FORECAST_SAMPLE
            ));
        }
        // Fourth reported state (#819): not a group — a count. Every one of
        // these files is *in* the list above; what could not be observed is
        // its age. Emitted only when > 0, so a fully observable run is
        // byte-identical to before. Deliberately avoids the literal substring
        // the trajectory extractor hard-stops on, and matches neither its row
        // regex (no leading `◦`) nor its numbered-entry regex.
        if never.age_unobservable > 0 {
            out.push_str(&format!(
                "    {DIM}age unobservable for {} of these — a shallow clone or a path git does not know hides the add date.{RESET}\n",
                never.age_unobservable
            ));
            out.push_str(&format!(
                "    {DIM}For those, absence from every column is not evidence the columns ever had a chance.{RESET}\n"
            ));
        }
        // Keep each sentence on one line: the caveat is the honest part of this
        // section, and a mid-sentence wrap makes it unmatchable for any reader
        // (test or human) scanning line by line.
        out.push_str(&format!(
            "    {DIM}the ranking above cannot see these — it is built from files I once guessed about.{RESET}\n"
        ));
        out.push_str(&format!(
            "    {DIM}Files with no recent churn have no risk score and are invisible to both views.{RESET}\n"
        ));
    }

    // Third state (#744): never forecast, but *already studied*. Surfaced
    // separately and annotated — never silently subtracted from the dark set
    // above (that would destroy the "no column ever named it" fact) and never
    // presented as unexplored (that spends the exploration budget re-lighting
    // a room I lit myself). The rows deliberately use the `▪ path — note`
    // shape, which matches neither `EPISTEMIC_NEVER_FORECAST_ROW_RE`
    // (`^\s*◦\s+(\S+)\s+\(risk\s`) nor `EPISTEMIC_ENTRY_RE` in
    // scripts/extract_trajectory.py, so the planner's dark-room list keeps
    // meaning exactly what it meant before.
    if !never.studied.is_empty() {
        out.push_str(&format!(
            "\n  {DIM}already studied, though no column ever named {}: {} scored file{}{RESET}\n",
            if never.studied.len() == 1 {
                "it"
            } else {
                "them"
            },
            never.studied.len(),
            if never.studied.len() == 1 { "" } else { "s" },
        ));
        for s in never.studied.iter().take(NEVER_FORECAST_SAMPLE) {
            out.push_str(&format!(
                "  ▪ {}{RESET} {DIM}(risk {:.1}) — {}{RESET}\n",
                s.path, s.risk_score, s.note
            ));
        }
        if never.studied.len() > NEVER_FORECAST_SAMPLE {
            out.push_str(&format!(
                "    {DIM}... (+{} more studied){RESET}\n",
                never.studied.len() - NEVER_FORECAST_SAMPLE
            ));
        }
        out.push_str(&format!(
            "    {DIM}these are lit rooms: an experiment already opened them, so they are not the dark set.{RESET}\n"
        ));
    }

    // Fourth state, third group (sibling of #744, one layer over): never
    // forecast, but the file is younger than most of the prediction history.
    // No snapshot *could* have named a path that did not exist, so its absence
    // from every column carries zero information — reporting it as dark points
    // the exploration budget at the brightest room in the building.
    //
    // The shape here is load-bearing for `scripts/extract_trajectory.py`:
    // the header must not contain the substring "never forecast"
    // (EPISTEMIC_NEVER_FORECAST_RE hard-stops collecting on it) and the rows
    // must not start with `◦` (EPISTEMIC_NEVER_FORECAST_ROW_RE) nor with the
    // studied group's `▪`. Hence `▫`.
    if !never.too_new.is_empty() {
        out.push_str(&format!(
            "\n  {DIM}too young to judge: {} scored file{} newer than most of the prediction history{RESET}\n",
            never.too_new.len(),
            if never.too_new.len() == 1 { "" } else { "s" },
        ));
        for t in never.too_new.iter().take(NEVER_FORECAST_SAMPLE) {
            out.push_str(&format!(
                "  ▫ {}{RESET} {DIM}(risk {:.1}) — added {}, {} snapshot{} since (needs ≥{} before absence means anything){RESET}\n",
                t.path,
                t.risk_score,
                t.added,
                t.opportunities,
                if t.opportunities == 1 { "" } else { "s" },
                MIN_FORECAST_OPPORTUNITIES,
            ));
        }
        if never.too_new.len() > NEVER_FORECAST_SAMPLE {
            out.push_str(&format!(
                "    {DIM}... (+{} more too young){RESET}\n",
                never.too_new.len() - NEVER_FORECAST_SAMPLE
            ));
        }
        out.push_str(&format!(
            "    {DIM}the threshold is a judgment call, not a measurement: below it, absence is about the file's age.{RESET}\n"
        ));
    }

    // Rendered last, below the never-forecast header, so the trajectory
    // extractor (which hard-stops collecting there) can never absorb it.
    out.push_str(&format_experiment_families(families));
    out
}

/// Handle `/risk epistemic` — read the existing snapshot + validation JSONL,
/// compute the ranking, print the report.
pub(crate) fn handle_risk_epistemic() {
    use crate::commands_risk_snapshots::{
        epistemic_ledger_notes, first_scored_age, founding_ts, read_first_scored,
        read_graded_ledger, read_snapshot_ledger, GradedLedger, SnapshotLedger,
        RISK_FIRST_SCORED_PATH, RISK_SNAPSHOT_PATH, RISK_VALIDATION_PATH,
    };
    use crate::format::{RESET, YELLOW};

    // Both ledgers are read through their three-state readers rather than a
    // bare `unwrap_or_default()`: this view's headline claims ("never
    // forecast", "never graded") are computed by *subtracting* the ledgers
    // from the scored file set, so a silently-dropped line does not shrink a
    // denominator here — it manufactures a blind spot, and that list is what
    // the trajectory extractor hands the planner as "study these next".
    let snapshot_ledger = read_snapshot_ledger(std::path::Path::new(RISK_SNAPSHOT_PATH));
    let graded_ledger = read_graded_ledger(std::path::Path::new(RISK_VALIDATION_PATH));
    for note in epistemic_ledger_notes(
        RISK_SNAPSHOT_PATH,
        &snapshot_ledger,
        RISK_VALIDATION_PATH,
        &graded_ledger,
    ) {
        println!("  {YELLOW}⚠ {note}{RESET}");
    }
    let snapshots = match snapshot_ledger {
        SnapshotLedger::Present { snapshots, .. } => snapshots,
        _ => Vec::new(),
    };
    let events = match graded_ledger {
        GradedLedger::Present { events, .. } => events,
        _ => Vec::new(),
    };

    // Study history — absent ledger is an honest empty, not an error.
    let experiment_content =
        std::fs::read_to_string(std::path::Path::new(EXPERIMENT_LEDGER_PATH)).unwrap_or_default();
    let experiments = parse_experiment_visits(&experiment_content);
    // Current risk scores, used only to break epistemic ties.
    let risk_scores = crate::commands_risk::top_risk_files(usize::MAX);
    let entries = compute_epistemic_ranking(&snapshots, &events, &risk_scores, &experiments);
    // The add-date resolver is consulted only for the never-forecast
    // candidates — never once per scored file.
    //
    // Two sources, in order. First the first-scored ledger: a record of when
    // *I* first scored a path, which is a fact about my own observation
    // history and so survives a shallow clone. Then git, which in this
    // checkout answers `None` for every pre-window file (the graft boundary
    // dates them all to the clone, and `git_added_ts` correctly refuses it).
    // Today the ledger discriminates nothing — every entry is in the founding
    // batch, which `first_scored_age` deliberately reads as unknown — so this
    // is byte-identical to git alone and starts paying off once paths appear
    // *after* that batch.
    let (first_scored, _dropped) = read_first_scored(std::path::Path::new(RISK_FIRST_SCORED_PATH));
    let founding = founding_ts(&first_scored);
    let never = never_forecast_files(&snapshots, &risk_scores, &experiments, &|path| {
        first_scored_age(path, &first_scored, founding)
            .map(|ts| ts.to_string())
            .or_else(|| git_added_ts(path))
    });
    // Same ledger text, read once: study history for the ranking, per-hypothesis
    // provenance for the guess-first scoreboard.
    let families = tally_hypothesis_families(&experiment_content);
    print!(
        "{}",
        format_epistemic_report(&snapshots, &entries, &never, &families)
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands_risk_families::FamilyTally;

    fn snap(day: u64, predicted: &[&str], emerging: &[&str]) -> ParsedSnapshot {
        ParsedSnapshot {
            day,
            git_hash: format!("hash{day}"),
            ts: format!("2026-07-{:02}T00:00:00Z", (day % 28) + 1),
            predicted: predicted.iter().map(|s| s.to_string()).collect(),
            emerging: emerging.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn graded(day: u64, paths: &[&str]) -> GradedEvent {
        GradedEvent {
            day,
            paths: paths.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_never_graded_ranks_above_graded() {
        // a.rs predicted and graded; b.rs predicted, never graded.
        let snapshots = vec![snap(100, &["src/a.rs", "src/b.rs"], &[])];
        let events = vec![graded(100, &["src/a.rs"])];
        let ranking = compute_epistemic_ranking(&snapshots, &events, &[], &[]);
        let pos_b = ranking.iter().position(|e| e.path == "src/b.rs");
        let pos_a = ranking.iter().position(|e| e.path == "src/a.rs");
        assert!(pos_b.is_some(), "never-graded file must appear");
        match pos_a {
            None => {} // graded, no other signal → absent (score 0)
            Some(pa) => assert!(pos_b.unwrap() < pa, "never-graded ranks above graded"),
        }
        let b = &ranking[pos_b.unwrap()];
        assert!(
            b.reasons.iter().any(|r| r.contains("never graded")),
            "reason names the blindness: {:?}",
            b.reasons
        );
    }

    #[test]
    fn test_graded_agreeing_file_absent() {
        // a.rs in both columns (agree) and graded — nothing left to learn.
        let snapshots = vec![snap(100, &["src/a.rs"], &["src/a.rs"])];
        let events = vec![graded(100, &["src/a.rs"])];
        let ranking = compute_epistemic_ranking(&snapshots, &events, &[], &[]);
        assert!(
            !ranking.iter().any(|e| e.path == "src/a.rs"),
            "fully-graded, agreeing file must rank absent"
        );
    }

    #[test]
    fn test_empty_snapshots_honest_message() {
        let ranking = compute_epistemic_ranking(&[], &[], &[], &[]);
        assert!(ranking.is_empty());
        let report = format_epistemic_report(
            &[],
            &ranking,
            &NeverForecastGroups::default(),
            &ExperimentFamilies::default(),
        );
        assert!(
            report.contains("no snapshots yet"),
            "empty state must be honest, got: {report}"
        );
    }

    #[test]
    fn test_all_graded_honest_message() {
        let snapshots = vec![snap(100, &["src/a.rs"], &["src/a.rs"])];
        let events = vec![graded(100, &["src/a.rs"])];
        let ranking = compute_epistemic_ranking(&snapshots, &events, &[], &[]);
        let report = format_epistemic_report(
            &snapshots,
            &ranking,
            &NeverForecastGroups::default(),
            &ExperimentFamilies::default(),
        );
        assert!(
            report.contains("no ungraded predictions"),
            "all-graded state must be honest, got: {report}"
        );
    }

    #[test]
    fn test_stale_file_gets_lower_weight_reason() {
        // stale.rs seen only in the first of 7 snapshots, never graded.
        let mut snapshots = vec![snap(100, &["src/stale.rs"], &[])];
        for d in 101..107 {
            snapshots.push(snap(d, &["src/other.rs"], &[]));
        }
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[], &[]);
        let stale = ranking
            .iter()
            .find(|e| e.path == "src/stale.rs")
            .expect("stale file must appear");
        assert!(
            stale
                .reasons
                .iter()
                .any(|r| r.contains("snapshots ago, no graded event since")),
            "stale reason present: {:?}",
            stale.reasons
        );
        // never-graded + stale > never-graded alone
        assert!(stale.score > W_NEVER_GRADED, "stale adds weight");
    }

    #[test]
    fn test_score_is_sum_of_signals() {
        // e.rs: never graded (2.0) + stale, seen only in the first of 6
        // snapshots with no graded event since. The gap is exactly
        // STALE_SNAPSHOT_GAP (5), which is the anchor point where the
        // magnitude-scaled `stale_weight` returns exactly W_STALE — so this
        // fixture still pins the sum of the two named consts.
        let mut snapshots = vec![snap(100, &[], &["src/e.rs"])];
        for d in 101..106 {
            snapshots.push(snap(d, &["src/other.rs"], &[]));
        }
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[], &[]);
        let e = ranking
            .iter()
            .find(|en| en.path == "src/e.rs")
            .expect("must appear");
        assert!(
            (e.score - (W_NEVER_GRADED + W_STALE)).abs() < 1e-9,
            "score {} should be W_NEVER_GRADED + W_STALE",
            e.score
        );
    }

    #[test]
    fn test_report_lists_reasons() {
        let snapshots = vec![snap(100, &["src/b.rs"], &[])];
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[], &[]);
        let report = format_epistemic_report(
            &snapshots,
            &ranking,
            &NeverForecastGroups::default(),
            &ExperimentFamilies::default(),
        );
        assert!(report.contains("src/b.rs"));
        assert!(report.contains("never graded"));
    }

    #[test]
    fn test_tie_break_by_risk_score_deterministic() {
        // a.rs and z.rs have identical epistemic scores (each alone at the
        // top of one column in its own snapshot, never graded). z.rs has the
        // higher current risk score, so it must rank first despite sorting
        // after a.rs alphabetically — and the ordering is stable across runs.
        let snapshots = vec![snap(100, &["src/a.rs"], &[]), snap(101, &[], &["src/z.rs"])];
        let risk_scores = vec![("src/a.rs".to_string(), 1.0), ("src/z.rs".to_string(), 9.0)];
        let first = compute_epistemic_ranking(&snapshots, &[], &risk_scores, &[]);
        assert!(
            (first[0].score - first[1].score).abs() < SCORE_EPSILON,
            "fixture must produce an epistemic tie, got {} vs {}",
            first[0].score,
            first[1].score
        );
        assert_eq!(first[0].path, "src/z.rs", "higher risk score wins the tie");
        assert_eq!(first[1].path, "src/a.rs");
        for _ in 0..5 {
            let again = compute_epistemic_ranking(&snapshots, &[], &risk_scores, &[]);
            let paths: Vec<&str> = again.iter().map(|e| e.path.as_str()).collect();
            let expected: Vec<&str> = first.iter().map(|e| e.path.as_str()).collect();
            assert_eq!(paths, expected, "ordering must be deterministic");
        }
    }

    #[test]
    fn test_tie_break_abstention_unscored_sorts_last() {
        // Abstention case (Day 144): a tied file the risk model has NO score
        // for is not silently absorbed — it explicitly sorts after scored
        // files within the tie, then by path among fellow unscored files.
        let snapshots = vec![
            snap(100, &["src/a.rs"], &[]),
            snap(101, &[], &["src/m.rs"]),
            snap(102, &["src/z.rs"], &[]),
        ];
        // Only z.rs has a current risk score; a.rs and m.rs abstain.
        let risk_scores = vec![("src/z.rs".to_string(), 0.5)];
        let ranking = compute_epistemic_ranking(&snapshots, &[], &risk_scores, &[]);
        let paths: Vec<&str> = ranking.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["src/z.rs", "src/a.rs", "src/m.rs"],
            "scored file first within the tie, then unscored by path"
        );
    }

    #[test]
    fn test_report_notes_tie_break() {
        let snapshots = vec![snap(100, &["src/a.rs"], &[]), snap(101, &["src/b.rs"], &[])];
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[], &[]);
        assert!(
            (ranking[0].score - ranking[1].score).abs() < SCORE_EPSILON,
            "fixture must tie"
        );
        let report = format_epistemic_report(
            &snapshots,
            &ranking,
            &NeverForecastGroups::default(),
            &ExperimentFamilies::default(),
        );
        assert!(
            report.contains("ordered by current risk score"),
            "report must note the tie-break honestly, got: {report}"
        );
    }

    #[test]
    fn test_report_no_tie_note_when_scores_distinct() {
        // Distinct scores from the surviving signals: stale.rs is never
        // graded *and* stale (2.0 + 0.5), other.rs is only never graded.
        let mut snapshots = vec![snap(100, &["src/stale.rs"], &[])];
        for d in 101..107 {
            snapshots.push(snap(d, &["src/other.rs"], &[]));
        }
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[], &[]);
        assert!(
            (ranking[0].score - ranking[1].score).abs() > SCORE_EPSILON,
            "fixture must NOT tie, got {:?}",
            ranking.iter().map(|e| e.score).collect::<Vec<_>>()
        );
        let report = format_epistemic_report(
            &snapshots,
            &ranking,
            &NeverForecastGroups::default(),
            &ExperimentFamilies::default(),
        );
        assert!(
            !report.contains("ordered by current risk score"),
            "no tie → no tie note, got: {report}"
        );
    }

    // --- Experiment ledger read-back (Day 151) -----------------------------
    //
    // `dreams/experiments.jsonl` was write-only: the blind-spot map could not
    // see the expeditions it sent, so it kept pointing the planner at files
    // already studied. These pin the handle.

    fn exp(path: &str, grade: &str, day: u32) -> ExperimentVisit {
        ExperimentVisit {
            path: path.to_string(),
            day,
            state: StudyState::Graded(grade.to_string()),
        }
    }

    fn visit(path: &str, day: u32) -> ExperimentVisit {
        ExperimentVisit {
            path: path.to_string(),
            day,
            state: StudyState::VisitedUngraded,
        }
    }

    #[test]
    fn test_parse_experiment_visits_keeps_summary_grades() {
        let ledger = concat!(
            r#"{"type":"experiment","day":150,"target":"src/a.rs","graded":null}"#,
            "\n",
            r#"{"type":"experiment_result","day":150,"target":"src/a.rs","graded":"miss"}"#,
            "\n",
            r#"{"type":"experiment_result","day":151,"target":"src/b.rs","graded":"partial"}"#,
            "\n",
        );
        let got = parse_experiment_visits(ledger);
        assert_eq!(
            got,
            vec![
                // The guess line is a visit with no grade of its own...
                visit("src/a.rs", 150),
                // ...and the result line carries the grade.
                exp("src/a.rs", "miss", 150),
                exp("src/b.rs", "partial", 151)
            ],
            "a non-null summary grade is still read verbatim"
        );
    }

    /// #711 (a): 6 of the 7 null-summary result lines in the real ledger carry
    /// real per-hypothesis grades. The round WAS graded; only the summary field
    /// is empty. Read the per-hypothesis records as the source of truth.
    #[test]
    fn test_null_summary_with_hypothesis_grades_is_graded() {
        let ledger = concat!(
            r#"{"type":"experiment_result","day":154,"target":"src/prompt.rs","graded":null,"#,
            r#""hypothesis_grades":[{"id":"h1","graded":"hit"},{"id":"h2","graded":"MISS"},"#,
            r#"{"id":"h3","graded":null}]}"#,
            "\n",
        );
        let got = parse_experiment_visits(ledger);
        assert_eq!(
            got,
            vec![ExperimentVisit {
                path: "src/prompt.rs".to_string(),
                day: 154,
                // Only the two records carrying a grade of their own count.
                state: StudyState::Graded("2 hypotheses graded".to_string()),
            }],
            "null summary + per-hypothesis grades = graded, derived from the records"
        );
    }

    /// #711 (b): a round with no grade anywhere is VISITED, not absent. It
    /// used to vanish entirely, which is the one reading that makes the
    /// ranking send the planner back to a file it already spent a session on.
    #[test]
    fn test_fully_ungraded_round_is_visited_not_absent() {
        let ledger = concat!(
            r#"{"type":"experiment","day":159,"target":"src/commands_todo.rs","graded":null}"#,
            "\n",
            // Result line with a note where the grades should be — a note is
            // not a grade.
            r#"{"type":"experiment_result","day":159,"target":"src/commands_todo.rs","#,
            r#""graded":null,"hypothesis_grades_note":"never got round to it"}"#,
            "\n",
            // An empty grade array is not grade evidence either.
            r#"{"type":"experiment_result","day":160,"target":"src/c.rs","graded":null,"#,
            r#""hypothesis_grades":[]}"#,
            "\n",
        );
        let got = parse_experiment_visits(ledger);
        assert_eq!(
            got,
            vec![
                visit("src/commands_todo.rs", 159),
                visit("src/commands_todo.rs", 159),
                visit("src/c.rs", 160),
            ],
            "ungraded rounds are recorded as visits, never dropped"
        );
    }

    #[test]
    fn test_parse_experiment_visits_skips_malformed_and_targetless() {
        let ledger = concat!(
            "not json at all\n",
            "\n",
            // graded but no target — nothing to attribute it to.
            r#"{"type":"experiment_result","day":150,"graded":"hit"}"#,
            "\n",
            // empty target is the same as no target.
            r#"{"type":"experiment_result","day":150,"target":"  ","graded":"hit"}"#,
            "\n",
            // an unrelated ledger line type contributes nothing.
            r#"{"type":"note","day":150,"target":"src/a.rs","graded":"hit"}"#,
            "\n",
            r#"{"broken":"#,
            "\n",
        );
        assert!(
            parse_experiment_visits(ledger).is_empty(),
            "malformed / unattributable lines contribute nothing and never panic"
        );
    }

    #[test]
    fn test_missing_ledger_is_empty_not_a_defect() {
        assert!(parse_experiment_visits("").is_empty());
    }

    #[test]
    fn test_graded_experiment_lowers_score_and_adds_its_own_reason() {
        let snapshots = vec![snap(100, &["src/a.rs"], &[])];
        let before = compute_epistemic_ranking(&snapshots, &[], &[], &[]);
        let a_before = before
            .iter()
            .find(|e| e.path == "src/a.rs")
            .expect("never-graded file ranks");

        let experiments = vec![exp("src/a.rs", "miss", 150)];
        let after = compute_epistemic_ranking(&snapshots, &[], &[], &experiments);
        let a_after = after
            .iter()
            .find(|e| e.path == "src/a.rs")
            .expect("still ranked, just less blind");

        assert!(
            a_after.score < a_before.score,
            "study discounts blindness: {} !< {}",
            a_after.score,
            a_before.score
        );
        assert!(
            (a_after.score - (a_before.score + W_RECENTLY_STUDIED)).abs() < SCORE_EPSILON,
            "discount is exactly W_RECENTLY_STUDIED"
        );
        assert!(
            a_after
                .reasons
                .iter()
                .any(|r| r == "studied by graded experiment (day 150, miss)"),
            "study gets its own reason string: {:?}",
            a_after.reasons
        );
        // Honesty: study is NOT validation grading. The never-graded fact is
        // still true of the validation ledger, so it must still be said.
        assert!(
            a_after.reasons.iter().any(|r| r.contains("never graded")),
            "both facts are true at once and both get said: {:?}",
            a_after.reasons
        );
    }

    /// Was `test_ungraded_experiment_changes_nothing`, and that name was the
    /// bug (#711): treating an ungraded round as no round at all is exactly
    /// what made the ranking send the planner back to files it had already
    /// visited. The round is now its own third value.
    #[test]
    fn test_ungraded_experiment_is_a_visit_not_a_nonevent() {
        let snapshots = vec![snap(100, &["src/a.rs", "src/b.rs"], &["src/c.rs"])];
        let baseline = compute_epistemic_ranking(&snapshots, &[], &[], &[]);
        let a_before = baseline
            .iter()
            .find(|e| e.path == "src/a.rs")
            .expect("never-graded file ranks");

        let ledger = r#"{"type":"experiment","day":150,"target":"src/a.rs","graded":null}"#;
        let experiments = parse_experiment_visits(ledger);
        assert_eq!(experiments, vec![visit("src/a.rs", 150)]);

        let after = compute_epistemic_ranking(&snapshots, &[], &[], &experiments);
        let a_after = after
            .iter()
            .find(|e| e.path == "src/a.rs")
            .expect("still ranked, just less blind");
        assert!(
            (a_after.score - (a_before.score + W_VISITED_UNGRADED)).abs() < SCORE_EPSILON,
            "discount is exactly W_VISITED_UNGRADED: {} vs baseline {}",
            a_after.score,
            a_before.score
        );
        assert!(
            a_after
                .reasons
                .iter()
                .any(|r| r == "visited by ungraded experiment (day 150)"),
            "the visit gets its own reason: {:?}",
            a_after.reasons
        );
        // Honesty: a visit is not a grading. It must never wear the word.
        assert!(
            !a_after
                .reasons
                .iter()
                .any(|r| r.starts_with("studied by graded experiment")),
            "an ungraded visit never claims to have been graded: {:?}",
            a_after.reasons
        );
        assert!(
            a_after.reasons.iter().any(|r| r.contains("never graded")),
            "the validation ledger is still ungraded and still says so: {:?}",
            a_after.reasons
        );
        // Untouched files are unaffected — the visit is attributed, not global.
        let fmt = |v: &[EpistemicEntry]| {
            v.iter()
                .filter(|e| e.path != "src/a.rs")
                .map(|e| format!("{}|{:.6}|{:?}", e.path, e.score, e.reasons))
                .collect::<Vec<_>>()
        };
        assert_eq!(fmt(&baseline), fmt(&after));
    }

    /// The two study states are separate facts and must read as separate
    /// sentences — a reader (and the trajectory extractor) has to be able to
    /// tell "I graded a guess here" from "I merely walked past".
    #[test]
    fn test_graded_and_visited_reasons_are_distinct() {
        let snapshots = vec![snap(100, &["src/a.rs", "src/b.rs"], &[])];
        let experiments = vec![exp("src/a.rs", "miss", 150), visit("src/b.rs", 151)];
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[], &experiments);
        let reason_of = |p: &str| {
            ranking
                .iter()
                .find(|e| e.path == p)
                .unwrap_or_else(|| panic!("{p} ranks"))
                .reasons
                .iter()
                .find(|r| r.contains("experiment"))
                .cloned()
                .unwrap_or_else(|| panic!("{p} has a study reason"))
        };
        let a = reason_of("src/a.rs");
        let b = reason_of("src/b.rs");
        assert_eq!(a, "studied by graded experiment (day 150, miss)");
        assert_eq!(b, "visited by ungraded experiment (day 151)");
        assert_ne!(a, b);
        assert!(
            !b.starts_with("studied by graded"),
            "visited ≠ studied-and-graded: {b}"
        );
    }

    /// A grade once earned is not erased by a later expedition that recorded
    /// nothing — whatever order the ledger lines arrive in.
    #[test]
    fn test_graded_round_outranks_a_later_ungraded_visit() {
        let snapshots = vec![snap(100, &["src/a.rs"], &[])];
        for experiments in [
            vec![exp("src/a.rs", "hit", 150), visit("src/a.rs", 162)],
            vec![visit("src/a.rs", 162), exp("src/a.rs", "hit", 150)],
        ] {
            let ranking = compute_epistemic_ranking(&snapshots, &[], &[], &experiments);
            let a = ranking.iter().find(|e| e.path == "src/a.rs").unwrap();
            assert!(
                a.reasons
                    .iter()
                    .any(|r| r == "studied by graded experiment (day 150, hit)"),
                "graded state survives a later bare visit: {:?}",
                a.reasons
            );
            assert!(
                !a.reasons.iter().any(|r| r.starts_with("visited by")),
                "one study reason, not two: {:?}",
                a.reasons
            );
        }
    }

    #[test]
    fn test_absent_ledger_leaves_ranking_byte_identical() {
        let snapshots = vec![
            snap(100, &["src/a.rs", "src/b.rs"], &["src/c.rs"]),
            snap(101, &["src/a.rs"], &[]),
        ];
        let events = vec![graded(101, &["src/a.rs"])];
        let risk = vec![("src/a.rs".to_string(), 3.0), ("src/b.rs".to_string(), 1.0)];
        let never = never_forecast_files(&snapshots, &risk, &[], &|_| None);
        let with_empty = compute_epistemic_ranking(&snapshots, &events, &risk, &[]);
        let report = format_epistemic_report(
            &snapshots,
            &with_empty,
            &never,
            &ExperimentFamilies::default(),
        );
        // The default path (no ledger on disk) must be provably unchanged.
        let parsed_empty = parse_experiment_visits("");
        let with_missing_file =
            compute_epistemic_ranking(&snapshots, &events, &risk, &parsed_empty);
        let report2 = format_epistemic_report(
            &snapshots,
            &with_missing_file,
            &never,
            &ExperimentFamilies::default(),
        );
        assert_eq!(report, report2);
        assert!(
            !report.contains("studied by graded experiment"),
            "no ledger → no study reason anywhere: {report}"
        );
    }

    #[test]
    fn test_latest_grade_wins_for_repeated_study() {
        let snapshots = vec![snap(100, &["src/a.rs"], &[])];
        let baseline = compute_epistemic_ranking(&snapshots, &[], &[], &[]);
        let a_before = baseline.iter().find(|e| e.path == "src/a.rs").unwrap();
        let experiments = vec![exp("src/a.rs", "miss", 150), exp("src/a.rs", "hit", 151)];
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[], &experiments);
        let a = ranking.iter().find(|e| e.path == "src/a.rs").unwrap();
        assert!(
            a.reasons
                .iter()
                .any(|r| r == "studied by graded experiment (day 151, hit)"),
            "most recent grade is the one reported: {:?}",
            a.reasons
        );
        // Discounted once, not once per experiment: exactly one
        // W_RECENTLY_STUDIED below the same fixture without any experiments.
        assert!(
            (a.score - (a_before.score + W_RECENTLY_STUDIED)).abs() < SCORE_EPSILON,
            "two graded experiments discount once: {} vs baseline {}",
            a.score,
            a_before.score
        );
    }

    // ---- hypothesis-family tally (archive-derived vs file-specific) ----

    /// Legacy ledger: every line predates per-hypothesis provenance. Both real
    /// families stay empty; the predating experiments are counted and named.
    #[test]
    fn test_families_legacy_ledger_counts_predating_experiments() {
        let ledger = concat!(
            r#"{"type":"experiment","day":150,"target":"src/a.rs","graded":null}"#,
            "\n",
            r#"{"type":"experiment_result","day":150,"target":"src/a.rs","graded":"miss"}"#,
            "\n",
            r#"{"type":"experiment_result","day":151,"target":"src/b.rs","graded":"hit"}"#,
            "\n",
        );
        let fam = tally_hypothesis_families(ledger);
        assert_eq!(fam.archive, FamilyTally::default());
        assert_eq!(fam.file_specific, FamilyTally::default());
        assert_eq!(fam.unknown, FamilyTally::default());
        assert_eq!(fam.experiments_without_hypotheses, 2);

        let report = format_epistemic_report(
            &[snap(1, &["src/a.rs"], &[])],
            &[],
            &NeverForecastGroups::default(),
            &fam,
        );
        assert!(
            report.contains("2 earlier experiment(s) predate per-hypothesis provenance"),
            "predating experiments are named: {report}"
        );
        assert!(
            report.contains("(no file-specific hypotheses recorded yet)"),
            "empty family says so instead of showing a rate: {report}"
        );
        assert!(
            !report.contains("0 hit / 0 graded"),
            "never a rate over an empty denominator: {report}"
        );
    }

    /// Mixed ledger: a file-specific hit, an archive miss, and an unrecognised
    /// provenance. The unrecognised one is an explicit third value — it must
    /// not be absorbed into either real family.
    #[test]
    fn test_families_mixed_ledger_unknown_is_its_own_bucket() {
        let ledger = concat!(
            r#"{"type":"experiment","day":151,"target":"src/a.rs","hypotheses":[{"id":"h1","provenance":"file_specific","claim":"c","evidence":"e"}]}"#,
            "\n",
            r#"{"type":"experiment_result","day":151,"target":"src/a.rs","graded":"hit","hypothesis_grades":[{"id":"h1","provenance":"file_specific","graded":"hit"},{"id":"h2","provenance":"archive","graded":"miss"},{"id":"h3","provenance":"wat","graded":"hit"}]}"#,
            "\n",
        );
        let fam = tally_hypothesis_families(ledger);
        assert_eq!(fam.file_specific.graded, 1);
        assert_eq!(fam.file_specific.hits, 1);
        assert_eq!(fam.archive.graded, 1);
        assert_eq!(fam.archive.hits, 0);
        assert_eq!(fam.unknown.graded, 1, "\"wat\" is Unknown, not archive");
        assert_eq!(fam.unknown.hits, 1);
        // This result carried per-hypothesis records, so it does not predate them.
        assert_eq!(fam.experiments_without_hypotheses, 0);

        let report = format_epistemic_report(
            &[snap(1, &["src/a.rs"], &[])],
            &[],
            &NeverForecastGroups::default(),
            &fam,
        );
        assert!(report.contains("file-specific"), "{report}");
        assert!(report.contains("archive"), "{report}");
        assert!(
            report.contains("unrecognised provenance"),
            "unknown bucket is disclosed, not hidden: {report}"
        );
    }

    /// A genre prior is a real third family, not a flattering `file_specific`.
    /// "Any stateful line-mode renderer forgets to reset at end of stream" is
    /// true of programs of this kind — a hit there proves the genre, not my
    /// model of the file — so it must land in its own bucket.
    #[test]
    fn test_families_genre_prior_is_its_own_bucket() {
        let ledger = concat!(
            r#"{"type":"experiment_result","day":154,"target":"src/format/markdown.rs","graded":"hit","hypothesis_grades":[{"id":"h2","provenance":"genre_prior","graded":"hit"},{"id":"h1","provenance":"file_specific","graded":"miss"}]}"#,
            "\n",
        );
        let fam = tally_hypothesis_families(ledger);
        assert_eq!(fam.genre_prior.graded, 1);
        assert_eq!(fam.genre_prior.hits, 1);
        assert_eq!(fam.file_specific.graded, 1);
        assert_eq!(fam.file_specific.hits, 0);
        assert_eq!(
            fam.unknown,
            FamilyTally::default(),
            "genre_prior is recognised, so it never falls to unknown"
        );
        // The header count must agree with the rows.
        assert_eq!(fam.total_graded(), 2);

        let report = format_epistemic_report(
            &[snap(1, &["src/a.rs"], &[])],
            &[],
            &NeverForecastGroups::default(),
            &fam,
        );
        assert!(
            report.contains("genre-prior"),
            "genre-prior row is rendered: {report}"
        );
        assert!(
            report.contains("2 graded hypotheses"),
            "total counts the genre-prior entry: {report}"
        );
    }

    /// An empty genre-prior tally prints no row at all — an always-zero row is
    /// noise, same call as the `unrecognised` precedent.
    #[test]
    fn test_families_empty_genre_prior_prints_no_row() {
        let ledger = concat!(
            r#"{"type":"experiment_result","day":151,"target":"src/a.rs","graded":"hit","hypothesis_grades":[{"id":"h1","provenance":"archive","graded":"hit"}]}"#,
            "\n",
        );
        let fam = tally_hypothesis_families(ledger);
        assert_eq!(fam.genre_prior, FamilyTally::default());
        let report = format_epistemic_report(
            &[snap(1, &["src/a.rs"], &[])],
            &[],
            &NeverForecastGroups::default(),
            &fam,
        );
        assert!(
            report.contains("chosen-experiment record"),
            "block still renders: {report}"
        );
        assert!(
            !report.contains("genre-prior"),
            "no zero row for an empty genre-prior family: {report}"
        );
    }

    /// Empty / missing ledger → the block prints nothing at all. Silence beats
    /// an empty scoreboard, and today's output stays byte-identical.
    #[test]
    fn test_families_empty_ledger_prints_nothing() {
        let fam = tally_hypothesis_families("");
        assert!(fam.is_empty());
        let snapshots = vec![snap(1, &["src/a.rs"], &[])];
        let with = format_epistemic_report(&snapshots, &[], &NeverForecastGroups::default(), &fam);
        let without = format_epistemic_report(
            &snapshots,
            &[],
            &NeverForecastGroups::default(),
            &ExperimentFamilies::default(),
        );
        assert_eq!(with, without);
        assert!(
            !with.contains("chosen-experiment record"),
            "no header when there is nothing to report: {with}"
        );
    }

    /// Consumer guard: no line of the new block may look like a ranked entry
    /// (`N. path score`) to `scripts/extract_trajectory.py::EPISTEMIC_ENTRY_RE`.
    #[test]
    fn test_families_block_does_not_look_like_a_ranked_entry() {
        let ledger = concat!(
            r#"{"type":"experiment_result","day":151,"target":"src/a.rs","graded":"hit","hypothesis_grades":[{"id":"h1","provenance":"archive","graded":"hit"},{"id":"h2","provenance":"file_specific","graded":"miss"}]}"#,
            "\n",
        );
        let fam = tally_hypothesis_families(ledger);
        let report = format_epistemic_report(
            &[snap(1, &["src/a.rs"], &[])],
            &[],
            &NeverForecastGroups::default(),
            &fam,
        );
        let block: Vec<&str> = report
            .lines()
            .skip_while(|l| !l.contains("chosen-experiment record"))
            .collect();
        assert!(!block.is_empty(), "block rendered: {report}");
        // Mirrors EPISTEMIC_ENTRY_RE: ^\s*\d+\.\s+(\S+)\s+(\d+(\.\d+)?)\s*$
        for line in &block {
            let t = line.trim();
            let looks_ranked = t
                .split_once(". ")
                .map(|(n, rest)| {
                    !n.is_empty()
                        && n.chars().all(|c| c.is_ascii_digit())
                        && rest.split_whitespace().count() == 2
                })
                .unwrap_or(false);
            assert!(
                !looks_ranked,
                "block line looks like a ranked entry: {line}"
            );
            assert!(
                !t.starts_with('\u{2022}'),
                "block must not reuse the reason bullet: {line}"
            );
        }
    }
}
