//! `/pr create`'s branch check (#1072): a detached HEAD is a repo without a
//! branch, not "not in a git repository". Every fixture is a temp dir, never
//! the project root (the `run_git` destructive guard forbids `checkout` there).

use super::*;
use std::path::Path;

/// Through the `src/git.rs` chokepoint (`run_git_in_dir`), like
/// `commands_git_undo_tests.rs`'s fixture. The `-c` flags pin identity and the
/// initial branch name so the machine's global config cannot decide a test.
fn fixture_git(dir: &Path, args: &[&str]) {
    let mut argv = vec![
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "-c",
        "init.defaultBranch=trunk",
        "-c",
        "commit.gpgsign=false",
    ];
    argv.extend_from_slice(args);
    if let Err(e) = run_git_in_dir(dir, &argv) {
        panic!("fixture git {args:?} failed: {e}");
    }
}

fn repo_with_commit() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    fixture_git(dir.path(), &["init", "-q"]);
    fixture_git(
        dir.path(),
        &["commit", "-q", "--allow-empty", "-m", "first"],
    );
    dir
}

/// (a) Detached HEAD: the detached refusal, so the caller returns before any
/// diff, description prompt or `gh pr create`.
#[test]
fn detached_head_gets_the_detached_refusal() {
    let dir = repo_with_commit();
    fixture_git(dir.path(), &["checkout", "-q", "--detach"]);
    assert_eq!(
        pr_create_branch_in(dir.path()),
        Err("error: HEAD is detached; check out a branch before /pr create (git switch -c <name>)")
    );
}

/// (b) Not a repo: the pre-#1072 message, byte-identical.
#[test]
fn not_a_repo_keeps_the_old_message() {
    let dir = tempfile::TempDir::new().unwrap();
    assert_eq!(
        pr_create_branch_in(dir.path()),
        Err("error: not in a git repository")
    );
}

/// (c) Near-miss: on a real branch the check passes the branch through, so
/// `/pr create` proceeds exactly as before.
#[test]
fn normal_branch_passes_through() {
    let dir = repo_with_commit();
    fixture_git(dir.path(), &["switch", "-q", "-c", "feature"]);
    assert_eq!(pr_create_branch_in(dir.path()), Ok("feature".to_string()));
}

/// Near-miss of (a): an unborn branch (fresh `git init`, no commits) is a
/// branch, not a detached HEAD, and must not be refused as detached.
#[test]
fn unborn_branch_is_not_detached() {
    let dir = tempfile::TempDir::new().unwrap();
    fixture_git(dir.path(), &["init", "-q"]);
    assert_eq!(pr_create_branch_in(dir.path()), Ok("trunk".to_string()));
}

/// The call site prints exactly these consts; the test strings above are
/// typed from the issue's intent, and this pins that the consts agree.
#[test]
fn refusal_consts_match_the_pinned_strings() {
    assert_eq!(PR_CREATE_NOT_A_REPO, "error: not in a git repository");
    assert!(PR_CREATE_DETACHED.contains("detached"));
    assert!(!PR_CREATE_DETACHED.contains("not in a git repository"));
}
