//! #1232: where a recursive rm operand lands relative to the working folder,
//! read lexically, so the parent folder asks however it is spelled.

/// What a recursive rm of `t` (quotes and a trailing `;` already removed)
/// wipes, if it is the working folder, the parent, a folder above it, or
/// everything in one of those; `None` when it names something else or is
/// not this rule's to judge.
///
/// The rule: cut at a glued `&`, `|`, `<` or `>`; a leading `$PWD`,
/// `${PWD}`, `${PWD:?}`, `$(pwd)` or `` `pwd` `` is the working folder
/// itself, then the path continues relative to it. Any other leading `$` or
/// `~`, or an absolute path, is not judged here (the variable, home and
/// top-level rules own those). Trailing `/`, `.` parts and each `name/..`
/// pair drop out. What remains: nothing is the working folder (`build/..`);
/// only `..` parts is the parent or above (`../`, `./..`, `build/../..`);
/// only `..` parts then one bare `*` is everything in that folder (`../*`).
/// A real name left after the `..`s (`../sibling`, `build/../dist`,
/// `../*.log`) passes.
///
/// Lexical, not resolved on disk: a symlinked `build` makes `build/..` a
/// different folder in the kernel's eyes, and a name holding a variable or a
/// glob is taken as one literal part. GNU rm 9.7 and BusyBox 1.37 refuse an
/// operand ending in `..` themselves (measured on #1232); asking anyway
/// flags what the model meant, and `../*` is the spelling they do run.
pub(super) fn rm_parent_target(t: &str) -> Option<&'static str> {
    let t = t.split(['&', '|', '<', '>']).next().unwrap_or("");
    let pwd = ["${PWD:?}", "${PWD}", "$PWD", "$(pwd)", "`pwd`"]
        .iter()
        .find_map(|p| t.strip_prefix(p));
    let rel = match pwd {
        Some(r) if r.is_empty() || r.starts_with('/') => r,
        Some(_) => return None, // `$PWDX`, `$PWD_OLD`: another variable
        None if t.is_empty() || t.starts_with(['$', '~', '/']) => return None,
        None => t,
    };
    let rel = rel.trim_end_matches([')', '`']);
    let mut parts: Vec<&str> = rel.split('/').filter(|p| !matches!(*p, "" | ".")).collect();
    let glob = parts.last() == Some(&"*");
    if glob {
        parts.pop();
    }
    let (mut ups, mut names) = (0usize, 0usize);
    for p in parts {
        match (p, names) {
            ("..", 0) => ups += 1,
            ("..", _) => names -= 1,
            _ => names += 1,
        }
    }
    match (names, ups, glob) {
        (0, 0, false) => Some("the working folder"),
        (0, 1, false) => Some("the parent folder"),
        (0, 1, true) => Some("everything in the parent folder"),
        (0, 2.., false) => Some("a folder above the parent"),
        (0, 2.., true) => Some("everything in a folder above the parent"),
        _ => None,
    }
}

/// #1235: the lexical POSIX normal form of an rm operand anchored at `/`,
/// `~`, `$HOME` or `${HOME}` that has a `..` part (quotes and a trailing `;`
/// already removed), else `None`. `name/..` cancels, `..` at the root stays
/// at the root, `.` and empty parts drop, and a glob tail like `*` is kept:
/// `/tmp/../*` is `/*` and `~/x/..` is `~`, as `os.path.normpath` reads them
/// (except `//`, which is `/` here as on Linux). A `..` above a home anchor
/// is kept (`~/../*`), because how deep the home folder sits is not in the
/// text. Lexical only: a symlink before the `..` is not followed.
pub(crate) fn normalise_anchored(t: &str) -> Option<String> {
    let t = t.split(['&', '|', '<', '>']).next().unwrap_or("");
    let (anchor, rest) = match t.strip_prefix('/') {
        Some(r) => ("/", r),
        None => ["${HOME}", "$HOME", "~"]
            .into_iter()
            .find_map(|a| t.strip_prefix(a).map(|r| (a, r)))
            .filter(|(_, r)| r.is_empty() || r.starts_with('/'))?, // not `$HOMEX`, `~user`
    };
    let rest = rest.trim_end_matches([')', '`']);
    if !rest.split('/').any(|p| p == "..") {
        return None;
    }
    let mut out: Vec<&str> = Vec::new();
    for p in rest.split('/').filter(|p| !matches!(*p, "" | ".")) {
        match (p, out.last()) {
            ("..", Some(&last)) if last != ".." => drop(out.pop()),
            ("..", _) if anchor == "/" => {} // `/..` is `/`
            _ => out.push(p),
        }
    }
    let path = out.join("/");
    Some(match anchor {
        "/" => format!("/{path}"),
        a if path.is_empty() => a.to_string(),
        a => format!("{a}/{path}"),
    })
}

/// #1235: what a recursive rm of `t` wipes when [`normalise_anchored`]'s
/// spelling of it is the root, everything in it, a top-level folder, the
/// home folder, or every home folder (`~/../*`, `$HOME/../*`).
pub(super) fn rm_anchored_target(t: &str) -> Option<&'static str> {
    let n = normalise_anchored(t)?;
    let home = !n.starts_with('/');
    let mut parts: Vec<&str> = n
        .split('/')
        .skip(usize::from(home))
        .filter(|p| !p.is_empty())
        .collect();
    let glob = parts.last() == Some(&"*");
    if glob {
        parts.pop();
    }
    let ups = parts.iter().take_while(|p| **p == "..").count();
    match (home, ups, parts.len() - ups, glob) {
        (false, _, 0, false) => Some("the filesystem root"),
        (false, _, 0, true) => Some("everything in the filesystem root"),
        (false, _, 1, _) => Some("top-level folder"),
        (true, 0, 0, false) => Some("the home folder"),
        (true, 0, 0, true) => Some("everything in the home folder"),
        (true, 1, 0, false) => Some("the folder holding every home folder"),
        (true, 1, 0, true) => Some("every home folder"),
        (true, 2.., 0, _) => Some("a folder above every home folder"),
        (true, 2, 1, _) => Some("a top-level folder, for a home like /home/me"),
        _ => None,
    }
}

#[cfg(test)]
mod anchored_tests {
    use super::super::analyze_bash_command;
    use super::normalise_anchored;
    use crate::hard_deny::hard_deny_match;

    /// Expected values from Python's `posixpath.normpath`, run once in a temp
    /// dir (home taken as `/home/me` for the `~` rows); `//` is `/` here.
    #[test]
    fn anchored_operands_normalise_like_normpath() {
        for (input, want) in [
            ("/tmp/../*", Some("/*")),
            ("/home/me/../../*", Some("/*")),
            ("/tmp/..", Some("/")),
            ("/../..", Some("/")),
            ("/tmp/x/../../home", Some("/home")),
            ("/tmp/x/../y", Some("/tmp/y")),
            ("/tmp/../", Some("/")),
            ("//tmp/../*", Some("/*")), // normpath keeps `//*`; Linux reads `/*`
            ("/tmp/x/..", Some("/tmp")),
            ("/home/me/x/../*", Some("/home/me/*")),
            ("/tmp/../*)", Some("/*")), // `$(rm -rf /tmp/../*)`
            ("~/../*", Some("~/../*")), // normpath: /home/me/../* is /home/*
            ("~/x/..", Some("~")),
            ("$HOME/../*", Some("$HOME/../*")),
            ("${HOME}/a/../b", Some("${HOME}/b")),
            ("~/proj/../proj2/target", Some("~/proj2/target")),
            ("/tmp/a/./b", None), // no `..`: left to the literal rules
            ("$HOMEX/..", None),
            ("~user/..", None),
            ("build/..", None), // relative: rm_parent_target's
        ] {
            assert_eq!(normalise_anchored(input).as_deref(), want, "{input}");
        }
    }

    const ASK: &str = "Destructive command: force-recursive delete targeting";

    /// (command, its normalised literal spelling, the exact warning, the hard
    /// deny label). Each row must ask exactly when its literal does, and be
    /// hard-blocked under the same rule.
    const ROWS: &[(&str, &str, &str, Option<&str>)] = &[
        (
            "rm -rf /tmp/../*",
            "rm -rf /*",
            "everything in the filesystem root '/tmp/../*'",
            Some("rm -rf /*"),
        ),
        (
            "rm -rf /home/me/../../*",
            "rm -rf /*",
            "everything in the filesystem root '/home/me/../../*'",
            Some("rm -rf /*"),
        ),
        (
            "rm -rf ~/../*",
            "rm -rf /home/*",
            "every home folder '~/../*'",
            None,
        ),
        (
            "rm -rf $HOME/../*",
            "rm -rf /home/*",
            "every home folder '$HOME/../*'",
            None,
        ),
        (
            "rm -rf ${HOME}/../*",
            "rm -rf /home/*",
            "every home folder '${HOME}/../*'",
            None,
        ),
        (
            "rm -rf /tmp/..",
            "rm -rf /",
            "the filesystem root '/tmp/..'",
            Some("rm -rf /"),
        ),
        (
            "rm -rf /../..",
            "rm -rf /",
            "the filesystem root '/../..'",
            Some("rm -rf /"),
        ),
        (
            "rm -rf /tmp/x/../../home",
            "rm -rf /home",
            "top-level folder '/tmp/x/../../home'",
            None,
        ),
        (
            "rm -rf /tmp/x/..",
            "rm -rf /tmp",
            "top-level folder '/tmp/x/..'",
            None,
        ),
        (
            "rm -rf ~/x/..",
            "rm -rf ~",
            "the home folder '~/x/..'",
            Some("rm -rf /"),
        ),
        (
            "rm -rf //tmp/../*",
            "rm -rf /*",
            "everything in the filesystem root '//tmp/../*'",
            Some("rm -rf /*"),
        ),
        (
            "rm -rf /tmp/../",
            "rm -rf /",
            "the filesystem root '/tmp/../'",
            Some("rm -rf /"),
        ),
        (
            "rm -rf \"/tmp/../\"*",
            "rm -rf /*",
            "everything in the filesystem root '\"/tmp/../\"*'",
            Some("rm -rf /*"),
        ),
    ];

    #[test]
    fn a_dotdot_spelling_of_root_top_level_or_home_asks_like_its_normal_form() {
        for (cmd, literal, reason, hard) in ROWS {
            let got = analyze_bash_command(cmd);
            assert_eq!(got, Some(format!("{ASK} {reason}")), "{cmd}");
            assert!(analyze_bash_command(literal).is_some(), "{literal}");
            assert_eq!(hard_deny_match(cmd).map(|h| h.label), *hard, "{cmd}");
            assert_eq!(
                hard_deny_match(literal).map(|h| h.label),
                *hard,
                "{literal}"
            );
        }
    }

    #[test]
    fn a_dotdot_path_that_stays_below_a_top_level_folder_still_passes() {
        for cmd in [
            "rm -rf /tmp/x/../y",
            "rm -rf ~/proj/../proj2/target",
            "rm -rf /tmp/a/./b",
        ] {
            assert_eq!(analyze_bash_command(cmd), None, "{cmd}");
            assert_eq!(hard_deny_match(cmd).map(|h| h.label), None, "{cmd}");
        }
    }
}
