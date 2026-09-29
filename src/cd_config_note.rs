//! `/cd` disclosure: name the new directory's config that is NOT applied (#869).
//!
//! After `/cd`, the launch directory's `[permissions]`, `[directories]`, hooks and
//! MCP servers stay in force and the new directory's `.yoyo.toml` is never read.
//! Reloading is the real fix and is still open (write-once `OnceLock`, and a naive
//! reload could *widen* the fence the user lives inside). This module is only the
//! disclosure half: it READS the new directory's file and says, specifically,
//! which safety-relevant sections it sets that this session is ignoring.
//!
//! **Disclosure is not a control.** Nothing here applies, merges or executes any
//! value from the file, and nothing here touches trust, permissions or globals.
//!
//! The file is read through yoyo's own config parsers — the same doors startup
//! uses — so "does this file set X" has exactly one statement per section and
//! cannot disagree with what a restart in that directory would actually load.

use std::path::Path;

/// What reading the new directory's `.yoyo.toml` produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CdConfigReading {
    /// No `.yoyo.toml` there — the common case; prints nothing.
    Absent,
    /// The file exists but could not be read (permissions, not UTF-8). This is
    /// deliberately distinct from "sets nothing": we cannot tell what it sets.
    Unreadable,
    /// The file was read; these sections are set (possibly none).
    Sets(Vec<&'static str>),
}

/// Section labels, in the fixed order they are reported.
const PERMISSIONS: &str = "[permissions]";
const DIRECTORIES: &str = "[directories]";
const HOOKS: &str = "hooks";
const MCP: &str = "MCP servers";

/// Which safety-relevant sections `toml_text` sets, in a fixed order:
/// `[permissions]` (allow/deny), `[directories]` (allow/deny), `hooks.<phase>.<tool>`
/// keys, and MCP servers (`mcp = [...]` or `[mcp_servers.*]`).
///
/// Uses the startup parsers rather than a second grammar. The hook check mirrors
/// `parse_hooks_from_config`'s key shape without calling it, because that function
/// prints unknown-phase warnings — about a file this session is not loading.
pub(crate) fn unapplied_config_sections(toml_text: &str) -> Vec<&'static str> {
    let mut out = Vec::new();

    let perms = crate::config::parse_permissions_from_config(toml_text);
    if !perms.allow.is_empty() || !perms.deny.is_empty() {
        out.push(PERMISSIONS);
    }

    let dirs = crate::config::parse_directories_from_config(toml_text);
    if !dirs.allow.is_empty() || !dirs.deny.is_empty() {
        out.push(DIRECTORIES);
    }

    let flat = crate::config::parse_config_file(toml_text);
    let has_hook = flat.iter().any(|(k, v)| {
        k.strip_prefix("hooks.")
            .and_then(|rest| rest.split_once('.'))
            .is_some_and(|(phase, tool)| {
                crate::hooks::HookPhase::parse(phase).is_some()
                    && !tool.is_empty()
                    && !v.trim().is_empty()
            })
    });
    if has_hook {
        out.push(HOOKS);
    }

    let flat_mcp = flat
        .get("mcp")
        .map(|raw| crate::config::parse_toml_array(raw))
        .unwrap_or_default();
    if !flat_mcp.is_empty() || !crate::config::parse_mcp_servers_from_config(toml_text).is_empty() {
        out.push(MCP);
    }

    out
}

/// Classify the result of reading `<dir>/.yoyo.toml`. `NotFound` is `Absent`;
/// any other error is `Unreadable` (never silently "sets nothing").
pub(crate) fn classify_read(read: std::io::Result<String>) -> CdConfigReading {
    match read {
        Ok(text) => CdConfigReading::Sets(unapplied_config_sections(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => CdConfigReading::Absent,
        Err(_) => CdConfigReading::Unreadable,
    }
}

/// The one-line note, or `None` when there is nothing to disclose — no file, or
/// a file setting none of the four sections. `None` is the regression surface:
/// every `/cd` into a directory without such config prints exactly what it did before.
/// Glyph-free when `plain`. The path is repository-controlled, so it is sanitized.
pub(crate) fn cd_config_note(
    new_dir_display: &str,
    reading: &CdConfigReading,
    plain: bool,
) -> Option<String> {
    let dir = crate::cli::sanitize_for_display(new_dir_display);
    let sep = if plain { ":" } else { " —" };
    match reading {
        CdConfigReading::Absent => None,
        CdConfigReading::Sets(sections) if sections.is_empty() => None,
        CdConfigReading::Sets(sections) => Some(format!(
            "  {dir}/.yoyo.toml sets {} and they are NOT applied in this session{sep} \
             the launch directory's settings stay in force. Restart yoyo here to use them (#869).",
            sections.join(", ")
        )),
        CdConfigReading::Unreadable => Some(format!(
            "  {dir}/.yoyo.toml exists but could not be read, and NONE of it is applied in this \
             session{sep} the launch directory's settings stay in force. Restart yoyo here to use it (#869)."
        )),
    }
}

/// Read `<target>/.yoyo.toml` and print the note to stderr in DIM. Read-only.
pub(crate) fn print_cd_config_note(target: &Path) {
    let reading = classify_read(std::fs::read_to_string(target.join(".yoyo.toml")));
    let note = cd_config_note(
        &target.display().to_string(),
        &reading,
        crate::format::is_plain_output(),
    );
    if let Some(note) = note {
        eprintln!("{}{note}{}", crate::format::DIM, crate::format::RESET);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERMS_TOML: &str = "model = \"x\"\n\n[permissions]\ndeny = [\"rm -rf *\"]\n";

    fn sets(text: &str) -> CdConfigReading {
        classify_read(Ok(text.to_string()))
    }

    #[test]
    fn permissions_deny_is_named() {
        // Anti-vacuous: the fixture really carries a deny entry.
        assert!(PERMS_TOML.contains("deny"));
        assert_eq!(unapplied_config_sections(PERMS_TOML), vec!["[permissions]"]);
        let note = cd_config_note("/repo", &sets(PERMS_TOML), false).expect("note");
        assert!(note.contains("[permissions]"), "{note}");
        assert!(note.contains("NOT applied"), "{note}");
        assert!(note.contains("/repo/.yoyo.toml"), "{note}");
        assert!(note.contains("#869"), "{note}");
    }

    #[test]
    fn model_only_config_sets_nothing_and_prints_nothing() {
        let text = "provider = \"anthropic\"\nmodel = \"x\"\n";
        assert_eq!(unapplied_config_sections(text), Vec::<&str>::new());
        assert_eq!(sets(text), CdConfigReading::Sets(vec![]));
        assert_eq!(cd_config_note("/repo", &sets(text), false), None);
        assert_eq!(cd_config_note("/repo", &sets(text), true), None);
    }

    #[test]
    fn absent_file_prints_nothing() {
        let absent = classify_read(Err(std::io::Error::from(std::io::ErrorKind::NotFound)));
        assert_eq!(absent, CdConfigReading::Absent);
        assert_eq!(cd_config_note("/repo", &absent, false), None);
    }

    #[test]
    fn all_four_sections_in_fixed_order() {
        let text = "mcp = [\"npx server\"]\n\
                    hooks.pre.bash = \"echo hi\"\n\
                    [directories]\ndeny = [\"secrets\"]\n\
                    [permissions]\nallow = [\"git *\"]\n";
        assert_eq!(
            unapplied_config_sections(text),
            vec!["[permissions]", "[directories]", "hooks", "MCP servers"]
        );
        let note = cd_config_note("/r", &sets(text), false).unwrap();
        assert!(
            note.contains("sets [permissions], [directories], hooks, MCP servers and"),
            "{note}"
        );
    }

    #[test]
    fn structured_mcp_servers_section_counts() {
        let text = "[mcp_servers.fs]\ncommand = \"npx\"\n";
        assert_eq!(unapplied_config_sections(text), vec!["MCP servers"]);
    }

    #[test]
    fn unknown_hook_phase_is_not_a_hook() {
        // Near-miss: a key with the hooks prefix that startup would refuse.
        let text = "hooks.prr.bash = \"echo\"\n";
        assert_eq!(unapplied_config_sections(text), Vec::<&str>::new());
    }

    #[test]
    fn unreadable_is_distinct_from_empty() {
        let bad = classify_read(Err(std::io::Error::from(
            std::io::ErrorKind::PermissionDenied,
        )));
        assert_eq!(bad, CdConfigReading::Unreadable);
        assert_ne!(bad, CdConfigReading::Sets(vec![]));
        let note = cd_config_note("/repo", &bad, false).expect("unreadable must be reported");
        assert!(note.contains("could not be read"), "{note}");
    }

    #[test]
    fn non_utf8_file_on_disk_reads_as_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".yoyo.toml");
        std::fs::write(&path, [0xff, 0xfe, 0x00, 0x80]).unwrap();
        assert_eq!(
            classify_read(std::fs::read_to_string(&path)),
            CdConfigReading::Unreadable
        );
        // And a directory without the file is Absent.
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            classify_read(std::fs::read_to_string(empty.path().join(".yoyo.toml"))),
            CdConfigReading::Absent
        );
    }

    #[test]
    fn plain_output_is_glyph_free() {
        for reading in [sets(PERMS_TOML), CdConfigReading::Unreadable] {
            let note = cd_config_note("/repo", &reading, true).unwrap();
            assert!(!note.contains('—'), "{note}");
            assert!(!note.contains('•'), "{note}");
        }
    }

    #[test]
    fn hostile_path_is_sanitized() {
        let hostile = "/repo\x1b[2J";
        assert!(hostile.as_bytes().contains(&0x1b)); // anti-vacuous
        let note = cd_config_note(hostile, &sets(PERMS_TOML), false).unwrap();
        assert!(!note.as_bytes().contains(&0x1b), "{note:?}");
    }
}
