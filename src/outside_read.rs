//! Ask-once fence for `read_file`, `search` and `list_files` outside the
//! directory yoyo was launched in (part of #879).
//!
//! **On by default when there is a terminal; off by default when there is
//! not (Day 232).** The switch is the environment variable
//! `YOYO_ASK_OUTSIDE_READS`, read once per process (see [`fence_on`]):
//! exactly `1` forces the fence on, exactly `0` forces it off, and anything
//! else (unset included) means "on iff stdin is a terminal" -- the same test
//! the fence already uses to choose between asking and refusing. The split is
//! by population because the two populations get different things from the
//! fence: an interactive user is asked once (a person is there to answer, and
//! Claude Code asks here too), while a piped/CI run could only be REFUSED,
//! because the fence never prompts without a terminal. So the non-interactive
//! default stays off, and off means [`check`] returns `None` for every call,
//! byte-identical to before. **Superseded claim, recorded rather than erased:**
//! until Day 232 this said the fence was off by default for everyone because
//! "a default that asks would break every piped/CI user with no terminal to
//! answer (#448)". That mechanism was false of this code (no terminal means a
//! refusal, not a hang); what was true is that a default-on fence would
//! refuse reads scripts rely on, which is why that population stays off.
//!
//! **The anchor is the LAUNCH directory, not the live cwd (Day 231).** The
//! property this fence protects is consent: the user agreed to reads under the
//! tree they launched yoyo in. That is provenance (who chose this tree), not
//! state (where the session happens to stand), and `/cd` is navigation, not
//! consent to read everything under the new directory. Until Day 231 the
//! fence compared against `current_dir()` at each call, so after `/cd ~` all
//! of `~`, `~/.ssh` included, counted as inside and nothing was asked.
//! Claude Code anchors on the launch dir too. So `main` records the cwd once,
//! canonicalized, before any tool runs ([`record_launch_dir`]), and that is
//! the anchor; the live cwd is a fallback only when nothing was recorded.
//! Approvals already given persist across `/cd` unchanged, so `/cd` never
//! makes an approved path ask again. The cost, stated: after `/cd` to a
//! directory outside the launch tree, reads there ask, once per directory
//! (see the remembered keys below; a yes does not cover subdirectories).
//!
//! **Fails closed when it cannot tell.** No recorded launch dir and an
//! unreadable cwd (deleted directory) means no anchor, and then every
//! resolvable target is outside, so it is ASKED about, never silently
//! allowed. A relative path with an unreadable cwd cannot be resolved at all,
//! so it is refused with a request for an absolute path. (Until Day 231
//! `current_dir().ok()?` returned "proceed" here.)
//!
//! **What "outside" means.** The target is resolved with `fs::canonicalize`
//! (symlinks followed, `..` removed; a relative path is joined to the current
//! directory first, where the tool itself resolves it), the same resolution
//! the directory fence uses for #996, and compared against the canonicalized
//! launch directory. A symlink inside the launch directory that points outside
//! it is outside. A path that cannot be resolved (it does not exist) is not
//! asked about: `read_file` reports its own error, and there is nothing to read.
//!
//! **Which tools, and which argument.** `read_file`'s `path`, and the ROOT
//! (`path`) of `search` and `list_files`. A missing root means the tool's
//! default, the current directory, and is checked as that directory at call
//! time (see `effective_root`), so after `/cd ~` an unrooted search asks about
//! `~` instead of walking it unasked (#879).
//!
//! **Ask once per directory, and the remembered key.** A yes is remembered for
//! the rest of the session as an [`Approval`]:
//! - a FILE target (any `read_file`, or a `search`/`list_files` root that is a
//!   file) is remembered as `Files(parent)`, the canonical parent directory,
//!   exactly, so three files in `/etc` ask once, and `search` on one of them
//!   rides the same yes;
//! - a DIRECTORY root of `search`/`list_files` is remembered as `Tree(dir)`,
//!   the canonical directory itself, exactly. Asking about `/a/b` does not
//!   cover a later root at `/a/b/c` (that asks again), the same exact-directory
//!   rule `read_file` uses.
//!
//! The two kinds never cover each other, because both tools walk recursively:
//! a yes to reading `~/.bashrc` (`Files(~)`) must not quietly grant a search
//! rooted at `~`, which would reach `~/.ssh`. Neither kind covers
//! subdirectories. A no refuses this call only and is not remembered, so the
//! next call there asks again.
//!
//! **No terminal, no prompt.** When stdin is not a terminal (`-p` in a pipe,
//! CI) the read is refused, and the refusal names the path and the ways to
//! allow it. The model receives it as the tool result. It carries the
//! `REFUSAL_STEM_MODE_ACTIVE` stem so the recovery-hint wrapper treats it as a
//! deliberate refusal rather than a flaky tool (#710).
//!
//! **`--allow-dir` is consent.** When the startup `--allow-dir` list is set, a
//! path has already passed it by the time this runs, so the user named that
//! directory and nothing is asked.
//!
//! **Not a sandbox.** This fences the three read tools' path argument only.
//! `bash` can still `cat` the same file, and a search rooted inside the launch directory
//! still follows whatever its own walker follows. `--restricted` and
//! `--allow-dir` remain the hard fences.
//!
//! **The write fence (Day 245) lives here too, and is a different animal.**
//! `YOYO_BLOCK_OUTSIDE_WRITES=1` (exactly `1`, read once per process) makes
//! `write_file` and `edit_file` REFUSE any target outside the launch
//! directory, in every mode: `--yes`, auto-edit and an "always" answer do not
//! reach it, and it never asks, because a hard block that a flag can skip is
//! the confirm prompt again. **Opt-in, for a user-facing reason:** a hard
//! refusal of every outside write breaks workflows that legitimately write
//! elsewhere (a generated file in a sibling repo, `/tmp` scratch), and the
//! reference (Claude Code's `blockWritesOutsideWorkingDirectories`, shipped
//! 2026-10-30) is opt-in too. An env var matches this file's read switch; a
//! `.yoyo.toml` key is a possible follow-up. **Anchor:** the same launch dir
//! ([`record_launch_dir`], recorded once at startup in `main`, never moved by
//! `/cd`), read at each call; "inside" is the one shared [`is_inside`]. The
//! TARGET resolves against the cwd at call time, where the tool really writes;
//! a target that does not exist yet canonicalizes its nearest existing
//! ancestor and appends the rest (`write_file` creates missing parents), so a
//! symlink inside pointing out is outside. Fails closed: a `..` or a dangling
//! symlink in the not-yet-existing part is refused (cost, stated: `new/../f`,
//! which would land inside, is refused too). `--allow-dir` is consent, as for
//! reads. **Not covered, plainly:** `bash` (it can still `echo > /anywhere`),
//! and `rename_symbol`, which takes no write target: it rewrites git-tracked
//! files listed from the current directory, so it reaches outside the launch
//! dir only after `/cd` into another tree.
//!
//! **Why not `confirm_file_operation`.** That helper's `a`/always answer flips a
//! session-wide auto-approve flag and offers to write a `[permissions] allow`
//! pattern to `.yoyo.toml`, neither of which means anything for a read fence,
//! so the prompt here is a plain y/n in the same visual style.

use std::collections::HashSet;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};

use crate::format::{GREEN, RED, RESET, YELLOW};

/// The opt-in switch.
pub(crate) const ENV_VAR: &str = "YOYO_ASK_OUTSIDE_READS";

/// The tools whose `path` argument this fence reads.
const FENCED_TOOLS: &[&str] = &["read_file", "search", "list_files"];

static ENABLED: OnceLock<bool> = OnceLock::new();

/// What a yes covers for the rest of the session (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Approval {
    /// Files directly in this canonical directory (a file target's parent).
    Files(PathBuf),
    /// A recursive `search`/`list_files` rooted at exactly this canonical directory.
    Tree(PathBuf),
}

impl Approval {
    fn dir(&self) -> &Path {
        match self {
            Approval::Files(d) | Approval::Tree(d) => d,
        }
    }
}

/// Approvals granted this session.
static APPROVED: LazyLock<Mutex<HashSet<Approval>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Only the exact value `1` forces the fence on.
fn enabled_from(value: Option<&str>) -> bool {
    value == Some("1")
}

/// Only the exact value `0` forces the fence off.
fn disabled_from(value: Option<&str>) -> bool {
    value == Some("0")
}

/// Is the fence on? `1` forces it on and `0` forces it off. Anything else
/// (unset, empty, a typo) takes the default, which splits by population:
/// on when there is a terminal to ask on, off when there is not. Off for a
/// non-interactive run is byte-identical to before Day 232, because turning
/// it on there could only REFUSE reads that work today (it never prompts
/// without a terminal, see `decide`), and a silent new refusal in a script
/// or CI job is the breakage to avoid. `interactive` is the same test the
/// fence uses to choose between asking and refusing.
fn fence_on(value: Option<&str>, interactive: bool) -> bool {
    if enabled_from(value) {
        return true;
    }
    if disabled_from(value) {
        return false;
    }
    interactive
}

fn is_enabled() -> bool {
    *ENABLED.get_or_init(|| {
        // Under `cargo test` stdin is the developer's terminal when run by
        // hand; a test that routes an outside read through a guarded tool
        // must not stop and wait for a y/n, so the default is the
        // non-interactive one there. The decision itself is table-tested
        // through `fence_on`.
        let interactive = !cfg!(test) && io::stdin().is_terminal();
        fence_on(std::env::var(ENV_VAR).ok().as_deref(), interactive)
    })
}

/// The directory yoyo was launched in, canonicalized, recorded once at
/// startup by [`record_launch_dir`] before any tool runs. `/cd` never moves it.
static LAUNCH_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Record the launch directory (the cwd at startup). First writer wins, so a
/// later `/cd` cannot re-anchor the fence. Called once from `main`.
pub(crate) fn record_launch_dir() {
    if let Ok(cwd) = std::env::current_dir() {
        let dir = std::fs::canonicalize(&cwd).unwrap_or(cwd);
        let _ = LAUNCH_DIR.set(dir);
    }
}

/// The fence's anchor: the recorded launch dir, else the cwd (tests and any
/// caller that never recorded one), else `None`, which [`resolve_outside`]
/// treats as "everything is outside" (fail closed). Pure.
fn anchor_from(recorded: Option<&Path>, cwd: Option<&Path>) -> Option<PathBuf> {
    recorded.or(cwd).map(Path::to_path_buf)
}

/// The canonical target when `path` resolves to somewhere outside `anchor`;
/// `None` when it is inside, or when it cannot be resolved at all. A relative
/// `path` is joined to `cwd` (where the tool itself will resolve it), but the
/// inside-test is against `anchor`, the launch dir. No anchor means no inside:
/// every resolvable target is outside, so it is asked about, never allowed.
fn resolve_outside(path: &str, cwd: Option<&Path>, anchor: Option<&Path>) -> Option<PathBuf> {
    let p = Path::new(path);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd?.join(p)
    };
    let target = std::fs::canonicalize(joined).ok()?;
    if is_inside(&target, anchor) {
        None
    } else {
        Some(target)
    }
}

/// THE definition of "inside", shared by the read fence and the write fence
/// so the two can never disagree: the canonical `target` is under the
/// canonicalized `anchor`. No anchor means nothing is inside (fail closed).
fn is_inside(target: &Path, anchor: Option<&Path>) -> bool {
    let Some(anchor) = anchor else {
        return false;
    };
    let root = std::fs::canonicalize(anchor).unwrap_or_else(|_| anchor.to_path_buf());
    target.starts_with(&root)
}

/// What a yes for `tool` on the canonical `target` is remembered as.
/// `read_file` is always a file read; a `search`/`list_files` root is a
/// `Tree` when it is a directory and a file read otherwise.
fn approval_for(tool: &str, target: &Path) -> Approval {
    if tool != "read_file" && target.is_dir() {
        return Approval::Tree(target.to_path_buf());
    }
    Approval::Files(
        target
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| target.to_path_buf()),
    )
}

/// How the refusals and the prompt name the anchor.
fn anchor_phrase(anchor: Option<&Path>) -> String {
    match anchor {
        Some(a) => format!("the directory yoyo was launched in '{}'", a.display()),
        None => "the directory yoyo was launched in (which could not be determined)".to_string(),
    }
}

/// The refusal a non-interactive run gets.
fn no_tty_refusal(tool: &str, target: &Path, anchor: Option<&Path>, approval: &Approval) -> String {
    let (holder, named) = match approval {
        Approval::Files(_) => ("the file", "its directory"),
        Approval::Tree(_) => ("that directory", "that directory"),
    };
    format!(
        "{ENV_VAR}=1{stem}{tool} refused '{}': it is outside {}, \
         and there is no terminal to ask on. To allow it, unset {ENV_VAR}, run yoyo from a \
         directory that contains {holder}, or name {named} with --allow-dir.",
        target.display(),
        anchor_phrase(anchor),
        stem = crate::tool_wrappers::REFUSAL_STEM_MODE_ACTIVE,
    )
}

/// The refusal for a relative path when the current directory cannot be
/// read: there is no target to ask about, so refuse rather than allow.
fn no_cwd_refusal(tool: &str, path: &str) -> String {
    format!(
        "The outside-read fence ({ENV_VAR}){stem}{tool} refused '{path}': the current \
         directory cannot be read, so this relative path cannot be checked against the \
         launch directory. Use an absolute path.",
        stem = crate::tool_wrappers::REFUSAL_STEM_MODE_ACTIVE,
    )
}

/// The refusal an interactive "no" gets.
fn declined_refusal(tool: &str, target: &Path) -> String {
    format!(
        "{}{tool} on '{}' (outside the directory yoyo was launched in)",
        crate::tool_wrappers::REFUSAL_STEM_PATH_DENIED,
        target.display()
    )
}

/// The whole decision, with every input injected. `None` = proceed.
#[allow(clippy::too_many_arguments)]
fn decide(
    enabled: bool,
    tool: &str,
    path: &str,
    explicit_allow: bool,
    cwd: Option<&Path>,
    anchor: Option<&Path>,
    store: &Mutex<HashSet<Approval>>,
    is_tty: bool,
    ask: &mut dyn FnMut(&str, &Path, &Approval) -> bool,
) -> Option<String> {
    if !enabled || !FENCED_TOOLS.contains(&tool) || explicit_allow {
        return None;
    }
    if cwd.is_none() && Path::new(path).is_relative() {
        return Some(no_cwd_refusal(tool, path));
    }
    let target = resolve_outside(path, cwd, anchor)?;
    let approval = approval_for(tool, &target);
    // Held across the prompt so two parallel calls cannot both ask.
    let mut approved = crate::sync_util::lock_or_recover(store);
    if approved.contains(&approval) {
        return None;
    }
    if !is_tty {
        return Some(no_tty_refusal(tool, &target, anchor, &approval));
    }
    if ask(tool, &target, &approval) {
        approved.insert(approval);
        None
    } else {
        Some(declined_refusal(tool, &target))
    }
}

/// Ask on stderr, read one line from stdin.
fn ask_on_terminal(tool: &str, target: &Path, approval: &Approval) -> bool {
    let target = crate::cli::sanitize_for_display(&target.display().to_string());
    let dir = crate::cli::sanitize_for_display(&approval.dir().display().to_string());
    let scope = match approval {
        Approval::Files(_) => format!("reads in {dir}"),
        Approval::Tree(_) => format!("{tool} over all of {dir} (recursive)"),
    };
    eprint!(
        "{YELLOW}  ⚠ {tool} wants '{target}', outside the directory yoyo was launched in. Allow {scope} this session? {RESET}({GREEN}y{RESET}/{RED}n{RESET}) "
    );
    io::stderr().flush().ok();
    let mut response = String::new();
    use std::io::BufRead;
    if io::stdin().lock().read_line(&mut response).is_err() {
        return false;
    }
    matches!(response.trim().to_lowercase().as_str(), "y" | "yes")
}

/// The path the tool will actually act on. `search`/`list_files` with no
/// `path` (absent, or not a string) walk the current directory: yoagent
/// 0.25.7's `SearchTool` falls back to its `root` (yoyo builds it with
/// `SearchTool::default()`, so `None`) and then `"."`, and `ListFilesTool`
/// uses `params["path"].as_str().unwrap_or(".")`. So a missing root is
/// checked as `"."`, which `decide` resolves against the cwd at call time
/// (#879). `read_file` with no path reads nothing (the tool reports the
/// missing argument), so there is nothing to check.
fn effective_root<'a>(tool: &str, path: Option<&'a str>) -> Option<&'a str> {
    match path {
        Some(p) => Some(p),
        None if tool == "search" || tool == "list_files" => Some("."),
        None => None,
    }
}

// ---------------------------------------------------------------------------
// The write fence (Day 245): opt-in, hard refusal, never asks.
// ---------------------------------------------------------------------------

/// The write fence's opt-in switch. Only the exact value `1` turns it on.
pub(crate) const WRITE_ENV_VAR: &str = "YOYO_BLOCK_OUTSIDE_WRITES";

/// The tools whose `path` argument the write fence reads.
const WRITE_FENCED_TOOLS: &[&str] = &["write_file", "edit_file"];

static WRITE_ENABLED: OnceLock<bool> = OnceLock::new();

#[cfg(test)]
thread_local! {
    /// Test-only replacement for the write fence's two process-global INPUTS
    /// (the env switch and the recorded launch dir), so a wiring test can turn
    /// the fence on for one `#[tokio::test]` (current-thread) without touching
    /// the process env that every other test shares. The decision itself still
    /// runs through [`decide_write`]; only where its inputs come from differs.
    static WRITE_FENCE_TEST_INPUTS: std::cell::RefCell<Option<(bool, PathBuf)>> =
        const { std::cell::RefCell::new(None) };
}

/// The write fence's (enabled, recorded launch dir) for this call.
fn write_fence_inputs() -> (bool, Option<PathBuf>) {
    #[cfg(test)]
    if let Some((on, anchor)) = WRITE_FENCE_TEST_INPUTS.with(|c| c.borrow().clone()) {
        return (on, Some(anchor));
    }
    let on =
        *WRITE_ENABLED.get_or_init(|| enabled_from(std::env::var(WRITE_ENV_VAR).ok().as_deref()));
    (on, LAUNCH_DIR.get().cloned())
}

/// Where a write to `path` would really land, canonically, even when the
/// file (or several of its parent directories) does not exist yet:
/// `write_file` creates missing parents. A relative `path` is joined to `cwd`
/// (where the tool resolves it). The nearest EXISTING ancestor is
/// canonicalized (symlinks followed, so `inside/link_to_tmp/f` resolves into
/// `/tmp`) and the not-yet-existing tail is appended. `None` = cannot be
/// resolved safely, which the caller treats as outside: a `..` in the
/// not-yet-existing tail, or an entry that exists but cannot be
/// canonicalized (a dangling symlink, which the write would follow).
fn resolve_write_target(path: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    let p = Path::new(path);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd?.join(p)
    };
    let mut existing = joined.as_path();
    let mut tail = Vec::new();
    loop {
        if let Ok(canon) = std::fs::canonicalize(existing) {
            let mut out = canon;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return Some(out);
        }
        if std::fs::symlink_metadata(existing).is_ok() {
            return None;
        }
        tail.push(existing.file_name()?.to_os_string());
        existing = existing.parent()?;
    }
}

/// The refusal a fenced write gets. Carries `REFUSAL_STEM_MODE_ACTIVE`, like
/// the read fence's refusal, so the recovery-hint wrapper treats it as a
/// deliberate refusal rather than a flaky tool (#710).
fn write_refusal(tool: &str, shown: &str, why: &str) -> String {
    format!(
        "{WRITE_ENV_VAR}=1{stem}{tool} refused '{shown}': {why}. This fence applies in every \
         mode, --yes included, and never asks. To allow it, unset {WRITE_ENV_VAR}, run yoyo \
         from a directory that contains the target, or list its directory with --allow-dir \
         (which then limits every file tool to the listed directories).",
        stem = crate::tool_wrappers::REFUSAL_STEM_MODE_ACTIVE,
    )
}

/// The whole write-fence decision, every input injected. `None` = proceed.
fn decide_write(
    enabled: bool,
    tool: &str,
    path: &str,
    explicit_allow: bool,
    cwd: Option<&Path>,
    anchor: Option<&Path>,
) -> Option<String> {
    if !enabled || !WRITE_FENCED_TOOLS.contains(&tool) || explicit_allow {
        return None;
    }
    if cwd.is_none() && Path::new(path).is_relative() {
        let why = "the current directory cannot be read, so this relative path cannot be \
                   checked against the launch directory; use an absolute path";
        return Some(write_refusal(tool, path, why));
    }
    match resolve_write_target(path, cwd) {
        Some(target) if is_inside(&target, anchor) => None,
        Some(target) => {
            let why = format!("it is outside {}", anchor_phrase(anchor));
            Some(write_refusal(tool, &target.display().to_string(), &why))
        }
        None => {
            let why = format!(
                "it cannot be resolved safely (a '..' or a dangling symlink in a part that does \
                 not exist yet), so it is treated as outside {}",
                anchor_phrase(anchor)
            );
            Some(write_refusal(tool, path, &why))
        }
    }
}

/// The write fence's entry point, reached through [`check`].
fn check_write(
    tool: &str,
    path: Option<&str>,
    restrictions: &crate::cli::DirectoryRestrictions,
) -> Option<String> {
    let (enabled, recorded) = write_fence_inputs();
    if !enabled {
        return None;
    }
    // No path: the tool reports the missing argument and writes nothing.
    let path = path?;
    let cwd = std::env::current_dir().ok();
    let anchor = anchor_from(recorded.as_deref(), cwd.as_deref());
    decide_write(
        true,
        tool,
        path,
        !restrictions.allow.is_empty(),
        cwd.as_deref(),
        anchor.as_deref(),
    )
}

/// The wrapper's entry point: `None` = proceed, `Some(refusal)` = refuse.
/// `path` is the call's `path` argument as a string, if any. Write tools go
/// to the write fence; everything else to the read fence. The two sets are
/// disjoint, so each fence is byte-identical to running alone.
pub(crate) fn check(
    tool: &str,
    path: Option<&str>,
    restrictions: &crate::cli::DirectoryRestrictions,
) -> Option<String> {
    if WRITE_FENCED_TOOLS.contains(&tool) {
        return check_write(tool, path, restrictions);
    }
    if !is_enabled() {
        return None;
    }
    let path = effective_root(tool, path)?;
    // A deleted or unreadable cwd is not a reason to allow: `None` flows on
    // and fails closed (see `resolve_outside` and `no_cwd_refusal`).
    let cwd = std::env::current_dir().ok();
    let anchor = anchor_from(LAUNCH_DIR.get().map(PathBuf::as_path), cwd.as_deref());
    decide(
        true,
        tool,
        path,
        !restrictions.allow.is_empty(),
        cwd.as_deref(),
        anchor.as_deref(),
        &APPROVED,
        io::stdin().is_terminal(),
        &mut ask_on_terminal,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A temp tree: `<root>/work` (the cwd), `<root>/a/{x,y}`, `<root>/b/z`.
    fn tree() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let work = root.join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("inside.txt"), "in").unwrap();
        for d in ["a", "b"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        for f in ["a/x", "a/y", "b/z", "outside.txt"] {
            std::fs::write(root.join(f), "out").unwrap();
        }
        (tmp, root, work)
    }

    fn store() -> Mutex<HashSet<Approval>> {
        Mutex::new(HashSet::new())
    }

    #[test]
    fn enabled_only_by_exact_one() {
        assert!(enabled_from(Some("1")));
        for v in [None, Some(""), Some("0"), Some("true"), Some(" 1")] {
            assert!(!enabled_from(v), "{v:?} must not enable the fence");
        }
    }

    /// Every env state x both populations. The near-miss that matters is
    /// unset + non-interactive = off: every piped `-p`, CI job and yoyo's own
    /// evolve loop behaves exactly as before Day 232.
    #[test]
    fn fence_on_by_env_and_population() {
        let rows: &[(Option<&str>, bool, bool)] = &[
            (Some("1"), true, true),
            (Some("1"), false, true),
            (Some("0"), true, false),
            (Some("0"), false, false),
            (None, true, true),
            (None, false, false),
            // Unrecognised values take the default, like unset.
            (Some(""), true, true),
            (Some(""), false, false),
            (Some("true"), false, false),
            (Some("yes"), true, true),
        ];
        for &(value, interactive, want) in rows {
            assert_eq!(
                fence_on(value, interactive),
                want,
                "env {value:?}, interactive={interactive}"
            );
        }
    }

    #[test]
    fn interactive_unset_is_on() {
        assert!(
            fence_on(None, true),
            "a terminal user who chose nothing is asked"
        );
    }

    #[test]
    fn non_interactive_unset_is_off() {
        assert!(!fence_on(None, false), "piped/CI runs are unchanged");
    }

    /// The no-cwd refusal is reachable with the env unset (interactive
    /// default), so it must not claim `YOYO_ASK_OUTSIDE_READS=1`.
    #[test]
    fn no_cwd_refusal_does_not_claim_the_env_was_set() {
        let msg = no_cwd_refusal("read_file", "x");
        assert!(!msg.contains("=1"), "{msg}");
        assert!(
            msg.contains(crate::tool_wrappers::REFUSAL_STEM_MODE_ACTIVE),
            "{msg}"
        );
        assert!(msg.contains("read_file refused 'x'"), "{msg}");
    }

    #[test]
    fn predicate_inside_outside_symlink_and_missing() {
        let (_tmp, root, work) = tree();
        assert_eq!(
            resolve_outside("inside.txt", Some(&work), Some(&work)),
            None
        );
        assert_eq!(
            resolve_outside(
                work.join("inside.txt").to_str().unwrap(),
                Some(&work),
                Some(&work)
            ),
            None
        );
        assert_eq!(
            resolve_outside("../outside.txt", Some(&work), Some(&work)),
            Some(root.join("outside.txt"))
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("outside.txt"), work.join("link")).unwrap();
            assert_eq!(
                resolve_outside("link", Some(&work), Some(&work)),
                Some(root.join("outside.txt")),
                "a symlink inside the cwd that points outside is outside"
            );
        }
        assert_eq!(
            resolve_outside("../does-not-exist", Some(&work), Some(&work)),
            None
        );
    }

    #[test]
    fn nonexistent_path_is_not_asked() {
        let (_tmp, _root, work) = tree();
        let asked = Cell::new(0);
        let mut ask = |_: &str, _: &Path, _: &Approval| {
            asked.set(asked.get() + 1);
            true
        };
        let s = store();
        let out = decide(
            true,
            "read_file",
            "../nope",
            false,
            Some(&work),
            Some(&work),
            &s,
            true,
            &mut ask,
        );
        assert_eq!(out, None);
        assert_eq!(asked.get(), 0);
    }

    #[test]
    fn ask_once_per_parent_directory() {
        let (_tmp, root, work) = tree();
        let s = store();
        let asked = std::cell::RefCell::new(Vec::new());
        let mut ask = |_: &str, _: &Path, a: &Approval| {
            asked.borrow_mut().push(a.dir().to_path_buf());
            true
        };
        for p in ["../a/x", "../a/y", "../b/z"] {
            assert_eq!(
                decide(
                    true,
                    "read_file",
                    p,
                    false,
                    Some(&work),
                    Some(&work),
                    &s,
                    true,
                    &mut ask
                ),
                None
            );
        }
        assert_eq!(*asked.borrow(), vec![root.join("a"), root.join("b")]);
    }

    #[test]
    fn a_no_refuses_and_is_not_remembered() {
        let (_tmp, root, work) = tree();
        let s = store();
        let asked = Cell::new(0);
        let mut no = |_: &str, _: &Path, _: &Approval| {
            asked.set(asked.get() + 1);
            false
        };
        let out = decide(
            true,
            "read_file",
            "../a/x",
            false,
            Some(&work),
            Some(&work),
            &s,
            true,
            &mut no,
        );
        assert_eq!(
            out,
            Some(format!(
                "User denied read_file on '{}' (outside the directory yoyo was launched in)",
                root.join("a/x").display()
            ))
        );
        decide(
            true,
            "read_file",
            "../a/x",
            false,
            Some(&work),
            Some(&work),
            &s,
            true,
            &mut no,
        );
        assert_eq!(asked.get(), 2, "a no must ask again next time");
        assert!(crate::tool_wrappers::is_deterministic_refusal(
            &out.unwrap()
        ));
    }

    #[test]
    fn non_interactive_refusal_full_text_and_no_ansi() {
        let cwd = Path::new("/work/proj");
        let msg = no_tty_refusal(
            "read_file",
            Path::new("/etc/hosts"),
            Some(cwd),
            &Approval::Files(PathBuf::from("/etc")),
        );
        assert_eq!(
            msg,
            "YOYO_ASK_OUTSIDE_READS=1 is active — read_file refused '/etc/hosts': it is outside \
             the directory yoyo was launched in '/work/proj', and there is no terminal to ask on. To allow it, \
             unset YOYO_ASK_OUTSIDE_READS, run yoyo from a directory that contains the file, or \
             name its directory with --allow-dir."
        );
        assert!(!msg.as_bytes().contains(&0x1b));
        assert!(crate::tool_wrappers::is_deterministic_refusal(&msg));
    }

    #[test]
    fn non_interactive_refuses_without_asking() {
        let (_tmp, root, work) = tree();
        let s = store();
        let mut ask =
            |_: &str, _: &Path, _: &Approval| -> bool { panic!("must not prompt without a tty") };
        let out = decide(
            true,
            "read_file",
            "../outside.txt",
            false,
            Some(&work),
            Some(&work),
            &s,
            false,
            &mut ask,
        );
        assert_eq!(
            out,
            Some(no_tty_refusal(
                "read_file",
                &root.join("outside.txt"),
                Some(work.as_path()),
                &Approval::Files(root.clone())
            ))
        );
        // An inside read is untouched even with no tty.
        assert_eq!(
            decide(
                true,
                "read_file",
                "inside.txt",
                false,
                Some(&work),
                Some(&work),
                &s,
                false,
                &mut ask
            ),
            None
        );
    }

    #[test]
    fn disabled_other_tools_and_allow_dir_are_untouched() {
        let (_tmp, _root, work) = tree();
        let s = store();
        let mut ask = |_: &str, _: &Path, _: &Approval| -> bool { panic!("must not prompt") };
        // Disabled: the regression surface, every user who has not opted in.
        assert_eq!(
            decide(
                false,
                "read_file",
                "../outside.txt",
                false,
                Some(&work),
                Some(&work),
                &s,
                false,
                &mut ask
            ),
            None
        );
        // Disabled is a passthrough for the two root-taking tools too.
        for tool in ["search", "list_files"] {
            assert_eq!(
                decide(
                    false,
                    tool,
                    "../a",
                    false,
                    Some(&work),
                    Some(&work),
                    &s,
                    false,
                    &mut ask
                ),
                None
            );
        }
        // Writes and anything unnamed are a different fence.
        for tool in ["write_file", "edit_file", "rename_symbol", "bash"] {
            assert_eq!(
                decide(
                    true,
                    tool,
                    "../outside.txt",
                    false,
                    Some(&work),
                    Some(&work),
                    &s,
                    false,
                    &mut ask
                ),
                None
            );
        }
        // --allow-dir already passed: consent given.
        assert_eq!(
            decide(
                true,
                "read_file",
                "../outside.txt",
                true,
                Some(&work),
                Some(&work),
                &s,
                false,
                &mut ask
            ),
            None
        );
    }

    /// Records every prompt as (tool, approval) and answers `yes`.
    fn recorder(
        log: &std::cell::RefCell<Vec<(String, Approval)>>,
        yes: bool,
    ) -> impl FnMut(&str, &Path, &Approval) -> bool + '_ {
        move |tool: &str, _: &Path, a: &Approval| {
            log.borrow_mut().push((tool.to_string(), a.clone()));
            yes
        }
    }

    #[test]
    fn search_and_list_files_outside_roots_ask_or_refuse() {
        let (_tmp, root, work) = tree();
        for tool in ["search", "list_files"] {
            // No tty: refused, naming the tool, the canonical root and --allow-dir.
            let s = store();
            let mut never = |_: &str, _: &Path, _: &Approval| -> bool { panic!("no tty") };
            let out = decide(
                true,
                tool,
                "../a",
                false,
                Some(&work),
                Some(&work),
                &s,
                false,
                &mut never,
            );
            assert_eq!(
                out,
                Some(format!(
                    "YOYO_ASK_OUTSIDE_READS=1 is active — {tool} refused '{}': it is outside \
                     the directory yoyo was launched in '{}', and there is no terminal to ask on. To allow \
                     it, unset YOYO_ASK_OUTSIDE_READS, run yoyo from a directory that contains \
                     that directory, or name that directory with --allow-dir.",
                    root.join("a").display(),
                    work.display()
                ))
            );
            assert!(crate::tool_wrappers::is_deterministic_refusal(
                out.as_deref().unwrap()
            ));
            // Tty: asked once, keyed on the directory itself, as a Tree.
            let log = std::cell::RefCell::new(Vec::new());
            let mut ask = recorder(&log, true);
            assert_eq!(
                decide(
                    true,
                    tool,
                    "../a",
                    false,
                    Some(&work),
                    Some(&work),
                    &s,
                    true,
                    &mut ask
                ),
                None
            );
            assert_eq!(
                decide(
                    true,
                    tool,
                    "../a",
                    false,
                    Some(&work),
                    Some(&work),
                    &s,
                    true,
                    &mut ask
                ),
                None
            );
            assert_eq!(
                *log.borrow(),
                vec![(tool.to_string(), Approval::Tree(root.join("a")))]
            );
            // A no names the tool and is not remembered.
            let s2 = store();
            let mut no = |_: &str, _: &Path, _: &Approval| false;
            assert_eq!(
                decide(
                    true,
                    tool,
                    "../b",
                    false,
                    Some(&work),
                    Some(&work),
                    &s2,
                    true,
                    &mut no
                ),
                Some(format!(
                    "User denied {tool} on '{}' (outside the directory yoyo was launched in)",
                    root.join("b").display()
                ))
            );
        }
    }

    #[test]
    fn inside_roots_pass_untouched() {
        let (_tmp, _root, work) = tree();
        std::fs::create_dir_all(work.join("sub")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(work.join("sub"), work.join("inlink")).unwrap();
        let s = store();
        let mut never = |_: &str, _: &Path, _: &Approval| -> bool { panic!("inside: no prompt") };
        // `.` is also what a missing root means (the tools default to the cwd).
        let mut roots = vec![".", "sub", "inside.txt"];
        if cfg!(unix) {
            roots.push("inlink");
        }
        for tool in ["search", "list_files"] {
            for r in &roots {
                assert_eq!(
                    decide(
                        true,
                        tool,
                        r,
                        false,
                        Some(&work),
                        Some(&work),
                        &s,
                        false,
                        &mut never
                    ),
                    None,
                    "{tool} rooted at {r} is inside"
                );
            }
            // A nonexistent root is not asked about: the tool reports it.
            assert_eq!(
                decide(
                    true,
                    tool,
                    "../nope",
                    false,
                    Some(&work),
                    Some(&work),
                    &s,
                    false,
                    &mut never
                ),
                None
            );
            // --allow-dir already passed: consent given.
            assert_eq!(
                decide(
                    true,
                    tool,
                    "../a",
                    true,
                    Some(&work),
                    Some(&work),
                    &s,
                    false,
                    &mut never
                ),
                None
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn root_symlink_inside_pointing_outside_is_outside() {
        let (_tmp, root, work) = tree();
        std::os::unix::fs::symlink(root.join("a"), work.join("escape")).unwrap();
        let log = std::cell::RefCell::new(Vec::new());
        let mut ask = recorder(&log, true);
        let s = store();
        assert_eq!(
            decide(
                true,
                "search",
                "escape",
                false,
                Some(&work),
                Some(&work),
                &s,
                true,
                &mut ask
            ),
            None
        );
        assert_eq!(
            *log.borrow(),
            vec![("search".to_string(), Approval::Tree(root.join("a")))],
            "canonicalized: the prompt and the key name the real directory"
        );
    }

    #[test]
    fn remembered_keys_are_exact_and_kinds_do_not_cover_each_other() {
        let (_tmp, root, work) = tree();
        std::fs::create_dir_all(root.join("a/deeper")).unwrap();
        let s = store();
        let log = std::cell::RefCell::new(Vec::new());
        let mut ask = recorder(&log, true);
        // A read of a file in `a` approves Files(a)...
        decide(
            true,
            "read_file",
            "../a/x",
            false,
            Some(&work),
            Some(&work),
            &s,
            true,
            &mut ask,
        );
        // ...and a search whose root is a FILE in `a` rides it (parent key)...
        decide(
            true,
            "search",
            "../a/y",
            false,
            Some(&work),
            Some(&work),
            &s,
            true,
            &mut ask,
        );
        // ...but a recursive search ROOTED at `a` is a different grant.
        decide(
            true,
            "search",
            "../a",
            false,
            Some(&work),
            Some(&work),
            &s,
            true,
            &mut ask,
        );
        // A Tree yes for `a` covers list_files at `a` (same key)...
        decide(
            true,
            "list_files",
            "../a",
            false,
            Some(&work),
            Some(&work),
            &s,
            true,
            &mut ask,
        );
        // ...and not a root one level down.
        decide(
            true,
            "list_files",
            "../a/deeper",
            false,
            Some(&work),
            Some(&work),
            &s,
            true,
            &mut ask,
        );
        assert_eq!(
            *log.borrow(),
            vec![
                ("read_file".to_string(), Approval::Files(root.join("a"))),
                ("search".to_string(), Approval::Tree(root.join("a"))),
                (
                    "list_files".to_string(),
                    Approval::Tree(root.join("a/deeper"))
                ),
            ]
        );
        // read_file is a file read even when handed a directory.
        assert_eq!(
            approval_for("read_file", &root.join("a")),
            Approval::Files(root.clone())
        );
    }

    /// #879: the anchor is the LAUNCH dir, not the live cwd. Two calls, as
    /// after `/cd`: the cwd has moved to `h`, the anchor is still `l`. A file
    /// under `h` is OUTSIDE (asked), although under the old rule (anchor ==
    /// cwd) it was inside and nothing was asked, which is `~/.ssh` after `/cd ~`.
    #[test]
    fn anchor_is_the_launch_dir_not_the_cwd_after_cd() {
        let (_tmp, root, work) = tree();
        let (l, h) = (work.clone(), root.join("a"));
        let s = store();
        let log = std::cell::RefCell::new(Vec::new());
        let mut ask = recorder(&log, false);
        // Old rule, stated so the test names the change: cwd-anchored, inside.
        assert_eq!(resolve_outside("x", Some(&h), Some(&h)), None);
        // New rule: launch-anchored, outside, and it asks.
        assert_eq!(resolve_outside("x", Some(&h), Some(&l)), Some(h.join("x")));
        let out = decide(
            true,
            "read_file",
            "x",
            false,
            Some(&h),
            Some(&l),
            &s,
            true,
            &mut ask,
        );
        assert!(out.is_some(), "a 'no' to a post-/cd read must refuse");
        assert_eq!(
            *log.borrow(),
            vec![("read_file".to_string(), Approval::Files(h.clone()))]
        );
        // Near miss: a target under the launch dir, reached from the moved cwd
        // by an absolute path, is still inside and never asked.
        let inside = l.join("inside.txt");
        let out = decide(
            true,
            "read_file",
            inside.to_str().unwrap(),
            false,
            Some(&h),
            Some(&l),
            &s,
            true,
            &mut ask,
        );
        assert_eq!(out, None);
        assert_eq!(log.borrow().len(), 1, "an inside read must not ask");
    }

    /// Users who never `/cd`: cwd == launch dir, outcome as before.
    #[test]
    fn never_cd_inside_is_untouched() {
        let (_tmp, _root, work) = tree();
        let s = store();
        let mut never = |_: &str, _: &Path, _: &Approval| -> bool { panic!("must not ask") };
        assert_eq!(
            decide(
                true,
                "read_file",
                "inside.txt",
                false,
                Some(&work),
                Some(&work),
                &s,
                false,
                &mut never
            ),
            None
        );
    }

    /// An approval given before `/cd` still covers its path afterwards.
    #[test]
    fn approval_survives_a_cwd_move() {
        let (_tmp, root, work) = tree();
        let s = store();
        let log = std::cell::RefCell::new(Vec::new());
        let mut ask = recorder(&log, true);
        // Before /cd: cwd == launch == work; read ../a/x, approve Files(a).
        assert_eq!(
            decide(
                true,
                "read_file",
                "../a/x",
                false,
                Some(&work),
                Some(&work),
                &s,
                true,
                &mut ask
            ),
            None
        );
        // After /cd into b: the same file by a new relative path, no re-ask.
        let b = root.join("b");
        assert_eq!(
            decide(
                true,
                "read_file",
                "../a/y",
                false,
                Some(&b),
                Some(&work),
                &s,
                true,
                &mut ask
            ),
            None
        );
        assert_eq!(
            log.borrow().len(),
            1,
            "an approved directory must not ask again after /cd"
        );
    }

    /// No anchor at all (nothing recorded, cwd unreadable): ASK, never allow.
    #[test]
    fn no_anchor_fails_closed() {
        let (_tmp, root, work) = tree();
        // Anti-vacuous: the fixture really has no anchor.
        assert_eq!(anchor_from(None, None), None);
        let s = store();
        let log = std::cell::RefCell::new(Vec::new());
        let mut ask = recorder(&log, false);
        let target = work.join("inside.txt");
        let out = decide(
            true,
            "read_file",
            target.to_str().unwrap(),
            false,
            None,
            None,
            &s,
            true,
            &mut ask,
        );
        assert!(out.is_some(), "a 'no' must refuse");
        assert_eq!(
            log.borrow().len(),
            1,
            "with no anchor, even this file must be asked about"
        );
        // No tty: refused, naming that the launch dir is unknown.
        let mut never = |_: &str, _: &Path, _: &Approval| -> bool { panic!("no tty, no ask") };
        let out = decide(
            true,
            "read_file",
            target.to_str().unwrap(),
            false,
            None,
            None,
            &s,
            false,
            &mut never,
        )
        .unwrap();
        assert!(out.contains("could not be determined"), "{out}");
        // A relative path with no readable cwd cannot be resolved: refused, not allowed.
        let out = decide(
            true,
            "read_file",
            "inside.txt",
            false,
            None,
            Some(&root),
            &s,
            false,
            &mut never,
        )
        .unwrap();
        assert!(out.contains("current directory cannot be read"), "{out}");
        assert!(crate::tool_wrappers::is_deterministic_refusal(&out));
    }

    /// The verdict for a call whose `path` argument is `root`, as `check`
    /// composes it: the effective root, then the decision. No tty, so an
    /// outside root is refused rather than asked.
    fn verdict_for(
        tool: &str,
        root: Option<&str>,
        cwd: Option<&Path>,
        launch: &Path,
    ) -> Option<String> {
        let mut never = |_: &str, _: &Path, _: &Approval| -> bool { panic!("no tty, no ask") };
        let path = effective_root(tool, root)?;
        decide(
            true,
            tool,
            path,
            false,
            cwd,
            Some(launch),
            &store(),
            false,
            &mut never,
        )
    }

    /// #879: a `search`/`list_files` with NO root walks the cwd (yoagent
    /// 0.25.7 defaults `path` to `.`), so after `/cd` out of the launch dir it
    /// must be checked as the cwd. Before this, `check` was never called
    /// without a `path`, and `/cd ~` + an unrooted search walked `~` unasked.
    #[test]
    fn unrooted_search_after_cd_is_checked_as_the_cwd() {
        let (_tmp, root, work) = tree();
        let moved = root.join("a");
        for tool in ["search", "list_files"] {
            assert_eq!(
                verdict_for(tool, None, Some(&moved), &work),
                Some(no_tty_refusal(
                    tool,
                    &moved,
                    Some(&work),
                    &Approval::Tree(moved.clone())
                )),
                "{tool} with no root, cwd outside the launch dir, must be refused"
            );
            // Tty: asked once, as a recursive Tree over the cwd.
            let log = std::cell::RefCell::new(Vec::new());
            let mut ask = recorder(&log, true);
            let s = store();
            let path = effective_root(tool, None).unwrap();
            assert_eq!(
                decide(
                    true,
                    tool,
                    path,
                    false,
                    Some(&moved),
                    Some(&work),
                    &s,
                    true,
                    &mut ask
                ),
                None
            );
            assert_eq!(
                *log.borrow(),
                vec![(tool.to_string(), Approval::Tree(moved.clone()))]
            );
        }
    }

    /// Near miss for the above: no `/cd` (cwd == launch dir, the common case),
    /// an unrooted search is inside and nothing is asked.
    #[test]
    fn unrooted_search_in_the_launch_dir_is_inside() {
        let (_tmp, _root, work) = tree();
        for tool in ["search", "list_files"] {
            assert_eq!(verdict_for(tool, None, Some(&work), &work), None);
        }
    }

    /// An explicit root is unchanged by the default, an empty root is the
    /// cwd too, `read_file` with no path is not ours to judge (the tool
    /// reports the missing argument), and an unreadable cwd fails closed
    /// through the existing `no_cwd_refusal`, not a second policy.
    #[test]
    fn unrooted_default_edges() {
        let (_tmp, root, work) = tree();
        let moved = root.join("a");
        let inside = work.join("inside.txt");
        for tool in ["search", "list_files"] {
            assert_eq!(effective_root(tool, Some("sub")), Some("sub"));
            assert_eq!(
                verdict_for(tool, Some(inside.to_str().unwrap()), Some(&moved), &work),
                None,
                "{tool}: explicit root inside the launch dir, from a moved cwd"
            );
            assert_eq!(
                verdict_for(tool, Some(""), Some(&moved), &work),
                verdict_for(tool, None, Some(&moved), &work),
                "{tool}: an empty root is the cwd, like no root"
            );
            assert!(verdict_for(tool, Some(""), Some(&moved), &work).is_some());
            assert_eq!(
                verdict_for(tool, None, None, &work),
                Some(no_cwd_refusal(tool, "."))
            );
        }
        assert_eq!(effective_root("read_file", None), None);
        assert_eq!(verdict_for("read_file", None, Some(&moved), &work), None);
        assert_eq!(effective_root("bash", None), None);
    }

    /// The recorded launch dir wins over the cwd; the cwd is only a fallback.
    #[test]
    fn anchor_prefers_recorded_launch_dir() {
        let (l, c) = (Path::new("/launch"), Path::new("/cwd"));
        assert_eq!(anchor_from(Some(l), Some(c)), Some(l.to_path_buf()));
        assert_eq!(anchor_from(None, Some(c)), Some(c.to_path_buf()));
        assert_eq!(anchor_from(Some(l), None), Some(l.to_path_buf()));
    }

    // --- The write fence (Day 245) ---

    /// `decide_write` with the fence on, the cwd and anchor both `work`.
    fn w(tool: &str, path: &str, work: &Path) -> Option<String> {
        decide_write(true, tool, path, false, Some(work), Some(work))
    }

    /// Every user who never sets the variable: nothing is refused, for any
    /// tool, inside or out. This is the whole regression surface.
    #[test]
    fn write_fence_off_refuses_nothing() {
        let (_tmp, root, work) = tree();
        let outside = root.join("outside.txt");
        for tool in ["write_file", "edit_file"] {
            for path in [outside.to_str().unwrap(), "../outside.txt", "inside.txt"] {
                assert_eq!(
                    decide_write(false, tool, path, false, Some(&work), Some(&work)),
                    None,
                    "{tool} {path}"
                );
            }
        }
    }

    /// The exact text the model receives, written from intent.
    #[test]
    fn write_fence_refusal_full_text() {
        let (_tmp, root, work) = tree();
        let outside = root.join("outside.txt");
        let want = format!(
            "YOYO_BLOCK_OUTSIDE_WRITES=1 is active — write_file refused '{o}': it is outside \
             the directory yoyo was launched in '{a}'. This fence applies in every mode, --yes \
             included, and never asks. To allow it, unset YOYO_BLOCK_OUTSIDE_WRITES, run yoyo \
             from a directory that contains the target, or list its directory with --allow-dir \
             (which then limits every file tool to the listed directories).",
            o = outside.display(),
            a = work.display(),
        );
        assert_eq!(
            w("write_file", outside.to_str().unwrap(), &work),
            Some(want)
        );
        // Classified like the read fence's refusal (#710).
        let msg = w("edit_file", outside.to_str().unwrap(), &work).unwrap();
        assert!(
            crate::tool_wrappers::is_deterministic_refusal(&msg),
            "{msg}"
        );
        assert!(msg.contains("edit_file refused '"), "{msg}");
    }

    #[test]
    fn write_fence_inside_new_nested_and_escapes() {
        let (_tmp, root, work) = tree();
        for tool in ["write_file", "edit_file"] {
            // Inside: existing, absolute, and not-yet-existing nested paths.
            assert_eq!(w(tool, "inside.txt", &work), None);
            assert_eq!(
                w(tool, work.join("inside.txt").to_str().unwrap(), &work),
                None
            );
            assert_eq!(w(tool, "new/dir/f.txt", &work), None);
            assert_eq!(
                w(tool, "../work/g.txt", &work),
                None,
                "an existing '..' is resolved"
            );
            // Outside: absolute, a `..` escape, and a nested not-yet-existing one.
            for p in [
                root.join("outside.txt").to_str().unwrap().to_string(),
                "../outside.txt".to_string(),
                "../fresh/dir/f.txt".to_string(),
            ] {
                let out = w(tool, &p, &work).unwrap_or_else(|| panic!("{tool} {p} allowed"));
                assert!(out.contains("it is outside the directory yoyo"), "{out}");
            }
            // A `..` inside a part that does not exist yet cannot be resolved: refused.
            let out = w(tool, "nope/../../outside.txt", &work).unwrap();
            assert!(out.contains("cannot be resolved safely"), "{out}");
            assert!(out.contains("'nope/../../outside.txt'"), "{out}");
            // The stated cost of failing closed: `new/../g.txt` would land
            // inside (write_file creates `new`), and is refused anyway.
            let out = w(tool, "new/../g.txt", &work).unwrap();
            assert!(out.contains("cannot be resolved safely"), "{out}");
        }
        // Anti-vacuous: the escape target really is the sibling of `work`.
        assert!(root.join("outside.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn write_fence_symlinks_inside_pointing_outside_are_outside() {
        let (_tmp, root, work) = tree();
        std::os::unix::fs::symlink(root.join("a"), work.join("link_to_a")).unwrap();
        std::os::unix::fs::symlink(root.join("gone"), work.join("dangling")).unwrap();
        // Anti-vacuous: the link exists and leads out of `work`.
        assert!(work.join("link_to_a/x").exists());
        let out = w("write_file", "link_to_a/new.txt", &work).unwrap();
        assert!(
            out.contains(&format!("'{}'", root.join("a/new.txt").display())),
            "{out}"
        );
        let out = w("write_file", "dangling", &work).unwrap();
        assert!(out.contains("cannot be resolved safely"), "{out}");
    }

    /// The anchor is the launch dir: a moved cwd changes where a relative
    /// path lands, never what counts as inside.
    #[test]
    fn write_fence_anchor_is_the_launch_dir_not_the_cwd() {
        let (_tmp, root, work) = tree();
        let moved = root.join("a");
        // Relative to the moved cwd: lands in `a`, outside the launch dir.
        let out = decide_write(
            true,
            "write_file",
            "f.txt",
            false,
            Some(&moved),
            Some(&work),
        );
        assert!(
            out.unwrap()
                .contains(&format!("'{}'", moved.join("f.txt").display())),
            "the target resolves against the cwd at call time"
        );
        // Old rule (cwd-anchored) would have allowed it.
        assert_eq!(
            decide_write(
                true,
                "write_file",
                "f.txt",
                false,
                Some(&moved),
                Some(&moved)
            ),
            None
        );
        // Near miss: the launch dir, reached by absolute path from the moved cwd.
        let p = work.join("g.txt");
        assert_eq!(
            decide_write(
                true,
                "write_file",
                p.to_str().unwrap(),
                false,
                Some(&moved),
                Some(&work)
            ),
            None
        );
    }

    #[test]
    fn write_fence_other_tools_allow_dir_no_cwd_and_no_anchor() {
        let (_tmp, root, work) = tree();
        let out = root.join("outside.txt");
        let o = out.to_str().unwrap();
        for tool in ["read_file", "search", "list_files", "bash", "rename_symbol"] {
            assert_eq!(w(tool, o, &work), None, "{tool} is not a write-fenced tool");
        }
        assert_eq!(
            decide_write(true, "write_file", o, true, Some(&work), Some(&work)),
            None,
            "--allow-dir is consent, as for the read fence"
        );
        let msg = decide_write(true, "write_file", "x", false, None, Some(&work)).unwrap();
        assert!(msg.contains("use an absolute path"), "{msg}");
        // No anchor: nothing is inside (fail closed), even the cwd itself.
        let msg = decide_write(true, "write_file", "inside.txt", false, Some(&work), None).unwrap();
        assert!(msg.contains("(which could not be determined)"), "{msg}");
    }

    /// Sets the write fence's inputs for this (current-thread) test only.
    struct WriteFenceOn;
    impl WriteFenceOn {
        fn set(on: bool, anchor: &Path) -> Self {
            WRITE_FENCE_TEST_INPUTS.with(|c| *c.borrow_mut() = Some((on, anchor.to_path_buf())));
            WriteFenceOn
        }
    }
    impl Drop for WriteFenceOn {
        fn drop(&mut self) {
            WRITE_FENCE_TEST_INPUTS.with(|c| *c.borrow_mut() = None);
        }
    }

    fn tool_ctx() -> yoagent::types::ToolContext {
        yoagent::types::ToolContext::new("test", "test")
    }

    /// Through both real wrappers (`maybe_guard` for the main agent,
    /// `maybe_guard_arc` for sub-agents): deleting either call site of
    /// `outside_read::check` in src/tool_wrappers.rs reddens this.
    #[tokio::test]
    async fn write_fence_is_wired_into_both_guarded_tools() {
        use yoagent::tools::{EditFileTool, WriteFileTool};
        let (_tmp, root, work) = tree();
        let none = crate::cli::DirectoryRestrictions::default();
        let outside = root.join("blocked.txt");
        let o = outside.to_str().unwrap().to_string();
        let _on = WriteFenceOn::set(true, &work);

        let writer = crate::tool_wrappers::maybe_guard(Box::new(WriteFileTool::new()), &none);
        let err = writer
            .execute(serde_json::json!({ "path": o, "content": "x" }), tool_ctx())
            .await
            .expect_err("outside write must be refused");
        assert!(
            format!("{err:?}").contains("write_file refused '"),
            "{err:?}"
        );
        assert!(!outside.exists(), "the refused write must not land");
        // Near miss: an inside write through the same wrapper lands.
        let inside = work.join("ok.txt");
        writer
            .execute(
                serde_json::json!({ "path": inside.to_str().unwrap(), "content": "x" }),
                tool_ctx(),
            )
            .await
            .expect("inside write must proceed");
        assert_eq!(std::fs::read_to_string(&inside).unwrap(), "x");

        let editor =
            crate::tool_wrappers::maybe_guard_arc(std::sync::Arc::new(EditFileTool::new()), &none);
        let target = root.join("outside.txt");
        let err = editor
            .execute(
                serde_json::json!({
                    "path": target.to_str().unwrap(), "old_text": "out", "new_text": "pwned"
                }),
                tool_ctx(),
            )
            .await
            .expect_err("outside edit must be refused");
        assert!(
            format!("{err:?}").contains("edit_file refused '"),
            "{err:?}"
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "out");

        // Fence off through the same wrapper: the outside write lands.
        drop(_on);
        let _off = WriteFenceOn::set(false, &work);
        writer
            .execute(serde_json::json!({ "path": o, "content": "x" }), tool_ctx())
            .await
            .expect("fence off: outside write proceeds as before");
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "x");
    }
}
