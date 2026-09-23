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

use crate::commands_risk_parse::{line_is_gradable, read_ledger_content, LedgerContent};
use crate::commands_risk_snapshots::{first_scored_age, founding_ts, read_first_scored};
use std::collections::BTreeMap;

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
    /// The **raw first-scored-ledger join**'s own reading of the same surprise
    /// list — the second instrument (Day 207, [`born_after_by_ledger`]), which
    /// compares `first_scored[p].ts` against the event's `ts` with no
    /// founding-batch guard and no special case for an unknown `ts`.
    ///
    /// The two instruments are not two spellings of one number and the fields
    /// are kept apart on purpose: `unhittable` is the *guarded* live reading
    /// that is recorded on the event, and these are the *raw* join's members.
    /// They agree on every ordinary post-ledger row and part company exactly
    /// where the guard is doing work — a founding-batch birthday (an unknown,
    /// not a birthday) or an unorderable `ts` — which is the case a reader
    /// must be able to see rather than have folded into one verdict.
    pub(crate) ledger_born_after: u32,
    /// Surprises whose ledger birthday is **exactly** the event's own `ts`: a
    /// date tie. Neither born-after (no foresight is missing) nor a hit worth
    /// crediting — the day-178 row is the one instance in this repo's ledger
    /// (`src/gasp_cli.rs`, event ts `2026-08-25T22:40:17Z`), where the git
    /// instrument reads born-after and the ledger join reads same-instant.
    pub(crate) ties: u32,
    /// The **git instrument** (Day 207, [`git_born_after_at`]): how many
    /// surprises `git cat-file -e <hash>:<path>` says are absent at the
    /// snapshot's own tree. `0` in a project with no git, or on a row whose
    /// hash does not resolve — read [`UnhittableCount::git_unmeasured`] beside
    /// it before reading the zero as a measurement.
    pub(crate) git_born_after: u32,
    /// Surprises the git instrument could **not** check: the snapshot's own
    /// hash is not a commit `git cat-file -t` resolves, so the check never ran
    /// for a single member of that row.
    ///
    /// This is the day-205 shape (`snapshot_git_hash = "dcc72f63"`,
    /// `src/config_paths.rs`). Before this field existed, a hash that failed to
    /// resolve made `cat-file -e` fail for *every* path — which is
    /// indistinguishable at that call site from "the path was not there yet" —
    /// so one unresolvable hash became one fake born-after file, rendered
    /// exactly like a real one. A failed measurement is not a finding: it is
    /// `unmeasured`, on the same rule the ledger join already applies to a path
    /// it has never seen.
    ///
    /// **Day 207 correction, recorded rather than erased.** The ledger stores
    /// `git_hash` as an **8-hex prefix**, and a prefix resolves the moment a
    /// commit starting with it is inside the clone's window — measured today,
    /// `dcc72f63` is `dcc72f6329bdf46b9558365833914f83ded4fc9f` (day 205) in a
    /// 52-commit checkout and fails in a shallower one. So a row of this field
    /// is a statement about **the checkout the reading was taken in**, not only
    /// about the ledger: the same row is `unmeasured` on a shallow fetch and
    /// `git_born_after` on a deep one. The day-205 example below is therefore
    /// an example of the *shape*, not of a permanently unresolvable hash.
    pub(crate) git_unmeasured: u32,
    /// Surprises the raw join found **no** ledger record for at all. A subset
    /// of `unmeasurable` by construction, and the leg that keeps an unknown
    /// birthday from being read as "after".
    pub(crate) ledger_unmeasured: u32,
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
/// What the **raw** first-scored-ledger join found in one surprise list.
///
/// Paths, not counts, for the same reason the census prints paths: a count a
/// reader cannot attribute is a number they have to take on faith, and the
/// question this answers ("*which* file did not exist yet?") is not answerable
/// from a total.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct LedgerBornAfter {
    /// First-scored **strictly after** the event's own `ts`. The comparison is
    /// `>`, deliberately: an equal `ts` is [`LedgerBornAfter::ties`], a
    /// different answer to a different question, not a member of this set.
    pub(crate) files: Vec<String>,
    /// First-scored at **exactly** the event's `ts`. Its own bucket because it
    /// is neither a hit nor a miss: nothing was born after the prediction, so
    /// no foresight is missing — but the two records disagree about the order,
    /// and that disagreement is the observed fact.
    pub(crate) ties: Vec<String>,
    /// No ledger record at all. **Unknown, never "after"** — this is the rule
    /// that keeps a failed measurement from reading as a born-after file, one
    /// instrument over: a ledger that has never seen a path cannot certify a
    /// birthday for it.
    pub(crate) unmeasured: Vec<String>,
}

/// The raw ledger join: compare each surprise's first-scored `ts` against the
/// event's own `ts`, with **no** founding-batch guard and **no** special case
/// for an unorderable `ts`.
///
/// Pure — no git call, no filesystem call, no clock — which is the whole point
/// of the fallback DREAM.md names: it needs nothing but the ledger it already
/// has. It is deliberately **not** a replacement for
/// [`count_unhittable_surprises`], whose guard is load-bearing (a founding-batch
/// birthday is an unknown, and reading it as a birthday is the survivor trap
/// this repo has documented twice); where the two disagree, the disagreement is
/// the reading, and `UnhittableCount` carries both.
///
/// `first_scored` is `(path, ts)` rows as they sit in
/// `.yoyo/risk_first_scored.jsonl`. The ledger is append-only, so the **earliest**
/// `ts` per path wins — the same rule `parse_first_scored` applies — and a path
/// repeated inside `surprises` is reported once per occurrence, because the row
/// itself repeated it.
pub(crate) fn born_after_by_ledger(
    surprises: &[String],
    event_ts: &str,
    first_scored: &[(String, String)],
) -> LedgerBornAfter {
    let mut birth: BTreeMap<&str, &str> = BTreeMap::new();
    for (path, ts) in first_scored {
        birth
            .entry(path.as_str())
            .and_modify(|existing| {
                if ts.as_str() < *existing {
                    *existing = ts.as_str();
                }
            })
            .or_insert(ts.as_str());
    }
    let mut out = LedgerBornAfter::default();
    for path in surprises {
        match birth.get(path.as_str()) {
            None => out.unmeasured.push(path.clone()),
            Some(&ts) if ts > event_ts => out.files.push(path.clone()),
            Some(&ts) if ts == event_ts => out.ties.push(path.clone()),
            // Strictly before: it existed when the prediction was made. No
            // bucket, because "hittable" is not a claim this join makes.
            Some(_) => {}
        }
    }
    out
}

/// Invariant, pinned by tests: `measured + unmeasurable == surprises.len()`.
pub(crate) fn count_unhittable_surprises(
    surprises: &[String],
    snapshot_ts: &str,
    first_scored: &std::collections::BTreeMap<String, String>,
    founding: Option<&str>,
) -> UnhittableCount {
    // The raw join runs first and fills only the three instrument fields; the
    // guarded loop below owns `unhittable`/`measured`/`unmeasurable` exactly as
    // it always has, so no recorded count moves (Day 207).
    let pairs: Vec<(String, String)> = first_scored
        .iter()
        .map(|(p, t)| (p.clone(), t.clone()))
        .collect();
    let raw = born_after_by_ledger(surprises, snapshot_ts, &pairs);
    let mut out = UnhittableCount {
        ledger_born_after: raw.files.len() as u32,
        ties: raw.ties.len() as u32,
        ledger_unmeasured: raw.unmeasured.len() as u32,
        ..Default::default()
    };
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

/// The **git instrument's decision**, as a pure function of what its two probes
/// returned. Its I/O half is [`git_born_after_at`], kept separate so the
/// day-205 shape is a table row here rather than a subprocess.
///
/// `members` is `(path, exists-at-that-tree)` — `exists` is what
/// `git cat-file -e <hash>:<path>` said, and it is only meaningful when
/// `hash_resolves` is true. That flag is `git cat-file -t <hash>` and it is the
/// whole point of this function: without it the two conditions produce the same
/// `false` and are indistinguishable, so a snapshot hash nobody can resolve
/// reads as *every file is born after it*.
pub(crate) fn git_born_after_by_check(
    hash_resolves: bool,
    members: &[(String, bool)],
) -> LedgerBornAfter {
    let mut out = LedgerBornAfter::default();
    if !hash_resolves {
        // The check did not run. Every member is unmeasured — including the
        // members that would have been born-after had it run, which we cannot
        // know, and which is exactly why this must not be reported as zero
        // born-after either.
        for (path, _) in members {
            out.unmeasured.push(path.clone());
        }
        return out;
    }
    for (path, exists) in members {
        if !exists {
            out.files.push(path.clone());
        }
    }
    out
}

/// I/O half of the git instrument: ask git whether each surprise existed at the
/// tree its own snapshot named.
///
/// One `git cat-file -t <hash>` first, then one `git cat-file -e <hash>:<path>`
/// per member **only if the hash resolves**. A non-resolving hash short-circuits
/// the per-member loop deliberately: nothing it could return would be a
/// measurement.
///
/// Read-only git, no clock, no filesystem walk of our own. In a directory with
/// no repository `run_git` errors, which lands on the same `unmeasured` branch
/// as an unresolvable hash — the honest answer, and the one that keeps a
/// non-git project from reading as "no file was born late".
pub(crate) fn git_born_after_at(hash: &str, surprises: &[String]) -> LedgerBornAfter {
    if hash.is_empty() || hash == "unknown" {
        // A sentinel is not a hash, so there is nothing to ask git — but the
        // members are still *unmeasured*, not absent from the answer: a
        // sentinel must not read as "no file was born late".
        let members: Vec<(String, bool)> = surprises.iter().map(|p| (p.clone(), false)).collect();
        return git_born_after_by_check(false, &members);
    }
    let resolves = crate::git::run_git(&["cat-file", "-t", hash]).is_ok();
    let mut members: Vec<(String, bool)> = Vec::new();
    if resolves {
        for path in surprises {
            let spec = format!("{hash}:{path}");
            let exists = crate::git::run_git(&["cat-file", "-e", &spec]).is_ok();
            members.push((path.clone(), exists));
        }
    } else {
        for path in surprises {
            members.push((path.clone(), false));
        }
    }
    git_born_after_by_check(resolves, &members)
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

/// The same join, with the git instrument run beside it — the composition the
/// live call sites use, because they hold the snapshot's own `git_hash` and are
/// the only place it is known.
///
/// The ledger reading is not replaced and its recorded count does not move: the
/// git numbers are carried on the same struct so the note can print two
/// instruments side by side. A caller that has no hash (the green path) keeps
/// [`count_unhittable_surprises_at`] and the git fields stay zero — zero *and*
/// zero `git_unmeasured`, which the note is careful not to read as agreement.
pub(crate) fn count_unhittable_surprises_with_git(
    surprises: &[String],
    snapshot_ts: &str,
    snapshot_git_hash: &str,
    ledger_path: &std::path::Path,
) -> UnhittableCount {
    let mut count = count_unhittable_surprises_at(surprises, snapshot_ts, ledger_path);
    let git = git_born_after_at(snapshot_git_hash, surprises);
    count.git_born_after = git.files.len() as u32;
    count.git_unmeasured = git.unmeasured.len() as u32;
    count
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
    // The git leg joins the existence test so the day-205 shape is *visible*
    // (`git_unmeasured` names a check that did not run) rather than silent.
    if count.unhittable == 0 && count.unmeasurable == 0 && count.git_unmeasured == 0 {
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
    if count.ties > 0 {
        // The day-178 leg: the two records disagree about the order and
        // neither wins. Named rather than dropped, because a tie that is
        // silently absent is a tie a reader will re-derive.
        let tv = if count.ties == 1 { "is" } else { "are" };
        note.push_str(&format!("; {} {tv} a date tie", count.ties));
    }
    // Two instruments, printed apart and only where they part company: the
    // ledger join (what is recorded) and the git check (what the tree says).
    // Silence here means they agree, never that one of them ran alone.
    if count.git_unmeasured > 0 {
        note.push_str(&format!(
            "; the git check could not run for {} (snapshot hash unresolved)",
            count.git_unmeasured
        ));
    }
    if count.git_born_after != count.unhittable {
        note.push_str(&format!(
            "; the git check reads {} born-after",
            count.git_born_after
        ));
    }
    // Glyph-free under plain output, matching the sibling refusal/notice
    // messages; the glyph lives at the call site so the note stays a string.
    Some(if plain { note } else { format!("📊 {note}") })
}

// ---------------------------------------------------------------------------
// The retrospective half (DREAM.md cycle 10, part 2): the live count above is
// ONE reading with no base rate. The dream's own sentence is *"plus a
// retrospective pass over the 115 post-ledger events reporting how many had ≥1
// such member"* — how often the `0%`-vs-`unhittable` ambiguity has actually been
// in play, over the whole history rather than the row in front of me.
// ---------------------------------------------------------------------------

/// One validation row reduced to what the retrospective pass needs: **when** it
/// was graded (`ts`) and which paths surprised it.
///
/// `ts` is `""` when the line carries no usable timestamp string; such a row has
/// no moment to compare a birthday against and is counted as
/// [`RetrospectiveCount::undated`] rather than silently folded into either
/// answer.
pub(crate) struct SurpriseRow {
    pub(crate) ts: String,
    pub(crate) surprises: Vec<String>,
}

/// Parse the validation ledger's text into rows carrying `ts` + `surprises`.
///
/// Only lines [`line_is_gradable`] admits are rows — the same rule the sibling
/// `parse_validation_events_counting` uses, reused rather than re-derived — so
/// the pass's population is the same set of *events* the accuracy report counts.
/// Non-event lines are not counted a second time here: `ledger_health_line` on
/// the `/risk accuracy` path already prints them out loud (`dropped` /
/// `ungradable`), and a line that is not an event belongs to no population.
pub(crate) fn parse_surprise_rows(content: &str) -> Vec<SurpriseRow> {
    let mut rows = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        if !line_is_gradable(&val) {
            continue;
        }
        let ts = val["ts"].as_str().unwrap_or("").to_string();
        let surprises = val["surprises"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        rows.push(SurpriseRow { ts, surprises });
    }
    rows
}

/// Is `ts` written in the one fixed-width shape every `ts` in both ledgers is
/// written in — `%Y-%m-%dT%H:%M:%SZ`, 20 bytes?
///
/// The whole join rests on lexicographic order being chronological, and that is
/// true *only* for this shape: a date-only `2026-09-03` or a stamp missing its
/// `Z` compares wrongly against a full one (`"2026-09-03" < "2026-09-03T..."`
/// is right, but a short stamp at a different length can land on either side).
/// A row whose `ts` is not in the shape therefore has **no placeable moment** and
/// is counted as `undated` rather than taking part in a compare it cannot
/// support. Stated limit, in the other direction: the *birthday* side is
/// [`first_scored_age`]'s contract, inherited unvalidated from
/// `parse_first_scored` — that side is the ledger writer's business
/// (`utc_timestamp`), and re-validating it here would be a second copy of the
/// live path's assumption rather than a repair of it.
fn is_placeable_ledger_ts(ts: &str) -> bool {
    let bytes = ts.as_bytes();
    if bytes.len() != 20 {
        return false;
    }
    for (i, b) in bytes.iter().enumerate() {
        let ok = match i {
            4 | 7 => *b == b'-',
            10 => *b == b'T',
            13 | 16 => *b == b':',
            19 => *b == b'Z',
            _ => b.is_ascii_digit(),
        };
        if !ok {
            return false;
        }
    }
    true
}

/// What the retrospective pass found. Every field is a count, never a rate: the
/// task's line is *"1 of 115 post-ledger grading events carried a file first
/// scored after the event"*, and a percentage would hide the denominator that
/// makes the count readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct RetrospectiveCount {
    /// Grading events **after** the first-scored ledger's own founding instant —
    /// the only rows this join can grade (see [`retrospective_unhittable`]).
    pub(crate) population: u32,
    /// How many of those carried ≥1 member whose first-scored `ts` is later than
    /// that row's own event `ts`.
    pub(crate) with_unhittable: u32,
    /// How many carried ≥1 member the join could **not** decide.
    pub(crate) with_unmeasurable: u32,
    /// Gradable rows with no placeable `ts` (absent, empty, or not
    /// [`is_placeable_ledger_ts`]) — out of the population because there is no
    /// moment to compare a birthday against.
    pub(crate) undated: u32,
}

/// Join every historical row's surprises against the first-scored ledger — the
/// retrospective reading of the same predicate [`count_unhittable_surprises`]
/// applies live.
///
/// **The boundary is deliberately not the live one, and the difference is
/// stated rather than left to be guessed.** The live path compares a birthday
/// against the *snapshot's* `ts`. A historical row cannot: its
/// `snapshot_git_hash` is unresolvable here (the CI clone is ~50 commits deep),
/// so the honest boundary is the **row's own `ts`** — the first-scored-ledger
/// join the dream names as the fallback. Both boundaries answer the same
/// question ("was this path first scored after the prediction that missed
/// it?"); they differ only in which moment is taken as "the prediction", and on
/// a live row those two moments are minutes apart.
///
/// **Post-ledger rows only, and the reason is the survivor trap.** A row earlier
/// than the ledger's founding instant cannot be graded this way: its
/// first-scored dates are the backfill batch, so reading them as evidence would
/// be exactly the trap the dream names twice. `founding` comes from
/// [`founding_ts`] over the same map, never from a hardcoded date. The boundary
/// compare is strict (`row.ts > founding`), matching the strictness
/// [`count_unhittable_surprises`] uses against the snapshot `ts`: a row stamped
/// at the founding instant itself is indistinguishable from the backfill
/// batch. When the ledger is empty (`founding == None`) there is no batch to
/// exclude — nothing has ever been backfilled — so every dated row is in the
/// population and every member is unmeasurable.
///
/// Reused rather than re-derived: [`first_scored_age`] carries the founding-batch
/// exclusion (a founding-batch path is *unknown*, never *unhittable*) and the
/// `birthday > row_ts` test is the same plain string compare the live path uses,
/// which is chronological only because both sides are the one fixed-width
/// ISO-8601 UTC shape.
pub(crate) fn retrospective_unhittable(
    rows: &[SurpriseRow],
    first_scored: &BTreeMap<String, String>,
    founding: Option<&str>,
) -> RetrospectiveCount {
    let mut out = RetrospectiveCount::default();
    for row in rows {
        let Some(row_ts) = Some(row.ts.as_str()).filter(|t| is_placeable_ledger_ts(t)) else {
            // No placeable moment: not a row this join can grade. Counting it
            // out loud is the whole point — an unorderable row must not read as
            // all-hittable (the live path makes the same call for `"unknown"`).
            out.undated += 1;
            continue;
        };
        if let Some(founding) = founding {
            if row_ts <= founding {
                continue;
            }
        }
        out.population += 1;
        let mut unhittable = false;
        let mut unmeasurable = false;
        for path in &row.surprises {
            match first_scored_age(path, first_scored, founding) {
                Some(birthday) if birthday > row_ts => unhittable = true,
                Some(_) => {}
                None => unmeasurable = true,
            }
        }
        if unhittable {
            out.with_unhittable += 1;
        }
        if unmeasurable {
            out.with_unmeasurable += 1;
        }
    }
    out
}

/// The three readable states of the retrospective reading, never folded into
/// one another.
///
/// A bare `0%` is exactly what this module exists to remove, so a zero must
/// never render as an absence: "no post-ledger rows" and "zero of N carried an
/// unhittable member" are different findings and are printed differently. The
/// fourth case — no ledger file at all — is [`retrospective_note`]'s `None`,
/// which keeps a project that has never recorded a risk grade byte-identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RetrospectiveReading {
    /// No validation ledger on disk: this project has never recorded a grade,
    /// so there is deliberately nothing to say.
    NoLedger,
    /// The ledger was read. `population == 0` is state (b), a readable zero.
    Counted(RetrospectiveCount),
    /// (c) The ledger exists but could not be read — an unknown, not a zero.
    Unreadable(String),
}

/// I/O wrapper for [`retrospective_unhittable`]: read both ledgers through the
/// existing reader seams and run the join.
///
/// The validation ledger goes through [`read_ledger_content`] — the *one*
/// missing/unreadable/present policy, shared with `read_validation_ledger` — and
/// the first-scored ledger through [`read_first_scored`], whose missing file is
/// an honest empty map. One composition for every call site.
pub(crate) fn retrospective_unhittable_at(
    validation_path: &std::path::Path,
    first_scored_path: &std::path::Path,
) -> RetrospectiveReading {
    let content = match read_ledger_content(validation_path) {
        LedgerContent::Missing => return RetrospectiveReading::NoLedger,
        LedgerContent::Unreadable(e) => return RetrospectiveReading::Unreadable(e),
        LedgerContent::Present(content) => content,
    };
    let rows = parse_surprise_rows(&content);
    let (map, _dropped) = read_first_scored(first_scored_path);
    let founding = founding_ts(&map);
    RetrospectiveReading::Counted(retrospective_unhittable(&rows, &map, founding))
}

/// The report line: a count with its denominator, never a percentage.
///
/// `None` is reserved for [`RetrospectiveReading::NoLedger`] — a project with no
/// risk ledger has no grading history to be honest about, and its `yoyo risk`
/// report stays byte-identical. The other two states always print, because
/// "nothing to grade yet" and "the ledger is unreadable" are readings, not
/// absences. Plain output is ASCII (no glyph *and* no em dash), matching
/// `unhittable_note` and the sibling refusal/notice messages.
pub(crate) fn retrospective_note(reading: &RetrospectiveReading, plain: bool) -> Option<String> {
    let body = match reading {
        RetrospectiveReading::NoLedger => return None,
        RetrospectiveReading::Unreadable(err) => format!(
            "unhittable: cannot read the validation ledger ({}) - the retrospective count \
             is unknown, not zero",
            crate::cli::sanitize_for_display(err)
        ),
        // State (b): a readable zero, in its own words.
        RetrospectiveReading::Counted(c) if c.population == 0 => {
            let mut note = String::from(
                "unhittable: no post-ledger grading events yet - \
                 this is not the same as zero unhittable",
            );
            if c.undated > 0 {
                note.push_str(&format!("; {} undated row(s)", c.undated));
            }
            note
        }
        RetrospectiveReading::Counted(c) => {
            let mut note = format!(
                "unhittable: {} of {} post-ledger grading events carried a file first scored \
                 after the event",
                c.with_unhittable, c.population
            );
            if c.with_unmeasurable > 0 {
                note.push_str(&format!(
                    "; {} carried at least one undecidable surprise",
                    c.with_unmeasurable
                ));
            }
            if c.undated > 0 {
                note.push_str(&format!("; {} undated row(s)", c.undated));
            }
            note
        }
    };
    Some(if plain { body } else { format!("📊 {body}") })
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
            ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
        };
        let note = unhittable_note(count, true).expect("an undecidable row still prints");
        assert!(note.contains("0 of 2 surprises were unhittable"), "{note}");
        assert!(note.contains("2 undecidable"), "{note}");
    }

    /// The two instruments printed apart, and the tie named — the task's
    /// "`ledger 3 / git 5 / 2 ties`" reading.
    #[test]
    fn the_note_prints_both_instruments_and_the_tie_count() {
        let count = UnhittableCount {
            unhittable: 3,
            measured: 4,
            unmeasurable: 0,
            ledger_born_after: 3,
            ties: 2,
            git_born_after: 5,
            git_unmeasured: 0,
            ledger_unmeasured: 0,
        };
        let note = unhittable_note(count, true).expect("note");
        assert!(note.contains("3 of 4 surprises were unhittable"), "{note}");
        assert!(note.contains("2 are a date tie"), "{note}");
        assert!(
            note.contains("the git check reads 5 born-after"),
            "the second instrument's number is printed, not implied: {note}"
        );
        // The two must not be presented as one number: the note names which is
        // which.
        assert!(note.contains("first scored after this snapshot"), "{note}");
    }

    /// A check that **did not run** is spoken, even when it is the only thing
    /// to say — this is the day-205 case, and before the git leg joined the
    /// guard it was silent (every count zero).
    #[test]
    fn the_note_speaks_when_only_the_git_check_failed_to_run() {
        let count = UnhittableCount {
            git_unmeasured: 1,
            ..Default::default()
        };
        let note = unhittable_note(count, true).expect("an unrun check is not silence");
        assert!(note.contains("the git check could not run for 1"), "{note}");
        assert!(
            !note.contains("the git check reads"),
            "nothing ran, so there is no reading to compare: {note}"
        );
        // Plain mode stays glyph-free, like every sibling message.
        assert!(note.is_ascii(), "{note}");
    }

    /// Agreement is silence: the git number is printed only where the two
    /// instruments part company, so an ordinary session's stderr does not grow
    /// a redundant clause.
    #[test]
    fn the_note_stays_quiet_when_both_instruments_agree() {
        let count = UnhittableCount {
            unhittable: 2,
            measured: 2,
            unmeasurable: 0,
            ledger_born_after: 2,
            ties: 0,
            git_born_after: 2,
            git_unmeasured: 0,
            ledger_unmeasured: 0,
        };
        let note = unhittable_note(count, true).expect("note");
        assert!(!note.contains("the git check reads"), "{note}");
        assert!(!note.contains("date tie"), "{note}");
        assert!(!note.contains("could not run"), "{note}");
    }
}

/// Tests for the **raw ledger join** ([`born_after_by_ledger`]) — the second
/// instrument — and for the git instrument's decision
/// ([`git_born_after_by_check`]), whose whole job is to keep a check that did
/// not run from reading as a finding.
///
/// The inputs here are deliberately *structurally distinct*, not one fixture at
/// different lengths: the one row carries a before, an after, a tie and an
/// absent member together, so a bug that returns a whole-list verdict (all
/// born-after, or none) cannot pass by agreeing with a homogeneous fixture.
#[cfg(test)]
mod born_after_ledger_tests {
    use super::*;

    fn pairs(rows: &[(&str, &str)]) -> Vec<(String, String)> {
        rows.iter()
            .map(|(p, t)| ((*p).to_string(), (*t).to_string()))
            .collect()
    }

    fn paths(rows: &[&str]) -> Vec<String> {
        rows.iter().map(|s| (*s).to_string()).collect()
    }

    /// A **well-formed id no checkout contains**, for the two subprocess tests
    /// below.
    ///
    /// **Day 207: the fixture these two tests used to share was ambient machine
    /// state.** It was `dcc72f63` — the real 8-hex `git_hash` the day-204
    /// ledger row carries — and both tests pinned "this hash does not resolve".
    /// A **prefix** resolves the moment a commit starting with it is inside the
    /// clone's window, so the premise was a fact about the depth of whichever
    /// clone ran the test: measured today `dcc72f63` is
    /// `dcc72f6329bdf46b9558365833914f83ded4fc9f` (day 205) and resolves in a
    /// 52-commit checkout while failing in a shallower one. It went red on a
    /// re-shallowed clone for the reason the deepening note only half recorded:
    /// `git fetch --depth=N` after a deep fetch leaves the deep objects **in the
    /// pack**, so the id keeps resolving although it is no longer reachable.
    /// The two tests therefore read the clone, not the code.
    ///
    /// The null oid is rejected by `git cat-file -t` in every repository, so the
    /// premise cannot go stale — and it is 40 hex characters, so it is **not**
    /// one of the sentinels `git_born_after_at` short-circuits on
    /// ([`a_sentinel_or_empty_hash_does_not_shell_out`]) and the real subprocess
    /// is the path under test.
    const ABSENT_OBJECT_ID: &str = "0000000000000000000000000000000000000000";

    /// One event, four surprises, four different answers. This is the test the
    /// task's Step 1 asks for by name: after / tied / absent / before, in a
    /// single list.
    #[test]
    fn one_row_partitions_after_tied_absent_and_before() {
        let ts = "2026-09-03T17:23:00Z";
        let ledger = pairs(&[
            ("src/old.rs", "2026-08-25T10:00:00Z"),  // before — existed
            ("src/born.rs", "2026-09-03T18:03:00Z"), // after — born late
            ("src/tie.rs", "2026-09-03T17:23:00Z"),  // exactly the event's ts
                                                     // src/absent.rs has no record at all
        ]);
        let surprises = paths(&["src/old.rs", "src/born.rs", "src/tie.rs", "src/absent.rs"]);
        let got = born_after_by_ledger(&surprises, ts, &ledger);

        assert_eq!(got.files, paths(&["src/born.rs"]), "{got:?}");
        assert_eq!(got.ties, paths(&["src/tie.rs"]), "{got:?}");
        assert_eq!(got.unmeasured, paths(&["src/absent.rs"]), "{got:?}");
        // The before member is in no bucket: "hittable" is not a claim this
        // join makes, so a member it can place early is simply placed.
        let all = got.files.len() + got.ties.len() + got.unmeasured.len();
        assert_eq!(all, 3, "the before member is in none of the three: {got:?}");
    }

    /// The tie is its own branch and not folded into either neighbour. Both
    /// directions matter: folding it into `files` invents a born-after file,
    /// folding it into "before" loses the day-178 row.
    #[test]
    fn the_tie_is_neither_born_after_nor_dropped() {
        let ts = "2026-08-25T22:40:17Z";
        // The real day-178 row, verbatim enough to be the named case: the
        // ledger holds this path at exactly the event's own ts.
        let ledger = pairs(&[("src/gasp_cli.rs", "2026-08-25T22:40:17Z")]);
        let got = born_after_by_ledger(&paths(&["src/gasp_cli.rs"]), ts, &ledger);
        assert!(got.files.is_empty(), "a tie is not born-after: {got:?}");
        assert_eq!(got.ties, paths(&["src/gasp_cli.rs"]), "{got:?}");
        assert!(got.unmeasured.is_empty(), "{got:?}");

        // One second later is a different answer — the boundary is real, not
        // an artifact of the fixture.
        let later = pairs(&[("src/gasp_cli.rs", "2026-08-25T22:40:18Z")]);
        let got_later = born_after_by_ledger(&paths(&["src/gasp_cli.rs"]), ts, &later);
        assert_eq!(
            got_later.files,
            paths(&["src/gasp_cli.rs"]),
            "{got_later:?}"
        );
        assert!(got_later.ties.is_empty(), "{got_later:?}");
    }

    /// A file the ledger has never seen has an **unknown** birthday, and
    /// unknown is not "after". This is the rule that keeps one instrument's
    /// failure from becoming the other instrument's finding.
    #[test]
    fn an_absent_record_is_unmeasured_never_born_after() {
        let got = born_after_by_ledger(
            &paths(&["src/never-seen.rs"]),
            "2026-09-03T17:23:00Z",
            &pairs(&[("src/other.rs", "2026-08-25T10:00:00Z")]),
        );
        assert!(got.files.is_empty(), "unknown is not born-after: {got:?}");
        assert_eq!(got.unmeasured, paths(&["src/never-seen.rs"]), "{got:?}");
    }

    /// The whole regression surface for a project with no ledger: an empty
    /// answer, not a false one. Every member unmeasured, no born-after.
    #[test]
    fn an_empty_ledger_makes_every_member_unmeasured() {
        let surprises = paths(&["src/a.rs", "src/b.rs"]);
        let got = born_after_by_ledger(&surprises, "2026-09-03T17:23:00Z", &[]);
        assert!(got.files.is_empty(), "{got:?}");
        assert!(got.ties.is_empty(), "{got:?}");
        assert_eq!(got.unmeasured.len(), 2, "{got:?}");
        assert_eq!(
            got,
            LedgerBornAfter {
                files: vec![],
                ties: vec![],
                unmeasured: surprises,
            }
        );
    }

    /// An empty surprise list is empty on every leg, whatever the ledger says:
    /// there is nothing to be born after.
    #[test]
    fn no_surprises_is_an_empty_answer() {
        let got = born_after_by_ledger(
            &[],
            "2026-09-03T17:23:00Z",
            &pairs(&[("src/a.rs", "2026-09-30T00:00:00Z")]),
        );
        assert_eq!(got, LedgerBornAfter::default());
    }

    /// The ledger is append-only and `parse_first_scored` keeps the earliest
    /// `ts` per path; this join must agree, or a re-scored file reads as
    /// newborn on its second sighting.
    #[test]
    fn the_earliest_record_per_path_wins() {
        let ledger = pairs(&[
            ("src/a.rs", "2026-09-30T00:00:00Z"), // later, listed first
            ("src/a.rs", "2026-08-25T10:00:00Z"), // the real birthday
        ]);
        let got = born_after_by_ledger(&paths(&["src/a.rs"]), "2026-09-03T17:23:00Z", &ledger);
        assert!(got.files.is_empty(), "earliest wins: {got:?}");
        assert!(got.unmeasured.is_empty(), "{got:?}");
    }

    /// **The named day-205 shape, which is the one row in this repo's ledger
    /// where the defect fires.** `snapshot_git_hash = "dcc72f63"` does not
    /// resolve, and `src/config_paths.rs` is the row's only surprise. Before
    /// this branch existed, `cat-file -e` failed for that path for the same
    /// reason it would fail for every path — so it was reported born-after when
    /// in fact **the measurement did not run at all**.
    #[test]
    fn an_unresolvable_hash_is_unmeasured_not_born_after() {
        // Anti-vacuous: the fixture really does carry the shape under test.
        // `git_born_after_by_check` is being handed `hash_resolves = false`,
        // which is what `git cat-file -t dcc72f63` returns here — the named
        // test below drives the real subprocess and asserts the same thing
        // about the real hash, so this fixture cannot silently drift into
        // describing a hash that resolves.
        let members = vec![("src/config_paths.rs".to_string(), false)];
        let got = git_born_after_by_check(false, &members);
        assert!(
            got.files.is_empty(),
            "a failed check is not a finding: {got:?}"
        );
        assert!(got.ties.is_empty(), "{got:?}");
        assert_eq!(got.unmeasured, paths(&["src/config_paths.rs"]), "{got:?}");
    }

    /// **The near-miss twin, and the whole regression surface of Step 2.** A
    /// *resolvable* hash with a genuinely absent path still reports born-after;
    /// the fix must narrow the unmeasured branch to an unresolvable hash and
    /// nothing else. One word in the wrong place inverts the instrument.
    #[test]
    fn a_resolvable_hash_with_an_absent_path_is_still_born_after() {
        let members = vec![
            ("src/gone.rs".to_string(), false),   // absent at that tree
            ("src/present.rs".to_string(), true), // present — not a finding
        ];
        let got = git_born_after_by_check(true, &members);
        assert_eq!(got.files, paths(&["src/gone.rs"]), "{got:?}");
        assert!(got.unmeasured.is_empty(), "{got:?}");
    }

    /// The two branches answer differently on the *same* members — which is
    /// what makes the flag load-bearing rather than decorative.
    #[test]
    fn the_resolvability_flag_changes_the_answer_for_one_member_set() {
        let members = vec![("src/a.rs".to_string(), false)];
        let resolved = git_born_after_by_check(true, &members);
        let unresolved = git_born_after_by_check(false, &members);
        assert_eq!(resolved.files.len(), 1);
        assert_eq!(unresolved.unmeasured.len(), 1);
        assert_ne!(resolved, unresolved);
    }

    /// The real subprocess on an id no checkout contains — the anti-vacuous
    /// half, and the I/O twin of the pure `git_born_after_by_check(false, ..)`
    /// test above. Resolvability is asserted **first and separately**: if this
    /// id ever resolved, the two assertions below would be describing a
    /// different world and both would need re-reading rather than silently
    /// agreeing with a fixture. (The twin this replaces, keyed on the day-205
    /// prefix `dcc72f63`, could not make that check at all — see
    /// `ABSENT_OBJECT_ID`.)
    #[test]
    fn an_absent_object_id_reaches_unmeasured_through_the_real_subprocess() {
        let resolves = crate::git::run_git(&["cat-file", "-t", ABSENT_OBJECT_ID]).is_ok();
        assert!(
            !resolves,
            "{ABSENT_OBJECT_ID} must NOT resolve — this is the absent-id fixture; \
             if it now resolves, the fixture and the test below have stopped \
             describing the same world and both need re-reading"
        );
        let got = git_born_after_at(ABSENT_OBJECT_ID, &paths(&["src/config_paths.rs"]));
        assert!(got.files.is_empty(), "{got:?}");
        assert_eq!(got.unmeasured, paths(&["src/config_paths.rs"]), "{got:?}");
        // The I/O half must agree with the pure half on the same input: a
        // wrapper that reached the probes with `true` would answer differently
        // here and nowhere else.
        assert_eq!(
            got,
            git_born_after_by_check(false, &[("src/config_paths.rs".to_string(), false)])
        );
    }

    /// A `git_hash` the ledgers actually contain as a sentinel is not a hash:
    /// treat it as unmeasured rather than shelling out with it.
    #[test]
    fn a_sentinel_or_empty_hash_does_not_shell_out() {
        for sentinel in ["", "unknown"] {
            let got = git_born_after_at(sentinel, &paths(&["src/a.rs"]));
            assert!(got.files.is_empty(), "{sentinel:?}: {got:?}");
            assert_eq!(got.unmeasured, paths(&["src/a.rs"]), "{sentinel:?}");
        }
    }

    /// The composition the live call sites use: the ledger's recorded count is
    /// untouched by the git leg, and the git numbers are carried beside it.
    #[test]
    fn the_git_wrapper_leaves_the_recorded_count_alone_and_fills_the_git_leg() {
        let dir = tempfile::tempdir().expect("temp dir");
        let ledger = dir.path().join("risk_first_scored.jsonl");
        std::fs::write(
            &ledger,
            concat!(
                "{\"path\":\"src/seed.rs\",\"ts\":\"2026-08-22T15:40:00Z\"}\n",
                "{\"path\":\"src/born.rs\",\"ts\":\"2026-09-03T18:03:00Z\"}\n",
            ),
        )
        .expect("seed ledger");
        let surprises = paths(&["src/born.rs", "src/absent.rs"]);

        let plain = count_unhittable_surprises_at(&surprises, "2026-09-03T17:23:00Z", &ledger);
        let with_git = count_unhittable_surprises_with_git(
            &surprises,
            "2026-09-03T17:23:00Z",
            ABSENT_OBJECT_ID,
            &ledger,
        );
        // The recorded half is byte-identical: only the git fields are new.
        assert_eq!(plain.unhittable, with_git.unhittable);
        assert_eq!(plain.measured, with_git.measured);
        assert_eq!(plain.unmeasurable, with_git.unmeasurable);
        assert_eq!(plain.ledger_born_after, with_git.ledger_born_after);
        assert_eq!(with_git.git_born_after, 0, "nothing measured: {with_git:?}");
        assert_eq!(with_git.git_unmeasured, 2, "{with_git:?}");
    }
}

/// Tests for the retrospective pass — the dream's second observable: how many
/// **post-ledger grading events** carried an unhittable member, over the whole
/// history rather than the one row in front of me.
#[cfg(test)]
mod retrospective_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn scored(rows: &[(&str, &str)]) -> BTreeMap<String, String> {
        rows.iter()
            .map(|(p, t)| ((*p).to_string(), (*t).to_string()))
            .collect()
    }

    fn row(ts: &str, surprises: &[&str]) -> SurpriseRow {
        SurpriseRow {
            ts: ts.to_string(),
            surprises: surprises.iter().map(|s| (*s).to_string()).collect(),
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
            with_unmeasurable: 86,
            undated: 0,
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
                with_unmeasurable: 0,
                undated: 0,
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
}
