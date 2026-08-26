//! The never-forecast section of `/risk epistemic` — the files the epistemic
//! ranking structurally *cannot* see.
//!
//! Extracted verbatim from `commands_risk_epistemic.rs` on Day 166 to make
//! room under the module-size gate; no behaviour change, and every call site
//! is unchanged via a re-export from that module.
//!
//! The ranking next door is built by iterating snapshot columns, so a file
//! that was never guessed about gets no entry at all. This module names that
//! blind spot explicitly rather than letting absence be absorbed by silence.

use crate::commands_risk_epistemic::{latest_study_state_by_path, ExperimentVisit, StudyState};
use crate::commands_risk_snapshots::ParsedSnapshot;

/// How many never-forecast files are listed as a sample. The honest *total*
/// is always printed alongside — the cap is a display budget, not a claim.
pub(crate) const NEVER_FORECAST_SAMPLE: usize = 5;

/// Display cap for a study annotation, in chars. Long grade summaries would
/// otherwise wrap the one-line-per-file shape this section depends on. The
/// cut is marked in-band by `truncate_reason`'s ellipsis (Day 162).
const STUDY_NOTE_MAX_CHARS: usize = 100;

/// How many snapshots must have been taken **after** a file first existed
/// before "never forecast" is evidence about the model rather than evidence
/// about the file's age.
///
/// This is a **judgment threshold, not a measurement**. Nothing was graded to
/// pick 5; it is simply a number large enough that a column had real chances
/// to name the file and small enough that a file survives one quiet day
/// without being excused forever. Below it, absence from every prediction
/// column carries no information — no snapshot could have named a path that
/// did not exist yet.
pub(crate) const MIN_FORECAST_OPPORTUNITIES: usize = 5;

/// A file the risk model scores but that has **never once** been forecast —
/// absent from every snapshot's reactive (`predicted`) and anticipatory
/// (`emerging`) column alike.
///
/// This is the ranking's structural blind spot (Day 149): the epistemic score
/// is built by iterating snapshot columns, so a file that was never guessed
/// about gets no entry at all — it is invisible to the very view whose job is
/// to find where the model is blindest. A detector for *known* unknowns.
/// Rather than blend these into the score (a list where everything is equally
/// unknown ranks nothing), they are an explicit third value.
pub(crate) struct NeverForecast {
    pub(crate) path: String,
    pub(crate) risk_score: f64,
}

/// A file no column ever forecast **but** that an experiment round has already
/// opened on purpose — the room I lit myself.
///
/// Kept apart from [`NeverForecast`] rather than silently subtracted from it
/// (Day 144: absence gets its own name). "No column ever guessed about it" and
/// "I once studied it" are two different facts; collapsing them destroys the
/// first, and dropping these files outright would hide the second. So they are
/// surfaced separately, annotated with when the visit happened and whether it
/// graded anything — never presented as unexplored (#744: round 43 was handed
/// `src/commands_lint.rs` as a dark room ~4.5h after round 40 studied it).
pub(crate) struct StudiedNeverForecast {
    pub(crate) path: String,
    pub(crate) risk_score: f64,
    /// Human-readable study annotation. Uses the ranked half's vocabulary —
    /// `studied by graded experiment (day N, …)` for a graded round,
    /// `visited by ungraded experiment (day N)` for one that scored nothing.
    /// A visit must never wear the word "graded".
    pub(crate) note: String,
}

/// A file no column ever forecast **but** that is younger than most of the
/// prediction history — a room I built this afternoon.
///
/// Kept apart from [`NeverForecast`] for the same reason [`StudiedNeverForecast`]
/// is (Day 144: absence gets its own name), one layer over. There are two
/// different absences here: "the columns had many chances to name this file and
/// never did" is evidence about the model, while "the file did not exist when
/// the snapshots were written" is no evidence at all — no snapshot *could* have
/// named a path that did not exist. Today's trajectory handed the planner
/// `src/commands_risk_parse.rs` and `src/help_data_guards.rs` as the two darkest
/// rooms in the building; both were authored hours earlier as pure extractions.
pub(crate) struct TooNewNeverForecast {
    pub(crate) path: String,
    pub(crate) risk_score: f64,
    /// The `YYYY-MM-DD` the file was added, for display. Derived from the
    /// resolved add-timestamp, never invented.
    pub(crate) added: String,
    /// How many snapshots were taken *after* the file existed. Always below
    /// [`MIN_FORECAST_OPPORTUNITIES`] for a file in this group.
    pub(crate) opportunities: usize,
}

/// The never-forecast set, split into its three honest states.
#[derive(Default)]
pub(crate) struct NeverForecastGroups {
    /// Never forecast **and** never studied **and** old enough that the
    /// columns had real chances — the actually-dark set. This is what the
    /// report leads with and what the exploration budget is for.
    pub(crate) dark: Vec<NeverForecast>,
    /// Never forecast but studied/visited on some day — lit rooms that no
    /// prediction column has ever named.
    pub(crate) studied: Vec<StudiedNeverForecast>,
    /// Never forecast but younger than the prediction history — absence here
    /// says nothing about the model.
    pub(crate) too_new: Vec<TooNewNeverForecast>,
    /// How many of the files in [`dark`](Self::dark) got there with an age
    /// that **could not be observed at all** — the resolver answered `None`,
    /// or answered something unparseable (#819).
    ///
    /// This is a *count*, not a fourth group: every one of these files is
    /// still in `dark`, because an unknown must never be promoted into the
    /// comfortable bucket (Day 144). What it fixes is the reading: `dark`
    /// mixes files verified old enough that the columns had real chances
    /// with files whose age nothing could measure, and printing them under
    /// one header renders "could not check" as "checked; clean".
    ///
    /// The shallow-clone case is the *common* one here — the harness
    /// checkout is shallow, so `git_added_ts` declines for every file older
    /// than the clone window (see [`shallow_boundary_hides_age`]).
    ///
    /// Only files that actually land in `dark` are counted. A studied file
    /// is classified before the age question is ever asked, so counting one
    /// there would be reporting an unknown that changed nothing.
    pub(crate) age_unobservable: usize,
}

/// Every scored path that appears in **no** snapshot's `predicted` column and
/// **no** snapshot's `emerging` column, across all snapshots — the files the
/// epistemic ranking structurally cannot see — split by whether an experiment
/// round has already visited them (`experiments`, the same ledger the ranked
/// half reads).
///
/// Ordering within each group: current risk score descending, then path
/// ascending (fully deterministic). Rationale — a file that *changes* (it has
/// a risk score at all) yet has never once been forecast is the most
/// consequential blind spot, so churny-but-unforecast files come first.
///
/// Returns empty groups when `snapshots` is empty: with no predictions on
/// record there is nothing to be blind *relative to*, and claiming every
/// scored file is unforecast would be a false "everything is blind" verdict.
///
/// Honest limitation (stated in the report too): the universe here is only
/// what the risk model scores. A file with no recent churn has no risk score
/// and is invisible to both views.
/// `added_ts` resolves a path's first-seen timestamp. It is a parameter rather
/// than a direct git call so the classification stays pure and testable — the
/// only I/O implementation is [`git_added_ts`], and it is invoked **only** for
/// the never-forecast candidate set (already a short list), never once per
/// scored file.
///
/// Classification order, and it is deliberate:
///   1. **studied** — if an experiment round opened the file, that fact is both
///      true and stronger, so it wins even for a file authored this afternoon.
///      This keeps the studied group byte-identical to #744.
///   2. **too new** — a known add-date with fewer than
///      [`MIN_FORECAST_OPPORTUNITIES`] snapshots taken after it.
///   3. **dark** — everything else, *including unknown age*. An unknown must
///      never be quietly promoted into the comfortable bucket (Day 144); if I
///      cannot date the file, I have not earned the right to excuse it.
pub(crate) fn never_forecast_files(
    snapshots: &[ParsedSnapshot],
    risk_scores: &[(String, f64)],
    experiments: &[ExperimentVisit],
    added_ts: &dyn Fn(&str) -> Option<String>,
) -> NeverForecastGroups {
    use std::collections::HashSet;

    if snapshots.is_empty() {
        return NeverForecastGroups::default();
    }

    let mut forecast: HashSet<&str> = HashSet::new();
    for snap in snapshots {
        for p in snap.predicted.iter().chain(snap.emerging.iter()) {
            forecast.insert(p.as_str());
        }
    }

    // Same precedence the ranked half uses (graded outranks a bare visit,
    // latest day wins within a state) — shared helper, never a second copy.
    let studied_states = latest_study_state_by_path(experiments);
    let snapshot_ts: Vec<String> = snapshots.iter().map(|s| s.ts.clone()).collect();

    let mut dark: Vec<NeverForecast> = Vec::new();
    let mut studied: Vec<StudiedNeverForecast> = Vec::new();
    let mut too_new: Vec<TooNewNeverForecast> = Vec::new();
    let mut age_unobservable = 0usize;
    for (path, score) in risk_scores.iter() {
        if forecast.contains(path.as_str()) {
            continue;
        }
        // 1. studied wins outright.
        if let Some((state, day)) = studied_states.get(path.as_str()) {
            studied.push(StudiedNeverForecast {
                path: path.clone(),
                risk_score: *score,
                note: study_note(state, *day),
            });
            continue;
        }
        // 2. too new to have been forecast.
        let added = added_ts(path);
        match forecast_opportunities(added.as_deref(), &snapshot_ts) {
            Some(n) if n < MIN_FORECAST_OPPORTUNITIES => {
                too_new.push(TooNewNeverForecast {
                    path: path.clone(),
                    risk_score: *score,
                    added: date_part(added.as_deref().unwrap_or_default()),
                    opportunities: n,
                });
                continue;
            }
            Some(_) => {}
            // The age could not be observed: no resolver answer, or one that
            // did not parse. The file still goes to `dark` (#819 adds a
            // report, never a reclassification) — but it is counted, so the
            // dark list stops reading as a measurement it never made.
            None => age_unobservable += 1,
        }
        // 3. dark — including unknown age.
        dark.push(NeverForecast {
            path: path.clone(),
            risk_score: *score,
        });
    }
    dark.sort_by(|a, b| risk_then_path(a.risk_score, &a.path, b.risk_score, &b.path));
    studied.sort_by(|a, b| risk_then_path(a.risk_score, &a.path, b.risk_score, &b.path));
    too_new.sort_by(|a, b| risk_then_path(a.risk_score, &a.path, b.risk_score, &b.path));
    NeverForecastGroups {
        dark,
        studied,
        too_new,
        age_unobservable,
    }
}

/// How many snapshots were taken **strictly after** a file first existed.
///
/// Pure. `None` means "unknown age" — either `added_ts` is `None` (no resolver
/// answer) or the value does not parse as an ISO-8601 instant. An unknown is
/// never a zero: a zero would say "this file had no chances", which is the
/// too-new verdict, and inventing that from a parse failure is exactly the
/// absorbed-absence bug this whole section exists to avoid.
///
/// Snapshot timestamps that do not parse are **not** counted: a timestamp I
/// cannot read is not one I can show to postdate the file.
pub(crate) fn forecast_opportunities(
    added_ts: Option<&str>,
    snapshot_timestamps: &[String],
) -> Option<usize> {
    let added = iso8601_sort_key(added_ts?)?;
    Some(
        snapshot_timestamps
            .iter()
            .filter_map(|ts| iso8601_sort_key(ts))
            .filter(|k| *k > added)
            .count(),
    )
}

/// Normalise an ISO-8601 instant to comparable seconds-from-a-fixed-epoch.
///
/// Lexicographic comparison would be enough *if* both sides always used the
/// same shape, but they come from two different producers: snapshot `ts` is
/// written by yoyo (`...Z`) and the add-date comes from `git log --format=%aI`
/// (which can carry a `+hh:mm` offset). So the offset is applied rather than
/// assumed away. Anything unparseable returns `None` — never a panic, never a
/// guessed value.
fn iso8601_sort_key(s: &str) -> Option<i64> {
    let s = s.trim();
    let b = s.as_bytes();
    // Minimum shape: YYYY-MM-DDTHH:MM:SS
    if b.len() < 19 {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> { s.get(from..to)?.parse::<i64>().ok() };
    if b[4] != b'-' || b[7] != b'-' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    if b[10] != b'T' && b[10] != b' ' {
        return None;
    }
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from a civil date (Howard Hinnant's algorithm), epoch-shifted to
    // 0000-03-01; only differences matter here, so the epoch is arbitrary.
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe;
    let mut secs = days * 86400 + h * 3600 + mi * 60 + sec;

    // Offset suffix: Z / +hh:mm / -hh:mm / +hhmm. Absent → treat as UTC.
    let rest = s.get(19..).unwrap_or("");
    let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    if let Some(sign_at) = rest.find(['+', '-']) {
        let sign = if rest.as_bytes()[sign_at] == b'+' {
            1
        } else {
            -1
        };
        let off = &rest[sign_at + 1..];
        let digits: String = off.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 2 {
            let oh: i64 = digits.get(0..2)?.parse().ok()?;
            let om: i64 = digits.get(2..4).and_then(|m| m.parse().ok()).unwrap_or(0);
            secs -= sign * (oh * 3600 + om * 60);
        }
    }
    Some(secs)
}

/// The `YYYY-MM-DD` prefix of an ISO-8601 instant, for display. Returns the
/// input trimmed when it is shorter than a date — never slices blindly.
fn date_part(ts: &str) -> String {
    let ts = ts.trim();
    match ts.get(0..10) {
        Some(d) if d.as_bytes()[4] == b'-' => d.to_string(),
        _ => ts.to_string(),
    }
}

/// I/O half: resolve when a path was first added, via git.
///
/// Deliberately outside [`forecast_opportunities`] so the classification stays
/// pure. Any failure — git missing, non-zero exit, empty output (path not in
/// history) — is `None`, i.e. unknown age, i.e. the file stays in the dark set
/// exactly as before this existed.
///
/// One further `None`: on a **shallow** clone, when the add-commit is a grafted
/// boundary root (see [`shallow_boundary_hides_age`]). The harness checkout is
/// shallow, so without that check every pre-window file reports the boundary's
/// date and the too-new split would be grading clone depth instead of file age.
pub(crate) fn git_added_ts(path: &str) -> Option<String> {
    // One call yields both the add-commit sha and its date, tab-separated.
    let out = crate::git::run_git(&[
        "log",
        "--diff-filter=A",
        "--format=%H%x09%aI",
        "-1",
        "--",
        path,
    ])
    .ok()?
    .trim()
    .to_string();
    // `git log` on an unknown path exits 0 with empty stdout — an absence, not
    // a date.
    let line = out.lines().next().filter(|l| !l.is_empty())?;
    let (sha, ts) = line.split_once('\t')?;
    let ts = ts.trim();
    if ts.is_empty() {
        return None;
    }

    // A shallow clone dates every pre-window file to the grafted boundary. That
    // is confidently wrong, not unknown, so it walks straight past the
    // `None → dark set` fallback unless caught here.
    let is_shallow = crate::git::run_git(&["rev-parse", "--is-shallow-repository"])
        .map(|s| s.trim() == "true")
        .unwrap_or(false);
    let graph_roots: Vec<String> = crate::git::run_git(&["rev-list", "--max-parents=0", "HEAD"])
        .map(|s| s.lines().map(|l| l.trim().to_string()).collect())
        .unwrap_or_default();
    if shallow_boundary_hides_age(is_shallow, sha, &graph_roots) {
        return None;
    }

    Some(ts.to_string())
}

/// Minimum characters a sha must have before a prefix comparison is allowed.
/// Below this a short string could prefix an unrelated commit by luck, and the
/// cost of a false match is a genuinely dark file silently excused.
const MIN_SHA_PREFIX: usize = 7;

/// A shallow clone's grafted boundary commit presents every surviving file
/// as an addition, so `git log --diff-filter=A` on such a repo dates old
/// files to the boundary. That is a fact about clone depth, not the file:
/// treat it as unknown age rather than a recent one.
///
/// `false` whenever the repo is not shallow — a full clone's root commit is a
/// real birthday and keeps its existing behaviour — and whenever either side of
/// the comparison is empty or shorter than [`MIN_SHA_PREFIX`].
pub(crate) fn shallow_boundary_hides_age(
    is_shallow: bool,
    add_sha: &str,
    graph_roots: &[String],
) -> bool {
    if !is_shallow {
        return false;
    }
    let add = add_sha.trim();
    if add.len() < MIN_SHA_PREFIX {
        return false;
    }
    graph_roots.iter().any(|root| {
        let root = root.trim();
        if root.len() < MIN_SHA_PREFIX {
            return false;
        }
        // Accept a prefix match in either direction: one command may abbreviate
        // where the other does not. Both sides are already ≥ MIN_SHA_PREFIX.
        add.starts_with(root) || root.starts_with(add)
    })
}

/// Shared ordering: risk score descending, path ascending.
fn risk_then_path(a_score: f64, a_path: &str, b_score: f64, b_path: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    b_score
        .partial_cmp(&a_score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a_path.cmp(b_path))
}

/// Render one study annotation. Deliberately the ranked half's vocabulary so
/// the two views can't drift apart, and deliberately *different* per state:
/// a round that graded nothing must never be described as graded.
fn study_note(state: &StudyState, day: u32) -> String {
    let text = match state {
        StudyState::Graded(grade) => format!("studied by graded experiment (day {day}, {grade})"),
        StudyState::VisitedUngraded => format!("visited by ungraded experiment (day {day})"),
        // Must not wear the word "graded" unqualified, exactly as
        // `VisitedUngraded` must not. A partial round still counts as "not
        // dark" for the never-forecast split — the file was forecast-less for
        // other reasons — it just doesn't get to claim whole-file coverage.
        StudyState::PartiallyGraded { summary, scope } => {
            format!("partial study (day {day}, {summary}) — scope: {scope}")
        }
    };
    // Char-boundary-safe, and the ellipsis marks the cut in-band.
    crate::commands_risk_epistemic::truncate_reason(&text, STUDY_NOTE_MAX_CHARS).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands_risk_epistemic::{
        compute_epistemic_ranking, format_epistemic_report, ExperimentFamilies,
    };
    use crate::commands_risk_snapshots::GradedEvent;
    use crate::format::{DIM, RESET, YELLOW};

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

    // ---- never-forecast section (Day 149) ----

    fn scores(pairs: &[(&str, f64)]) -> Vec<(String, f64)> {
        pairs.iter().map(|(p, s)| (p.to_string(), *s)).collect()
    }

    #[test]
    fn test_predicted_file_is_not_never_forecast() {
        let snapshots = vec![snap(100, &["src/a.rs"], &[])];
        let never = never_forecast_files(&snapshots, &scores(&[("src/a.rs", 5.0)]), &[], &|_| None);
        assert!(
            never.dark.is_empty(),
            "a file in the reactive column has been forecast: {:?}",
            never.dark.iter().map(|n| &n.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_emerging_only_file_is_not_never_forecast() {
        let snapshots = vec![snap(100, &[], &["src/a.rs"])];
        let never = never_forecast_files(&snapshots, &scores(&[("src/a.rs", 5.0)]), &[], &|_| None);
        assert!(
            never.dark.is_empty(),
            "a file in the anticipatory column has been forecast"
        );
    }

    #[test]
    fn test_scored_file_in_no_snapshot_is_never_forecast() {
        // src/update.rs is the Day 149 journal's real example: churns, scores,
        // and appears in zero of the saved predictions.
        let snapshots = vec![snap(100, &["src/a.rs"], &["src/b.rs"])];
        let never = never_forecast_files(
            &snapshots,
            &scores(&[("src/a.rs", 5.0), ("src/update.rs", 3.2)]),
            &[],
            &|_| None,
        );
        assert_eq!(never.dark.len(), 1, "exactly the unforecast file");
        assert_eq!(never.dark[0].path, "src/update.rs");
        assert!((never.dark[0].risk_score - 3.2).abs() < 1e-9);
    }

    #[test]
    fn test_never_forecast_ordering_risk_desc_then_path() {
        let snapshots = vec![snap(100, &["src/seen.rs"], &[])];
        let never = never_forecast_files(
            &snapshots,
            &scores(&[
                ("src/low.rs", 1.0),
                ("src/zeta.rs", 4.0),
                ("src/alpha.rs", 4.0),
                ("src/high.rs", 9.0),
            ]),
            &[],
            &|_| None,
        );
        let paths: Vec<&str> = never.dark.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["src/high.rs", "src/alpha.rs", "src/zeta.rs", "src/low.rs"],
            "higher risk first, ties broken by path ascending"
        );
    }

    #[test]
    fn test_never_forecast_empty_snapshots_makes_no_claim() {
        // No snapshots = nothing to be blind *relative to*. Claiming every
        // scored file is unforecast here would be a false "everything is
        // blind" verdict, not an honest observation.
        let never = never_forecast_files(&[], &scores(&[("src/a.rs", 5.0)]), &[], &|_| None);
        assert!(never.dark.is_empty(), "empty snapshots → no claim");
    }

    #[test]
    fn test_report_age_unobservable_clause_present_and_absent() {
        // Emission point: the string a caller receives. Both directions —
        // the clause with its real count when > 0, and *nothing* when the
        // whole set's age was observable (the byte-identical common path).
        let snapshots: Vec<ParsedSnapshot> =
            (1..=6).map(|d| snap(d, &["src/seen.rs"], &[])).collect();
        let pairs = scores(&[("src/a.rs", 5.0), ("src/b.rs", 4.0)]);
        let ranking = compute_epistemic_ranking(&snapshots, &[], &pairs, &[]);

        // (a) one of the two cannot be dated.
        let mixed = never_forecast_files(&snapshots, &pairs, &[], &|p| {
            if p == "src/b.rs" {
                Some("2020-01-01T00:00:00Z".to_string())
            } else {
                None
            }
        });
        assert_eq!(mixed.age_unobservable, 1);
        let with =
            format_epistemic_report(&snapshots, &ranking, &mixed, &ExperimentFamilies::default());
        assert!(
            with.contains("age unobservable for 1 of these"),
            "the count must reach the reader, got: {with}"
        );
        assert!(
            with.contains("not evidence the columns ever had a chance"),
            "what the count *means* is the other half of the clause, got: {with}"
        );

        // (b) near-miss: every age observed → the clause must not appear.
        let observed = never_forecast_files(&snapshots, &pairs, &[], &|_| {
            Some("2020-01-01T00:00:00Z".to_string())
        });
        assert_eq!(observed.age_unobservable, 0);
        let without = format_epistemic_report(
            &snapshots,
            &ranking,
            &observed,
            &ExperimentFamilies::default(),
        );
        assert!(
            !without.contains("age unobservable"),
            "a fully observable run says nothing, got: {without}"
        );
    }

    #[test]
    fn test_age_unobservable_clause_does_not_collide_with_the_trajectory_parser() {
        // scripts/extract_trajectory.py hard-stops collecting at the literal
        // "never forecast" and matches dark rows with `^\s*◦\s+(\S+)\s+\(risk`.
        // The clause must trip neither, or the planner's dark-room list moves.
        let snapshots = vec![snap(100, &["src/seen.rs"], &[])];
        let pairs = scores(&[("src/a.rs", 5.0)]);
        let never = never_forecast_files(&snapshots, &pairs, &[], &|_| None);
        let ranking = compute_epistemic_ranking(&snapshots, &[], &pairs, &[]);
        let report =
            format_epistemic_report(&snapshots, &ranking, &never, &ExperimentFamilies::default());
        for line in report.lines().filter(|l| l.contains("age unobservable")) {
            let plain = line
                .replace(RESET.0, "")
                .replace(DIM.0, "")
                .replace(YELLOW.0, "");
            assert!(
                !plain.contains("never forecast"),
                "clause must not re-trigger the hard stop: {plain}"
            );
            assert!(
                !plain.trim_start().starts_with('◦'),
                "clause must not read as a dark row: {plain}"
            );
        }
    }

    #[test]
    fn test_report_never_forecast_total_and_sample_cap() {
        let snapshots = vec![snap(100, &["src/seen.rs"], &[])];
        let mut pairs: Vec<(String, f64)> = Vec::new();
        for i in 0..12 {
            pairs.push((format!("src/f{i:02}.rs"), 12.0 - i as f64));
        }
        let never = never_forecast_files(&snapshots, &pairs, &[], &|_| None);
        assert_eq!(never.dark.len(), 12);
        let ranking = compute_epistemic_ranking(&snapshots, &[], &pairs, &[]);
        let report =
            format_epistemic_report(&snapshots, &ranking, &never, &ExperimentFamilies::default());
        assert!(
            report.contains("12 scored files have never appeared in any prediction"),
            "honest total must be printed, got: {report}"
        );
        // Count rows by their glyph, not by a colored prefix: ANSI codes are
        // emitted unless NO_COLOR is set, so a literal "  ◦ src/f" match would
        // silently find zero rows in a colored run.
        let sample_rows = report
            .lines()
            .filter(|line| {
                let plain = line
                    .replace(RESET.0, "")
                    .replace(DIM.0, "")
                    .replace(YELLOW.0, "");
                plain.trim_start().starts_with('◦') && plain.contains("src/f")
            })
            .count();
        assert_eq!(sample_rows, NEVER_FORECAST_SAMPLE, "sample is capped at 5");
        assert!(
            report.contains("(+7 more)"),
            "remainder must be stated, got: {report}"
        );
        assert!(
            report.contains("have no risk score and are invisible to both views"),
            "the honest limitation caveat is required, got: {report}"
        );
    }

    #[test]
    fn test_report_renders_never_forecast_even_with_no_ranked_entries() {
        // The ranked list can be empty (everything predicted was graded)
        // while the blindest files are exactly the ones never predicted —
        // the section must survive that early return.
        let snapshots = vec![snap(100, &["src/a.rs"], &["src/a.rs"])];
        let events = vec![graded(100, &["src/a.rs"])];
        let ranking = compute_epistemic_ranking(&snapshots, &events, &[], &[]);
        assert!(ranking.is_empty(), "fixture must produce an empty ranking");
        let never =
            never_forecast_files(&snapshots, &scores(&[("src/update.rs", 2.0)]), &[], &|_| {
                None
            });
        let report =
            format_epistemic_report(&snapshots, &ranking, &never, &ExperimentFamilies::default());
        assert!(
            report.contains("no ungraded predictions"),
            "existing empty-ranking message stays, got: {report}"
        );
        assert!(
            report.contains("src/update.rs"),
            "never-forecast section must still render, got: {report}"
        );
    }

    #[test]
    fn test_report_never_forecast_rows_are_not_numbered_entries() {
        // The trajectory parser matches ranked entries as `N. path score`.
        // If the never-forecast rows took that shape they would be silently
        // absorbed as ranked entries and push real ones out of the top-N
        // budget (Day 141). Pin the distinct bullet shape.
        let snapshots = vec![snap(100, &["src/a.rs"], &[])];
        let never =
            never_forecast_files(&snapshots, &scores(&[("src/update.rs", 2.0)]), &[], &|_| {
                None
            });
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[], &[]);
        let report =
            format_epistemic_report(&snapshots, &ranking, &never, &ExperimentFamilies::default());
        for line in report.lines() {
            let plain: String = line.replace(RESET.0, "").replace(DIM.0, "");
            if plain.contains("src/update.rs") {
                assert!(
                    plain.trim_start().starts_with('◦'),
                    "never-forecast rows use the ◦ glyph, not `N. path score`: {line:?}"
                );
            }
        }
    }

    #[test]
    fn test_report_never_forecast_section_absent_when_empty() {
        let snapshots = vec![snap(100, &["src/a.rs"], &[])];
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[], &[]);
        let report = format_epistemic_report(
            &snapshots,
            &ranking,
            &NeverForecastGroups::default(),
            &ExperimentFamilies::default(),
        );
        assert!(
            !report.contains("never forecast"),
            "no unforecast files → no section, got: {report}"
        );
    }

    // ---- too-new state (sibling of #744, one layer over) ----

    /// Snapshot timestamps in ascending order, one per day of July 2026.
    fn ts_list(days: &[u32]) -> Vec<String> {
        days.iter()
            .map(|d| format!("2026-07-{d:02}T00:00:00Z"))
            .collect()
    }

    #[test]
    fn test_forecast_opportunities_table() {
        let snaps = ts_list(&[1, 5, 10, 20]);
        let cases: &[(Option<&str>, Option<usize>, &str)] = &[
            // Older than every snapshot → every snapshot was a chance.
            (Some("2026-06-01T00:00:00Z"), Some(4), "older than all"),
            // Added after all but the last → exactly one chance.
            (Some("2026-07-15T00:00:00Z"), Some(1), "added late"),
            // Added after the last snapshot → no chance at all.
            (Some("2026-07-25T00:00:00Z"), Some(0), "newer than all"),
            // Strictly greater: a snapshot at the same instant is not a chance.
            (
                Some("2026-07-20T00:00:00Z"),
                Some(0),
                "equal ts is not after",
            ),
            // git's %aI shape with an offset, not a Z — same instant as 10:00Z
            // on the 15th, so still exactly one later snapshot.
            (Some("2026-07-15T12:00:00+02:00"), Some(1), "offset shape"),
            // Unknown age stays unknown — never a zero.
            (None, None, "no add-date"),
            (Some(""), None, "empty add-date"),
            (Some("not a date"), None, "unparseable add-date"),
        ];
        for (added, want, label) in cases {
            assert_eq!(
                forecast_opportunities(*added, &snaps),
                *want,
                "case: {label}"
            );
        }
    }

    #[test]
    fn test_forecast_opportunities_ignores_unreadable_snapshot_ts() {
        // A timestamp I cannot read is not one I can show to postdate the file.
        let snaps = vec![
            "2026-07-10T00:00:00Z".to_string(),
            "garbage".to_string(),
            String::new(),
        ];
        assert_eq!(
            forecast_opportunities(Some("2026-07-01T00:00:00Z"), &snaps),
            Some(1)
        );
    }

    #[test]
    fn test_old_file_with_many_later_snapshots_stays_dark() {
        let snapshots: Vec<ParsedSnapshot> =
            (1..=6).map(|d| snap(d, &["src/seen.rs"], &[])).collect();
        let never = never_forecast_files(&snapshots, &scores(&[("src/old.rs", 4.0)]), &[], &|_| {
            Some("2020-01-01T00:00:00Z".to_string())
        });
        assert_eq!(never.dark.len(), 1, "the columns had six real chances");
        assert_eq!(never.dark[0].path, "src/old.rs");
        assert!(never.too_new.is_empty());
    }

    #[test]
    fn test_file_added_after_all_but_one_snapshot_is_too_new() {
        let snapshots: Vec<ParsedSnapshot> =
            (1..=6).map(|d| snap(d, &["src/seen.rs"], &[])).collect();
        // snap(d) carries ts 2026-07-{d+1}; added just before the last one.
        let last = snapshots.last().unwrap().ts.clone();
        let added = "2026-07-06T12:00:00Z".to_string();
        assert!(added < last, "fixture: one snapshot must postdate the add");
        let never =
            never_forecast_files(&snapshots, &scores(&[("src/fresh.rs", 4.0)]), &[], &|_| {
                Some(added.clone())
            });
        assert!(never.dark.is_empty(), "a file this young is not evidence");
        assert_eq!(never.too_new.len(), 1);
        assert_eq!(never.too_new[0].path, "src/fresh.rs");
        assert_eq!(never.too_new[0].opportunities, 1);
        assert_eq!(
            never.too_new[0].added, "2026-07-06",
            "date part for display"
        );
    }

    #[test]
    fn test_unknown_add_date_is_dark_not_too_new() {
        // An unknown must never be quietly promoted into the comfortable
        // bucket (Day 144). No resolver answer → unchanged behaviour.
        let snapshots = vec![snap(1, &["src/seen.rs"], &[])];
        let never = never_forecast_files(&snapshots, &scores(&[("src/x.rs", 4.0)]), &[], &|_| None);
        assert_eq!(never.dark.len(), 1, "unknown age stays dark");
        assert!(never.too_new.is_empty(), "unknown age is not too-new");
        assert_eq!(
            never.age_unobservable, 1,
            "#819: still dark, but the unknown is now counted rather than absorbed"
        );
    }

    #[test]
    fn test_age_unobservable_counts_only_the_unmeasurable_dark_files() {
        // Both directions in one fixture (a discriminator tested only on the
        // side that fires is vacuous green): one file whose age nothing can
        // observe, one whose age is observed and genuinely old. Both land in
        // `dark`; only the first is counted.
        let snapshots: Vec<ParsedSnapshot> =
            (1..=6).map(|d| snap(d, &["src/seen.rs"], &[])).collect();
        let never = never_forecast_files(
            &snapshots,
            &scores(&[("src/unknown.rs", 5.0), ("src/old.rs", 4.0)]),
            &[],
            &|p| {
                if p == "src/old.rs" {
                    Some("2020-01-01T00:00:00Z".to_string())
                } else {
                    None
                }
            },
        );
        assert_eq!(never.dark.len(), 2, "#819 reports, it does not reclassify");
        assert_eq!(
            never.age_unobservable, 1,
            "only the file whose age could not be observed is counted"
        );
    }

    #[test]
    fn test_unparseable_add_date_counts_as_unobservable() {
        // A date that does not parse is not a measurement either — the
        // resolver spoke, but nothing usable came back.
        let snapshots = vec![snap(1, &["src/seen.rs"], &[])];
        let never = never_forecast_files(&snapshots, &scores(&[("src/x.rs", 4.0)]), &[], &|_| {
            Some("not-a-date".to_string())
        });
        assert_eq!(never.dark.len(), 1);
        assert_eq!(never.age_unobservable, 1);
    }

    #[test]
    fn test_studied_file_with_unobservable_age_is_not_counted() {
        // Classification order is studied → too_new → dark, so a studied file
        // never reaches the age question. Counting one there would report an
        // unknown that changed nothing.
        let snapshots = vec![snap(1, &["src/seen.rs"], &[])];
        let visits = vec![crate::commands_risk_epistemic::ExperimentVisit {
            path: "src/lit.rs".to_string(),
            day: 170,
            state: StudyState::Graded("hit".to_string()),
        }];
        let never = never_forecast_files(
            &snapshots,
            &scores(&[("src/lit.rs", 4.0)]),
            &visits,
            &|_| None,
        );
        assert_eq!(never.studied.len(), 1);
        assert!(never.dark.is_empty());
        assert_eq!(
            never.age_unobservable, 0,
            "the age question was never asked for this file"
        );
    }

    #[test]
    fn test_partially_studied_file_is_annotated_not_called_graded() {
        // #839, second consumer. `latest_study_state_by_path` has two readers
        // and wiring one is the "two doors, one policy, one deaf" shape.
        // A partial round still counts as "not dark" here — the file was
        // forecast-less for other reasons — it just must not wear the word
        // "graded" unqualified, exactly as `VisitedUngraded` must not.
        let snapshots = vec![snap(1, &["src/seen.rs"], &[])];
        let visits = vec![crate::commands_risk_epistemic::ExperimentVisit {
            path: "src/safety.rs".to_string(),
            day: 179,
            state: StudyState::PartiallyGraded {
                summary: "1 hit / 3 miss".to_string(),
                scope: "three named surfaces only".to_string(),
            },
        }];
        let never = never_forecast_files(
            &snapshots,
            &scores(&[("src/safety.rs", 4.0)]),
            &visits,
            &|_| None,
        );
        assert_eq!(never.studied.len(), 1, "not dark: a round did name it");
        assert!(never.dark.is_empty());
        let note = &never.studied[0].note;
        assert!(note.starts_with("partial study"), "{note}");
        assert!(note.contains("day 179"), "{note}");
        assert!(
            !note.contains("studied by graded experiment (day "),
            "must not read as a whole-file study: {note}"
        );
    }

    #[test]
    fn test_studied_wins_over_too_new() {
        // Precedence pin: studied is checked first, so a file that is both
        // studied and brand new keeps the stronger, truer annotation.
        let snapshots = vec![snap(1, &["src/seen.rs"], &[])];
        let visits = vec![crate::commands_risk_epistemic::ExperimentVisit {
            path: "src/both.rs".to_string(),
            day: 166,
            state: StudyState::Graded("hit".to_string()),
        }];
        let never = never_forecast_files(
            &snapshots,
            &scores(&[("src/both.rs", 4.0)]),
            &visits,
            &|_| Some("2026-07-28T00:00:00Z".to_string()),
        );
        assert_eq!(never.studied.len(), 1, "studied group wins");
        assert!(never.too_new.is_empty());
        assert!(never.dark.is_empty());
        assert!(never.studied[0]
            .note
            .starts_with("studied by graded experiment"));
    }

    #[test]
    fn test_empty_snapshots_manufacture_no_too_new_group() {
        let never = never_forecast_files(&[], &scores(&[("src/a.rs", 5.0)]), &[], &|_| {
            Some("2026-07-28T00:00:00Z".to_string())
        });
        assert!(never.dark.is_empty());
        assert!(never.studied.is_empty());
        assert!(
            never.too_new.is_empty(),
            "no snapshots → no claim of any kind"
        );
    }

    #[test]
    fn test_too_new_block_does_not_collide_with_the_trajectory_parser() {
        // scripts/extract_trajectory.py hard-stops collecting on the substring
        // "never forecast" and matches dark rows with `^\s*◦\s+(\S+)\s+\(risk`.
        // The too-new block must be invisible to both, and must not wear the
        // studied group's ▪ either.
        let snapshots = vec![snap(1, &["src/seen.rs"], &[])];
        let never =
            never_forecast_files(&snapshots, &scores(&[("src/fresh.rs", 4.0)]), &[], &|_| {
                Some("2026-07-28T00:00:00Z".to_string())
            });
        assert_eq!(never.too_new.len(), 1, "fixture must produce the group");
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[], &[]);
        let report =
            format_epistemic_report(&snapshots, &ranking, &never, &ExperimentFamilies::default());
        let header = report
            .lines()
            .map(|l| {
                l.replace(RESET.0, "")
                    .replace(DIM.0, "")
                    .replace(YELLOW.0, "")
            })
            .find(|l| l.contains("too young to judge"))
            .expect("too-new header must render");
        assert!(
            !header.contains("never forecast"),
            "header must not trip EPISTEMIC_NEVER_FORECAST_RE: {header:?}"
        );
        for line in report.lines() {
            let plain = line
                .replace(RESET.0, "")
                .replace(DIM.0, "")
                .replace(YELLOW.0, "");
            if plain.contains("src/fresh.rs") {
                let t = plain.trim_start();
                assert!(
                    t.starts_with('▫'),
                    "too-new rows use ▫, never ◦ or ▪: {plain:?}"
                );
            }
        }
        assert!(
            report.contains("needs ≥5 before absence means anything"),
            "the threshold must be stated in-band, got: {report}"
        );
    }

    // ---- shallow-clone age suppression (Day 166) ----

    #[test]
    fn test_shallow_boundary_hides_age_table() {
        const ROOT: &str = "abc1234def5678901234567890abcdef12345678";
        const OTHER: &str = "99998888777766665555444433332222111100ff";
        let roots = vec![ROOT.to_string()];
        let two_roots = vec![OTHER.to_string(), ROOT.to_string()];

        let cases: &[(bool, &str, &[String], bool, &str)] = &[
            // The defect this exists for: a shallow clone dates every surviving
            // pre-window file to the grafted boundary root.
            (true, ROOT, &roots, true, "shallow + add-sha is the root"),
            // A real in-window addition (a file created today) is a real
            // birthday even on a shallow clone.
            (
                true,
                OTHER,
                &roots,
                false,
                "shallow + add-sha is not a root",
            ),
            // A full clone's root commit IS the file's real birthday — this
            // must keep behaving exactly as it did before the helper existed.
            (
                false,
                ROOT,
                &roots,
                false,
                "full clone + add-sha is the root",
            ),
            // Either command may abbreviate; a 7-char prefix still matches.
            (true, "abc1234", &roots, true, "short add-sha prefixes root"),
            (
                true,
                ROOT,
                &["abc1234".to_string()],
                true,
                "short root prefixes add-sha",
            ),
            // Below the 7-char floor a lucky prefix must not excuse a
            // genuinely dark file.
            (
                true,
                "abc",
                &roots,
                false,
                "3-char add-sha is under the floor",
            ),
            (
                true,
                ROOT,
                &["abc".to_string()],
                false,
                "3-char root is under the floor",
            ),
            // Absence and whitespace are never a match, and never a panic.
            (true, "", &roots, false, "empty add-sha"),
            (true, ROOT, &[], false, "empty roots"),
            (true, "   ", &roots, false, "whitespace-only add-sha"),
            (
                true,
                ROOT,
                &["   ".to_string()],
                false,
                "whitespace-only root",
            ),
            (true, "", &[String::new()], false, "empty both sides"),
            // Trimming happens on both sides before comparing.
            (
                true,
                "  abc1234def5678901234567890abcdef12345678\n",
                &roots,
                true,
                "padded add-sha still matches",
            ),
            (
                true,
                ROOT,
                &[format!(" {ROOT} ")],
                true,
                "padded root still matches",
            ),
            // Any entry may match, not just the first.
            (true, ROOT, &two_roots, true, "matches a later root"),
        ];

        for (is_shallow, add_sha, graph_roots, want, label) in cases {
            assert_eq!(
                shallow_boundary_hides_age(*is_shallow, add_sha, graph_roots),
                *want,
                "case: {label}"
            );
        }
    }
}
