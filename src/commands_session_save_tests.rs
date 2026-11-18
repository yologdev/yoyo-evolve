//! `/save [path] [--force]` (#1211): the parser, the overwrite verdict
//! (including "this is the file this process saved or loaded"), the real
//! write path in a tempdir, and the refusal's remedy round trip.

use super::*;

fn want(path: &str, force: bool) -> SaveArgs {
    SaveArgs {
        path: path.to_string(),
        force,
    }
}

#[test]
fn save_args_force_anywhere_is_a_flag_not_part_of_the_path() {
    // One row per input: where each word lands (path vs flag).
    let rows = [
        ("/save", want("yoyo-session.json", false)),
        ("/save --force", want("yoyo-session.json", true)),
        ("/save a.md", want("a.md", false)),
        ("/save a.md --force", want("a.md", true)),
        ("/save --force a.md", want("a.md", true)),
        ("/save out/a b.json", want("out/a b.json", false)),
        ("/save out/a b.json --force", want("out/a b.json", true)),
    ];
    for (input, expected) in rows {
        assert_eq!(parse_save_args(input).unwrap(), expected, "{input}");
    }
}

#[test]
fn save_args_refuse_a_mistyped_flag_instead_of_naming_a_file_after_it() {
    let err = parse_save_args("/save s.json --forse").unwrap_err();
    assert_eq!(
        err,
        "unknown flag '--forse' for /save (the only flag is --force). Nothing was written."
    );
}

#[test]
fn save_verdict_table() {
    use ExportVerdict::*;
    // (exists, is_dir, force, own_file) -> verdict
    let rows = [
        ((false, false, false, false), Write),
        ((true, false, false, false), RefuseExists),
        ((true, false, true, false), Write),
        ((true, false, false, true), Write),
        ((true, true, false, false), RefuseDirectory),
        ((true, true, true, false), RefuseDirectory),
        ((true, true, true, true), RefuseDirectory),
    ];
    for ((exists, is_dir, force, own), expected) in rows {
        assert_eq!(
            save_target_verdict(exists, is_dir, force, own),
            expected,
            "exists={exists} is_dir={is_dir} force={force} own={own}"
        );
    }
}

#[test]
fn save_refuses_to_replace_a_file_it_did_not_write() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.md");
    std::fs::write(&file, "hi").unwrap();
    let path = file.to_str().unwrap();
    let err = write_save(path, "[]", false, None).unwrap_err();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hi");
    assert_eq!(
        err,
        format!(
            "{path} already exists. Nothing was written. To replace it: /save {path} --force (or choose another file name)"
        )
    );
}

#[test]
fn save_refuses_another_sessions_file_when_this_process_saved_elsewhere() {
    let dir = tempfile::tempdir().unwrap();
    let mine = dir.path().join("mine.json");
    let theirs = dir.path().join("theirs.json");
    std::fs::write(&mine, "[]").unwrap();
    std::fs::write(&theirs, "[\"other session\"]").unwrap();
    let own = mine.canonicalize().unwrap();
    assert!(write_save(theirs.to_str().unwrap(), "[]", false, Some(&own)).is_err());
    assert_eq!(
        std::fs::read_to_string(&theirs).unwrap(),
        "[\"other session\"]"
    );
}

#[test]
fn save_replaces_with_force() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.md");
    std::fs::write(&file, "hi").unwrap();
    write_save(file.to_str().unwrap(), "[]", true, None).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "[]");
}

#[test]
fn saving_your_own_file_again_needs_no_force() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.json");
    let path = file.to_str().unwrap();
    write_save(path, "[1]", false, None).unwrap();
    // What handle_save records after a successful write.
    let own = file.canonicalize().unwrap();
    write_save(path, "[1,2]", false, Some(&own)).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "[1,2]");
}

#[test]
fn own_file_matches_through_a_different_spelling_of_the_same_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let file = dir.path().join("s.json");
    std::fs::write(&file, "[]").unwrap();
    let own = file.canonicalize().unwrap();
    let other_spelling = dir.path().join("sub").join("..").join("s.json");
    assert!(is_own_session_file(&other_spelling, Some(&own)));
    assert!(!is_own_session_file(&file, None));
    assert!(!is_own_session_file(
        &dir.path().join("nope.json"),
        Some(&own)
    ));
}

/// `/load x` then `/save x`: the file the process loaded is its own file.
#[test]
fn load_then_save_the_same_file_needs_no_force() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.json");
    std::fs::write(&file, "[\"loaded\"]").unwrap();
    // handle_load records this canonical path on success.
    let own = file.canonicalize().unwrap();
    write_save(file.to_str().unwrap(), "[]", false, Some(&own)).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "[]");
}

#[test]
fn save_writes_a_new_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("new.json");
    write_save(file.to_str().unwrap(), "[]", false, None).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "[]");
}

#[cfg(unix)]
#[test]
fn save_does_not_write_through_a_dangling_symlink_without_force() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("s.json");
    let target = dir.path().join("elsewhere.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(write_save(link.to_str().unwrap(), "[]", false, None).is_err());
    assert!(!target.exists());
}

#[test]
fn save_refuses_a_directory_and_does_not_offer_force() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_str().unwrap();
    for force in [false, true] {
        let err = write_save(path, "[]", force, None).unwrap_err();
        assert!(err.starts_with(&format!("{path} is a directory.")), "{err}");
        assert!(!err.contains("--force"), "{err}");
        assert!(err.contains("/save "), "{err}");
    }
}

/// The remedy the refusal prints, fed back through the parser, must name the
/// same file with force set, for the default path and for typed paths.
#[test]
fn save_refusal_remedy_round_trips_through_the_parser() {
    for input in ["/save", "/save a.md", "/save out/a b.json"] {
        let args = parse_save_args(input).unwrap();
        let msg = save_refusal_message(&args.path, ExportVerdict::RefuseExists);
        let remedy = msg
            .split("To replace it: ")
            .nth(1)
            .and_then(|r| r.split(" (or choose").next())
            .unwrap();
        assert_eq!(remedy, format!("/save {} --force", args.path));
        assert_eq!(
            parse_save_args(remedy).unwrap(),
            want(&args.path, true),
            "{input}"
        );
    }
}
