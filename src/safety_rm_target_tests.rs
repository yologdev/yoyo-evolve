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

const EMPTY: &str = " — an empty expansion would delete from cwd or /";

/// #1230: bash and dash set `PWD` at startup, so `$PWD/<child>` is not an
/// "unresolved variable". (command, the variable the warning must name, or
/// `None` for a row that must pass.) Working-folder rows (`$PWD`, `"$PWD/"`)
/// are in [`ROWS`] above and are re-asserted here by reason.
const PWD_ROWS: &[(&str, Option<&str>)] = &[
    // Must now pass: a child of the working folder.
    ("rm -rf \"$PWD/target\"", None),
    ("rm -rf $PWD/target", None),
    ("rm -rf \"${PWD}/build/out\"", None),
    ("rm -rf \"$PWD\"/target", None),
    ("cd /tmp && rm -rf \"$PWD/target\"", None), // cd moves PWD, never empties it
    // Other variables can be empty.
    ("rm -rf $OLDPWD/x", Some("$OLDPWD")),
    ("rm -rf $PWDX/x", Some("$PWDX")),
    ("rm -rf $PWD_DIR/x", Some("$PWD_DIR")),
    ("rm -rf ${PWD:-}/x", Some("$PWD")),
    // A second variable is still flagged, by its own name.
    ("rm -rf \"$PWD/$X\"", Some("$X")),
    ("rm -rf $PWD/target $X", Some("$X")),
    // Not a child: the folder itself or its parent.
    ("rm -rf \"$PWD/..\"", Some("$PWD")),
    ("rm -rf $PWD/x/..", Some("$PWD")),
    ("rm -rf \"$PWD/.\"", Some("$PWD")),
    ("rm -rf $PWD/*/", Some("$PWD")),
    ("rm -rf $PWD/..&&ls", Some("$PWD")),
    // The command names PWD to change it, so $PWD may be empty.
    ("unset PWD; rm -rf \"$PWD/target\"", Some("$PWD")),
    ("PWD= rm -rf \"$PWD/target\"", Some("$PWD")),
    ("PWD=; rm -rf \"$PWD/target\"", Some("$PWD")),
    ("local PWD; rm -rf \"$PWD/target\"", Some("$PWD")),
];

#[test]
fn a_pwd_child_passes_and_pwd_itself_its_parent_or_a_rewritten_pwd_still_asks() {
    let mut wrong = Vec::new();
    for (cmd, var) in PWD_ROWS {
        let want = var.map(|v| {
            format!("Destructive command: rm -rf target contains unresolved variable {v}{EMPTY}")
        });
        let got = analyze_bash_command(cmd);
        if got != want {
            wrong.push(format!("{cmd}: got {got:?}, want {want:?}"));
        }
    }
    // The working folder itself keeps #1227's reason, not this rule's.
    for cmd in [
        "rm -rf $PWD",
        "rm -rf \"$PWD\"",
        "rm -rf ${PWD}",
        "rm -rf $PWD/",
        "rm -rf \"$PWD/\"",
    ] {
        let got = analyze_bash_command(cmd).unwrap_or_default();
        if !got.starts_with(CWD) {
            wrong.push(format!(
                "{cmd}: got {got:?}, want the working-folder warning"
            ));
        }
    }
    assert!(wrong.is_empty(), "rows off:\n{}", wrong.join("\n"));
}
