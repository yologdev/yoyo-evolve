//! The one reader of "which branch am I on?" (#1072).
//!
//! `crate::git::git_branch()` (cwd) and `context::git_branch_in(dir)` (the
//! system prompt's `Branch:` line) both delegate here, so there is one
//! definition rather than copies that agree today.
//!
//! Why `git symbolic-ref --short -q HEAD` and not `git rev-parse --abbrev-ref
//! HEAD` (which both readers used before Day 237):
//!
//! | state                         | `rev-parse --abbrev-ref`    | `symbolic-ref --short -q` |
//! |-------------------------------|-----------------------------|---------------------------|
//! | on a branch with commits      | `main`                      | `main`                    |
//! | unborn branch (fresh `init`)  | exit 128, "ambiguous HEAD"  | `main`, exit 0            |
//! | detached HEAD                 | the literal `HEAD`, exit 0  | no output, exit 1         |
//! | not a repo                    | exit 128                    | exit 128                  |
//!
//! So the old reader returned `None` for the first state of every new project
//! (which `/doctor` rendered as "detached") and `Some("HEAD")` for a real
//! detached HEAD. This one names the unborn branch and returns `None` for a
//! detached HEAD, which is what "no current branch" actually means.

use std::path::Path;

/// The current branch of the repo containing `dir`, by its short name.
///
/// `Some(name)` on a branch, including an unborn one with no commits yet.
/// `None` for a detached HEAD, outside a git repo, or when git is missing.
pub fn current_branch_in(dir: &Path) -> Option<String> {
    crate::git::run_git_in_dir(dir, &["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run git in a temp repo, hermetic against the runner's config and any
    /// inherited `GIT_*` redirection (same shape as context.rs's fixture).
    fn fixture_git(dir: &Path, args: &[&str]) {
        let mut cmd = std::process::Command::new("git");
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
        cmd.env("GIT_CONFIG_SYSTEM", "/dev/null");
        for (key, _) in std::env::vars() {
            if key.starts_with("GIT_") && key != "GIT_CONFIG_GLOBAL" && key != "GIT_CONFIG_SYSTEM" {
                cmd.env_remove(key);
            }
        }
        cmd.args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "-c",
            "init.defaultBranch=trunk",
            "-c",
            "commit.gpgsign=false",
        ]);
        let out = cmd
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git should be runnable in tests");
        assert!(
            out.status.success(),
            "git {args:?} failed in {dir:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn unborn_repo() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        fixture_git(dir.path(), &["init", "-q"]);
        dir
    }

    fn repo_with_commit() -> tempfile::TempDir {
        let dir = unborn_repo();
        std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        fixture_git(dir.path(), &["add", "a.txt"]);
        fixture_git(dir.path(), &["commit", "-q", "-m", "first"]);
        dir
    }

    /// The bug row: a fresh `git init` has a branch, it just has no commits.
    #[test]
    fn unborn_branch_is_named() {
        let dir = unborn_repo();
        assert_eq!(current_branch_in(dir.path()), Some("trunk".to_string()));
    }

    /// Near-miss: the ordinary case must be unchanged.
    #[test]
    fn branch_with_commits_is_named() {
        let dir = repo_with_commit();
        assert_eq!(current_branch_in(dir.path()), Some("trunk".to_string()));
    }

    /// A detached HEAD has no current branch: `None`, never `Some("HEAD")`.
    #[test]
    fn detached_head_is_none_not_literal_head() {
        let dir = repo_with_commit();
        fixture_git(dir.path(), &["checkout", "-q", "--detach"]);
        assert_eq!(current_branch_in(dir.path()), None);
    }

    #[test]
    fn not_a_repo_is_none() {
        let dir = tempfile::TempDir::new().unwrap();
        assert_eq!(current_branch_in(dir.path()), None);
    }

    /// Emission point: the system prompt's `## Git Status` block. Before #1072
    /// an unborn repo got no block at all and a detached HEAD got `Branch: HEAD`.
    /// A detached HEAD must keep the block (it is still a repo), not lose it to
    /// the new `None`; a plain dir still gets no block.
    #[test]
    fn git_status_context_renders_each_state() {
        use crate::context::get_git_status_context_from as status;
        let unborn = unborn_repo();
        let s = status(unborn.path()).expect("an unborn repo is a repo");
        assert!(s.contains("Branch: trunk\n"), "got: {s}");

        let detached = repo_with_commit();
        fixture_git(detached.path(), &["checkout", "-q", "--detach"]);
        let s = status(detached.path()).expect("a detached HEAD is still a repo");
        assert!(s.contains("Branch: (detached HEAD)\n"), "got: {s}");
        assert!(!s.contains("Branch: HEAD"), "got: {s}");

        let plain = tempfile::TempDir::new().unwrap();
        assert_eq!(status(plain.path()), None);
    }
}
