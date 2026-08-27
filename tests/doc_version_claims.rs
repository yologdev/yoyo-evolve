//! Freshness gate for prose claims about what an **external dependency**
//! cannot do.
//!
//! Why this exists, stated plainly: `src/gasp.rs` used to assert that
//! `task-result` (#683 item 5) was *unreachable* because `ProjectRef`,
//! `ArtifactRef` and `PatchStatus` "do not appear anywhere in the published
//! yoagent source". That was true when it was written against yoagent 0.16.3.
//! It went stale the moment the pin moved to 0.16.4/0.16.5, where all three
//! are re-exported from `yoagent::gasp` — and nothing failed, because prose
//! has no compiler. **Eight sessions** (#763, #765, #782, #785, #787, #789,
//! #803, plus the docs-only #788) opened that file, read an authoritative
//! "impossible", believed it, and exited with an empty diff.
//!
//! The convention that came out of it: any prose claim asserting what an
//! external dependency *cannot* do carries a marker naming the yoagent
//! version it was checked against, and this test pins every such marker to
//! whatever `Cargo.lock` actually resolves. When the pin moves, this fails and
//! names the file, the line and the remedy — instead of the sentence quietly
//! becoming false again.
//!
//! This lives in `tests/` rather than inside the modules it scans on purpose:
//! `src/gasp.rs` is behind the default-off `gasp` cargo feature, so a
//! `#[cfg(test)]` block inside it would never run in a plain `cargo test` —
//! i.e. exactly the run CI and every evolve session performs. Files are read
//! as *text* here, so the guard works regardless of any feature flag.
//!
//! **What this cannot do, said out loud so "could not check" never reads as
//! "checked; clean":** a marker checks that *someone looked at the current
//! version*, never that the prose above it is correct. Presence is
//! mechanically checkable; truth is not. A paragraph that is flatly wrong and
//! carries a current marker passes exactly as an honest one does — the same
//! limit `tests/blind_round_grades.rs` states about grades.
//!
//! Deliberately narrow, three ways:
//! * Only markers are checked. Prose elsewhere may freely mention other
//!   versions as history ("landed in 0.16.4") — that is a statement about the
//!   past, not a claim about the current build, and must not fail this test.
//! * There is **no** blanket scan for "impossible"/"unreachable" wording. It
//!   would fire on the many honest uses of those words and train the next
//!   reader to ignore the gate.
//! * The convention is for *external dependency* claims only. A marker on a
//!   claim about yoyo's own code would pin it to a version that has nothing to
//!   do with it.

use std::path::{Path, PathBuf};

/// The marker carrying the yoagent version a dependency claim was checked
/// against. Note the trailing colon: prose may name the convention
/// (`yoagent-version-claim`, no colon) without becoming a marker itself.
const MARKER: &str = "yoagent-version-claim:";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read {} — {e}", path.display()))
}

/// Pull the resolved `yoagent` version out of `Cargo.lock`: the first
/// `version = ` line following the `name = "yoagent"` line. Matched on the
/// exact name so that a package merely sharing the prefix (`yoagent-state`)
/// cannot be picked up by accident.
fn locked_yoagent_version(lock: &str) -> Option<String> {
    let mut in_yoagent = false;
    for line in lock.lines() {
        let line = line.trim();
        if line.starts_with("name = ") {
            in_yoagent = line == r#"name = "yoagent""#;
            continue;
        }
        if in_yoagent {
            if let Some(rest) = line.strip_prefix("version = ") {
                return Some(rest.trim_matches('"').to_string());
            }
        }
    }
    None
}

/// Every marker in `source`, as `(1-based line number, claimed version)`.
/// Pure: the caller supplies the text, so this is table-testable without
/// touching the filesystem.
fn markers_in(source: &str) -> Vec<(usize, String)> {
    source
        .lines()
        .enumerate()
        .filter_map(|(i, line)| line.split_once(MARKER).map(|(_, rest)| (i + 1, rest)))
        .map(|(lineno, rest)| {
            // Read only as far as the first `-->` AFTER the marker. This used
            // to be `.trim_end_matches("-->")`, which strips the arrow from the
            // END of the line only — so a marker sitting mid-line swallowed
            // every following character into the "claimed version" (Day 180,
            // #845: CLAUDE.md's bullets are single ~10k-char lines, and the
            // gate was green only because the marker happened to sit last).
            //
            // The `None` fallback is a near-miss guard, not a convenience:
            // `src/gasp.rs` writes its markers in Rust `//` comments with no
            // `-->` at all, so an extractor that REQUIRED the arrow would read
            // zero markers repo-wide and fail the anti-vacuous invariant — the
            // one failure this gate must never have.
            let claimed = match rest.split_once("-->") {
                Some((before_arrow, _)) => before_arrow,
                None => rest,
            };
            let version = claimed
                .trim()
                .trim_end_matches("-->")
                .trim()
                .trim_matches('"')
                .to_string();
            (lineno, version)
        })
        .collect()
}

/// Every `*.rs` under `src/`, recursively, plus `CLAUDE.md` — the two places
/// dependency claims are written. Paths are returned relative to the repo
/// root so failure messages are copy-pasteable.
fn scanned_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    collect_rs(&root.join("src"), root, &mut out);
    out.sort();
    if root.join("CLAUDE.md").is_file() {
        out.push("CLAUDE.md".to_string());
    }
    out
}

fn collect_rs(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, root, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

#[test]
fn every_dependency_version_claim_names_the_pinned_yoagent_version() {
    let root = repo_root();
    let lock = read(&root.join("Cargo.lock"));
    let locked = locked_yoagent_version(&lock).expect(
        "no `yoagent` package version found in Cargo.lock — has the lockfile format changed?",
    );

    let files = scanned_files(&root);
    let mut found = 0usize;
    let mut stale: Vec<String> = Vec::new();

    for rel in &files {
        let text = read(&root.join(rel));
        for (lineno, claimed) in markers_in(&text) {
            found += 1;
            if claimed != locked {
                stale.push(format!(
                    "  {rel}:{lineno}\n    claimed:  yoagent {claimed}\n    \
                     Cargo.lock resolves: yoagent {locked}\n    \
                     re-read the paragraph above this marker — it may have gone stale.\n    \
                     Then either re-verify the claim against yoagent {locked} and update the \
                     marker to {locked}, or correct the prose."
                ));
            }
        }
    }

    // A scanner that finds nothing and passes is the vacuous-green shape. If
    // the convention is deleted, this test must say so rather than go quiet.
    assert!(
        found > 0,
        "no `{MARKER}` marker found in any of the {} scanned files (src/**/*.rs + CLAUDE.md).\n\
         This gate exists because a stale dependency claim in src/gasp.rs cost eight sessions.\n\
         If the convention was deliberately retired, delete this test in the same commit; \
         a scanner with nothing to scan passes for the wrong reason.",
        files.len()
    );

    assert!(
        stale.is_empty(),
        "\n{} stale dependency-version claim(s) — the prose was checked against an older \
         yoagent than the one Cargo.lock now pins:\n\n{}\n",
        stale.len(),
        stale.join("\n\n")
    );
}

#[test]
fn locked_version_parser_ignores_prefix_sharing_packages() {
    // `yoagent-state` shares a prefix with `yoagent`; an exact-name match is
    // what keeps the guard reading the right package.
    let lock = r#"
[[package]]
name = "yoagent-state"
version = "0.4.1"

[[package]]
name = "yoagent"
version = "0.16.5"
"#;
    assert_eq!(locked_yoagent_version(lock).as_deref(), Some("0.16.5"));
    assert_eq!(
        locked_yoagent_version("name = \"other\"\nversion = \"1.0\""),
        None
    );
}

#[test]
fn marker_extraction_reads_version_and_line_and_tolerates_comment_syntax() {
    assert_eq!(
        markers_in("//! <!-- yoagent-version-claim: 0.16.5 -->"),
        vec![(1, "0.16.5".to_string())]
    );
    // Line numbers are 1-based and several markers in one file are all seen —
    // the whole point of the widening is that a file may carry more than one.
    assert_eq!(
        markers_in("nope\n// yoagent-version-claim: 1.0\nnope\n/// yoagent-version-claim: 2.0"),
        vec![(2, "1.0".to_string()), (4, "2.0".to_string())]
    );
    assert!(markers_in("//! nothing to see here").is_empty());
    // Naming the convention without the colon must NOT register as a marker,
    // or CLAUDE.md's description of the rule would become an instance of it.
    assert!(markers_in("the `yoagent-version-claim` convention").is_empty());
}

#[test]
fn marker_extraction_stops_at_the_closing_arrow_not_the_end_of_line() {
    // Day 180 (#845): the extractor used `.trim_end_matches("-->")`, which only
    // strips the arrow from the END of a line — so a marker sitting in the
    // MIDDLE of one swallowed everything after it as part of the version. That
    // is not a hypothetical shape: CLAUDE.md's bullets are single lines of ~10k
    // characters, and the gate was green only by luck of the marker happening
    // to sit last. It cost a whole finished, green task to a revert.
    //
    // Row 1 — the defect. Prose after the closing arrow must not be read as
    // part of the claimed version.
    assert_eq!(
        markers_in(
            "- `x.rs` — a long bullet <!-- yoagent-version-claim: 0.16.6 --> and then \
             more prose about the decorator, with `code` and — dashes — after it."
        ),
        vec![(1, "0.16.6".to_string())]
    );

    // Row 2 — near-miss guard: NO `-->` anywhere. `src/gasp.rs` carries its two
    // live markers in Rust `//` comments with no terminator, so an extractor
    // that *required* the arrow would silently read zero markers repo-wide —
    // strictly worse than the bug, since the anti-vacuous invariant is the one
    // thing this gate must never lose.
    assert_eq!(
        markers_in("// yoagent-version-claim: 0.16.6"),
        vec![(1, "0.16.6".to_string())]
    );

    // Row 3 — near-miss guard: the end-of-line shape every existing marker uses
    // is byte-identical to before. That is the whole regression surface.
    assert_eq!(
        markers_in("//! <!-- yoagent-version-claim: 0.16.6 -->"),
        vec![(1, "0.16.6".to_string())]
    );

    // Row 4 — line numbers stay 1-based and correct when a mid-line marker is
    // mixed in with the other shapes.
    assert_eq!(
        markers_in(
            "nope\n\
             prose <!-- yoagent-version-claim: 1.0 --> trailing words\n\
             nope\n\
             // yoagent-version-claim: 2.0\n\
             //! <!-- yoagent-version-claim: 3.0 -->"
        ),
        vec![
            (2, "1.0".to_string()),
            (4, "2.0".to_string()),
            (5, "3.0".to_string()),
        ]
    );
}

#[test]
fn the_scan_covers_nested_src_modules_and_claude_md() {
    // Guard against a walker that only reads the top level: `src/format/` is a
    // real subdirectory, and a claim written there must not be invisible.
    let files = scanned_files(&repo_root());
    assert!(files.iter().any(|f| f == "src/gasp.rs"), "{files:?}");
    assert!(files.iter().any(|f| f == "src/format/mod.rs"), "{files:?}");
    assert!(files.iter().any(|f| f == "CLAUDE.md"), "{files:?}");
}
