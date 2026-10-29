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
/// Scope: all five retry sites, including yoagent's internal `ProviderRetry`
/// door (`handle_provider_retry`, #989). On that door yoagent has ALREADY
/// decided to retry; yoyo can only abort or let it through. Let through on a
/// pipe, the reader sees the dying partial, its line closed, one blank line,
/// then the retried answer (`PARTIAL_\n\nPONG\n`, measured Day 225 under
/// `opt_in_provider_retry_after_partial_succeeds_with_attempts_separated`):
/// the renderer's own newlines (the error-stop `MessageEnd` closes the line,
/// the next attempt's first text delta opens with one) keep the two attempts
/// apart, so no extra separator is emitted here.
///
/// Superseded claim, recorded rather than erased (Day 225): this paragraph
/// said the `ProviderRetry` door deliberately ignores the opt-in because #991
/// (`retry_safe_events`) would answer that door by making duplicates
/// impossible. #991 has not landed, and yoagent >= 0.24.3 (#1007) reclassifies
/// Anthropic 529 / `overloaded_error` as a retried `RateLimited`, which moves
/// Overloaded onto this door: an opted-in loop would have lost the turn again
/// (the Day-219 loss). The upgrade needed a decision before #991, so the door
/// now reads the opt-in. Default (off) is unchanged: it still aborts (#989).
/// That upgrade landed Day 225 (#1007): 0.25.2's `ProviderError::classify`
/// sends 529 and in-stream `overloaded_error` to the retried `RateLimited`
/// (`provider/traits.rs:299-306`, `:435`), so the reclassification is live.
///
/// #991 (Day 226): on a single-shot run whose stdout is an unreserved pipe,
/// `handle_prompt_events` (`src/prompt.rs`) reads its events through
/// `retry_safe_pipe::RetrySafeGate`, i.e. yoagent's `RetrySafeEvents`, so a
/// yoagent-retried attempt never reaches the pipe and its `ProviderRetry` arm
/// skips `handle_provider_retry` entirely (`!gate.is_filtering() && ...`):
/// on that path this door neither aborts nor reads the opt-in, because there
/// is nothing on the pipe to duplicate. The paragraph above (the opt-in on
/// this door, `PARTIAL_\n\nPONG\n`) now describes only the paths the gate
/// does not cover: a terminal (where `should_retry_after_partial` allows the
/// retry anyway) and the REPL with stdout piped. yoyo's OWN four retry sites
/// still consult this rule on every path: the gate hands a final failure its
/// partial text back, so a pipe that holds it is still not retried by default.
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
/// `opted_in`: the #997 opt-in, read by every caller (Day 225: `handle_provider_retry` too).
pub(super) fn retry_blocked_by_streamed_partial(dying_text: &str, opted_in: bool) -> bool {
    let streamed = !crate::format::stdout_reserved() && !dying_text.is_empty();
    !retry_after_partial_allowed(opted_in, std::io::stdout().is_terminal(), streamed)
}

/// The one stderr line printed when [`should_retry_after_partial`] refuses.
/// Glyph-free on purpose, so it needs no plain-output variant.
pub(super) const RETRY_SKIPPED_AFTER_PARTIAL_NOTE: &str = "(not retrying: part of the answer was \
already written to stdout, which is not a terminal, and a retry would print it again; rerun the \
command)";

/// #989/#991: after yoagent retries in-loop, drop the dying attempt's text
/// (everything from `start`, the byte offset its `MessageStart` recorded) from
/// both accumulators, so the outcome holds the retry's text, not both joined.
/// Shared by the text loop and the stream-json loop: one cut, never two copies.
/// Char-boundary safe (#250): `start` is a past `len()`, and a mismatch leaves
/// the text alone rather than slicing inside a character.
pub(super) fn drop_dying_attempt(collected: &mut String, since_tool: &mut String, start: usize) {
    let Some(dying) = collected.get(start.min(collected.len())..) else {
        return; // not a char boundary: leave the text alone
    };
    let dying_len = dying.len();
    if dying_len == 0 {
        return;
    }
    // `since_tool` was cleared at the last tool end, so it holds the dying
    // text as a suffix only when no tool ran inside the dying attempt.
    if since_tool.ends_with(dying) {
        let keep = since_tool.len() - dying_len;
        if since_tool.is_char_boundary(keep) {
            since_tool.truncate(keep);
        }
    }
    let keep = collected.len() - dying_len;
    collected.truncate(keep);
}

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
