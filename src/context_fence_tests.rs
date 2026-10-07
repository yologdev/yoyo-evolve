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

// ---- #1002 part 3: `.yoyo/memory.json` behind the same fence ----

const MEM_TOKEN: &str = "MEMTOK_1002_ZQ";
const MEM_JSON: &str = r#"{"entries":[{"note":"MEMTOK_1002_ZQ","timestamp":"2026-01-01 00:00","category":"general"}]}"#;
/// The exact section `format_memories_for_prompt` renders for `MEM_JSON`,
/// typed here as the before-state the near-miss must keep byte-identical.
const MEM_SECTION: &str = "## Project Memories\n\n- MEMTOK_1002_ZQ (2026-01-01 00:00)";

/// `secret/mem.json` holding the token, and `proj/.yoyo/memory.json` a symlink to it.
fn memory_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::TempDir::new().unwrap();
    let base = root.path().canonicalize().unwrap();
    let secret = base.join("secret");
    let proj = base.join("proj");
    std::fs::create_dir_all(&secret).unwrap();
    std::fs::create_dir_all(proj.join(".yoyo")).unwrap();
    std::fs::write(secret.join("mem.json"), MEM_JSON).unwrap();
    symlink("../../secret/mem.json", proj.join(".yoyo/memory.json")).unwrap();
    // Anti-vacuous: the fixture file really carries the token, and the
    // unrestricted loader really reads it through the link.
    let raw = std::fs::read_to_string(secret.join("mem.json")).unwrap();
    assert!(raw.contains(MEM_TOKEN), "fixture must contain the token");
    let open = load_project_context_from(&proj).unwrap_or_default();
    assert!(open.contains(MEM_TOKEN), "unfenced load must read the link");
    (root, secret, proj)
}

#[test]
fn memory_link_into_denied_dir_is_not_loaded() {
    let (_root, secret, proj) = memory_fixture();
    let (ctx, refused) = load_project_context_restricted(&proj, &deny(&secret));
    let ctx = ctx.unwrap_or_default();
    assert!(
        !ctx.contains(MEM_TOKEN),
        "denied memory reached the prompt: {ctx}"
    );
    assert!(!ctx.contains("## Project Memories"));
    let names: Vec<&str> = refused.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, vec![".yoyo/memory.json"]);
}

#[test]
fn memory_link_outside_allow_list_is_not_loaded() {
    let (_root, _secret, proj) = memory_fixture();
    let (ctx, refused) = load_project_context_restricted(&proj, &allow(&[&proj]));
    assert!(!ctx.unwrap_or_default().contains(MEM_TOKEN));
    assert_eq!(refused.len(), 1);
}

#[test]
fn memory_link_with_fence_not_covering_target_still_loads() {
    // Near-miss: a deny list that names some other directory does not cover
    // the link's target, so the memory must load.
    let (root, _secret, proj) = memory_fixture();
    let other = root.path().canonicalize().unwrap().join("other");
    std::fs::create_dir_all(&other).unwrap();
    let (ctx, refused) = load_project_context_restricted(&proj, &deny(&other));
    assert!(ctx.unwrap_or_default().contains(MEM_TOKEN));
    assert!(refused.is_empty());
}

#[test]
fn plain_memory_file_without_fence_is_byte_identical() {
    // The regression surface: an ordinary memory.json, no fence. The section
    // must be exactly what it was before #1002 part 3.
    let root = tempfile::TempDir::new().unwrap();
    let proj = root.path().canonicalize().unwrap();
    std::fs::create_dir_all(proj.join(".yoyo")).unwrap();
    std::fs::write(proj.join(".yoyo/memory.json"), MEM_JSON).unwrap();
    let before = load_project_context_from(&proj).expect("memory must load");
    let start = before.find("## Project Memories").expect("memory section");
    assert_eq!(&before[start..], MEM_SECTION);

    let (fenced, refused) =
        load_project_context_restricted(&proj, &DirectoryRestrictions::default());
    assert_eq!(fenced.as_deref(), Some(before.as_str()));
    assert!(refused.is_empty());
    let (inside, refused) = load_project_context_restricted(&proj, &allow(&[&proj]));
    assert_eq!(inside.as_deref(), Some(before.as_str()));
    assert!(refused.is_empty());
}

#[test]
fn missing_memory_file_is_silent_under_a_fence() {
    let root = tempfile::TempDir::new().unwrap();
    let proj = root.path().canonicalize().unwrap();
    let (_, refused) = load_project_context_restricted(&proj, &deny(&proj.join("nope")));
    assert!(refused.is_empty());
    let (_, refused) = load_project_context_restricted(&proj, &allow(&[&proj.join("x")]));
    assert!(refused.is_empty(), "absent memory.json must not warn");
}

#[test]
fn memory_refused_warning_names_memory_not_instructions() {
    for plain in [false, true] {
        let w = instruction_file_refused_warning(".yoyo/memory.json", "Access denied", plain);
        assert!(w.contains(".yoyo/memory.json") && w.contains("not loaded"));
        assert!(w.contains("memory file"), "{w}");
        assert!(!w.contains("instruction file"), "{w}");
    }
}
