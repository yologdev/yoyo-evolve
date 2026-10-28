//! Google's introductory rates, billed by date (#1012, Day 242).
//!
//! Google's pricing page (https://ai.google.dev/gemini-api/docs/pricing,
//! fetched 2026-10-28) bills four ids at half their regular Standard-tier rate
//! "through" a stated date, and the regular rate "starting" the day after.
//! The variation's selector is the calendar date, and yoyo has a clock, so the
//! table models it instead of billing the higher rate for two months
//! (cost-tracking.md's error-direction rule: the high fallback is only for a
//! variation whose selector never reaches yoyo).
//!
//! **Where the boundary sits, and which way it errs.** Google does not name a
//! time zone. Each promo here ends at 00:00 UTC on the first regular day, the
//! earliest reading of "through <date>"; if Google means a later zone, the
//! hours in between bill the regular rate, so the error reads high, never low.
//! A cost is priced when it is displayed, so a November session summed in
//! December reads at the regular rate: high again.
//!
//! Only the rows yoyo prices today are listed. Google's page carries the same
//! introductory window on other ids (TTS, robotics), which the table does not
//! price exactly, and on the Batch/Flex/Priority tiers, which yoyo never sends.

/// `(input, cache_write, cache_read, output)` in USD per million tokens, the
/// same order as `model_pricing_with`.
type Rates = (f64, f64, f64, f64);

/// 2026-12-01T00:00:00Z. Gemini 4 Argon is $2 / $10 / $0.10 cache read
/// "through November 30, 2026", then $4 / $20 / $0.20.
const ARGON_PROMO_ENDS_UNIX: u64 = 1_796_083_200;

/// 2027-01-01T00:00:00Z. Gemini 3.6 and 3.8 Flash are $0.75 / $3.75 / $0.075
/// "through December 31, 2026", then $1.50 / $7.50 / $0.15.
const FLASH_PROMO_ENDS_UNIX: u64 = 1_798_761_600;

/// `(exact model id, first second of the regular rate, introductory rates)`.
/// `gemini-3.7-flash` is here because Google's deprecations page routes it to
/// 3.8 Flash, and the table bills a retired id what its successor bills.
const PROMOS: &[(&str, u64, Rates)] = &[
    (
        "gemini-4-argon",
        ARGON_PROMO_ENDS_UNIX,
        (2.00, 0.0, 0.10, 10.00),
    ),
    (
        "gemini-3.8-flash",
        FLASH_PROMO_ENDS_UNIX,
        (0.75, 0.0, 0.075, 3.75),
    ),
    (
        "gemini-3.7-flash",
        FLASH_PROMO_ENDS_UNIX,
        (0.75, 0.0, 0.075, 3.75),
    ),
    (
        "gemini-3.6-flash",
        FLASH_PROMO_ENDS_UNIX,
        (0.75, 0.0, 0.075, 3.75),
    ),
];

/// The introductory rate for `model` at `now_unix_secs`, or `None` when the
/// id has no promo or its promo has ended (the regular arm then applies).
/// Exact ids only: `gemini-3.8-flash-tts` is a different model with its own
/// price. Pure: the caller reads the clock.
pub(super) fn promo_pricing_at(model: &str, now_unix_secs: u64) -> Option<Rates> {
    PROMOS
        .iter()
        .find(|(id, ends, _)| *id == model && now_unix_secs < *ends)
        .map(|(_, _, rates)| *rates)
}

#[cfg(test)]
thread_local! {
    /// Test seam: a per-thread clock, so a test can price on either side of a
    /// promo end without depending on the day it runs.
    static TEST_NOW: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// Run `f` with this thread's pricing clock forced to `now_unix_secs`.
#[cfg(test)]
pub(crate) fn with_now<T>(now_unix_secs: u64, f: impl FnOnce() -> T) -> T {
    TEST_NOW.with(|c| c.set(Some(now_unix_secs)));
    let out = f();
    TEST_NOW.with(|c| c.set(None));
    out
}

/// The clock the price lookup applies: the system time, read once per lookup.
/// A clock set before 1970 reads as "every promo over" (the regular rate),
/// so a broken clock errs high.
pub(super) fn now_unix_secs() -> u64 {
    #[cfg(test)]
    if let Some(forced) = TEST_NOW.with(|c| c.get()) {
        return forced;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(u64::MAX, |d| d.as_secs())
}

/// First second after every promo here has ended: a test pinning a regular
/// rate prices at this instant.
#[cfg(test)]
pub(crate) const AFTER_ALL_PROMOS_UNIX: u64 = FLASH_PROMO_ENDS_UNIX;

#[cfg(test)]
#[path = "promo_tests.rs"]
mod promo_tests;
