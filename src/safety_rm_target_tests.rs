//! #1227: a recursive delete aimed at a top-level folder (`/home`, `/Users`,
//! `/data`: any single-component absolute path) or at the working folder
//! spelt as `$PWD` / `$(pwd)` / `` `pwd` `` / `./` asks like `rm -rf .`.
//! Pure-function rows: nothing here runs a command.

use super::analyze_bash_command;
use crate::hard_deny::hard_deny_match;

const TOP: &str = "Destructive command: force-recursive delete targeting top-level folder";
const CWD: &str = "Destructive command: force-recursive delete targeting the working folder";

/// (command, the start of the exact warning before its quoted target, the
/// quoted target), or `None` for a row that must pass.
const ROWS: &[(&str, Option<(&str, &str)>)] = &[
    // Top-level folders.
    ("rm -rf /home", Some((TOP, "/home"))),
    ("rm -rf /Users", Some((TOP, "/Users"))),
    ("rm -rf /root", Some((TOP, "/root"))),
    ("rm -rf /mnt", Some((TOP, "/mnt"))),
    ("rm -rf /media", Some((TOP, "/media"))),
    ("rm -rf /tmp", Some((TOP, "/tmp"))),
    ("rm -rf /data", Some((TOP, "/data"))), // made up: the rule is the shape, not a list
    (
        "rm -r /home/",
        Some((
            "Destructive command: recursive delete targeting top-level folder",
            "/home/",
        )),
    ),
    ("sudo rm -rf /home", Some((TOP, "/home"))),
    // The working folder.
    ("rm -rf \"$PWD\"", Some((CWD, "\"$PWD\""))),
    ("rm -rf $PWD", Some((CWD, "$PWD"))),
    ("rm -rf $(pwd)", Some((CWD, "$(pwd)"))),
    ("rm -rf `pwd`", Some((CWD, "`pwd`"))),
    ("rm -rf \"$(pwd)\"", Some((CWD, "\"$(pwd)\""))),
    ("rm -rf ./", Some((CWD, "./"))),
    ("rm -rf \"$PWD\"/", Some((CWD, "\"$PWD\"/"))),
    // Beyond the issue's table: no -f, braces, a glob, a glued `;`.
    (
        "rm -r \"$PWD\"",
        Some((
            "Destructive command: recursive delete targeting the working folder",
            "\"$PWD\"",
        )),
    ),
    ("rm -rf ${PWD}/", Some((CWD, "${PWD}/"))),
    ("rm -rf /home/*", Some((TOP, "/home/*"))),
    ("rm -rf /home; ls", Some((TOP, "/home;"))),
    // Near-misses that must still pass.
    ("rm -rf ./target", None),
    ("rm -rf /tmp/x", None),
    ("rm -rf build/", None),
    ("rm -r node_modules", None),
    ("rm -f /home/me/notes.txt", None), // not recursive
    ("rm -rf build && cd /app", None),  // `/app` belongs to `cd`
    ("rm -rf build > /out.log", None),  // a redirect target, not an rm target
    // The cwd spelt as an absolute path needs the cwd, which this check
    // does not take (#1229).
    ("rm -rf /home/me/proj", None),
];

#[test]
fn rm_of_a_top_level_or_working_folder_asks_and_near_misses_pass() {
    let mut wrong = Vec::new();
    for (cmd, want) in ROWS {
        let want = want.map(|(head, target)| format!("{head} '{target}'"));
        let got = analyze_bash_command(cmd);
        // The hard block (no confirmation overrides it) is not widened here.
        let hard = hard_deny_match(cmd).map(|h| h.label);
        println!("ROW | {cmd} | analyze: {got:?} | hard_deny: {hard:?}");
        if got != want || hard.is_some() {
            wrong.push(format!("{cmd}: got {got:?} / hard {hard:?}, want {want:?}"));
        }
    }
    assert!(wrong.is_empty(), "rows off:\n{}", wrong.join("\n"));
}

/// Not #1227's rule: the Day-144 empty-variable rule asks about `$PWD/target`
/// because `$PWD` reads as a variable that might be empty (#1230).
#[test]
fn pwd_child_is_asked_about_by_the_empty_variable_rule_not_the_folder_rule() {
    let got = analyze_bash_command("rm -rf \"$PWD/target\"").unwrap_or_default();
    assert!(
        got.starts_with("Destructive command: rm -rf target contains unresolved variable $PWD"),
        "{got}"
    );
}
