//! #1106: the auto path keeps the NEWEST entries at its cap (it used to freeze at
//! the oldest 20), never evicts a `/remember` entry, un-freezes a store written
//! before the `auto` field existed, and does not save a content-free panic header.

use super::*;

fn store_path(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join(".yoyo").join("memory.json")
}

fn user_entry(note: &str) -> MemoryEntry {
    MemoryEntry {
        note: note.to_string(),
        timestamp: "2026-01-01 00:00".to_string(),
        category: "convention".to_string(),
        auto: false,
    }
}

fn notes(path: &Path) -> Vec<String> {
    load_memories_from(path)
        .entries
        .into_iter()
        .map(|e| e.note)
        .collect()
}

#[test]
fn under_the_cap_saving_appends_exactly_as_before() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(&dir);
    let mut seed = ProjectMemory::default();
    seed.entries.push(user_entry("use pnpm, not npm"));
    save_memories_to(&seed, &path).unwrap();

    assert!(auto_remember_to("first auto fact", &path));
    assert!(auto_remember_to("second auto fact", &path));
    assert_eq!(
        notes(&path),
        vec!["use pnpm, not npm", "first auto fact", "second auto fact"]
    );
    let loaded = load_memories_from(&path);
    assert!(
        !loaded.entries[0].auto,
        "the /remember entry stays a user entry"
    );
    assert!(loaded.entries[1].auto && loaded.entries[2].auto);
}

#[test]
fn a_user_entry_serializes_without_the_auto_field() {
    // Files holding only /remember entries stay byte-identical to the old format.
    let json = serde_json::to_string(&user_entry("x")).unwrap();
    assert_eq!(
        json,
        r#"{"note":"x","timestamp":"2026-01-01 00:00","category":"convention"}"#
    );
}

#[test]
fn at_the_cap_the_newest_is_kept_and_the_oldest_auto_entry_goes() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(&dir);
    for i in 0..MAX_AUTO_MEMORIES {
        assert!(auto_remember_to(&format!("auto fact {i}"), &path));
    }
    assert!(auto_remember_to("auto fact newest", &path));
    let got = notes(&path);
    assert_eq!(got.len(), MAX_AUTO_MEMORIES);
    assert!(!got.contains(&"auto fact 0".to_string()), "{got:?}");
    assert_eq!(got[0], "auto fact 1");
    assert_eq!(got.last().unwrap(), "auto fact newest");
}

#[test]
fn a_full_store_never_evicts_a_user_entry() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(&dir);
    let mut seed = ProjectMemory::default();
    // User entries interleaved with (and older than) the auto ones.
    seed.entries.push(user_entry("user A"));
    seed.entries.push(user_entry("user B"));
    save_memories_to(&seed, &path).unwrap();
    for i in 0..MAX_AUTO_MEMORIES - 3 {
        assert!(auto_remember_to(&format!("auto {i}"), &path));
    }
    let mut mid = load_memories_from(&path);
    mid.entries.push(user_entry("user C"));
    save_memories_to(&mid, &path).unwrap();
    for i in MAX_AUTO_MEMORIES - 3..MAX_AUTO_MEMORIES + 10 {
        assert!(auto_remember_to(&format!("auto {i}"), &path));
    }
    let loaded = load_memories_from(&path);
    let users: Vec<&str> = loaded
        .entries
        .iter()
        .filter(|e| !is_auto_entry(e))
        .map(|e| e.note.as_str())
        .collect();
    assert_eq!(users, vec!["user A", "user B", "user C"]);
    let autos = loaded.entries.iter().filter(|e| is_auto_entry(e)).count();
    assert_eq!(autos, MAX_AUTO_MEMORIES);
    assert_eq!(loaded.entries.len(), MAX_AUTO_MEMORIES + 3);
    assert_eq!(
        loaded.entries.last().unwrap().note,
        format!("auto {}", MAX_AUTO_MEMORIES + 9)
    );
}

#[test]
fn a_legacy_store_frozen_at_the_old_cap_unfreezes() {
    // A pre-#1106 file: no `auto` field anywhere, 20 notes the builders wrote.
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(&dir);
    let mut entries = Vec::new();
    for i in 0..MAX_AUTO_MEMORIES {
        let note = if i % 2 == 0 {
            build_fix_memory_note("cargo test", i + 1)
        } else {
            format!("[bug] Test expectation in `cargo test`: thread 't{i}' (1) panicked at src/a.rs:{i}:9:")
        };
        entries.push(format!(
            r#"{{"note":{},"timestamp":"2026-08-12 0{}:00","category":"general"}}"#,
            serde_json::to_string(&note).unwrap(),
            i % 10
        ));
    }
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!(r#"{{"entries":[{}]}}"#, entries.join(","))).unwrap();
    let before = notes(&path);
    assert_eq!(before.len(), MAX_AUTO_MEMORIES);
    assert!(load_memories_from(&path).entries.iter().all(|e| !e.auto));

    assert!(auto_remember_to("a fact from after the freeze", &path));
    let after = notes(&path);
    assert_eq!(after.len(), MAX_AUTO_MEMORIES);
    assert_eq!(after.last().unwrap(), "a fact from after the freeze");
    assert_eq!(after[..MAX_AUTO_MEMORIES - 1], before[1..]);
}

#[test]
fn legacy_recognition_is_exact_about_the_builders_openings() {
    let auto = |n: &str| is_auto_entry(&user_entry(n));
    assert!(auto(&build_fix_memory_note("make", 2)));
    for cat in ["import", "type", "borrow", "lifetime", "unused", "syntax"] {
        let note = build_learn_memory_note("make", &["e".to_string()], cat).unwrap();
        assert!(auto(&note), "{note}");
    }
    // Near misses: a user's own wording that only resembles a builder's.
    assert!(!auto("Watch fix: remember to run make twice"));
    assert!(!auto("[bug] Test expectation: flaky on CI"));
    assert!(!auto("[build] Type issue"));
    assert!(!auto("always run cargo fmt"));
}

#[test]
fn a_content_free_panic_header_is_not_saved() {
    let header = "thread 'tests::t' (4861) panicked at src/main.rs:7:9:".to_string();
    assert_eq!(
        build_learn_memory_note("cargo test", &[header], "test_assertion"),
        None
    );
    // Trailing whitespace after the header is still nothing.
    let spaced = "thread 'tests::t' panicked at src/main.rs:7:9:   ".to_string();
    assert_eq!(
        build_learn_memory_note("cargo test", &[spaced], "test_assertion"),
        None
    );
}

#[test]
fn a_panic_header_with_its_assertion_is_saved_message_first() {
    // The watch parser joins the header and the assertion line with a space.
    let long_name = "commands_risk_epistemic::tests::test_graded_and_visited_reasons_are_distinct";
    let msg = format!(
        "thread '{long_name}' (51860) panicked at src/commands_risk_epistemic.rs:412:5: \
         assertion `left == right` failed: graded must differ"
    );
    let note = build_learn_memory_note("cargo test", &[msg], "test_assertion").unwrap();
    assert_eq!(
        note,
        format!(
            "[bug] Test expectation in `cargo test`: assertion `left == right` failed: \
             graded must differ ({long_name} at src/commands_risk_epistemic.rs:412:5)"
        )
    );
}

#[test]
fn a_long_assertion_is_cut_on_a_char_boundary() {
    let detail = "é".repeat(200);
    let msg = format!("thread 't' panicked at src/a.rs:1:1: {detail}");
    let note = build_learn_memory_note("cargo test", &[msg], "test_assertion").unwrap();
    let shown = format!("{}…", "é".repeat(120));
    assert_eq!(
        note,
        format!("[bug] Test expectation in `cargo test`: {shown} (t at src/a.rs:1:1)")
    );
}

#[test]
fn non_header_assertion_messages_are_unchanged() {
    // Legacy panic shape and plain assertion text carry content inline: as before.
    let legacy = "thread 'tests::my_test' panicked at 'assertion failed: x', src/lib.rs:99:9";
    assert_eq!(
        build_learn_memory_note("cargo test", &[legacy.to_string()], "test_assertion").unwrap(),
        format!("[bug] Test expectation in `cargo test`: {legacy}")
    );
    let plain = "assertion `left == right` failed";
    assert_eq!(
        build_learn_memory_note("cargo test", &[plain.to_string()], "test_assertion").unwrap(),
        format!("[bug] Test expectation in `cargo test`: {plain}")
    );
}
