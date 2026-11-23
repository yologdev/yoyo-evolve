//! What a command the bash safety check warns about (`analyze_bash_command`
//! returns a reason) does in a run nobody can answer a prompt in (#1231).
//!
//! | how the run was started                      | warned command |
//! |----------------------------------------------|----------------|
//! | REPL on a terminal, no `--yes`               | asks           |
//! | explicit `--yes` / `-y` (with or without -p) | runs (#1202 A) |
//! | prompt on the command line, no `--yes`       | **refused**    |
//! | piped stdin, no prompt flag, no `--yes`      | runs (unchanged) |
//!
//! A user who typed `--yes` chose all-or-nothing. A user who typed only
//! `-p "..."` (or `--prompt`, a bare `yoyo "..."`, `--print -p`) never chose
//! it; auto-approve there is implied because there is no terminal to ask on,
//! so the honest answer to "may I run this?" is "no". Piped stdin keeps
//! running warned commands because every loop script runs yoyo that way
//! (census on #1231); changing it is a separate decision, not made here.
//! The hard deny list and user deny rules apply on every row, before this.

use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarnedCommandPolicy {
    /// The REPL's confirm prompt decides.
    Ask,
    /// Runs without asking.
    Run,
    /// Refused with [`unattended_refusal`].
    Refuse,
}

/// The decision, pure. `config_auto_approve` is `Config.auto_approve`
/// (`--yes` or `--print`); `explicit_yes` is whether `--yes`/`-y` itself was
/// given; `prompt_on_command_line` is whether the prompt came from argv (`-p`,
/// `--prompt`, a bare word) rather than stdin. Auto-approve is in force when
/// the config says so or there is no terminal to ask on (as in `main`).
pub fn warned_command_policy(
    config_auto_approve: bool,
    explicit_yes: bool,
    prompt_on_command_line: bool,
    stdin_is_tty: bool,
) -> WarnedCommandPolicy {
    let interactive = stdin_is_tty && !prompt_on_command_line;
    if !config_auto_approve && interactive {
        WarnedCommandPolicy::Ask
    } else if explicit_yes || !prompt_on_command_line {
        WarnedCommandPolicy::Run
    } else {
        WarnedCommandPolicy::Refuse
    }
}

/// Set once, in `main`, from [`warned_command_policy`]. Read when the bash
/// tools are built and by a sub-agent's bash, so a child is never more
/// permissive than its parent. Tests never set it; they drive the pure parts.
static REFUSE_WARNED: AtomicBool = AtomicBool::new(false);

pub fn set_refuse_warned(refuse: bool) {
    REFUSE_WARNED.store(refuse, Ordering::Relaxed);
}

pub fn refuses_warned() -> bool {
    REFUSE_WARNED.load(Ordering::Relaxed)
}

/// What to do instead, for the run's user. The flag must parse as an
/// explicit yes next to `-p` (round-trip test in the tests module).
pub const UNATTENDED_REMEDY: &str = "Nobody can answer a confirmation prompt in this run \
     (a prompt given on the command line without --yes), so it was not run. To let \
     commands like this run without asking, rerun with --yes.";

/// The shared refusal shape: the warning, then who cannot confirm and what to
/// do. The sub-agent's refusal (`tools::child_confirm_refusal`) uses it too.
pub fn needs_confirmation(who_cannot: &str, warning: &str, then: &str) -> String {
    format!("Command needs user confirmation that {who_cannot} cannot obtain: {warning} {then}")
}

/// The refusal for a warned command under [`WarnedCommandPolicy::Refuse`].
pub fn unattended_refusal(warning: &str) -> String {
    needs_confirmation("this run", warning, UNATTENDED_REMEDY)
}

#[cfg(test)]
#[path = "warned_policy_tests.rs"]
mod warned_policy_tests;
