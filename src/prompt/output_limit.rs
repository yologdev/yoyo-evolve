//! A reply cut off at the output-token limit says so on stderr.
//!
//! Before this module `StopReason::Length` was classified `Ignore`
//! (`prompt_retry::classify_stop_reason`) and nothing else in `src/` read it,
//! so `yoyo --max-tokens 30 -p "numbers 1 to 200"` printed `1 … 15`, exited 0
//! and wrote nothing to stderr: a truncated answer reported as a complete one.
//!
//! yoagent 0.27.1 does not continue on `Length` by itself: `agent_loop.rs:947`
//! maps it to `RunEnd::Completed`. Since 0.27.0 `on_stop` IS called for it (it
//! said not, 0.25.2), so an extension could continue; yoyo installs none (#1142).
//! So the cut-off message is the turn's end, and the note keys on the FINAL assistant
//! message only: a `Length` stop that was followed by more turns (tool calls
//! in a truncated message) is not what the user sees as the answer ending.
//!
//! What each door's reader can tell, which is why the mechanism is one stderr
//! line for all of them: `-p` into a pipe — the reader sees only stdout bytes
//! and cannot tell a cut-off from an ending; `-p` on a terminal and the REPL —
//! the human sees the text stop mid-sentence, but not why. Stdout stays
//! exactly as streamed.
//!
//! The exit code follows WHO SET THE LIMIT (@yuanhao's decision on #1026,
//! 2026-11-03): a `max_tokens` the user set (flag or config, which is exactly
//! when `agent.max_tokens` is `Some`) exits 0 with the note, because a user who
//! sets a low limit on purpose would otherwise see every run fail; a limit the
//! user never chose (a preset or the base request default) exits 1, because a
//! script reading only stdout would take the cut answer as complete. A refusal
//! exits 1 too: no answer was produced. `final_incomplete` reads the final
//! stop, `incomplete_forces_failure` is the whole decision.
//!
//! Volume is a warning (yellow, `warning:`), never DIM: a truncated answer is
//! not trivia. Not quiet-gated, like `max_tokens_ceiling_warning`.

use super::PromptEventState;
use crate::format::{is_plain_output, RESET, YELLOW};
use crate::prompt_retry::{classify_stop_reason, StopHandling};
use yoagent::{AgentMessage, Message, StopReason};

/// Why a run's final reply is not a complete answer (#1026). `None` on
/// `PromptOutcome::incomplete` means the reply ended normally (or failed in a
/// way another field already records).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incomplete {
    /// Cut at an output limit the user never chose (preset / base default).
    OutputLimitDefault,
    /// Cut at a `max_tokens` the user set by flag or config.
    OutputLimitUserSet,
    /// The model refused: no answer was produced.
    Refusal,
}

/// The exit-code half of the owner's decision on #1026: does this ending make
/// the run a failure? Only a user-set limit is a chosen, complete-as-asked end.
pub fn incomplete_forces_failure(incomplete: Option<Incomplete>) -> bool {
    match incomplete {
        None | Some(Incomplete::OutputLimitUserSet) => false,
        Some(Incomplete::OutputLimitDefault) | Some(Incomplete::Refusal) => true,
    }
}

/// The `incomplete` field of the `--output-format json` result object (#1026
/// item 1): `null` when the final reply ended normally (a failed run is
/// already `is_error`), else `{"reason": "output_limit", "limit_set_by":
/// "user" | "default"}` or `{"reason": "refusal"}`. A field of its own, not a
/// new meaning for `is_error` (the #895 `external_servers` precedent).
pub fn incomplete_json(incomplete: Option<Incomplete>) -> serde_json::Value {
    match incomplete {
        None => serde_json::Value::Null,
        Some(Incomplete::OutputLimitDefault) => {
            serde_json::json!({"reason": "output_limit", "limit_set_by": "default"})
        }
        Some(Incomplete::OutputLimitUserSet) => {
            serde_json::json!({"reason": "output_limit", "limit_set_by": "user"})
        }
        Some(Incomplete::Refusal) => serde_json::json!({"reason": "refusal"}),
    }
}

/// The stop reason of the LAST assistant message of the run, if any.
fn final_stop_reason(messages: &[AgentMessage]) -> Option<&StopReason> {
    messages.iter().rev().find_map(|m| match m {
        AgentMessage::Llm(Message::Assistant { stop_reason, .. }) => Some(stop_reason),
        _ => None,
    })
}

/// True when the LAST assistant message of the run stopped on the output
/// limit. Reads through `classify_stop_reason` so the routing lives in one
/// place: `StopHandling::OutputLimit` is the `Length` arm.
pub(super) fn final_stop_hit_output_limit(messages: &[AgentMessage]) -> bool {
    final_stop_reason(messages)
        .is_some_and(|r| classify_stop_reason(r) == StopHandling::OutputLimit)
}

/// How the run's FINAL assistant message ended, for the exit code: earlier
/// turns that were cut or refused and then continued do not count.
/// `max_tokens` is `agent.max_tokens`, `Some` only when the user set it.
pub(super) fn final_incomplete(
    messages: &[AgentMessage],
    max_tokens: Option<u32>,
) -> Option<Incomplete> {
    match classify_stop_reason(final_stop_reason(messages)?) {
        StopHandling::OutputLimit if max_tokens.is_some() => Some(Incomplete::OutputLimitUserSet),
        StopHandling::OutputLimit => Some(Incomplete::OutputLimitDefault),
        StopHandling::RefusalNotice => Some(Incomplete::Refusal),
        StopHandling::InspectError | StopHandling::Ignore => None,
    }
}

/// The stderr line for a reply cut off at the output limit. Pure; colour is
/// added by the caller. Glyph-free when `plain`.
pub(crate) fn output_limit_note(max_tokens: Option<u32>, plain: bool) -> String {
    let limit = match max_tokens {
        Some(n) => format!("max_tokens = {n}"),
        None => "the model's default max_tokens".to_string(),
    };
    let marker = if plain { "" } else { "⚠ " };
    format!(
        "{marker}warning: reply stopped at the output limit ({limit}); \
         raise --max-tokens or max_tokens in .yoyo.toml"
    )
}

/// The AgentEnd step of `handle_prompt_events`: the existing handling first
/// (it flushes any held stdout text), then the note, so on a terminal the
/// warning lands after the last streamed byte.
pub(super) fn end_turn(
    state: &mut PromptEventState,
    messages: Vec<AgentMessage>,
    model: &str,
    max_tokens: Option<u32>,
) {
    let cut = final_stop_hit_output_limit(&messages);
    state.incomplete = final_incomplete(&messages, max_tokens);
    state.handle_agent_end(messages, model);
    if cut {
        print_output_limit_note(max_tokens);
    }
}

/// The one printer of the output-limit note, shared by the text path's
/// `end_turn` and stream-json's AgentEnd arm (#1026 item 1), so the two
/// doors cannot drift into two wordings. stderr only: stream-json's stdout
/// is pure JSON lines.
pub(super) fn print_output_limit_note(max_tokens: Option<u32>) {
    let note = output_limit_note(max_tokens, is_plain_output());
    eprintln!("\n{YELLOW}  {note}{RESET}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use yoagent::{Content, StopReason, Usage};

    fn assistant(stop_reason: StopReason) -> AgentMessage {
        let text = vec![Content::Text {
            text: "1 2 3".into(),
        }];
        AgentMessage::Llm(Message::assistant(
            text,
            stop_reason,
            "m",
            "p",
            Usage::default(),
        ))
    }

    fn user(text: &str) -> AgentMessage {
        AgentMessage::Llm(Message::user(text))
    }

    #[test]
    fn output_limit_note_table() {
        let tail = "raise --max-tokens or max_tokens in .yoyo.toml";
        let cases = [
            (
                Some(30),
                true,
                format!("warning: reply stopped at the output limit (max_tokens = 30); {tail}"),
            ),
            (
                Some(30),
                false,
                format!("⚠ warning: reply stopped at the output limit (max_tokens = 30); {tail}"),
            ),
            (
                None,
                true,
                format!(
                    "warning: reply stopped at the output limit \
                     (the model's default max_tokens); {tail}"
                ),
            ),
            (
                None,
                false,
                format!(
                    "⚠ warning: reply stopped at the output limit \
                     (the model's default max_tokens); {tail}"
                ),
            ),
        ];
        for (max, plain, want) in cases {
            let got = output_limit_note(max, plain);
            assert_eq!(got, want, "max={max:?} plain={plain}");
            if plain {
                assert!(got.is_ascii(), "plain note must be glyph-free: {got:?}");
            }
        }
    }

    #[test]
    fn length_routes_to_the_output_limit_note_and_normal_ends_do_not() {
        assert_eq!(
            classify_stop_reason(&StopReason::Length),
            StopHandling::OutputLimit
        );
        assert!(final_stop_hit_output_limit(&[
            user("hi"),
            assistant(StopReason::Length)
        ]));
        // Near-miss: every normal ending stays silent, as before.
        for r in [StopReason::Stop, StopReason::ToolUse, StopReason::Aborted] {
            assert_ne!(classify_stop_reason(&r), StopHandling::OutputLimit);
            assert!(
                !final_stop_hit_output_limit(&[user("hi"), assistant(r.clone())]),
                "{r:?} must not warn"
            );
        }
        assert!(!final_stop_hit_output_limit(&[]));
    }

    #[test]
    fn only_the_final_assistant_message_counts() {
        // A Length stop followed by a later, normally-ended turn: the answer
        // the user sees ended normally, so no note.
        assert!(!final_stop_hit_output_limit(&[
            assistant(StopReason::Length),
            user("tool result"),
            assistant(StopReason::Stop),
        ]));
        // A trailing non-assistant message does not hide the final stop.
        assert!(final_stop_hit_output_limit(&[
            assistant(StopReason::Stop),
            user("tool result"),
            assistant(StopReason::Length),
            user("stop marker"),
        ]));
    }

    /// #1026, the owner's decision as a table: who set the limit decides.
    #[test]
    fn incomplete_forces_failure_table() {
        let rows = [
            (None, false),
            (Some(Incomplete::OutputLimitUserSet), false),
            (Some(Incomplete::OutputLimitDefault), true),
            (Some(Incomplete::Refusal), true),
        ];
        for (incomplete, fails) in rows {
            assert_eq!(
                incomplete_forces_failure(incomplete),
                fails,
                "{incomplete:?}"
            );
        }
    }

    /// The provenance split: `max_tokens` is `Some` only when the user set it,
    /// and only the FINAL assistant message decides.
    #[test]
    fn final_incomplete_reads_the_final_stop_and_who_set_the_limit() {
        let cut = [user("hi"), assistant(StopReason::Length)];
        assert_eq!(
            final_incomplete(&cut, None),
            Some(Incomplete::OutputLimitDefault)
        );
        assert_eq!(
            final_incomplete(&cut, Some(30)),
            Some(Incomplete::OutputLimitUserSet)
        );
        let refused = [user("hi"), assistant(StopReason::Refusal)];
        assert_eq!(final_incomplete(&refused, None), Some(Incomplete::Refusal));
        assert_eq!(
            final_incomplete(&refused, Some(30)),
            Some(Incomplete::Refusal)
        );
        // Near-misses: normal ends, errors, and an earlier cut followed by a
        // normal final turn are all `None`.
        for r in [
            StopReason::Stop,
            StopReason::ToolUse,
            StopReason::Aborted,
            StopReason::Error,
        ] {
            assert_eq!(
                final_incomplete(&[assistant(r.clone())], None),
                None,
                "{r:?}"
            );
        }
        let continued = [
            assistant(StopReason::Length),
            user("tool result"),
            assistant(StopReason::Stop),
        ];
        assert_eq!(final_incomplete(&continued, None), None);
        assert_eq!(final_incomplete(&[], None), None);
    }
}
