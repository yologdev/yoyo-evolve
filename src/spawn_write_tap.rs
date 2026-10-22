//! `/spawn` write tap, first cut (#1014): count the paths that changed in the
//! **main checkout**, outside the worker's own worktree, while a spawn worker ran.
//!
//! Why this exists: a spawn worker's file tools are fenced to its worktree
//! (#716), but its `bash` is only *pinned* there (cwd), not fenced — an absolute
//! path or a `cd ..` still reaches the parent's checkout. The most likely
//! outside target is that checkout itself, so that is what this measures:
//! `git status` of the parent repo just before the worker starts, again just
//! after it finishes (before `-o` writes its output file, which the user asked
//! for), and the paths that appear only in the second snapshot.
//!
//! **What it measures, stated plainly, and what it does not:**
//! - It sees only paths **git sees in the parent repo**: not `/tmp`, not
//!   `$HOME`, not another repo, and not `.gitignore`d files.
//! - A path that was **already dirty** before the worker started and is changed
//!   again is **not** counted (the snapshot holds names, not contents). A dirty
//!   path the run *cleaned* (e.g. `git checkout -- f`) is not counted either.
//! - Attribution is **"while it ran"**, not "by the worker": an edit by the user
//!   or by another process during the run is counted too, which is why the
//!   printed line says "while it ran" and never "the worker wrote".
//! - Exclusions, from what a foreground `/spawn` itself writes in the checkout:
//!   only `.yoyo/worktrees/` (the worker's own tree, which `git status` lists as
//!   an untracked nested repo). Its other writes are a git branch for the
//!   handoff and `git worktree` metadata, both under `.git/`, which `git status`
//!   never lists. `.yoyo/spawn_runs/` manifests belong to `--parallel` only.
//!   Nothing else is excluded: a worker that writes `.yoyo/memory.json` in the
//!   main checkout did write outside its worktree, and is counted.
//! - `None` from [`snapshot`] means **could not check** (not a repository, git
//!   missing or failing). It is a distinct state and never reads as zero.
//! - Only the foreground `/spawn` path is wired; `--bg` and `--parallel` are
//!   residue on #1014.

use std::collections::BTreeSet;
use std::path::Path;

/// Prefix of the worker's own tree, relative to the repo root.
const WORKTREES_PREFIX: &str = ".yoyo/worktrees/";

/// How many names the printed line shows before "and N more".
const MAX_NAMES: usize = 10;

/// The set of paths `git status` reports as changed or untracked in the repo
/// containing `repo`, relative to the repo root (porcelain paths always are).
/// `None` = could not check. Read-only: runs no destructive git subcommand.
/// Goes through the `src/git.rs` chokepoint via `run_git_output` (raw output:
/// `run_git_in_dir` trims, which would eat the first record's leading space).
pub fn snapshot(repo: &Path) -> Option<BTreeSet<String>> {
    let dir = repo.to_string_lossy();
    let out = crate::git::run_git_output(&[
        "-C",
        &dir,
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
    ])
    .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(parse_porcelain_z(&out.stdout))
}

/// Parse `git status --porcelain=v1 -z`: records are `XY path\0`, and a rename
/// or copy (`X` = `R`/`C`) is followed by one more `\0`-terminated field, the
/// original path, which is skipped (the new name is the changed path).
fn parse_porcelain_z(raw: &[u8]) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    let mut fields = raw.split(|b| *b == 0).filter(|f| !f.is_empty());
    while let Some(rec) = fields.next() {
        if rec.len() < 4 {
            continue;
        }
        paths.insert(String::from_utf8_lossy(&rec[3..]).into_owned());
        if rec[0] == b'R' || rec[0] == b'C' {
            fields.next();
        }
    }
    paths
}

/// Paths in `after` that were not in `before`, minus the worker's own tree.
pub fn outside_writes(before: &BTreeSet<String>, after: &BTreeSet<String>) -> Vec<String> {
    after
        .difference(before)
        .filter(|p| !p.starts_with(WORKTREES_PREFIX))
        .cloned()
        .collect()
}

/// The one line the tap prints. Glyph-free when `plain`; every path is passed
/// through `sanitize_for_display`, because path names come from the repo.
pub fn tap_line(result: Option<&[String]>, plain: bool) -> String {
    let mark = if plain { "warning:" } else { "⚠" };
    let Some(paths) = result else {
        return format!(
            "  {mark} write tap could not check the main checkout \
             (not a git repository, or git failed)"
        );
    };
    let n = paths.len();
    let what = format!(
        "{n} {} in the main checkout outside the worker's worktree while it ran",
        if n == 1 {
            "path changed"
        } else {
            "paths changed"
        }
    );
    if n == 0 {
        return format!("  write tap: {what} (only paths git sees are counted)");
    }
    let mut names: Vec<String> = paths
        .iter()
        .take(MAX_NAMES)
        .map(|p| crate::cli::sanitize_for_display(p))
        .collect();
    if n > MAX_NAMES {
        names.push(format!("and {} more", n - MAX_NAMES));
    }
    format!("  {mark} write tap: {what}: {}", names.join(", "))
}

/// Take the second snapshot and print the tap line on stderr.
pub fn report(before: Option<&BTreeSet<String>>, repo: &Path) {
    let result = before
        .zip(snapshot(repo))
        .map(|(b, a)| outside_writes(b, &a));
    let plain = crate::format::is_plain_output();
    let line = tap_line(result.as_deref(), plain);
    let colour = if result.as_ref().is_some_and(|r| r.is_empty()) {
        &crate::format::DIM
    } else {
        &crate::format::YELLOW
    };
    eprintln!("{colour}{line}{}", crate::format::RESET);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn outside_writes_table() {
        let rows: &[(&[&str], &[&str], &[&str])] = &[
            // new file outside the worktree -> counted
            (&[], &["src/new.rs"], &["src/new.rs"]),
            // worker's own tree -> excluded
            (&[], &[".yoyo/worktrees/spawn-1-2/"], &[]),
            (&[], &[".yoyo/worktrees/spawn-1-2/a.rs"], &[]),
            // pre-existing dirty path, unchanged in the set -> not counted
            (&["README.md"], &["README.md"], &[]),
            // pre-existing dirty path plus a new one -> only the new one
            (&["README.md"], &["README.md", "b.txt"], &["b.txt"]),
            // other .yoyo bookkeeping is NOT excluded
            (&[], &[".yoyo/memory.json"], &[".yoyo/memory.json"]),
            // a dirty path that became clean is not counted
            (&["gone.rs"], &[], &[]),
            // empty before/after -> empty
            (&[], &[], &[]),
        ];
        for (before, after, want) in rows {
            let got = outside_writes(&set(before), &set(after));
            let want: Vec<String> = want.iter().map(|s| s.to_string()).collect();
            assert_eq!(got, want, "before={before:?} after={after:?}");
        }
    }

    #[test]
    fn porcelain_z_parse_takes_new_name_of_rename() {
        let raw = b" M src/a.rs\0?? new file.txt\0R  to.rs\0from.rs\0A  c.rs\0";
        assert_eq!(
            parse_porcelain_z(raw),
            set(&["src/a.rs", "new file.txt", "to.rs", "c.rs"])
        );
    }

    #[test]
    fn tap_line_zero() {
        let none: Vec<String> = vec![];
        let want = "  write tap: 0 paths changed in the main checkout outside the worker's \
                    worktree while it ran (only paths git sees are counted)";
        assert_eq!(tap_line(Some(&none), false), want);
        assert_eq!(tap_line(Some(&none), true), want);
    }

    #[test]
    fn tap_line_one_plain_and_not() {
        let one = vec!["src/x.rs".to_string()];
        assert_eq!(
            tap_line(Some(&one), false),
            "  ⚠ write tap: 1 path changed in the main checkout outside the worker's \
             worktree while it ran: src/x.rs"
        );
        assert_eq!(
            tap_line(Some(&one), true),
            "  warning: write tap: 1 path changed in the main checkout outside the worker's \
             worktree while it ran: src/x.rs"
        );
    }

    #[test]
    fn tap_line_many_is_capped() {
        let many: Vec<String> = (0..12).map(|i| format!("f{i}")).collect();
        assert_eq!(
            tap_line(Some(&many), true),
            "  warning: write tap: 12 paths changed in the main checkout outside the worker's \
             worktree while it ran: f0, f1, f2, f3, f4, f5, f6, f7, f8, f9, and 2 more"
        );
        let three: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        assert_eq!(
            tap_line(Some(&three), false),
            "  ⚠ write tap: 3 paths changed in the main checkout outside the worker's \
             worktree while it ran: a, b, c"
        );
    }

    #[test]
    fn tap_line_could_not_check_is_not_zero() {
        assert_eq!(
            tap_line(None, false),
            "  ⚠ write tap could not check the main checkout (not a git repository, or git failed)"
        );
        let plain = tap_line(None, true);
        assert_eq!(
            plain,
            "  warning: write tap could not check the main checkout (not a git repository, or git failed)"
        );
        assert!(!plain.contains(" 0 "));
    }

    #[test]
    fn tap_line_plain_is_glyph_free() {
        let one = vec!["a".to_string()];
        for line in [tap_line(Some(&one), true), tap_line(None, true)] {
            assert!(line.is_ascii(), "plain line has a non-ASCII glyph: {line}");
        }
    }

    #[test]
    fn tap_line_escapes_control_bytes_in_paths() {
        let hostile = vec!["evil\x1b[31mred".to_string()];
        assert!(
            hostile[0].as_bytes().contains(&0x1b),
            "fixture lost its ESC"
        );
        for plain in [true, false] {
            let out = tap_line(Some(&hostile), plain);
            assert!(
                !out.as_bytes().contains(&0x1b),
                "raw ESC reached output: {out:?}"
            );
            assert!(out.contains("evil"), "path vanished: {out:?}");
        }
    }

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("spawn git");
        assert!(out.status.success(), "git {args:?} failed: {out:?}");
    }

    /// A scratch repo with one commit, in a tempdir — never the project root.
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
    fn positive_control_real_repo_counts_a_new_outside_file() {
        let tmp = scratch_repo();
        let p = tmp.path();
        let before = snapshot(p).expect("a real repo must be checkable");
        std::fs::write(p.join("outside.txt"), "x").unwrap();
        let after = snapshot(p).expect("still checkable");
        assert_eq!(
            outside_writes(&before, &after),
            vec!["outside.txt".to_string()]
        );
    }

    #[test]
    fn near_miss_real_repo_worktree_write_is_not_counted() {
        let tmp = scratch_repo();
        let p = tmp.path();
        let before = snapshot(p).expect("checkable");
        let wt = p.join(".yoyo").join("worktrees").join("spawn-x");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join("inside.txt"), "x").unwrap();
        let after = snapshot(p).expect("checkable");
        assert!(
            after.iter().any(|a| a.starts_with(WORKTREES_PREFIX)),
            "fixture not seen by git: {after:?}"
        );
        assert_eq!(outside_writes(&before, &after), Vec::<String>::new());
    }

    #[test]
    fn snapshot_of_a_non_repo_is_could_not_check() {
        // A `.git` file pointing nowhere makes git fail right here instead of
        // walking up into whatever repo the tempdir happens to sit in.
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join(".git"), "gitdir: /nonexistent/yoyo-tap\n").unwrap();
        assert_eq!(snapshot(tmp.path()), None);
        // A directory that does not exist cannot even spawn git there.
        assert_eq!(snapshot(&tmp.path().join("missing")), None);
    }
}
