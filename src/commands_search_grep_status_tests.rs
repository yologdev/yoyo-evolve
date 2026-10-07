//! #982 tests for `/grep` / `yoyo grep` error reporting: a grep that could not
//! search (missing path, unreadable dir) must not render "No matches found."
//! Lives in its own file because `commands_search.rs` sits at the module-size
//! gate (the `commands_config_get_tests.rs` pattern).

use crate::commands_search::{
    format_grep_count_results, format_grep_results, format_grep_results_with_context,
    run_grep_count_in, run_grep_in, run_grep_with_context_in, GrepArgs, GrepMatch,
};
use crate::format::{DIM, RESET};
use crate::grep_status::{
    grep_failure_message, GrepRun, GREP_ERROR_LINE_MAX_BYTES, GREP_ERROR_MAX_LINES,
};
use tempfile::TempDir;

fn match_args(pattern: &str) -> GrepArgs {
    GrepArgs {
        pattern: pattern.to_string(),
        path: ".".to_string(),
        case_sensitive: true,
        context_lines: None,
        include: None,
        exclude: None,
        count_only: false,
    }
}

fn count_args(pattern: &str) -> GrepArgs {
    GrepArgs {
        count_only: true,
        ..match_args(pattern)
    }
}

fn context_args(pattern: &str) -> GrepArgs {
    GrepArgs {
        context_lines: Some((1, 1)),
        ..match_args(pattern)
    }
}

#[test]
fn grep_failure_message_table() {
    // Left: (exit code, stderr, path). Right: exactly what a caller gets.
    let cases: &[(Option<i32>, &str, &str, Option<&str>)] = &[
        // 0 = matches, 1 = genuine no-match: never an error (near-miss).
        (Some(0), "", "src", None),
        (Some(1), "", "src", None),
        // grep's own stderr on 1 is ignored too — status is the authority.
        (Some(1), "grep: noise\n", "src", None),
        (
            Some(2),
            "grep: /nope: No such file or directory\n",
            "/nope",
            Some("grep could not search `/nope` (exit status 2)\n    grep: /nope: No such file or directory"),
        ),
        (
            Some(128),
            "fatal: bad\n\nUse '--'\n",
            "p",
            Some("grep could not search `p` (exit status 128)\n    fatal: bad\n    Use '--'"),
        ),
        (None, "", "p", Some("grep could not search `p` (killed by a signal)")),
        (Some(2), "", ".", Some("grep could not search `.` (exit status 2)")),
    ];
    for (code, stderr, path, want) in cases {
        assert_eq!(
            grep_failure_message(*code, stderr, path).as_deref(),
            *want,
            "code={code:?} stderr={stderr:?}"
        );
    }
}

#[test]
fn grep_failure_message_sanitizes_path_and_stderr() {
    let path = "evil\u{1b}[2Jdir";
    let stderr = "grep: evil\u{1b}[2Jdir: Permission denied\n";
    // Anti-vacuous: the fixtures really carry the hostile byte.
    assert!(path.as_bytes().contains(&0x1b) && stderr.as_bytes().contains(&0x1b));
    let out = grep_failure_message(Some(2), stderr, path).unwrap();
    assert!(!out.as_bytes().contains(&0x1b), "{out:?}");
    assert!(out.contains("evil\\x1b[2Jdir"), "{out:?}");
}

#[test]
fn grep_failure_message_caps_lines_on_a_char_boundary_and_counts_the_rest() {
    // 🐙 is 4 bytes; 199 + 4 straddles the 200-byte cap, so a raw byte
    // index would panic (#250).
    let long = format!("{}🐙🐙", "a".repeat(GREP_ERROR_LINE_MAX_BYTES - 1));
    let out = grep_failure_message(Some(2), &long, "p").unwrap();
    let shown = out.lines().nth(1).unwrap().trim_start();
    assert_eq!(
        shown,
        format!("{}…", "a".repeat(GREP_ERROR_LINE_MAX_BYTES - 1))
    );

    let many: String = (1..=7).map(|i| format!("grep: f{i}: denied\n")).collect();
    let out = grep_failure_message(Some(2), &many, "p").unwrap();
    assert_eq!(out.lines().count(), 1 + GREP_ERROR_MAX_LINES + 1, "{out}");
    assert!(out.ends_with("\n    (… 2 more)"), "{out}");
    assert!(out.contains("f5: denied") && !out.contains("f6: denied"));
}

fn grep_scratch() -> TempDir {
    let tmp = TempDir::new().unwrap();
    // Not a git repo, so the plain-grep arm runs (as for a non-git user).
    assert!(!crate::git::run_git_output(&[
        "-C",
        &tmp.path().to_string_lossy(),
        "rev-parse",
        "--is-inside-work-tree"
    ])
    .map(|o| o.status.success())
    .unwrap_or(false));
    std::fs::write(tmp.path().join("a.txt"), "needle_marker here\n").unwrap();
    tmp
}

#[test]
fn grep_missing_path_is_an_error_in_every_mode_not_no_matches() {
    let tmp = grep_scratch();
    let path = "does_not_exist_zz9".to_string();
    let m = run_grep_in(
        tmp.path(),
        &GrepArgs {
            path: path.clone(),
            ..match_args("x")
        },
    )
    .unwrap();
    let c = run_grep_count_in(
        tmp.path(),
        &GrepArgs {
            path: path.clone(),
            ..count_args("x")
        },
    )
    .unwrap();
    let x = run_grep_with_context_in(
        tmp.path(),
        &GrepArgs {
            path: path.clone(),
            ..context_args("x")
        },
    )
    .unwrap();
    for err in [&m.error, &c.error, &x.error] {
        let err = err.as_deref().expect("missing path must be an error");
        assert!(
            err.starts_with("grep could not search `does_not_exist_zz9` (exit status 2)"),
            "{err}"
        );
        assert!(err.contains("No such file or directory"), "{err}");
    }
    assert!(m.found.is_empty() && c.found.is_empty() && x.found.is_empty());
}

#[test]
fn grep_genuine_no_match_keeps_the_old_line_and_no_error() {
    // Near-miss: an existing readable dir with zero matches is NOT an error,
    // and renders byte-identically to before.
    let tmp = grep_scratch();
    let m = run_grep_in(tmp.path(), &match_args("definitely_absent_zz9")).unwrap();
    assert_eq!(
        m,
        GrepRun {
            found: vec![],
            error: None
        }
    );
    let c = run_grep_count_in(tmp.path(), &count_args("definitely_absent_zz9")).unwrap();
    assert_eq!(c.error, None);
    let x = run_grep_with_context_in(tmp.path(), &context_args("definitely_absent_zz9")).unwrap();
    assert_eq!(x.error, None);
    let old = format!("{DIM}  No matches found.{RESET}\n");
    assert_eq!(
        format_grep_results(&m.found, "definitely_absent_zz9", true),
        old
    );
    assert_eq!(format_grep_count_results(&c.found), old);
    assert_eq!(
        format_grep_results_with_context(&x.found, "definitely_absent_zz9", true),
        old
    );
}

#[test]
fn grep_match_is_unchanged_and_carries_no_error() {
    let tmp = grep_scratch();
    let m = run_grep_in(tmp.path(), &match_args("needle_marker")).unwrap();
    assert_eq!(
        m,
        GrepRun {
            found: vec![GrepMatch {
                file: "./a.txt".to_string(),
                line_num: 1,
                text: "needle_marker here".to_string(),
            }],
            error: None,
        }
    );
}

#[cfg(unix)]
#[test]
fn grep_partial_unreadable_dir_keeps_matches_and_the_error() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = grep_scratch();
    let locked = tmp.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::write(locked.join("f.txt"), "needle_marker hidden\n").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = std::fs::read_dir(&locked).is_ok(); // root ignores modes
    let m = run_grep_in(tmp.path(), &match_args("needle_marker"));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    if readable_anyway {
        return;
    }
    let m = m.unwrap();
    assert_eq!(m.found.len(), 1, "{m:?}");
    assert_eq!(m.found[0].file, "./a.txt");
    let err = m.error.expect("unreadable dir must surface");
    assert!(err.contains("Permission denied"), "{err}");
}
