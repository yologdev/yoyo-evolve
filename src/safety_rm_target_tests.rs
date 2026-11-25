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
    // Not a child: the folder itself or its parent. `$PWD/..`, `$PWD/x/..`,
    // `$PWD/.` and `$PWD/..&&ls` still ask, now with the parent/working-folder
    // reason first (#1232, PARENT_ROWS below).
    ("rm -rf $PWD/*/", Some("$PWD")),
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

const PARENT: &str = "Destructive command: force-recursive delete targeting the parent folder";
const ABOVE: &str =
    "Destructive command: force-recursive delete targeting a folder above the parent";
const ALL_IN_PARENT: &str =
    "Destructive command: force-recursive delete targeting everything in the parent folder";
const ABOVE_ALL: &str =
    "Destructive command: force-recursive delete targeting everything in a folder above the parent";

/// #1232: a recursive delete that resolves, lexically, to the parent folder
/// or above asks however it is spelled, not only as the exact token `..`.
/// GNU rm 9.7 and BusyBox 1.37 themselves refuse every operand ending in
/// `..` (measured, posted on #1232); the one spelling they run is a glob
/// into the parent (`../*`), which deletes the working folder with its
/// siblings. Asking on the refused rows still flags what the model meant.
const PARENT_ROWS: &[(&str, Option<(&str, &str)>)] = &[
    ("rm -rf ..", Some((PARENT, ".."))),
    ("rm -rf ../", Some((PARENT, "../"))),
    ("rm -rf ./..", Some((PARENT, "./.."))),
    ("rm -rf ../..", Some((ABOVE, "../.."))),
    ("rm -rf build/../..", Some((PARENT, "build/../.."))), // build/.. cancels
    ("rm -rf ${PWD:?}/..", Some((PARENT, "${PWD:?}/.."))),
    ("rm -rf \"$(pwd)/..\"", Some((PARENT, "\"$(pwd)/..\""))),
    // Unguarded $PWD asked before via the empty-variable rule (#1230); the
    // parent/working-folder reason now comes first.
    ("rm -rf \"$PWD/..\"", Some((PARENT, "\"$PWD/..\""))),
    ("rm -rf $PWD/..&&ls", Some((PARENT, "$PWD/..&&ls"))),
    ("rm -rf $PWD/x/..", Some((CWD, "$PWD/x/.."))),
    ("rm -rf \"$PWD/.\"", Some((CWD, "\"$PWD/.\""))),
    // The spelling rm does run: everything in the parent, cwd included.
    ("rm -rf ../*", Some((ALL_IN_PARENT, "../*"))),
    ("rm -rf ../../*", Some((ABOVE_ALL, "../../*"))),
    ("rm -rf build/..", Some((CWD, "build/.."))), // the working folder
    ("rm -rf ${PWD:?}", Some((CWD, "${PWD:?}"))),
    ("(rm -rf ../)", Some((PARENT, "../)"))),
    ("cd sub && rm -rf ../..", Some((ABOVE, "../.."))),
    ("rm -rf ..;", Some((PARENT, "..;"))),
    (
        "rm -r ../",
        Some((
            "Destructive command: recursive delete targeting the parent folder",
            "../",
        )),
    ),
    // A redirect is skipped, not a stop (the exact-`..` check it replaces
    // read past it).
    ("rm -rf 2>/dev/null ..", Some((PARENT, ".."))),
    ("rm -rf > /tmp/log ../", Some((PARENT, "../"))),
    ("rm -rf 2>/dev/null /home", Some((TOP, "/home"))),
    ("rm -rf build > ../out.log", None), // the redirect's target, not rm's
    ("rm -rf build 2> ..", None),
    // Near-misses: a real name is left after the `..`s.
    ("rm -rf ../sibling", None),
    ("rm -rf ./build", None),
    ("rm -rf build/../dist", None),
    ("rm -rf ../*.log", None), // only some files in the parent
    // `${X:?}` aborts if X is empty, and X/.. is the parent of an unknown
    // folder, not of the working folder.
    ("rm -rf ${X:?}/..", None),
    // A `..` after a separator is not an rm operand.
    ("rm -rf build && cd ..", None),
    ("rm -r build; cd ../..", None),
];

#[test]
fn a_recursive_delete_of_the_parent_asks_however_it_is_spelled() {
    let mut wrong = Vec::new();
    for (cmd, want) in PARENT_ROWS {
        let want = want.map(|(head, target)| format!("{head} '{target}'"));
        let got = analyze_bash_command(cmd);
        println!("PARENT | {cmd} | {got:?}");
        if got != want {
            wrong.push(format!("{cmd}: got {got:?}, want {want:?}"));
        }
    }
    assert!(wrong.is_empty(), "rows off:\n{}", wrong.join("\n"));
}

/// #1236: an rm operand with `..` judged on the path the kernel deletes.
/// Pure rows: the home value and the prefix resolver are passed in, so nothing
/// here reads `HOME`, touches the filesystem (except the one tempdir row) or
/// runs rm.
mod resolved_dotdot {
    use super::super::safety_rm_parent::{
        resolved_anchored, resolved_anchored_with, rm_anchored_target_with,
    };
    use std::path::{Path, PathBuf};

    const ROOT_GLOB: &str = "everything in the filesystem root";

    fn link_to(from: &'static str, to: &'static str) -> impl Fn(&Path) -> Option<PathBuf> {
        move |p| (p == Path::new(from)).then(|| PathBuf::from(to))
    }

    fn verdict(
        t: &str,
        home: Option<&str>,
        resolve: impl Fn(&Path) -> Option<PathBuf>,
    ) -> (Option<String>, Option<&'static str>) {
        let r = resolved_anchored_with(t, home, resolve);
        (r.clone(), rm_anchored_target_with(t, r))
    }

    #[test]
    fn a_dotdot_after_a_symlink_to_root_is_judged_as_root() {
        let (path, label) = verdict("/tmp/link/../*", None, link_to("/tmp/link", "/"));
        assert_eq!(path.as_deref(), Some("/*"));
        assert_eq!(label, Some(ROOT_GLOB));
        let (path, _) = verdict("/tmp/link/..", None, link_to("/tmp/link", "/"));
        assert_eq!(path.as_deref(), Some("/"));
    }

    #[test]
    fn an_unresolvable_prefix_keeps_the_lexical_verdict() {
        let (path, label) = verdict("/tmp/link/../*", None, |_| None);
        assert_eq!(path.as_deref(), Some("/tmp/*"));
        assert_eq!(
            label,
            Some("top-level folder"),
            "today's verdict, unchanged"
        );
    }

    #[test]
    fn roots_home_dotdot_is_the_root() {
        for t in ["~/../*", "$HOME/../*", "${HOME}/../*"] {
            let (path, label) = verdict(t, Some("/root"), |_| None);
            assert_eq!(path.as_deref(), Some("/*"), "{t}");
            assert_eq!(label, Some(ROOT_GLOB), "{t}");
        }
    }

    #[test]
    fn an_ordinary_homes_dotdot_still_asks_as_every_home_folder() {
        let (path, label) = verdict("~/../*", Some("/home/me"), |_| None);
        assert_eq!(path.as_deref(), Some("/home/*"));
        assert_eq!(label, Some("every home folder"));
    }

    #[test]
    fn no_home_keeps_todays_reading() {
        for home in [None, Some(""), Some("relative")] {
            let (path, label) = verdict("~/../*", home, |_| None);
            assert_eq!(path, None, "{home:?}");
            assert_eq!(label, Some("every home folder"), "{home:?}");
        }
    }

    #[test]
    fn a_resolved_path_below_a_top_level_folder_still_passes() {
        let (path, label) = verdict("/tmp/a/../b", None, link_to("/tmp/a", "/tmp/x"));
        assert_eq!(path.as_deref(), Some("/tmp/b"));
        assert_eq!(label, None);
    }

    #[test]
    fn an_operand_without_dotdot_never_calls_the_resolver() {
        let panics = |p: &Path| -> Option<PathBuf> { panic!("resolved {p:?}") };
        for t in ["/tmp/build", "/tmp/a/./b", "~/proj", "/"] {
            let (path, label) = verdict(t, Some("/root"), panics);
            assert_eq!((path, label), (None, None), "{t}");
        }
    }

    /// The production resolver on a real symlink to `/`: only the checkers
    /// are called, nothing is deleted.
    #[cfg(unix)]
    #[test]
    fn the_production_resolver_follows_a_real_symlink_to_root() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink("/", &link).unwrap();
        let op = format!("{}/../*", link.display());
        assert_eq!(resolved_anchored(&op).as_deref(), Some("/*"), "{op}");
        let cmd = format!("rm -rf {op}");
        assert_eq!(
            crate::hard_deny::hard_deny_match(&cmd).map(|h| h.label),
            Some("rm -rf /*"),
            "{cmd}"
        );
        assert!(super::analyze_bash_command(&cmd)
            .is_some_and(|w| w.contains(&format!("{ROOT_GLOB} '{op}'"))));
    }
}
