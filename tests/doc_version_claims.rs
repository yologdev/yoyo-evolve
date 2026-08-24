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
            let version = rest
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
fn the_scan_covers_nested_src_modules_and_claude_md() {
    // Guard against a walker that only reads the top level: `src/format/` is a
    // real subdirectory, and a claim written there must not be invisible.
    let files = scanned_files(&repo_root());
    assert!(files.iter().any(|f| f == "src/gasp.rs"), "{files:?}");
    assert!(files.iter().any(|f| f == "src/format/mod.rs"), "{files:?}");
    assert!(files.iter().any(|f| f == "CLAUDE.md"), "{files:?}");
}
