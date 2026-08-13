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
    never_forecast_files, NeverForecastGroups, NEVER_FORECAST_SAMPLE,
};

/// Weight for "predicted but never graded" — the strongest blindness signal:
/// the model has made claims about this file that no outcome ever tested.
pub(crate) const W_NEVER_GRADED: f64 = 2.0;

/// Lower weight for staleness: last seen ≥ [`STALE_SNAPSHOT_GAP`] snapshots
/// ago with no graded event since.
pub(crate) const W_STALE: f64 = 0.5;

/// A file last seen this many snapshots ago (or more) counts as stale.
pub(crate) const STALE_SNAPSHOT_GAP: usize = 5;

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
    /// A round named this file but recorded no grade anywhere. The expedition
    /// happened; it produced nothing falsifiable.
    VisitedUngraded,
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
        let state = match val["graded"].as_str() {
            Some(g) if !g.trim().is_empty() => StudyState::Graded(g.trim().to_string()),
            _ => match graded_hypothesis_count(&val) {
                0 => StudyState::VisitedUngraded,
                n => StudyState::Graded(format!("{n} hypotheses graded")),
            },
        };
        out.push(ExperimentVisit { path, day, state });
    }
    out
}

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
        self.total_graded() == 0 && self.experiments_without_hypotheses == 0
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
fn format_experiment_families(fam: &ExperimentFamilies) -> String {
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
    out
}

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
const REASON_MAX_CHARS: usize = 140;

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
pub(crate) struct EpistemicEntry {
    pub(crate) path: String,
    pub(crate) score: f64,
    /// Human-readable reasons, e.g. "predicted 5×, never graded".
    pub(crate) reasons: Vec<String>,
    /// Current risk score, used only to break epistemic-score ties.
    /// `None` is the explicit abstention case: the risk model has no score
    /// for this file — it sorts after scored files within a tie.
    pub(crate) risk_score: Option<f64>,
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
                let held_is_graded = matches!(state, StudyState::Graded(_));
                let new_is_graded = matches!(visit.state, StudyState::Graded(_));
                let replace = match (held_is_graded, new_is_graded) {
                    (false, true) => true,
                    (true, false) => false,
                    _ => visit.day >= *day,
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
            score += W_STALE;
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
            }
        }

        if score > 0.0 {
            entries.push(EpistemicEntry {
                path: path.clone(),
                score,
                reasons,
                risk_score: score_by_path.get(path.as_str()).copied(),
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
        for (i, e) in entries.iter().take(REPORT_TOP_N).enumerate() {
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
            "\n  {DIM}high score = the model is blindest here; an outcome touching these files teaches the most{RESET}\n"
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
            if never.studied.len() == 1 { "it" } else { "them" },
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

    // Rendered last, below the never-forecast header, so the trajectory
    // extractor (which hard-stops collecting there) can never absorb it.
    out.push_str(&format_experiment_families(families));
    out
}

/// Handle `/risk epistemic` — read the existing snapshot + validation JSONL,
/// compute the ranking, print the report.
pub(crate) fn handle_risk_epistemic() {
    use crate::commands_risk_snapshots::{
        parse_all_snapshots, parse_graded_events, RISK_SNAPSHOT_PATH, RISK_VALIDATION_PATH,
    };

    let snapshot_content =
        std::fs::read_to_string(std::path::Path::new(RISK_SNAPSHOT_PATH)).unwrap_or_default();
    let validation_content =
        std::fs::read_to_string(std::path::Path::new(RISK_VALIDATION_PATH)).unwrap_or_default();

    let snapshots = parse_all_snapshots(&snapshot_content);
    let events = parse_graded_events(&validation_content);
    // Study history — absent ledger is an honest empty, not an error.
    let experiment_content =
        std::fs::read_to_string(std::path::Path::new(EXPERIMENT_LEDGER_PATH)).unwrap_or_default();
    let experiments = parse_experiment_visits(&experiment_content);
    // Current risk scores, used only to break epistemic ties.
    let risk_scores = crate::commands_risk::top_risk_files(usize::MAX);
    let entries = compute_epistemic_ranking(&snapshots, &events, &risk_scores, &experiments);
    let never = never_forecast_files(&snapshots, &risk_scores, &experiments);
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
        let report = format_epistemic_report(&[], &ranking, &NeverForecastGroups::default(), &ExperimentFamilies::default());
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
        let report =
            format_epistemic_report(&snapshots, &ranking, &NeverForecastGroups::default(), &ExperimentFamilies::default());
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
        // e.rs: never graded (2.0) + stale, seen only in the first of 7
        // snapshots with no graded event since (0.5).
        let mut snapshots = vec![snap(100, &[], &["src/e.rs"])];
        for d in 101..107 {
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
        let report =
            format_epistemic_report(&snapshots, &ranking, &NeverForecastGroups::default(), &ExperimentFamilies::default());
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
        let report =
            format_epistemic_report(&snapshots, &ranking, &NeverForecastGroups::default(), &ExperimentFamilies::default());
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
        let report =
            format_epistemic_report(&snapshots, &ranking, &NeverForecastGroups::default(), &ExperimentFamilies::default());
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
        let never = never_forecast_files(&snapshots, &risk, &[]);
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

        let report = format_epistemic_report(&[snap(1, &["src/a.rs"], &[])], &[], &NeverForecastGroups::default(), &fam);
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

        let report = format_epistemic_report(&[snap(1, &["src/a.rs"], &[])], &[], &NeverForecastGroups::default(), &fam);
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

        let report = format_epistemic_report(&[snap(1, &["src/a.rs"], &[])], &[], &NeverForecastGroups::default(), &fam);
        assert!(
            report.contains("genre-prior"),
            "genre-prior row is rendered: {report}"
        );
        assert!(
            report.contains("2 graded hypotheses"),
            "total counts the genre-prior entry: {report}"
        );
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
        let report = format_epistemic_report(&[snap(1, &["src/a.rs"], &[])], &[], &NeverForecastGroups::default(), &fam);
        assert!(
            report.contains("chosen-experiment record"),
            "block still renders: {report}"
        );
        assert!(
            !report.contains("genre-prior"),
            "no zero row for an empty genre-prior family: {report}"
        );
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

    /// Empty / missing ledger → the block prints nothing at all. Silence beats
    /// an empty scoreboard, and today's output stays byte-identical.
    #[test]
    fn test_families_empty_ledger_prints_nothing() {
        let fam = tally_hypothesis_families("");
        assert!(fam.is_empty());
        let snapshots = vec![snap(1, &["src/a.rs"], &[])];
        let with = format_epistemic_report(&snapshots, &[], &NeverForecastGroups::default(), &fam);
        let without = format_epistemic_report(&snapshots, &[], &NeverForecastGroups::default(), &ExperimentFamilies::default());
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
        let report = format_epistemic_report(&[snap(1, &["src/a.rs"], &[])], &[], &NeverForecastGroups::default(), &fam);
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

#[cfg(test)]
mod reason_truncation_tests {
    use super::*;

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
        }];
        let report =
            format_epistemic_report(&snapshots, &entries, &NeverForecastGroups::default(), &ExperimentFamilies::default());
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
        }];
        let report =
            format_epistemic_report(&snapshots, &entries, &NeverForecastGroups::default(), &ExperimentFamilies::default());
        assert!(
            !report.contains("shortened for display"),
            "silence is correct when nothing was cut:\n{report}"
        );
    }
}
