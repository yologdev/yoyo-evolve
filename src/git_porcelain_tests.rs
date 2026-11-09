//! #1169: git's porcelain output starts with a **significant** space when the
//! first changed file has no staged change (` M Cargo.toml`). The stdout
//! decoder used to `.trim()` both ends, which ate that space, so line 1 read as
//! `M Cargo.toml`: the banner counted it as staged (`1 staged, 1 modified`
//! instead of `2 modified`), `/git stage` offered `[M] argo.toml` (a path that
//! does not exist), and a one-character filename vanished from the list.
//!
//! Every fixture here runs real git in a tempdir through `run_git_in_dir`
//! (never the project root: `run_git` has a destructive-command guard under
//! `cfg(test)`), so the parsers are fed the helper's actual bytes, not a
//! string typed to look like them.

use super::{git_stdout, parse_status_files, run_git_in_dir};
use crate::banner::parse_git_status_counts;
use std::path::Path;

/// Fixture setup also goes through the chokepoint (tests/git_chokepoint.rs);
/// the destructive guard only refuses the project root, and this is a tempdir.
fn git(root: &Path, args: &[&str]) {
    let mut argv = vec!["-c", "user.email=t@example.com", "-c", "user.name=t"];
    argv.extend(["-c", "commit.gpgsign=false"]);
    argv.extend_from_slice(args);
    if let Err(e) = run_git_in_dir(root, &argv) {
        panic!("git {args:?} failed: {e}");
    }
}

/// A repo with `files` committed, then each file in `edit` rewritten (unstaged).
fn repo_with_unstaged_edits(files: &[&str], edit: &[&str]) -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let root = tmp.path();
    git(root, &["init", "--quiet"]);
    for f in files {
        let p = root.join(f);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).expect("mkdir");
        }
        std::fs::write(&p, "one\n").expect("write fixture");
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "--quiet", "-m", "init"]);
    for f in edit {
        std::fs::write(root.join(f), "two\n").expect("edit fixture");
    }
    tmp
}

#[test]
fn git_stdout_keeps_leading_space_and_drops_trailing_newline() {
    assert_eq!(git_stdout(b" M a\n M b\n"), " M a\n M b");
    assert_eq!(git_stdout(b"abc123\n"), "abc123");
    assert_eq!(git_stdout(b"\n"), "");
    assert_eq!(git_stdout(b""), "");
}

#[test]
fn unstaged_first_line_counts_as_modified_not_staged() {
    let tmp =
        repo_with_unstaged_edits(&["Cargo.toml", "src/lib.rs"], &["Cargo.toml", "src/lib.rs"]);
    let out = run_git_in_dir(tmp.path(), &["status", "--porcelain"]).expect("status");
    // Whole-string equality: the leading space on line 1 is the bug.
    assert_eq!(out, " M Cargo.toml\n M src/lib.rs");
    assert_eq!(
        parse_git_status_counts(&out),
        (0, 2, 0),
        "two unstaged edits are 0 staged / 2 modified, not 1 staged / 1 modified"
    );
    assert_eq!(
        parse_status_files(&out),
        vec![
            ("M".to_string(), "Cargo.toml".to_string()),
            ("M".to_string(), "src/lib.rs".to_string()),
        ],
        "/git stage must offer the real path, not `argo.toml`"
    );
}

#[test]
fn one_char_filename_on_first_line_survives() {
    let tmp = repo_with_unstaged_edits(&["a", "z.txt"], &["a", "z.txt"]);
    let out = run_git_in_dir(tmp.path(), &["status", "--porcelain"]).expect("status");
    assert_eq!(out, " M a\n M z.txt");
    assert_eq!(
        parse_status_files(&out),
        vec![
            ("M".to_string(), "a".to_string()),
            ("M".to_string(), "z.txt".to_string()),
        ],
        "a one-character filename on line 1 must not disappear"
    );
}

/// Near-miss: a first line whose index column is set (`M  x`) has no leading
/// space to lose, so it must still count as staged after the fix.
#[test]
fn staged_only_first_line_still_counts_as_staged() {
    let tmp = repo_with_unstaged_edits(&["x"], &["x"]);
    git(tmp.path(), &["add", "x"]);
    let out = run_git_in_dir(tmp.path(), &["status", "--porcelain"]).expect("status");
    assert_eq!(out, "M  x");
    assert_eq!(parse_git_status_counts(&out), (1, 0, 0));
    assert_eq!(
        parse_status_files(&out),
        vec![("M".to_string(), "x".to_string())]
    );
}

/// The same decoder feeds `/git status` (`--short`) and `/diff`'s `--stat`
/// summary, whose first line also starts with a space.
#[test]
fn short_status_and_diff_stat_keep_line_one_aligned() {
    let tmp =
        repo_with_unstaged_edits(&["Cargo.toml", "src/lib.rs"], &["Cargo.toml", "src/lib.rs"]);
    let short = run_git_in_dir(tmp.path(), &["status", "--short"]).expect("status --short");
    assert_eq!(short, " M Cargo.toml\n M src/lib.rs");
    let stat = run_git_in_dir(tmp.path(), &["diff", "--stat"]).expect("diff --stat");
    let first = stat.lines().next().expect("diff --stat has a first line");
    let second = stat.lines().nth(1).expect("diff --stat has a second line");
    assert!(
        first.starts_with(" Cargo.toml") && second.starts_with(" src/lib.rs"),
        "both --stat rows must keep git's leading space, got:\n{stat}"
    );
}
