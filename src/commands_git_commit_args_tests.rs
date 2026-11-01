//! #1111: `yoyo commit` / `/commit` refuse an unknown dash-led flag instead of
//! putting it in the commit message (`--ammend` used to make a NEW commit titled
//! `--ammend`). `-m`/`--message` are accepted the way git users type them, `--`
//! ends flag parsing, and plain words keep their old meaning byte-identically.

use super::*;

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

/// (REPL input after `/commit`, expected message, expected bad flag). Every row
/// is checked on both doors: the REPL's whitespace split and the shell's argv.
const ROWS: &[(&str, &str, Option<&str>)] = &[
    ("--ammend", "", Some("--ammend")),
    ("--bogus", "", Some("--bogus")),
    ("--ammend fix x", "fix x", Some("--ammend")),
    ("-m fix x", "fix x", None),
    ("--message fix x", "fix x", None),
    ("-m", "", Some("-m")),
    ("--message", "", Some("--message")),
    ("-a -m", "", Some("-m")),
    ("-- --weird msg", "--weird msg", None),
    ("fix -- -x", "fix -x", None),
    // Near misses: message words that merely contain or start with a dash.
    ("fix the bug", "fix the bug", None),
    ("rename foo -> bar", "rename foo -> bar", None),
    ("bump -5 percent", "bump -5 percent", None),
    ("fix - typo", "fix - typo", None),
    // Known flags parse as before.
    ("--amend", "", None),
    ("-a --amend --dry-run fix", "fix", None),
    ("--ai", "", None),
];

#[test]
fn every_row_parses_the_same_on_both_doors() {
    for &(input, message, bad) in ROWS {
        let words: Vec<&str> = input.split_whitespace().collect();
        for (door, parsed) in [
            ("repl", parse_commit_args(input)),
            ("shell", commit_args_from_argv(&argv(&words))),
        ] {
            assert_eq!(parsed.message, message, "{door}: {input:?}");
            assert_eq!(parsed.bad_flag.as_deref(), bad, "{door}: {input:?}");
        }
    }
}

#[test]
fn known_flags_still_set_their_fields() {
    let p = parse_commit_args("-a --amend --dry-run fix");
    assert!(p.auto_stage && p.amend && p.dry_run && !p.ai);
    assert!(parse_commit_args("--generate").ai);
    assert!(wants_ai_commit("/commit --ai -m msg"));
}

#[test]
fn a_spaced_shell_element_is_message_text_not_a_flag() {
    // `yoyo commit "-a thing"`: one argv element with a space is a message.
    let p = commit_args_from_argv(&argv(&["-x thing"]));
    assert_eq!((p.message.as_str(), p.bad_flag), ("-x thing", None));
}

#[test]
fn the_refusal_names_the_flag_and_suggests_amend() {
    let msg = commit_bad_flag_message("--ammend", false);
    assert!(
        msg.contains("unknown flag: --ammend (did you mean --amend?)"),
        "{msg}"
    );
    assert!(msg.starts_with("usage: yoyo commit "), "{msg}");
    assert!(msg.contains("nothing was committed"), "{msg}");
    assert!(!msg.contains("--ai"), "shell door has no --ai: {msg}");
    let repl = commit_bad_flag_message("--ammend", true);
    assert!(
        repl.starts_with("usage: /commit ") && repl.contains("[--ai]"),
        "{repl}"
    );
}

#[test]
fn an_unrelated_flag_gets_no_suggestion() {
    let msg = commit_bad_flag_message("--bogus", false);
    assert!(msg.contains("unknown flag: --bogus\n"), "{msg}");
    assert!(!msg.contains("did you mean"), "{msg}");
}

#[test]
fn a_bare_m_says_it_needs_a_message() {
    let msg = commit_bad_flag_message("-m", true);
    assert!(msg.contains("-m needs a message"), "{msg}");
    assert!(!msg.contains("unknown flag"), "{msg}");
}

#[test]
fn both_doors_refuse_with_exit_2_before_any_git_call() {
    // Exit 2 comes before `run_git`, so these run from any cwd without committing.
    assert_eq!(handle_commit_argv_status(&argv(&["--ammend"])), 2);
    assert_eq!(handle_commit_argv_status(&argv(&["-a", "-m"])), 2);
    assert_eq!(handle_commit_status("/commit --ammend"), 2);
}
