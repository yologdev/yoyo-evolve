//! Orphan-module gate: every `*.rs` file under `src/` must be named by some
//! `mod` declaration, or be registered as a deliberate exception.
//!
//! Why this exists. On Day 173 `src/commands_risk_families.rs` was a tracked,
//! 296-line source file that rustc never saw — no `mod commands_risk_families;`
//! declaration named it anywhere. `cargo build`, `cargo test`,
//! `clippy --all-targets -- -D warnings` and `cargo fmt -- --check` are all
//! *structurally* blind to such a file: it can be written, committed, reviewed
//! and reported as a delivered extraction while never being part of the
//! program. Nothing I owned could tell me that a source file I had written was
//! not part of me. This gate is that missing sense.
//!
//! Shape borrowed verbatim from its two siblings (`tests/module_size.rs`,
//! `tests/blind_round_grades.rs`): walk the tree, split the pure decision from
//! the I/O, fail on the unnamed case, allow a *registered* exception, and
//! ratchet the register so it can only shrink.
//!
//! # What this gate CANNOT do
//!
//! Read this part before trusting a green run. "Could not check" must not read
//! as "checked; clean".
//!
//! 1. **It matches `mod NAME;` by name, not by resolved module path.** Two
//!    files with the same stem in different directories (`src/a/util.rs` and
//!    `src/b/util.rs`) are indistinguishable to it, so a single `mod util;`
//!    anywhere vouches for both — one of them could still be an orphan and this
//!    gate would call it reachable.
//! 2. **It does not parse `#[cfg(...)]`.** A `mod` behind a disabled feature
//!    counts as reachable. That is deliberate — `src/gasp.rs` is
//!    `#[cfg(feature = "gasp")]` and genuinely *is* part of the crate — but it
//!    means "reachable" here means **declared**, not "compiled in this
//!    configuration". Nor does it read `#[path = "..."]` attributes (there are
//!    none in `src/` today); one would silently break the name match.
//! 3. **It says nothing about whether the code inside is used.** A file can be
//!    declared, compiled, and entirely dead. Clippy's `dead_code` lint covers
//!    that — but only once the file is compiled at all, which is exactly the
//!    precondition this gate checks.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Files under `src/` that deliberately have no `mod` declaration.
///
/// **Debt, not absolution.** An entry does not make a file part of the crate;
/// it records by hand that an uncompiled source file is sitting in `src/` and
/// that I knew. The ratchet below (a registered file that becomes reachable, or
/// vanishes, is fatal) is what stops it from becoming permission.
///
/// It ships **empty**, and that is the honest state: there are no deliberate
/// exceptions today. The one real orphan that motivated this gate
/// (`src/commands_risk_families.rs`, a duplicate of live code in
/// `src/commands_risk_epistemic.rs`) was **deleted**, not registered —
/// registering it would have laundered the exact defect this gate exists to
/// catch.
const REGISTERED_ORPHANS: &[(&str, &str)] = &[];

/// A way the orphan gate can be violated. Two values, running in **opposite
/// directions** — the same two-direction discipline the module-size gate uses.
#[derive(Debug, PartialEq, Eq)]
enum OrphanViolation {
    /// A `src/` file that no `mod` declaration names and that is not in the
    /// register. The defect this gate exists for.
    Unreachable { path: String },
    /// A registered file that is now reachable, or has vanished. The ratchet:
    /// an exception list only pays itself down if *improving* is also a
    /// failure, otherwise progress leaves silent headroom nobody granted.
    RegisteredDebtPaid { path: String, reason: String },
}

impl OrphanViolation {
    fn message(&self) -> String {
        match self {
            OrphanViolation::Unreachable { path } => format!(
                "{path} is a tracked source file that no `mod` declaration names, so rustc \
                 never compiles it — cargo build, cargo test, clippy and fmt --check are all \
                 blind to it.\n     \
                 Fix (either one): add `mod {};` to the right parent module (usually \
                 src/main.rs), or — if it is deliberately not compiled — add \
                 (\"{path}\", \"<reason>\") to REGISTERED_ORPHANS in tests/orphan_modules.rs.\n     \
                 The gate does not forbid an uncompiled file. It forbids an unnamed one.",
                module_stem(path),
            ),
            OrphanViolation::RegisteredDebtPaid { path, reason } => format!(
                "{path} is listed in REGISTERED_ORPHANS but is now reachable (or no longer \
                 exists) — the debt is paid.\n     \
                 Fix: delete the line (\"{path}\", \"{reason}\") from REGISTERED_ORPHANS in \
                 tests/orphan_modules.rs. Fatal on purpose: the register only ratchets down \
                 if a repair is also a failure."
            ),
        }
    }
}

/// The module name a `src/` path is declared under: `src/a/b.rs` → `b`,
/// `src/x/mod.rs` → `x`, `src/main.rs` → `main`.
///
/// This is the whole of the name-based matching — see limit 1 in the module
/// doc: the *directory* is discarded, so stems collide across directories.
fn module_stem(path: &str) -> String {
    let mut parts = path.rsplit('/');
    let file = parts.next().unwrap_or(path);
    let stem = file.strip_suffix(".rs").unwrap_or(file);
    if stem == "mod" {
        parts.next().unwrap_or(stem).to_string()
    } else {
        stem.to_string()
    }
}

/// Extract every `mod NAME;` declaration from one file's source text.
///
/// Deliberately narrow: a **semicolon** declaration only. An inline
/// `mod tests { ... }` block declares no file, so it must not vouch for one.
/// `pub` / `pub(crate)` prefixes are accepted; a trailing line comment is not
/// parsed away because none exists in `src/` today and a false *positive* here
/// only ever makes the gate more permissive, never falsely fatal.
fn declared_mods(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let mut rest = line.trim();
        for prefix in ["pub(crate) ", "pub ", "pub(super) "] {
            if let Some(stripped) = rest.strip_prefix(prefix) {
                rest = stripped.trim_start();
            }
        }
        let Some(rest) = rest.strip_prefix("mod ") else {
            continue;
        };
        let Some(name) = rest.trim().strip_suffix(';') else {
            continue;
        };
        let name = name.trim();
        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            out.push(name.to_string());
        }
    }
    out
}

/// Pure checker: given every `src/` file, every `mod` name declared anywhere in
/// them, and the register, report every violation.
///
/// No I/O, so the fatal branch is provable against a fabricated file list
/// rather than by creating a real orphan on disk.
fn classify(
    files: &[String],
    declared: &[String],
    register: &[(&str, &str)],
) -> Vec<OrphanViolation> {
    let mut violations = Vec::new();

    let reachable = |path: &String| -> bool {
        // The crate root is reachable by definition — nothing declares it.
        path == "src/main.rs" || declared.iter().any(|d| *d == module_stem(path))
    };

    for path in files {
        let registered = register.iter().any(|(p, _)| p == path);
        match (reachable(path), registered) {
            (false, false) => violations.push(OrphanViolation::Unreachable { path: path.clone() }),
            (true, true) => {
                let reason = register
                    .iter()
                    .find(|(p, _)| p == path)
                    .map(|(_, r)| (*r).to_string())
                    .unwrap_or_default();
                violations.push(OrphanViolation::RegisteredDebtPaid {
                    path: path.clone(),
                    reason,
                });
            }
            _ => {}
        }
    }

    // A registered file that no longer exists is its own case — reported, not
    // silently ignored, because a rename would otherwise retire an entry
    // without anyone deciding to.
    for (path, reason) in register {
        if !files.iter().any(|p| p == path) {
            violations.push(OrphanViolation::RegisteredDebtPaid {
                path: (*path).to_string(),
                reason: (*reason).to_string(),
            });
        }
    }

    violations
}

/// Recursively collect `*.rs` files under `dir`, returning `(relative path,
/// source text)` with forward slashes so the register reads the same on every
/// platform.
fn collect_rs_files(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => panic!("cannot read {}: {e}", dir.display()),
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();

    for path in paths {
        if path.is_dir() {
            collect_rs_files(&path, root, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, content));
        }
    }
}

/// Say out loud what the gate did **not** check, through a raw stderr handle
/// rather than `eprintln!` — libtest's capture hook discards macro output from
/// *passing* tests, and a limit disclosure that only prints on failure is a
/// disclosure nobody reads.
fn write_scope_note(file_count: usize) {
    let mut err = std::io::stderr();
    let _ = writeln!(
        err,
        "\norphan-module gate: {file_count} file(s) checked for a `mod` declaration by name. \
         It does not resolve module paths (same-stem files in different directories are \
         indistinguishable), does not read `#[cfg]`/`#[path]`, and says nothing about whether \
         declared code is used. See tests/orphan_modules.rs.\n"
    );
    let _ = err.flush();
}

#[test]
fn every_src_file_is_named_by_a_mod_declaration() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &root, &mut files);

    assert!(
        files.len() > 10,
        "module walk found only {} files — the walk is broken, not the repo",
        files.len()
    );

    let mut declared: Vec<String> = Vec::new();
    for (_, source) in &files {
        declared.extend(declared_mods(source));
    }
    let paths: Vec<String> = files.iter().map(|(p, _)| p.clone()).collect();

    write_scope_note(paths.len());

    let violations = classify(&paths, &declared, REGISTERED_ORPHANS);
    if !violations.is_empty() {
        let report = violations
            .iter()
            .map(|v| format!("  - {}", v.message()))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "orphan-module gate failed ({} violation(s)):\n{report}\n\n\
             This gate lives in tests/orphan_modules.rs.",
            violations.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn module_stem_handles_files_dirs_and_root() {
        assert_eq!(module_stem("src/a.rs"), "a");
        assert_eq!(module_stem("src/format/highlight.rs"), "highlight");
        assert_eq!(module_stem("src/format/mod.rs"), "format");
        assert_eq!(module_stem("src/main.rs"), "main");
    }

    #[test]
    fn declared_mods_reads_semicolon_declarations_only() {
        let src = "\
mod a;
pub mod b;
pub(crate) mod c;
    #[cfg(test)] mod ignored_same_line;
mod inline { fn f() {} }
mod tests {
";
        // The inline blocks declare no file and must not vouch for one.
        assert_eq!(declared_mods(src), owned(&["a", "b", "c"]));
    }

    #[test]
    fn a_declared_file_passes() {
        let v = classify(&owned(&["src/main.rs", "src/a.rs"]), &owned(&["a"]), &[]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn a_dir_module_is_reached_by_its_directory_name() {
        let v = classify(
            &owned(&["src/main.rs", "src/format/mod.rs", "src/format/cost.rs"]),
            &owned(&["format", "cost"]),
            &[],
        );
        assert!(v.is_empty(), "{v:?}");
    }

    /// The whole point: a fabricated orphan is fatal, and the message names
    /// both remedies verbatim.
    #[test]
    fn an_undeclared_file_is_a_violation() {
        let v = classify(&owned(&["src/main.rs", "src/orphan.rs"]), &owned(&[]), &[]);
        assert_eq!(
            v,
            vec![OrphanViolation::Unreachable {
                path: "src/orphan.rs".to_string()
            }]
        );
        let msg = v[0].message();
        assert!(msg.contains("mod orphan;"), "{msg}");
        assert!(msg.contains("REGISTERED_ORPHANS"), "{msg}");
        assert!(msg.contains("never compiles it"), "{msg}");
    }

    #[test]
    fn crate_root_needs_no_declaration() {
        let v = classify(&owned(&["src/main.rs"]), &owned(&[]), &[]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn a_registered_orphan_passes_but_stays_named() {
        let v = classify(
            &owned(&["src/main.rs", "src/scratch.rs"]),
            &owned(&[]),
            &[("src/scratch.rs", "kept for X")],
        );
        assert!(v.is_empty(), "{v:?}");
    }

    /// The ratchet, direction 1: the file got declared, so the entry must go.
    #[test]
    fn a_registered_file_that_became_reachable_is_fatal() {
        let v = classify(
            &owned(&["src/main.rs", "src/scratch.rs"]),
            &owned(&["scratch"]),
            &[("src/scratch.rs", "kept for X")],
        );
        assert_eq!(
            v,
            vec![OrphanViolation::RegisteredDebtPaid {
                path: "src/scratch.rs".to_string(),
                reason: "kept for X".to_string(),
            }]
        );
        assert!(
            v[0].message().contains("the debt is paid"),
            "{}",
            v[0].message()
        );
    }

    /// The ratchet, direction 2: the file vanished.
    #[test]
    fn a_registered_file_that_vanished_is_fatal() {
        let v = classify(
            &owned(&["src/main.rs"]),
            &owned(&[]),
            &[("src/gone.rs", "kept for X")],
        );
        assert_eq!(
            v,
            vec![OrphanViolation::RegisteredDebtPaid {
                path: "src/gone.rs".to_string(),
                reason: "kept for X".to_string(),
            }]
        );
    }

    /// Limit 1 from the module doc, pinned as a test rather than only as prose:
    /// one declaration vouches for every same-stem file, whatever directory it
    /// is in. This is a known hole, recorded so it fails loudly if the matching
    /// ever silently changes.
    #[test]
    fn same_stem_files_in_different_dirs_are_indistinguishable() {
        let v = classify(
            &owned(&["src/main.rs", "src/a/util.rs", "src/b/util.rs"]),
            &owned(&["util"]),
            &[],
        );
        assert!(
            v.is_empty(),
            "known limit: one `mod util;` vouches for both — {v:?}"
        );
    }
}
