//! #993: what a Ctrl+C (SIGINT) mid-turn prints, and whether it happened.

use crate::format::{DIM, RESET};

/// #993: set by `main.rs` for the non-interactive doors (`-p`, piped stdin).
static SINGLE_SHOT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// #993: set when a Ctrl+C (SIGINT) cut a turn short.
pub(super) static INTERRUPTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// #1016: set when the event stream closed before the run's `AgentEnd`.
static STREAM_CUT_SHORT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Mark this process as a single-shot run (`-p` / piped prompt).
pub fn mark_single_shot() {
    SINGLE_SHOT.store(true, std::sync::atomic::Ordering::SeqCst);
}

pub(super) fn is_single_shot() -> bool {
    SINGLE_SHOT.load(std::sync::atomic::Ordering::SeqCst)
}

/// Whether a SIGINT interrupted a turn in this process (#993: `-p` exits 130).
pub fn was_interrupted() -> bool {
    INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst)
}

/// #1016: did a turn's event stream close before its `AgentEnd` (the agent
/// task died mid-run)? A single-shot run then counts as failed: exit 1, and
/// `--output-format json` reports `is_error: true`.
pub fn stream_cut_short() -> bool {
    STREAM_CUT_SHORT.load(std::sync::atomic::Ordering::SeqCst)
}

/// #1016: record a stream that closed without `AgentEnd` and say so. Nobody
/// chooses this ending, and stdout holds only what streamed before it, so a
/// script must be able to tell it from a finished reply.
pub(super) fn record_stream_cut_short() {
    use std::io::Write;
    STREAM_CUT_SHORT.store(true, std::sync::atomic::Ordering::SeqCst);
    let note = stream_cut_short_note();
    if is_single_shot() {
        let _ = std::io::stderr().write_all(note.as_bytes());
    } else {
        super::write_chrome(&note);
    }
}

/// #1016: the note for a reply cut off by a dead agent task.
pub(super) fn stream_cut_short_note() -> String {
    format!("\n{DIM}  (reply cut off: the agent stopped before finishing the turn){RESET}\n")
}

/// #993/#1015/#1016: the exit code a cut-short single-shot run should use:
/// 143 (128 + SIGTERM) or 129 (128 + SIGHUP) when that signal ended it,
/// 130 (128 + SIGINT) for Ctrl+C, `None` when nothing interrupted it.
pub fn interrupted_exit_code() -> Option<i32> {
    interrupted_exit_code_from(super::sigterm::terminated_exit_code(), was_interrupted())
}

fn interrupted_exit_code_from(terminated: Option<i32>, interrupted: bool) -> Option<i32> {
    if terminated.is_some() {
        terminated
    } else if interrupted {
        Some(130)
    } else {
        None
    }
}

/// #993/#1015: the one policy for a turn cut short by a signal (Ctrl+C or,
/// on a pipe, SIGTERM): stop the spinner, abort the agent, flush the text
/// the retry-safe gate has not printed yet, both released-but-queued and
/// still held (#1016; it was produced before the signal, and
/// the unfiltered stream would already have shown it), close the line, and
/// print the interrupt note. Both `select!` arms call this; neither copies it.
pub(super) fn finish_interrupted_turn(
    mut state: super::PromptEventState,
    gate: &mut super::retry_safe_pipe::RetrySafeGate,
    agent: &mut yoagent::agent::Agent,
) -> super::PromptResult {
    use std::io::Write;
    if let Some(s) = state.spinner.take() {
        s.stop();
    }
    agent.abort();
    // #1016: released-but-unprinted queue text first, then held text.
    let unprinted = gate.take_unprinted_text();
    if !unprinted.is_empty() {
        state.handle_message_update_text(&unprinted);
    }
    if state.in_text {
        super::write_stream_text("\n");
    }
    let _ = std::io::stdout().flush();
    // #993: single-shot runs get a stderr note; REPL bytes unchanged.
    INTERRUPTED.store(true, std::sync::atomic::Ordering::SeqCst);
    let single = is_single_shot();
    let note = interrupt_note(single);
    if single {
        let _ = std::io::stderr().write_all(note.as_bytes());
    } else {
        super::write_chrome(&note);
    }
    super::PromptResult::Done {
        collected_text: state.collected_text,
        text_since_last_tool: state.text_since_last_tool,
        usage: state.usage,
        last_tool_error: state.last_tool_error,
        last_tool_name: state.last_tool_name,
    }
}

/// #993: the note printed when Ctrl+C interrupts a turn. The REPL row is the
/// historical hint, byte for byte; a single-shot run has already exited, so
/// it gets a plain note (the caller writes it to stderr).
pub(super) fn interrupt_note(single_shot: bool) -> String {
    if single_shot {
        format!("\n{DIM}  (interrupted){RESET}\n")
    } else {
        format!("\n{DIM}  (interrupted — press Ctrl+C again to exit){RESET}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupt_note_repl_row_is_the_old_hint_and_single_shot_drops_again() {
        assert_eq!(
            interrupt_note(false),
            format!("\n{DIM}  (interrupted — press Ctrl+C again to exit){RESET}\n")
        );
        let single = interrupt_note(true);
        assert_eq!(single, format!("\n{DIM}  (interrupted){RESET}\n"));
        assert!(!single.contains("again"));
    }

    #[test]
    fn stream_cut_short_note_says_the_reply_was_cut_off() {
        assert_eq!(
            stream_cut_short_note(),
            format!(
                "\n{DIM}  (reply cut off: the agent stopped before finishing the turn){RESET}\n"
            )
        );
        // Glyph-free: the note reaches pipes and logs.
        assert!(stream_cut_short_note().is_ascii());
    }

    #[test]
    fn interrupted_exit_code_prefers_sigterm_then_sigint() {
        let term = Some(super::super::sigterm::SIGTERM_EXIT);
        let hup = Some(super::super::sigterm::SIGHUP_EXIT);
        assert_eq!(interrupted_exit_code_from(None, false), None);
        assert_eq!(interrupted_exit_code_from(None, true), Some(130));
        // The SIGTERM arm also marks the turn interrupted; the code is 143.
        assert_eq!(interrupted_exit_code_from(term, true), Some(143));
        assert_eq!(interrupted_exit_code_from(term, false), Some(143));
        // #1016: SIGHUP likewise wins over the interrupted flag, with 129.
        assert_eq!(interrupted_exit_code_from(hup, true), Some(129));
        assert_eq!(interrupted_exit_code_from(hup, false), Some(129));
        // #1016: SIGQUIT likewise, with 131.
        let quit = Some(super::super::sigterm::SIGQUIT_EXIT);
        assert_eq!(interrupted_exit_code_from(quit, true), Some(131));
        assert_eq!(interrupted_exit_code_from(quit, false), Some(131));
    }
}
