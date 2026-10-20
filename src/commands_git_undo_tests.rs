//! `yoyo undo --last-commit`'s exit status when git refuses (#1058).
//!
//! Lives beside `commands_git.rs` (declared there with `#[path]`) so the
//! module-size gate on that file is not spent on fixtures. Every repo here is
//! a tempdir: `run_git`'s `#[cfg(test)]` guard forbids destructive git in the
//! project root, and `handle_undo_last_commit_in` takes the dir explicitly.

use super::*;
use std::fs;
use std::path::Path;

/// Through the `src/git.rs` chokepoint (`-C <repo>`), like `run_git_undo_in`.
fn git_out(repo: &Path, args: &[&str]) -> std::process::Output {
    let dir = repo.to_string_lossy();
    let mut argv: Vec<&str> = vec!["-C", &dir];
    argv.extend_from_slice(args);
    crate::git::run_git_output(&argv).expect("git runs")
}

fn git(repo: &Path, args: &[&str]) {
    let out = git_out(repo, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_stdout(repo: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&git_out(repo, args).stdout)
        .trim()
        .to_string()
}

/// A temp repo with a local identity, so only the fixture decides whether
/// `git revert` succeeds (not the machine's global config).
fn init_repo(repo: &Path) {
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.email", "test@test.com"]);
    git(repo, &["config", "user.name", "Test"]);
    git(repo, &["config", "commit.gpgsign", "false"]);
    fs::write(repo.join("a.txt"), "initial").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "initial"]);
}

#[test]
fn last_commit_exits_1_when_git_revert_refuses_a_merge_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let main_branch = git_stdout(repo, &["rev-parse", "--abbrev-ref", "HEAD"]);

    git(repo, &["checkout", "-q", "-b", "side"]);
    fs::write(repo.join("b.txt"), "side").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "side work"]);
    git(repo, &["checkout", "-q", &main_branch]);
    git(
        repo,
        &["merge", "-q", "--no-ff", "-m", "merge side", "side"],
    );

    // Anti-vacuous: HEAD really is a merge (two parents), the input git
    // refuses to revert without -m.
    let parents = git_stdout(repo, &["rev-list", "--parents", "-n", "1", "HEAD"]);
    assert_eq!(
        parents.split_whitespace().count(),
        3,
        "fixture HEAD must be a merge commit, got: {parents}"
    );
    let head_before = git_stdout(repo, &["rev-parse", "HEAD"]);

    let outcome = handle_undo_last_commit_in(repo);

    assert_eq!(undo_status_code(&outcome), 1, "a refused revert exits 1");
    assert!(outcome.git_failed);
    assert!(
        outcome.context.is_none(),
        "no context note when nothing was reverted"
    );
    assert_eq!(
        git_stdout(repo, &["rev-parse", "HEAD"]),
        head_before,
        "the refused revert must not have moved HEAD"
    );
}

#[test]
fn last_commit_exits_0_when_a_plain_commit_reverts() {
    // Near-miss: the same door on an ordinary commit still succeeds, exits 0,
    // and the revert really happened.
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    fs::write(repo.join("a.txt"), "changed").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "change a"]);

    let outcome = handle_undo_last_commit_in(repo);

    assert_eq!(undo_status_code(&outcome), 0, "a successful revert exits 0");
    assert!(!outcome.git_failed);
    assert!(
        outcome.context.is_some(),
        "the REPL still gets its context note"
    );
    assert_eq!(fs::read_to_string(repo.join("a.txt")).unwrap(), "initial");
    assert!(
        git_stdout(repo, &["log", "-1", "--format=%s"]).starts_with("Revert"),
        "HEAD should be the revert commit"
    );
}

#[test]
fn last_commit_with_no_commits_still_exits_0() {
    // Unchanged branch, pinned so the #1058 change is visibly scoped: a repo
    // with nothing to undo prints "(no commits to undo)" and is not a refusal.
    let tmp = tempfile::tempdir().unwrap();
    git(tmp.path(), &["init", "-q"]);
    let outcome = handle_undo_last_commit_in(tmp.path());
    assert_eq!(undo_status_code(&outcome), 0);
    assert!(outcome.context.is_none());
}
