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

/// The longest a single retry will sleep when the user has **opted in** with
/// `--wait-for-reset`, and no session budget is known.
///
/// **This is a judgment threshold, not a measurement.** Nothing measured says
/// 6h is the right number; it is the point past which a "wait" is really
/// "come back tomorrow", and it exists so that opting in cannot mean an
/// *unbounded* sleep — [`MAX_RETRY_AFTER`]'s 24h absurd-value clamp is the
/// only other bound in this file, and 24h is not a wait.
pub const MAX_RESET_WAIT: Duration = Duration::from_secs(6 * 3600);

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

/// The ceiling on an inline retry wait: never more than [`MAX_INLINE_RETRY_AFTER`],
/// and never more than the session has left. The budget can only **shrink** the
/// ceiling, never raise it — a session with three hours left still must not sleep
/// an hour inside one retry.
///
/// Extracted from the expression it used to be inline in as the corollary test of
/// blind round 80: cargo-mutants' two genres are `FnValue` (replace a whole function
/// *body* with a value guessed from the return type) and operator replacement, so a
/// clamp buried inside a larger function is invisible to it while a clamp with its
/// own function boundary is not. See the mutation-testing block in CLAUDE.md.
fn inline_retry_ceiling(budget_remaining: Duration) -> Duration {
    budget_remaining.min(MAX_INLINE_RETRY_AFTER)
}

/// The ceiling when the user opted into `--wait-for-reset`: never more than
/// [`MAX_RESET_WAIT`], and still never more than the session has left.
///
/// Sibling of [`inline_retry_ceiling`], and deliberately a separate `fn` rather
/// than one parameterised clamp: cargo-mutants cannot substitute one method call
/// for another, so a clamp only enters the mutant population when it owns a
/// function boundary (blind round 80 — see the mutation-testing block in
/// CLAUDE.md). Two boundaries, two mutants; one shared clamp would be one.
fn reset_wait_ceiling(budget_remaining: Duration) -> Duration {
    budget_remaining.min(MAX_RESET_WAIT)
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
///
/// This is the thin impure wrapper: it reads the process-global
/// `cli::is_wait_for_reset()` and delegates every decision to
/// [`retry_wait_decision_with`], so a test never has to write a global (the
/// `apply_effort_hint` / `apply_effort_hint_with` split in `prompt.rs`, Day 177).
/// It also emits the long-wait notice, because an invisible multi-hour sleep is
/// a bug even when it is the right sleep (the #794 `⚡ auto-continuing` rule).
/// The 3-argument signature is unchanged, so `prompt.rs`'s two retry loops need
/// no edit.
pub fn retry_wait_decision(
    attempt: u32,
    error_msg: &str,
    budget_remaining: Option<Duration>,
) -> RetryWait {
    let decision =
        retry_wait_decision_with(attempt, error_msg, budget_remaining, is_wait_for_reset());
    if let RetryWait::Wait(delay) = decision {
        if !crate::format::is_quiet() {
            if let Some(notice) = rate_limit_wait_notice(delay, crate::format::is_plain_output()) {
                eprintln!("{notice}");
            }
        }
    }
    decision
}

/// Whether the opt-in long-wait behaviour is on. Split out so the one global
/// read has a name and the decision half stays pure.
fn is_wait_for_reset() -> bool {
    crate::cli::is_wait_for_reset()
}

/// Decision half of [`retry_wait_decision`], with the opt-in injected.
///
/// `wait_for_reset == false` is **byte-identical to the pre-flag behaviour on
/// every input** — that is every existing user, and it is the regression risk,
/// so it is pinned by an explicit equality test rather than by inspection.
///
/// `wait_for_reset == true` raises the ceiling from [`MAX_INLINE_RETRY_AFTER`]
/// to `min(MAX_RESET_WAIT, budget_remaining)`, keeping the existing rule that
/// the budget may only ever *shrink* a ceiling. Nothing else moves: the no-hint
/// path, the `MIN_RETRY_DELAY_MS` floor and the `GiveUp` message are unchanged.
///
/// **Deliberate divergence from a rival, not an oversight.** Claude Code
/// v2.1.234 ships this behaviour default-ON with an opt-out. A process that can
/// silently sleep for hours is not a product-safe default (#448), so yoyo ships
/// it default-OFF and opt-in.
///
/// # Why there is no terminal-error check here (Day 181 — traced, not assumed)
///
/// Claude Code v2.1.24x later shipped *"persistent retry mode now fails
/// immediately on organization spend-limit and out-of-credits errors instead of
/// waiting indefinitely for a reset"*. That is the expensive failure: an
/// out-of-credits error **never clears**, so sleeping [`MAX_RESET_WAIT`]
/// against it is a silent hang, and an opted-in user reads it as the flag
/// working. This function has no such check — and that is correct, because the
/// guard is **upstream and total**. Traced rather than reasoned about:
///
/// 1. `PromptResult::RetriableError` is constructed at exactly **one** non-test
///    site in `src/`: `PromptEventState::into_result` in `prompt.rs`. Every
///    other occurrence of the name is a `match` pattern or a test.
/// 2. That construction fires only when `PromptEventState::retriable_error` is
///    `Some`, and that field is assigned at exactly **one** non-test site,
///    inside an `else if is_retriable_error(err_msg)` arm.
/// 3. [`crate::prompt_retry::is_retriable_error`] checks its non-retriable list
///    — which includes `insufficient_quota`, `billing hard limit`,
///    `credit balance`, `out of credits`, `plan limit`, `spending limit`,
///    `budget exceeded`, `quota exceeded`, `payment required` and `402` —
///    **first**, and returns `false` on a match before the retriable list is
///    ever consulted. So a terminal message loses even when it also carries a
///    retriable keyword such as a `429`.
/// 4. Both `retry_wait_decision` call sites in `prompt.rs` sit inside
///    `PromptResult::RetriableError` arms.
///
/// A spend-limit message therefore cannot reach this function. **No second
/// guard was added**, deliberately: a duplicate check here would be a second
/// statement of one policy, and two copies that agree today are how they
/// disagree tomorrow. What the fix *is*: the discrimination is now asserted on
/// **both** sides (see `prompt_retry`'s `terminal_*` / `genuine_rate_limit_*`
/// tests) and step 1–2 of the trace — the single gated assignment site the
/// whole argument rests on — is pinned by
/// `the_only_retriable_error_assignment_site_is_gated_by_is_retriable_error`
/// below, so a future second assignment site fails a test instead of quietly
/// opening this path.
pub fn retry_wait_decision_with(
    attempt: u32,
    error_msg: &str,
    budget_remaining: Option<Duration>,
    wait_for_reset: bool,
) -> RetryWait {
    let Some(hint) = retry_after_hint(error_msg) else {
        return RetryWait::Wait(retry_delay(attempt));
    };

    let affordable = match (budget_remaining, wait_for_reset) {
        (Some(remaining), false) => inline_retry_ceiling(remaining),
        (Some(remaining), true) => reset_wait_ceiling(remaining),
        (None, false) => MAX_INLINE_RETRY_AFTER,
        (None, true) => MAX_RESET_WAIT,
    };

    if hint > affordable {
        return RetryWait::GiveUp { retry_after: hint };
    }

    RetryWait::Wait(hint.max(Duration::from_millis(MIN_RETRY_DELAY_MS)))
}

/// The line printed when yoyo is about to sleep longer than it ever would
/// without `--wait-for-reset`.
///
/// `None` at or below [`MAX_INLINE_RETRY_AFTER`] — short waits stay silent
/// exactly as before, so nothing new prints on the common path. Above it, the
/// wait is named in human units and declared *deliberate*, along with the flag
/// that turns it off: a user who sees a frozen prompt for two hours and was
/// never told is looking at a hang, not a feature.
pub fn rate_limit_wait_notice(delay: Duration, plain: bool) -> Option<String> {
    if delay <= MAX_INLINE_RETRY_AFTER {
        return None;
    }
    let marker = if plain { "" } else { "⏳ " };
    // Glyph-free under plain output means bullets AND em dashes (the
    // `git_redirection_refusal_message` convention).
    let dash = if plain { "-" } else { "—" };
    Some(format!(
        "{marker}waiting ~{} for the provider's rate limit to reset. \
         This wait is deliberate {dash} you asked for it with --wait-for-reset; \
         drop that flag to stop and report instead.",
        crate::format::format_duration(delay)
    ))
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

/// How many extra attempts a malformed tool-call turn is worth.
///
/// **This is a judgment threshold, not a measurement.** Nothing measured says 1
/// is right; it is the point where "one wasted turn" is cheaper than "a dead
/// run", and it is 1 rather than 3 precisely because #646's original fear is
/// real and unquantified: nobody has measured how often a resample reproduces
/// the same malformed turn. One attempt buys the common case (a one-off
/// sampling accident) without turning a deterministic failure into a burned
/// budget.
pub(crate) const MAX_MALFORMED_RETRIES: u32 = 1;

/// Should a fatal turn be resampled because its tool call arrived malformed?
///
/// The #646 class — the model emitted a tool call whose arguments never
/// assembled, so the tool never ran — is usually a sampling accident, and both
/// API-retry loops in `prompt.rs` rewind the message list to the pre-prompt
/// state before a retry (`agent.save_messages()` / `restore_messages`), so the
/// broken block is **not** carried into the resample. That rewind is what makes
/// one more attempt worth taking; without it a retry would reproduce the
/// failure deterministically, which is exactly what #646 refused.
///
/// **Narrowness is the whole safety property.** Auth and rate-limit shapes are
/// checked FIRST and always lose, even when the message also names a tool call:
/// those belong to `is_retriable_error` / `retry_wait_decision`, which own a
/// real reset-time policy, and a second policy here would fight it. The other
/// fatal shape — `pause_turn` with no `error_message` — is deliberately
/// excluded: its cause is not known to be a sampling accident, so a resample is
/// not known to help, and inventing a retry for it would be a confident guess
/// wearing a fix's clothes.
pub(crate) fn malformed_tool_call_retry(err_msg: &str, attempt: u32) -> bool {
    let lower = err_msg.to_lowercase();
    let has_code = |codes: &[&str]| {
        codes
            .iter()
            .any(|c| crate::prompt_retry::contains_status_code(&lower, c))
    };

    // Auth and rate limits are somebody else's policy, and they win outright.
    // A different sample against a broken credential or a closed door fails
    // identically, so retrying only spends the budget.
    if crate::tool_wrappers::AUTH_ERROR_SHAPES
        .iter()
        .any(|n| lower.contains(n))
        || has_code(crate::tool_wrappers::AUTH_STATUS_CODES)
        || crate::tool_wrappers::RATE_LIMIT_ERROR_SHAPES
            .iter()
            .any(|n| lower.contains(n))
        || has_code(crate::tool_wrappers::RATE_LIMIT_STATUS_CODES)
    {
        return false;
    }

    attempt < MAX_MALFORMED_RETRIES && crate::prompt::is_dropped_tool_args_error(err_msg)
}

/// The one line printed before a malformed-tool-call resample.
///
/// An invisible extra API turn is a bug even when it is the right turn (the
/// `⚡ auto-continuing` rule), so the retry announces itself. Pure, so the
/// wording has exactly one statement; the caller owns the `is_quiet()` gate.
/// Glyph-free under plain output means the marker **and** the em dash.
pub(crate) fn malformed_retry_notice(plain: bool) -> String {
    let marker = if plain { "" } else { "⚡ " };
    let dash = if plain { "-" } else { "—" };
    format!(
        "{marker}the model's tool call arrived malformed {dash} resampling once \
         (this is not a provider failure)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pin behind `retry_wait_decision_with`'s "no terminal check here"
    /// paragraph (Day 181).
    ///
    /// That argument rests on two structural facts in `prompt.rs`, and neither
    /// was asserted anywhere before this test: `PromptResult::RetriableError`
    /// is constructed at exactly one non-test site, and the field it reads is
    /// assigned at exactly one non-test site, gated on `is_retriable_error`.
    /// A second, ungated assignment site would open a path from a terminal
    /// billing error straight into a 6-hour sleep — silently, since nothing
    /// else in the tree would notice.
    ///
    /// **Deliberately weak, and its own doc says so:** this is a text scan, not
    /// a control-flow analysis. It proves the gate is *present and singular*,
    /// never that it fires — the same discipline the `format/mod.rs` wrapper
    /// guards state about themselves. Needles are assembled at runtime so this
    /// test cannot match itself.
    #[test]
    fn the_only_retriable_error_assignment_site_is_gated_by_is_retriable_error() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/prompt.rs"),
        )
        .expect("src/prompt.rs must be readable");

        // Truncate at the test module: `prompt.rs`'s own tests assign the field
        // directly, which is legitimate and must not count as a production site.
        let marker = format!("#[cfg{}]\nmod tests", "(test)");
        let prod = match src.find(&marker) {
            Some(i) => &src[..i],
            None => panic!("test-module marker not found — has prompt.rs been restructured?"),
        };

        let construction = format!("PromptResult::{} {{", "RetriableError");
        let assignment = format!("self.{} = Some(", "retriable_error");
        let gate = format!("is_{}(", "retriable_error");
        let field_read = format!("= self.{} {{", "retriable_error");

        // The construction needle deliberately is NOT counted: it also matches
        // the three `match` *patterns* that consume the variant, and a text
        // scan cannot tell a pattern from a construction. What is checkable —
        // and is the actual reachability chain — is that the variant is built
        // from exactly one field read, which is fed by exactly one gated write.
        let reads: Vec<_> = prod.match_indices(&field_read).collect();
        assert_eq!(
            reads.len(),
            1,
            "expected exactly one non-test read of the retriable_error field in prompt.rs; \
             found {} — each feeds a RetriableError and needs its own trace",
            reads.len()
        );
        let read_at = reads[0].0;
        let after_read = &prod[read_at..(read_at + 80).min(prod.len())];
        assert!(
            after_read.contains(&construction),
            "the retriable_error field read no longer feeds {construction:?} directly — \
             re-trace before trusting --wait-for-reset"
        );

        let sites: Vec<_> = prod.match_indices(&assignment).collect();
        assert_eq!(
            sites.len(),
            1,
            "expected exactly one non-test assignment to the retriable_error field; \
             found {} — each must be gated by {gate:?}",
            sites.len()
        );

        // The gate sits in the `else if` immediately above the assignment.
        let at = sites[0].0;
        let mut window_start = at.saturating_sub(400);
        while window_start > 0 && !prod.is_char_boundary(window_start) {
            window_start -= 1;
        }
        assert!(
            prod[window_start..at].contains(&gate),
            "the assignment to retriable_error is no longer gated by {gate:?} — \
             a terminal billing error can now reach retry_wait_decision's 6h sleep"
        );
    }

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

    /// The budget may only ever SHRINK the inline ceiling, never raise it.
    /// Both directions plus the exact boundary — a discriminator tested only on
    /// the side that fires is vacuous green.
    #[test]
    fn inline_retry_ceiling_lets_the_budget_shrink_but_never_raise() {
        let cap = MAX_INLINE_RETRY_AFTER;
        let table = [
            // (budget remaining, expected ceiling, why)
            (
                Duration::from_secs(5),
                Duration::from_secs(5),
                "budget below the cap wins",
            ),
            (
                cap,
                cap,
                "exactly at the cap: the boundary is the cap itself",
            ),
            (
                cap + Duration::from_secs(1),
                cap,
                "one second over: the cap wins",
            ),
            (
                Duration::from_secs(60 * 60 * 3),
                cap,
                "a three-hour budget cannot raise the cap",
            ),
            (
                Duration::ZERO,
                Duration::ZERO,
                "an exhausted budget shrinks it to nothing",
            ),
        ];
        for (budget, expected, why) in table {
            assert_eq!(
                inline_retry_ceiling(budget),
                expected,
                "inline_retry_ceiling({budget:?}) — {why}"
            );
        }
        // The property the table is sampling, stated directly: the result is never
        // above the cap and never above the budget.
        for secs in [0u64, 1, 59, 120, 121, 100_000] {
            let budget = Duration::from_secs(secs);
            let got = inline_retry_ceiling(budget);
            assert!(
                got <= cap,
                "ceiling {got:?} rose above the cap for {budget:?}"
            );
            assert!(
                got <= budget,
                "ceiling {got:?} rose above the budget {budget:?}"
            );
        }
    }

    // ---- opt-in long rate-limit wait (Day 178) -----------------------

    /// The pre-flag rule, transcribed verbatim from the body that shipped on
    /// Day 177. This exists so the byte-identity claim below is checked against
    /// a *second statement* of the old behaviour rather than against the new
    /// code agreeing with itself.
    fn legacy_retry_wait_decision(
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

    /// THE regression test. `wait_for_reset == false` is every existing user,
    /// so it must be byte-identical to the pre-flag rule on every input shape:
    /// the no-hint backoff path, the honoured short reset, the floor, the
    /// budget-shrunk ceiling, and the long-reset `GiveUp`.
    #[test]
    fn wait_for_reset_off_is_byte_identical_to_the_legacy_rule() {
        let messages = [
            // No parseable hint — the exponential-backoff path.
            "429 Too Many Requests",
            "the server is overloaded",
            "retry after 5 minutes", // marker present, unit refused
            // Hints, short and long.
            "rate limited, retry-after: 5",
            "retry after 30s",
            "retry after 1ms",
            "retry after 120s", // exactly at the ceiling
            "retry after 121s", // one second over
            "error: Rate limited, retry after Some(14454000)ms",
        ];
        let budgets = [
            None,
            Some(Duration::ZERO),
            Some(Duration::from_secs(10)),
            Some(Duration::from_secs(120)),
            Some(Duration::from_secs(45 * 60)),
            Some(Duration::from_secs(6 * 3600)),
        ];
        for msg in messages {
            for budget in budgets {
                for attempt in [1u32, 3, 20] {
                    let legacy = legacy_retry_wait_decision(attempt, msg, budget);
                    let now = retry_wait_decision_with(attempt, msg, budget, false);
                    if retry_after_hint(msg).is_none() {
                        // retry_delay is jittered, so the two cannot be compared
                        // by value; the claim is that both take the same branch
                        // and land in the same window.
                        match (legacy, now) {
                            (RetryWait::Wait(a), RetryWait::Wait(b)) => {
                                assert!(
                                    a <= Duration::from_secs(90) && b <= Duration::from_secs(90),
                                    "no-hint path left retry_delay's range: {a:?} / {b:?}"
                                );
                            }
                            other => panic!("no-hint path must Wait, got {other:?} for {msg:?}"),
                        }
                    } else {
                        assert_eq!(
                            legacy, now,
                            "off-mode diverged from the legacy rule for \
                             msg={msg:?} budget={budget:?} attempt={attempt}"
                        );
                    }
                }
            }
        }
    }

    /// Opting in raises the ceiling — and only the ceiling. The same 4h reset
    /// that gives up by default is waited out, while the budget still shrinks
    /// it and `MAX_RESET_WAIT` still bounds it.
    #[test]
    fn wait_for_reset_on_raises_the_ceiling_but_stays_bounded() {
        let four_hours = "error: Rate limited, retry after Some(14454000)ms";
        let four_hours_d = Duration::from_millis(14_454_000);

        // The headline: default gives up, opted-in waits.
        assert_eq!(
            retry_wait_decision_with(1, four_hours, None, false),
            RetryWait::GiveUp {
                retry_after: four_hours_d
            },
            "default must still stop at the inline ceiling"
        );
        assert_eq!(
            retry_wait_decision_with(1, four_hours, None, true),
            RetryWait::Wait(four_hours_d),
            "opted in, a 4h reset under MAX_RESET_WAIT must be honoured"
        );

        // The budget may only SHRINK the raised ceiling, never raise it.
        assert_eq!(
            retry_wait_decision_with(1, four_hours, Some(Duration::from_secs(45 * 60)), true),
            RetryWait::GiveUp {
                retry_after: four_hours_d
            },
            "a 45m budget cannot outlast a 4h reset even when opted in"
        );

        // MAX_RESET_WAIT still bounds it: 24h clamps to MAX_RETRY_AFTER, which
        // is above the 6h ceiling, so opting in does NOT mean an unbounded sleep.
        assert_eq!(
            retry_wait_decision_with(1, "retry after 86400", None, true),
            RetryWait::GiveUp {
                retry_after: MAX_RETRY_AFTER
            },
            "opting in must not license a 24h sleep"
        );

        // The no-hint path is untouched by the flag.
        match retry_wait_decision_with(1, "server error", None, true) {
            RetryWait::Wait(d) => assert!(d <= Duration::from_secs(90), "not capped: {d:?}"),
            other => panic!("no hint must never give up, got {other:?}"),
        }
    }

    /// The raised ceiling's own clamp, both directions plus the boundary.
    #[test]
    fn reset_wait_ceiling_lets_the_budget_shrink_but_never_raise() {
        let cap = MAX_RESET_WAIT;
        for (budget, expected, why) in [
            (
                Duration::from_secs(60),
                Duration::from_secs(60),
                "budget below the cap wins",
            ),
            (
                cap,
                cap,
                "exactly at the cap: the boundary is the cap itself",
            ),
            (
                cap + Duration::from_secs(1),
                cap,
                "one second over: the cap wins",
            ),
            (
                Duration::from_secs(24 * 3600),
                cap,
                "a day-long budget cannot raise the cap",
            ),
        ] {
            assert_eq!(reset_wait_ceiling(budget), expected, "{why}");
        }
        // And it really is higher than the default ceiling — otherwise the
        // whole flag is a no-op wearing a name.
        assert!(
            MAX_RESET_WAIT > MAX_INLINE_RETRY_AFTER,
            "the opt-in ceiling must exceed the default one"
        );
    }

    /// Emission point: the literal string a caller receives, in BOTH
    /// directions. A long sleep nobody announced is a hang, and a notice that
    /// fires on ordinary short retries is noise on the common path.
    #[test]
    fn rate_limit_wait_notice_speaks_only_above_the_inline_ceiling() {
        // Silent at and below the inline ceiling — the untouched common path.
        assert_eq!(rate_limit_wait_notice(Duration::from_secs(5), false), None);
        assert_eq!(
            rate_limit_wait_notice(MAX_INLINE_RETRY_AFTER, false),
            None,
            "exactly at the ceiling must stay silent (inclusive boundary)"
        );

        // One second over: it speaks.
        let notice = rate_limit_wait_notice(MAX_INLINE_RETRY_AFTER + Duration::from_secs(1), false)
            .expect("a wait past the inline ceiling must be announced");
        assert!(notice.contains("2m 1s"), "must name the wait: {notice}");
        assert!(
            notice.contains("deliberate"),
            "must say the wait was chosen, not a hang: {notice}"
        );
        assert!(
            notice.contains("--wait-for-reset"),
            "must name the flag that turns it off: {notice}"
        );

        // A four-hour wait renders in hours.
        let long = rate_limit_wait_notice(Duration::from_millis(14_454_000), false)
            .expect("a 4h wait must be announced");
        assert!(long.contains("4h 0m"), "misrendered long wait: {long}");

        // Glyph-free under plain output, and otherwise the same sentence.
        let plain = rate_limit_wait_notice(Duration::from_secs(3600), true)
            .expect("plain mode still announces");
        assert!(
            plain.is_ascii(),
            "plain output must carry no glyphs: {plain}"
        );
        assert!(plain.starts_with("waiting"), "plain output shape: {plain}");
    }

    /// The shapes that MUST fire: a malformed tool call on the first attempt.
    ///
    /// Every needle here is one `is_dropped_tool_args_error` really matches, so
    /// this pins the seam between the two functions rather than restating one.
    #[test]
    fn malformed_retry_fires_on_the_dropped_args_shape_once() {
        let shapes = [
            "tool call arguments were never assembled",
            "tool_call never completed",
            "assistant emitted a tool call with incomplete arguments",
            "tool call produced no arguments",
        ];
        for s in shapes {
            assert!(
                malformed_tool_call_retry(s, 0),
                "first attempt must resample: {s}"
            );
            // The bound is the whole safety story: one extra turn, never a chain.
            assert!(
                !malformed_tool_call_retry(s, MAX_MALFORMED_RETRIES),
                "must stop at MAX_MALFORMED_RETRIES: {s}"
            );
            assert!(
                !malformed_tool_call_retry(s, 7),
                "must never resample deep into a run: {s}"
            );
        }
    }

    /// The near-miss guards, and they are the half that matters — a
    /// discriminator tested only on the side that fires is vacuous green.
    #[test]
    fn malformed_retry_refuses_every_other_shape() {
        // The OTHER fatal shape. Its cause is not known to be a sampling
        // accident, so a resample is not known to help. Verbatim from
        // `prompt.rs`'s empty-message branch.
        assert!(
            !malformed_tool_call_retry(
                "turn ended with an error but no message — treating the response \
                 as incomplete (no resume path)",
                0
            ),
            "the pause_turn shape must stay surface-and-stop"
        );

        // Auth and rate limits belong to a policy that already exists.
        for s in [
            "401 Unauthorized",
            "403 Forbidden",
            "invalid api key",
            "authentication failed",
            "permission denied",
            "429 Too Many Requests",
            "rate limit exceeded",
            "the model is overloaded",
            "quota exhausted",
            "at capacity",
        ] {
            assert!(
                !malformed_tool_call_retry(s, 0),
                "auth/rate-limit shape must never resample here: {s}"
            );
        }

        // The sharp one: a message carrying BOTH. Auth is checked first and
        // always wins, so a broken credential cannot buy a wasted turn by
        // also mentioning a tool call.
        assert!(
            !malformed_tool_call_retry(
                "401 Unauthorized: tool call arguments were never assembled",
                0
            ),
            "auth must win over the malformed shape when a message carries both"
        );

        // Ordinary failures, transient shapes, and prose that merely says
        // "tool call" are all untouched.
        for s in [
            "the file was not found",
            "connection reset by peer",
            "500 Internal Server Error",
            "tool call failed: exit status 1",
            "",
        ] {
            assert!(
                !malformed_tool_call_retry(s, 0),
                "unrelated shape must not resample: {s}"
            );
        }
    }

    /// An invisible extra API turn is a bug even when it is the right turn.
    #[test]
    fn malformed_retry_notice_says_what_happened_and_stays_plain_safe() {
        let notice = malformed_retry_notice(false);
        assert!(notice.contains("malformed"), "names the cause: {notice}");
        assert!(notice.contains("once"), "names the bound: {notice}");
        assert!(
            notice.contains("not a provider failure"),
            "a deliberate resample must not read as a crash: {notice}"
        );

        // Glyph-free under plain output means the marker AND the em dash —
        // an assertion has caught the em-dash half before.
        let plain = malformed_retry_notice(true);
        assert!(
            plain.is_ascii(),
            "plain output must carry no glyphs: {plain}"
        );
        assert!(!plain.contains('—'), "plain output must carry no em dash");
        assert!(!plain.contains('⚡'), "plain output must carry no marker");
        assert!(
            plain.contains("malformed") && plain.contains("once"),
            "plain output must say the same thing: {plain}"
        );
    }
}
