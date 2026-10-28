//! Removing a `/spawn` worker's git worktree — without destroying work it
//! still holds.
//!
//! **The authority.** Every path that removes a spawn worktree goes through
//! [`remove_spawn_worktree_dir`]: the foreground and `--bg` completion
//! cleanups, the two output-file-write-failure cleanups (all four via
//! [`cleanup_spawn_worktree`]) and the stale sweep run on every `/spawn`
//! ([`cleanup_stale_worktrees`]). The guard lives here, inside the one
//! function that issues `git worktree remove --force`, so every caller —
//! including future ones — is fail-closed rather than opted in one by one.
//!
//! **What "holds work" means**, checked in the worktree itself before any
//! removal:
//! - `git status --porcelain` is non-empty (modified or untracked files — the
//!   case a failed handoff commit leaves behind: a rejecting pre-commit hook,
//!   `commit.gpgsign` with no agent, no `user.email`);
//! - `HEAD` carries commits reachable from no branch, tag or remote (the
//!   worker committed on the detached HEAD itself, or the handoff commit
//!   landed but naming it with a branch failed).
//!
//! Either one keeps the worktree and prints a note naming its path, what it
//! holds, and how to inspect or discard it. If the check itself cannot run
//! (status errors, or the directory is not a worktree of its own — `git -C`
//! would otherwise walk up and report the *parent* repo's status), the
//! worktree is kept and the note says it could not be checked: "could not
//! check" never reads as "clean".
//!
//! **Not counted:** ignored files (`.gitignore`d build output etc.). The
//! handoff's `git add -A` does not carry them either, so they were never part
//! of what a worker hands back.
//!
//! **Live workers are never swept (#1057, #1097).** A worker holds an
//! exclusive OS advisory lock on a SIBLING file of its worktree
//! (`.yoyo/worktrees/spawn-3-17.lock`, never inside the tree, where it would
//! show in the worktree's own `git status`) for as long as it runs. The stale
//! sweep adds every candidate whose lock another open handle holds — in this
//! yoyo process or any other — to the same `live` skip list its own tracker
//! feeds. The OS drops the lock when the holder dies, so a crashed session
//! leaves an unheld file and its worktree is swept as before.
//!
//! **Branches:** nothing here deletes a branch (no `git branch -d/-D`); the
//! handoff branch `spawn/<id>-<sha>` survives every removal by design.

use std::path::{Path, PathBuf};

use crate::commands_spawn::WorktreeInfo;
use crate::format::{RESET, YELLOW};

/// What a cleanup call did with the worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeCleanup {
    /// The worktree is gone (or was already gone).
    Removed,
    /// The worktree still holds work (or could not be checked) and was left
    /// in place; a note naming its path was printed to stderr.
    Kept,
}

/// What a worktree holds that a removal would destroy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Holding {
    /// Nothing uncommitted and no commit outside every branch/tag/remote.
    Clean,
    /// Work a removal would destroy.
    Work {
        uncommitted: usize,
        unreachable_commits: usize,
    },
    /// The check could not run; treated as "keep", never as clean.
    CouldNotCheck(String),
}

pub(crate) fn run_git_in(dir: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::run_git_in_dir(dir, args)
}

pub(crate) fn repo_root(start: &Path) -> PathBuf {
    run_git_in(start, &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .unwrap_or_else(|_| start.to_path_buf())
}

/// Inspect a worktree for work a removal would destroy.
///
/// `start` is the commit the worktree was created at (`WorktreeInfo::branch`,
/// a short SHA); commits up to it are never counted as unreachable even when
/// the main checkout itself sits on a detached HEAD. `None` (the stale sweep,
/// which has no record) just checks against branches, tags and remotes.
pub(crate) fn worktree_holding(wt: &Path, start: Option<&str>) -> Holding {
    // `git -C <dir>` walks UP to find a repository. A spawn worktree lives
    // inside the main repo, so a directory that is not a worktree of its own
    // would silently report the parent's status — the wrong repo.
    let top = match run_git_in(wt, &["rev-parse", "--show-toplevel"]) {
        Ok(t) => PathBuf::from(t),
        Err(e) => return Holding::CouldNotCheck(e),
    };
    let same = match (top.canonicalize(), wt.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if !same {
        return Holding::CouldNotCheck("not a git worktree of its own".to_string());
    }

    let uncommitted = match run_git_in(wt, &["status", "--porcelain"]) {
        Ok(s) => s.lines().filter(|l| !l.trim().is_empty()).count(),
        Err(e) => return Holding::CouldNotCheck(e),
    };

    let mut args = vec![
        "rev-list",
        "--count",
        "HEAD",
        "--not",
        "--branches",
        "--tags",
        "--remotes",
    ];
    if let Some(s) = start.filter(|s| !s.trim().is_empty()) {
        args.push(s);
    }
    let unreachable_commits = match run_git_in(wt, &args) {
        Ok(n) => match n.trim().parse::<usize>() {
            Ok(n) => n,
            Err(_) => return Holding::CouldNotCheck(format!("unreadable rev-list count: {n}")),
        },
        Err(e) => return Holding::CouldNotCheck(e),
    };

    if uncommitted == 0 && unreachable_commits == 0 {
        Holding::Clean
    } else {
        Holding::Work {
            uncommitted,
            unreachable_commits,
        }
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The note printed when a worktree is kept. `None` for [`Holding::Clean`].
/// Glyph-free when `plain` (no `⚠`, no em dash), like the sibling notes.
pub(crate) fn kept_worktree_note(path: &Path, holding: &Holding, plain: bool) -> Option<String> {
    let p = crate::cli::sanitize_for_display(&path.display().to_string());
    let (mark, dash) = if plain {
        ("warning:", "-")
    } else {
        ("⚠", "—")
    };
    let what = match holding {
        Holding::Clean => return None,
        Holding::Work {
            uncommitted,
            unreachable_commits,
        } => {
            let mut parts = Vec::new();
            if *uncommitted > 0 {
                parts.push(plural(
                    *uncommitted,
                    "uncommitted change",
                    "uncommitted changes",
                ));
            }
            if *unreachable_commits > 0 {
                parts.push(format!(
                    "{} on no branch (save with: git -C {p} branch <name> HEAD)",
                    plural(*unreachable_commits, "commit", "commits")
                ));
            }
            format!("it holds {}", parts.join(" and "))
        }
        Holding::CouldNotCheck(e) => {
            let e = crate::cli::sanitize_for_display(e);
            format!("could not check it for uncommitted work ({e})")
        }
    };
    Some(format!(
        "  {mark} kept spawn worktree {p} {dash} {what}, so nothing was deleted\n    \
         inspect: git -C {p} status   discard when done: git worktree remove --force {p}"
    ))
}

/// The one place a spawn worktree directory is removed. Checks for held work
/// first; removes exactly as before only when the worktree is clean.
fn remove_spawn_worktree_dir(root: &Path, path: &Path, start: Option<&str>) -> WorktreeCleanup {
    let holding = worktree_holding(path, start);
    if let Some(note) = kept_worktree_note(path, &holding, crate::format::is_plain_output()) {
        // A safety fact: deliberately not quiet-gated.
        eprintln!("{YELLOW}{note}{RESET}");
        return WorktreeCleanup::Kept;
    }

    let wt_str = path.to_string_lossy().to_string();
    // Clean: force-remove first, then plain remove, then manual cleanup.
    if run_git_in(root, &["worktree", "remove", "--force", &wt_str]).is_err()
        && run_git_in(root, &["worktree", "remove", &wt_str]).is_err()
    {
        let _ = std::fs::remove_dir_all(path);
        let _ = run_git_in(root, &["worktree", "prune"]);
    }
    // The lock sibling goes with its directory (#1097); an owner still holding
    // it keeps a handle to the unlinked file until it drops the guard.
    let _ = std::fs::remove_file(worktree_lock_path(path));
    WorktreeCleanup::Removed
}

/// Remove a spawn worktree, cleaning up both the directory and the git
/// metadata — unless it still holds work (see the module docs), in which case
/// it is kept, a note is printed, and `Ok(Kept)` is returned.
///
/// Idempotent — calling it on an already-removed worktree is a no-op.
pub fn cleanup_spawn_worktree(
    repo_dir: &Path,
    info: &WorktreeInfo,
) -> Result<WorktreeCleanup, String> {
    let root = repo_root(repo_dir);

    // If the worktree directory doesn't exist any more, just prune stale metadata.
    if !info.path.exists() {
        let _ = std::fs::remove_file(worktree_lock_path(&info.path));
        let _ = run_git_in(&root, &["worktree", "prune"]);
        return Ok(WorktreeCleanup::Removed);
    }

    match remove_spawn_worktree_dir(&root, &info.path, Some(&info.branch)) {
        WorktreeCleanup::Kept => Ok(WorktreeCleanup::Kept),
        WorktreeCleanup::Removed if info.path.exists() => Err(format!(
            "failed to remove worktree directory: {}",
            info.path.display()
        )),
        WorktreeCleanup::Removed => Ok(WorktreeCleanup::Removed),
    }
}

/// One spelling per directory, so a live path given relative, with a
/// trailing slash or through a symlink (`/tmp` vs `/private/tmp`) still
/// matches the sweep's `read_dir` spelling. A path that cannot be
/// canonicalized (already gone) falls back to its normalized components.
fn path_key(p: &Path) -> PathBuf {
    p.canonicalize()
        .unwrap_or_else(|_| p.components().collect())
}

/// The stale sweep's decision, pure so tests can drive it without the
/// process-global tracker. A candidate is swept only when its age is known,
/// is over `max_age`, and it is NOT in `live`: the worktree of a `--bg`
/// worker this process is still running (#1057). Root-dir mtime is not an
/// activity signal: a running worker that has not written yet, or whose
/// work is all committed, looks exactly as old as an abandoned one, so the
/// tracker's state decides instead. Held work is not decided here:
/// [`remove_spawn_worktree_dir`] still keeps a dirty worktree (Day 234).
pub(crate) fn worktrees_to_sweep(
    candidates: &[(PathBuf, Option<std::time::Duration>)],
    max_age: std::time::Duration,
    live: &[PathBuf],
) -> Vec<PathBuf> {
    let live: std::collections::HashSet<PathBuf> = live.iter().map(|p| path_key(p)).collect();
    candidates
        .iter()
        .filter(|(_, age)| age.is_some_and(|a| a > max_age))
        .filter(|(p, _)| !live.contains(&path_key(p)))
        .map(|(p, _)| p.clone())
        .collect()
}

impl crate::commands_spawn::SpawnTracker {
    /// Record the worktree a spawn runs in, so the stale sweep can tell a
    /// live worker's worktree from an abandoned one (#1057).
    pub(crate) fn set_worktree(&self, id: usize, path: &Path) {
        let mut tasks = crate::sync_util::lock_or_recover(&self.inner);
        if let Some(t) = tasks.iter_mut().find(|t| t.id == id) {
            t.worktree = Some(path.to_path_buf());
        }
    }

    /// Worktrees of spawns whose status is still `Running`. Keyed on state,
    /// not on how a worker ends: a worker that completes, fails or panics
    /// mid-run drops out only when its status leaves `Running` (a panicked
    /// worker stays `Running`, so its worktree is kept: the safe direction).
    pub(crate) fn live_worktrees(&self) -> Vec<PathBuf> {
        self.snapshot()
            .into_iter()
            .filter(|t| t.status == crate::commands_spawn::SpawnStatus::Running)
            .filter_map(|t| t.worktree)
            .collect()
    }
}

/// The lock file of a spawn worktree: a SIBLING of the directory
/// (`spawn-3-17` -> `spawn-3-17.lock`), not inside it. A file inside would
/// show in the worktree's `git status`, turning every live worktree "dirty"
/// for the held-work guard and the /spawn write tap.
pub(crate) fn worktree_lock_path(worktree: &Path) -> PathBuf {
    let mut name = worktree
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".lock");
    worktree.with_file_name(name)
}

/// A held worktree lock (#1097). Dropping it — including the OS closing the
/// handle when the process dies, however it dies — releases the lock.
#[derive(Debug)]
pub(crate) struct WorktreeLock {
    _file: std::fs::File,
}

/// Take the exclusive advisory lock for a worker's worktree, creating the lock
/// file if needed. Uses std's `File::try_lock` (flock on Unix, LockFileEx on
/// Windows), never blocking. `None` if the file cannot be opened or locked —
/// including on a platform where std's file locking is unsupported — and the
/// worker then runs unlocked, which is the behaviour before #1097: only this
/// process's tracker protects it.
pub(crate) fn hold_worktree_lock(worktree: &Path) -> Option<WorktreeLock> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(worktree_lock_path(worktree))
        .ok()?;
    file.try_lock().ok()?;
    Some(WorktreeLock { _file: file })
}

/// Is the worktree's lock held by an open handle other than the one this call
/// opens — another yoyo process's worker, or a worker of this process (flock
/// and LockFileEx conflict per handle)? `try_lock` refused with WouldBlock:
/// held, so live. Taken: release it at once (drop) and not live. No lock file
/// (a worktree from before #1097, or a crashed create) is not live. Any other
/// error, including "unsupported on this platform", is NOT live: the sweep then
/// behaves exactly as before #1097, and the held-work guard still keeps a dirty
/// worktree.
pub(crate) fn held_by_another(worktree: &Path) -> bool {
    let Ok(file) = std::fs::File::open(worktree_lock_path(worktree)) else {
        return false;
    };
    matches!(file.try_lock(), Err(std::fs::TryLockError::WouldBlock))
}

/// Clean up spawn worktrees older than `max_age`.
///
/// Scans `<repo>/.yoyo/worktrees/spawn-*` and removes any whose root
/// directory modified-time is older than `max_age` ago, except the worktrees
/// in `live` (still-running `--bg` workers of this process, #1057) or whose
/// lock another handle holds (a worker of ANY yoyo process, #1097, via
/// [`held_by_another`], added to the same `live` list) — through
/// the same held-work guard as [`cleanup_spawn_worktree`], so a stale
/// worktree that still holds uncommitted work is kept and named, never
/// silently removed.
pub fn cleanup_stale_worktrees(repo_dir: &Path, max_age: std::time::Duration, live: &[PathBuf]) {
    let root = repo_root(repo_dir);
    let wt_dir = root.join(".yoyo").join("worktrees");
    if !wt_dir.is_dir() {
        return;
    }
    let now = std::time::SystemTime::now();
    if let Ok(entries) = std::fs::read_dir(&wt_dir) {
        let candidates: Vec<(PathBuf, Option<std::time::Duration>)> = entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("spawn-"))
            .filter(|e| e.path().is_dir())
            .map(|e| {
                let age = e
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|mt| now.duration_since(mt).ok());
                (e.path(), age)
            })
            .collect();
        // A worker of any yoyo process holds its worktree's lock (#1097).
        let mut live = live.to_vec();
        live.extend(
            candidates
                .iter()
                .map(|(p, _)| p)
                .filter(|p| held_by_another(p))
                .cloned(),
        );
        for path in worktrees_to_sweep(&candidates, max_age, &live) {
            let _ = remove_spawn_worktree_dir(&root, &path, None);
        }
        // Prune any dangling worktree metadata.
        let _ = run_git_in(&root, &["worktree", "prune"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands_spawn::create_spawn_worktree;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("spawn git");
        assert!(out.status.success(), "git {args:?} failed: {out:?}");
    }

    /// A scratch repo with one commit on a branch, in a tempdir — never the
    /// project root (`run_git_in_dir`'s destructive guard).
    fn scratch_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        let p = tmp.path();
        git(p, &["init", "-q"]);
        git(p, &["config", "user.email", "t@t"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("README.md"), "hello").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-q", "-m", "init"]);
        tmp
    }

    #[test]
    fn dirty_worktree_is_kept_with_its_uncommitted_file() {
        let tmp = scratch_repo();
        let info = create_spawn_worktree(tmp.path(), 1).expect("create");
        let file = info.path.join("worker_output.txt");
        std::fs::write(&file, "work a failed handoff left behind").unwrap();
        // Anti-vacuous: the fixture really is dirty.
        assert_eq!(
            worktree_holding(&info.path, Some(&info.branch)),
            Holding::Work {
                uncommitted: 1,
                unreachable_commits: 0
            }
        );

        let r = cleanup_spawn_worktree(tmp.path(), &info).expect("cleanup");
        assert_eq!(r, WorktreeCleanup::Kept);
        assert!(info.path.is_dir(), "dirty worktree dir was removed");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "work a failed handoff left behind"
        );
    }

    #[test]
    fn clean_worktree_is_removed_exactly_as_before() {
        let tmp = scratch_repo();
        let info = create_spawn_worktree(tmp.path(), 2).expect("create");
        assert_eq!(
            worktree_holding(&info.path, Some(&info.branch)),
            Holding::Clean
        );
        let r = cleanup_spawn_worktree(tmp.path(), &info).expect("cleanup");
        assert_eq!(r, WorktreeCleanup::Removed);
        assert!(!info.path.exists(), "clean worktree should be removed");
    }

    #[test]
    fn commit_on_no_branch_is_kept() {
        let tmp = scratch_repo();
        let info = create_spawn_worktree(tmp.path(), 3).expect("create");
        std::fs::write(info.path.join("a.txt"), "x").unwrap();
        git(&info.path, &["add", "-A"]);
        git(
            &info.path,
            &["commit", "-q", "-m", "worker committed on detached HEAD"],
        );
        assert_eq!(
            worktree_holding(&info.path, Some(&info.branch)),
            Holding::Work {
                uncommitted: 0,
                unreachable_commits: 1
            }
        );
        let r = cleanup_spawn_worktree(tmp.path(), &info).expect("cleanup");
        assert_eq!(r, WorktreeCleanup::Kept);
        assert!(info.path.is_dir());

        // Near-miss: once a branch names the commit (what the handoff does),
        // the same worktree is clean and goes.
        git(&info.path, &["branch", "spawn/3-test", "HEAD"]);
        let r = cleanup_spawn_worktree(tmp.path(), &info).expect("cleanup");
        assert_eq!(r, WorktreeCleanup::Removed);
        assert!(!info.path.exists());
    }

    #[test]
    fn status_failure_keeps_the_worktree() {
        let tmp = scratch_repo();
        let info = create_spawn_worktree(tmp.path(), 4).expect("create");
        // Break the worktree's link to its repository: every git call in it
        // now fails, so the check cannot run.
        std::fs::write(info.path.join(".git"), "gitdir: /nonexistent/yoyo-test\n").unwrap();
        let h = worktree_holding(&info.path, Some(&info.branch));
        assert!(matches!(h, Holding::CouldNotCheck(_)), "got {h:?}");
        let r = cleanup_spawn_worktree(tmp.path(), &info).expect("cleanup");
        assert_eq!(r, WorktreeCleanup::Kept);
        assert!(info.path.join("README.md").exists());
    }

    #[test]
    fn a_plain_directory_is_not_judged_by_the_parent_repo_status() {
        // `git -C` walks up: a non-worktree dir inside the (clean) main repo
        // would otherwise read as "clean" and be removed.
        let tmp = scratch_repo();
        let dir = tmp.path().join(".yoyo").join("worktrees").join("spawn-9-1");
        std::fs::create_dir_all(&dir).unwrap();
        let h = worktree_holding(&dir, None);
        assert!(matches!(h, Holding::CouldNotCheck(_)), "got {h:?}");
    }

    #[test]
    fn stale_sweep_keeps_dirty_and_removes_clean() {
        let tmp = scratch_repo();
        let dirty = create_spawn_worktree(tmp.path(), 5).expect("create");
        let clean = create_spawn_worktree(tmp.path(), 6).expect("create");
        std::fs::write(dirty.path.join("nested.txt"), "crashed worker").unwrap();

        cleanup_stale_worktrees(tmp.path(), std::time::Duration::ZERO, &[]);

        assert!(dirty.path.join("nested.txt").exists(), "dirty swept");
        assert!(!clean.path.exists(), "clean stale worktree should go");
    }

    // Moved from commands_spawn.rs (size cap) with the new `live` argument.
    #[test]
    fn test_cleanup_stale_worktrees() {
        use crate::commands_spawn::list_spawn_worktrees;
        let tmp = scratch_repo();
        let repo = tmp.path();

        let _info = create_spawn_worktree(repo, 10).expect("create worktree");

        // With a very large max_age, nothing should be cleaned.
        cleanup_stale_worktrees(repo, std::time::Duration::from_secs(999_999), &[]);
        assert_eq!(list_spawn_worktrees(repo).len(), 1);

        // With zero max_age, everything should be cleaned.
        cleanup_stale_worktrees(repo, std::time::Duration::ZERO, &[]);
        assert!(
            list_spawn_worktrees(repo).is_empty(),
            "stale worktree should be cleaned"
        );
    }

    const HOUR: std::time::Duration = std::time::Duration::from_secs(3600);

    fn old() -> Option<std::time::Duration> {
        Some(HOUR * 2)
    }

    /// #1057's table at the decision point. Dirty-vs-clean is decided later,
    /// by `remove_spawn_worktree_dir` (pinned by
    /// `stale_sweep_keeps_dirty_and_removes_clean` and the real-repo test below).
    #[test]
    fn sweep_decision_table() {
        let a = PathBuf::from("/nonexistent/yoyo-1057/spawn-1-1");
        let b = PathBuf::from("/nonexistent/yoyo-1057/spawn-2-2");
        let young = Some(std::time::Duration::from_secs(60));
        // (candidates, live, expected swept)
        type Candidates = Vec<(PathBuf, Option<std::time::Duration>)>;
        let rows: Vec<(Candidates, Vec<PathBuf>, Vec<PathBuf>)> = vec![
            // old, not live -> swept (today's behaviour survives)
            (vec![(a.clone(), old())], vec![], vec![a.clone()]),
            // old, live -> kept
            (vec![(a.clone(), old())], vec![a.clone()], vec![]),
            // old, a DIFFERENT spawn is live -> this one is still swept
            (
                vec![(a.clone(), old()), (b.clone(), old())],
                vec![b.clone()],
                vec![a.clone()],
            ),
            // young, not live -> kept
            (vec![(a.clone(), young)], vec![], vec![]),
            // age unreadable -> kept (as before)
            (vec![(a.clone(), None)], vec![], vec![]),
            // live given with a trailing slash -> still kept
            (
                vec![(a.clone(), old())],
                vec![PathBuf::from("/nonexistent/yoyo-1057/spawn-1-1/")],
                vec![],
            ),
            // live given with a `.` component -> still kept
            (
                vec![(a.clone(), old())],
                vec![PathBuf::from("/nonexistent/yoyo-1057/./spawn-1-1")],
                vec![],
            ),
        ];
        for (i, (cands, live, want)) in rows.iter().enumerate() {
            assert_eq!(&worktrees_to_sweep(cands, HOUR, live), want, "row {i}");
        }
    }

    /// A symlinked spelling (macOS `/tmp` -> `/private/tmp` shape) of a live
    /// worktree must not miss.
    #[cfg(unix)]
    #[test]
    fn sweep_matches_a_live_path_spelled_through_a_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        let wt = real.join("spawn-3-3");
        std::fs::create_dir_all(&wt).unwrap();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let live = vec![link.join("spawn-3-3")];
        assert_ne!(live[0], wt, "anti-vacuous: the two spellings differ");
        assert!(worktrees_to_sweep(&[(wt.clone(), old())], HOUR, &live).is_empty());
        // Near-miss: with nothing live the same candidate is swept.
        assert_eq!(
            worktrees_to_sweep(&[(wt.clone(), old())], HOUR, &[]),
            vec![wt]
        );
    }

    /// End to end through the tracker: a clean stale worktree whose `--bg`
    /// worker is still Running survives the sweep; once that worker
    /// completes, the next sweep removes it.
    #[test]
    fn stale_sweep_skips_a_running_workers_worktree_until_it_finishes() {
        use crate::commands_spawn::SpawnTracker;
        let tmp = scratch_repo();
        let live_wt = create_spawn_worktree(tmp.path(), 7).expect("create");
        let tracker = SpawnTracker::new();
        let id = tracker.register_with_bg("long task", None, true);
        tracker.set_worktree(id, &live_wt.path);
        assert_eq!(tracker.live_worktrees(), vec![live_wt.path.clone()]);

        cleanup_stale_worktrees(
            tmp.path(),
            std::time::Duration::ZERO,
            &tracker.live_worktrees(),
        );
        assert!(
            live_wt.path.exists(),
            "a running worker's worktree was swept"
        );

        tracker.complete(id, "done".to_string());
        assert!(tracker.live_worktrees().is_empty(), "finished is not live");
        cleanup_stale_worktrees(
            tmp.path(),
            std::time::Duration::ZERO,
            &tracker.live_worktrees(),
        );
        assert!(
            !live_wt.path.exists(),
            "a finished worker's clean worktree should go"
        );
    }

    #[test]
    fn a_failed_worker_is_not_live() {
        use crate::commands_spawn::SpawnTracker;
        let tracker = SpawnTracker::new();
        let id = tracker.register_with_bg("t", None, true);
        tracker.set_worktree(id, Path::new("/nonexistent/spawn-9-9"));
        tracker.fail(id, "boom".to_string());
        assert!(tracker.live_worktrees().is_empty());
    }

    #[test]
    fn kept_note_names_path_counts_and_recovery() {
        let p = Path::new("/r/.yoyo/worktrees/spawn-1-2");
        let note = kept_worktree_note(
            p,
            &Holding::Work {
                uncommitted: 3,
                unreachable_commits: 0,
            },
            false,
        )
        .expect("note");
        assert_eq!(
            note,
            "  ⚠ kept spawn worktree /r/.yoyo/worktrees/spawn-1-2 — it holds 3 uncommitted \
             changes, so nothing was deleted\n    inspect: git -C /r/.yoyo/worktrees/spawn-1-2 \
             status   discard when done: git worktree remove --force /r/.yoyo/worktrees/spawn-1-2"
        );
        let one = kept_worktree_note(
            p,
            &Holding::Work {
                uncommitted: 1,
                unreachable_commits: 2,
            },
            false,
        )
        .unwrap();
        assert!(
            one.contains("1 uncommitted change and 2 commits on no branch"),
            "{one}"
        );
        let cnc = kept_worktree_note(p, &Holding::CouldNotCheck("boom".into()), false).unwrap();
        assert!(
            cnc.contains("could not check it for uncommitted work (boom)"),
            "{cnc}"
        );
        assert_eq!(kept_worktree_note(p, &Holding::Clean, false), None);
        assert_eq!(kept_worktree_note(p, &Holding::Clean, true), None);
    }

    #[test]
    fn kept_note_is_glyph_free_when_plain() {
        let p = Path::new("/r/w");
        for h in [
            Holding::Work {
                uncommitted: 1,
                unreachable_commits: 1,
            },
            Holding::CouldNotCheck("e".into()),
        ] {
            let n = kept_worktree_note(p, &h, true).unwrap();
            assert!(n.is_ascii(), "plain note carries a glyph: {n}");
            assert!(n.contains("warning: kept spawn worktree /r/w - "), "{n}");
        }
    }

    // -- cross-process live lock (#1097) ----------------------------------

    /// Poll `cond` for up to 3s. A released lock can read as held for a moment:
    /// a sibling test thread forking a child (git, python3) copies this
    /// process's open handles until that child's exec closes them (O_CLOEXEC),
    /// and flock follows the open file, not the process. Measured on Day 242:
    /// a single immediate check failed 3 of 12 runs. Only the "not live (any
    /// more)" side gets this slack; every "is live" assertion stays immediate.
    fn eventually(mut cond: impl FnMut() -> bool) -> bool {
        for _ in 0..300 {
            if cond() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }

    fn swept(repo: &Path, wt: &Path) -> bool {
        eventually(|| {
            cleanup_stale_worktrees(repo, std::time::Duration::ZERO, &[]);
            !wt.exists()
        })
    }

    /// Spawn a child process that takes flock(2) on `lock` and sleeps, and
    /// wait until it says it holds it. python3's fcntl.flock first, the
    /// util-linux `flock` tool second; `None` (test prints SKIPPED) if neither.
    fn child_holding(lock: &Path) -> Option<std::process::Child> {
        use std::io::BufRead;
        let l = lock.to_string_lossy().to_string();
        let py = "import fcntl,sys,time\nf=open(sys.argv[1],'a')\n\
                  fcntl.flock(f,fcntl.LOCK_EX)\nprint('locked',flush=True)\ntime.sleep(30)";
        let tries: [(&str, Vec<&str>); 2] = [
            ("python3", vec!["-c", py, &l]),
            ("flock", vec![&l, "sh", "-c", "echo locked; exec sleep 30"]),
        ];
        for (prog, args) in tries {
            let Ok(mut child) = Command::new(prog)
                .args(&args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
            else {
                continue;
            };
            let mut line = String::new();
            let out = child.stdout.take().expect("piped stdout");
            let _ = std::io::BufReader::new(out).read_line(&mut line);
            if line.trim() == "locked" {
                return Some(child);
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        None
    }

    /// The property option 2 was chosen for: another PROCESS holding the lock
    /// makes the worktree live, and that process dying releases it — no pid,
    /// no stale marker.
    #[test]
    fn a_lock_held_by_another_process_is_live_until_that_process_dies() {
        let tmp = scratch_repo();
        let wt = create_spawn_worktree(tmp.path(), 8).expect("create");
        let lock = worktree_lock_path(&wt.path);
        std::fs::write(&lock, "").unwrap();
        let Some(mut child) = child_holding(&lock) else {
            eprintln!("SKIPPED: neither python3 nor flock available to hold a lock");
            return;
        };
        assert!(held_by_another(&wt.path), "child's lock not seen as live");

        // The sweep keeps it with an EMPTY in-process live list.
        cleanup_stale_worktrees(tmp.path(), std::time::Duration::ZERO, &[]);
        assert!(
            wt.path.exists(),
            "another process's live worktree was swept"
        );

        child.kill().expect("kill");
        child.wait().expect("wait");
        assert!(
            eventually(|| !held_by_another(&wt.path)),
            "a dead process still holds it"
        );
        assert!(
            swept(tmp.path(), &wt.path),
            "dead owner's clean worktree should go"
        );
        assert!(!lock.exists(), "the lock sibling outlived its worktree");
    }

    /// In-process: the guard this module hands workers is what makes them
    /// live, and dropping it ends that.
    #[test]
    fn hold_worktree_lock_makes_live_until_dropped() {
        let tmp = scratch_repo();
        let wt = create_spawn_worktree(tmp.path(), 9).expect("create");
        let guard = hold_worktree_lock(&wt.path).expect("lock");
        let lock = worktree_lock_path(&wt.path);
        assert_eq!(lock.parent(), wt.path.parent(), "lock is a sibling");
        assert!(held_by_another(&wt.path));
        // Not inside the tree, so the held-work guard still reads it clean.
        assert_eq!(worktree_holding(&wt.path, None), Holding::Clean);
        cleanup_stale_worktrees(tmp.path(), std::time::Duration::ZERO, &[]);
        assert!(wt.path.exists(), "a locked worktree was swept");
        drop(guard);
        assert!(
            eventually(|| !held_by_another(&wt.path)),
            "dropped, still held"
        );
        assert!(swept(tmp.path(), &wt.path), "unlocked worktree should go");
        assert!(!lock.exists(), "the lock sibling outlived its worktree");
    }

    /// Near-misses: no lock file, and an unheld file a crashed owner left.
    #[test]
    fn missing_or_unheld_lock_is_not_live_and_is_swept_with_its_sibling() {
        let tmp = scratch_repo();
        let none = create_spawn_worktree(tmp.path(), 11).expect("create");
        let crashed = create_spawn_worktree(tmp.path(), 12).expect("create");
        let crashed_lock = worktree_lock_path(&crashed.path);
        std::fs::write(&crashed_lock, "").unwrap();
        assert!(!worktree_lock_path(&none.path).exists());
        assert!(!held_by_another(&none.path), "no lock file read as live");
        assert!(
            eventually(|| !held_by_another(&crashed.path)),
            "unheld lock read as live"
        );

        assert!(
            swept(tmp.path(), &none.path),
            "lock-less stale worktree should go"
        );
        assert!(
            swept(tmp.path(), &crashed.path),
            "crashed owner's worktree should go"
        );
        assert!(
            !crashed_lock.exists(),
            "crashed owner's lock file left behind"
        );
    }

    #[test]
    fn lock_path_is_the_directory_name_plus_lock() {
        let p = Path::new("/r/.yoyo/worktrees/spawn-3-17");
        assert_eq!(
            worktree_lock_path(p),
            PathBuf::from("/r/.yoyo/worktrees/spawn-3-17.lock")
        );
    }
}
