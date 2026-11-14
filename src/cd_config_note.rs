//! `/cd` disclosure: name the new directory's config that is NOT applied (#869),
//! and say that its `[permissions]` deny entries now ARE (Day 218, `cd_deny.rs`),
//! and its `[directories]` deny entries too (Day 226, file tools, below).
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
//! Day 218: one value IS applied, by the `/cd` arm in `dispatch.rs` rather than
//! here — the reading's `deny` entries, appended to `cd_deny`'s session list,
//! which both bash executors read at call time. Deny only narrows, so it needs
//! no trust check. This module reports it from the same reading `dispatch.rs`
//! appended, so the note cannot disagree with what was applied.
//! Day 226: the reading's `[directories] deny` entries are applied the same way,
//! resolved against the `/cd` target at append time and kept in this module's
//! append-only list, which both path guards in `tool_wrappers.rs` read at call
//! time (`path_guard_refusal`). Superseded (Day 228): this said `[directories]
//! allow` is NOT applied because intersecting it with the startup allow list was
//! a design call. The call is made: the new directory's allow list is now a
//! SECOND fence a path must also pass (startup allow AND /cd allow), checked
//! inside `cd_denied_dir_refusal_with`, so every reader already routed through
//! that helper gets it. Unlike deny it is REPLACED by each /cd (see
//! `apply_cd_dir_fence_with` for why).
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
    /// The file was read: the NOT-applied sections it sets (possibly none), its
    /// `[permissions] deny` entries, which `/cd` does apply (bash only), its
    /// `[directories] deny` entries as written, which `/cd` applies to the file
    /// tools after resolving them against the new directory (Day 226), and its
    /// `[directories] allow` entries as written, which `/cd` applies the same
    /// way as an extra narrowing fence (Day 228).
    Sets(Vec<&'static str>, Vec<String>, Vec<String>, Vec<String>),
}

/// Section labels, in the fixed order they are reported.
/// Only the widening halves: both deny halves (Days 218, 226) and the
/// `[directories] allow` half (Day 228) only narrow, so `/cd` applies them.
const PERMISSIONS: &str = "[permissions] allow";
const HOOKS: &str = "hooks";
const MCP: &str = "MCP servers";

/// Every section this module can name. All three decide what the session may do
/// (what bash may run, what runs around a tool call, which external tools
/// exist), so a note naming any of them is a WARNING, not chrome.
/// Read by `note_volume`; built from the constants above, never re-spelled.
const SAFETY_SECTIONS: [&str; 3] = [PERMISSIONS, HOOKS, MCP];

/// Which safety-relevant sections `toml_text` sets that `/cd` does NOT apply,
/// in a fixed order: `[permissions] allow`, `hooks.<phase>.<tool>` keys, and
/// MCP servers (`mcp = [...]` or `[mcp_servers.*]`).
///
/// Uses the startup parsers rather than a second grammar. The hook check mirrors
/// `parse_hooks_from_config`'s key shape without calling it, because that function
/// prints unknown-phase warnings — about a file this session is not loading.
pub(crate) fn unapplied_config_sections(toml_text: &str) -> Vec<&'static str> {
    let mut out = Vec::new();

    let perms = crate::config::parse_permissions_from_config(toml_text);
    if !perms.allow.is_empty() {
        out.push(PERMISSIONS);
    }

    // [directories] is not checked here: both of its halves only narrow and
    // are applied on /cd (deny Day 226, allow Day 228).

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
        Ok(text) => {
            let dirs = crate::config::parse_directories_from_config(&text);
            CdConfigReading::Sets(
                unapplied_config_sections(&text),
                crate::config::parse_permissions_from_config(&text).deny,
                dirs.deny,
                dirs.allow,
            )
        }
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
        CdConfigReading::Sets(sections, deny, dir_deny, dir_allow)
            if !deny.is_empty() || !dir_deny.is_empty() || !dir_allow.is_empty() =>
        {
            let mut applied = Vec::new();
            // Each applied list names the executor it fences and the one it
            // does not: neither list reaches the other kind of tool.
            for (n, what, one, many, scope, other) in [
                (deny.len(), "[permissions] deny", "pattern", "patterns", "bash commands", "file tools never read"),
                (dir_deny.len(), "[directories] deny", "entry", "entries", "the file tools", "bash never reads"),
            ] {
                let (noun, verb, them) = if n == 1 { (one, "is", "it") } else { (many, "are", "them") };
                if n > 0 {
                    applied.push(format!(
                        "its {n} {what} {noun} {verb} now enforced on {scope} for the rest of \
                         this session (another /cd does not remove {them}; {other} this list)"
                    ));
                }
            }
            if !dir_allow.is_empty() {
                let n = dir_allow.len();
                let (noun, them) = if n == 1 { ("entry", "it") } else { ("entries", "them") };
                applied.push(format!(
                    "its {n} [directories] allow {noun} now also fence the file tools: a path \
                     outside {them} is refused until the next /cd replaces this list (the launch \
                     allow list still applies too; bash never reads this list)"
                ));
            }
            let tail = if sections.is_empty() {
                "Its [permissions] allow, hooks and MCP servers would still not be reloaded on \
                 /cd (#1040)."
                    .to_string()
            } else {
                format!(
                    "It also sets {} and they are NOT applied{sep} the launch directory's \
                     settings stay in force, except that its own [permissions] allow and hooks pause outside its tree. Restart yoyo here to use them (#1040).",
                    sections.join(", ")
                )
            };
            Some(format!("  {dir}/.yoyo.toml: {}. {tail}", applied.join("; ")))
        }
        CdConfigReading::Sets(sections, _, _, _) if sections.is_empty() => None,
        CdConfigReading::Sets(sections, _, _, _) => Some(format!(
            "  {dir}/.yoyo.toml sets {} and they are NOT applied in this session{sep} \
             the launch directory's settings stay in force, except that its own [permissions] allow and hooks pause outside its tree. Restart yoyo here to use them (#1040).",
            sections.join(", ")
        )),
        CdConfigReading::Unreadable => Some(format!(
            "  {dir}/.yoyo.toml exists but could not be read, and NONE of it is applied in this \
             session{sep} the launch directory's settings stay in force, except that its own [permissions] allow and hooks pause outside its tree. Restart yoyo here to use it (#1040)."
        )),
    }
}

/// How loud the note is. Chosen from the note's own severity, never copied from
/// the DIM "project context is not reloaded" line printed just above it (#869,
/// Day 214): DIM reads as "skip this", and "your deny list is not in force"
/// is the one thing on that screen a user must not skip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoteVolume {
    /// Names at least one of `SAFETY_SECTIONS`: yellow, `warning:`-prefixed.
    Warning,
    /// Everything else: DIM, byte-identical to the Day-213 rendering.
    Chrome,
}

/// `Warning` iff the reading names a safety section. `Unreadable` names none —
/// we cannot tell what the file sets — so it stays `Chrome`, byte-identical to
/// before; raising it would warn on content we have not seen.
pub(crate) fn note_volume(reading: &CdConfigReading) -> NoteVolume {
    match reading {
        CdConfigReading::Sets(sections, deny, dir_deny, dir_allow)
            if !deny.is_empty()
                || !dir_deny.is_empty()
                || !dir_allow.is_empty()
                || sections.iter().any(|s| SAFETY_SECTIONS.contains(s)) =>
        {
            NoteVolume::Warning
        }
        _ => NoteVolume::Chrome,
    }
}

/// The exact bytes written to stderr for `note` at `volume`. Pure apart from
/// the colour constants' own NO_COLOR handling. Under `plain` a warning carries
/// no glyph and no escape bytes; chrome is rendered exactly as Day 213 did.
pub(crate) fn render_cd_config_note(note: &str, volume: NoteVolume, plain: bool) -> String {
    use crate::format::{DIM, RESET, YELLOW};
    match volume {
        NoteVolume::Chrome => format!("{DIM}{note}{RESET}"),
        NoteVolume::Warning if plain => format!("warning: {}", note.trim_start()),
        NoteVolume::Warning => format!("{YELLOW}⚠ warning: {}{RESET}", note.trim_start()),
    }
}

/// Read and classify `<target>/.yoyo.toml`. Read-only; one read per `/cd`, so
/// the deny entries applied and the note printed come from the same bytes.
pub(crate) fn read_cd_config(target: &Path) -> CdConfigReading {
    // Never `std::fs::read_to_string`: a FIFO there would hang `/cd` (#1047).
    classify_read(crate::non_regular_read::read_regular_to_string(
        &target.join(".yoyo.toml"),
    ))
}

/// The `[permissions] deny` entries `/cd` applies; empty unless the file was read.
pub(crate) fn deny_entries(reading: &CdConfigReading) -> &[String] {
    match reading {
        CdConfigReading::Sets(_, deny, _, _) => deny,
        _ => &[],
    }
}

/// The `[directories] deny` entries `/cd` applies, resolved against `target`
/// NOW: a relative `deny = ["secrets"]` means `<target>/secrets`, and must keep
/// meaning that after a later `/cd` moves the process cwd elsewhere. Absolute
/// and `~` entries are kept as written (`check_path` expands `~` itself).
pub(crate) fn dir_deny_entries(target: &Path, reading: &CdConfigReading) -> Vec<String> {
    let CdConfigReading::Sets(_, _, dir_deny, _) = reading else {
        return Vec::new();
    };
    resolve_against(target, dir_deny)
}

/// The `[directories] allow` entries `/cd` applies, resolved exactly like
/// [`dir_deny_entries`] (relative -> `<target>/<entry>`, absolute and `~` kept).
pub(crate) fn dir_allow_entries(target: &Path, reading: &CdConfigReading) -> Vec<String> {
    let CdConfigReading::Sets(_, _, _, dir_allow) = reading else {
        return Vec::new();
    };
    resolve_against(target, dir_allow)
}

/// Bind relative entries to `target` at `/cd` time; one rule for both halves.
fn resolve_against(target: &Path, entries: &[String]) -> Vec<String> {
    entries
        .iter()
        .map(|e| {
            if e.starts_with('~') || Path::new(e).is_absolute() {
                e.clone()
            } else {
                target.join(e).to_string_lossy().into_owned()
            }
        })
        .collect()
}

/// Directories denied by `/cd` (#869, Day 226): process-wide, append-only,
/// read at call time by both path guards in `tool_wrappers.rs`. Same shape and
/// seam as `cd_deny`'s bash list; tests drive a local handle, never this one.
static CD_DENIED_DIRS: std::sync::LazyLock<crate::cd_deny::DenyAdditions> =
    std::sync::LazyLock::new(Default::default);

/// The `[directories] allow` list of the most recent `/cd` (#869, Day 228),
/// read at call time inside [`cd_denied_dir_refusal_with`] — the one helper
/// every `/cd`-aware reader already routes through — so no reader needs its own
/// wiring. Empty until a `/cd` lands in a directory whose config sets it.
static CD_ALLOWED_DIRS: std::sync::LazyLock<crate::cd_deny::DenyAdditions> =
    std::sync::LazyLock::new(Default::default);

/// The `/cd` consumer for `[directories]`, over explicit handles: append the
/// resolved deny entries to `deny` (never removes) and install the resolved
/// allow entries as THE allow list in `allow`, dropping the previous one
/// (empty when the file is absent, unreadable or sets none). Returns the
/// number of NEW deny entries.
///
/// Allow is deliberately REPLACED, unlike deny's append. Deny entries name places to
/// stay out of, and the union of several projects' "stay out" lists is still a
/// sensible fence. Allow entries name the ONLY places to be, and the
/// intersection of two unrelated projects' allow lists is usually empty — it
/// would refuse every path, including the directory the user just moved to.
/// So each `/cd` brings its own allow list (or none). This can only widen back
/// to the launch fence, never past it: the startup allow list is a separate
/// check (`path_guard_refusal_lists`) that no `/cd` can remove, and the `/cd`
/// deny list stays append-only.
pub(crate) fn apply_cd_dir_fence_with(
    deny: &crate::cd_deny::DenyAdditions,
    allow: &crate::cd_deny::DenyAdditions,
    target: &Path,
    reading: &CdConfigReading,
) -> usize {
    *crate::sync_util::lock_or_recover(allow) = dir_allow_entries(target, reading);
    crate::cd_deny::append_deny_with(deny, &dir_deny_entries(target, reading))
}

/// The production `/cd` consumer, over the process-wide lists.
pub(crate) fn apply_cd_dir_fence(target: &Path, reading: &CdConfigReading) -> usize {
    apply_cd_dir_fence_with(&CD_DENIED_DIRS, &CD_ALLOWED_DIRS, target, reading)
}

/// Refusal from the `/cd`-applied `[directories]` lists, or `None`: the deny
/// list in `list` and the process-wide `/cd` allow list. Both empty is `None`
/// with no filesystem call.
pub(crate) fn cd_denied_dir_refusal_with(
    list: &crate::cd_deny::DenyAdditions,
    path: &str,
) -> Option<String> {
    cd_dir_refusal_with(list, &CD_ALLOWED_DIRS, path)
}

/// [`cd_denied_dir_refusal_with`] with the allow list passed in, so tests drive
/// local handles. Matching is `DirectoryRestrictions::check_path` itself
/// (symlinks, wildcards, unresolvable paths, deny before allow) — never a
/// second matcher. With an empty allow list it is byte-identical to the Day-226
/// deny-only check.
pub(crate) fn cd_dir_refusal_with(
    deny: &crate::cd_deny::DenyAdditions,
    allow: &crate::cd_deny::DenyAdditions,
    path: &str,
) -> Option<String> {
    let r = crate::config::DirectoryRestrictions {
        allow: crate::sync_util::lock_or_recover(allow).clone(),
        deny: crate::sync_util::lock_or_recover(deny).clone(),
    };
    if r.is_empty() {
        return None;
    }
    r.check_path(path).err()
}

/// True when `/cd` has applied no `[directories]` entry of either kind — the
/// test a fast path must use instead of `deny.is_empty()`, which goes stale
/// the moment a `/cd` sets only an allow list.
pub(crate) fn cd_dir_fence_is_empty(list: &crate::cd_deny::DenyAdditions) -> bool {
    cd_dir_fence_is_empty_with(list, &CD_ALLOWED_DIRS)
}

/// [`cd_dir_fence_is_empty`] over explicit handles.
fn cd_dir_fence_is_empty_with(
    deny: &crate::cd_deny::DenyAdditions,
    allow: &crate::cd_deny::DenyAdditions,
) -> bool {
    crate::sync_util::lock_or_recover(deny).is_empty()
        && crate::sync_util::lock_or_recover(allow).is_empty()
}

/// What both path guards ask: the startup fence first, byte-identical to
/// before, then whatever `/cd` has applied so far.
pub(crate) fn path_guard_refusal_with(
    list: &crate::cd_deny::DenyAdditions,
    startup: &crate::config::DirectoryRestrictions,
    path: &str,
) -> Option<String> {
    path_guard_refusal_lists(list, &CD_ALLOWED_DIRS, startup, path)
}

/// [`path_guard_refusal_with`] with the `/cd` allow list passed in. Both
/// fences must pass: a path the `/cd` allow list admits is still refused if
/// it lies outside the startup allow list (monotonic — `/cd` only narrows).
pub(crate) fn path_guard_refusal_lists(
    deny: &crate::cd_deny::DenyAdditions,
    allow: &crate::cd_deny::DenyAdditions,
    startup: &crate::config::DirectoryRestrictions,
    path: &str,
) -> Option<String> {
    startup
        .check_path(path)
        .err()
        .or_else(|| cd_dir_refusal_with(deny, allow, path))
}

/// A clone of the process-wide `/cd` directory-deny handle, for readers outside
/// this module that compose their own refusal wording (`commands_file_fence`'s
/// `@path`/`/add`/`/explain`/`--image` doors, #869). Mirrors
/// `cd_deny::cd_added_deny_handle`.
pub(crate) fn cd_denied_dirs_handle() -> crate::cd_deny::DenyAdditions {
    std::sync::Arc::clone(&CD_DENIED_DIRS)
}

/// A clone of the process-wide `/cd` allow-list handle, for `/permissions`.
pub(crate) fn cd_allowed_dirs_handle() -> crate::cd_deny::DenyAdditions {
    std::sync::Arc::clone(&CD_ALLOWED_DIRS)
}

/// The production guard call, reading the process-wide `/cd` list.
pub(crate) fn path_guard_refusal(
    startup: &crate::config::DirectoryRestrictions,
    path: &str,
) -> Option<String> {
    path_guard_refusal_with(&CD_DENIED_DIRS, startup, path)
}

/// Is `<target>/.yoyo.toml` the very file this session loaded at startup (#1032)?
/// Both paths are canonicalized, so a symlink or a `..` spelling still matches.
/// `false` when nothing was loaded or either path cannot be canonicalized, so
/// a failure falls back to today's note, never to silence.
pub(crate) fn is_loaded_config(target: &Path, loaded: Option<&Path>) -> bool {
    let Some(loaded) = loaded else {
        return false;
    };
    match (
        std::fs::canonicalize(target.join(".yoyo.toml")),
        std::fs::canonicalize(loaded),
    ) {
        (Ok(here), Ok(loaded)) => here == loaded,
        _ => false,
    }
}

/// `reading` as `/cd` should report it: when the file IS the loaded config
/// (back in the launch directory), its sections are applied, so the
/// NOT-applied list is dropped (#1032). Deny entries and unreadable files are
/// untouched; any other directory gets `reading` back unchanged.
pub(crate) fn reading_for_note(reading: &CdConfigReading, is_loaded: bool) -> CdConfigReading {
    match reading {
        CdConfigReading::Sets(_, deny, dir_deny, dir_allow) if is_loaded => CdConfigReading::Sets(
            Vec::new(),
            deny.clone(),
            dir_deny.clone(),
            dir_allow.clone(),
        ),
        other => other.clone(),
    }
}

/// The fences this session actually enforces from startup, as raw entries:
/// the bash deny list the executors were built with and the final
/// `[directories]` lists (after `--deny`/`--deny-dir` overrides and
/// `--restricted`'s extra allow entry). Recorded once, in `cli.rs`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StartupFences {
    pub deny: Vec<String>,
    pub dir_deny: Vec<String>,
    pub dir_allow: Vec<String>,
}

static STARTUP_FENCES: std::sync::OnceLock<StartupFences> = std::sync::OnceLock::new();

/// Record the startup fences. First writer wins.
pub(crate) fn record_startup_fences(deny: &[String], dirs: &crate::config::DirectoryRestrictions) {
    let _ = STARTUP_FENCES.set(StartupFences {
        deny: deny.to_vec(),
        dir_deny: dirs.deny.clone(),
        dir_allow: dirs.allow.clone(),
    });
}

/// `reading` minus whatever the startup policy already enforces, but ONLY when
/// `<target>/.yoyo.toml` is the loaded file (`is_loaded`). `/cd` home must not
/// re-append or re-announce the launch deny list (#869, Day 230). The rule is
/// "already enforced", not "same file": a `--deny` flag overrides the file's
/// deny at startup, and a file edited since startup has new entries, and both
/// of those still apply and are announced. Per list:
/// - `[permissions] deny`: an entry is dropped iff the startup list holds it.
/// - `[directories] deny`: dropped iff it is absolute or `~` AND the startup
///   list holds it verbatim. A relative startup entry resolves against the
///   CURRENT cwd, so `/cd`'s target-anchored copy is not a duplicate of it.
/// - `[directories] allow`: replaced by every `/cd`, so it is dropped as a
///   whole iff it equals the startup allow list as a set; then in the launch
///   dir both resolve to the same paths and the startup list alone is the same
///   fence. Any difference keeps it (dropping part of an allow list widens).
///
/// A foreign directory (`!is_loaded`) or no record gets `reading` unchanged.
pub(crate) fn without_startup_enforced(
    reading: &CdConfigReading,
    is_loaded: bool,
    startup: Option<&StartupFences>,
) -> CdConfigReading {
    let (CdConfigReading::Sets(sections, deny, dir_deny, dir_allow), true, Some(st)) =
        (reading, is_loaded, startup)
    else {
        return reading.clone();
    };
    let deny = deny
        .iter()
        .filter(|e| !st.deny.contains(e))
        .cloned()
        .collect();
    let dir_deny = dir_deny
        .iter()
        .filter(|e| {
            let anchored = e.starts_with('~') || Path::new(e.as_str()).is_absolute();
            !(anchored && st.dir_deny.contains(e))
        })
        .cloned()
        .collect();
    let same_allow = {
        let mut a = dir_allow.clone();
        let mut b = st.dir_allow.clone();
        a.sort();
        a.dedup();
        b.sort();
        b.dedup();
        a == b
    };
    let dir_allow = if same_allow {
        Vec::new()
    } else {
        dir_allow.clone()
    };
    CdConfigReading::Sets(sections.clone(), deny, dir_deny, dir_allow)
}

/// The reading `/cd` acts on: the one read of `<target>/.yoyo.toml`, minus
/// what the startup policy already enforces when it is the loaded file. The
/// deny append, the `[directories]` fence and the note all take THIS value,
/// so what is applied and what is announced cannot disagree.
pub(crate) fn reading_for_cd(target: &Path, reading: &CdConfigReading) -> CdConfigReading {
    let is_loaded = is_loaded_config(target, crate::config::loaded_config_resolved_path());
    without_startup_enforced(reading, is_loaded, STARTUP_FENCES.get())
}

/// Print the note for an already-read `reading` to stderr.
pub(crate) fn print_cd_config_note(target: &Path, reading: &CdConfigReading) {
    let plain = crate::format::is_plain_output();
    let is_loaded = is_loaded_config(target, crate::config::loaded_config_resolved_path());
    let reading = &reading_for_note(reading, is_loaded);
    let note = cd_config_note(&target.display().to_string(), reading, plain);
    // Deliberately NOT gated on quiet mode, for either volume. A warning is not
    // chrome: `--quiet` is auto-on whenever stdin and stdout are both piped, so a
    // quiet-gated warning is silent for exactly the runs nobody watches live —
    // the reason the max_tokens ceiling warning was un-gated on Day 211 (#964).
    // (The DIM note was never quiet-gated either; it stays as it was.)
    if let Some(note) = note {
        eprintln!(
            "{}",
            render_cd_config_note(&note, note_volume(reading), plain)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERMS_TOML: &str = "model = \"x\"\n\n[permissions]\ndeny = [\"rm -rf *\"]\n";
    const ALLOW_TOML: &str = "[permissions]\nallow = [\"git *\"]\n";
    const DIR_DENY_TOML: &str = "[directories]\ndeny = [\"secrets\", \"/abs/vault\"]\n";
    const DIR_ALLOW_TOML: &str = "[directories]\nallow = [\"src\"]\n";

    fn sets(text: &str) -> CdConfigReading {
        classify_read(Ok(text.to_string()))
    }

    #[test]
    fn permissions_deny_is_named_as_enforced() {
        // Anti-vacuous: the fixture really carries a deny entry.
        assert!(PERMS_TOML.contains("deny"));
        // Day 218: deny is applied on /cd, so it is no longer an unapplied section.
        assert_eq!(unapplied_config_sections(PERMS_TOML), Vec::<&str>::new());
        let reading = sets(PERMS_TOML);
        assert_eq!(deny_entries(&reading), ["rm -rf *".to_string()]);
        let note = cd_config_note("/repo", &reading, false).expect("note");
        assert_eq!(
            note,
            "  /repo/.yoyo.toml: its 1 [permissions] deny pattern is now enforced on bash \
             commands for the rest of this session (another /cd does not remove it; file tools \
             never read this list). Its [permissions] allow, hooks and MCP servers would still \
             not be reloaded on /cd (#1040)."
        );
    }

    #[test]
    fn deny_plus_unapplied_sections_names_both() {
        // The hook goes before the header: under `[permissions]` TOML reads
        // it as `permissions.hooks.pre.bash`, no hook at all (#1184).
        let text = "hooks.pre.bash = \"echo\"\n\
                    [permissions]\nallow = [\"git *\"]\ndeny = [\"a *\", \"b *\"]\n";
        let reading = sets(text);
        assert_eq!(note_volume(&reading), NoteVolume::Warning);
        let note = cd_config_note("/r", &reading, true).unwrap();
        assert_eq!(
            note,
            "  /r/.yoyo.toml: its 2 [permissions] deny patterns are now enforced on bash \
             commands for the rest of this session (another /cd does not remove them; file tools \
             never read this list). It also sets [permissions] allow, hooks and they are NOT \
             applied: the launch directory's settings stay in force, except that its own [permissions] allow and hooks pause outside its tree. Restart yoyo here to use \
             them (#1040)."
        );
    }

    #[test]
    fn allow_only_keeps_the_day_213_wording() {
        // Near-miss: no deny entry, so nothing is applied and the old text stands.
        let reading = sets(ALLOW_TOML);
        assert!(deny_entries(&reading).is_empty());
        assert_eq!(
            cd_config_note("/repo", &reading, false).unwrap(),
            "  /repo/.yoyo.toml sets [permissions] allow and they are NOT applied in this \
             session — the launch directory's settings stay in force, except that its own [permissions] allow and hooks pause outside its tree. Restart yoyo here to use \
             them (#1040)."
        );
    }

    #[test]
    fn model_only_config_sets_nothing_and_prints_nothing() {
        let text = "provider = \"anthropic\"\nmodel = \"x\"\n";
        assert_eq!(unapplied_config_sections(text), Vec::<&str>::new());
        assert_eq!(
            sets(text),
            CdConfigReading::Sets(vec![], vec![], vec![], vec![])
        );
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
    fn all_unapplied_sections_in_fixed_order() {
        let text = "mcp = [\"npx server\"]\n\
                    hooks.pre.bash = \"echo hi\"\n\
                    [directories]\nallow = [\"src\"]\ndeny = [\"secrets\"]\n\
                    [permissions]\nallow = [\"git *\"]\n";
        assert_eq!(
            unapplied_config_sections(text),
            vec!["[permissions] allow", "hooks", "MCP servers"]
        );
        let note = cd_config_note("/r", &sets(text), false).unwrap();
        assert!(
            note.contains("sets [permissions] allow, hooks, MCP servers and they are NOT"),
            "{note}"
        );
        // Day 228: [directories] allow is reported as APPLIED, never as unapplied.
        assert!(
            note.contains("its 1 [directories] allow entry now also fence the file tools"),
            "{note}"
        );
        // Day 226: the deny half of the same section is reported as APPLIED.
        assert!(
            note.starts_with(
                "  /r/.yoyo.toml: its 1 [directories] deny entry is now enforced on the file tools"
            ),
            "{note}"
        );
    }

    #[test]
    fn measured_b_fixture_names_every_widening_section_as_not_applied() {
        // Day 231 (#869, #1040): the exact .yoyo.toml of the built-binary run. After
        // `/cd` into it, trusted or not, its hook did not run, `echo hi` still asked,
        // and its MCP server never started, which is what this note says.
        let text = "hooks.post.bash = \"echo HOOKFIRED >> /tmp/m\"\n\
                    [permissions]\nallow = [\"echo *\"]\n\
                    [mcp_servers.x]\ncommand = \"sh\"\nargs = [\"-c\", \"touch /tmp/n\"]\n";
        // Anti-vacuous: the fixture really carries all three widening sections.
        for needle in ["hooks.post.bash", "allow = ", "[mcp_servers.x]"] {
            assert!(text.contains(needle), "{needle}");
        }
        let reading = sets(text);
        assert_eq!(note_volume(&reading), NoteVolume::Warning);
        assert_eq!(
            cd_config_note("/tmp/B", &reading, false).unwrap(),
            "  /tmp/B/.yoyo.toml sets [permissions] allow, hooks, MCP servers and they are NOT \
             applied in this session — the launch directory's settings stay in force, except \
             that its own [permissions] allow and hooks pause outside its tree. Restart yoyo \
             here to use them (#1040)."
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
        assert_ne!(bad, CdConfigReading::Sets(vec![], vec![], vec![], vec![]));
        assert!(deny_entries(&bad).is_empty()); // nothing applied from unread bytes
        let note = cd_config_note("/repo", &bad, false).expect("unreadable must be reported");
        assert!(note.contains("could not be read"), "{note}");
    }

    /// #1047: a FIFO at `<target>/.yoyo.toml` used to hang `/cd` on open. It
    /// now reads as Unreadable (nothing applied, the note says so) at once.
    #[cfg(unix)]
    #[test]
    fn fifo_target_config_is_unreadable_not_a_hang() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join(".yoyo.toml");
        let made = std::process::Command::new("mkfifo").arg(&fifo).status();
        if !made.map(|s| s.success()).unwrap_or(false) {
            eprintln!("mkfifo unavailable; skipping");
            return;
        }
        let target = dir.path().to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || tx.send(read_cd_config(&target)).ok());
        let reading = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("read_cd_config hung on a FIFO .yoyo.toml (#1047)");
        assert_eq!(reading, CdConfigReading::Unreadable);
        assert!(deny_entries(&reading).is_empty());
        let note = cd_config_note("/repo", &reading, true).expect("must be reported");
        assert!(note.contains(".yoyo.toml"), "{note}");
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
    // --- Day 214: volume follows severity (#869) ---

    /// One fixture per safety section, each parsed through the real detector.
    const SAFETY_FIXTURES: [&str; 3] = [
        ALLOW_TOML,
        "hooks.pre.bash = \"echo hi\"\n",
        "[mcp_servers.fs]\ncommand = \"npx\"\n",
    ];

    #[test]
    fn every_safety_section_is_detected_and_warns() {
        assert!(!SAFETY_SECTIONS.is_empty()); // anti-vacuous
        let mut seen = Vec::new();
        for text in SAFETY_FIXTURES {
            let reading = sets(text);
            let CdConfigReading::Sets(found, _, _, _) = &reading else {
                panic!("{text:?} did not read as Sets");
            };
            assert_eq!(found.len(), 1, "{text:?} -> {found:?}"); // anti-vacuous
            seen.push(found[0]);
            assert_eq!(note_volume(&reading), NoteVolume::Warning, "{text:?}");
        }
        // The fixtures cover the whole list, derived from the detector's output.
        assert_eq!(seen, SAFETY_SECTIONS.to_vec());
        // A deny-only file names no unapplied section, and still warns: it
        // changes what bash may run for the rest of the session.
        assert_eq!(note_volume(&sets(PERMS_TOML)), NoteVolume::Warning);
        // Same for a [directories]-deny-only file (Day 226): it changes what
        // the file tools may touch, so it warns though it names no section.
        assert_eq!(note_volume(&sets(DIR_DENY_TOML)), NoteVolume::Warning);
        // And a [directories]-allow-only file (Day 228): it narrows where the
        // file tools may go, so it warns though it names no unapplied section.
        let allow_only = sets(DIR_ALLOW_TOML);
        assert_eq!(
            unapplied_config_sections(DIR_ALLOW_TOML),
            Vec::<&str>::new()
        );
        assert_eq!(note_volume(&allow_only), NoteVolume::Warning);
    }

    #[test]
    fn safety_note_renders_as_yellow_warning_not_dim() {
        let reading = sets(PERMS_TOML);
        let note = cd_config_note("/repo", &reading, false).unwrap();
        let out = render_cd_config_note(&note, note_volume(&reading), false);
        assert!(
            out.starts_with("\x1b[33m⚠ warning: /repo/.yoyo.toml: its 1 [permissions] deny"),
            "{out:?}"
        );
        assert!(out.ends_with("\x1b[0m"), "{out:?}");
        assert!(!out.contains("\x1b[2m"), "{out:?}");
    }

    #[test]
    fn safety_note_plain_is_glyph_and_escape_free() {
        let reading = sets(PERMS_TOML);
        let note = cd_config_note("/repo", &reading, true).unwrap();
        let out = render_cd_config_note(&note, note_volume(&reading), true);
        assert!(
            out.starts_with("warning: /repo/.yoyo.toml: its 1 [permissions] deny"),
            "{out:?}"
        );
        assert!(!out.as_bytes().contains(&0x1b), "{out:?}");
        assert!(!out.contains('⚠') && !out.contains('—'), "{out:?}");
    }

    #[test]
    fn unreadable_note_stays_dim_and_byte_identical() {
        // Near-miss: the one note that names no safety section. Full-string
        // assert_eq against the Day-213 rendering, in both plain modes.
        let reading = CdConfigReading::Unreadable;
        assert_eq!(note_volume(&reading), NoteVolume::Chrome);
        let note = cd_config_note("/repo", &reading, false).unwrap();
        assert_eq!(
            render_cd_config_note(&note, note_volume(&reading), false),
            "\x1b[2m  /repo/.yoyo.toml exists but could not be read, and NONE of it is applied \
             in this session — the launch directory's settings stay in force, except that its own [permissions] allow and hooks pause outside its tree. Restart yoyo here \
             to use it (#1040).\x1b[0m"
        );
        let plain = cd_config_note("/repo", &reading, true).unwrap();
        assert_eq!(
            render_cd_config_note(&plain, note_volume(&reading), true),
            "\x1b[2m  /repo/.yoyo.toml exists but could not be read, and NONE of it is applied \
             in this session: the launch directory's settings stay in force, except that its own [permissions] allow and hooks pause outside its tree. Restart yoyo here \
             to use it (#1040).\x1b[0m"
        );
    }

    #[test]
    fn empty_and_absent_readings_are_chrome_and_print_nothing() {
        let model_only = sets("model = \"x\"\n");
        for r in [model_only, CdConfigReading::Absent] {
            assert_eq!(note_volume(&r), NoteVolume::Chrome);
            assert_eq!(cd_config_note("/repo", &r, false), None);
        }
    }

    /// Deliberately WEAK source guard: proves the print site does not consult
    /// quiet mode, never that the line reaches a terminal. A warning is not
    /// chrome and must not vanish under the auto-quiet of piped runs.
    #[test]
    fn print_site_is_not_quiet_gated() {
        let src = include_str!("cd_config_note.rs");
        let start = src.find("pub(crate) fn print_cd_config_note").unwrap();
        let end = start + src[start..].find("#[cfg(test)]").unwrap();
        let body = &src[start..end];
        assert!(body.contains("render_cd_config_note(")); // anti-vacuous
        let needle = ["is_", "quiet"].concat();
        assert!(
            !body.contains(&needle),
            "print_cd_config_note must not gate on quiet"
        );
    }

    // --- Day 226: [directories] deny applies to the file tools after /cd ---

    use crate::cd_deny::DenyAdditions;
    use crate::config::DirectoryRestrictions;

    fn local() -> DenyAdditions {
        Default::default()
    }

    fn tmp() -> (tempfile::TempDir, std::path::PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().canonicalize().unwrap();
        (d, p)
    }

    fn p(base: &Path, rel: &str) -> String {
        base.join(rel).to_string_lossy().into_owned()
    }

    const HOOK_TOML: &str = "hooks.pre.bash = \"echo hi\"\n";

    /// The note `/cd` prints for `target`, given which file the session loaded.
    fn note_at(target: &Path, loaded: Option<&Path>) -> Option<String> {
        let reading = read_cd_config(target);
        let reading = reading_for_note(&reading, is_loaded_config(target, loaded));
        cd_config_note(&target.display().to_string(), &reading, true)
    }

    #[test]
    fn cd_back_into_launch_dir_does_not_call_loaded_config_unapplied() {
        // #1032: launch dir A sets a hook; /cd back into A must not say it is NOT applied.
        let (_d, base) = tmp();
        let a = base.join("A");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(a.join(".yoyo.toml"), HOOK_TOML).unwrap();
        // Anti-vacuous: the file really does set an unapplied section.
        assert_eq!(read_cd_config(&a), sets(HOOK_TOML));
        assert!(
            matches!(read_cd_config(&a), CdConfigReading::Sets(ref s, ..) if s == &vec!["hooks"])
        );

        // Loaded path spelled with `..`.
        let dotted = base.join("A").join("..").join("A").join(".yoyo.toml");
        assert_eq!(note_at(&a, Some(&dotted)), None);

        // Reached through a symlinked directory.
        #[cfg(unix)]
        {
            let link = base.join("link-to-A");
            std::os::unix::fs::symlink(&a, &link).unwrap();
            assert_eq!(note_at(&link, Some(&a.join(".yoyo.toml"))), None);
            assert_eq!(note_at(&a, Some(&link.join(".yoyo.toml"))), None);
        }
    }

    #[test]
    fn different_dir_with_same_sections_keeps_the_exact_warning() {
        // Near-miss guard for #1032: B sets the same hook, A's file was loaded.
        let (_d, base) = tmp();
        let (a, b) = (base.join("A"), base.join("B"));
        for d in [&a, &b] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join(".yoyo.toml"), HOOK_TOML).unwrap();
        }
        let expected = format!(
            "  {}/.yoyo.toml sets hooks and they are NOT applied in this session: the launch \
             directory's settings stay in force, except that its own [permissions] allow and \
             hooks pause outside its tree. Restart yoyo here to use them (#1040).",
            b.display()
        );
        assert_eq!(
            note_at(&b, Some(&a.join(".yoyo.toml"))),
            Some(expected.clone())
        );
        // No loaded config at all: unchanged.
        assert_eq!(note_at(&b, None), Some(expected));
        assert!(!is_loaded_config(&b, None));
    }

    #[test]
    fn missing_loaded_file_falls_back_to_the_warning_not_silence() {
        let (_d, base) = tmp();
        let a = base.join("A");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(a.join(".yoyo.toml"), HOOK_TOML).unwrap();
        let gone = base.join("gone").join(".yoyo.toml");
        assert!(!is_loaded_config(&a, Some(&gone)));
        assert!(note_at(&a, Some(&gone)).unwrap().contains("NOT applied"));
    }

    #[test]
    fn loaded_config_keeps_its_applied_deny_entries() {
        // Only the NOT-applied list is dropped; deny entries still report as applied.
        let reading = sets(PERMS_TOML);
        assert_eq!(reading_for_note(&reading, true), reading);
        let both = sets("[permissions]\nallow = [\"git *\"]\ndeny = [\"rm *\"]\n");
        match reading_for_note(&both, true) {
            CdConfigReading::Sets(s, d, _, _) => {
                assert!(s.is_empty());
                assert_eq!(d, vec!["rm *".to_string()]);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(reading_for_note(&both, false), both);
        assert_eq!(
            reading_for_note(&CdConfigReading::Unreadable, true),
            CdConfigReading::Unreadable
        );
    }

    // --- Day 230 (#869): /cd home re-appends and re-announces nothing ---

    const HOME_DENY_TOML: &str = "[permissions]\ndeny = [\"rm *\"]\n";

    fn fences(deny: &[&str], dir_deny: &[&str], dir_allow: &[&str]) -> StartupFences {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect();
        StartupFences {
            deny: v(deny),
            dir_deny: v(dir_deny),
            dir_allow: v(dir_allow),
        }
    }

    /// What `/cd` does with `target`, end to end over local lists: the bash
    /// deny it appends, and the note it prints (plain).
    fn cd_into(
        target: &Path,
        is_loaded: bool,
        st: &StartupFences,
        bash: &DenyAdditions,
    ) -> Option<String> {
        let reading = without_startup_enforced(&read_cd_config(target), is_loaded, Some(st));
        crate::cd_deny::append_deny_with(bash, deny_entries(&reading));
        let reading = reading_for_note(&reading, is_loaded);
        cd_config_note(&target.display().to_string(), &reading, true)
    }

    fn foreign_deny_note(dir: &Path) -> String {
        format!(
            "  {}/.yoyo.toml: its 1 [permissions] deny pattern is now enforced on bash commands \
             for the rest of this session (another /cd does not remove it; file tools never read \
             this list). Its [permissions] allow, hooks and MCP servers would still not be \
             reloaded on /cd (#1040).",
            dir.display()
        )
    }

    #[test]
    fn cd_home_neither_reappends_nor_announces_the_startup_deny() {
        let (_d, base) = tmp();
        let a = base.join("A");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(a.join(".yoyo.toml"), HOME_DENY_TOML).unwrap();
        // Anti-vacuous: the raw reading really carries the deny entry.
        assert_eq!(deny_entries(&read_cd_config(&a)), ["rm *".to_string()]);
        let st = fences(&["rm *"], &[], &[]);
        let bash = local();
        // Two round trips home.
        assert_eq!(cd_into(&a, true, &st, &bash), None);
        assert_eq!(cd_into(&a, true, &st, &bash), None);
        assert!(crate::sync_util::lock_or_recover(&bash).is_empty());
        // Nothing loosened: the startup list still refuses it.
        assert!(crate::cd_deny::effective_user_deny_refusal(&st.deny, &bash, "rm foo").is_some());
    }

    /// The exact launch-dir fixture measured on #869 (2026-10-16T09:23Z): a
    /// deny pattern AND a hook. Silent on the way home since 528dea35; this pins
    /// it at the rendered bytes, through the same composition `/cd` runs.
    const HOME_DENY_HOOK_TOML: &str =
        "hooks.post.bash = \"echo hi\"\n\n[permissions]\ndeny = [\"rm *\"]\n";

    /// The stderr bytes `/cd` would write (plain), or `None` for silence.
    fn rendered_cd(target: &Path, is_loaded: bool, st: &StartupFences) -> Option<String> {
        let reading = without_startup_enforced(&read_cd_config(target), is_loaded, Some(st));
        let reading = reading_for_note(&reading, is_loaded);
        cd_config_note(&target.display().to_string(), &reading, true)
            .map(|n| render_cd_config_note(&n, note_volume(&reading), true))
    }

    #[test]
    fn cd_home_with_deny_and_hook_renders_nothing() {
        let (_d, base) = tmp();
        let a = base.join("A");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(a.join(".yoyo.toml"), HOME_DENY_HOOK_TOML).unwrap();
        // Anti-vacuous: the raw reading carries both the deny and the hook.
        let raw = read_cd_config(&a);
        assert_eq!(deny_entries(&raw), ["rm *".to_string()]);
        assert_eq!(
            raw,
            CdConfigReading::Sets(vec![HOOKS], vec!["rm *".into()], vec![], vec![])
        );
        assert_eq!(note_volume(&raw), NoteVolume::Warning);
        let st = fences(&["rm *"], &[], &[]);
        assert_eq!(rendered_cd(&a, true, &st), None);
    }

    #[test]
    fn foreign_dir_with_deny_and_hook_still_warns_byte_identically() {
        // Near-miss: the same bytes in a directory that is NOT the loaded file
        // keep the full warning, captured before this test was added.
        let (_d, base) = tmp();
        let b = base.join("B");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join(".yoyo.toml"), HOME_DENY_HOOK_TOML).unwrap();
        let st = fences(&["rm *"], &[], &[]);
        let want = format!(
            "warning: {}/.yoyo.toml: its 1 [permissions] deny pattern is now enforced on bash \
             commands for the rest of this session (another /cd does not remove it; file tools \
             never read this list). It also sets hooks and they are NOT applied: the launch \
             directory's settings stay in force, except that its own [permissions] allow and \
             hooks pause outside its tree. Restart yoyo here to use them (#1040).",
            b.display()
        );
        assert_eq!(rendered_cd(&b, false, &st), Some(want));
    }

    #[test]
    fn foreign_dir_with_the_same_deny_keeps_its_exact_note_and_append() {
        // Near-miss guard: B is not the loaded file, so nothing is dropped.
        let (_d, base) = tmp();
        let b = base.join("B");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join(".yoyo.toml"), HOME_DENY_TOML).unwrap();
        let st = fences(&["rm *"], &[], &[]);
        let bash = local();
        assert_eq!(cd_into(&b, false, &st, &bash), Some(foreign_deny_note(&b)));
        assert_eq!(
            *crate::sync_util::lock_or_recover(&bash),
            ["rm *".to_string()]
        );
        // Round trip back into B: still announced, still one copy.
        assert_eq!(cd_into(&b, false, &st, &bash), Some(foreign_deny_note(&b)));
        assert_eq!(
            *crate::sync_util::lock_or_recover(&bash),
            ["rm *".to_string()]
        );
        // No startup record at all: unchanged too.
        let r = read_cd_config(&b);
        assert_eq!(without_startup_enforced(&r, true, None), r);
    }

    #[test]
    fn cd_home_still_applies_deny_the_startup_did_not_enforce() {
        // `--deny "git push *"` overrode the file's deny at startup, so "rm *"
        // was never enforced; coming home applies and announces it, as today.
        let (_d, base) = tmp();
        let a = base.join("A");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(a.join(".yoyo.toml"), HOME_DENY_TOML).unwrap();
        let st = fences(&["git push *"], &[], &[]);
        let bash = local();
        assert_eq!(cd_into(&a, true, &st, &bash), Some(foreign_deny_note(&a)));
        assert_eq!(
            *crate::sync_util::lock_or_recover(&bash),
            ["rm *".to_string()]
        );
    }

    #[test]
    fn cd_home_directories_drop_only_what_startup_enforces_identically() {
        let r = sets("[directories]\ndeny = [\"secrets\", \"/abs/vault\"]\nallow = [\"src\"]\n");
        // Absolute entry already enforced: dropped. Relative one resolves
        // against the current cwd at startup, so /cd's anchored copy stays.
        // Allow equal as a set: dropped.
        let st = fences(&[], &["secrets", "/abs/vault"], &["src"]);
        assert_eq!(
            without_startup_enforced(&r, true, Some(&st)),
            CdConfigReading::Sets(vec![], vec![], vec!["secrets".into()], vec![])
        );
        // Allow differs (e.g. --restricted added the cwd): kept whole.
        let st = fences(&[], &[], &["src", "/launch"]);
        assert_eq!(
            without_startup_enforced(&r, true, Some(&st)),
            CdConfigReading::Sets(
                vec![],
                vec![],
                vec!["secrets".into(), "/abs/vault".into()],
                vec!["src".into()]
            )
        );
    }

    #[test]
    fn dir_deny_note_says_applied_to_file_tools() {
        let reading = sets(DIR_DENY_TOML);
        // Deny is applied, so it is no longer an unapplied section.
        assert_eq!(unapplied_config_sections(DIR_DENY_TOML), Vec::<&str>::new());
        assert_eq!(
            cd_config_note("/repo", &reading, false).unwrap(),
            "  /repo/.yoyo.toml: its 2 [directories] deny entries are now enforced on the file \
             tools for the rest of this session (another /cd does not remove them; bash never \
             reads this list). Its [permissions] allow, hooks and MCP servers would still not be \
             reloaded on /cd (#1040)."
        );
        // Both deny lists at once: each named, each with its own executor.
        let both = sets("[permissions]\ndeny = [\"rm *\"]\n[directories]\ndeny = [\"s\"]\n");
        assert_eq!(
            cd_config_note("/r", &both, true).unwrap(),
            "  /r/.yoyo.toml: its 1 [permissions] deny pattern is now enforced on bash commands \
             for the rest of this session (another /cd does not remove it; file tools never read \
             this list); its 1 [directories] deny entry is now enforced on the file tools for the \
             rest of this session (another /cd does not remove it; bash never reads this list). \
             Its [permissions] allow, hooks and MCP servers would still not be reloaded on /cd \
             (#1040)."
        );
    }

    #[test]
    fn relative_entry_is_resolved_against_the_cd_target_and_stays_there() {
        let (_a_guard, a) = tmp();
        let reading = sets(DIR_DENY_TOML);
        let entries = dir_deny_entries(&a, &reading);
        // Relative -> <target>/secrets; absolute kept as written.
        assert_eq!(entries, vec![p(&a, "secrets"), "/abs/vault".to_string()]);
        let list = local();
        assert_eq!(crate::cd_deny::append_deny_with(&list, &entries), 2);
        // A path in A/secrets is refused even though the process cwd is not A.
        assert_ne!(std::env::current_dir().unwrap(), a); // anti-vacuous: different base
        assert!(cd_denied_dir_refusal_with(&list, &p(&a, "secrets/key.txt")).is_some());
        // Near-miss: the same relative spelling under the CURRENT cwd is not
        // denied, because the entry was bound to A at append time.
        assert_eq!(cd_denied_dir_refusal_with(&list, "secrets/key.txt"), None);
        // `~` entries are left for check_path to expand, never joined.
        let tilde = sets("[directories]\ndeny = [\"~/vault\"]\n");
        assert_eq!(dir_deny_entries(&a, &tilde), vec!["~/vault".to_string()]);
        // Nothing applied from a reading that names no entries or was unread.
        assert!(dir_deny_entries(&a, &CdConfigReading::Unreadable).is_empty());
        assert!(dir_deny_entries(&a, &CdConfigReading::Absent).is_empty());
    }

    #[test]
    fn guard_refuses_a_cd_denied_dir_and_passes_near_misses() {
        let (_g, t) = tmp();
        let secrets = p(&t, "secrets");
        let list = local();
        crate::cd_deny::append_deny_with(&list, std::slice::from_ref(&secrets));
        let none = DirectoryRestrictions::default();
        let hit = p(&t, "secrets/a.txt");
        assert_eq!(
            path_guard_refusal_with(&list, &none, &hit),
            Some(format!(
                "Access denied: '{hit}' is under restricted directory '{secrets}'"
            ))
        );
        for near in [p(&t, "secretsXYZ/a"), p(&t, "other.txt")] {
            assert_eq!(path_guard_refusal_with(&list, &none, &near), None, "{near}");
        }
        // A second /cd elsewhere appends; it never removes the first entry.
        let (_g2, b) = tmp();
        crate::cd_deny::append_deny_with(&list, &dir_deny_entries(&b, &sets(DIR_DENY_TOML)));
        assert!(path_guard_refusal_with(&list, &none, &hit).is_some());
        assert!(path_guard_refusal_with(&list, &none, &p(&b, "secrets/x")).is_some());
    }

    #[test]
    fn empty_cd_list_leaves_the_startup_guard_byte_identical() {
        let (_g, t) = tmp();
        let empty = local();
        let startups = [
            DirectoryRestrictions::default(),
            DirectoryRestrictions {
                allow: vec![],
                deny: vec![p(&t, "secrets")],
            },
            DirectoryRestrictions {
                allow: vec![p(&t, "src")],
                deny: vec![],
            },
        ];
        let paths = [p(&t, "secrets/a"), p(&t, "src/main.rs"), p(&t, "other")];
        let mut refusals = 0;
        for r in &startups {
            for path in &paths {
                let before = r.check_path(path).err();
                refusals += usize::from(before.is_some());
                assert_eq!(
                    path_guard_refusal_with(&empty, r, path),
                    before,
                    "{r:?} {path}"
                );
            }
        }
        assert!(refusals > 0); // anti-vacuous: the table exercises refusals too
                               // And the startup refusal keeps its precedence over a /cd entry.
        let list = local();
        crate::cd_deny::append_deny_with(&list, &[p(&t, "secrets")]);
        assert_eq!(
            path_guard_refusal_with(&list, &startups[2], &p(&t, "secrets/a")),
            startups[2].check_path(&p(&t, "secrets/a")).err()
        );
    }

    // --- Day 228: [directories] allow narrows the file tools after /cd ---

    /// A real project dir: `.yoyo.toml` with `toml`, plus `src/a.rs` and
    /// `other/b.rs` on disk, so `check_path` resolves real files.
    fn project(toml: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let (g, t) = tmp();
        std::fs::create_dir_all(t.join("src")).unwrap();
        std::fs::create_dir_all(t.join("other")).unwrap();
        std::fs::write(t.join("src/a.rs"), "fn a() {}\n").unwrap();
        std::fs::write(t.join("other/b.rs"), "fn b() {}\n").unwrap();
        std::fs::write(t.join(".yoyo.toml"), toml).unwrap();
        (g, t)
    }

    #[test]
    fn cd_allow_admits_inside_and_refuses_outside() {
        let (_g, t) = project(DIR_ALLOW_TOML);
        let reading = read_cd_config(&t);
        let (deny, allow) = (local(), local());
        assert_eq!(apply_cd_dir_fence_with(&deny, &allow, &t, &reading), 0);
        // Relative "src" is bound to the /cd target, like deny entries.
        assert_eq!(dir_allow_entries(&t, &reading), vec![p(&t, "src")]);
        let none = DirectoryRestrictions::default();
        let inside = p(&t, "src/a.rs");
        let outside = p(&t, "other/b.rs");
        assert!(Path::new(&inside).is_file() && Path::new(&outside).is_file()); // real files
        assert_eq!(
            path_guard_refusal_lists(&deny, &allow, &none, &inside),
            None
        );
        assert_eq!(
            path_guard_refusal_lists(&deny, &allow, &none, &outside),
            Some(format!(
                "Access denied: '{outside}' is not under any allowed directory"
            ))
        );
        // The note says it was applied, and how long it lasts.
        assert_eq!(
            cd_config_note("/r", &reading, true).unwrap(),
            "  /r/.yoyo.toml: its 1 [directories] allow entry now also fence the file tools: a \
             path outside it is refused until the next /cd replaces this list (the launch allow \
             list still applies too; bash never reads this list). Its [permissions] allow, hooks \
             and MCP servers would still not be reloaded on /cd (#1040)."
        );
        assert!(!cd_dir_fence_is_empty_with(&deny, &allow));
    }

    #[test]
    fn no_cd_allow_leaves_the_refusal_byte_identical() {
        // Near-miss: a /cd target whose config sets deny but no allow. Every
        // verdict and every byte matches the Day-226 deny-only check.
        let (_g, t) = project("[directories]\ndeny = [\"other\"]\n");
        let reading = read_cd_config(&t);
        let (deny, allow) = (local(), local());
        apply_cd_dir_fence_with(&deny, &allow, &t, &reading);
        assert!(snapshot_of(&allow).is_empty());
        let day_226 = DirectoryRestrictions {
            allow: vec![],
            deny: dir_deny_entries(&t, &reading),
        };
        let mut refusals = 0;
        for path in [p(&t, "src/a.rs"), p(&t, "other/b.rs"), p(&t, "x")] {
            let before = day_226.check_path(&path).err();
            refusals += usize::from(before.is_some());
            assert_eq!(cd_dir_refusal_with(&deny, &allow, &path), before, "{path}");
        }
        assert_eq!(refusals, 1); // anti-vacuous: the table holds a refusal
                                 // And a dir with no config at all applies nothing.
        let (_g2, empty) = tmp();
        let (d2, a2) = (local(), local());
        apply_cd_dir_fence_with(&d2, &a2, &empty, &read_cd_config(&empty));
        assert!(cd_dir_fence_is_empty_with(&d2, &a2));
        assert_eq!(cd_dir_refusal_with(&d2, &a2, &p(&t, "other/b.rs")), None);
    }

    #[test]
    fn startup_allow_still_applies_under_a_cd_allow() {
        // Monotonic: a path inside the /cd allow but outside the launch allow
        // is refused by the launch list; both fences must pass.
        let (_g, t) = project(DIR_ALLOW_TOML);
        let (deny, allow) = (local(), local());
        apply_cd_dir_fence_with(&deny, &allow, &t, &read_cd_config(&t));
        let launch = DirectoryRestrictions {
            allow: vec![p(&t, "other")],
            deny: vec![],
        };
        let in_cd_only = p(&t, "src/a.rs");
        assert_eq!(
            path_guard_refusal_lists(&deny, &allow, &launch, &in_cd_only),
            launch.check_path(&in_cd_only).err()
        );
        assert!(launch.check_path(&in_cd_only).is_err()); // anti-vacuous
                                                          // And the reverse: inside the launch allow, outside the /cd allow.
        let in_launch_only = p(&t, "other/b.rs");
        assert!(launch.check_path(&in_launch_only).is_ok());
        assert!(path_guard_refusal_lists(&deny, &allow, &launch, &in_launch_only).is_some());
    }

    #[test]
    fn next_cd_replaces_the_allow_list_and_keeps_deny() {
        let (_g1, d1) = project("[directories]\nallow = [\"src\"]\ndeny = [\"src/secret\"]\n");
        let (_g2, d2) = tmp(); // no .yoyo.toml
        let (deny, allow) = (local(), local());
        apply_cd_dir_fence_with(&deny, &allow, &d1, &read_cd_config(&d1));
        let d1_outside = p(&d1, "other/b.rs");
        let d1_secret = p(&d1, "src/secret/k");
        assert!(cd_dir_refusal_with(&deny, &allow, &d1_outside).is_some());
        assert!(cd_dir_refusal_with(&deny, &allow, &p(&d2, "f")).is_some()); // D1's allow bites D2
        apply_cd_dir_fence_with(&deny, &allow, &d2, &read_cd_config(&d2));
        // D1's allow no longer applies, in D1 or in D2.
        assert!(snapshot_of(&allow).is_empty());
        assert_eq!(cd_dir_refusal_with(&deny, &allow, &d1_outside), None);
        assert_eq!(cd_dir_refusal_with(&deny, &allow, &p(&d2, "f")), None);
        // D1's deny stays, byte-identical to the deny-only refusal.
        assert_eq!(
            cd_dir_refusal_with(&deny, &allow, &d1_secret),
            Some(format!(
                "Access denied: '{d1_secret}' is under restricted directory '{}'",
                p(&d1, "src/secret")
            ))
        );
    }

    fn snapshot_of(list: &DenyAdditions) -> Vec<String> {
        crate::sync_util::lock_or_recover(list).clone()
    }

    /// Production `check_path` readers that do NOT consult the `/cd`-appended
    /// `[directories] deny` list, each with the reason it is still unwired.
    /// Every reason names #869. The placement decision (Day 227) keeps the
    /// `/cd` list BESIDE `DirectoryRestrictions::check_path`, so a reader
    /// nobody wires stays fail-open after `/cd`; this register is the price:
    /// "unwired" is a test failure unless it is written down here. Wiring a
    /// reader must remove its row (a stale row fails the guard too), so the
    /// register's length is the class's remaining size.
    const UNWIRED_CHECK_PATH_READERS: &[(&str, &str)] = &[(
        "src/skill_fence.rs",
        "#869 residue, now #1040: skills are loaded at startup; reloading them after /cd is #1040",
    )];

    /// Every function a `/cd`-aware reader routes through. A `check_path` site
    /// whose enclosing function names one of these consults the `/cd` list.
    fn cd_routing_needles() -> Vec<String> {
        vec![
            ["cd_denied_dir_", "refusal_with("].concat(),
            ["cd_dir_", "refusal_with("].concat(),
            ["path_guard_", "refusal("].concat(),
            ["path_guard_", "refusal_with("].concat(),
        ]
    }

    /// The production part of a source file: everything before the first
    /// `#[cfg(...test...)]` attribute that opens a `mod`. `*_tests.rs` files
    /// are test modules in their own right and have no production part.
    fn production_part(rel: &str, src: &str) -> String {
        if rel.ends_with("_tests.rs") || rel.ends_with("/tests.rs") {
            return String::new();
        }
        let lines: Vec<&str> = src.lines().collect();
        let mut end = lines.len();
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim_start();
            if t.starts_with("#[cfg(") && t.contains("test") {
                let next = lines.get(i + 1).map(|n| n.trim_start()).unwrap_or("");
                if next.starts_with("mod ") || next.starts_with("pub(crate) mod ") {
                    end = i;
                    break;
                }
            }
        }
        lines[..end].join("\n")
    }

    fn rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                rs_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    /// `(file, routed)` for every production `.check_path(` call site in src/.
    /// "Routed" means the text of its enclosing function (from the nearest
    /// preceding `fn ` to the next one) names a `/cd` routing needle.
    fn check_path_sites() -> Vec<(String, bool)> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        rs_files(&root.join("src"), &mut files);
        files.sort();
        let call = [".check_", "path("].concat();
        let needles = cd_routing_needles();
        let mut sites = Vec::new();
        for f in files {
            let rel = f
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let prod = production_part(&rel, &std::fs::read_to_string(&f).unwrap());
            for (at, _) in prod.match_indices(&call) {
                let start = prod[..at].rfind("fn ").unwrap_or(0);
                let end = prod[at..].find("fn ").map_or(prod.len(), |e| at + e);
                let body = &prod[start..end];
                let routed = needles.iter().any(|n| body.contains(n.as_str()));
                sites.push((rel.clone(), routed));
            }
        }
        sites
    }

    /// Deliberately WEAK, repo-wide source guard for #869's placement: no
    /// production `check_path` reader may skip the `/cd` deny list unless its
    /// file is registered with a reason. It proves a routing call is PRESENT
    /// in the same function, never that it fires; and a bare `std::fs` read
    /// with no `check_path` at all is on no list (Day 221's limit).
    #[test]
    fn no_check_path_reader_skips_the_cd_deny_list_unregistered() {
        let sites = check_path_sites();
        // Anti-vacuous: the walk found readers, and the wired ones are among them.
        assert!(sites.len() > 1, "walk found no check_path sites: {sites:?}");
        assert!(
            sites
                .iter()
                .any(|(f, routed)| *routed && f == "src/commands_file_fence.rs"),
            "the file-tool/@path slice must be seen as wired: {sites:?}"
        );
        let registered: Vec<&str> = UNWIRED_CHECK_PATH_READERS.iter().map(|r| r.0).collect();
        for (file, reason) in UNWIRED_CHECK_PATH_READERS {
            assert!(reason.contains("#869"), "{file}: reason must name #869");
        }
        let mut sorted = registered.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), registered.len(), "duplicate register row");
        let unwired: std::collections::BTreeSet<&str> = sites
            .iter()
            .filter(|(_, routed)| !routed)
            .map(|(f, _)| f.as_str())
            .collect();
        for f in &unwired {
            assert!(
                registered.contains(f),
                "{f} calls check_path without consulting the /cd deny list \
                 (route it through cd_config_note, or register it with a #869 reason)"
            );
        }
        for f in &registered {
            assert!(
                unwired.contains(f),
                "stale register row: {f} has no unwired check_path site; remove it"
            );
        }
    }

    /// Deliberately WEAK source guard: proves both path guards in
    /// tool_wrappers.rs (argument check and result fence) call the /cd-aware
    /// refusal and that no direct
    /// startup-only check remains there, never that the call fires.
    #[test]
    fn both_path_guards_consult_the_cd_list() {
        let src = include_str!("tool_wrappers.rs");
        let call = ["cd_config_note::", "path_guard_refusal("].concat();
        // 4 = two argument guards (GuardedTool, ArcGuardedTool) + the two
        // result fences beside them for search/list_files (#1022), which must
        // use this same /cd-aware predicate rather than a second one.
        assert_eq!(
            src.matches(&call).count(),
            4,
            "both guards and both result fences must call it"
        );
        let direct = ["restrictions.", "check_path("].concat();
        assert_eq!(
            src.matches(&direct).count(),
            0,
            "a guard bypasses the /cd list"
        );
    }
}
