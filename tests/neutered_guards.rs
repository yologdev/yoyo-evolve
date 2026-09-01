//! Neutered-guard gate: a deliberate sabotage marker left in the tree must fail
//! `cargo test`.
//!
//! **Why this exists, measured rather than argued.** `main` was RED for ~22
//! hours (2026-08-31T22:15 → 2026-09-01T20:12) and a human fixed it from
//! outside the loop. The defect, verbatim from `src/cli.rs` as it shipped:
//!
//! ```text
//! pub(crate) fn sanitize_for_display(s: &str) -> String {
//!     return s.to_string(); // NEUT-ERED POSITIVE CONTROL
//!     ...real implementation, unreachable...
//! }
//! ```
//!
//! An agent ran a positive control **correctly** — neutered the function,
//! watched the tests go red, proved the guard can fail — and then did not
//! restore it. 8 tests failed for four sessions. Every later task's own gate
//! then failed, so every task reverted, *including any task that might have
//! fixed the redness*. The cost of a red-tree escape is not one session; it is
//! every session until a human intervenes.
//!
//! The discipline in CLAUDE.md covers **running** a positive control and says
//! nothing about **restoring** it. That second half had no owner, no gate and
//! no reader: 27 occurrences of "positive control" against 20 restore claims,
//! so ~7 documented controls carry no recorded restore.
//!
//! The agent **labelled its own sabotage**, which is the disciplined thing to
//! do. This gate makes the label load-bearing: if you mark it, the tree refuses
//! to be green until you unmark it. That is the Day-181 lesson — what makes a
//! lesson bind is a required field attached to each *act*, not narration above
//! it — and the same shape as `yoagent-version-claim` in
//! `tests/doc_version_claims.rs`.
//!
//! # What this does NOT fix — stated plainly, because an overstated value claim trains the next reader to ignore the gate
//!
//! 1. **It does not close the wrap-up hole.** `scripts/evolve.sh:3436-3439` is
//!    `git add -A` → `git commit -m "session wrap-up"` with **no build/test
//!    check**, and that file is protected. A `tests/` gate only runs when
//!    `cargo test` runs, so a marker swept in by the wrap-up sweep is as
//!    invisible to this gate as the 8 failing tests already were. What it
//!    closes is the **task gate** window: a marker present when the task's own
//!    `cargo build && cargo test` runs now fails there, in the session that
//!    introduced it.
//! 2. **It is redundant with rustc for the observed instance.** A `return`
//!    before a live body is `unreachable_code`, which
//!    `clippy --all-targets -- -D warnings` already makes fatal, and the
//!    neutering did break 8 tests. For *that* specimen detection already
//!    existed and was bypassed.
//! 3. **The genuinely new coverage is the SILENT case:** a neutered branch no
//!    test covers, and a neutered helper in `scripts/*.py` — where `cargo test`
//!    provides zero coverage and CLAUDE.md records positive controls being run
//!    constantly (e.g. "neutering `page_is_stale` to always return `False`").
//!    A Python script left neutered ships in total silence today.
//!
//! # The self-match problem
//!
//! This file is itself scanned by this gate. Every needle **and every test
//! fixture** is therefore assembled at **runtime** (`format!("{}{}", "NEUT",
//! "ERED")`), never written as a whole literal — the discipline
//! `tests/feature_gated_tests.rs` and the source-level guards in `src/` already
//! use. For the same reason the marker words are **hyphenated in this prose**
//! (`NEUT-ERED`, `SABO-TAGE`, `DO NOT COM-MIT`, `TEMPORARILY DIS-ABLED`): a
//! doc comment cannot call `format!`, so the only way to name them here
//! without self-triggering is to break them. `gate_does_not_flag_its_own_source`
//! pins that the discipline held.
//!
//! # Two branches, running in opposite directions
//!
//! 1. An **unregistered marker → fatal**, with both remedies printed verbatim:
//!    *restore the guard you neutered* (stated first — it is almost always the
//!    right one), or paste the literal register line. **The gate does not
//!    forbid a marker, it forbids an unnamed one** (the Day-166 module-size
//!    lesson: a gate whose only remedy is a whole-task revert eats the correct
//!    work sitting beside the violation).
//! 2. A **registered entry whose file vanished, or whose line no longer carries
//!    a marker → fatal** — the ratchet, since an exception list only pays
//!    itself down if *improving* is a failure too. A register entry with an
//!    **empty or whitespace-only reason is fatal** as well: an unnamed debt
//!    wearing a name is not a name.
//!
//! **Anti-vacuous, and asserted FIRST:** a walk finding zero files, or an empty
//! marker set, fails loudly rather than reporting a clean tree. A scanner that
//! finds nothing and passes is this very defect wearing the opposite sign, and
//! it is quieter than the bug.
//!
//! # The census, dated 2026-09-01
//!
//! **135 files scanned; 0 markers found.** The register ships **empty** — the
//! tree is clean, and a measured zero is a real result. `scripts/` was included
//! in that scan (the assessment had checked only `src/` and `tests/`), so the
//! 3681 protected lines of `scripts/evolve.sh` are covered and carry nothing.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Markers whose presence is deliberate and permanent, as
/// `(path, 1-based line, why)`.
///
/// **Debt, not absolution.** An entry does not make a marker acceptable, it
/// records by hand that one is staying and that someone decided so; branch 2 is
/// what stops it becoming permission. It can only shrink.
///
/// Ships **empty**: the Day-185 scan found 0 markers across 135 files.
const REGISTERED_MARKERS: &[(&str, usize, &str)] = &[];

/// Directories walked, relative to `CARGO_MANIFEST_DIR`.
///
/// `.md` is deliberately **not** scanned and no docs root is listed here:
/// CLAUDE.md documents this very convention and would self-trigger.
const SCAN_ROOTS: &[&str] = &["src", "tests", "scripts"];

/// File extensions scanned. `.py` and `.sh` are the point — `cargo test` gives
/// them zero coverage, so a neutered Python helper ships in total silence
/// without this gate.
const SCAN_EXTENSIONS: &[&str] = &["rs", "py", "sh"];

/// The marker words, assembled at **runtime** so this file cannot match itself.
///
/// Matched **case-sensitively as uppercase**, so ordinary prose ("neutering the
/// capper fails exactly 4 tests", "a sabotage marker") does not fire. The set is
/// deliberately narrow and deliberately excludes `POSITIVE CONTROL`, which
/// appears in honest documentation throughout the repo — a gate that fires on
/// legitimate prose is the `tests/doc_version_claims.rs` trap already named in
/// CLAUDE.md ("no blanket scan for 'impossible'/'unreachable' wording — it would
/// fire on the many honest uses and train the next reader to ignore the gate").
fn sabotage_markers() -> Vec<String> {
    vec![
        format!("{}{}", "NEUT", "ERED"),
        format!("{} {}", "DO NOT", "COMMIT"),
        format!("{}{}", "SABO", "TAGE"),
        format!("{} {}", "TEMPORARILY", "DISABLED"),
    ]
}

/// One marker sighting: where it is and which marker it is.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FoundMarker {
    path: String,
    line: usize,
    marker: String,
}

/// A way this gate can be violated. Three values; the first runs in the
/// opposite direction from the other two.
#[derive(Debug, PartialEq, Eq)]
enum SabotageViolation {
    /// A marker nobody named. The defect this gate exists for.
    Unregistered {
        path: String,
        line: usize,
        marker: String,
    },
    /// A registered entry whose file vanished, or whose line no longer carries
    /// a marker — the debt is paid. The ratchet.
    RegisteredMarkerGone {
        path: String,
        line: usize,
        reason: String,
    },
    /// A register entry with an empty or whitespace-only reason. An unnamed
    /// debt wearing a name is not a name, and the reason is the only part of
    /// the entry a human can act on.
    EmptyReason { path: String, line: usize },
}

impl SabotageViolation {
    fn message(&self) -> String {
        match self {
            SabotageViolation::Unregistered { path, line, marker } => format!(
                "{path}:{line} carries the sabotage marker `{marker}` — a guard, function or \
                 classifier is deliberately broken and the tree cannot be green while it is.\n     \
                 Fix (best): RESTORE THE GUARD YOU BROKE. A positive control has two halves — \
                 break it and watch it fail by name, then restore it and watch it pass — and \
                 this gate exists because the second half was skipped once and cost ~22 hours \
                 of red main plus four reverted sessions.\n     \
                 Fix (only if the marker is deliberately permanent): add \
                 (\"{path}\", {line}, \"<why this marker is deliberately permanent>\") to \
                 REGISTERED_MARKERS in tests/neutered_guards.rs.\n     \
                 The gate does not forbid a marker. It forbids an unnamed one."
            ),
            SabotageViolation::RegisteredMarkerGone { path, line, reason } => format!(
                "{path}:{line} is listed in REGISTERED_MARKERS but no longer carries a marker \
                 (the line changed, or the file is gone) — the debt is paid.\n     \
                 Fix: delete the line (\"{path}\", {line}, \"{reason}\") from \
                 REGISTERED_MARKERS in tests/neutered_guards.rs. Fatal on purpose: an \
                 exception list only ratchets down if a repair is also a failure."
            ),
            SabotageViolation::EmptyReason { path, line } => format!(
                "the REGISTERED_MARKERS entry for {path}:{line} has an empty reason.\n     \
                 Fix: write why that marker is deliberately permanent, or delete the entry and \
                 restore the guard. An unnamed debt wearing a name is not a name — the reason is \
                 the only part of the entry a human can act on."
            ),
        }
    }
}

/// Pure scanner: every marker sighting in one file's text, with **1-based**
/// line numbers so the register reads the same as an editor's gutter.
///
/// A plain substring match, case-sensitively. This is a text scan, not a
/// parser: a marker inside a string literal, or inside an ordinary comment that
/// merely quotes one, fires exactly as a real sabotage does.
fn scan_source(path: &str, source: &str, markers: &[String]) -> Vec<FoundMarker> {
    let mut out = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        for marker in markers {
            if line.contains(marker.as_str()) {
                out.push(FoundMarker {
                    path: path.to_string(),
                    line: idx + 1,
                    marker: marker.clone(),
                });
            }
        }
    }
    out
}

/// Pure checker: given every marker sighting and the register, return every
/// violation. All filesystem work stays at the single call site, so both fatal
/// branches can be proven against **fabricated** inputs rather than by planting
/// a real marker in `src/` — the discipline `tests/orphan_modules.rs` uses.
fn classify(found: &[FoundMarker], register: &[(&str, usize, &str)]) -> Vec<SabotageViolation> {
    let mut violations = Vec::new();

    for hit in found {
        let registered = register
            .iter()
            .any(|(p, l, _)| *p == hit.path && *l == hit.line);
        if !registered {
            violations.push(SabotageViolation::Unregistered {
                path: hit.path.clone(),
                line: hit.line,
                marker: hit.marker.clone(),
            });
        }
    }

    for (path, line, reason) in register {
        if reason.trim().is_empty() {
            violations.push(SabotageViolation::EmptyReason {
                path: (*path).to_string(),
                line: *line,
            });
        }
        let still_there = found.iter().any(|h| h.path == *path && h.line == *line);
        if !still_there {
            violations.push(SabotageViolation::RegisteredMarkerGone {
                path: (*path).to_string(),
                line: *line,
                reason: (*reason).to_string(),
            });
        }
    }

    violations
}

/// Recursively collect `(relative path, source text)` for every scannable file
/// under `dir`, with forward slashes so the register reads the same on every
/// platform.
///
/// Bytes are decoded **lossily**: a marker is pure ASCII and survives
/// replacement of an invalid sequence, so lossy decoding cannot hide one, while
/// a hard failure on one stray byte would take the whole gate down. An
/// unreadable directory or file panics rather than being skipped — "could not
/// check" must never read as "checked; clean".
fn collect_files(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => panic!("cannot read {}: {e}", dir.display()),
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();

    for path in paths {
        if path.is_dir() {
            collect_files(&path, root, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| SCAN_EXTENSIONS.contains(&e))
        {
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
}

/// The anti-vacuous check, pure so it is testable and so neutering the walk is
/// a control that actually fires.
///
/// `None` means the scan was capable of finding something. `Some(msg)` means it
/// was not, and that is fatal: a scanner that finds nothing and passes is this
/// very defect wearing the opposite sign.
fn anti_vacuous_failure(file_count: usize, marker_count: usize) -> Option<String> {
    if marker_count == 0 {
        return Some(
            "the sabotage marker set is EMPTY, so this gate can never fire. That is the defect \
             it exists to catch, wearing the opposite sign. See sabotage_markers() in \
             tests/neutered_guards.rs."
                .to_string(),
        );
    }
    if file_count == 0 {
        return Some(format!(
            "the scan walked {:?} and found ZERO files, so this is a broken walk reporting a \
             clean tree — not a clean tree. See SCAN_ROOTS/SCAN_EXTENSIONS in \
             tests/neutered_guards.rs.",
            SCAN_ROOTS
        ));
    }
    None
}

/// Say out loud what the gate did **not** check, through a raw
/// `std::io::stderr()` handle rather than `eprintln!` — libtest's capture hook
/// intercepts the `print!`/`eprint!` macros and discards output from *passing*
/// tests, which is exactly what a clean run produces. A silent gate stops
/// teaching anything.
fn write_scope_note(file_count: usize, marker_count: usize) {
    let mut err = std::io::stderr();
    let _ = writeln!(
        err,
        "\nneutered-guard gate: {file_count} file(s) scanned for {marker_count} sabotage \
         marker(s) across {:?}.\n\
         Limits: (1) it does NOT cover the session wrap-up sweep — scripts/evolve.sh commits \
         with `git add -A` and no build/test check, and that file is protected, so a marker \
         swept in there is invisible here; what this closes is the task-gate window. \
         (2) For the compile-visible case it is redundant with rustc — a `return` before a live \
         body is `unreachable_code`, which clippy -D warnings already makes fatal. \
         (3) The genuinely new coverage is the SILENT case: a neutered branch no test covers, \
         and a neutered helper in scripts/*.py, where cargo test gives zero coverage. \
         (4) It is a text scan, not a parser: a marker inside a string literal or a quoting \
         comment fires exactly as a real sabotage does. See tests/neutered_guards.rs.\n",
        SCAN_ROOTS
    );
    let _ = err.flush();
}

#[test]
fn no_deliberate_sabotage_marker_survives_in_the_tree() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let markers = sabotage_markers();

    let mut files = Vec::new();
    for dir in SCAN_ROOTS {
        collect_files(&root.join(dir), &root, &mut files);
    }

    // Anti-vacuous FIRST: a broken walk or an empty marker set must fail as
    // itself, never as a clean bill of health.
    if let Some(msg) = anti_vacuous_failure(files.len(), markers.len()) {
        panic!("neutered-guard gate is vacuous: {msg}");
    }

    write_scope_note(files.len(), markers.len());

    let mut found = Vec::new();
    for (path, source) in &files {
        found.extend(scan_source(path, source, &markers));
    }

    let violations = classify(&found, REGISTERED_MARKERS);
    if !violations.is_empty() {
        let report = violations
            .iter()
            .map(|v| format!("  - {}", v.message()))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "neutered-guard gate failed ({} violation(s)):\n{report}\n\n\
             This gate lives in tests/neutered_guards.rs.",
            violations.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line carrying `markers[i]`, assembled at runtime so this test file
    /// cannot match itself.
    fn line_with(markers: &[String], i: usize) -> String {
        format!("    return true; // {} POSITIVE CONTROL", markers[i])
    }

    #[test]
    fn scan_source_finds_every_marker_with_one_based_lines() {
        let markers = sabotage_markers();
        assert_eq!(
            markers.len(),
            4,
            "the marker set changed — update this table"
        );

        for (i, marker) in markers.iter().enumerate() {
            let source = format!("fn a() {{}}\n{}\nfn b() {{}}\n", line_with(&markers, i));
            let found = scan_source("src/x.rs", &source, &markers);
            assert_eq!(
                found,
                vec![FoundMarker {
                    path: "src/x.rs".to_string(),
                    line: 2,
                    marker: marker.clone(),
                }],
                "marker {i} was not found on line 2"
            );
        }
    }

    #[test]
    fn scan_source_is_case_sensitive_so_honest_prose_does_not_fire() {
        // The near-miss guard, and the half that matters: this repo's prose is
        // full of honest lowercase uses ("neutering the capper fails exactly 4
        // tests"), and a gate that fires on them trains the next reader to
        // paste past it. A discriminator tested only on the side that fires is
        // vacuous green.
        let markers = sabotage_markers();
        let honest = format!(
            "// {} the capper fails exactly 4 tests; a {} marker is a label.\n\
             // Positive control run rather than assumed. POSITIVE CONTROL.\n\
             let x = 1;\n",
            "neutering", "sabotage"
        );
        assert_eq!(
            scan_source("src/x.rs", &honest, &markers),
            vec![],
            "lowercase prose must not fire the gate"
        );
    }

    #[test]
    fn an_unregistered_marker_is_fatal_and_prints_both_remedies() {
        let markers = sabotage_markers();
        let found = vec![FoundMarker {
            path: "src/cli.rs".to_string(),
            line: 42,
            marker: markers[0].clone(),
        }];
        let violations = classify(&found, &[]);
        assert_eq!(violations.len(), 1);

        let msg = violations[0].message();
        assert!(
            msg.contains("src/cli.rs:42"),
            "message must name the site: {msg}"
        );
        assert!(
            msg.contains("RESTORE THE GUARD YOU BROKE"),
            "restore must be stated first as the best remedy: {msg}"
        );
        assert!(
            msg.contains("(\"src/cli.rs\", 42, \"<why this marker is deliberately permanent>\")"),
            "message must print the pasteable register line: {msg}"
        );
        assert!(
            msg.contains("It forbids an unnamed one"),
            "message must say the gate forbids an unnamed marker, not a marker: {msg}"
        );
    }

    #[test]
    fn a_registered_marker_is_not_a_violation() {
        // The other near-miss guard: the escape hatch IS the point, so a named
        // marker must pass cleanly.
        let markers = sabotage_markers();
        let found = vec![FoundMarker {
            path: "scripts/x.py".to_string(),
            line: 7,
            marker: markers[2].clone(),
        }];
        let register: &[(&str, usize, &str)] =
            &[("scripts/x.py", 7, "deliberately permanent, see #123")];
        assert_eq!(classify(&found, register), vec![]);
    }

    #[test]
    fn the_ratchet_fires_when_a_registered_marker_is_gone() {
        let register: &[(&str, usize, &str)] = &[("src/gone.rs", 9, "was a control")];
        let violations = classify(&[], register);
        assert_eq!(
            violations,
            vec![SabotageViolation::RegisteredMarkerGone {
                path: "src/gone.rs".to_string(),
                line: 9,
                reason: "was a control".to_string(),
            }]
        );
        assert!(
            violations[0].message().contains("the debt is paid"),
            "the ratchet must say the debt is paid: {}",
            violations[0].message()
        );
    }

    #[test]
    fn an_empty_reason_is_fatal() {
        let markers = sabotage_markers();
        let found = vec![FoundMarker {
            path: "src/a.rs".to_string(),
            line: 3,
            marker: markers[1].clone(),
        }];
        // Reason is whitespace-only: present, and still not a name.
        let register: &[(&str, usize, &str)] = &[("src/a.rs", 3, "   ")];
        let violations = classify(&found, register);
        assert_eq!(
            violations,
            vec![SabotageViolation::EmptyReason {
                path: "src/a.rs".to_string(),
                line: 3,
            }]
        );
    }

    #[test]
    fn anti_vacuous_fires_on_an_empty_walk_and_an_empty_marker_set() {
        assert!(anti_vacuous_failure(0, 4).is_some(), "empty walk must fail");
        assert!(
            anti_vacuous_failure(135, 0).is_some(),
            "empty marker set must fail"
        );
        // The near-miss: a real scan must NOT trip the anti-vacuous branch.
        assert_eq!(anti_vacuous_failure(135, 4), None);
    }

    #[test]
    fn gate_does_not_flag_its_own_source() {
        // The sharpest landmine in this gate: it scans itself. Every needle and
        // every fixture above is assembled at runtime, and this pins that the
        // discipline held.
        let markers = sabotage_markers();
        let me = include_str!("neutered_guards.rs");
        let found = scan_source("tests/neutered_guards.rs", me, &markers);
        assert_eq!(
            found,
            vec![],
            "this gate matched its own source — a needle or fixture was written as a whole \
             literal instead of being assembled at runtime"
        );
    }
}
