//! #991: on a single-shot `-p` / piped run whose stdout is a pipe (not a
//! terminal, not reserved by `--print`/json), yoyo reads the agent's events
//! through yoagent's [`RetrySafeEvents`] filter, so an attempt yoagent retries
//! internally never reaches stdout at all. Bytes on a pipe cannot be taken
//! back; holding each attempt until it succeeds means there is nothing to take
//! back. The trade, named: text arrives once per assistant message instead of
//! token by token. A pipe has no one watching tokens arrive.
//!
//! What the filter does (yoagent 0.25.2 `retry.rs`, `docs/concepts/retry.md`):
//! - a successful attempt arrives whole (start, every update, end) at its end;
//! - a retried attempt disappears; only its `ProviderRetry` remains;
//! - an attempt that fails for good arrives as start + `Error`/`Aborted` end,
//!   WITHOUT its deltas;
//! - an attempt left open when the stream ends is dropped;
//! - every other event (tools, `ToolExecutionUpdate` from sub-agents) passes
//!   through at once, unfiltered.
//!
//! The third and fourth rows would lose text yoyo used to keep (a turn that
//! dies for good keeps its partial answer on stdout; Ctrl+C keeps what was
//! streamed, #993). So this gate shadows the raw text of the attempt in
//! flight: a final failure gets its text back as one synthetic text delta just
//! before its end, an attempt still open when the stream closes gets its text
//! back as one synthetic delta at the close (#1016), and Ctrl+C can flush
//! whatever is still held.

use std::collections::VecDeque;
use std::io::{IsTerminal, Write};

use yoagent::retry::RetrySafeEvents;
use yoagent::{AgentEvent, AgentMessage, Message, StopReason, StreamDelta};

/// Should this run's text event loop hold attempts until they succeed?
/// Only single-shot runs (never the REPL), only an unreserved stdout (never
/// `--print`/json, which already hold the answer back), only a non-terminal
/// stdout (a terminal is unchanged: a human sees the restart).
pub(super) fn retry_safe_pipe_applies(
    single_shot: bool,
    stdout_reserved: bool,
    stdout_is_terminal: bool,
) -> bool {
    single_shot && !stdout_reserved && !stdout_is_terminal
}

/// Live-state wrapper of [`retry_safe_pipe_applies`].
pub(super) fn retry_safe_pipe_live() -> bool {
    retry_safe_pipe_applies(
        super::interrupt::is_single_shot(),
        crate::format::stdout_reserved(),
        std::io::stdout().is_terminal(),
    )
}

fn failed_assistant_end(event: &AgentEvent) -> Option<&AgentMessage> {
    match event {
        AgentEvent::MessageEnd { message } => match message {
            AgentMessage::Llm(Message::Assistant {
                stop_reason: StopReason::Error | StopReason::Aborted,
                ..
            }) => Some(message),
            _ => None,
        },
        _ => None,
    }
}

fn is_assistant_start(event: &AgentEvent) -> bool {
    matches!(
        event,
        AgentEvent::MessageStart {
            message: AgentMessage::Llm(Message::Assistant { .. })
        }
    )
}

/// The event source of the text loop: either the raw receiver (filter off,
/// byte-identical to before) or the raw receiver through [`RetrySafeEvents`].
pub(super) struct RetrySafeGate {
    filter: Option<RetrySafeEvents>,
    queue: VecDeque<AgentEvent>,
    closed: bool,
    /// Raw text of the attempt currently open (held by the filter).
    open_text: String,
    /// The `MessageStart` message of that open attempt, so a close inside it
    /// can release its text as a well-formed update (#1016).
    open_message: Option<AgentMessage>,
    /// Raw text of an attempt that ended with Error/Aborted and is held until
    /// the next event says whether it was retried.
    failed_text: String,
    /// #1016: where [`Drop`] writes unprinted text if the event loop panics
    /// while this gate is alive. Stdout in production; a buffer in tests.
    panic_sink: Box<dyn std::io::Write + Send>,
    /// #1016: the loop was handed the run's `AgentEnd`. yoagent sends it on
    /// every ending it reaches (done, error, abort, rejected input), so a
    /// close without it means the agent task died (a panic) mid-run.
    saw_agent_end: bool,
}

impl RetrySafeGate {
    pub(super) fn new(filtering: bool) -> Self {
        Self {
            filter: filtering.then(RetrySafeEvents::new),
            queue: VecDeque::new(),
            closed: false,
            open_text: String::new(),
            open_message: None,
            failed_text: String::new(),
            panic_sink: Box::new(std::io::stdout()),
            saw_agent_end: false,
        }
    }

    /// Test seam: the same gate, writing its panic flush into `sink`.
    #[cfg(test)]
    fn with_panic_sink(filtering: bool, sink: Box<dyn std::io::Write + Send>) -> Self {
        let mut gate = Self::new(filtering);
        gate.panic_sink = sink;
        gate
    }

    /// Is the filter on? (Off: every event is passed through untouched.)
    pub(super) fn is_filtering(&self) -> bool {
        self.filter.is_some()
    }

    /// Feed one raw event; queue whatever the filter releases. Released final
    /// failures get their shadowed text back as a synthetic delta first.
    /// `None` = the raw stream ended.
    fn feed(&mut self, raw: Option<AgentEvent>) {
        let Some(filter) = self.filter.as_mut() else {
            self.queue.extend(raw);
            return;
        };
        let released = match &raw {
            Some(event) => filter.push(event.clone()),
            None => filter.finish(),
        };
        for event in released {
            if let Some(message) = failed_assistant_end(&event) {
                let text = std::mem::take(&mut self.failed_text);
                if !text.is_empty() {
                    self.queue.push_back(AgentEvent::MessageUpdate {
                        message: message.clone(),
                        delta: StreamDelta::Text { delta: text },
                    });
                }
            }
            self.queue.push_back(event);
        }
        // Shadow bookkeeping AFTER the push: a held failure is released by the
        // NEXT event, so its text must still be in `failed_text` at that point.
        match raw {
            Some(AgentEvent::MessageUpdate {
                delta: StreamDelta::Text { delta },
                ..
            }) => self.open_text.push_str(&delta),
            Some(ref event) if is_assistant_start(event) => {
                self.open_text.clear();
                if let AgentEvent::MessageStart { message } = event {
                    self.open_message = Some(message.clone());
                }
            }
            Some(ref event) if failed_assistant_end(event).is_some() => {
                self.failed_text = std::mem::take(&mut self.open_text);
            }
            Some(AgentEvent::MessageEnd { .. }) => self.open_text.clear(),
            Some(AgentEvent::ProviderRetry { .. }) => {
                // The retried attempt disappeared; so does its text.
                self.open_text.clear();
                self.failed_text.clear();
            }
            None => {
                self.release_unended_attempt();
                self.closed = true;
            }
            _ => {}
        }
    }

    /// #1016: the raw stream closed while an attempt was still open (no
    /// `MessageEnd`, no `ProviderRetry`). Keyed on that STATE, not on the cause
    /// of the close: a panic inside yoagent's spawned agent task never unwinds
    /// through this gate, it only closes the channel, so a cause-keyed flush
    /// (the `Drop` below, `panicking()`) cannot see it.
    ///
    /// Policy: before the hold (#991) this text had already reached stdout,
    /// token by token, so dropping it here would make this the one ending that
    /// loses it -- SIGINT, SIGTERM/SIGHUP/SIGQUIT and a panic in yoyo's loop all
    /// keep it. yoagent's own `RetrySafeEvents::finish()` drops `Held::Open`;
    /// yoyo differs on purpose. The property #976/#989 protect is "no reader
    /// receives two attempts it cannot tell apart", and an un-ended attempt
    /// that is the LAST thing on the wire is one attempt, not two. A retried
    /// attempt never gets here: `ProviderRetry` already cleared its text.
    ///
    /// Released as one synthetic text delta at the back of the queue, after
    /// anything `finish()` released, so the loop prints it through its normal
    /// path (markdown renderer, `collected_text`) exactly once, with no
    /// `MessageEnd` -- the same shape the raw stream had before the hold.
    fn release_unended_attempt(&mut self) {
        let text = std::mem::take(&mut self.open_text);
        let Some(message) = self.open_message.take() else {
            return;
        };
        if !text.is_empty() {
            self.queue.push_back(AgentEvent::MessageUpdate {
                message,
                delta: StreamDelta::Text { delta: text },
            });
        }
    }

    /// Next event for the loop. Cancel-safe for `tokio::select!`: the only
    /// await is `rx.recv()`, and state changes happen after it returns.
    pub(super) async fn next(
        &mut self,
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<AgentEvent>,
    ) -> Option<AgentEvent> {
        loop {
            if let Some(event) = self.queue.pop_front() {
                return Some(self.seen(event));
            }
            if self.closed {
                return None;
            }
            let raw = rx.recv().await;
            if self.filter.is_none() {
                self.closed = raw.is_none();
                return raw.map(|event| self.seen(event));
            }
            self.feed(raw);
        }
    }

    fn seen(&mut self, event: AgentEvent) -> AgentEvent {
        if matches!(event, AgentEvent::AgentEnd { .. }) {
            self.saw_agent_end = true;
        }
        event
    }

    /// #1016: the stream closed and the run never reached its `AgentEnd`, so
    /// the reply the user got is whatever streamed before the agent task
    /// died. Keyed on that state, filter on or off, not on the cause.
    pub(super) fn ended_without_agent_end(&self) -> bool {
        self.closed && !self.saw_agent_end
    }

    /// #1016: called by the loop once `next()` returns `None`; records a run
    /// that ended without its `AgentEnd` (exit 1, a note on stderr).
    pub(super) fn report_unfinished_run(&self) {
        if self.ended_without_agent_end() {
            super::interrupt::record_stream_cut_short();
        }
    }

    /// Text the filter is still holding (an attempt in flight, or a failed
    /// one awaiting its verdict). Empty when the filter is off.
    fn take_held_text(&mut self) -> String {
        let mut text = std::mem::take(&mut self.failed_text);
        text.push_str(&std::mem::take(&mut self.open_text));
        text
    }

    /// #993/#1016: every byte of model text the loop has not printed yet, in
    /// wire order, each exactly once: first text already RELEASED into the
    /// queue (a finished attempt is released all at once and the loop pops
    /// one event per `select!`, which polls its arms in random order, so a
    /// signal arm can win with the queue non-empty), then text still HELD.
    /// Keyed on what is left, not on which signal or panic ended the loop:
    /// the interrupt path and [`Drop`] both call this. Empty when the filter
    /// is off (the queue is only filled while filtering).
    pub(super) fn take_unprinted_text(&mut self) -> String {
        let mut text: String = self
            .queue
            .drain(..)
            .filter_map(|e| match e {
                AgentEvent::MessageUpdate {
                    delta: StreamDelta::Text { delta },
                    ..
                } => Some(delta),
                _ => None,
            })
            .collect();
        text.push_str(&self.take_held_text());
        text
    }
}

/// #1016: a panic in the event loop unwinds through the loop's frame and
/// drops this gate (Cargo.toml sets no `panic = "abort"`). Without this, the
/// text the gate holds, plus any text it already released into `queue` that
/// the loop had not printed yet, died with it. Flushes ONLY while panicking:
/// a normal drop writes nothing, exactly as before, so an attempt discarded
/// for a retry can never reach stdout this way (#991's double-attempt glue).
/// Never panics: write errors are ignored, nothing is unwrapped.
impl Drop for RetrySafeGate {
    fn drop(&mut self) {
        if !std::thread::panicking() || self.filter.is_none() {
            return;
        }
        let text = self.take_unprinted_text();
        if !text.is_empty() {
            let _ = self.panic_sink.write_all(text.as_bytes());
            let _ = self.panic_sink.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yoagent::Usage;

    fn assistant(stop_reason: StopReason) -> AgentMessage {
        AgentMessage::Llm(Message::assistant(
            Vec::new(),
            stop_reason,
            "m",
            "p",
            Usage::default(),
        ))
    }

    fn start() -> AgentEvent {
        AgentEvent::MessageStart {
            message: assistant(StopReason::Stop),
        }
    }
    fn text(t: &str) -> AgentEvent {
        AgentEvent::MessageUpdate {
            message: assistant(StopReason::Stop),
            delta: StreamDelta::Text { delta: t.into() },
        }
    }
    fn end(reason: StopReason) -> AgentEvent {
        AgentEvent::MessageEnd {
            message: assistant(reason),
        }
    }
    fn retry() -> AgentEvent {
        AgentEvent::provider_retry(1, 3, "slow down", std::time::Duration::ZERO)
    }

    /// Text deltas the loop would print, in order, after the stream ends.
    fn printed(gate: &mut RetrySafeGate, events: Vec<AgentEvent>) -> Vec<String> {
        for e in events {
            gate.feed(Some(e));
        }
        gate.feed(None);
        gate.queue
            .drain(..)
            .filter_map(|e| match e {
                AgentEvent::MessageUpdate {
                    delta: StreamDelta::Text { delta },
                    ..
                } => Some(delta),
                _ => None,
            })
            .collect()
    }

    /// Stand-in for stdout that the test can read after the gate is gone.
    #[derive(Clone, Default)]
    struct Sink(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Sink {
        fn bytes(&self) -> String {
            String::from_utf8(self.0.lock().unwrap_or_else(|e| e.into_inner()).clone())
                .expect("utf8")
        }
    }

    /// Feed `events` into a filtering gate over `sink`, then end its life
    /// either by a panic (`panic = true`, caught here) or a normal drop.
    fn gate_dies(sink: &Sink, events: Vec<AgentEvent>, panic: bool) {
        let mut gate = RetrySafeGate::with_panic_sink(true, Box::new(sink.clone()));
        for e in events {
            gate.feed(Some(e));
        }
        if panic {
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let _gate = gate;
                panic!("probe: the event loop panicked while text was held");
            }));
            assert!(caught.is_err(), "the probe really panicked");
        } else {
            drop(gate);
        }
    }

    #[test]
    fn panic_while_text_is_held_flushes_it() {
        let sink = Sink::default();
        gate_dies(&sink, vec![start(), text("HEL"), text("D_")], true);
        assert_eq!(sink.bytes(), "HELD_");
    }

    #[test]
    fn panic_flushes_released_but_unprinted_text_before_the_held_attempt() {
        // A finished attempt is released into the queue at its MessageEnd; the
        // loop panics before printing it, while the next attempt is held.
        let sink = Sink::default();
        let mut gate = RetrySafeGate::with_panic_sink(true, Box::new(sink.clone()));
        for e in [start(), text("ONE "), end(StopReason::Stop)] {
            gate.feed(Some(e));
        }
        // Anti-vacuous: the released attempt really is sitting in the queue.
        assert!(!gate.queue.is_empty());
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _gate = gate;
            panic!("probe");
        }));
        assert!(caught.is_err());
        assert_eq!(sink.bytes(), "ONE ");
    }

    /// A non-panicking drop with NO close still writes nothing through `Drop`,
    /// and that is not the defect. The Day-228 reading of this row ("it pins
    /// the lost text") was narrower than it looked: the row never feeds
    /// `None`, and the defect state is "stream CLOSED inside an un-ended
    /// attempt", which production always reaches through `feed(None)` before
    /// the gate drops. That state is now released into the queue at close (see
    /// `stream_closing_inside_an_attempt_reaches_the_loop_exactly_once`), so
    /// the loop prints it and this drop has nothing left to write. Production
    /// leaves the loop without a close only through the signal returns, which
    /// take the held text first (#993/#1015).
    #[test]
    fn normal_drop_without_a_close_writes_nothing_through_drop() {
        let sink = Sink::default();
        gate_dies(&sink, vec![start(), text("HELD_")], false);
        assert_eq!(sink.bytes(), "");
    }

    /// Drive the gate the way `handle_prompt_events` does: real channel, real
    /// `next()`, until `None`. Returns the text deltas the loop would print.
    async fn loop_prints(filtering: bool, events: Vec<AgentEvent>, sink: &Sink) -> String {
        let mut gate = RetrySafeGate::with_panic_sink(filtering, Box::new(sink.clone()));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        for e in events {
            tx.send(e).expect("send");
        }
        drop(tx); // the agent task is gone: the channel closes
        let mut out = String::new();
        while let Some(event) = gate.next(&mut rx).await {
            if let AgentEvent::MessageUpdate {
                delta: StreamDelta::Text { delta },
                ..
            } = event
            {
                out.push_str(&delta);
            }
        }
        drop(gate); // normal drop, not panicking
        out
    }

    fn agent_end() -> AgentEvent {
        AgentEvent::agent_end(Vec::new(), yoagent::SessionStats::default())
    }

    /// Drive `next()` over a real channel to `None`, as the loop does, and
    /// return whether the gate saw the run end without its `AgentEnd`.
    async fn ends_without_agent_end(filtering: bool, events: Vec<AgentEvent>) -> bool {
        let mut gate = RetrySafeGate::with_panic_sink(filtering, Box::new(Sink::default()));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        for e in events {
            tx.send(e).expect("send");
        }
        drop(tx);
        // Before the close is seen, nothing is reported: the run may still end.
        assert!(!gate.ended_without_agent_end());
        while gate.next(&mut rx).await.is_some() {}
        gate.ended_without_agent_end()
    }

    /// #1016 (the 2026-10-24T09:36Z row: "exit code for the closed-mid-attempt
    /// ending"): the stream closing inside an attempt, with no `AgentEnd`, is
    /// what `report_unfinished_run` keys on, on a pipe (filter on) and on a
    /// terminal (filter off) alike. Before this fix nothing recorded it and
    /// the run exited 0, the same as a finished reply.
    #[tokio::test]
    async fn stream_closed_mid_attempt_counts_as_an_unfinished_run() {
        let cut = vec![start(), text("CUT"), text("_OFF")];
        assert!(ends_without_agent_end(true, cut.clone()).await);
        assert!(ends_without_agent_end(false, cut).await);
        // Between messages (e.g. mid-tool) the run is just as unfinished.
        let between = vec![start(), text("A"), end(StopReason::Stop)];
        assert!(ends_without_agent_end(true, between.clone()).await);
        assert!(ends_without_agent_end(false, between).await);
    }

    /// Near-misses: every ending yoagent reaches sends `AgentEnd`, and those
    /// are not unfinished, whether the attempt completed, was retried and
    /// then completed, or ended in a final error (that one fails on its own
    /// path, `last_api_error`, not this one).
    #[tokio::test]
    async fn runs_that_reach_agent_end_are_not_unfinished() {
        let done = vec![start(), text("DONE"), end(StopReason::Stop), agent_end()];
        let retried = vec![
            start(),
            text("PARTIAL_"),
            end(StopReason::Error),
            retry(),
            start(),
            text("DONE"),
            end(StopReason::Stop),
            agent_end(),
        ];
        let failed = vec![start(), text("X"), end(StopReason::Error), agent_end()];
        for filtering in [true, false] {
            assert!(!ends_without_agent_end(filtering, done.clone()).await);
            assert!(!ends_without_agent_end(filtering, retried.clone()).await);
            assert!(!ends_without_agent_end(filtering, failed.clone()).await);
        }
    }

    /// Near-miss on stdout: a completed, retried-then-completed run prints
    /// byte-identically with its `AgentEnd` on the wire, filter on and off.
    #[tokio::test]
    async fn agent_end_does_not_change_what_the_loop_prints() {
        let retried = vec![
            start(),
            text("PARTIAL_"),
            end(StopReason::Error),
            retry(),
            start(),
            text("DONE"),
            end(StopReason::Stop),
            agent_end(),
        ];
        let sink = Sink::default();
        assert_eq!(loop_prints(true, retried, &sink).await, "DONE");
        assert_eq!(sink.bytes(), "");
        let done = vec![start(), text("DONE"), end(StopReason::Stop), agent_end()];
        assert_eq!(loop_prints(false, done, &sink).await, "DONE");
        assert_eq!(sink.bytes(), "");
    }

    /// #1016, emission point. At HEAD before this fix the loop printed "" and
    /// the sink got "": the cut-off attempt's text was lost on this one ending.
    #[tokio::test]
    async fn stream_closing_inside_an_attempt_reaches_the_loop_exactly_once() {
        let sink = Sink::default();
        let out = loop_prints(true, vec![start(), text("CUT"), text("_OFF")], &sink).await;
        assert_eq!(out, "CUT_OFF");
        assert_eq!(
            sink.bytes(),
            "",
            "Drop must not repeat what the loop printed"
        );
    }

    /// The near-miss that matters (#976/#989): a retried attempt, then the
    /// stream closes with nothing new, prints nothing.
    #[tokio::test]
    async fn retried_attempt_then_close_prints_nothing() {
        let sink = Sink::default();
        let out = loop_prints(
            true,
            vec![start(), text("PARTIAL_"), end(StopReason::Error), retry()],
            &sink,
        )
        .await;
        assert_eq!(out, "");
        assert_eq!(sink.bytes(), "");
        // Same, when the retry's attempt was open but streamed no text yet.
        let sink = Sink::default();
        let out = loop_prints(
            true,
            vec![
                start(),
                text("PARTIAL_"),
                end(StopReason::Error),
                retry(),
                start(),
            ],
            &sink,
        )
        .await;
        assert_eq!(out, "");
        assert_eq!(sink.bytes(), "");
    }

    /// A complete attempt then a close: byte-identical to before (once).
    #[tokio::test]
    async fn complete_attempt_then_close_prints_once() {
        let sink = Sink::default();
        let out = loop_prints(
            true,
            vec![start(), text("DONE"), end(StopReason::Stop)],
            &sink,
        )
        .await;
        assert_eq!(out, "DONE");
        assert_eq!(sink.bytes(), "");
    }

    /// Filter off (terminal stdout): every delta passes through as before,
    /// and the close adds nothing.
    #[tokio::test]
    async fn filter_off_close_inside_an_attempt_is_unchanged() {
        let sink = Sink::default();
        let out = loop_prints(false, vec![start(), text("CUT"), text("_OFF")], &sink).await;
        assert_eq!(out, "CUT_OFF");
        assert_eq!(sink.bytes(), "");
    }

    #[test]
    fn a_retried_attempt_never_reaches_stdout_through_drop() {
        let retried = vec![start(), text("PARTIAL_"), end(StopReason::Error), retry()];
        for panic in [false, true] {
            let sink = Sink::default();
            gate_dies(&sink, retried.clone(), panic);
            assert_eq!(sink.bytes(), "", "panic={panic}");
        }
        // Anti-vacuous: the same failure WITHOUT the retry is still held, and
        // a panic flushes it, so the empty rows above are the retry's doing.
        let sink = Sink::default();
        gate_dies(
            &sink,
            vec![start(), text("PARTIAL_"), end(StopReason::Error)],
            true,
        );
        assert_eq!(sink.bytes(), "PARTIAL_");
    }

    #[test]
    fn filter_off_gate_never_writes_on_panic() {
        // Off: the loop printed every delta itself; Drop must not repeat any.
        let sink = Sink::default();
        let mut gate = RetrySafeGate::with_panic_sink(false, Box::new(sink.clone()));
        gate.feed(Some(text("SHOWN")));
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _gate = gate;
            panic!("probe");
        }));
        assert!(caught.is_err());
        assert_eq!(sink.bytes(), "");
    }

    #[test]
    fn only_a_single_shot_unreserved_pipe_is_filtered() {
        // (single_shot, reserved, tty) -> filter?
        let table = [
            (true, false, false, true),   // -p | cat: the door #991 is about
            (true, false, true, false),   // -p on a terminal: unchanged
            (true, true, false, false),   // --print / json: already held back
            (false, false, false, false), // REPL with stdout piped: unchanged
            (false, false, true, false),
            (true, true, true, false),
        ];
        for (single, reserved, tty, want) in table {
            assert_eq!(
                retry_safe_pipe_applies(single, reserved, tty),
                want,
                "single={single} reserved={reserved} tty={tty}"
            );
        }
    }

    #[test]
    fn retried_attempt_text_never_reaches_the_loop() {
        let mut gate = RetrySafeGate::new(true);
        let out = printed(
            &mut gate,
            vec![
                start(),
                text("PARTIAL_"),
                end(StopReason::Error),
                retry(),
                start(),
                text("PONG"),
                end(StopReason::Stop),
            ],
        );
        assert_eq!(out, vec!["PONG".to_string()]);
    }

    #[test]
    fn final_failure_keeps_its_partial_once() {
        let mut gate = RetrySafeGate::new(true);
        let out = printed(
            &mut gate,
            vec![
                start(),
                text("PART"),
                text("IAL_"),
                end(StopReason::Error),
                retry(),
                start(),
                text("PARTIAL_"),
                end(StopReason::Error),
            ],
        );
        // Anti-vacuous: the retried copy is gone, the final one survives once.
        assert_eq!(out, vec!["PARTIAL_".to_string()]);
    }

    /// #1016: the channel closing inside an attempt (no `MessageEnd`) is the
    /// state yoagent's agent task leaves when it dies — every path it returns
    /// from closes the attempt it opened. The text is released, once, at close.
    #[test]
    fn stream_closing_inside_an_attempt_releases_its_text() {
        let mut gate = RetrySafeGate::new(true);
        let out = printed(&mut gate, vec![start(), text("CUT"), text("_OFF")]);
        assert_eq!(out, vec!["CUT_OFF".to_string()]);
        assert_eq!(gate.take_held_text(), "", "released, not also held");
    }

    /// Near-miss: a retried attempt stays gone even when the NEXT attempt is
    /// the one cut off — only the open attempt's own text is released.
    #[test]
    fn close_inside_the_retry_attempt_releases_only_that_attempt() {
        let mut gate = RetrySafeGate::new(true);
        let out = printed(
            &mut gate,
            vec![
                start(),
                text("PARTIAL_"),
                end(StopReason::Error),
                retry(),
                start(),
                text("SECOND"),
            ],
        );
        assert_eq!(out, vec!["SECOND".to_string()]);
    }

    /// Near-miss: a finished attempt then a close prints its text exactly once.
    #[test]
    fn close_after_a_finished_attempt_adds_nothing() {
        let mut gate = RetrySafeGate::new(true);
        let out = printed(
            &mut gate,
            vec![start(), text("DONE"), end(StopReason::Stop)],
        );
        assert_eq!(out, vec!["DONE".to_string()]);
    }

    #[test]
    fn held_text_is_available_to_an_interrupt() {
        let mut gate = RetrySafeGate::new(true);
        gate.feed(Some(start()));
        gate.feed(Some(text("ABC")));
        assert!(gate.queue.is_empty(), "the open attempt is held");
        assert_eq!(gate.take_held_text(), "ABC");
        assert_eq!(gate.take_held_text(), "", "taken once");
    }

    /// The loop's view: pop ONE event, as one `select!` iteration does.
    fn pop_one(gate: &mut RetrySafeGate) -> Option<AgentEvent> {
        gate.queue.pop_front()
    }

    /// #1016 (row from the 2026-10-15T01:04Z comment): an attempt that got
    /// its `MessageEnd` is released into the queue all at once; the loop pops
    /// one event per `select!`, so a signal arm can win while the rest sit
    /// unprinted. Probed before the fix: the interrupt flush returned `""`.
    #[test]
    fn interrupt_flush_keeps_released_but_unprinted_text() {
        let mut gate = RetrySafeGate::new(true);
        gate.feed(Some(start()));
        gate.feed(Some(text("REL")));
        gate.feed(Some(text("EASED")));
        gate.feed(Some(end(StopReason::Stop)));
        assert!(matches!(
            pop_one(&mut gate),
            Some(AgentEvent::MessageStart { .. })
        ));
        assert_eq!(
            gate.queue.len(),
            3,
            "anti-vacuous: text sits released, unprinted"
        );
        assert_eq!(gate.take_unprinted_text(), "RELEASED");
        assert_eq!(gate.take_unprinted_text(), "", "taken once");
    }

    /// Wire order: a finished attempt's released text comes before the next
    /// attempt's held text, and neither is lost.
    #[test]
    fn interrupt_flush_puts_released_text_before_held_text() {
        let mut gate = RetrySafeGate::new(true);
        for e in [start(), text("FIRST_"), end(StopReason::Stop)] {
            gate.feed(Some(e));
        }
        gate.feed(Some(start()));
        gate.feed(Some(text("SECOND")));
        assert_eq!(gate.take_unprinted_text(), "FIRST_SECOND");
    }

    /// Near-miss: text the loop already printed is not printed again.
    #[test]
    fn interrupt_flush_after_the_loop_drained_the_queue_is_empty() {
        let mut gate = RetrySafeGate::new(true);
        for e in [start(), text("DONE"), end(StopReason::Stop)] {
            gate.feed(Some(e));
        }
        while pop_one(&mut gate).is_some() {}
        assert_eq!(gate.take_unprinted_text(), "");
    }

    /// Near-miss: a retried attempt is cleared by `ProviderRetry`, never
    /// released, so an interrupt during the retry keeps only the live one.
    #[test]
    fn interrupt_flush_never_releases_a_retried_attempt() {
        let mut gate = RetrySafeGate::new(true);
        for e in [
            start(),
            text("PARTIAL_"),
            end(StopReason::Error),
            retry(),
            start(),
            text("LIVE"),
        ] {
            gate.feed(Some(e));
        }
        assert_eq!(gate.take_unprinted_text(), "LIVE");
    }

    /// Near-miss: nothing held, nothing queued -> nothing to write (the
    /// interrupt path's bytes are unchanged from before).
    #[test]
    fn interrupt_flush_with_nothing_left_is_empty() {
        assert_eq!(RetrySafeGate::new(true).take_unprinted_text(), "");
        assert_eq!(RetrySafeGate::new(false).take_unprinted_text(), "");
    }

    #[test]
    fn filter_off_passes_every_event_through_unchanged() {
        let mut gate = RetrySafeGate::new(false);
        let out = printed(
            &mut gate,
            vec![
                start(),
                text("PARTIAL_"),
                end(StopReason::Error),
                retry(),
                start(),
                text("PONG"),
                end(StopReason::Stop),
            ],
        );
        assert_eq!(out, vec!["PARTIAL_".to_string(), "PONG".to_string()]);
        assert_eq!(gate.take_held_text(), "");
    }
}
