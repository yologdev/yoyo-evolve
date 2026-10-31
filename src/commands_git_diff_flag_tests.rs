//! #1107: `yoyo diff` / `/diff` refuse an unknown flag instead of dropping it
//! and showing a different diff (`--cahced` printed unstaged + staged, exit 0).
//!
//! The parser is shared by both doors, so the near-miss rows pin it directly:
//! every accepted spelling parses to exactly the options it parsed to before
//! (`unknown_flag: None`), which is what keeps the diff itself byte-identical.

use super::*;

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

#[test]
fn a_misspelled_flag_is_recorded_on_both_doors() {
    assert_eq!(
        diff_options_from_argv(&argv(&["--cahced"])).unknown_flag,
        Some("--cahced".to_string())
    );
    assert_eq!(
        parse_diff_args("/diff --cahced").unknown_flag,
        Some("--cahced".to_string())
    );
    // A short flag yoyo does not implement is unknown too (git's -p is not
    // something this command honours), and the first unknown one is named.
    assert_eq!(
        parse_diff_args("/diff -p --bogus").unknown_flag,
        Some("-p".to_string())
    );
}

#[test]
fn the_shell_door_refuses_with_exit_2_before_any_git_call() {
    // Exit 2 is returned before `run_git`, so this runs from any cwd.
    assert_eq!(handle_diff_argv_status(&argv(&["--cahced"])), 2);
    assert_eq!(handle_diff_argv_status(&argv(&["--staged", "--bogus"])), 2);
    assert_eq!(handle_diff_status("/diff --cahced"), 2);
}

#[test]
fn refusal_messages_name_the_flag_and_the_doors_own_usage() {
    assert_eq!(
        diff_unknown_flag_message("--cahced", false),
        "usage: yoyo diff [--staged|--cached] [--name-only] [--stat] [--functions] \
         [<file>|<rev>|<rev>..<rev>]\nunknown flag: --cahced"
    );
    assert_eq!(
        diff_unknown_flag_message("--cahced", true),
        "usage: /diff [--staged|--cached] [--name-only] [--stat] [--functions] [--explain] \
         [<file>|<rev>|<rev>..<rev>]\nunknown flag: --cahced"
    );
    // The shell usage does not advertise --explain: that door has no agent.
    assert!(!diff_unknown_flag_message("-x", false).contains("--explain"));
}

/// Near misses: what each accepted input parsed to before #1107, field by field.
#[test]
fn accepted_inputs_parse_exactly_as_before() {
    let base = DiffOptions {
        staged_only: false,
        name_only: false,
        stat_only: false,
        explain: false,
        functions: false,
        file: None,
        ref_range: None,
        unknown_flag: None,
    };
    let rows: Vec<(&[&str], DiffOptions)> = vec![
        (&[], base.clone()),
        (
            &["--staged"],
            DiffOptions {
                staged_only: true,
                ..base.clone()
            },
        ),
        (
            &["--cached"],
            DiffOptions {
                staged_only: true,
                ..base.clone()
            },
        ),
        (
            &["--name-only"],
            DiffOptions {
                name_only: true,
                ..base.clone()
            },
        ),
        (
            &["--stat"],
            DiffOptions {
                stat_only: true,
                ..base.clone()
            },
        ),
        (
            &["--explain"],
            DiffOptions {
                explain: true,
                ..base.clone()
            },
        ),
        (
            &["--functions"],
            DiffOptions {
                functions: true,
                ..base.clone()
            },
        ),
        (
            &["HEAD~1..HEAD"],
            DiffOptions {
                ref_range: Some("HEAD~1..HEAD".to_string()),
                ..base.clone()
            },
        ),
        (
            &["zzz"],
            DiffOptions {
                ref_range: Some("zzz".to_string()),
                ..base.clone()
            },
        ),
        // `--` was dropped as an unknown flag before; it is now accepted as the
        // separator and still changes nothing. Cargo.toml exists at the crate
        // root, where unit tests run, so it classifies as a file as before.
        (
            &["--", "Cargo.toml"],
            DiffOptions {
                file: Some("Cargo.toml".to_string()),
                ..base.clone()
            },
        ),
        (
            &["--staged", "--", "Cargo.toml"],
            DiffOptions {
                staged_only: true,
                file: Some("Cargo.toml".to_string()),
                ..base.clone()
            },
        ),
        // After `--` nothing is a flag, so a dash-led word is not refused
        // (it is still ignored, exactly as before).
        (&["--", "-weird"], base.clone()),
    ];
    for (words, want) in rows {
        assert_eq!(diff_options_from_argv(&argv(words)), want, "argv {words:?}");
        let line = format!("/diff {}", words.join(" "));
        assert_eq!(parse_diff_args(&line), want, "repl {line:?}");
    }
}
