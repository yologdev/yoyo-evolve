//! Feature-gated test-visibility gate: every `tests/*.rs` file whose first
//! attribute hides the whole file behind a cargo feature must be registered as
//! a deliberate, named exception.
//!
//! Why this exists. `tests/gasp_cli_run_ordering.rs` carries a file-level inner
//! `cfg(feature = "gasp")` attribute. The `gasp` feature is default-off, so a
//! plain `cargo test` compiles that file to **zero tests** — and
//! `scripts/evolve.sh:376` runs plain `cargo test`. On Day 179 three
//! consecutive evolve sessions (02:01, 03:29, 03:49) each recorded
//! `tasks 2/2 ✅ — build OK, tests OK` while `main` was CI-red on exactly that
//! test.
//!
//! The harness gate was not reporting a false green. It was reporting a **true
//! green over a denominator that silently excluded the failing test**. That is
//! my own rule — *"could not check" must never read as "checked; clean"* —
//! failing inside the one instrument every one of my 178 days of self-model
//! calibration has been graded by. `scripts/evolve.sh` is protected, so the
//! remedy has to live on my side of the boundary; this is that enabler.
//!
//! Shape borrowed verbatim from its five siblings (`tests/module_size.rs`,
//! `tests/blind_round_grades.rs`, `tests/orphan_modules.rs`,
//! `tests/doc_version_claims.rs`, `tests/global_state_races.rs`): walk the
//! tree, split the pure decision from the I/O, fail on the *unnamed* case,
//! allow a **registered** exception, and ratchet the register so it can only
//! shrink.
//!
//! This gate file is deliberately **not** feature-gated. A gate that hides
//! itself behind the exact mechanism it exists to enumerate would be the joke
//! writing itself.
//!
//! # What this gate CANNOT do
//!
//! Read this part before trusting a green run.
//!
//! 1. **It matches the file-level inner attribute only.** A file whose
//!    individual tests carry per-item `cfg(feature = ...)` attributes is
//!    *partially* invisible to a plain `cargo test`, and this gate says nothing
//!    at all about it.
//! 2. **It does not verify that anything actually runs the named feature.**
//!    "Registered" means *acknowledged*, never *covered*. The register records
//!    that I knew a file was invisible to the harness — it is not evidence that
//!    some other runner compiles it.
//! 3. **It is a text scan, not a Rust parser.** An attribute inside a comment
//!    or a string literal would be mis-read (there are none today, and the
//!    needle this scanner looks for is assembled at runtime precisely so this
//!    file's own source cannot match itself).
//!
//! And the largest limit, stated plainly rather than implied: this makes the
//! blindness **named**, not **cured**. The harness still does not run those
//! tests. Only CI does.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Integration-test files that deliberately hide behind a cargo feature.
///
/// Entries are `(path, feature, why this is acceptable and what actually runs
/// it)`.
///
/// **Debt, not absolution.** An entry does not make a file visible to the
/// harness; it records by hand that a test file exists which a plain
/// `cargo test` compiles to nothing, and that I knew. The ratchet below (a
/// registered file that loses its gate, changes feature, or vanishes is fatal)
/// is what stops it from becoming permission.
const REGISTERED_FEATURE_GATED_TESTS: &[(&str, &str, &str)] = &[(
    "tests/gasp_cli_run_ordering.rs",
    "gasp",
    "#831's four-process run-ordering check needs a gasp-featured binary, and the \
     `gasp` feature is default-off (#683). What actually runs it is CI's \
     `Test (--features gasp)` step (.github/workflows/ci.yml:45,52). What never runs \
     it is the harness gate: scripts/evolve.sh:376 runs plain `cargo test`, which \
     compiles this file to zero tests. Named, not cured.",
)];

/// A way the feature-gate register can be violated. Four values running in
/// **two opposite directions** — the same two-direction discipline every
/// sibling gate uses.
#[derive(Debug, PartialEq, Eq)]
enum FeatureGateViolation {
    /// The walk found no test files at all. A scanner that finds nothing and
    /// passes is the vacuous-green shape, so this is fatal rather than quiet.
    EmptyScan,
    /// A feature-gated test file nobody named. The defect this gate exists for.
    Unregistered { path: String, feature: String },
    /// Ratchet: a registered file that no longer carries a file-level gate.
    RegisteredNoLongerGated { path: String, feature: String },
    /// Ratchet: a registered file whose gate now names a different feature.
    RegisteredFeatureChanged {
        path: String,
        recorded: String,
        actual: String,
    },
    /// Ratchet: a registered file that has vanished.
    RegisteredFileVanished { path: String, feature: String },
    /// A register entry whose reason is empty or whitespace-only. An unnamed
    /// debt wearing a name is not a name.
    EmptyReason { path: String, feature: String },
}

impl FeatureGateViolation {
    fn message(&self) -> String {
        match self {
            FeatureGateViolation::EmptyScan => "the walk over tests/ found zero *.rs files — the walk is broken, not the repo. \
                 A scanner that finds nothing and passes is exactly the vacuous green this gate \
                 exists to refuse."
                .to_string(),
            FeatureGateViolation::Unregistered { path, feature } => format!(
                "{path} hides its whole file behind cargo feature `{feature}`, so a plain \
                 `cargo test` compiles it to ZERO tests — and scripts/evolve.sh:376 runs plain \
                 `cargo test`, so the harness gate can never see it fail.\n     \
                 Fix (either one): remove the file-level gate so the harness runs it, or add\n       \
                 (\"{path}\", \"{feature}\", \"<why this is acceptable and what actually runs it>\"),\n     \
                 to REGISTERED_FEATURE_GATED_TESTS in tests/feature_gated_tests.rs.\n     \
                 The gate does not forbid a feature-gated test file. It forbids an unnamed one."
            ),
            FeatureGateViolation::RegisteredNoLongerGated { path, feature } => format!(
                "{path} is registered as gated behind `{feature}`, but it no longer carries a \
                 file-level feature gate — the debt is paid.\n     \
                 Fix: delete its entry from REGISTERED_FEATURE_GATED_TESTS in \
                 tests/feature_gated_tests.rs. Fatal on purpose: the register only ratchets down \
                 if a repair is also a failure."
            ),
            FeatureGateViolation::RegisteredFeatureChanged {
                path,
                recorded,
                actual,
            } => format!(
                "{path} is registered as gated behind `{recorded}`, but the file now gates on \
                 `{actual}`.\n     \
                 Fix: update its entry in REGISTERED_FEATURE_GATED_TESTS in \
                 tests/feature_gated_tests.rs to read `{actual}`, and re-read the reason beside \
                 it — a different feature is a different claim about what runs the file."
            ),
            FeatureGateViolation::RegisteredFileVanished { path, feature } => format!(
                "{path} is registered as gated behind `{feature}`, but no such file exists under \
                 tests/ — the debt is paid (or the file was renamed).\n     \
                 Fix: delete its entry from REGISTERED_FEATURE_GATED_TESTS in \
                 tests/feature_gated_tests.rs, or re-register it under its new path."
            ),
            FeatureGateViolation::EmptyReason { path, feature } => format!(
                "{path} is registered as gated behind `{feature}` with an empty reason.\n     \
                 Fix: write, in REGISTERED_FEATURE_GATED_TESTS in \
                 tests/feature_gated_tests.rs, why this is acceptable and what actually runs the \
                 file. An unnamed debt wearing a name is not a name."
            ),
        }
    }
}

/// The literal a file-level feature gate starts with, **assembled at runtime**.
///
/// This is not decoration. The scanner below walks every `tests/*.rs` file,
/// which includes this one; a compile-time literal here would sit in this
/// file's own source and the gate would classify itself as feature-gated. Same
/// needle-built-at-runtime discipline the source-level guards in `src/` use.
fn attr_prefix() -> String {
    format!("#!{}", "[cfg(feature")
}

/// The feature name of a file-level inner `cfg(feature = "...")` attribute, or
/// `None` when the file carries none.
///
/// Deliberately narrow: the *trimmed* line must **start** with the inner
/// attribute marker, so a mention inside a `//!` doc comment or an indented
/// string literal is not a match. See limit 3 in the module doc — this is a
/// text scan, not a Rust parser.
fn file_level_feature_gate(source: &str) -> Option<String> {
    let prefix = attr_prefix();
    for line in source.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(prefix.as_str()) else {
            continue;
        };
        let mut quoted = rest.split('"');
        quoted.next()?;
        let name = quoted.next()?;
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

/// Pure checker: given every `tests/*.rs` file paired with the feature its
/// file-level gate names (if any), and the register, report every violation.
///
/// No I/O, so both fatal directions are provable against a **fabricated** file
/// list rather than by planting a real feature-gated file in `tests/`.
fn classify(
    files: &[(String, Option<String>)],
    register: &[(&str, &str, &str)],
) -> Vec<FeatureGateViolation> {
    if files.is_empty() {
        return vec![FeatureGateViolation::EmptyScan];
    }

    let mut violations = Vec::new();

    for (path, gate) in files {
        let entry = register.iter().find(|(p, _, _)| p == path);
        match (gate, entry) {
            (Some(feature), None) => violations.push(FeatureGateViolation::Unregistered {
                path: path.clone(),
                feature: feature.clone(),
            }),
            (Some(actual), Some((_, recorded, reason))) => {
                if actual != recorded {
                    violations.push(FeatureGateViolation::RegisteredFeatureChanged {
                        path: path.clone(),
                        recorded: (*recorded).to_string(),
                        actual: actual.clone(),
                    });
                } else if reason.trim().is_empty() {
                    violations.push(FeatureGateViolation::EmptyReason {
                        path: path.clone(),
                        feature: actual.clone(),
                    });
                }
            }
            (None, Some((_, recorded, _))) => {
                violations.push(FeatureGateViolation::RegisteredNoLongerGated {
                    path: path.clone(),
                    feature: (*recorded).to_string(),
                })
            }
            (None, None) => {}
        }
    }

    // A registered file that no longer exists is its own case — reported, not
    // silently ignored, because a rename would otherwise retire an entry
    // without anyone deciding to.
    for (path, feature, _) in register {
        if !files.iter().any(|(p, _)| p == path) {
            violations.push(FeatureGateViolation::RegisteredFileVanished {
                path: (*path).to_string(),
                feature: (*feature).to_string(),
            });
        }
    }

    violations
}

/// Collect top-level `tests/*.rs` files as `(relative path, source text)` with
/// forward slashes so the register reads the same on every platform.
///
/// Top-level only, deliberately: cargo builds one integration-test binary per
/// `tests/*.rs`, and a file in a subdirectory is a module included by one of
/// those rather than a target of its own.
fn collect_test_files(dir: &Path, root: &Path) -> Vec<(String, Option<String>)> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => panic!("cannot read {}: {e}", dir.display()),
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();

    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() || !path.extension().is_some_and(|e| e == "rs") {
            continue;
        }
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push((rel, file_level_feature_gate(&content)));
    }
    out
}

/// Say out loud what the gate did **not** check, through a raw stderr handle
/// rather than `eprintln!` — libtest's capture hook discards macro output from
/// *passing* tests, and a limit disclosure that only prints on failure is a
/// disclosure nobody reads.
fn write_limits(file_count: usize, gated_count: usize) {
    let mut err = std::io::stderr();
    let _ = writeln!(
        err,
        "\nfeature-gated test-visibility gate: {file_count} tests/*.rs file(s) scanned, \
         {gated_count} hidden behind a file-level cargo feature and registered.\n\
         Limits: (1) it sees the file-level inner attribute only — per-item \
         `cfg(feature = ...)` on individual tests is invisible to it; (2) it does NOT verify \
         that anything runs the named feature, so \"registered\" means acknowledged, never \
         covered; (3) it is a text scan, not a Rust parser.\n\
         This gate makes the blindness named, not cured: the harness still runs plain \
         `cargo test`, so those files are still zero tests there. See \
         tests/feature_gated_tests.rs.\n"
    );
    let _ = err.flush();
}

#[test]
fn every_feature_gated_test_file_is_registered() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let files = collect_test_files(&root.join("tests"), &root);

    let gated = files.iter().filter(|(_, g)| g.is_some()).count();
    write_limits(files.len(), gated);

    let violations = classify(&files, REGISTERED_FEATURE_GATED_TESTS);
    if !violations.is_empty() {
        let report = violations
            .iter()
            .map(|v| format!("  - {}", v.message()))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "feature-gated test-visibility gate failed ({} violation(s)):\n{report}\n\n\
             This gate lives in tests/feature_gated_tests.rs.",
            violations.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gated(v: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
        v.iter()
            .map(|(p, g)| ((*p).to_string(), g.map(|s| s.to_string())))
            .collect()
    }

    #[test]
    fn detects_a_file_level_inner_attribute() {
        let src = format!(
            "//! doc\n{}\n\nuse std::io;\n",
            "#![cfg(feature = \"gasp\")]"
        );
        assert_eq!(file_level_feature_gate(&src), Some("gasp".to_string()));
    }

    #[test]
    fn an_ungated_file_reports_none() {
        assert_eq!(file_level_feature_gate("//! doc\nuse std::io;\n"), None);
    }

    /// Limit 1, pinned as a test rather than only as prose: a per-item
    /// attribute is invisible to this gate.
    #[test]
    fn per_item_attributes_are_invisible() {
        let src = format!("{}\nfn hidden() {{}}\n", "#[cfg(feature = \"gasp\")]");
        assert_eq!(file_level_feature_gate(&src), None);
    }

    /// The needle is built at runtime for exactly this reason: the gate scans
    /// `tests/*.rs`, which includes itself, and a compile-time literal would
    /// make it classify its own source as feature-gated.
    #[test]
    fn this_gate_file_does_not_classify_itself_as_gated() {
        let me = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/feature_gated_tests.rs"
        ))
        .expect("gate file must be readable");
        assert_eq!(file_level_feature_gate(&me), None);
    }

    /// Anti-vacuous: a walk that finds nothing must fail loudly, not pass.
    #[test]
    fn an_empty_scan_is_fatal() {
        let v = classify(&[], &[]);
        assert_eq!(v, vec![FeatureGateViolation::EmptyScan]);
        assert!(
            v[0].message().contains("vacuous green"),
            "{}",
            v[0].message()
        );
    }

    #[test]
    fn an_ungated_unregistered_file_passes() {
        let v = classify(&gated(&[("tests/a.rs", None)]), &[]);
        assert!(v.is_empty(), "{v:?}");
    }

    /// Branch 1: the defect this gate exists for.
    #[test]
    fn an_unregistered_gated_file_is_fatal() {
        let v = classify(&gated(&[("tests/a.rs", Some("gasp"))]), &[]);
        assert_eq!(
            v,
            vec![FeatureGateViolation::Unregistered {
                path: "tests/a.rs".to_string(),
                feature: "gasp".to_string(),
            }]
        );
        let msg = v[0].message();
        assert!(msg.contains("ZERO tests"), "{msg}");
        assert!(msg.contains("scripts/evolve.sh:376"), "{msg}");
        // The pasteable remedy line, verbatim.
        assert!(
            msg.contains("(\"tests/a.rs\", \"gasp\", \""),
            "remedy line must be pasteable: {msg}"
        );
        assert!(msg.contains("It forbids an unnamed one"), "{msg}");
    }

    #[test]
    fn a_registered_gated_file_passes() {
        let v = classify(
            &gated(&[("tests/a.rs", Some("gasp"))]),
            &[("tests/a.rs", "gasp", "CI runs it with --features gasp")],
        );
        assert!(v.is_empty(), "{v:?}");
    }

    /// Ratchet, direction 1: the gate was removed.
    #[test]
    fn a_registered_file_that_lost_its_gate_is_fatal() {
        let v = classify(
            &gated(&[("tests/a.rs", None)]),
            &[("tests/a.rs", "gasp", "CI runs it")],
        );
        assert_eq!(
            v,
            vec![FeatureGateViolation::RegisteredNoLongerGated {
                path: "tests/a.rs".to_string(),
                feature: "gasp".to_string(),
            }]
        );
        assert!(
            v[0].message().contains("the debt is paid"),
            "{}",
            v[0].message()
        );
    }

    /// Ratchet, direction 2: the recorded feature name went stale.
    #[test]
    fn a_registered_file_whose_feature_changed_is_fatal() {
        let v = classify(
            &gated(&[("tests/a.rs", Some("newname"))]),
            &[("tests/a.rs", "gasp", "CI runs it")],
        );
        assert_eq!(
            v,
            vec![FeatureGateViolation::RegisteredFeatureChanged {
                path: "tests/a.rs".to_string(),
                recorded: "gasp".to_string(),
                actual: "newname".to_string(),
            }]
        );
        let msg = v[0].message();
        // Both numbers verbatim, so the fix is a one-line edit.
        assert!(msg.contains("`gasp`") && msg.contains("`newname`"), "{msg}");
    }

    /// Ratchet, direction 3: the file vanished.
    #[test]
    fn a_registered_file_that_vanished_is_fatal() {
        let v = classify(
            &gated(&[("tests/other.rs", None)]),
            &[("tests/gone.rs", "gasp", "CI runs it")],
        );
        assert_eq!(
            v,
            vec![FeatureGateViolation::RegisteredFileVanished {
                path: "tests/gone.rs".to_string(),
                feature: "gasp".to_string(),
            }]
        );
    }

    /// An unnamed debt wearing a name is not a name.
    #[test]
    fn a_registered_entry_with_a_blank_reason_is_fatal() {
        let v = classify(
            &gated(&[("tests/a.rs", Some("gasp"))]),
            &[("tests/a.rs", "gasp", "   ")],
        );
        assert_eq!(
            v,
            vec![FeatureGateViolation::EmptyReason {
                path: "tests/a.rs".to_string(),
                feature: "gasp".to_string(),
            }]
        );
    }

    /// The live register must not be vacuous either: it ships with exactly one
    /// honest entry, and every entry must carry a real reason.
    #[test]
    fn the_live_register_is_non_empty_and_every_reason_is_written() {
        assert!(
            !REGISTERED_FEATURE_GATED_TESTS.is_empty(),
            "the register ships with one known entry; an empty register means the walk or the \
             register drifted"
        );
        for (path, feature, reason) in REGISTERED_FEATURE_GATED_TESTS {
            assert!(!path.is_empty() && !feature.is_empty(), "{path}/{feature}");
            assert!(
                reason.trim().len() > 20,
                "{path} needs a reason naming what actually runs it"
            );
        }
    }
}
