//! #982 tests for `/find` / `yoyo find` on a walk that could not read a
//! directory: the readable matches still come back, the unreadable directory is
//! reported, and a fully readable tree is byte-identical to before. A sibling
//! file because `commands_search.rs` sits on the module-size gate.

use crate::commands_search::{
    find_files_in, find_unreadable_note, render_find_matches, FIND_UNREADABLE_MAX_SHOWN,
    FIND_UNREADABLE_PATH_MAX_BYTES,
};
use crate::format::{BOLD, DIM, GREEN, RESET};
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

/// `ok/target_zz.txt` and `locked/target_zz.txt` in a non-git tempdir.
fn two_dir_tree() -> TempDir {
    let dir = TempDir::new().unwrap();
    for sub in ["ok", "locked"] {
        std::fs::create_dir(dir.path().join(sub)).unwrap();
        std::fs::write(dir.path().join(sub).join("target_zz.txt"), "x").unwrap();
    }
    dir
}

/// Restores `locked`'s permissions on drop, so the tempdir can be removed
/// even when an assertion panics first.
struct Unlock(std::path::PathBuf);
impl Drop for Unlock {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

#[test]
fn unreadable_subdir_keeps_readable_match_and_reports_the_dir() {
    let dir = two_dir_tree();
    let root = dir.path().to_str().unwrap().to_string();
    let locked = dir.path().join("locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let _unlock = Unlock(locked.clone());
    if std::fs::read_dir(&locked).is_ok() {
        eprintln!("skipping: chmod 000 did not block read_dir (running as root?)");
        return;
    }

    let (matches, unreadable) = find_files_in(&root, "target_zz");
    let paths: Vec<&str> = matches.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(paths, vec![format!("{root}/ok/target_zz.txt").as_str()]);
    assert_eq!(
        unreadable,
        vec![(format!("{root}/locked"), "permission denied".to_string())]
    );
    let note = find_unreadable_note(&unreadable).expect("an unreadable dir must produce a note");
    assert_eq!(
        note,
        format!(
            "  find could not read 1 directory; the list above may be incomplete:\n    {root}/locked (permission denied)"
        )
    );
}

#[test]
fn readable_tree_has_no_errors_and_output_is_unchanged() {
    // Near-miss: the same tree, nothing locked.
    let dir = two_dir_tree();
    let root = dir.path().to_str().unwrap().to_string();
    let (matches, unreadable) = find_files_in(&root, "target_zz");
    assert!(unreadable.is_empty(), "got {unreadable:?}");
    assert_eq!(find_unreadable_note(&unreadable), None);
    assert_eq!(matches.len(), 2);

    // Byte-identical to the pre-#982 printing (same bytes `println!` produced).
    let hl = |p: &str| {
        let head = p.strip_suffix("target_zz.txt").unwrap();
        format!("{head}{BOLD}{GREEN}target_zz{RESET}.txt")
    };
    let expected = format!(
        "{DIM}  2 files matching 'target_zz':\n    {}\n    {}\n{RESET}\n",
        hl(&matches[0].path),
        hl(&matches[1].path)
    );
    assert_eq!(render_find_matches("target_zz", &matches), expected);
    // The no-match case is unchanged too (its exit code is undecided on #982).
    assert_eq!(
        render_find_matches("nope", &[]),
        format!("{DIM}  No files matching 'nope'.{RESET}\n\n")
    );
}

#[test]
fn note_caps_paths_on_a_char_boundary_and_counts_the_rest() {
    let long = format!("{}✓tail", "a".repeat(FIND_UNREADABLE_PATH_MAX_BYTES - 1));
    let mut dirs = vec![(long, "permission denied".to_string())];
    for i in 0..FIND_UNREADABLE_MAX_SHOWN + 2 {
        dirs.push((format!("d{i}\x1b[31m"), "permission denied".to_string()));
    }
    let note = find_unreadable_note(&dirs).unwrap();
    // Anti-vacuous: the fixture really carries an ESC byte.
    assert!(dirs[1].0.as_bytes().contains(&0x1b));
    assert!(!note.as_bytes().contains(&0x1b), "{note:?}");
    let esc = crate::cli::sanitize_for_display("\x1b[31m");
    let mut expected = format!(
        "  find could not read 8 directories; the list above may be incomplete:\n    {}… (permission denied)",
        "a".repeat(FIND_UNREADABLE_PATH_MAX_BYTES - 1)
    );
    for i in 0..FIND_UNREADABLE_MAX_SHOWN - 1 {
        expected.push_str(&format!("\n    d{i}{esc} (permission denied)"));
    }
    expected.push_str("\n    (… 3 more)");
    assert_eq!(note, expected);
}
