//! #1135: the `yoyo map` footer names the extractor that actually produced the
//! symbols, counted per file in the loop — never the availability probe.

use super::*;

/// A scratch project of two Rust files, each with one public fn, so every
/// scanned file yields symbols under either extractor.
fn scratch_project() -> (tempfile::TempDir, Vec<String>) {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.rs"), "pub fn alpha() {}\n").unwrap();
    std::fs::write(dir.path().join("b.rs"), "pub fn beta() {}\n").unwrap();
    (dir, vec!["a.rs".to_string(), "b.rs".to_string()])
}

fn run(ast_extractor: Option<&AstExtractor>) -> (Vec<FileSymbols>, MapBackendUsed) {
    let (dir, files) = scratch_project();
    let tl = dir.path().to_string_lossy().to_string();
    let (entries, used, _) =
        build_repo_map_core_with(&files, Some(&tl), None, true, ast_extractor, None);
    (entries, used)
}

fn fake_ast_symbol(name: &str) -> Vec<Symbol> {
    vec![Symbol {
        name: name.to_string(),
        kind: SymbolKind::Function,
        is_public: true,
        line: 1,
    }]
}

/// The issue's case: the probe said ast-grep is available (an extractor is
/// passed), but every scan fails (returns `None`). Every file fell back to
/// regex, so the footer must say regex — not ast-grep.
#[test]
fn probe_available_but_every_scan_fails_reports_regex() {
    let always_fails = |_: &str, _: &str| -> Option<Vec<Symbol>> { None };
    let (entries, used) = run(Some(&always_fails));
    // Anti-vacuous: the regex fallback really did produce the symbols.
    assert_eq!(
        entries.len(),
        2,
        "both files should map via the regex fallback"
    );
    assert_eq!(
        used,
        MapBackendUsed {
            ast_grep_files: 0,
            regex_files: 2
        }
    );
    assert_eq!(used.label(), "using regex");
    assert!(!used.label().contains("ast-grep"));
}

#[test]
fn every_scan_succeeds_reports_ast_grep() {
    let always_ok = |_: &str, _: &str| Some(fake_ast_symbol("from_ast_grep"));
    let (entries, used) = run(Some(&always_ok));
    assert!(entries
        .iter()
        .all(|e| e.symbols.iter().all(|s| s.name == "from_ast_grep")));
    assert_eq!(
        used,
        MapBackendUsed {
            ast_grep_files: 2,
            regex_files: 0
        }
    );
    assert_eq!(used.label(), "using ast-grep");
}

/// Near-miss: ast-grep not tried at all (probe unavailable or `--regex`) —
/// the label is byte-identical to the pre-#1135 footer.
#[test]
fn probe_unavailable_reports_regex_byte_identical() {
    let (entries, used) = run(None);
    assert_eq!(entries.len(), 2);
    assert_eq!(used.label(), "using regex");
}

#[test]
fn mixed_outcomes_report_both_counts() {
    let only_a = |path: &str, _: &str| (path == "a.rs").then(|| fake_ast_symbol("alpha"));
    let (_, used) = run(Some(&only_a));
    assert_eq!(
        used,
        MapBackendUsed {
            ast_grep_files: 1,
            regex_files: 1
        }
    );
    assert_eq!(used.label(), "using ast-grep for 1 file, regex for 1 file");
}

#[test]
fn map_backend_label_table() {
    let rows: &[(usize, usize, &str)] = &[
        (0, 0, "no files scanned"),
        (1, 0, "using ast-grep"),
        (169, 0, "using ast-grep"),
        (0, 1, "using regex"),
        (0, 169, "using regex"),
        (1, 1, "using ast-grep for 1 file, regex for 1 file"),
        (3, 1, "using ast-grep for 3 files, regex for 1 file"),
        (1, 4, "using ast-grep for 1 file, regex for 4 files"),
        (10, 20, "using ast-grep for 10 files, regex for 20 files"),
    ];
    for &(a, r, want) in rows {
        assert_eq!(map_backend_label(a, r), want, "row ({a}, {r})");
    }
}
