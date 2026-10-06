//! #976: whether a turn that died mid-stream may be retried.
//!
//! Plain `-p` and piped stdin stream answer text to stdout as it arrives. A
//! retry re-runs the same prompt, so on a pipe it wrote the dead attempt's
//! partial answer once per attempt (six copies measured Day 216 under the
//! stub test `armed_doors_keep_streamed_partial_answer_when_turn_dies_mid_stream`).
//! Bytes on a pipe cannot be retracted, so the fix is a decision made BEFORE
//! the retry. Every retry site in `prompt.rs` asks this one function.

use std::io::IsTerminal;

/// May a turn that died with a retriable error be re-run?
///
/// Only `(false, true)` says no: stdout is not a terminal AND the dying attempt
/// already streamed answer text to it. A terminal is unchanged (a human can see
/// the restart), and an attempt that died before writing anything (the common
/// overload-at-request-start case) retries exactly as before. This only ever
/// narrows retries, and only where retrying corrupts the output.
pub(super) fn should_retry_after_partial(
    stdout_is_terminal: bool,
    streamed_this_attempt: bool,
) -> bool {
    stdout_is_terminal || !streamed_this_attempt
}

/// [`should_retry_after_partial`] widened by the `retry_after_partial` opt-in
/// (#997, default **off**, so the default is byte-identical to #976).
///
/// The trade, named rather than hidden: opted in, a pipe that already holds the
/// dying attempt's partial text gets that text AGAIN from the retry (duplicate
/// partial on stdout) in exchange for not losing the turn to one transient
/// `Overloaded`. The dying attempt's text is still dropped from the returned
/// answer — only the bytes already written to the pipe stay. The retry cap is
/// unchanged, so an every-attempt failure still ends nonzero.
///
/// Scope: yoyo's own retry loops only. yoagent's internal `ProviderRetry`
/// door (`handle_provider_retry`, #989) does not consult the opt-in, because
/// #991 (`retry_safe_events`) answers that door by making duplicates
/// impossible rather than accepting them. That asymmetry is deliberate, not a
/// "two doors, one deaf" miss.
pub(super) fn retry_after_partial_allowed(
    opted_in: bool,
    stdout_is_terminal: bool,
    streamed_this_attempt: bool,
) -> bool {
    opted_in || should_retry_after_partial(stdout_is_terminal, streamed_this_attempt)
}

/// Live-state wrapper both retry loops call: one condition, never two copies.
/// The dying attempt's text reached stdout only when stdout is NOT reserved for
/// a final payload (`--print`/json hold it back, so they retry safely).
/// `opted_in`: the #997 opt-in (yoyo's loops) or `false` (`handle_provider_retry`).
pub(super) fn retry_blocked_by_streamed_partial(dying_text: &str, opted_in: bool) -> bool {
    let streamed = !crate::format::stdout_reserved() && !dying_text.is_empty();
    !retry_after_partial_allowed(opted_in, std::io::stdout().is_terminal(), streamed)
}

/// The one stderr line printed when [`should_retry_after_partial`] refuses.
/// Glyph-free on purpose, so it needs no plain-output variant.
pub(super) const RETRY_SKIPPED_AFTER_PARTIAL_NOTE: &str = "(not retrying: part of the answer was \
already written to stdout, which is not a terminal, and a retry would print it again; rerun the \
command)";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_pipe_that_already_holds_text_refuses_the_retry() {
        // (stdout_is_terminal, streamed_this_attempt) -> retry?
        let table = [
            (true, true, true),   // tty: a human sees the restart
            (true, false, true),  // tty, nothing written yet
            (false, false, true), // pipe, died before any text: retry as today
            (false, true, false), // pipe already holds the partial: refuse
        ];
        for (tty, streamed, want) in table {
            assert_eq!(
                should_retry_after_partial(tty, streamed),
                want,
                "tty={tty} streamed={streamed}"
            );
        }
    }

    #[test]
    fn opt_in_only_lifts_the_one_refusing_row() {
        // (opted_in, tty, streamed) -> retry?
        let table = [
            (false, true, true, true),
            (false, true, false, true),
            (false, false, false, true),
            (false, false, true, false), // default: #976 refusal unchanged
            (true, true, true, true),
            (true, true, false, true),
            (true, false, false, true),
            (true, false, true, true), // #997: opted in, the pipe retries
        ];
        for (opted_in, tty, streamed, want) in table {
            assert_eq!(
                retry_after_partial_allowed(opted_in, tty, streamed),
                want,
                "opted_in={opted_in} tty={tty} streamed={streamed}"
            );
        }
    }

    #[test]
    fn skip_note_names_the_reason_and_the_remedy() {
        assert_eq!(
            RETRY_SKIPPED_AFTER_PARTIAL_NOTE,
            "(not retrying: part of the answer was already written to stdout, which is not a \
             terminal, and a retry would print it again; rerun the command)"
        );
        assert!(RETRY_SKIPPED_AFTER_PARTIAL_NOTE.is_ascii());
    }

    /// Weak source-level guard, and it says so: it proves every retry site in
    /// `prompt.rs` CALLS the shared wrapper (two `RetriableError` arms and two
    /// malformed-tool-call resamples), never that the retry decision fires.
    /// The stub test in `tests/print_stdout_contract.rs` is the behavioural proof.
    #[test]
    fn every_retry_site_consults_the_shared_rule() {
        let src = include_str!("../prompt.rs");
        let body = &src[..src.find(&format!("#[cfg({})]\nmod tests", "test")).unwrap()];
        let call = format!("{}(&", "retry_blocked_by_streamed_partial");
        // 5 = four yoyo retry sites + yoagent's internal retry (`handle_provider_retry`, #989).
        assert_eq!(body.matches(&call).count(), 5, "one call per retry site");
        let malformed = format!("{}(", "malformed_tool_call_retry");
        assert_eq!(body.matches(&malformed).count(), 2, "two resample sites");
        let arms = format!(
            "{} {{\n                error_msg,\n",
            "PromptResult::RetriableError"
        );
        assert_eq!(body.matches(&arms).count(), 2, "two looping RetriableError arms (the overflow arm's inner match binds `error_msg: retry_err` and never retries)");
    }
}
