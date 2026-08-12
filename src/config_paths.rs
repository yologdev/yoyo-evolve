//! Config-file precedence helpers.
//!
//! yoyo loads exactly ONE config file — the first that exists in precedence
//! order `./.yoyo.toml` → `~/.yoyo.toml` → `~/.config/yoyo/config.toml` — and
//! never merges them (`config::load_config_file`). Every consequence of that
//! rule that a user needs told about lives here, as pure functions over path
//! lists so they can be tested without touching a real home directory.
//!
//! Extracted from `commands_config.rs` on Day 165 (#735): these are path
//! predicates, not command handling, and the host module was one edit away
//! from the module-size cap.

/// Detect which on-disk config file (if any) would be loaded by
/// `cli::load_config_file()`, using the same precedence order:
/// 1. `./.yoyo.toml` (project-level)
/// 2. `~/.yoyo.toml` (home shorthand)
/// 3. `~/.config/yoyo/config.toml` (XDG user-level)
///
/// Returns the path to the first file that exists, or `None` if no
/// config file is present in any location. This is a read-only
/// introspection helper — it never reads or parses the file itself,
/// it just tells you which path would be chosen.
///
/// Kept as a separate function (rather than calling `load_config_file`
/// directly) because the existing loader is private to `cli.rs` and
/// this path-only view is all `/config show` needs. The loader path
/// and this one are unit-tested together indirectly via
/// `test_config_file_path_precedence` below.
pub(crate) fn detect_loaded_config_path() -> Option<std::path::PathBuf> {
    existing_config_paths().into_iter().next()
}

/// Every config file that exists on disk, highest precedence first.
///
/// Mirrors `config::load_config_file`, which loads exactly ONE file — the
/// first that exists — and does **not** merge them:
/// `./.yoyo.toml` → `~/.yoyo.toml` → `~/.config/yoyo/config.toml`.
pub(crate) fn existing_config_paths() -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    // Project-level: ./.yoyo.toml
    let project = std::path::PathBuf::from(".yoyo.toml");
    if project.exists() {
        found.push(project);
    }
    // Home shorthand: ~/.yoyo.toml
    if let Some(path) = crate::cli::home_config_path() {
        if path.exists() {
            found.push(path);
        }
    }
    // XDG user-level: ~/.config/yoyo/config.toml
    if let Some(path) = crate::cli::user_config_path() {
        if path.exists() {
            found.push(path);
        }
    }
    found
}

/// Decide whether a config file that was just written will actually be read.
///
/// Because loading is first-existing-file-wins (never a merge), writing to a
/// lower-precedence file while a higher-precedence one exists is a write that
/// nothing will ever read — the whole file is shadowed, not just the key.
///
/// `existing` must be the config files that exist on disk, highest precedence
/// first (see [`existing_config_paths`]). Returns the path that shadows
/// `written`, or `None` when `written` is itself the highest-precedence
/// existing file.
///
/// If `written` is not part of the precedence chain at all we return `None` and
/// make **no claim** — that's an explicit "unknown", not a quiet vote either
/// way.
pub(crate) fn shadowing_config_file(
    written: &std::path::Path,
    existing: &[std::path::PathBuf],
) -> Option<std::path::PathBuf> {
    let position = existing.iter().position(|p| p == written)?;
    if position == 0 {
        None
    } else {
        existing.first().cloned()
    }
}

/// The honest note printed when a `/config set` write landed in a file that a
/// higher-precedence config file shadows.
pub(crate) fn shadowed_write_warning(
    written: &std::path::Path,
    shadow: &std::path::Path,
) -> String {
    format!(
        "⚠ {} is not the config yoyo loads here — {} takes precedence and is read instead, \
so this value will not take effect in this directory (it applies wherever {} is absent).",
        written.display(),
        shadow.display(),
        shadow.display()
    )
}

/// The sibling of [`shadowing_config_file`], looking the *other* way down the
/// precedence chain (#735).
///
/// `shadowing_config_file` asks "will the file I just wrote be ignored?" — it
/// answers `None` at position 0 and so structurally cannot see the inverse
/// case: a write that *creates* a new highest-precedence file silently demotes
/// whatever config yoyo was reading before, because loading is
/// first-existing-wins with no merge. Every other setting in the demoted file
/// stops being read, and the green checkmark says nothing about it.
///
/// `before` must be the precedence-ordered existing files captured **before**
/// the write, and `after` the same list recomputed afterwards. The pre-write
/// capture is load-bearing: recomputing both sides after the write is exactly
/// what makes the shadow guard blind here, since the new file is already in
/// the list by then.
///
/// Returns the file that yoyo was loading and no longer loads, or `None` when
/// nothing went dark: the write did not become the head of the chain, there was
/// no config file before, or the file written is the same one already in use.
pub(crate) fn demoted_config_file(
    written: &std::path::Path,
    before: &[std::path::PathBuf],
    after: &[std::path::PathBuf],
) -> Option<std::path::PathBuf> {
    if after.first().map(|p| p.as_path()) != Some(written) {
        return None;
    }
    let previous = before.first()?;
    (previous.as_path() != written).then(|| previous.clone())
}

/// The honest note printed when a `/config set` write created a file that now
/// outranks the config yoyo had been reading.
pub(crate) fn demoted_write_warning(
    written: &std::path::Path,
    demoted: &std::path::Path,
) -> String {
    format!(
        "⚠ {} now takes precedence over {}, and yoyo loads exactly one config file (no merging) \
— so every other setting in {} has stopped being read here.",
        written.display(),
        demoted.display(),
        demoted.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // === /config set --global shadowing (Day 151) ===
    //
    // yoyo loads exactly ONE config file (first existing in precedence order,
    // no merging). Writing to a lower-precedence file while a higher-precedence
    // one exists is a write nothing will ever read — so the unconditional
    // "✓ Set k = v in <path>" confirmation asserted the container (the write
    // landed) and not the payload (the setting will be honoured).

    #[test]
    fn test_shadowing_none_when_written_is_highest_precedence() {
        let project = std::path::PathBuf::from(".yoyo.toml");
        let home = std::path::PathBuf::from("/home/u/.yoyo.toml");
        let existing = vec![project.clone(), home];
        assert_eq!(shadowing_config_file(&project, &existing), None);
    }

    #[test]
    fn test_shadowing_detects_project_file_over_global_write() {
        let project = std::path::PathBuf::from(".yoyo.toml");
        let home = std::path::PathBuf::from("/home/u/.yoyo.toml");
        let existing = vec![project.clone(), home.clone()];
        assert_eq!(
            shadowing_config_file(&home, &existing),
            Some(project),
            "a --global write must be reported as shadowed by an existing project config"
        );
    }

    #[test]
    fn test_shadowing_none_for_sole_existing_file() {
        let home = std::path::PathBuf::from("/home/u/.yoyo.toml");
        let existing = vec![home.clone()];
        assert_eq!(shadowing_config_file(&home, &existing), None);
    }

    #[test]
    fn test_shadowing_makes_no_claim_for_path_outside_precedence_chain() {
        // Explicit third value (Day 144): "not part of the chain" is unknown,
        // not "shadowed" and not silently "fine by omission" — we say nothing
        // rather than let the convenient neighbour absorb it.
        let project = std::path::PathBuf::from(".yoyo.toml");
        let elsewhere = std::path::PathBuf::from("/tmp/some-other.toml");
        let existing = vec![project];
        assert_eq!(shadowing_config_file(&elsewhere, &existing), None);
    }

    #[test]
    fn test_shadowed_write_warning_names_both_files() {
        let msg = shadowed_write_warning(
            &std::path::PathBuf::from("/home/u/.yoyo.toml"),
            &std::path::PathBuf::from(".yoyo.toml"),
        );
        assert!(
            msg.contains("/home/u/.yoyo.toml"),
            "warning must name the file written: {msg}"
        );
        assert!(
            msg.contains(".yoyo.toml"),
            "warning must name the shadowing file: {msg}"
        );
        // It must say the write will not take effect — the payload claim.
        let lower = msg.to_ascii_lowercase();
        assert!(
            lower.contains("not") || lower.contains("won't") || lower.contains("override"),
            "warning must state the write is not in effect: {msg}"
        );
    }

    #[test]
    fn test_existing_config_paths_are_in_precedence_order() {
        // Whatever exists on this machine, the order must mirror
        // config::load_config_file's first-wins chain.
        let paths = existing_config_paths();
        let project = std::path::PathBuf::from(".yoyo.toml");
        if paths.len() > 1 && paths.contains(&project) {
            assert_eq!(
                paths[0], project,
                "project-level .yoyo.toml must come first in precedence order"
            );
        }
        // detect_loaded_config_path must be exactly the head of that list.
        assert_eq!(detect_loaded_config_path(), paths.first().cloned());
    }

    // === /config set --global demotion (Day 165, #735) ===
    //
    // The inverse of the block above: the write lands and is honoured, but
    // creating a new highest-precedence file makes every other setting in the
    // previously-loaded file stop being read. Same underlying fact (one file,
    // first-existing-wins, no merge) seen from the other end.

    #[test]
    fn test_demotion_detects_xdg_config_going_dark_on_global_write() {
        // Exactly the #735 report: ~/.config/yoyo/config.toml holds the
        // settings, `/config set ... --global` creates ~/.yoyo.toml.
        let xdg = std::path::PathBuf::from("/home/u/.config/yoyo/config.toml");
        let home = std::path::PathBuf::from("/home/u/.yoyo.toml");
        let before = vec![xdg.clone()];
        let after = vec![home.clone(), xdg.clone()];
        assert_eq!(
            demoted_config_file(&home, &before, &after),
            Some(xdg),
            "a --global write that outranks an existing XDG config must name the demoted file"
        );
    }

    #[test]
    fn test_demotion_none_when_writing_the_file_already_loaded() {
        // Rewriting the file yoyo already reads changes nothing about which
        // file is read — no warning, or the guard becomes noise on every set.
        let home = std::path::PathBuf::from("/home/u/.yoyo.toml");
        let before = vec![home.clone()];
        let after = vec![home.clone()];
        assert_eq!(demoted_config_file(&home, &before, &after), None);
    }

    #[test]
    fn test_demotion_none_when_write_is_itself_shadowed() {
        // A --global write underneath an existing project config demotes
        // nothing (it is the shadowing guard's case, not this one) — the two
        // warnings must not both fire for one write.
        let project = std::path::PathBuf::from(".yoyo.toml");
        let home = std::path::PathBuf::from("/home/u/.yoyo.toml");
        let before = vec![project.clone()];
        let after = vec![project, home.clone()];
        assert_eq!(demoted_config_file(&home, &before, &after), None);
    }

    #[test]
    fn test_demotion_none_when_no_config_existed_before() {
        // First config file ever: nothing went dark. Absence is its own value
        // here (Day 144), not "demoted something unnamed".
        let home = std::path::PathBuf::from("/home/u/.yoyo.toml");
        assert_eq!(
            demoted_config_file(&home, &[], std::slice::from_ref(&home)),
            None
        );
    }

    #[test]
    fn test_demoted_write_warning_names_both_files_and_the_consequence() {
        let msg = demoted_write_warning(
            &std::path::PathBuf::from("/home/u/.yoyo.toml"),
            &std::path::PathBuf::from("/home/u/.config/yoyo/config.toml"),
        );
        assert!(
            msg.contains("/home/u/.yoyo.toml"),
            "warning must name the file written: {msg}"
        );
        assert!(
            msg.contains("/home/u/.config/yoyo/config.toml"),
            "warning must name the file that stopped being read: {msg}"
        );
        let lower = msg.to_ascii_lowercase();
        assert!(
            lower.contains("stopped being read") || lower.contains("no longer"),
            "warning must state the consequence, not just the precedence: {msg}"
        );
    }
}
