//! `/export [path] [--force]`: the parser, the overwrite verdict, the real
//! write path in a tempdir, and the refusal's remedy round trip.

use super::*;

#[test]
fn export_args_force_anywhere_is_a_flag_not_part_of_the_path() {
    let want = |path: &str, force| ExportArgs {
        path: path.to_string(),
        force,
    };
    // One row per input: where each word lands (path vs flag).
    let rows = [
        ("/export", want("conversation.md", false)),
        ("/export --force", want("conversation.md", true)),
        ("/export notes.md --force", want("notes.md", true)),
        ("/export --force notes.md", want("notes.md", true)),
        ("/export my  notes.md", want("my  notes.md", false)),
        ("/export my --force notes.md", want("my notes.md", true)),
        ("/export a b.md --force", want("a b.md", true)),
    ];
    for (input, expected) in rows {
        assert_eq!(parse_export_args(input).unwrap(), expected, "{input}");
    }
}

#[test]
fn export_args_refuse_a_mistyped_flag_instead_of_naming_a_file_after_it() {
    let err = parse_export_args("/export notes.md --forse").unwrap_err();
    assert!(err.contains("'--forse'"), "{err}");
    assert!(err.contains("Nothing was written"), "{err}");
}

#[test]
fn export_verdict_table() {
    use ExportVerdict::*;
    assert_eq!(export_target_verdict(false, false, false), Write);
    assert_eq!(export_target_verdict(false, false, true), Write);
    assert_eq!(export_target_verdict(true, false, false), RefuseExists);
    assert_eq!(export_target_verdict(true, false, true), Write);
    assert_eq!(export_target_verdict(true, true, false), RefuseDirectory);
    assert_eq!(export_target_verdict(true, true, true), RefuseDirectory);
}

#[test]
fn export_refuses_to_overwrite_an_existing_file_without_force() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.md");
    std::fs::write(&file, "precious").unwrap();
    let path = file.to_str().unwrap();
    let err = write_export(path, "# Conversation", false).unwrap_err();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "precious");
    assert_eq!(
        err,
        format!(
            "{path} already exists. Nothing was written. To replace it: /export {path} --force (or choose another file name)"
        )
    );
}

#[test]
fn export_overwrites_an_existing_file_with_force() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.md");
    std::fs::write(&file, "precious").unwrap();
    write_export(file.to_str().unwrap(), "# Conversation", true).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Conversation");
}

#[test]
fn export_writes_a_new_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("new.md");
    write_export(file.to_str().unwrap(), "# Conversation", false).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Conversation");
}

#[cfg(unix)]
#[test]
fn export_does_not_write_through_a_dangling_symlink_without_force() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("link.md");
    let target = dir.path().join("elsewhere.md");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let err = write_export(link.to_str().unwrap(), "x", false).unwrap_err();
    assert!(err.contains("already exists"), "{err}");
    assert!(!target.exists());
}

#[test]
fn export_refuses_a_directory_and_does_not_offer_force() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_str().unwrap();
    for force in [false, true] {
        let err = write_export(path, "x", force).unwrap_err();
        assert!(err.contains("is a directory"), "{err}");
        assert!(!err.contains("--force"), "{err}");
    }
}

/// The remedy the refusal prints, fed back through the parser, must name the
/// same file with force set, for the default path and for typed paths.
#[test]
fn export_refusal_remedy_round_trips_through_the_parser() {
    for input in ["/export", "/export notes.md", "/export out/a b.md"] {
        let args = parse_export_args(input).unwrap();
        let msg = export_refusal_message(&args.path, ExportVerdict::RefuseExists);
        let remedy = msg
            .split("To replace it: ")
            .nth(1)
            .and_then(|r| r.split(" (or choose").next())
            .unwrap();
        assert_eq!(remedy, format!("/export {} --force", args.path));
        assert_eq!(
            parse_export_args(remedy).unwrap(),
            ExportArgs {
                path: args.path.clone(),
                force: true
            },
            "{input}"
        );
    }
}
