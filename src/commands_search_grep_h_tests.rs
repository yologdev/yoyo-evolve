//! #1087: `/grep` on a single file outside a git repo.
//!
//! The non-git arms of `run_grep_in`, `run_grep_count_in` and
//! `run_grep_with_context_in` shell plain `grep -r`. With a single FILE operand
//! grep leaves the filename off every line (`1:hello`, a bare `1` under `-c`),
//! so the `file:line:text` / `file:count` parsers dropped every match and the
//! user was told `No matches found.` about a file that matches. `-H`
//! (`--with-filename`) is GNU/BSD grep's flag for exactly this; `git grep`
//! always prints the filename, which is why the git arm never had the bug.
//!
//! Every fixture lives in a NON-git tempdir, and each test asserts that first:
//! a tempdir that happened to sit inside a work tree would route to the git
//! arm and pass by agreeing with code this file is not about.

use super::*;

/// A scratch dir that is provably not inside a git work tree, holding
/// `a` and `note.txt` (both `hello\nworld\n`), `miss` (no `hello`), and
/// `d/x` (a directory with one matching file).
fn non_git_scratch() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let in_repo = crate::git::run_git_output(&[
        "-C",
        &root.to_string_lossy(),
        "rev-parse",
        "--is-inside-work-tree",
    ])
    .map(|o| o.status.success())
    .unwrap_or(false);
    assert!(
        !in_repo,
        "fixture must be outside any git work tree, or the git arm is what runs"
    );
    std::fs::write(root.join("a"), "hello\nworld\n").unwrap();
    std::fs::write(root.join("note.txt"), "hello\nworld\n").unwrap();
    std::fs::write(root.join("miss"), "nothing here\n").unwrap();
    std::fs::create_dir(root.join("d")).unwrap();
    std::fs::write(root.join("d").join("x"), "hello\nworld\n").unwrap();
    tmp
}

fn args(input: &str) -> GrepArgs {
    parse_grep_args(input).expect("valid /grep input")
}

fn one_match(file: &str) -> Vec<GrepMatch> {
    vec![GrepMatch {
        file: file.to_string(),
        line_num: 1,
        text: "hello".to_string(),
    }]
}

#[test]
fn single_file_operand_reports_its_match_1087() {
    // The #1087 row: `yoyo grep hello a` said "No matches found." and exited 1.
    let tmp = non_git_scratch();
    let run = run_grep_in(tmp.path(), &args("/grep hello a")).unwrap();
    assert_eq!(run.error, None);
    assert_eq!(run.found, one_match("a"));
    // The status `handle_grep_status` derives: non-empty found → exit 0.
    assert!(!run.found.is_empty());
}

#[test]
fn single_file_operand_other_spellings_report_their_match() {
    let tmp = non_git_scratch();
    for (input, file) in [
        ("/grep hello ./a", "./a"),
        ("/grep hello note.txt", "note.txt"),
    ] {
        let run = run_grep_in(tmp.path(), &args(input)).unwrap();
        assert_eq!(run.error, None, "{input}");
        assert_eq!(run.found, one_match(file), "{input}");
    }
}

#[test]
fn single_file_operand_count_mode_counts_one() {
    // `grep -rc hello a` printed a bare `1`, which the `file:count` parser drops.
    let tmp = non_git_scratch();
    let run = run_grep_count_in(tmp.path(), &args("/grep -c hello a")).unwrap();
    assert_eq!(run.error, None);
    assert_eq!(
        run.found,
        vec![GrepCountEntry {
            file: "a".to_string(),
            count: 1
        }]
    );
}

#[test]
fn single_file_operand_context_mode_names_the_file() {
    // Before: `1:hello\n2-world\n`, which the context formatter rendered with a
    // "0 matches" footer because no line looked like `file:line:text`.
    let tmp = non_git_scratch();
    let run = run_grep_with_context_in(tmp.path(), &args("/grep -C 1 hello a")).unwrap();
    assert_eq!(run.error, None);
    assert_eq!(run.found, "a:1:hello\na-2-world\n");
    let visible = strip_ansi_for_test(&format_grep_results_with_context(
        &run.found, "hello", false,
    ));
    assert!(
        visible.contains("1 match") && !visible.contains("0 matches"),
        "context footer must count the match: {visible:?}"
    );
}

#[test]
fn single_file_with_no_match_still_reports_no_matches() {
    // Near-miss: `-H` must not invent a match, and a zero count (`miss:0`)
    // must still be filtered, so the user still sees "No matches found." (exit 1).
    let tmp = non_git_scratch();
    let run = run_grep_in(tmp.path(), &args("/grep hello miss")).unwrap();
    assert_eq!(run.error, None);
    assert!(run.found.is_empty());
    assert_eq!(
        format_grep_results(&run.found, "hello", false),
        format!("{DIM}  No matches found.{RESET}\n")
    );

    let count = run_grep_count_in(tmp.path(), &args("/grep -c hello miss")).unwrap();
    assert_eq!(count.error, None);
    assert!(
        count.found.is_empty(),
        "a zero count is not a match: {count:?}"
    );
    assert_eq!(
        format_grep_count_results(&count.found),
        format!("{DIM}  No matches found.{RESET}\n")
    );
}

#[test]
fn directory_operands_are_byte_identical_to_before() {
    // Near-miss: with a directory operand `grep -r` already printed filenames,
    // so `-H` changes nothing there. These are the pre-#1087 outputs.
    let tmp = non_git_scratch();

    let run = run_grep_in(tmp.path(), &args("/grep hello d")).unwrap();
    assert_eq!(run.found, one_match("d/x"));
    let count = run_grep_count_in(tmp.path(), &args("/grep -c hello d")).unwrap();
    assert_eq!(
        count.found,
        vec![GrepCountEntry {
            file: "d/x".to_string(),
            count: 1
        }]
    );
    let ctx = run_grep_with_context_in(tmp.path(), &args("/grep -C 1 hello d")).unwrap();
    assert_eq!(ctx.found, "d/x:1:hello\nd/x-2-world\n");

    // `.`: directory order is filesystem-dependent, so compare sorted.
    let mut dot = run_grep_in(tmp.path(), &args("/grep hello ."))
        .unwrap()
        .found;
    dot.sort_by(|l, r| l.file.cmp(&r.file));
    let mut expected: Vec<GrepMatch> = ["./a", "./d/x", "./note.txt"]
        .iter()
        .flat_map(|f| one_match(f))
        .collect();
    expected.sort_by(|l, r| l.file.cmp(&r.file));
    assert_eq!(dot, expected);
}

/// Visible text only (what the terminal shows), so the context assertion reads
/// the consumer's medium rather than escape codes.
fn strip_ansi_for_test(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
