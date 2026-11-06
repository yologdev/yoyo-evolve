//! #993: what a Ctrl+C (SIGINT) mid-turn prints, and whether it happened.

use crate::format::{DIM, RESET};

/// #993: set by `main.rs` for the non-interactive doors (`-p`, piped stdin).
static SINGLE_SHOT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// #993: set when a Ctrl+C (SIGINT) cut a turn short.
pub(super) static INTERRUPTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// #1016: set when the event stream closed before the run's `AgentEnd`
/// (#1142: either kind, cut short or crashed; both exit 1).
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
/// task died mid-run, by a panic or otherwise; see [`RunEnd`])? A single-shot run then counts as failed: exit 1, and
/// `--output-format json` reports `is_error: true`.
pub fn stream_cut_short() -> bool {
    STREAM_CUT_SHORT.load(std::sync::atomic::Ordering::SeqCst)
}

/// #1142: panics seen by the hook [`install_panic_counter`] puts in front of
/// the default one. A turn snapshots this at its start, so a run that ended
/// without `AgentEnd` can be told apart: a panic happened in between (the
/// agent task crashed) or none did (the stream was cut short some other way).
static PANICS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// #1142: count every panic in this process, then run the hook that was
/// there before (the default one prints `thread '...' panicked at ...` to
/// stderr exactly as before). Process-wide: it reaches every panic on every
/// thread, and changes nothing about any of them except this count. Idempotent.
pub fn install_panic_counter() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            PANICS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            previous(info);
        }));
    });
}

/// #1142: panics counted so far (0 when the counter was never installed).
pub(super) fn panic_count() -> usize {
    PANICS.load(std::sync::atomic::Ordering::SeqCst)
}

/// #1016/#1142: how a turn's event stream ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RunEnd {
    /// The run reached its `AgentEnd` (done, error, abort, rejected input).
    Finished,
    /// The stream closed without `AgentEnd` and nothing panicked meanwhile.
    CutShort,
    /// The stream closed without `AgentEnd` and a panic happened during the
    /// turn: the agent task crashed. Not a limit; a bug in yoyo or yoagent.
    Crashed,
}

/// #1142: the decision. A panic counted between the turn's start and the
/// stream's close turns an unfinished run into a crash; a finished run is
/// finished whatever else panicked.
pub(super) fn classify_run_end(
    ended_without_agent_end: bool,
    panics_at_start: usize,
    panics_now: usize,
) -> RunEnd {
    if !ended_without_agent_end {
        RunEnd::Finished
    } else if panics_now > panics_at_start {
        RunEnd::Crashed
    } else {
        RunEnd::CutShort
    }
}

/// #1016/#1142: the note for each ending (`None` for a finished run).
pub(super) fn run_end_note(end: RunEnd) -> Option<String> {
    match end {
        RunEnd::Finished => None,
        RunEnd::CutShort => Some(stream_cut_short_note()),
        RunEnd::Crashed => Some(crashed_note()),
    }
}

/// #1016/#1142: record an unfinished run and say how it ended. Nobody
/// chooses this ending, and stdout holds only what streamed before it, so a
/// script must be able to tell it from a finished reply: both unfinished
/// kinds set the flag `main` turns into exit 1 (`is_error: true` in json).
pub(super) fn record_unfinished_run(end: RunEnd) {
    use std::io::Write;
    let Some(note) = run_end_note(end) else {
        return;
    };
    STREAM_CUT_SHORT.store(true, std::sync::atomic::Ordering::SeqCst);
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

/// #1142: the note for an agent task that panicked mid-turn. Until Day 250
/// this case got the "reply cut off" note, which reads as a truncated stream
/// or a limit; it is a crash, and the panic line above it is the evidence.
pub(super) fn crashed_note() -> String {
    format!(
        "\n{DIM}  (the agent crashed before finishing the turn: a panic, printed above. \
         That is a bug in yoyo or yoagent, not a limit you hit; please report it at \
         https://github.com/yologdev/yoyo-evolve/issues){RESET}\n"
    )
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
        incomplete: state.incomplete,
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

    /// #1142: the decision table, driven through the deciding function.
    #[test]
    fn run_end_tells_a_crash_from_a_cut_off_stream_from_a_finished_run() {
        // (a) closed without AgentEnd, no panic during the turn: cut off.
        assert_eq!(classify_run_end(true, 3, 3), RunEnd::CutShort);
        let cut = run_end_note(RunEnd::CutShort).expect("a note");
        assert!(cut.contains("reply cut off"), "{cut}");
        // (b) closed without AgentEnd and a panic happened: a crash, never
        // worded as a cut-off stream.
        assert_eq!(classify_run_end(true, 3, 4), RunEnd::Crashed);
        assert_eq!(classify_run_end(true, 0, 2), RunEnd::Crashed);
        let crash = run_end_note(RunEnd::Crashed).expect("a note");
        assert!(crash.contains("crashed"), "{crash}");
        assert!(crash.contains("panic"), "{crash}");
        assert!(!crash.contains("cut off"), "{crash}");
        assert!(crash.is_ascii(), "the note reaches pipes and logs");
        // (c) the run reached AgentEnd: no note, even if something panicked.
        assert_eq!(classify_run_end(false, 3, 3), RunEnd::Finished);
        assert_eq!(classify_run_end(false, 3, 9), RunEnd::Finished);
        assert_eq!(run_end_note(RunEnd::Finished), None);
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
