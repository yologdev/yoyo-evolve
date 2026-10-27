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
        let _ = writeln!(out, "{}", escape_unsafe_json_chars(&json));
    }
    if matches!(event, AgentEvent::AgentStart) {
        if let Some(n) = drain_restored(store) {
            let _ = writeln!(out, "{}", session_restored_line(n));
        }
    }
}

/// True for a char that is invisible or reorders text on a terminal: the
/// Unicode `Cf` (format) category, plus DEL (U+007F), the C1 controls
/// (U+0080–U+009F) and the line/paragraph separators U+2028/U+2029 (#1092).
/// C0 controls are not listed: serde_json already escapes those itself.
fn is_unsafe_json_char(c: char) -> bool {
    matches!(
        c,
        '\u{7F}'..='\u{9F}'
            | '\u{AD}'
            | '\u{600}'..='\u{605}'
            | '\u{61C}'
            | '\u{6DD}'
            | '\u{70F}'
            | '\u{890}'..='\u{891}'
            | '\u{8E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
    )
}

/// Rewrite already-serialized JSON so every [`is_unsafe_json_char`] is a
/// lowercase `\uxxxx` escape (a UTF-16 surrogate pair above U+FFFF), the
/// same treatment serde_json gives C0 controls. Sound after serialization
/// because every JSON structural character is ASCII, so these chars can
/// only sit inside strings, and any parser reads the escape back as the
/// identical char. Text without one is returned byte-identical.
pub(crate) fn escape_unsafe_json_chars(json: &str) -> String {
    if !json.chars().any(is_unsafe_json_char) {
        return json.to_string();
    }
    let mut out = String::with_capacity(json.len() + 16);
    for c in json.chars() {
        if is_unsafe_json_char(c) {
            let mut units = [0u16; 2];
            for u in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{:04x}", u));
            }
        } else {
            out.push(c);
        }
    }
    out
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
    fn escape_round_trips_to_the_identical_value() {
        let v =
            serde_json::json!({"s": "x\u{202E}y\u{200B}z\u{7F}\u{85}\u{FEFF}\u{E0041}\u{2028}"});
        let raw = serde_json::to_string(&v).unwrap();
        let out = escape_unsafe_json_chars(&raw);
        let back: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn escape_leaves_no_raw_unsafe_char() {
        let v =
            serde_json::json!({"s": "x\u{202E}y\u{200B}z\u{7F}\u{85}\u{FEFF}\u{E0041}\u{2028}"});
        let raw = serde_json::to_string(&v).unwrap();
        // Anti-vacuous: serde alone leaves the bidi override raw.
        assert!(raw.contains('\u{202E}'), "fixture must carry raw U+202E");
        let out = escape_unsafe_json_chars(&raw);
        assert!(!out.chars().any(is_unsafe_json_char), "{out}");
        assert!(!out.as_bytes().windows(3).any(|w| w == [0xe2, 0x80, 0xae]));
        for needle in [
            "\\u202e",
            "\\u200b",
            "\\u007f",
            "\\u0085",
            "\\ufeff",
            "\\u2028",
            // U+E0041 as a UTF-16 surrogate pair.
            "\\udb40\\udc41",
        ] {
            assert!(out.contains(needle), "missing {needle} in {out}");
        }
    }

    #[test]
    fn escape_is_byte_identical_for_ordinary_text() {
        // Near-miss: accented, symbol, CJK, astral emoji and serde's own
        // escapes must pass through untouched.
        let s = r#"{"s":"hello é ✓ 漢字 🐙 \n\t\"q\""}"#;
        assert_eq!(escape_unsafe_json_chars(s), s);
    }

    #[test]
    fn stream_event_line_is_escaped_at_the_writer() {
        // Emission point: the bytes write_stream_event_with puts on stdout.
        let event = AgentEvent::ToolExecutionStart {
            tool_call_id: "c1".to_string(),
            tool_name: "bash".to_string(),
            args: serde_json::json!({"command": "echo x\u{202E}y"}),
        };
        let mut buf = Vec::new();
        write_stream_event_with(&mut buf, &event, &Mutex::new(None));
        assert!(!buf.windows(3).any(|w| w == [0xe2, 0x80, 0xae]));
        let line = String::from_utf8(buf).unwrap();
        assert!(line.contains("\\u202e"), "{line}");
        let parsed: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(parsed["args"]["command"], "echo x\u{202E}y");
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
