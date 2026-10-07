//! #1002 part 1: project instruction files (CLAUDE.md, AGENTS.md, …) honour
//! `--deny-dir` / `--allow-dir` at their **resolved** target, through the same
//! `DirectoryRestrictions::check_path` the file tools use since #996.
//!
//! Every fixture lives in a tempdir and is passed to the dir-taking seam
//! `load_project_context_restricted`; nothing here reads or writes the repo cwd.
#![cfg(unix)]

use crate::config::DirectoryRestrictions;
use crate::context::{
    instruction_file_refused_warning, load_project_context_from, load_project_context_restricted,
};
use std::os::unix::fs::symlink;

/// `secret/notes.md` + `secret/agents.md` holding tokens, and a `proj/` whose
/// CLAUDE.md and AGENTS.md are symlinks into `secret/`.
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::TempDir::new().unwrap();
    let base = root.path().canonicalize().unwrap();
    let secret = base.join("secret");
    let proj = base.join("proj");
    std::fs::create_dir_all(&secret).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(secret.join("notes.md"), "SECRET_TOKEN_ZZ9\n").unwrap();
    std::fs::write(secret.join("agents.md"), "AGENTS_SECRET_KK3\n").unwrap();
    symlink("../secret/notes.md", proj.join("CLAUDE.md")).unwrap();
    symlink("../secret/agents.md", proj.join("AGENTS.md")).unwrap();
    (root, secret, proj)
}

fn deny(dir: &std::path::Path) -> DirectoryRestrictions {
    DirectoryRestrictions {
        allow: vec![],
        deny: vec![dir.display().to_string()],
    }
}

fn allow(dirs: &[&std::path::Path]) -> DirectoryRestrictions {
    DirectoryRestrictions {
        allow: dirs.iter().map(|d| d.display().to_string()).collect(),
        deny: vec![],
    }
}

#[test]
fn instruction_link_into_denied_dir_is_not_loaded() {
    let (_root, secret, proj) = fixture();
    // Anti-vacuous: the unrestricted loader really does read both tokens, so
    // their absence below is the fence, not a broken fixture.
    let open = load_project_context_from(&proj).expect("fixture must load");
    assert!(open.contains("SECRET_TOKEN_ZZ9") && open.contains("AGENTS_SECRET_KK3"));

    let (ctx, refused) = load_project_context_restricted(&proj, &deny(&secret));
    let ctx = ctx.unwrap_or_default();
    assert!(!ctx.contains("SECRET_TOKEN_ZZ9"), "denied CLAUDE.md leaked");
    assert!(
        !ctx.contains("AGENTS_SECRET_KK3"),
        "denied AGENTS.md leaked"
    );
    let names: Vec<&str> = refused.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, vec!["CLAUDE.md", "AGENTS.md"]);
}

#[test]
fn instruction_link_outside_allow_list_is_not_loaded() {
    let (_root, _secret, proj) = fixture();
    let (ctx, refused) = load_project_context_restricted(&proj, &allow(&[&proj]));
    let ctx = ctx.unwrap_or_default();
    assert!(!ctx.contains("SECRET_TOKEN_ZZ9"));
    assert!(!ctx.contains("AGENTS_SECRET_KK3"));
    assert_eq!(refused.len(), 2);
}

#[test]
fn instruction_link_resolving_inside_allowed_set_still_loads() {
    // Near-miss: same links, but the target is inside the allowed set. The
    // check is on the resolved target, so it must pass — the same verdict the
    // file tools give a link entry whose target is allowed (#998 consistency).
    let (_root, secret, proj) = fixture();
    let (ctx, refused) = load_project_context_restricted(&proj, &allow(&[&proj, &secret]));
    let ctx = ctx.expect("allowed links must load");
    assert!(ctx.contains("SECRET_TOKEN_ZZ9") && ctx.contains("AGENTS_SECRET_KK3"));
    assert!(refused.is_empty());
}

#[test]
fn regular_file_and_no_restrictions_are_byte_identical() {
    // The whole regression surface: every user without --deny-dir/--allow-dir,
    // and every plain instruction file inside an allowed dir.
    let root = tempfile::TempDir::new().unwrap();
    let proj = root.path().canonicalize().unwrap();
    std::fs::write(proj.join("CLAUDE.md"), "PLAIN_RULES_QQ1\n").unwrap();
    let before = load_project_context_from(&proj);
    assert!(before.as_deref().unwrap_or("").contains("PLAIN_RULES_QQ1"));

    let (none, refused) = load_project_context_restricted(&proj, &DirectoryRestrictions::default());
    assert_eq!(none, before);
    assert!(refused.is_empty());

    let (inside, refused) = load_project_context_restricted(&proj, &allow(&[&proj]));
    assert_eq!(inside, before);
    assert!(refused.is_empty());
}

#[test]
fn refused_warning_names_file_sanitizes_and_is_plain_safe() {
    let reason = "Access denied: '/x/\u{1b}[31mCLAUDE.md' is under restricted directory '/x'";
    assert!(reason.as_bytes().contains(&0x1b), "anti-vacuous fixture");
    for plain in [false, true] {
        let w = instruction_file_refused_warning("CLAUDE.md", reason, plain);
        assert!(w.contains("CLAUDE.md"));
        assert!(w.contains("not loaded"));
        assert!(
            !w.as_bytes().contains(&0x1b),
            "control byte reached the terminal"
        );
        if plain {
            assert!(w.is_ascii(), "plain output must be glyph-free: {w}");
        }
    }
}
