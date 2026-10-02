//! The `{"type":"sessionRestored","messages":N}` NDJSON line (#979 half 2).
//!
//! In `--output-format stream-json` a caller could not tell a continued run
//! from a fresh one: the only signal was human stderr wording, which is not a
//! contract. This line is that contract.
//!
//! - Emitted ONLY when a session was actually restored (`--continue` or
//!   `--continue-strict` whose restore succeeded), as the line immediately
//!   after the first `{"type":"agentStart"}` of the process, and only once:
//!   the store is drained, so a later turn's `agentStart` does not repeat it.
//! - `messages` is the count `restore_session_result` returned.
//! - No restore (no flag, or a lenient `--continue` whose restore failed)
//!   records nothing, so the stream is byte-identical to before. A failed
//!   restore deliberately does NOT emit `messages: 0`: that would read as
//!   "restored an empty session", and a strict caller already has the exit.
//! - Only the stream-json writer drains the store; text, json and `--print`
//!   never read it, so their output is unchanged.

use std::io::Write;
use std::sync::Mutex;
use yoagent::AgentEvent;

/// The restored message count, waiting for the first `agentStart`.
static RESTORED: Mutex<Option<usize>> = Mutex::new(None);

/// Record that `n` messages were restored into the agent. Called by
/// `main.rs` on a successful restore only.
pub(crate) fn record_session_restored(n: usize) {
    *crate::sync_util::lock_or_recover(&RESTORED) = Some(n);
}

/// One-shot drain, taking the store as a parameter so tests drive a local one.
fn drain_restored(store: &Mutex<Option<usize>>) -> Option<usize> {
    crate::sync_util::lock_or_recover(store).take()
}

/// The exact NDJSON line (no trailing newline). Built with `format!` rather
/// than `serde_json::json!`, whose map sorts keys and would put `messages`
/// before `type`; every other stream-json line leads with `type`. A `usize`
/// is always a valid JSON number, so nothing needs escaping.
pub(crate) fn session_restored_line(n: usize) -> String {
    format!(r#"{{"type":"sessionRestored","messages":{n}}}"#)
}

/// Write one yoagent event as an NDJSON line to `out`, followed — only after
/// an `agentStart`, and only if `store` still holds a count — by the
/// `sessionRestored` line. The real stream-json writer, parameterised so
/// tests read the bytes a caller would.
pub(crate) fn write_stream_event_with<W: Write>(
    out: &mut W,
    event: &AgentEvent,
    store: &Mutex<Option<usize>>,
) {
    if let Ok(json) = serde_json::to_string(event) {
        let _ = writeln!(out, "{json}");
    }
    if matches!(event, AgentEvent::AgentStart) {
        if let Some(n) = drain_restored(store) {
            let _ = writeln!(out, "{}", session_restored_line(n));
        }
    }
}

/// The production writer: stdout and the process-global store.
pub(crate) fn write_stream_event(event: &AgentEvent) {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    write_stream_event_with(&mut lock, event, &RESTORED);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(events: &[AgentEvent], store: &Mutex<Option<usize>>) -> Vec<String> {
        let mut buf = Vec::new();
        for e in events {
            write_stream_event_with(&mut buf, e, store);
        }
        String::from_utf8(buf)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn turn() -> Vec<AgentEvent> {
        vec![
            AgentEvent::AgentStart,
            AgentEvent::agent_end(vec![], Default::default()),
        ]
    }

    #[test]
    fn line_shape_table() {
        for (n, want) in [
            (0, r#"{"type":"sessionRestored","messages":0}"#),
            (1, r#"{"type":"sessionRestored","messages":1}"#),
            (42, r#"{"type":"sessionRestored","messages":42}"#),
        ] {
            assert_eq!(session_restored_line(n), want);
        }
    }

    #[test]
    fn restore_emits_line_directly_after_agent_start() {
        let store = Mutex::new(Some(7));
        let lines = run(&turn(), &store);
        assert_eq!(lines[0], r#"{"type":"agentStart"}"#);
        assert_eq!(lines[1], r#"{"type":"sessionRestored","messages":7}"#);
        assert_eq!(lines.len(), 3, "agentStart, sessionRestored, agentEnd");
    }

    #[test]
    fn no_restore_is_byte_identical_to_plain_events() {
        // Near-miss: an empty store writes exactly what serde produces.
        let store = Mutex::new(None);
        let lines = run(&turn(), &store);
        let plain: Vec<String> = turn()
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect();
        assert_eq!(lines, plain);
    }

    #[test]
    fn second_turn_does_not_repeat_the_line() {
        let store = Mutex::new(Some(3));
        let mut events = turn();
        events.extend(turn());
        let lines = run(&events, &store);
        let restored: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.contains("sessionRestored"))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            restored,
            vec![1],
            "exactly once, after the first agentStart"
        );
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn only_agent_start_drains_the_store() {
        // An agentEnd arriving first must not consume the one-shot.
        let store = Mutex::new(Some(2));
        let lines = run(
            &[
                AgentEvent::agent_end(vec![], Default::default()),
                AgentEvent::AgentStart,
            ],
            &store,
        );
        assert_eq!(lines[2], r#"{"type":"sessionRestored","messages":2}"#);
    }
}
