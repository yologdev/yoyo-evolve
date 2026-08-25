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
    existing_config_paths_in(std::path::Path::new("."))
}

/// Directory-taking seam for [`existing_config_paths`]: `project_dir` is where the
/// project rung (`.yoyo.toml`) is looked for, instead of the process cwd.
///
/// This exists because the returned chain is compared **by path equality** against
/// a file someone just wrote (`shadowing_config_file` / `demoted_config_file`), and
/// the setup wizard writes `project_dir.join(".yoyo.toml")` — an absolute path in
/// production. A chain whose project rung is the bare relative `.yoyo.toml` can
/// never compare equal to it, so the guard would be structurally incapable of
/// firing. Callers that know their project dir pass it; everyone else keeps the
/// cwd-relative form, byte-identical to before (`Path::new(".").join(".yoyo.toml")`
/// is `./.yoyo.toml`, which is why the wrapper special-cases the bare name).
///
/// There is exactly ONE statement of the precedence ladder — this function. A
/// second copy would drift from `config::load_config_file`, which is the whole
/// property these helpers exist to mirror.
pub(crate) fn existing_config_paths_in(project_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    // Project-level: <project_dir>/.yoyo.toml
    let project = if project_dir == std::path::Path::new(".") {
        std::path::PathBuf::from(".yoyo.toml")
    } else {
        project_dir.join(".yoyo.toml")
    };
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

// === Persisted per-directory workspace trust (Day 178, #749 item 1) ===
//
// The four project-config trust gates (`gate_mcp_sources`, `gate_project_permissions`,
// `gate_project_hooks`, `gate_goal_verify`) all refuse by default and name
// `--trust-project` as the escape hatch — which applies to **one run**. A user who
// genuinely trusts a checkout has to retype it on every invocation, and that is exactly
// the pressure that turns into "always pass the flag", i.e. back to the unsafe default by
// habit. This is the remembered half: `--trust-project-always` records the directory in a
// user-level store, and later runs in that same directory are trusted without a flag.
//
// It lives in this module because "which persisted **user-level** file governs this
// directory?" is the question this file already answers for the config ladder — the trust
// store is that same question one surface over.

/// The name of the trust store inside the user's XDG config directory.
const TRUSTED_DIRS_FILE: &str = "trusted_dirs";

/// Whether `content` already lists `dir` as a trusted directory.
///
/// Pure. `content` is the whole store file body; `dir` is compared against each
/// **non-blank, non-comment** line, trimmed, by **exact string equality**.
///
/// Exact match only, and that is the security design rather than a simplification:
/// a prefix/subdirectory match would silently widen a security control across
/// directories the user never saw — trusting `/home/me/proj` must not trust
/// `/home/me/proj/vendor/evil` (a path a dependency can create) and must not trust
/// `/home/me/proj-other` (a different checkout entirely).
pub(crate) fn trusted_dirs_contains(content: &str, dir: &std::path::Path) -> bool {
    let needle = dir.to_string_lossy();
    content.lines().any(|line| {
        let line = line.trim();
        !line.is_empty() && !line.starts_with('#') && line == needle
    })
}

/// The store body with `dir` appended on its own line.
///
/// Pure and **idempotent**: a directory already listed returns `content` unchanged,
/// so re-running `--trust-project-always` never grows a duplicate line. The result
/// always ends in exactly one trailing newline.
pub(crate) fn append_trusted_dir(content: &str, dir: &std::path::Path) -> String {
    if trusted_dirs_contains(content, dir) {
        return content.to_string();
    }
    let mut out = content.trim_end_matches('\n').to_string();
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&dir.to_string_lossy());
    out.push('\n');
    out
}

/// Where the trust store lives: `$XDG_CONFIG_HOME/yoyo/trusted_dirs`, else
/// `~/.config/yoyo/trusted_dirs`.
///
/// Derived from [`crate::cli::user_config_path`] (the XDG rung of the config ladder)
/// rather than resolving XDG a second time — one statement of that rule, never two to
/// drift apart.
///
/// **The store is user-level and never project-level, on purpose: a repo must never be
/// able to trust itself.** There is deliberately no `./.yoyo/trusted_dirs` lookup — that
/// would let the very file whose provenance is in question grant its own trust, which is
/// the provenance question `crate::config::config_path_is_project_local` already answers
/// for configs.
pub(crate) fn trusted_dirs_path() -> Option<std::path::PathBuf> {
    let config = crate::cli::user_config_path()?;
    Some(config.with_file_name(TRUSTED_DIRS_FILE))
}

/// Canonicalize a directory for storage/lookup.
///
/// `.`, `..` and a symlinked checkout must not present as a different string than the
/// one that was trusted. **Failure returns `None` rather than falling back to the raw
/// path**: "could not check" must not read as "trusted".
fn canonical_dir(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    std::fs::canonicalize(dir).ok()
}

/// Whether a previous `--trust-project-always` recorded this exact directory.
///
/// Missing store, unreadable store, or a directory that cannot be canonicalized all
/// return `false` — every failure mode lands on "not trusted".
pub(crate) fn dir_is_trusted(dir: &std::path::Path) -> bool {
    let Some(dir) = canonical_dir(dir) else {
        return false;
    };
    let Some(path) = trusted_dirs_path() else {
        return false;
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return false;
    };
    trusted_dirs_contains(&content, &dir)
}

/// Record `dir` in the user-level trust store, creating it if needed.
///
/// Read-modify-write through the idempotent [`append_trusted_dir`], so repeated calls
/// leave one line. Returns the path written on success. A directory that cannot be
/// canonicalized is **refused** rather than stored raw.
pub(crate) fn remember_trusted_dir(dir: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let dir = canonical_dir(dir).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "could not resolve the directory to a real path",
        )
    })?;
    let path = trusted_dirs_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "could not locate a user config directory",
        )
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    std::fs::write(&path, append_trusted_dir(&existing, &dir))?;
    Ok(path)
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

    // === Persisted per-directory workspace trust (Day 178, #749 item 1) ===
    //
    // Driven through the two pure functions with fabricated content: no HOME /
    // XDG_CONFIG_HOME mutation (those are process-globals — see
    // tests/global_state_races.rs) and no set_current_dir (#780 spent two tasks
    // removing those).

    fn p(s: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(s)
    }

    #[test]
    fn test_trusted_dirs_contains_exact_match_only() {
        // The table is the security design. Each row is (store body, queried dir,
        // expected), and the rows that must be *false* are the point: a prefix or
        // subdirectory match would silently widen a security control across
        // directories the user never saw.
        let store = "/home/me/proj\n";
        let cases: &[(&str, &str, bool)] = &[
            // The exact directory that was trusted.
            (store, "/home/me/proj", true),
            // A subdirectory — a path a dependency can create inside the repo.
            (store, "/home/me/proj/vendor/evil", false),
            // A sibling sharing the prefix — a different checkout entirely.
            (store, "/home/me/proj-other", false),
            // A parent — trusting a child must never trust the tree above it.
            (store, "/home/me", false),
            // Nothing is trusted by an empty or missing store.
            ("", "/home/me/proj", false),
            ("\n\n  \n", "/home/me/proj", false),
        ];
        for (content, dir, expected) in cases {
            assert_eq!(
                trusted_dirs_contains(content, &p(dir)),
                *expected,
                "trusted_dirs_contains({content:?}, {dir:?}) should be {expected}"
            );
        }
    }

    #[test]
    fn test_trusted_dirs_contains_ignores_blank_and_comment_lines() {
        let content = "# yoyo trusted directories\n\n  /home/me/proj  \n\n#/home/me/commented\n";
        // A line with surrounding whitespace still matches — trimmed, then compared.
        assert!(trusted_dirs_contains(content, &p("/home/me/proj")));
        // A commented-out entry grants nothing. This is the revocation path: a user
        // can comment a line instead of deleting it and the trust must really be gone.
        assert!(!trusted_dirs_contains(content, &p("/home/me/commented")));
        // The comment marker itself is not a directory.
        assert!(!trusted_dirs_contains(
            content,
            &p("# yoyo trusted directories")
        ));
    }

    #[test]
    fn test_append_trusted_dir_is_idempotent_and_newline_terminated() {
        // Empty store: one line, one trailing newline.
        let first = append_trusted_dir("", &p("/home/me/proj"));
        assert_eq!(first, "/home/me/proj\n");

        // Re-running --trust-project-always must not grow a duplicate line.
        let again = append_trusted_dir(&first, &p("/home/me/proj"));
        assert_eq!(
            again, first,
            "appending an already-trusted directory must return the body unchanged"
        );

        // A second, different directory is appended beneath the first.
        let two = append_trusted_dir(&first, &p("/home/me/other"));
        assert_eq!(two, "/home/me/proj\n/home/me/other\n");
        assert!(trusted_dirs_contains(&two, &p("/home/me/proj")));
        assert!(trusted_dirs_contains(&two, &p("/home/me/other")));

        // A store that lost its trailing newline is repaired rather than concatenated
        // onto — exactly one trailing newline, and no glued-together line.
        let unterminated = append_trusted_dir("/home/me/proj", &p("/home/me/other"));
        assert_eq!(unterminated, "/home/me/proj\n/home/me/other\n");

        // Comments and blank lines in the file are preserved by the append.
        let with_comment = append_trusted_dir("# mine\n\n", &p("/home/me/proj"));
        assert_eq!(with_comment, "# mine\n/home/me/proj\n");
    }

    #[test]
    fn test_trusted_dirs_path_is_user_level_never_project_level() {
        // A repo must never be able to trust itself: the store sits beside the XDG
        // user config, never at a project-relative path. If no user config dir can be
        // located we get None — "could not check" must not become "trusted".
        let Some(path) = trusted_dirs_path() else {
            return;
        };
        assert!(
            path.is_absolute(),
            "the trust store must be an absolute user-level path, got {}",
            path.display()
        );
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some(TRUSTED_DIRS_FILE)
        );
        // It shares a directory with the XDG config rung — one XDG resolver, not two.
        assert_eq!(
            path.parent(),
            crate::cli::user_config_path()
                .as_deref()
                .and_then(|p| p.parent()),
            "the trust store must live beside the user config, not in a second location"
        );
    }

    #[test]
    fn test_dir_is_trusted_refuses_a_directory_that_cannot_be_canonicalized() {
        // The default path for every user with no store: not trusted. A path that
        // does not exist cannot be canonicalized, and the failure lands on `false`
        // rather than falling back to the raw string.
        assert!(!dir_is_trusted(&p(
            "/nonexistent-yoyo-trust-probe/definitely/not/here"
        )));
    }
}
