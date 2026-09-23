//! Price-drift alarm, general sweep — do the cost table's hardcoded `f64` rows
//! still agree with an external catalogue?
//!
//! # Why this exists, and what it is NOT
//!
//! `builtin_model_pricing` (the parent module) is ~200 lines of `f64` literals.
//! When a vendor reprices, yoyo keeps charging the old numbers in every `cost_usd`
//! it prints and every `/cost` a user reads, and **nothing in the repo notices**.
//! That is not hypothetical: one served model was priced 3.7x apart under two ids
//! (`deepseek-v4-flash` riding `deepseek-r1`'s row) until Day 204, found by a human
//! reading, not by a mechanism (#937).
//!
//! yoagent hit the identical class and shipped `tests/price_audit.rs` in response;
//! this file ports that *shape*, not that table.
//!
//! **A test that agrees with the table is vacuous against drift, because the table
//! is what drifts.** So the only thing that can find a reprice is a source the
//! table does not author — here, [models.dev](https://models.dev/api.json), an
//! MIT-licensed JSON catalogue.
//!
//! Two properties this instrument deliberately does not have:
//!
//! - **It never auto-updates the table.** A failure is a *drift alarm* that sends
//!   a human to the vendor's own pricing page. models.dev is community-maintained
//!   and therefore not authoritative; if the two disagree, the answer is "go read
//!   the vendor", never "copy the catalogue".
//! - **It does not fold states together.** A row the catalogue carries that yoyo
//!   cannot price is `unpriced` — never a match and never a silent drop. That is
//!   the difference between a coverage gap and a clean audit.
//!
//! # How to run it
//!
//! ```text
//! cargo test audit_table_against_models_dev -- --ignored --nocapture
//! ```
//!
//! It is `#[ignore]`d because it needs the network, so it never runs in CI; the
//! release checklist (`skills/release/SKILL.md`) is what makes it fire before a
//! publish. Everything else in this file is offline and runs on every `cargo test`.

use super::{model_pricing_overrides, model_pricing_with};

/// The four rates yoyo tracks per model, in yoyo's own order:
/// `(input, cache_write, cache_read, output)` per MTok. Named because the tuple
/// appears in every signature here and an anonymous `(f64, f64, f64, f64)` at a
/// call site is four numbers whose order nobody can check by reading.
type Rates = (f64, f64, f64, f64);

/// The table's price for a yoyo model id, with no user overrides — exactly what
/// a run with no `[model_pricing]` config sees. `model_pricing` itself is
/// test-private inside the parent module's `mod tests`, so this goes through the
/// public resolver rather than reaching into a sibling.
fn price_of(yoyo_id: &str) -> Option<Rates> {
    model_pricing_with(model_pricing_overrides(), yoyo_id)
}

/// [models.dev](https://github.com/anomalyco/models.dev)'s full catalogue, one JSON
/// document for every provider. Chosen over the alternatives because it carries
/// `cache_read` *and* `cache_write`, which the cost table needs and many aggregators omit.
const MODELS_DEV_URL: &str = "https://models.dev/api.json";

/// Two rates count as the same price when they are within this *relative* distance.
///
/// Relative rather than absolute, and 1% rather than 0: vendor pages round, and
/// catalogue aggregators disagree with each other in the third decimal (0.003 vs
/// 0.003625 for DeepSeek's cache read is a real pair — a 20.8% gap on a rate that
/// is a rounding artefact at the *third* decimal of a dollar). Exact equality
/// would therefore false-positive on rows that are not repricing at all, and an
/// alarm that cries on every run gets trained out of use — the failure mode where
/// the instrument is present and unread, which is strictly worse than absent.
///
/// 1% is tight enough that a real repricing (DeepSeek's flash row moving
/// 0.15 → 0.20 is +33%; the Day-204 bug was +267% on input) is never absorbed.
const REL_TOL: f64 = 0.01;

/// One row of the external catalogue, already reduced to the four rates yoyo tracks,
/// in **yoyo's own tuple order**: `(input, cache_write, cache_read, output)` per MTok.
///
/// `provider` is carried so the network test can resolve a catalogue id back to a
/// yoyo id — the same id string can exist under two catalogue providers.
#[derive(Debug, Clone, PartialEq)]
struct CatalogueRow {
    provider: String,
    id: String,
    price: Rates,
}

impl CatalogueRow {
    /// The join key between the two sides: `provider/id`.
    ///
    /// Both halves are load-bearing. Joining on the bare id would work *today*
    /// (measured 2026-09-23 against the live catalogue: zero ids appear under
    /// more than one of the nine mapped providers) and would break silently the
    /// day two of them share a model name — comparing one provider's model
    /// against another provider's price and reporting the difference as drift.
    /// A join that happens to hold is not an enforcement, so the key carries the
    /// provider by construction and the day it matters is a miss, not a wrong
    /// answer.
    fn key(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }
}

/// What an audit found. Nothing is folded: a row yoyo does not price is `unpriced`,
/// never a match and never a silent drop.
#[derive(Debug, Default, PartialEq)]
struct AuditReport {
    /// Rows present in BOTH sides.
    compared: usize,
    /// Of those, within [`REL_TOL`].
    matched: usize,
    /// `(id, ours, theirs)` — named, because "drift detected" without the id and
    /// both readings is a message nobody can act on. A row lands here when a
    /// *rate* moved: input, cache write or output.
    drifted: Vec<(String, Rates, Rates)>,
    /// Rows where the **only** cell that differs is `cache_read`, which the table
    /// deliberately leaves at `0.0` for providers whose caching it does not model
    /// (the parent module documents that convention).
    ///
    /// Reported as its own count rather than folded into `drifted`, and neither
    /// hidden nor failed. Folding it in buries a real repricing among rows whose
    /// input/output are byte-identical — measured 2026-09-23, that was 11 of the
    /// 20 rows that fired on the first run. Hiding it would be worse: when a
    /// provider serves a cached prompt, yoyo genuinely under-reports the cost.
    /// So the count is the honest statement that the table is short a rate the
    /// provider does charge, and it is printed on every run.
    cache_read_only: Vec<String>,
    /// Catalogue ids yoyo cannot price at all.
    unpriced: Vec<String>,
}

/// Are two rates the same price, to a relative tolerance?
///
/// An exact zero on both sides is a match (yoyo's `cache_write` is legitimately
/// `0.0` for providers that do not bill cache writes, and so is an absent
/// catalogue cell); zero on one side only is a drift unless the other side is
/// also within tolerance of zero, which it cannot be by construction.
fn within_rel_tolerance(a: f64, b: f64, rel_tol: f64) -> bool {
    if a == b {
        return true;
    }
    let scale = a.abs().max(b.abs());
    if scale == 0.0 {
        return true;
    }
    (a - b).abs() / scale <= rel_tol
}

/// All four cells must agree. A single-cell comparator would report a row as
/// matched while its output rate has doubled.
fn cells_within(ours: Rates, theirs: Rates, rel_tol: f64) -> bool {
    within_rel_tolerance(ours.0, theirs.0, rel_tol)
        && within_rel_tolerance(ours.1, theirs.1, rel_tol)
        && within_rel_tolerance(ours.2, theirs.2, rel_tol)
        && within_rel_tolerance(ours.3, theirs.3, rel_tol)
}

/// True when the two readings agree on input, cache write and output, and differ
/// only on `cache_read` — i.e. the one cell this table intentionally leaves at
/// `0.0` for providers whose caching it does not model.
///
/// Stated as "the other three agree" rather than "cache_read differs" on purpose:
/// a row that also moved its input or output rate is a repricing and must not be
/// absorbed here.
fn only_cache_read_differs(ours: Rates, theirs: Rates, rel_tol: f64) -> bool {
    within_rel_tolerance(ours.0, theirs.0, rel_tol)
        && within_rel_tolerance(ours.1, theirs.1, rel_tol)
        && within_rel_tolerance(ours.3, theirs.3, rel_tol)
        && !within_rel_tolerance(ours.2, theirs.2, rel_tol)
}

/// The whole comparison, with the network removed — feed it two lists and it says
/// what disagrees. Pure, so the offline tests below are the real specification of
/// the alarm's behaviour and the network test is only its transport.
///
/// Both sides are keyed by [`CatalogueRow::key`] (`provider/id`); the `ours` side
/// arrives that way from [`ours_from_catalogue`].
fn compare_catalogue(
    ours: &[(String, Rates)],
    catalogue: &[CatalogueRow],
    rel_tol: f64,
) -> AuditReport {
    let mut report = AuditReport::default();
    for row in catalogue {
        let key = row.key();
        match ours.iter().find(|(id, _)| *id == key) {
            // Not priced by yoyo at all: a coverage gap, reported as one, and
            // deliberately NOT counted as compared — counting it either way would
            // let a fully-priced-looking audit contain zero real comparisons.
            None => report.unpriced.push(key),
            Some((_, our_price)) => {
                report.compared += 1;
                if cells_within(*our_price, row.price, rel_tol) {
                    report.matched += 1;
                } else if only_cache_read_differs(*our_price, row.price, rel_tol) {
                    // Every other rate agrees, so this is the known
                    // cache-read coverage gap, not a repricing.
                    report.cache_read_only.push(key);
                } else {
                    report.drifted.push((key, *our_price, row.price));
                }
            }
        }
    }
    report
}

// ---------------------------------------------------------------------------
// Catalogue id -> yoyo id
// ---------------------------------------------------------------------------

/// Providers yoyo prices *and* models.dev keys by the same name.
///
/// Deliberately a short explicit list rather than a sweep of all 223 catalogue
/// providers: an aggregator provider (`openrouter`) keys models by a *prefixed*
/// id, and several yoyo providers (`ollama`, `bedrock`, `github`) have no
/// one-to-one catalogue counterpart at all. Sweeping everything would compare
/// ids that name different things — the confident-wrong-diagnosis shape this
/// repo keeps recording.
const PROVIDER_KEYS: &[&str] = &[
    "anthropic",
    "openai",
    "google",
    "deepseek",
    "xai",
    "mistral",
    "groq",
    "cerebras",
    "zai",
];

/// Catalogue ids that name the *same model* as a differently-spelled yoyo id.
///
/// Small and explicit on purpose. Every entry is a claim that two spellings are
/// one model, and a wrong entry would compare two different models' prices and
/// call the difference drift — so this stays a hand-checked table rather than a
/// fuzzy matcher over ids.
const ID_ALIASES: &[(&str, &str)] = &[
    // Google names its Gemini 3 family `gemini-3-*`; yoyo's own provider table
    // ships `gemini-3.0-*`. Same model, two spellings.
    ("gemini-3-pro", "gemini-3.0-pro"),
    ("gemini-3-flash", "gemini-3.0-flash"),
];

/// Strip a release-date suffix and any `provider/` prefix from a catalogue id.
///
/// `claude-haiku-4-5-20251001` and `claude-haiku-4-5-2025-10-01` both name the
/// model yoyo ships as `claude-haiku-4-5`, and the catalogue carries both
/// spellings across providers. Slice-free: everything is found with
/// `rsplit_once('-')`, so no byte index can land inside a multi-byte character
/// (the #250 rule — `&id[..n]` panics on a non-ASCII id).
fn normalize_catalogue_id(id: &str) -> &str {
    let id = match id.rsplit_once('/') {
        Some((_, tail)) => tail,
        None => id,
    };
    let Some((head, tail)) = id.rsplit_once('-') else {
        return id;
    };
    // `-YYYYMMDD`
    if tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit()) {
        return head;
    }
    // `-YYYY-MM-DD` — the tail is the day, and `head` ends `-YYYY-MM`.
    if tail.len() == 2 && tail.bytes().all(|b| b.is_ascii_digit()) {
        if let Some((h2, t2)) = head.rsplit_once('-') {
            if t2.len() == 2 && t2.bytes().all(|b| b.is_ascii_digit()) {
                if let Some((h3, t3)) = h2.rsplit_once('-') {
                    if t3.len() == 4 && t3.bytes().all(|b| b.is_ascii_digit()) {
                        return h3;
                    }
                }
            }
        }
    }
    id
}

/// The yoyo model id whose arm prices this catalogue row, or `None` when yoyo
/// does not ship that model at all.
///
/// Only ids yoyo *ships* (`known_models_for_provider`) are compared. That is the
/// honest population: yoyo's lookup is a chain of `contains` arms, so comparing
/// the `f64` a fuzzy arm happens to produce against an arbitrary catalogue row
/// would manufacture drift out of a substring.
fn yoyo_id_for(provider: &str, catalogue_id: &str) -> Option<&'static str> {
    let normalized = normalize_catalogue_id(catalogue_id);
    if let Some((_, yoyo_id)) = ID_ALIASES.iter().find(|(cat, _)| *cat == normalized) {
        return Some(yoyo_id);
    }
    crate::providers::known_models_for_provider(provider)
        .iter()
        .copied()
        .find(|id| *id == normalized)
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

/// "Could not check" must not read as "checked; clean" — the standing repo rule
/// the harness lint states for `evolve.sh`, applied here: a failed fetch is a
/// loud, named failure.
fn audit_did_not_run(err: &str) -> String {
    format!(
        "PRICE DRIFT AUDIT DID NOT RUN — the pricing table is UNVERIFIED against {MODELS_DEV_URL}, \
         which is not the same as checked and clean.\n\
         fetch/parse failure: {err}\n\
         Fix the network or the payload, then re-run: \
         cargo test audit_table_against_models_dev -- --ignored --nocapture"
    )
}

/// Fetch the catalogue. `curl` rather than a new dependency: this runs by hand
/// before a release, and `serde_json` is already what parses the reply.
fn fetch_models_dev_json() -> Result<String, String> {
    let out = std::process::Command::new("curl")
        .args(["-sSL", "--max-time", "60", "--fail", MODELS_DEV_URL])
        .output()
        .map_err(|e| format!("curl could not be spawned: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "curl exited {} ({})",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("curl's output was not valid UTF-8: {e}"))
}

/// What [`parse_catalogue`] read: the comparable rows, plus the count of
/// catalogue entries that publish no `cost` object at all.
///
/// The second number is a **state, not a skip**. models.dev carries non-text
/// entries (image and TTS models) whose only costing is per image, and they are
/// simply not comparable to a per-MTok rate — measured 2026-09-23 against the
/// live catalogue, `openai.chatgpt-image-latest` is one. Dropping them silently
/// would shrink the denominator invisibly; failing on them would make the audit
/// unrunnable. So they are counted and printed, and the count is what a reader
/// checks against their own expectation of the catalogue.
#[derive(Debug)]
struct ParsedCatalogue {
    rows: Vec<CatalogueRow>,
    /// Entries in the swept providers that carry no `cost` object.
    no_cost_published: usize,
}

/// Parse the catalogue down to the rows in providers yoyo prices.
///
/// `reasoning` is deliberately neither read nor compared: for DeepSeek it
/// duplicates `output`, and one fact compared twice is two chances to disagree
/// over one number. A missing *cell* is filled with `0.0` rather than treated as
/// a structure change — and a renamed cell is caught loudly rather than
/// silently, because it makes the cell read `0.0` and the row then drifts.
fn parse_catalogue(json: &str) -> Result<ParsedCatalogue, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("models.dev payload was not JSON: {e}"))?;
    let root = root
        .as_object()
        .ok_or_else(|| "models.dev payload was not a JSON object".to_string())?;

    let mut rows = Vec::new();
    let mut no_cost_published = 0usize;
    for provider in PROVIDER_KEYS {
        let Some(models) = root
            .get(*provider)
            .and_then(|p| p.get("models"))
            .and_then(|m| m.as_object())
        else {
            // A renamed provider key is a FAILED audit, not an empty one — the
            // silent-drop direction. The caller turns this into a loud failure.
            return Err(format!(
                "models.dev payload has no `{provider}.models` object — a moved, renamed or \
                 dropped provider key would silently shrink this audit to nothing"
            ));
        };
        for (id, entry) in models {
            let Some(cost) = entry.get("cost").and_then(|c| c.as_object()) else {
                // Priced per image / per second, or not priced at all: not a
                // per-MTok row and not comparable. Counted, never dropped.
                no_cost_published += 1;
                continue;
            };
            let cell =
                |name: &str| -> f64 { cost.get(name).and_then(|v| v.as_f64()).unwrap_or(0.0) };
            rows.push(CatalogueRow {
                provider: (*provider).to_string(),
                id: id.clone(),
                // yoyo's order: (input, cache_write, cache_read, output).
                price: (
                    cell("input"),
                    cell("cache_write"),
                    cell("cache_read"),
                    cell("output"),
                ),
            });
        }
    }
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(ParsedCatalogue {
        rows,
        no_cost_published,
    })
}

/// Build the yoyo side of the comparison: every catalogue row this repo can
/// actually price, keyed by the *catalogue* id so the two sides join on one key.
fn ours_from_catalogue(catalogue: &[CatalogueRow]) -> Vec<(String, Rates)> {
    catalogue
        .iter()
        .filter_map(|row| {
            let yoyo_id = yoyo_id_for(&row.provider, &row.id)?;
            price_of(yoyo_id).map(|price| (row.key(), price))
        })
        .collect()
}

/// The alarm. Network-only, `#[ignore]`d, **never in CI**.
///
/// Read it before a release: paste the summary line into the release notes, and
/// on any `drifted > 0` go read the vendor's pricing page *before publishing*.
/// The number is the alarm; the decision is a human's.
#[test]
#[ignore = "network: fetches models.dev; run before a release, never in CI"]
fn audit_table_against_models_dev() {
    let json = fetch_models_dev_json().unwrap_or_else(|e| panic!("{}", audit_did_not_run(&e)));
    let ParsedCatalogue {
        rows: catalogue,
        no_cost_published,
    } = parse_catalogue(&json).unwrap_or_else(|e| panic!("{}", audit_did_not_run(&e)));

    // ANTI-VACUOUS: an empty payload must fail loudly rather than pass green over
    // nothing. This is the failure the whole repo keeps recording.
    assert!(
        !catalogue.is_empty(),
        "models.dev returned zero rows for the providers yoyo prices — a restructured payload \
         is a FAILED audit, not a clean one"
    );

    let ours = ours_from_catalogue(&catalogue);
    let report = compare_catalogue(&ours, &catalogue, REL_TOL);

    println!("\n{:<44} {:<28} {:<28} state", "id", "yoyo", "models.dev");
    println!("{}", "-".repeat(120));
    for row in &catalogue {
        let Some(yoyo_id) = yoyo_id_for(&row.provider, &row.id) else {
            continue;
        };
        let Some(our) = price_of(yoyo_id) else {
            continue;
        };
        let state = if cells_within(our, row.price, REL_TOL) {
            "match"
        } else {
            "DRIFT"
        };
        println!(
            "{:<44} {:<28} {:<28} {}",
            row.key(),
            format!("{:?}", our),
            format!("{:?}", row.price),
            state
        );
    }

    // Coverage gaps printed with counts, never silently skipped: a mapping the
    // instrument cannot see is a hole it cannot report. Bounded sample so a
    // few-hundred-row catalogue stays readable.
    const UNPRICED_SAMPLE: usize = 12;
    println!(
        "\nprice audit: {} catalogue row(s) yoyo cannot price (coverage gap, not a failure). \
         First {}: {:?}",
        report.unpriced.len(),
        UNPRICED_SAMPLE.min(report.unpriced.len()),
        &report.unpriced[..UNPRICED_SAMPLE.min(report.unpriced.len())]
    );
    println!(
        "price audit: {} row(s) differ ONLY in `cache_read` — the cell the table leaves at 0.0 \
         for providers whose caching it does not model; every other rate agrees: {:?}",
        report.cache_read_only.len(),
        report.cache_read_only
    );
    println!(
        "price audit: {no_cost_published} catalogue entry/entries publish no `cost` object \
         (priced per image or per second) and are not comparable to a per-MTok rate"
    );
    println!(
        "price audit: SUMMARY compared {}, matched {}, drifted {}, cache_read_only {}, unpriced {} \
         of {} catalogue rows (rel_tol {}%)",
        report.compared,
        report.matched,
        report.drifted.len(),
        report.cache_read_only.len(),
        report.unpriced.len(),
        catalogue.len(),
        REL_TOL * 100.0
    );

    // A run that compared nothing is the vacuous green this file exists to refuse.
    assert!(
        report.compared > 0,
        "the audit compared ZERO rows — a shrunk catalogue or a broken id mapping is a FAILED \
         audit, not a clean one (unpriced {} of {})",
        report.unpriced.len(),
        catalogue.len()
    );

    assert!(
        report.drifted.is_empty(),
        "PRICE DRIFT ALARM — {} row(s) disagree between the pricing table and models.dev:\n  {}\n\
         This is a drift alarm, not a gate: read the vendor's own pricing page and decide. \
         Do NOT auto-patch the pricing constants, and do NOT edit this test's expectation to \
         agree — a test that agrees with the table is vacuous against drift, because the table \
         is what drifted (#937).",
        report.drifted.len(),
        report
            .drifted
            .iter()
            .map(|(id, ours, theirs)| format!("{id}: table {ours:?} vs models.dev {theirs:?}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Offline tests — the comparator's real specification
// ---------------------------------------------------------------------------

/// A fixture catalogue: three rows, each with a distinct price shape.
fn fixture_catalogue() -> Vec<CatalogueRow> {
    vec![
        row("deepseek", "alpha", (1.0, 0.0, 0.1, 2.0)),
        row("deepseek", "beta", (3.0, 0.0, 0.0, 15.0)),
        row("anthropic", "gamma", (5.0, 6.25, 0.5, 25.0)),
    ]
}

/// The yoyo side that agrees with [`fixture_catalogue`] exactly, keyed the way
/// the comparator joins.
///
/// Built from [`CatalogueRow::key`] directly rather than through
/// [`ours_from_catalogue`] on purpose: the resolver only prices ids this repo
/// actually ships, and these fixtures are deliberately *not* real models — the
/// comparator's specification must not depend on the table whose drift it detects.
fn fixture_ours() -> Vec<(String, Rates)> {
    fixture_catalogue()
        .into_iter()
        .map(|row| (row.key(), row.price))
        .collect()
}

/// A catalogue row, spelled the way every fixture below spells one.
fn row(provider: &str, id: &str, price: Rates) -> CatalogueRow {
    CatalogueRow {
        provider: provider.to_string(),
        id: id.to_string(),
        price,
    }
}

/// **Requirement 1 — anti-vacuous first.** A comparator over two empty lists
/// reports "no drift" and has checked nothing; the repo has recorded that
/// exact failure shape many times. `compared > 0` is asserted *before* anything
/// about `drifted`, so a version of `compare_catalogue` that silently skips
/// every row cannot pass this file.
#[test]
fn comparator_is_anti_vacuous_before_it_is_anything_else() {
    let report = compare_catalogue(&fixture_ours(), &fixture_catalogue(), REL_TOL);
    assert!(
        report.compared > 0,
        "the comparator compared nothing — every other assertion in this file would be vacuous"
    );
    assert_eq!(report.compared, 3, "all three fixture rows are priced");
    assert_eq!(report.matched, 3);
    assert!(report.drifted.is_empty());
    assert!(report.unpriced.is_empty());
}

/// **Requirement 2 — drift is named, exactly.** Perturb ONE row beyond
/// tolerance: exactly that id drifts, and every other row is still matched.
#[test]
fn drift_names_the_perturbed_row_and_only_that_row() {
    let mut ours = fixture_ours();
    // beta: output 15.0 -> 20.0, a +25% repricing, far outside REL_TOL.
    ours[1].1 .3 = 20.0;

    let report = compare_catalogue(&ours, &fixture_catalogue(), REL_TOL);
    assert_eq!(report.compared, 3);
    assert_eq!(report.drifted.len(), 1, "exactly one row moved");
    assert_eq!(
        report.drifted[0].0, "deepseek/beta",
        "and it is the perturbed row, named by the key both sides join on"
    );
    assert_eq!(report.drifted[0].1, (3.0, 0.0, 0.0, 20.0), "ours, named");
    assert_eq!(report.drifted[0].2, (3.0, 0.0, 0.0, 15.0), "theirs, named");
    assert_eq!(
        report.matched, 2,
        "the other two rows must still be matched, not swallowed by the drift"
    );
}

/// **Requirement 3 — near-miss guard, whole value.** A row INSIDE tolerance
/// produces no drift, and the whole report is compared with `assert_eq!` against
/// a full literal — not a `contains`, which would pass on a report that also
/// drifted something else.
///
/// A tolerance that swallows everything is this comparator's own blind spot, so
/// the second half asserts the same 0.4% gap DOES drift once the tolerance is
/// tightened to 0 — i.e. the tolerance is what absorbed it, not the comparator
/// ignoring the cell.
#[test]
fn a_row_inside_tolerance_does_not_drift_but_the_boundary_is_what_saved_it() {
    let mut ours = fixture_ours();
    // gamma input 5.0 -> 5.02: 0.4% off, inside the default 1%.
    ours[2].1 .0 = 5.02;

    let report = compare_catalogue(&ours, &fixture_catalogue(), REL_TOL);
    assert_eq!(
        report,
        AuditReport {
            compared: 3,
            matched: 3,
            drifted: Vec::new(),
            cache_read_only: Vec::new(),
            unpriced: Vec::new(),
        },
        "a 0.4% difference is catalogue rounding, not a repricing"
    );

    // The same input, with the tolerance removed: the cell IS compared.
    let strict = compare_catalogue(&ours, &fixture_catalogue(), 0.0);
    assert_eq!(
        strict.drifted.len(),
        1,
        "with rel_tol = 0 the 0.4% gap must surface — otherwise the tolerance is not \
         what absorbed it and the previous assertion proves nothing"
    );
    assert_eq!(strict.drifted[0].0, "anthropic/gamma");
}

/// The cache-read-only split, pinned in both directions: a row whose *only*
/// difference is `cache_read` is classified as the coverage gap, and a row that
/// ALSO moved a real rate is NOT absorbed by it.
///
/// The second half is the load-bearing one — a classifier written as
/// "cache_read differs" instead of "the other three agree" would pass the first
/// assertion and silently swallow every repricing on a provider that also has
/// unmodelled caching, which is most of them.
#[test]
fn cache_read_only_is_a_separate_state_and_cannot_absorb_a_real_repricing() {
    // gpt-4o-shaped: input/output identical, cache_read 0.0 vs 1.25.
    let cat = vec![row("openai", "gpt-4o", (2.5, 0.0, 1.25, 10.0))];
    let ours = vec![(cat[0].key(), (2.5, 0.0, 0.0, 10.0))];
    let report = compare_catalogue(&ours, &cat, REL_TOL);
    assert_eq!(report.matched, 0, "it is not a match");
    assert!(
        report.drifted.is_empty(),
        "and it is not rate drift — the other three cells agree"
    );
    assert_eq!(report.cache_read_only, vec!["openai/gpt-4o".to_string()]);

    // The same row with its OUTPUT rate also moved: real drift, not absorbed.
    let cat = vec![row("openai", "gpt-4o", (2.5, 0.0, 1.25, 15.0))];
    let ours = vec![(cat[0].key(), (2.5, 0.0, 0.0, 10.0))];
    let report = compare_catalogue(&ours, &cat, REL_TOL);
    assert_eq!(
        report.drifted.len(),
        1,
        "a row that moved output AND has unmodelled caching is a repricing"
    );
    assert!(
        report.cache_read_only.is_empty(),
        "and must not be classified into the coverage-gap bucket"
    );
}

/// **Requirement 4 — `unpriced` is a state, not a skip.** A catalogue id absent
/// from `ours` appears in `unpriced` and is NOT counted in `compared`. Folding it
/// into `compared` would inflate the audit; dropping it silently is how a
/// coverage gap becomes a clean audit.
#[test]
fn a_row_yoyo_cannot_price_is_unpriced_and_not_compared() {
    let mut catalogue = fixture_catalogue();
    catalogue.push(CatalogueRow {
        provider: "deepseek".to_string(),
        id: "delta".to_string(),
        price: (9.0, 0.0, 0.0, 9.0),
    });

    let report = compare_catalogue(&fixture_ours(), &catalogue, REL_TOL);
    assert_eq!(
        report.compared, 3,
        "the unpriced row must not be counted as compared"
    );
    assert_eq!(report.matched, 3);
    assert_eq!(report.unpriced, vec!["deepseek/delta".to_string()]);
    assert!(
        report.drifted.is_empty(),
        "an unpriced row is a coverage gap, not drift"
    );
}

/// **Requirement 5 — a real end-to-end row, offline.** The Day-204 fix, pinned
/// against silent regression: `deepseek-flash` and its own alias
/// `deepseek-v4-flash` are one served model and must resolve to one price tuple.
/// Costs no network.
#[test]
fn the_two_deepseek_flash_spellings_resolve_to_one_price() {
    let overrides = model_pricing_overrides();
    let canonical = model_pricing_with(overrides, "deepseek-flash")
        .expect("deepseek-flash is the id the evolve loop runs on; it must be priced");
    let alias = model_pricing_with(overrides, "deepseek-v4-flash")
        .expect("deepseek-v4-flash is the id `.yoyo.toml` names; it must be priced");
    assert_eq!(
        canonical, alias,
        "one served model priced twice is exactly the Day-204 defect (#937): the two ids must \
         never diverge again"
    );
    // Near-miss guard: the sibling the row was wrongly sharing before Day 204.
    assert_ne!(
        canonical,
        model_pricing_with(overrides, "deepseek-r1").expect("deepseek-r1 is priced"),
        "deepseek-r1 is a different model and must not share this row"
    );
}

/// **Requirement 6 — the tolerance is relative, and its reason is a fact about
/// the data rather than a preference.** Fixed absolute epsilon cannot express
/// "0.4% on a $5 rate" and "0.4% on a $0.003 rate" at once; this pins that the
/// comparison is a RATIO by using one gap at two scales.
#[test]
fn tolerance_is_relative_so_one_ratio_holds_at_two_price_scales() {
    let cheap = |ours: f64, theirs: f64| {
        let cat = vec![row("deepseek", "x", (theirs, 0.0, 0.0, theirs))];
        let ours = vec![(cat[0].key(), (ours, 0.0, 0.0, ours))];
        compare_catalogue(&ours, &cat, REL_TOL).drifted.len()
    };
    // ~0.4% at both scales: absorbed both times, because the test is a ratio.
    assert_eq!(cheap(5.0, 5.02), 0, "0.4% of $5 is not a repricing");
    assert_eq!(cheap(0.15, 0.1506), 0, "0.4% of $0.15 is not a repricing");
    // And a real repricing at the cheap end is NOT absorbed: 0.15 -> 0.20.
    assert_eq!(
        cheap(0.15, 0.20),
        1,
        "a +33% repricing on a cheap rate must still fire — the tolerance is a rounding \
         budget, not an amnesty"
    );
    // A rate that goes to zero is a drift, not an exact match on both sides.
    assert_eq!(cheap(0.003, 0.0), 1, "a rate zeroed out is a repricing");
}

/// The id normalisation, pinned with the spellings the catalogue actually carries
/// — including the two date forms, which is why `rsplit_once` chains rather than
/// one suffix check.
#[test]
fn catalogue_ids_normalise_to_the_ids_yoyo_ships() {
    assert_eq!(
        normalize_catalogue_id("claude-haiku-4-5-20251001"),
        "claude-haiku-4-5"
    );
    assert_eq!(
        normalize_catalogue_id("claude-haiku-4-5-2025-10-01"),
        "claude-haiku-4-5"
    );
    assert_eq!(
        normalize_catalogue_id("anthropic/claude-sonnet-4-5"),
        "claude-sonnet-4-5",
        "an aggregator-style provider prefix must not defeat the join"
    );
    assert_eq!(
        normalize_catalogue_id("claude-opus-5"),
        "claude-opus-5",
        "a version-numbered id is not a date suffix and must pass through byte-identically"
    );
    assert_eq!(
        normalize_catalogue_id("gpt-5.5"),
        "gpt-5.5",
        "a dotted minor version is not a date suffix either"
    );
}

/// The mapping is what decides the audit's population, so it gets its own pins:
/// a shipped id resolves, a date-suffixed catalogue spelling of it resolves, an
/// alias resolves, and an id yoyo does not ship does NOT — the last being the one
/// that keeps the sweep from manufacturing drift out of a fuzzy `contains` arm.
#[test]
fn the_mapping_covers_shipped_ids_and_refuses_unshipped_ones() {
    assert_eq!(
        yoyo_id_for("deepseek", "deepseek-flash"),
        Some("deepseek-flash")
    );
    assert_eq!(
        yoyo_id_for("anthropic", "claude-haiku-4-5-20251001"),
        Some("claude-haiku-4-5")
    );
    assert_eq!(
        yoyo_id_for("google", "gemini-3-pro"),
        Some("gemini-3.0-pro"),
        "the alias table exists for exactly this: two spellings of one model"
    );
    assert_eq!(
        yoyo_id_for("deepseek", "deepseek-v4-flash-vision-exp"),
        None,
        "a catalogue model yoyo does not ship is a coverage gap, not a comparison"
    );
    assert_eq!(yoyo_id_for("deepseek", "not-a-model-at-all"), None);
}

/// The `ours` builder is the join, and it must produce the priced rows and skip
/// the rest — the property the whole audit rests on. Asserted with a count
/// against a fixture, so it cannot pass by producing nothing.
#[test]
fn ours_from_catalogue_keeps_only_rows_this_repo_can_price() {
    let catalogue = vec![
        CatalogueRow {
            provider: "deepseek".to_string(),
            id: "deepseek-flash".to_string(),
            price: (0.15, 0.0, 0.003, 0.60),
        },
        CatalogueRow {
            provider: "deepseek".to_string(),
            id: "deepseek-v4-flash-vision-exp".to_string(),
            price: (0.15, 0.0, 0.003, 0.60),
        },
    ];
    let ours = ours_from_catalogue(&catalogue);
    assert_eq!(ours.len(), 1, "only the shipped id is on the yoyo side");
    assert_eq!(ours[0].0, "deepseek/deepseek-flash");
    assert_eq!(ours[0].1, (0.15, 0.0, 0.003, 0.60));

    // Anti-vacuous, and the coverage side: the unpriced row is reported as such.
    let report = compare_catalogue(&ours, &catalogue, REL_TOL);
    assert_eq!(report.compared, 1);
    assert_eq!(report.matched, 1);
    assert_eq!(
        report.unpriced,
        vec!["deepseek/deepseek-v4-flash-vision-exp".to_string()]
    );
}

/// A catalogue row keyed by a provider yoyo does not price must not resolve, and
/// the same id string under two providers is two rows — the reason `provider` is
/// carried on [`CatalogueRow`] at all.
#[test]
fn an_unmapped_provider_is_a_coverage_gap_and_provider_is_part_of_the_key() {
    assert_eq!(yoyo_id_for("openrouter", "deepseek-flash"), None);
    let catalogue = vec![
        row("deepseek", "deepseek-flash", (0.15, 0.0, 0.003, 0.60)),
        row("openrouter", "deepseek-flash", (0.15, 0.0, 0.003, 0.60)),
    ];
    let report = compare_catalogue(&ours_from_catalogue(&catalogue), &catalogue, REL_TOL);
    assert_eq!(report.compared, 1, "only the mapped provider is compared");
    assert_eq!(
        report.unpriced,
        vec!["openrouter/deepseek-flash".to_string()],
        "the same id under an unmapped provider is a separate, unpriced row"
    );

    // The composite key is what keeps those two rows apart: one bare-id join
    // would have compared the openrouter row against yoyo's deepseek price.
    assert_ne!(
        row("deepseek", "deepseek-flash", (0.15, 0.0, 0.003, 0.60)).key(),
        row("openrouter", "deepseek-flash", (0.15, 0.0, 0.003, 0.60)).key()
    );
}

/// The payload parser: the cost keys yoyo tracks are read, `reasoning` is
/// deliberately ignored (one fact compared twice), an absent cell is a zero
/// rather than a parse failure, and a missing provider is a loud error rather
/// than an empty list.
#[test]
fn the_payload_parser_reads_the_tracked_cells_and_refuses_a_missing_provider() {
    // Every mapped provider must be present, or the parse is a loud failure —
    // so the well-formed fixture carries all of them and only `deepseek` has rows.
    let mut payload = String::from("{");
    for (i, provider) in PROVIDER_KEYS.iter().enumerate() {
        if i > 0 {
            payload.push(',');
        }
        if *provider == "deepseek" {
            payload.push_str(
                r#""deepseek": { "models": {
                    "deepseek-flash": { "cost": { "input": 0.15, "output": 0.6,
                                                  "cache_read": 0.003, "reasoning": 0.6 } },
                    "deepseek-nocache": { "cost": { "input": 1.0, "output": 2.0 } }
                } }"#,
            );
        } else {
            payload.push_str(&format!(r#""{provider}": {{ "models": {{}} }}"#));
        }
    }
    payload.push('}');
    let parsed = parse_catalogue(&payload).expect("a well-formed payload parses");
    let rows = parsed.rows;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        parsed.no_cost_published, 0,
        "every fixture entry publishes a cost object"
    );
    let flash = rows.iter().find(|r| r.id == "deepseek-flash").unwrap();
    assert_eq!(
        flash.price,
        (0.15, 0.0, 0.003, 0.6),
        "tuple order is (input, cache_write, cache_read, output) and `reasoning` is not read"
    );
    let nocache = rows.iter().find(|r| r.id == "deepseek-nocache").unwrap();
    assert_eq!(nocache.price, (1.0, 0.0, 0.0, 2.0));

    // An entry with no `cost` object at all is COUNTED, not dropped and not a
    // hard failure: `chatgpt-image-latest` is the live example (2026-09-23).
    let with_imagery = payload.replace(
        r#""deepseek-nocache": { "cost": { "input": 1.0, "output": 2.0 } }"#,
        r#""deepseek-imagery": { "cost": null }"#,
    );
    let parsed = parse_catalogue(&with_imagery).expect("a row with no cost is not a parse error");
    assert_eq!(
        parsed.no_cost_published, 1,
        "the un-costed entry must be visible as a count, not vanish"
    );
    assert!(
        parsed.rows.iter().all(|r| r.id != "deepseek-imagery"),
        "and it must not be compared as if it had rates"
    );

    // A payload that has lost one of the providers this audit sweeps must fail,
    // not quietly compare fewer rows. The error names the FIRST missing provider
    // in `PROVIDER_KEYS` order, so this excludes `anthropic` explicitly rather
    // than by position.
    let missing_anthropic =
        payload.replace(r#""anthropic": { "models": {} }"#, r#""anthropic": {}"#);
    assert_ne!(missing_anthropic, payload, "the fixture must really change");
    let err = parse_catalogue(&missing_anthropic)
        .expect_err("a payload missing a mapped provider must not parse clean");
    assert!(
        err.contains("anthropic.models"),
        "the error must name the provider that went missing: {err}"
    );
    let err = parse_catalogue("not json").expect_err("non-JSON must not parse clean");
    assert!(err.contains("not JSON"), "{err}");
}

/// The failure wording, pinned: "could not check" must not read as
/// "checked; clean", and it must carry the exact command to re-run.
#[test]
fn the_did_not_run_message_says_not_checked_and_quotes_the_command() {
    let msg = audit_did_not_run("curl exited 6 (Could not resolve host)");
    assert!(msg.contains("DID NOT RUN"), "{msg}");
    assert!(msg.contains("UNVERIFIED"), "{msg}");
    assert!(msg.contains("Could not resolve host"), "{msg}");
    assert!(
        msg.contains("cargo test audit_table_against_models_dev -- --ignored --nocapture"),
        "{msg}"
    );
}
