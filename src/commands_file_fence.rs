//! The directory fence for files the user names into a prompt (#1002):
//! `@path` mentions, `/add`, `/explain` and `--image`. Kept apart from
//! `commands_file.rs` so the one check and its refusal wording live together.

use crate::format::{RED, RESET, YELLOW};

/// Why a file the user named (`/add`, `@path`, `/explain`, `--image`) was not
/// read. The two arms are rendered differently: a fence refusal is a yellow
/// warning (the fence working as configured), a failed read a red `✗` (#1002).
#[derive(Debug, Clone, PartialEq)]
pub enum AddReadError {
    /// The session's `--deny-dir`/`--allow-dir` fence refused the path at its
    /// resolved target. Carries the full, glyph-correct warning line.
    Refused(String),
    /// The read itself failed (missing file, permissions, bad range, ...).
    Failed(String),
}

impl std::fmt::Display for AddReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AddReadError::Refused(m) | AddReadError::Failed(m) => f.write_str(m),
        }
    }
}

/// The warning printed when the directory fence refuses a user-named file.
/// `reason` is `DirectoryRestrictions::check_path`'s own text, which already
/// names the path and the fence entry; both strings are user/config-authored,
/// so they are sanitized before reaching the terminal.
pub(crate) fn add_fence_refusal_message(path: &str, reason: &str, plain: bool) -> String {
    let path = crate::cli::sanitize_for_display(path);
    let reason = crate::cli::sanitize_for_display(reason);
    if plain {
        format!("warning: {path} not sent to the model - {reason}")
    } else {
        format!("⚠ {path} not sent to the model — {reason}")
    }
}

/// The one fence check for every door that inlines a user-named file into a
/// prompt (#1002): `/add`, `@path` mentions, `/explain` and `--image` all
/// read through `commands_file::read_file_for_add` / `read_image_for_add`, which call
/// this before touching the bytes. It asks `DirectoryRestrictions::check_path`
/// — the resolver the file tools and prompt loaders use — so a symlink is
/// judged at its target. With no restrictions it short-circuits to `Ok`.
pub(crate) fn admit_for_add(
    path: &str,
    restrictions: &crate::config::DirectoryRestrictions,
) -> Result<(), AddReadError> {
    restrictions.check_path(path).map_err(|reason| {
        AddReadError::Refused(add_fence_refusal_message(
            path,
            &reason,
            crate::format::is_plain_output(),
        ))
    })
}

/// The coloured line a door prints when a user-named file was not read: a
/// fence refusal is a yellow warning, a failed read keeps its red `✗`.
pub(crate) fn add_read_error_line(e: &AddReadError) -> String {
    match e {
        AddReadError::Refused(m) => format!("{YELLOW}  {m}{RESET}"),
        AddReadError::Failed(m) => format!("{RED}  ✗ {m}{RESET}"),
    }
}
