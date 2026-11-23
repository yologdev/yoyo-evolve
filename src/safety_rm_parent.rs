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
