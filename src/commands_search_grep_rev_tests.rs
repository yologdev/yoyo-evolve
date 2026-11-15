//! #1201: `/grep` reads every word after the pattern as a path, never as a
//! git revision.
//!
//! The git arm used to build `git grep -n -- <pattern> <paths...>`. That `--`
//! only ends options: git still parses the words after the pattern as
//! revisions, so a path word spelled like a branch or tag (`main`, `v1`,
//! `HEAD`) was searched as a commit or refused as "ambiguous argument". The
//! fixed shape is `git grep -n -e <pattern> -- <paths...>`, which is what
//! yoagent's own search tool sends (`tools/search.rs`).
//!
//! Every row goes through all three git-arm builders (plain, count, context),
//! in a scratch `git init` tempdir built through `run_git_in_dir` (the
//! src/git.rs chokepoint) — never the real repo.

use super::*;
use std::path::Path;

fn git(root: &Path, argv: &[&str]) {
    // Through the src/git.rs chokepoint, aimed at the tempdir (never the repo).
    let mut full = vec![
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "tag.gpgsign=false",
    ];
    full.extend_from_slice(argv);
    crate::git::run_git_in_dir(root, &full)
        .unwrap_or_else(|e| panic!("git {argv:?} failed in the fixture: {e}"));
}

/// A committed repo on branch `main`, with tag `v1`, and directories named
/// `main/` and `v1/` that share those ref names.
fn scratch() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    for (path, body) in [
        ("src/main.rs", "fn main() {}\n"),
        ("main/lib.rs", "fn in_main_dir() {}\n"),
        ("v1/api.rs", "fn in_v1_dir() {}\n"),
        ("dash.txt", "-foo is a dash-led word\n"),
    ] {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    git(root, &["init", "-q"]);
    git(root, &["checkout", "-q", "-b", "main"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "fixture"]);
    git(root, &["tag", "v1"]);
    git(root, &["branch", "dev"]); // a branch with NO same-named path
    tmp
}

fn args(pattern: &str, paths: &[&str]) -> GrepArgs {
    GrepArgs {
        pattern: pattern.to_string(),
        paths: if paths.is_empty() {
            vec![".".to_string()]
        } else {
            paths.iter().map(|s| s.to_string()).collect()
        },
        case_sensitive: true,
        context_lines: None,
        include: None,
        exclude: None,
        count_only: false,
    }
}

/// Run one input through all three builders and return, per builder, the
/// sorted files it reported plus its error (if any).
fn all_three(root: &Path, a: &GrepArgs) -> Vec<(&'static str, Vec<String>, Option<String>)> {
    let plain = run_grep_in(root, a).expect("plain grep ran");
    let mut plain_files: Vec<String> = plain
        .found
        .iter()
        .map(|m| format!("{}:{}", m.file, m.line_num))
        .collect();
    plain_files.sort();

    let mut ca = a.clone();
    ca.count_only = true;
    let count = run_grep_count_in(root, &ca).expect("count grep ran");
    let mut count_files: Vec<String> = count
        .found
        .iter()
        .map(|e| format!("{}:{}", e.file, e.count))
        .collect();
    count_files.sort();

    let mut xa = a.clone();
    xa.context_lines = Some((1, 1));
    let ctx = run_grep_with_context_in(root, &xa).expect("context grep ran");
    let mut ctx_files: Vec<String> = ctx
        .found
        .lines()
        .filter(|l| !l.is_empty() && *l != "--")
        .map(|l| l.to_string())
        .collect();
    ctx_files.sort();

    vec![
        ("plain", plain_files, plain.error),
        ("count", count_files, count.error),
        ("context", ctx_files, ctx.error),
    ]
}

fn assert_rows(
    root: &Path,
    a: &GrepArgs,
    plain: &[&str],
    count: &[&str],
    context: &[&str],
    what: &str,
) {
    assert_rows_missing(root, a, plain, count, context, None, what);
}

/// As `assert_rows`, but `missing` names a path word that is not a path:
/// every builder must report it beside the matches the other words found.
fn assert_rows_missing(
    root: &Path,
    a: &GrepArgs,
    plain: &[&str],
    count: &[&str],
    context: &[&str],
    missing: Option<&str>,
    what: &str,
) {
    let expected = [plain, count, context];
    for ((name, got, err), want) in all_three(root, a).into_iter().zip(expected) {
        match missing {
            None => assert_eq!(err, None, "{what}: {name} builder reported an error"),
            Some(word) => {
                let err = err.unwrap_or_else(|| panic!("{what}: {name} dropped `{word}` silently"));
                assert!(
                    err.contains(&format!("{word}: no such file or directory")),
                    "{what}: {name} builder must name `{word}`: {err}"
                );
                assert!(!err.contains("ambiguous"), "{what}: {name}: {err}");
            }
        }
        assert_eq!(got, want, "{what}: {name} builder");
    }
}

#[test]
fn plain_directory_path_still_matches() {
    let tmp = scratch();
    assert_rows(
        tmp.path(),
        &args("fn", &["src/"]),
        &["src/main.rs:1"],
        &["src/main.rs:1"],
        &["src/main.rs:1:fn main() {}"],
        "plain dir src/",
    );
}

#[test]
fn file_path_is_searched_while_a_branch_named_main_exists() {
    let tmp = scratch();
    // The #1201 repro: `yoyo grep fn main src/main.rs` with branch `main`.
    // `main` is a dir here too, so both words are real paths.
    assert_rows(
        tmp.path(),
        &args("fn", &["main", "src/main.rs"]),
        &["main/lib.rs:1", "src/main.rs:1"],
        &["main/lib.rs:1", "src/main.rs:1"],
        &[
            "main/lib.rs:1:fn in_main_dir() {}",
            "src/main.rs:1:fn main() {}",
        ],
        "path words main + src/main.rs with branch main",
    );
}

#[test]
fn branch_only_word_does_not_turn_the_file_into_a_revision_path() {
    let tmp = scratch();
    // The exact shape this repo hit: branch `dev`, no `dev` path. Before,
    // git read `dev` as a revision and searched `dev:src/main.rs`; the
    // parser dropped those prefixed lines and `/grep` said "No matches".
    // After: src/main.rs is searched, and `dev` is named as no such path
    // rather than vanishing into a revision.
    assert_rows_missing(
        tmp.path(),
        &args("fn", &["dev", "src/main.rs"]),
        &["src/main.rs:1"],
        &["src/main.rs:1"],
        &["src/main.rs:1:fn main() {}"],
        Some("dev"),
        "branch-only word dev + src/main.rs",
    );
}

#[test]
fn dir_sharing_a_tag_name_is_a_path_not_ambiguous() {
    let tmp = scratch();
    assert_rows(
        tmp.path(),
        &args("fn", &["v1"]),
        &["v1/api.rs:1"],
        &["v1/api.rs:1"],
        &["v1/api.rs:1:fn in_v1_dir() {}"],
        "dir v1/ with tag v1",
    );
}

#[test]
fn head_path_word_is_not_a_search_of_the_head_revision() {
    let tmp = scratch();
    // No file called HEAD: the honest answer is "no such path HEAD" and
    // nothing found, not every match in the HEAD commit under a `HEAD:` prefix.
    assert_rows_missing(
        tmp.path(),
        &args("fn", &["HEAD"]),
        &[],
        &[],
        &[],
        Some("HEAD"),
        "path word HEAD",
    );
}

#[test]
fn dash_led_pattern_is_still_a_pattern() {
    let tmp = scratch();
    assert_rows(
        tmp.path(),
        &args("-foo", &[]),
        &["dash.txt:1"],
        &["dash.txt:1"],
        &["dash.txt:1:-foo is a dash-led word"],
        "dash-led pattern -foo",
    );
}

#[test]
fn no_path_words_searches_the_whole_tree() {
    let tmp = scratch();
    assert_rows(
        tmp.path(),
        &args("fn", &[]),
        &["main/lib.rs:1", "src/main.rs:1", "v1/api.rs:1"],
        &["main/lib.rs:1", "src/main.rs:1", "v1/api.rs:1"],
        &[
            "main/lib.rs:1:fn in_main_dir() {}",
            "src/main.rs:1:fn main() {}",
            "v1/api.rs:1:fn in_v1_dir() {}",
        ],
        "no path words",
    );
}
