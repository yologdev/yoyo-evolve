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
/// #1236: also judged on [`resolved_anchored`]'s reading, which wins when it
/// is the root (`/tmp/link/../*` with `link -> /`, root's `~/../*`).
pub(super) fn rm_anchored_target(t: &str) -> Option<&'static str> {
    rm_anchored_target_with(t, resolved_anchored(t))
}

/// [`rm_anchored_target`] with the resolved reading passed in, so tests never
/// touch `HOME` or the filesystem. The stricter reading wins: the lexical
/// label unless the resolved path is the root, and never a pass where the
/// lexical reading asks.
pub(crate) fn rm_anchored_target_with(t: &str, resolved: Option<String>) -> Option<&'static str> {
    let lexical = normalise_anchored(t).and_then(|n| anchored_label(&n));
    let resolved = resolved.and_then(|n| anchored_label(&n));
    let is_root = |l: &str| l.ends_with("the filesystem root");
    match (lexical, resolved) {
        (Some(l), Some(r)) if is_root(r) && !is_root(l) => Some(r),
        (l, r) => l.or(r),
    }
}

/// #1236: the absolute path the kernel deletes for an anchored operand with a
/// `..`: the real home put in for `~`/`$HOME`/`${HOME}`, and the part before
/// the first `..` resolved with `std::fs::canonicalize` (symlinks followed).
/// The symlink can change between this check and the run, so this narrows
/// the hole and does not seal it.
pub(crate) fn resolved_anchored(t: &str) -> Option<String> {
    let home = std::env::var("HOME").ok();
    resolved_anchored_with(t, home.as_deref(), |p| std::fs::canonicalize(p).ok())
}

/// [`resolved_anchored`] with the home value and prefix resolver passed in.
/// `None` when the operand has no `..` (the resolver is never called), is not
/// anchored, or is home-anchored with no absolute home. A prefix the resolver
/// can't read (missing, no permission) keeps its lexical spelling.
pub(crate) fn resolved_anchored_with(
    t: &str,
    home: Option<&str>,
    resolve: impl Fn(&std::path::Path) -> Option<std::path::PathBuf>,
) -> Option<String> {
    let t = t.split(['&', '|', '<', '>']).next().unwrap_or("");
    let abs = match t.strip_prefix('/') {
        Some(_) => t.to_string(),
        None => {
            let rest = ["${HOME}", "$HOME", "~"]
                .into_iter()
                .find_map(|a| t.strip_prefix(a))
                .filter(|r| r.is_empty() || r.starts_with('/'))?;
            let home = home.filter(|h| h.starts_with('/'))?;
            format!("{}{rest}", home.trim_end_matches('/'))
        }
    };
    let abs = abs.trim_end_matches([')', '`']);
    let parts: Vec<&str> = abs.split('/').filter(|p| !matches!(*p, "" | ".")).collect();
    let first_up = parts.iter().position(|p| *p == "..")?;
    let prefix = format!("/{}", parts[..first_up].join("/"));
    let base = match first_up {
        0 => None,
        _ => resolve(std::path::Path::new(&prefix)).and_then(|p| p.to_str().map(str::to_string)),
    }
    .filter(|b| b.starts_with('/'))
    .unwrap_or(prefix);
    let mut out: Vec<&str> = Vec::new();
    for p in base.split('/').chain(parts[first_up..].iter().copied()) {
        match p {
            "" | "." => {}
            ".." => drop(out.pop()), // `/..` is `/`
            _ => out.push(p),
        }
    }
    Some(format!("/{}", out.join("/")))
}

/// The label for a normal form from [`normalise_anchored`] or
/// [`resolved_anchored`].
fn anchored_label(n: &str) -> Option<&'static str> {
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
        // #1236: these rows read the real HOME; where its parent is `/` (root's
        // `/root`) `~/../*` is the root and is pinned by the pure rows instead.
        let root_home = super::resolved_anchored("~/..").as_deref() == Some("/");
        for (cmd, literal, reason, hard) in ROWS {
            if root_home && reason.starts_with("every home folder") {
                continue;
            }
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

/// #1229: a recursive rm whose operand is an absolute path naming the working
/// folder `cwd`, a folder holding it, or everything in one of those
/// (`/home/me/proj`, `/home/me/proj/`, `/home/me`, `/home/me/proj/*`). The rm
/// itself is read by the same scan that judges `$PWD` (`check_rm_destruction`),
/// run on the raw-pass text with each hit put back as `$PWD`, so flags,
/// separators, redirects and wrappers mean what they mean there. Not judged:
/// a `cd` before the rm (the folder is the one yoyo knows, so `cd /x && rm -rf
/// <cwd>` still asks, erring toward asking) and relative spellings (those are
/// `rm_parent_target`'s).
pub(crate) fn rm_targets_cwd_or_ancestor(command: &str, cwd: &std::path::Path) -> Option<String> {
    let cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| lexical(&cwd.to_string_lossy()));
    let text = crate::bash_parse::raw_pass_text(command, &super::strip_quoted_regions(command));
    let pieces: Vec<&str> = text.split_inclusive(char::is_whitespace).collect();
    pieces.iter().enumerate().find_map(|(i, piece)| {
        let token = piece.trim_end();
        let (what, marked) = cwd_operand(token, &cwd)?;
        let mut probe: Vec<&str> = pieces.clone();
        let blank = &piece[token.len()..];
        let swapped = format!("{marked}{blank}");
        probe[i] = &swapped;
        let probe = probe.concat();
        let msg = super::check_rm_destruction(&probe, &probe.to_lowercase())?;
        let severity = if msg.contains("force-recursive") {
            "force-"
        } else {
            ""
        };
        Some(format!(
            "Destructive command: {severity}recursive delete targeting {what} '{token}'"
        ))
    })
}

/// What `token` names relative to `cwd` (already canonical), and the token
/// re-spelt with `$PWD` in its place; `None` unless it is an absolute path
/// equal to `cwd` or one of its ancestors, compared by component (so
/// `/home/me/proj-old` is not an ancestor of `/home/me/proj`). Judged both
/// lexically and, when it exists, on disk (symlinks followed, as #1236);
/// either reading hitting is a hit.
///
/// The lexical reading needs no file to exist; the on-disk one is [`on_disk`].
fn cwd_operand(token: &str, cwd: &std::path::Path) -> Option<(&'static str, String)> {
    let cut = token.find(['&', '|', '<', '>']).unwrap_or(token.len());
    let (head, tail) = token.split_at(cut);
    let unquoted: String = head.chars().filter(|c| !matches!(c, '"' | '\'')).collect();
    let path = unquoted.trim_end_matches([';', ')', '`', '}']);
    let closers = &unquoted[path.len()..];
    let (path, glob) = match path.strip_suffix("/*") {
        Some(p) => (p, true),
        None => (path, false),
    };
    if !path.starts_with('/') {
        return None;
    }
    let readings = [Some(lexical(path)), on_disk(path, glob)];
    let what = readings.into_iter().flatten().find_map(|p| {
        let here = p == cwd;
        (here || cwd.starts_with(&p)).then_some((here, glob))
    })?;
    let what = match what {
        (true, false) => "the working folder",
        (true, true) => "everything in the working folder",
        (false, false) => "a folder holding the working folder",
        (false, true) => "everything in a folder holding the working folder",
    };
    let glob = if glob { "/*" } else { "" };
    Some((what, format!("$PWD{glob}{closers}{tail}")))
}

/// Where the kernel lands for a recursive rm of `path`: every part but the
/// last resolved (symlinks followed), and the last too when it is `.`, `..`,
/// ends in `/` or is globbed into. A bare `rm -rf link` unlinks the symlink
/// itself, so its last part is kept as written. `None` when the folder holding
/// it can't be read. Looked up at check time, so a link swapped before the run
/// is not caught.
fn on_disk(path: &str, glob: bool) -> Option<std::path::PathBuf> {
    let trimmed = path.trim_end_matches('/');
    let last = trimmed.rsplit('/').next().unwrap_or("");
    if glob || trimmed.len() != path.len() || matches!(last, "" | "." | "..") {
        return std::fs::canonicalize(path).ok();
    }
    let parent = &trimmed[..trimmed.len() - last.len()];
    let base = std::fs::canonicalize(if parent.is_empty() { "/" } else { parent }).ok()?;
    Some(base.join(last))
}

/// `p` (absolute) with empty and `.` parts dropped and each `..` taking off
/// the part before it, as the kernel reads it when no symlink is involved.
fn lexical(p: &str) -> std::path::PathBuf {
    let mut out = std::path::PathBuf::from("/");
    for part in p.split('/') {
        match part {
            "" | "." => {}
            ".." => drop(out.pop()),
            part => out.push(part),
        }
    }
    out
}

#[cfg(test)]
mod cwd_tests {
    use super::super::analyze_bash_command_in;
    use super::rm_targets_cwd_or_ancestor;

    /// A real temp dir as the working folder (`<tmp>/home/me/proj`), canonical
    /// so `/tmp` symlinks (macOS) don't split the readings. Nothing is deleted:
    /// these are pure checks on strings plus `canonicalize`.
    fn dirs() -> (tempfile::TempDir, String, String) {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let cwd = root.join("home/me/proj");
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::create_dir_all(root.join("home/me/other")).unwrap();
        let cwd = cwd.to_str().unwrap().to_string();
        let parent = root.join("home/me").to_str().unwrap().to_string();
        (tmp, cwd, parent)
    }

    const ASK: &str = "Destructive command: force-recursive delete targeting";

    #[test]
    fn the_working_folder_or_a_folder_holding_it_asks_by_its_absolute_path() {
        let (_tmp, cwd, parent) = dirs();
        let here = std::path::Path::new(&cwd);
        let rows = [
            (
                format!("rm -rf {cwd}"),
                format!("{ASK} the working folder '{cwd}'"),
            ),
            (
                format!("rm -rf {cwd}/"),
                format!("{ASK} the working folder '{cwd}/'"),
            ),
            (
                format!("rm -rf \"{cwd}\""),
                format!("{ASK} the working folder '\"{cwd}\"'"),
            ),
            (
                format!("rm -rf {parent}"),
                format!("{ASK} a folder holding the working folder '{parent}'"),
            ),
            (
                format!("rm -r -f {cwd}"),
                format!("{ASK} the working folder '{cwd}'"),
            ),
            (
                format!("cd /x && rm -rf {cwd}"),
                format!("{ASK} the working folder '{cwd}'"),
            ),
            (
                format!("rm -rf {cwd}/*"),
                format!("{ASK} everything in the working folder '{cwd}/*'"),
            ),
            (
                format!("rm -rf {cwd}/../proj"),
                format!("{ASK} the working folder '{cwd}/../proj'"),
            ),
            (
                format!("rm -r {cwd}"),
                format!(
                    "Destructive command: recursive delete targeting the working folder '{cwd}'"
                ),
            ),
        ];
        for (cmd, want) in rows {
            assert_eq!(
                rm_targets_cwd_or_ancestor(&cmd, here),
                Some(want.clone()),
                "{cmd}"
            );
            assert_eq!(
                analyze_bash_command_in(&cmd, Some(here)),
                Some(want),
                "{cmd}"
            );
        }
    }

    #[test]
    fn a_child_sibling_lookalike_or_non_recursive_path_still_passes() {
        let (_tmp, cwd, parent) = dirs();
        let here = std::path::Path::new(&cwd);
        for cmd in [
            format!("rm -rf {cwd}/target"), // #1229's own near-miss row
            format!("rm -rf {parent}/other"),
            format!("rm -rf {cwd}-old"),
            format!("rm {cwd}/file"),
            format!("rm -f {cwd}"),
            format!("ls {cwd}"),
            format!("ls -r {cwd}"),
            format!("rm -rf build && ls {cwd}"),
            format!("git commit -m \"rm -rf {cwd}\""),
        ] {
            assert_eq!(rm_targets_cwd_or_ancestor(&cmd, here), None, "{cmd}");
            assert_eq!(analyze_bash_command_in(&cmd, Some(here)), None, "{cmd}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_the_working_folder_asks_when_deleted_recursively() {
        let (tmp, cwd, _) = dirs();
        let link = std::fs::canonicalize(tmp.path()).unwrap().join("link");
        std::os::unix::fs::symlink(&cwd, &link).unwrap();
        let here = std::path::Path::new(&cwd);
        for tail in ["/", "/*", "/."] {
            let cmd = format!("rm -rf {}{tail}", link.display());
            let what = match tail {
                "/*" => "everything in the working folder",
                _ => "the working folder",
            };
            let want = format!("{ASK} {what} '{}{tail}'", link.display());
            assert_eq!(rm_targets_cwd_or_ancestor(&cmd, here), Some(want), "{cmd}");
        }
        // A bare `rm -rf link` unlinks the symlink and leaves the folder.
        let cmd = format!("rm -rf {}", link.display());
        assert_eq!(rm_targets_cwd_or_ancestor(&cmd, here), None, "{cmd}");
        // A symlinked folder on the way is followed: `<link>/../proj` is the
        // cwd's parent's `proj`, i.e. the cwd, read on disk.
        let up = std::fs::canonicalize(tmp.path()).unwrap().join("up");
        std::os::unix::fs::symlink(std::path::Path::new(&cwd).join(".."), &up).unwrap();
        let cmd = format!("rm -rf {}/proj", up.display());
        let want = format!("{ASK} the working folder '{}/proj'", up.display());
        assert_eq!(rm_targets_cwd_or_ancestor(&cmd, here), Some(want), "{cmd}");
    }

    #[test]
    fn with_no_working_folder_the_absolute_path_is_not_judged() {
        let (_tmp, cwd, _) = dirs();
        assert_eq!(
            analyze_bash_command_in(&format!("rm -rf {cwd}"), None),
            None
        );
        // The rules that never needed the folder still ask.
        assert!(analyze_bash_command_in("rm -rf $PWD", None).is_some());
    }
}
