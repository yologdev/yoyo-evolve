//! #1091: `/grep` with several path operands searches each one.
//!
//! Before, `parse_grep_args` joined every token after the pattern into one
//! path, so `yoyo grep hello a b` asked grep for a file literally named
//! `a b` and failed with "grep could not search `a b`" (exit 2). The
//! convention this follows is the reference tools': `grep -rnH hello a b`
//! and `git grep -n hello -- a b` both search two paths.
//!
//! The joiner merged inputs that are now distinct, so each gets its own row:
//! no path, one path, two paths, ONE path that contains a space (reached as a
//! single quoted token), and a missing second path. Every row runs in both a
//! non-git tempdir (plain-grep arm) and a `git init` tempdir (git-grep arm).

use super::*;

/// Write the fixture files: `a`, `b` and `a b` each hold `hello\nworld\n`.
fn write_files(root: &std::path::Path) {
    for name in ["a", "b", "a b"] {
        std::fs::write(root.join(name), "hello\nworld\n").unwrap();
    }
}

fn in_git(root: &std::path::Path) -> bool {
    crate::git::run_git_output(&[
        "-C",
        &root.to_string_lossy(),
        "rev-parse",
        "--is-inside-work-tree",
    ])
    .map(|o| o.status.success())
    .unwrap_or(false)
}

/// A scratch dir provably outside any git work tree (plain-grep arm).
fn non_git_scratch() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    assert!(!in_git(tmp.path()), "fixture must be outside any git repo");
    write_files(tmp.path());
    tmp
}

/// A scratch `git init` dir with every fixture file added (git-grep arm).
/// Through the src/git.rs chokepoint, aimed at the tempdir (never the repo).
fn git_scratch() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_files(tmp.path());
    for argv in [&["init", "-q"][..], &["add", "-A"][..]] {
        crate::git::run_git_in_dir(tmp.path(), argv)
            .unwrap_or_else(|e| panic!("git {argv:?} failed in the fixture: {e}"));
    }
    assert!(in_git(tmp.path()), "fixture must be a git work tree");
    tmp
}

fn args(input: &str) -> GrepArgs {
    parse_grep_args(input).expect("valid /grep input")
}

fn matches(files: &[&str]) -> Vec<GrepMatch> {
    files
        .iter()
        .map(|f| GrepMatch {
            file: f.to_string(),
            line_num: 1,
            text: "hello".to_string(),
        })
        .collect()
}

fn sorted(mut v: Vec<GrepMatch>) -> Vec<GrepMatch> {
    v.sort_by(|x, y| x.file.cmp(&y.file));
    v
}

#[test]
fn parse_keeps_each_path_operand_separate_1091() {
    assert_eq!(args("/grep hello").paths, ["."]);
    assert_eq!(args("/grep hello a").paths, ["a"]);
    assert_eq!(args("/grep hello a b").paths, ["a", "b"]);
    // Near-miss: one quoted operand containing a space stays ONE path.
    assert_eq!(args("/grep hello \"a b\"").paths, ["a b"]);
    assert_eq!(args("/grep -- -x a b").paths, ["a", "b"]);
}

#[test]
fn two_paths_search_both_in_both_arms_1091() {
    for tmp in [non_git_scratch(), git_scratch()] {
        let run = run_grep_in(tmp.path(), &args("/grep hello a b")).unwrap();
        assert_eq!(run.error, None, "git={}", in_git(tmp.path()));
        assert_eq!(sorted(run.found), matches(&["a", "b"]));
    }
}

#[test]
fn one_path_is_unchanged_in_both_arms() {
    for tmp in [non_git_scratch(), git_scratch()] {
        let run = run_grep_in(tmp.path(), &args("/grep hello a")).unwrap();
        assert_eq!(run.error, None);
        assert_eq!(run.found, matches(&["a"]));
    }
}

#[test]
fn quoted_path_with_a_space_is_one_path_in_both_arms() {
    for tmp in [non_git_scratch(), git_scratch()] {
        let run = run_grep_in(tmp.path(), &args("/grep hello \"a b\"")).unwrap();
        assert_eq!(run.error, None, "git={}", in_git(tmp.path()));
        assert_eq!(run.found, matches(&["a b"]));
    }
}

#[test]
fn no_path_searches_everything_in_both_arms() {
    for tmp in [non_git_scratch(), git_scratch()] {
        let run = run_grep_in(tmp.path(), &args("/grep hello")).unwrap();
        assert_eq!(run.error, None);
        let mut files: Vec<String> = run.found.into_iter().map(|m| m.file).collect();
        files.sort();
        // Plain grep prints `./a` for the `.` operand; git grep prints `a`.
        let want: Vec<String> = if in_git(tmp.path()) {
            vec!["a".into(), "a b".into(), "b".into()]
        } else {
            vec!["./a".into(), "./a b".into(), "./b".into()]
        };
        assert_eq!(files, want);
    }
}

#[test]
fn count_and_context_modes_search_each_path() {
    let tmp = non_git_scratch();
    let run = run_grep_count_in(tmp.path(), &args("/grep -c hello a b")).unwrap();
    assert_eq!(run.error, None);
    let mut counts: Vec<(String, u32)> = run.found.into_iter().map(|e| (e.file, e.count)).collect();
    counts.sort();
    assert_eq!(counts, vec![("a".into(), 1), ("b".into(), 1)]);

    let run = run_grep_with_context_in(tmp.path(), &args("/grep -A 1 hello a b")).unwrap();
    assert_eq!(run.error, None);
    assert!(
        run.found.contains("a:1:hello\na-2-world\n")
            && run.found.contains("b:1:hello\nb-2-world\n"),
        "both files' context: {:?}",
        run.found
    );
}

#[test]
fn missing_second_path_keeps_the_first_paths_matches_and_reports_the_error() {
    // Plain grep: matches from `a`, an error naming `nope`, grep's exit 2,
    // which `handle_grep_status` turns into GREP_ERROR_EXIT (prints both).
    let tmp = non_git_scratch();
    let run = run_grep_in(tmp.path(), &args("/grep hello a nope")).unwrap();
    assert_eq!(run.found, matches(&["a"]));
    let err = run
        .error
        .expect("a missing operand is an error, not a no-match");
    assert!(
        err.contains("nope"),
        "error must name the missing path: {err}"
    );
    assert!(
        err.starts_with("grep could not search `a`, `nope`"),
        "operands listed separately, never re-joined into `a nope`: {err}"
    );
}

#[test]
fn missing_path_in_the_git_arm_is_gits_own_fatal() {
    // git grep refuses an unknown pathspec outright (exit 128, nothing printed),
    // so the git arm reports the error with no matches: git's convention, kept.
    let tmp = git_scratch();
    let run = run_grep_in(tmp.path(), &args("/grep hello a nope")).unwrap();
    assert!(run.found.is_empty());
    let err = run.error.expect("unknown pathspec is an error");
    assert!(
        err.contains("exit status 128") && err.contains("nope"),
        "{err}"
    );
}
