//! Rate-limit aware retry: read a provider's own reset time and decide
//! whether waiting it out is possible at all.
//!
//! Split out of `prompt_retry` on Day 177 — that file crossed the fatal
//! 2000-line module cap when this landed, and this family is a coherent
//! unit: parse the hint, decide, say so. Nothing here does I/O.
//!
//! Why it exists: `retry_delay` caps at ~90s including jitter, so a provider
//! answering "come back in 4 hours" used to be met with five ≤90s sleeps
//! against the same refusal, and a final error that never said when the
//! limit resets. Measured, not hypothetical: two of the four gradeable
//! post-#808 sessions died exactly this way
//! (`scripts/measure_abstentions.py`'s provider-error bucket, Day 175).

use crate::prompt_retry::{retry_delay, MIN_RETRY_DELAY_MS};
use std::time::Duration;

/// Sanity clamp on a parsed retry-after value.
///
/// A provider naming a reset further out than this is clamped rather than
/// rejected: the *decision* (stop retrying) is the same either way, and
/// rejecting it would fall back to exponential backoff — the exact
/// attempt-burning this exists to stop. The reported time then understates
/// the real reset, which is the safer direction to be wrong in.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(24 * 3600);

/// The longest a single retry is willing to sleep inline when no session
/// budget is known.
///
/// **This is a judgment threshold, not a measurement.** Nothing measured says
/// 120s is the right number; it is the point past which sleeping is worse
/// than reporting — a user staring at a silent prompt for minutes has less
/// information than one told when the limit resets.
pub const MAX_INLINE_RETRY_AFTER: Duration = Duration::from_secs(120);

/// Parse an explicit reset time out of a provider error message.
///
/// Recognises only the shapes that actually occur, and returns `None` for
/// everything else — `None` is the byte-identical common path (today's
/// exponential backoff), so a message this cannot read costs a user nothing.
///
/// Handled shapes (case-insensitive), after a `retry after` / `retry_after` /
/// `retry-after` marker:
///
/// - `retry after Some(14454000)ms` — the shape observed in two dead sessions
///   (`scripts/measure_abstentions.py`'s provider-error bucket, Day 175).
/// - `retry after 14454000ms`, `retry_after: 14454000 ms` — same fact, no
///   `Option` wrapper.
/// - `retry-after: 3600` — the HTTP header shape, whose unit is **seconds**.
///
/// Units are the trap: a millisecond value read as seconds turns a 4-hour
/// wait into 4 seconds, and the reverse turns 4 seconds into 4 hours. So a
/// millisecond reading requires an explicit `ms` suffix, a bare number is
/// seconds (the header contract), and any *other* unit word yields `None`
/// rather than a guess.
///
/// Digit-boundary discipline is deliberate (the Day-174 `contains_status_code`
/// lesson, same file): the whole digit run is consumed, so `144` can never be
/// read out of `14454000`, and a value that overflows `u64` returns `None`
/// instead of panicking.
pub fn retry_after_hint(error_msg: &str) -> Option<Duration> {
    let lower = error_msg.to_lowercase();
    for marker in ["retry after", "retry_after", "retry-after"] {
        // `split_once` keeps us on char boundaries without any byte indexing.
        if let Some((_, rest)) = lower.split_once(marker) {
            if let Some(d) = parse_retry_after_value(rest) {
                return Some(d.min(MAX_RETRY_AFTER));
            }
        }
    }
    None
}

/// Pure tail of `retry_after_hint`: read a value + unit from the text that
/// follows the marker. Anything unexpected between the marker and the digits
/// (a word, a sentence) yields `None` rather than scanning ahead for some
/// unrelated number elsewhere in the message.
fn parse_retry_after_value(rest: &str) -> Option<Duration> {
    let mut s = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ':' || c == '=');
    // `Some(14454000)ms` — the Rust `Option` debug wrapper yoagent surfaces.
    if let Some(inner) = s.strip_prefix("some(") {
        s = inner.trim_start();
    }

    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    // Overflow returns None rather than panicking or wrapping.
    let value: u64 = digits.parse().ok()?;

    let after = s.strip_prefix(digits.as_str())?;
    let after = after.trim_start_matches(')').trim_start();
    let unit: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();

    match unit.as_str() {
        "ms" => Some(Duration::from_millis(value)),
        // A bare value after a retry-after marker is seconds (HTTP header).
        "" | "s" | "sec" | "secs" | "second" | "seconds" => Some(Duration::from_secs(value)),
        // Some other unit word ("minutes", "a while") — do not guess.
        _ => None,
    }
}

/// What the retry loop should do about one retriable error.
///
/// Three-valued in spirit and two-valued in code: `Wait` covers both "no
/// reset was named, back off as usual" and "the provider named a short
/// reset, honour it", while `GiveUp` is the case that had no representation
/// at all before — the provider named a reset this session cannot outlast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryWait {
    /// Sleep this long and retry.
    Wait(Duration),
    /// The provider named a reset far enough out that retrying inside this
    /// session cannot succeed. Stop, and say when it resets.
    GiveUp { retry_after: Duration },
}

/// Decide how long to wait before the next retry — or whether to stop.
///
/// Rules, in order:
///
/// 1. No parseable reset time → `Wait(retry_delay(attempt))`. This is
///    **byte-identical to the pre-existing behaviour** and is the path every
///    user takes whose provider does not name a reset.
/// 2. A reset longer than this session can wait → `GiveUp`. Sleeping ≤90s
///    against a 4-hour limit only burns the remaining attempts against the
///    same refusal.
/// 3. Otherwise → `Wait(hint)`: the provider's own number is better
///    information than an exponential guess, floored at `MIN_RETRY_DELAY_MS`
///    so `retry after 1ms` cannot spin.
///
/// `budget_remaining` (from `prompt_budget::session_budget_remaining()`) can
/// only make the ceiling **smaller**, never larger. That is a deliberate
/// deviation from a naive "prefer the budget" reading: a session with three
/// hours left still should not sleep for an hour inside one retry, so the
/// affordable wait is `min(budget, MAX_INLINE_RETRY_AFTER)`.
pub fn retry_wait_decision(
    attempt: u32,
    error_msg: &str,
    budget_remaining: Option<Duration>,
) -> RetryWait {
    let Some(hint) = retry_after_hint(error_msg) else {
        return RetryWait::Wait(retry_delay(attempt));
    };

    let affordable = match budget_remaining {
        Some(remaining) => remaining.min(MAX_INLINE_RETRY_AFTER),
        None => MAX_INLINE_RETRY_AFTER,
    };

    if hint > affordable {
        return RetryWait::GiveUp { retry_after: hint };
    }

    RetryWait::Wait(hint.max(Duration::from_millis(MIN_RETRY_DELAY_MS)))
}

/// The user-facing line printed when a retry-after is too long to wait out.
///
/// Pure so it can be asserted at the emission point. It has three jobs, and
/// the first is the one the old behaviour got wrong: say that yoyo stopped
/// **on purpose**, so a deliberate refusal does not read as a mysterious
/// failure. Duration rendering reuses `format::format_duration` rather than
/// hand-rolling a second formatter.
pub fn rate_limit_giveup_message(retry_after: Duration) -> String {
    format!(
        "stopped retrying on purpose: the provider says its rate limit resets in ~{}. \
         Retrying now cannot succeed — this is not a transient error. \
         Try again after the limit resets.",
        crate::format::format_duration(retry_after)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- retry-after parsing (Day 177) -------------------------------
    // Measured evidence: two post-#808 sessions died on the verbatim string
    // asserted first below, each burning every attempt against a ~4h reset.

    #[test]
    fn test_retry_after_hint_reads_the_observed_shapes() {
        // The exact string from the two dead sessions.
        let observed = retry_after_hint("error: Rate limited, retry after Some(14454000)ms")
            .expect("the observed rate-limit shape must parse");
        assert_eq!(observed, Duration::from_millis(14_454_000));
        assert!(
            observed > Duration::from_secs(4 * 3600) && observed < Duration::from_secs(5 * 3600),
            "14454000ms should read as ~4h, got {observed:?}"
        );

        // Same fact without the Option wrapper.
        assert_eq!(
            retry_after_hint("rate_limit_error: retry after 14454000ms"),
            Some(Duration::from_millis(14_454_000))
        );
        assert_eq!(
            retry_after_hint("429: retry_after: 14454000 ms"),
            Some(Duration::from_millis(14_454_000))
        );

        // HTTP header shape — the unit is SECONDS, not milliseconds. Getting
        // this backwards turns an hour into a second or vice versa.
        assert_eq!(
            retry_after_hint("429 Too Many Requests (retry-after: 3600)"),
            Some(Duration::from_secs(3600))
        );
        assert_eq!(
            retry_after_hint("Retry-After: 30 seconds"),
            Some(Duration::from_secs(30))
        );

        // Case-insensitive.
        assert_eq!(
            retry_after_hint("RETRY AFTER 5S"),
            Some(Duration::from_secs(5))
        );
    }

    #[test]
    fn test_retry_after_hint_near_misses_return_none() {
        // A discriminator tested only on the side that fires is vacuous green,
        // so pin every shape that must NOT be read as a reset time.

        // Ordinary rate-limit message, no number at all.
        assert_eq!(retry_after_hint("rate limit exceeded"), None);
        assert_eq!(retry_after_hint("429 Too Many Requests"), None);

        // Numbers that are not reset times — a token count and a request id.
        // The marker is absent, so nothing may be scavenged from the digits.
        assert_eq!(
            retry_after_hint("prompt is too long: 402134 tokens > 200000 maximum"),
            None
        );
        assert_eq!(
            retry_after_hint("rate limited (request id req_012345678)"),
            None
        );

        // Marker present but the value is prose, not a number.
        assert_eq!(retry_after_hint("rate limited, retry after a while"), None);
        assert_eq!(retry_after_hint("retry after some(unknown)ms"), None);

        // Marker present, number present, but the unit is one we refuse to
        // guess about — "5 minutes" must not silently become 5 seconds.
        assert_eq!(retry_after_hint("retry after 5 minutes"), None);
        assert_eq!(retry_after_hint("retry after 2 hours"), None);

        // Overflowing u64 returns None instead of panicking or wrapping.
        assert_eq!(
            retry_after_hint("retry after 999999999999999999999999ms"),
            None
        );

        // Multi-byte input must not panic anywhere in the scan.
        assert_eq!(retry_after_hint("速度制限 ✓ retry after ✓"), None);
    }

    #[test]
    fn test_retry_after_hint_consumes_the_whole_digit_run() {
        // The Day-174 `contains_status_code` lesson, same file: a partial
        // digit match produces a confidently wrong number. 14454000ms must
        // never be read as 144ms (or any other prefix).
        let d = retry_after_hint("retry after 14454000ms").unwrap();
        assert_eq!(d, Duration::from_millis(14_454_000));
        assert!(d > Duration::from_secs(1), "truncated digit run: {d:?}");
    }

    #[test]
    fn test_retry_after_hint_clamps_absurd_values() {
        // A week-long reset clamps to the 24h sanity ceiling rather than
        // being rejected — the decision (stop retrying) is the same, and
        // rejecting it would fall back to burning attempts.
        let d = retry_after_hint("retry after 604800").expect("must still parse");
        assert_eq!(d, MAX_RETRY_AFTER);
    }

    #[test]
    fn test_retry_wait_decision_without_hint_is_todays_behaviour() {
        // The byte-identity assertion for every user whose provider does not
        // name a reset: same exponential backoff, same jitter window.
        for msg in [
            "429 Too Many Requests",
            "the server is overloaded",
            "connection reset by peer",
        ] {
            match retry_wait_decision(1, msg, None) {
                RetryWait::Wait(d) => assert!(
                    d >= Duration::from_millis(500) && d <= Duration::from_millis(1500),
                    "attempt 1 must stay in retry_delay's range for {msg:?}: {d:?}"
                ),
                other => panic!("no hint must never give up, got {other:?} for {msg:?}"),
            }
        }
        // And a later attempt still tracks retry_delay's cap.
        match retry_wait_decision(20, "server error", Some(Duration::from_secs(2700))) {
            RetryWait::Wait(d) => assert!(d <= Duration::from_secs(90), "not capped: {d:?}"),
            other => panic!("expected Wait, got {other:?}"),
        }
    }

    #[test]
    fn test_retry_wait_decision_gives_up_on_a_long_reset() {
        let four_hours = "error: Rate limited, retry after Some(14454000)ms";

        // 45-minute session budget cannot outlast a 4h reset.
        assert_eq!(
            retry_wait_decision(1, four_hours, Some(Duration::from_secs(45 * 60))),
            RetryWait::GiveUp {
                retry_after: Duration::from_millis(14_454_000)
            }
        );

        // No budget known → the MAX_INLINE_RETRY_AFTER judgment threshold.
        assert_eq!(
            retry_wait_decision(1, four_hours, None),
            RetryWait::GiveUp {
                retry_after: Duration::from_millis(14_454_000)
            }
        );
    }

    #[test]
    fn test_retry_wait_decision_honours_a_short_reset() {
        // The provider's own number beats an exponential guess.
        assert_eq!(
            retry_wait_decision(1, "rate limited, retry-after: 5", None),
            RetryWait::Wait(Duration::from_secs(5))
        );
        // Still honoured when a generous budget exists.
        assert_eq!(
            retry_wait_decision(3, "retry after 30s", Some(Duration::from_secs(3600))),
            RetryWait::Wait(Duration::from_secs(30))
        );
        // A near-instant reset is floored, so the loop cannot spin hot.
        assert_eq!(
            retry_wait_decision(1, "retry after 1ms", None),
            RetryWait::Wait(Duration::from_millis(MIN_RETRY_DELAY_MS))
        );
        // A budget smaller than the hint gives up even below the constant.
        assert_eq!(
            retry_wait_decision(1, "retry after 60s", Some(Duration::from_secs(10))),
            RetryWait::GiveUp {
                retry_after: Duration::from_secs(60)
            }
        );
    }

    #[test]
    fn test_rate_limit_giveup_message_names_the_reset_and_the_intent() {
        // Emission point: the literal string a caller receives.
        let msg = rate_limit_giveup_message(Duration::from_millis(14_454_000));
        assert!(
            msg.contains("~4h 0m"),
            "must name the reset time in human units: {msg}"
        );
        assert!(
            msg.contains("stopped retrying on purpose"),
            "must say the stop was deliberate, not a mystery failure: {msg}"
        );
        assert!(
            msg.contains("not a transient error"),
            "must say a retry will not fix it: {msg}"
        );

        // A short reset renders in its own units, not padded to hours.
        let short = rate_limit_giveup_message(Duration::from_secs(90));
        assert!(short.contains("1m 30s"), "short reset misrendered: {short}");
    }
}
