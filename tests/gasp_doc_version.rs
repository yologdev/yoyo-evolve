//! Freshness gate for the upstream-blocker analysis recorded in `src/gasp.rs`.
//!
//! Why this test exists, stated plainly: `src/gasp.rs` used to assert that
//! `task-result` (#683 item 5) was *unreachable* because `ProjectRef`,
//! `ArtifactRef` and `PatchStatus` "do not appear anywhere in the published
//! yoagent source". That was true when it was written against yoagent 0.16.3.
//! It went stale when the pin moved to 0.16.5, where all three are re-exported
//! from `yoagent::gasp` — and nothing failed, because prose has no compiler.
//! Five sessions (#765, #782, #785, #787, #789) opened that file, read an
//! authoritative-sounding "impossible", and exited without a diff.
//!
//! So the corrected claim now carries a machine-readable marker naming the
//! yoagent version it was checked against, and this test pins that marker to
//! whatever `Cargo.lock` actually resolves. When the pin moves, this fails and
//! says what to re-check, instead of the sentence quietly becoming false again.
//!
//! This lives in `tests/` rather than inside `src/gasp.rs` on purpose: that
//! module is behind the default-off `gasp` cargo feature, so a `#[cfg(test)]`
//! block inside it would never run in a plain `cargo test` — i.e. exactly the
//! run that CI and every evolve session performs. The file is read as *text*
//! here, so the guard works regardless of the feature flag.
//!
//! Deliberately narrow: only the marker is checked. Prose elsewhere in the file
//! may freely mention other versions as history ("landed in 0.16.4") — that is
//! a statement about the past, not a claim about the current build, and must
//! not fail this test.

use std::path::{Path, PathBuf};

/// The marker carrying the yoagent version the blocker analysis was run against.
const MARKER: &str = "yoagent-version-claim:";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read {} — {e}", path.display()))
}

/// Pull the resolved `yoagent` version out of `Cargo.lock`: the first
/// `version = "..."` line following the `name = "yoagent"` entry.
///
/// Matches the package name exactly, so sibling crates that merely share the
/// prefix (`yoagent-state`) cannot be picked up by accident.
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

/// Every version string carried by a `yoagent-version-claim:` marker, in order.
fn claimed_versions(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| line.split_once(MARKER))
        .map(|(_, rest)| {
            rest.trim()
                .trim_end_matches("-->")
                .trim()
                .trim_matches('"')
                .to_string()
        })
        .collect()
}

#[test]
fn gasp_blocker_claim_names_the_pinned_yoagent_version() {
    let root = repo_root();
    let gasp = read(&root.join("src/gasp.rs"));
    let lock = read(&root.join("Cargo.lock"));

    let locked = locked_yoagent_version(&lock).expect(
        "no `yoagent` package version found in Cargo.lock — has the lockfile format changed?",
    );

    let claimed = claimed_versions(&gasp);

    // A second marker is a second source of truth, and drift between two
    // markers is this same bug one level up.
    assert_eq!(
        claimed.len(),
        1,
        "expected exactly one `{MARKER}` marker in src/gasp.rs, found {}: {claimed:?}. \
         Two markers are two sources of truth and will drift apart.",
        claimed.len()
    );

    assert_eq!(
        claimed[0], locked,
        "\nsrc/gasp.rs claims its yoagent upstream-blocker analysis was performed against \
         yoagent {}, but Cargo.lock now pins yoagent {}.\n\
         Why this matters: that file states which upstream types `task-result` (#683 item 5) \
         needs and whether they exist. A version-stale claim of that kind already cost five \
         sessions (#765, #782, #785, #787, #789), which each read \"unreachable\", believed it, \
         and exited without a diff.\n\
         Re-check the claim against the pinned version, then update the marker:\n  \
         grep -rn 'ProjectRef' ~/.cargo/registry/src/*/yoagent-{}/src/\n",
        claimed[0], locked, locked
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
fn marker_extraction_reads_the_version_and_tolerates_comment_syntax() {
    assert_eq!(
        claimed_versions("//! <!-- yoagent-version-claim: 0.16.5 -->"),
        vec!["0.16.5".to_string()]
    );
    // Two markers must be visible as two, so the count assertion can fire.
    assert_eq!(
        claimed_versions("// yoagent-version-claim: 1.0\n// yoagent-version-claim: 2.0").len(),
        2
    );
    assert!(claimed_versions("//! nothing to see here").is_empty());
}
