//! Exit-status tests for `yoyo ast` (Day 244): a missing ast-grep and a failed
//! search used to exit 0, the same as a search that ran. These drive the pure
//! classifier and status map; the real-binary rows, with `sg` controlled on
//! PATH, are in tests/integration.rs (`ast_exit_status_*`).

use super::*;
use crate::commands_search::{GREP_ERROR_EXIT, GREP_NO_MATCH_EXIT};

#[test]
fn ast_outcome_status_copies_greps_convention() {
    assert_eq!(ast_outcome_status(&AstOutcome::Missing), GREP_ERROR_EXIT);
    assert_eq!(
        ast_outcome_status(&AstOutcome::Error("ast-grep error: boom".into())),
        GREP_ERROR_EXIT
    );
    assert_eq!(
        ast_outcome_status(&AstOutcome::NoMatches),
        GREP_NO_MATCH_EXIT
    );
    assert_eq!(
        ast_outcome_status(&AstOutcome::Matches("a.rs:1:x".into())),
        0
    );
    // The three nonzero answers a script must tell apart are distinct from 0,
    // and "could not search" is not "searched, nothing found".
    assert_ne!(GREP_ERROR_EXIT, GREP_NO_MATCH_EXIT);
    assert_ne!(GREP_NO_MATCH_EXIT, 0);
}

#[test]
fn classify_sg_output_keeps_the_old_rendering_rules() {
    // Success with output: matches.
    assert_eq!(
        classify_sg_output(true, "a.rs:1:x\n", ""),
        AstOutcome::Matches("a.rs:1:x\n".into())
    );
    // Success, nothing printed: no matches.
    assert_eq!(classify_sg_output(true, "", ""), AstOutcome::NoMatches);
    assert_eq!(classify_sg_output(true, "  \n", ""), AstOutcome::NoMatches);
    // Failure with empty stderr and stdout: no matches (ast-grep may exit 1 on
    // no match), not an error.
    assert_eq!(classify_sg_output(false, "", ""), AstOutcome::NoMatches);
    // Failure with output and no stderr: the output is still matches.
    assert_eq!(
        classify_sg_output(false, "a.rs:1:x\n", ""),
        AstOutcome::Matches("a.rs:1:x\n".into())
    );
    // Failure with stderr: an error, even if stdout has text.
    assert_eq!(
        classify_sg_output(false, "", "bad pattern\n"),
        AstOutcome::Error("ast-grep error: bad pattern".into())
    );
    assert_eq!(
        classify_sg_output(false, "partial\n", "bad pattern"),
        AstOutcome::Error("ast-grep error: bad pattern".into())
    );
}

#[test]
fn usage_errors_exit_two_before_sg_is_looked_for() {
    for input in ["/ast", "/ast --lang", "/ast --in"] {
        assert_eq!(handle_ast_grep_status(input), 2, "{input:?}");
    }
}

#[test]
fn version_output_accepts_ast_grep_and_rejects_util_linux_sg() {
    // Captured verbatim on the Day-248 runner: `/usr/bin/sg --version`.
    assert!(!is_ast_grep_version_output("sg from util-linux 2.41.3\n"));
    // Near-miss that must pass: ast-grep's own line, under either name.
    assert!(is_ast_grep_version_output("ast-grep 0.39.5\n"));
    assert!(is_ast_grep_version_output("  ast-grep 0.39.5"));
    assert!(is_ast_grep_version_output("ast-grep"));
    for reject in ["", "\n", "garbage", "ast-grepper 1.0", "not ast-grep 0.1"] {
        assert!(!is_ast_grep_version_output(reject), "{reject:?}");
    }
}

#[test]
fn resolver_prefers_ast_grep_and_accepts_sg_only_when_it_is_ast_grep() {
    let util_linux = || Some((true, "sg from util-linux 2.41.3\n".to_string()));
    let real = || Some((true, "ast-grep 0.39.5\n".to_string()));
    // The #1131 host: no ast-grep, util-linux sg exits 0 -> not installed.
    let got = resolve_ast_grep_binary(AST_GREP_CANDIDATES, |n| {
        if n == "sg" {
            util_linux()
        } else {
            None
        }
    });
    assert_eq!(got, None);
    // Real ast-grep installed only as `sg` (older packages) still resolves.
    let got = resolve_ast_grep_binary(
        AST_GREP_CANDIDATES,
        |n| {
            if n == "sg" {
                real()
            } else {
                None
            }
        },
    );
    assert_eq!(got, Some("sg"));
    // Both present: `ast-grep` wins, and `sg` is never probed.
    let mut probed = Vec::new();
    let got = resolve_ast_grep_binary(AST_GREP_CANDIDATES, |n| {
        probed.push(n.to_string());
        real()
    });
    assert_eq!(got, Some("ast-grep"));
    assert_eq!(probed, ["ast-grep"]);
    // ast-grep-looking stdout with a failed exit does not count.
    let got = resolve_ast_grep_binary(AST_GREP_CANDIDATES, |_| {
        Some((false, "ast-grep 0.39.5".to_string()))
    });
    assert_eq!(got, None);
}
