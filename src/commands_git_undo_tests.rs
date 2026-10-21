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

// ── #1059: outside a git repository, "git couldn't look" is not "nothing there" ──

/// A temp dir that is NOT a git repo. Anti-vacuous: assert git agrees it is not
/// one, so a tempdir that happens to sit inside a repo cannot make the
/// not-a-repo tests pass by testing the repo branch instead.
fn non_repo_dir() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let out = git_out(tmp.path(), &["rev-parse", "--git-dir"]);
    assert!(
        !out.status.success(),
        "fixture must not be inside a git repository"
    );
    tmp
}

#[test]
fn last_commit_exits_1_outside_a_git_repository() {
    let tmp = non_repo_dir();
    let outcome = handle_undo_last_commit_in(tmp.path());
    assert_eq!(
        undo_status_code(&outcome),
        1,
        "git could not look: not success"
    );
    assert!(outcome.context.is_none());
}

#[test]
fn all_exits_1_outside_a_git_repository_and_touches_nothing() {
    let tmp = non_repo_dir();
    fs::write(tmp.path().join("keep.txt"), "mine").unwrap();
    let mut history = TurnHistory::new();
    let outcome = handle_undo_all_in(tmp.path(), &mut history);
    assert_eq!(
        undo_status_code(&outcome),
        1,
        "git could not look: not success"
    );
    assert!(outcome.context.is_none());
    assert_eq!(
        fs::read_to_string(tmp.path().join("keep.txt")).unwrap(),
        "mine"
    );
}

#[test]
fn not_a_repo_message_names_the_cause_without_a_success_glyph() {
    let msg = undo_not_a_repo_message(
        "fatal: not a git repository (or any of the parent directories): .git",
    );
    assert_eq!(msg, "error: not in a git repository");
    assert!(!msg.contains('✓'));
    // Near-miss: a different failure (git missing) is reported as itself, not
    // dressed as "not a repository".
    let other = undo_not_a_repo_message("git not found: No such file or directory");
    assert_eq!(
        other,
        "error: git could not run: git not found: No such file or directory"
    );
}

#[test]
fn all_on_a_clean_repo_still_exits_0() {
    // Near-miss for #1059: git did look, and the true answer is "nothing to undo".
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    let mut history = TurnHistory::new();
    let outcome = handle_undo_all_in(tmp.path(), &mut history);
    assert_eq!(undo_status_code(&outcome), 0);
    assert!(outcome.context.is_none());
}

#[test]
fn all_reverts_changes_in_the_given_dir() {
    // The dir seam really targets `root`, not the process cwd.
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    fs::write(repo.join("a.txt"), "edited").unwrap();
    fs::write(repo.join("new.txt"), "untracked").unwrap();
    let mut history = TurnHistory::new();
    let outcome = handle_undo_all_in(repo, &mut history);
    assert_eq!(undo_status_code(&outcome), 0);
    assert!(outcome.context.is_some());
    assert_eq!(fs::read_to_string(repo.join("a.txt")).unwrap(), "initial");
    assert!(!repo.join("new.txt").exists());
}
