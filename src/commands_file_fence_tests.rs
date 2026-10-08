//! #1002, the user's-own-prompt channel: `@path` mentions, `/add`, `/explain`
//! and the image reader honour `--deny-dir`/`--allow-dir` at the file's
//! RESOLVED target, through the one check in `read_file_for_add` /
//! `read_image_for_add`. Every fixture lives in a temp dir; the paths are
//! absolute, so nothing here reads or writes the repo cwd.
#![cfg(unix)]

use crate::commands_file::{
    build_explain_prompt, expand_file_mentions, handle_add, read_file_for_add, read_image_for_add,
    AddReadError, AddResult,
};
use crate::commands_file_fence::add_fence_refusal_message;
use crate::config::DirectoryRestrictions;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

const TOKEN: &str = "SECRET_MENTION_TOKEN_Q7";

/// `secret/notes.md` (holds TOKEN), `secret/pic.png`, `proj/` with
/// `proj/notes.md -> ../secret/notes.md`, `proj/pic.png -> ../secret/pic.png`,
/// an ordinary `proj/plain.md`, and `proj/inner.md -> plain.md` (a link whose
/// target is inside proj). All paths canonical.
struct Fixture {
    _tmp: tempfile::TempDir,
    secret: PathBuf,
    proj: PathBuf,
}

fn fixture() -> Fixture {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let secret = base.join("secret");
    let proj = base.join("proj");
    std::fs::create_dir_all(&secret).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(secret.join("notes.md"), format!("{TOKEN}\n")).unwrap();
    std::fs::write(secret.join("pic.png"), b"PNG_BYTES_SECRET").unwrap();
    std::fs::write(proj.join("plain.md"), "hello plain\n").unwrap();
    symlink("../secret/notes.md", proj.join("notes.md")).unwrap();
    symlink("../secret/pic.png", proj.join("pic.png")).unwrap();
    symlink("plain.md", proj.join("inner.md")).unwrap();
    Fixture {
        _tmp: tmp,
        secret,
        proj,
    }
}

fn deny(dir: &Path) -> DirectoryRestrictions {
    DirectoryRestrictions {
        allow: vec![],
        deny: vec![dir.display().to_string()],
    }
}

fn allow(dir: &Path) -> DirectoryRestrictions {
    DirectoryRestrictions {
        allow: vec![dir.display().to_string()],
        deny: vec![],
    }
}

fn none() -> DirectoryRestrictions {
    DirectoryRestrictions::default()
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

/// The exact warning line a door receives for a deny refusal, spelled out
/// from literals (not from `check_path`) so the reason text is pinned too.
fn expected_deny_line(path: &str, denied: &str) -> String {
    let reason = format!("Access denied: '{path}' is under restricted directory '{denied}'");
    if crate::format::is_plain_output() {
        format!("warning: {path} not sent to the model - {reason}")
    } else {
        format!("⚠ {path} not sent to the model — {reason}")
    }
}

fn texts(results: &[AddResult]) -> Vec<String> {
    results
        .iter()
        .map(|r| match r {
            AddResult::Text { content, .. } => content.clone(),
            AddResult::Image { data, .. } => format!("IMAGE:{data}"),
        })
        .collect()
}

// ---------------------------------------------------------------------
// The composer, both plain values, whole string.
// ---------------------------------------------------------------------

#[test]
fn refusal_message_is_whole_line_in_both_modes() {
    assert_eq!(
        add_fence_refusal_message("a/b.md", "Access denied: 'a/b.md' is under restricted directory 'a'", false),
        "⚠ a/b.md not sent to the model — Access denied: 'a/b.md' is under restricted directory 'a'"
    );
    assert_eq!(
        add_fence_refusal_message("a/b.md", "Access denied: 'a/b.md' is under restricted directory 'a'", true),
        "warning: a/b.md not sent to the model - Access denied: 'a/b.md' is under restricted directory 'a'"
    );
}

#[test]
fn refusal_message_escapes_control_bytes() {
    let hostile = "x\u{1b}[31m.md";
    assert!(hostile.as_bytes().contains(&0x1b), "anti-vacuous");
    let out = add_fence_refusal_message(hostile, hostile, false);
    assert!(!out.as_bytes().contains(&0x1b), "{out:?}");
}

// ---------------------------------------------------------------------
// Refusals: a denied symlinked target, a direct denied path, allow-list.
// ---------------------------------------------------------------------

#[test]
fn read_file_for_add_refuses_symlink_into_denied_dir_with_exact_line() {
    let f = fixture();
    let link = s(&f.proj.join("notes.md"));
    let err = read_file_for_add(&link, None, &deny(&f.secret)).unwrap_err();
    assert_eq!(
        err,
        AddReadError::Refused(expected_deny_line(&link, &s(&f.secret)))
    );
    assert!(!err.to_string().contains(TOKEN));
}

#[test]
fn mention_of_symlink_into_denied_dir_is_not_inlined() {
    let f = fixture();
    let link = s(&f.proj.join("notes.md"));
    // Anti-vacuous: unfenced, this exact mention DOES inline the token.
    let (_, open) = expand_file_mentions(&format!("look @{link}"), &none());
    assert!(
        texts(&open).concat().contains(TOKEN),
        "fixture must leak unfenced"
    );

    let input = format!("look @{link} please");
    let (text, results) = expand_file_mentions(&input, &deny(&f.secret));
    assert!(results.is_empty(), "{:?}", texts(&results));
    assert_eq!(
        text, input,
        "the mention stays as typed; the rest goes through"
    );
    assert!(!text.contains(TOKEN));
}

#[test]
fn mention_direct_path_into_denied_dir_is_not_inlined() {
    let f = fixture();
    let direct = s(&f.secret.join("notes.md"));
    let input = format!("see @{direct}");
    let (text, results) = expand_file_mentions(&input, &deny(&f.secret));
    assert!(results.is_empty());
    assert_eq!(text, input);
}

#[test]
fn denied_mention_does_not_block_the_other_mentions() {
    let f = fixture();
    let link = s(&f.proj.join("notes.md"));
    let plain = s(&f.proj.join("plain.md"));
    let (text, results) = expand_file_mentions(&format!("@{link} and @{plain}"), &deny(&f.secret));
    assert_eq!(text, format!("@{link} and plain.md"));
    assert_eq!(
        texts(&results),
        vec![format!("**{plain}**\n```markdown\nhello plain\n\n```")]
    );
}

#[test]
fn add_refuses_symlink_and_direct_path_into_denied_dir() {
    let f = fixture();
    for p in [f.proj.join("notes.md"), f.secret.join("notes.md")] {
        let (results, added) = handle_add(&format!("/add {}", s(&p)), &deny(&f.secret));
        assert!(results.is_empty(), "{}: {:?}", s(&p), texts(&results));
        assert!(added.is_empty());
    }
}

#[test]
fn image_symlink_into_denied_dir_is_refused_on_both_doors() {
    let f = fixture();
    let link = s(&f.proj.join("pic.png"));
    let err = read_image_for_add(&link, &deny(&f.secret)).unwrap_err();
    assert_eq!(
        err,
        AddReadError::Refused(expected_deny_line(&link, &s(&f.secret)))
    );
    let (results, _) = handle_add(&format!("/add {link}"), &deny(&f.secret));
    assert!(results.is_empty());
    let (text, results) = expand_file_mentions(&format!("@{link}"), &deny(&f.secret));
    assert!(results.is_empty());
    assert_eq!(text, format!("@{link}"));
}

#[test]
fn allow_list_refuses_link_whose_target_is_outside_it() {
    let f = fixture();
    let link = s(&f.proj.join("notes.md"));
    let err = read_file_for_add(&link, None, &allow(&f.proj)).unwrap_err();
    assert!(matches!(err, AddReadError::Refused(_)), "{err:?}");
    let (_, results) = expand_file_mentions(&format!("@{link}"), &allow(&f.proj));
    assert!(results.is_empty(), "{:?}", texts(&results));
}

#[test]
fn explain_refuses_denied_target() {
    let f = fixture();
    let link = s(&f.proj.join("notes.md"));
    assert_eq!(
        build_explain_prompt(&format!("/explain {link}"), &deny(&f.secret)),
        None
    );
    // Anti-vacuous: unfenced, /explain inlines the token.
    let open = build_explain_prompt(&format!("/explain {link}"), &none()).unwrap();
    assert!(open.contains(TOKEN));
}

// ---------------------------------------------------------------------
// Near-misses: allowed files, allowed links, and no fence at all are
// byte-identical to the unrestricted read.
// ---------------------------------------------------------------------

#[test]
fn allowed_file_and_allowed_symlink_are_unchanged() {
    let f = fixture();
    let plain = s(&f.proj.join("plain.md"));
    let inner = s(&f.proj.join("inner.md"));
    for r in [deny(&f.secret), allow(&f.proj)] {
        for p in [&plain, &inner] {
            let input = format!("look @{p}");
            assert_eq!(
                expand_file_mentions(&input, &r).0,
                expand_file_mentions(&input, &none()).0
            );
            assert_eq!(
                texts(&expand_file_mentions(&input, &r).1),
                vec![format!("**{p}**\n```markdown\nhello plain\n\n```")]
            );
            let (res, added) = handle_add(&format!("/add {p}"), &r);
            assert_eq!(
                texts(&res),
                texts(&handle_add(&format!("/add {p}"), &none()).0)
            );
            assert_eq!(added, vec![p.clone()]);
            assert_eq!(
                read_file_for_add(p, None, &r),
                Ok(("hello plain\n".to_string(), 1))
            );
        }
    }
}

#[test]
fn no_restrictions_inline_exactly_as_before() {
    let f = fixture();
    let link = s(&f.proj.join("notes.md"));
    let (text, results) = expand_file_mentions(&format!("look @{link}"), &none());
    assert_eq!(text, "look notes.md");
    assert_eq!(
        texts(&results),
        vec![format!("**{link}**\n```markdown\n{TOKEN}\n\n```")]
    );
    let (res, added) = handle_add(&format!("/add {link}"), &none());
    assert_eq!(
        texts(&res),
        vec![format!("**{link}**\n```markdown\n{TOKEN}\n\n```")]
    );
    assert_eq!(added, vec![link]);
}
