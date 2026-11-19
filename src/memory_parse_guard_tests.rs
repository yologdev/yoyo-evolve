//! #1216: a `.yoyo/memory.json` that exists but does not parse is never written
//! over by a load -> modify -> save. Every writer (`/remember`, `/forget`,
//! auto-remember and its eviction) goes through the strict loader and leaves
//! the file's bytes alone; display readers still show an empty store, but warn.

use super::*;

/// One trailing comma: the hand-edit typo from the #1216 repro.
const BROKEN: &str = r#"{"entries":[{"note":"keep me","timestamp":"2026-11-19T00:00:00Z"},]}"#;
/// The same store without the comma (the recall row).
const VALID: &str = r#"{"entries":[{"note":"keep me","timestamp":"2026-11-19T00:00:00Z"}]}"#;

fn store(dir: &tempfile::TempDir, body: Option<&str>) -> PathBuf {
    let path = dir.path().join(".yoyo").join("memory.json");
    if let Some(body) = body {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
    }
    path
}

fn notes(path: &Path) -> Vec<String> {
    load_memories_for_write(path)
        .unwrap()
        .entries
        .into_iter()
        .map(|e| e.note)
        .collect()
}

#[test]
fn remember_refuses_a_broken_store_and_leaves_its_bytes_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = store(&dir, Some(BROKEN));
    let err = crate::commands_memory::remember_at(&path, "new note", "general").unwrap_err();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BROKEN);
    assert!(err.contains("memory.json"), "names the file: {err}");
    assert!(err.contains("line 1 column"), "names where: {err}");
    assert!(
        err.contains("move the file aside"),
        "says the remedy: {err}"
    );
}

#[test]
fn remember_on_a_valid_store_keeps_both_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = store(&dir, Some(VALID));
    let total = crate::commands_memory::remember_at(&path, "new note", "general").unwrap();
    assert_eq!(total, 2);
    assert_eq!(notes(&path), ["keep me", "new note"]);
}

#[test]
fn remember_on_an_absent_store_creates_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = store(&dir, None);
    let total = crate::commands_memory::remember_at(&path, "first", "general").unwrap();
    assert_eq!(total, 1);
    assert_eq!(notes(&path), ["first"]);
}

#[test]
fn forget_refuses_a_broken_store_and_leaves_its_bytes_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = store(&dir, Some(BROKEN));
    let err = crate::commands_memory::forget_at(&path, 0).unwrap_err();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BROKEN);
    assert!(
        err.contains("memory.json") && err.contains("line 1 column"),
        "{err}"
    );
}

#[test]
fn auto_remember_and_its_eviction_never_write_a_broken_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = store(&dir, Some(BROKEN));
    // More than MAX_AUTO_MEMORIES calls, so the eviction branch would run too
    // if the loader handed back an empty store.
    for i in 0..=MAX_AUTO_MEMORIES {
        assert!(!auto_remember_to(&format!("auto note {i}"), &path));
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BROKEN);
}

#[test]
fn display_reader_stays_lenient_but_the_error_names_file_and_position() {
    let dir = tempfile::tempdir().unwrap();
    let path = store(&dir, Some(BROKEN));
    assert!(load_memories_from(&path).entries.is_empty());
    let reason = read_memories(&path).unwrap_err();
    assert!(reason.contains(&path.display().to_string()), "{reason}");
    assert!(reason.contains("line 1 column"), "{reason}");
}

#[cfg(unix)]
#[test]
fn an_unreadable_store_is_a_refusal_for_writers_not_an_empty_store() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = store(&dir, Some(VALID));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o200)).unwrap();
    if std::fs::read_to_string(&path).is_ok() {
        return eprintln!("running as root: permissions not enforced; skipping");
    }
    assert!(load_memories_for_write(&path).is_err());
    assert!(crate::commands_memory::remember_at(&path, "x", "general").is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), VALID);
}

/// Moved here from memory.rs's inline tests (size cap): the display reader
/// still treats garbage as an empty store (it now warns, #1216).
#[test]
fn test_load_memories_invalid_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = store(&dir, Some("not valid json at all {{{"));
    let memory = load_memories_from(&path);
    assert!(
        memory.entries.is_empty(),
        "Invalid JSON should return empty memory"
    );
}
