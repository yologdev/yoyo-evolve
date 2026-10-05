//! Physical path resolution for `[directories]` / `--allow-dir` fences (#996 part 3).
//!
//! Lives beside `config.rs` (declared there with `#[path]`) so the fence's
//! resolver and its symlink tables do not grow that grandfathered module.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

/// Link hops followed before a path is declared unresolvable — the same
/// order as Linux's SYMLOOP_MAX (40). A loop exceeds it by construction.
pub(super) const MAX_LINK_HOPS: usize = 40;

/// Resolve `absolute` the way the kernel will, component by component.
///
/// Each existing component that is a symlink is replaced by its target
/// (relative targets resolve against the link's parent), and `..` is applied
/// only to the already-resolved physical prefix — never lexically across a
/// link, which is how `R/link/../x` escaped (#996). Components that do not
/// exist yet are appended verbatim, so a dangling link resolves to the path
/// its target names, and that path is what the fence judges.
///
/// Returns `None` when the path cannot be resolved (more than
/// [`MAX_LINK_HOPS`] hops, i.e. a loop, or an unreadable link). Callers must
/// treat `None` as "refuse", never as "inside".
pub(super) fn resolve_physical(absolute: &Path) -> Option<PathBuf> {
    let mut resolved = PathBuf::new();
    // Pending components, stored reversed so `pop` yields the next one.
    let mut pending: Vec<OsString> = Vec::new();
    push_components(absolute, &mut resolved, &mut pending);
    let mut hops = 0usize;

    while let Some(comp) = pending.pop() {
        if comp == "." {
            continue;
        }
        if comp == ".." {
            // `resolved` is a physical location (every link in it has been
            // replaced), so popping it is what the kernel's `..` does.
            resolved.pop();
            continue;
        }
        let next = resolved.join(&comp);
        match std::fs::symlink_metadata(&next) {
            Ok(meta) if meta.file_type().is_symlink() => {
                hops += 1;
                if hops > MAX_LINK_HOPS {
                    return None;
                }
                let target = std::fs::read_link(&next).ok()?;
                // `resolved` stays at the link's parent: a relative target is
                // resolved from there; an absolute one resets it.
                push_components(&target, &mut resolved, &mut pending);
            }
            // A real file/dir, or a component that does not exist (yet).
            _ => resolved = next,
        }
    }
    Some(resolved)
}

/// Queue `path`'s components onto `pending` (reversed). A root/prefix in
/// `path` resets `resolved` to that root, as an absolute link target does.
fn push_components(path: &Path, resolved: &mut PathBuf, pending: &mut Vec<OsString>) {
    let mut rest: Vec<OsString> = Vec::new();
    for c in path.components() {
        match c {
            Component::Prefix(p) => *resolved = PathBuf::from(p.as_os_str()),
            Component::RootDir => resolved.push(Component::RootDir.as_os_str()), // absolute push replaces
            Component::CurDir => {}
            Component::ParentDir => rest.push(OsString::from("..")),
            Component::Normal(n) => rest.push(n.to_os_string()),
        }
    }
    if path.has_root() {
        // RootDir without a Prefix (unix): start from "/" alone.
        if !matches!(path.components().next(), Some(Component::Prefix(_))) {
            *resolved = PathBuf::from(Component::RootDir.as_os_str());
        }
    }
    pending.extend(rest.into_iter().rev());
}

#[cfg(all(test, unix))]
mod tests {
    use crate::config::DirectoryRestrictions;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};

    /// An allowed root R and an outside dir O, both canonical, both in one
    /// temp dir that is never the repo.
    struct Fence {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        outside: PathBuf,
    }

    fn fence() -> Fence {
        let tmp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(tmp.path()).unwrap();
        let root = base.join("root");
        let outside = base.join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        Fence {
            _tmp: tmp,
            root,
            outside,
        }
    }

    fn allow_only(root: &Path) -> DirectoryRestrictions {
        DirectoryRestrictions {
            allow: vec![root.to_string_lossy().to_string()],
            deny: vec![],
        }
    }

    fn verdict(r: &DirectoryRestrictions, p: &Path) -> Result<(), String> {
        r.check_path(&p.to_string_lossy())
    }

    // ---------------------------------------------------------------
    // The three escapes: each one was ALLOWED before #996 part 3.
    // ---------------------------------------------------------------

    /// Escape 1: `R/link -> O/does-not-exist`. Writing `R/link` creates a file
    /// in O, so the fence must judge the link's target, not its spelling.
    #[test]
    fn escape_dangling_link_out_of_root_is_refused() {
        let f = fence();
        let target = f.outside.join("does-not-exist");
        assert!(!target.exists(), "anti-vacuous: the target must dangle");
        let link = f.root.join("link");
        symlink(&target, &link).unwrap();
        let r = allow_only(&f.root);
        assert!(
            verdict(&r, &link).is_err(),
            "a dangling link whose target is outside the allowed root must be refused"
        );
        // And a path through it (the target's would-be child) as well.
        assert!(verdict(&r, &link.join("child.txt")).is_err());
    }

    /// Escape 2: `R/link -> O/sub`, then `R/link/../x`. Lexically `..` cancels
    /// the link; the kernel follows the link first, so `..` lands in O.
    #[test]
    fn escape_link_then_parent_dir_is_refused() {
        let f = fence();
        let sub = f.outside.join("sub");
        std::fs::create_dir(&sub).unwrap();
        let link = f.root.join("link");
        symlink(&sub, &link).unwrap();
        let candidate = link.join("..").join("x");
        assert!(
            !f.outside.join("x").exists(),
            "anti-vacuous: x must not exist, or canonicalize alone would resolve it"
        );
        let r = allow_only(&f.root);
        assert!(
            verdict(&r, &candidate).is_err(),
            "R/link/../x resolves to O/x when link points into O; it must be refused"
        );
    }

    /// Escape 3: a link loop. `canonicalize` fails with ELOOP, and the
    /// fallback must not read the unresolvable path as "inside R".
    #[test]
    fn escape_link_loop_is_refused() {
        let f = fence();
        let a = f.root.join("a");
        let b = f.root.join("b");
        symlink(&b, &a).unwrap();
        symlink(&a, &b).unwrap();
        let r = allow_only(&f.root);
        assert!(verdict(&r, &a).is_err(), "a symlink loop must be refused");
        assert!(verdict(&r, &a.join("x")).is_err());
    }

    /// A self-loop is the one-link form of escape 3.
    #[test]
    fn escape_self_loop_is_refused() {
        let f = fence();
        let a = f.root.join("self");
        symlink(&a, &a).unwrap();
        assert!(verdict(&allow_only(&f.root), &a).is_err());
    }

    // ---------------------------------------------------------------
    // Near-misses: each one was ALLOWED before and must stay allowed.
    // ---------------------------------------------------------------

    /// (a) A new, not-yet-existing file directly in R.
    #[test]
    fn near_miss_new_file_in_root_is_allowed() {
        let f = fence();
        assert_eq!(
            verdict(&allow_only(&f.root), &f.root.join("new.txt")),
            Ok(())
        );
    }

    /// (b) A new file in a not-yet-existing subdir of R.
    #[test]
    fn near_miss_new_file_in_new_subdir_is_allowed() {
        let f = fence();
        let p = f.root.join("newdir").join("deeper").join("new.txt");
        assert_eq!(verdict(&allow_only(&f.root), &p), Ok(()));
    }

    /// (c) An in-root symlink `R/l -> R/real` and a new file through it.
    #[test]
    fn near_miss_in_root_link_and_new_file_through_it_are_allowed() {
        let f = fence();
        let real = f.root.join("real");
        std::fs::create_dir(&real).unwrap();
        let l = f.root.join("l");
        symlink(&real, &l).unwrap();
        let r = allow_only(&f.root);
        assert_eq!(verdict(&r, &l), Ok(()));
        assert_eq!(verdict(&r, &l.join("new.txt")), Ok(()));
        // Relative in-root target too.
        let rel = f.root.join("rel");
        symlink("real", &rel).unwrap();
        assert_eq!(verdict(&r, &rel.join("new.txt")), Ok(()));
    }

    /// (d) `R/sub/../x` with no links: plain lexical `..` inside the root,
    /// both with `sub` existing and with `sub` not existing.
    #[test]
    fn near_miss_plain_parent_dir_inside_root_is_allowed() {
        let f = fence();
        let r = allow_only(&f.root);
        let missing = f.root.join("sub").join("..").join("x");
        assert_eq!(verdict(&r, &missing), Ok(()));
        std::fs::create_dir(f.root.join("sub")).unwrap();
        assert_eq!(verdict(&r, &missing), Ok(()));
    }

    /// Inverse probe: a dangling link whose target is INSIDE R is allowed —
    /// the target is what is judged, not the fact that it dangles.
    #[test]
    fn inverse_dangling_link_into_root_is_allowed() {
        let f = fence();
        let link = f.root.join("dangling_in");
        symlink(f.root.join("not-yet"), &link).unwrap();
        let r = allow_only(&f.root);
        assert_eq!(verdict(&r, &link), Ok(()));
        let rel = f.root.join("dangling_rel");
        symlink("not-yet-either", &rel).unwrap();
        assert_eq!(verdict(&r, &rel), Ok(()));
    }

    /// The deny arm only narrows: a dangling link into a denied dir is denied,
    /// and so is `link/../x` landing in a denied dir.
    #[test]
    fn deny_arm_judges_the_link_target_too() {
        let f = fence();
        let deny = DirectoryRestrictions {
            allow: vec![],
            deny: vec![f.outside.to_string_lossy().to_string()],
        };
        let link = f.root.join("to_secret");
        symlink(f.outside.join("secret.txt"), &link).unwrap();
        assert!(verdict(&deny, &link).is_err());

        let sub = f.outside.join("sub");
        std::fs::create_dir(&sub).unwrap();
        let link2 = f.root.join("to_sub");
        symlink(&sub, &link2).unwrap();
        assert!(verdict(&deny, &link2.join("..").join("x")).is_err());
        // Near-miss: an ordinary file in R is untouched by a deny on O.
        assert_eq!(verdict(&deny, &f.root.join("ok.txt")), Ok(()));
    }

    // ── #998: a configured ENTRY that is itself a symlink ──────────────────
    // `check_path` resolves every configured entry with `resolve_path`, so an
    // entry is judged at its link TARGET. These pin that for the entry side
    // (the tests above pin the candidate side). Everything lives under
    // `fence().root`, a canonical temp dir, never the repo.

    fn entries(allow: &[&Path], deny: &[&Path]) -> DirectoryRestrictions {
        let s = |v: &[&Path]| v.iter().map(|p| p.to_string_lossy().to_string()).collect();
        DirectoryRestrictions {
            allow: s(allow),
            deny: s(deny),
        }
    }

    /// (a) The consistency premise the #998 decision rests on: an allow entry
    /// that is a link to an EXISTING directory has always matched at its
    /// target (`fs::canonicalize` follows links), so a candidate inside the
    /// target is allowed.
    #[test]
    fn entry_existing_link_allow_matches_at_its_target() {
        let f = fence();
        let real = f.root.join("real");
        std::fs::create_dir(&real).unwrap();
        let link = f.root.join("link");
        symlink(&real, &link).unwrap();
        let r = entries(&[&link], &[]);
        assert_eq!(verdict(&r, &real.join("f.txt")), Ok(()));
        // Near-miss: a sibling of the target is still outside the entry.
        assert!(verdict(&r, &f.outside.join("f.txt")).is_err());
    }

    /// (b) A dangling deny entry denies at its (not-yet-created) target.
    #[test]
    fn entry_dangling_link_deny_denies_at_its_target() {
        let f = fence();
        let gone = f.root.join("gone");
        let dlink = f.root.join("dlink");
        symlink(&gone, &dlink).unwrap();
        assert!(!gone.exists(), "fixture: the deny entry must dangle");
        let r = entries(&[], &[&dlink]);
        assert!(verdict(&r, &gone.join("f.txt")).is_err());
        // Near-miss: a deny on `gone` says nothing about the rest of root.
        assert_eq!(verdict(&r, &f.root.join("ok.txt")), Ok(()));
    }

    /// (c) The #998 decision: a dangling ALLOW entry also matches at its
    /// target, the same as (a). Refusing here would make the entry's meaning
    /// change the moment its target directory is created (inert today, wide
    /// tomorrow), a time-dependent boundary.
    #[test]
    fn entry_dangling_link_allow_matches_at_its_target() {
        let f = fence();
        let later = f.root.join("later");
        let alink = f.root.join("alink");
        symlink(&later, &alink).unwrap();
        assert!(!later.exists(), "fixture: the allow entry must dangle");
        let r = entries(&[&alink], &[]);
        assert_eq!(verdict(&r, &later.join("f.txt")), Ok(()));
        // Same verdict once the target exists: the boundary does not move.
        std::fs::create_dir(&later).unwrap();
        assert_eq!(verdict(&r, &later.join("f.txt")), Ok(()));
        // Near-miss: outside the target is still refused.
        assert!(verdict(&r, &f.outside.join("f.txt")).is_err());
    }

    /// (d) A looping allow entry falls back to its literal path, and a
    /// candidate under the loop is itself unresolvable, so it is refused;
    /// the entry is inert rather than wide.
    #[test]
    fn entry_looping_link_allow_is_inert() {
        let f = fence();
        let a = f.root.join("a");
        let b = f.root.join("b");
        symlink(&b, &a).unwrap();
        symlink(&a, &b).unwrap();
        let r = entries(&[&a], &[]);
        assert!(verdict(&r, &a.join("f.txt")).is_err());
        assert!(verdict(&r, &f.root.join("f.txt")).is_err());
        assert!(verdict(&r, &f.outside.join("f.txt")).is_err());
    }

    /// (e) Near-miss: an ordinary existing directory entry allows its own
    /// child and refuses a sibling directory's child, exactly as before.
    #[test]
    fn entry_plain_directory_allow_is_unchanged() {
        let f = fence();
        let r = entries(&[&f.root], &[]);
        assert_eq!(verdict(&r, &f.root.join("f.txt")), Ok(()));
        assert!(verdict(&r, &f.outside.join("f.txt")).is_err());
    }
}
