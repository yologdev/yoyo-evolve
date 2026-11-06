//! yoagent's price table as `/cost`'s first source (#1012 slice 1, Day 251).
//!
//! @yuanhao decided on 2026-11-03 that yoyo reuses yoagent's vendor-checked
//! prices instead of keeping its own. This module is the lookup; the ids it
//! answers for are deleted from yoyo's table (`no_yoyo_price_row_is_shadowed_by_yoagent`).

/// yoagent's price for `id`, from the process-wide resolved table (bundled
/// snapshot, plus any `YOAGENT_PRICES` override or fetched layer installed
/// before this call: read per lookup, nothing cached, so a live layer that
/// slice 2 installs later is seen).
///
/// Flat rates only: an entry's `context_tiers` are ignored here exactly as the
/// flat lookup always ignored them. The per-request tier path
/// (`estimate_request_cost_with`) is still yoyo's own (#1012 slice 3).
pub(super) fn yoagent_price_row(id: &str) -> Option<(f64, f64, f64, f64)> {
    crate::agent_builder::ensure_bundled_prices();
    unique_cost_for_id(&yoagent::provider::prices::global::resolved(), id).map(|c| cost_row(&c))
}

/// The one price `table` gives `id`. yoagent keys prices by (provider, id) and
/// yoyo by id alone, so an id listed under several providers is used only when
/// every listing has the same rates. When they differ (on yoagent 0.27.1,
/// `gpt-6-luna` under `openai` and `openai-decisions`) this returns `None` and
/// the lookup falls through to yoyo's own table rather than guess a provider.
pub(super) fn unique_cost_for_id(
    table: &yoagent::provider::PriceTable,
    id: &str,
) -> Option<yoagent::provider::CostConfig> {
    let mut found: Option<&yoagent::provider::CostConfig> = None;
    for (_, model, entry) in table.iter() {
        if model != id {
            continue;
        }
        match found {
            None => found = Some(&entry.cost),
            Some(prev) if *prev == entry.cost => {}
            Some(_) => return None,
        }
    }
    found.cloned()
}

/// A yoagent `CostConfig` as yoyo's `(input, cache_write, cache_read, output)`.
/// yoagent reads a cache rate of `0.0` as "bill at the input rate"
/// (`CostConfig::cost_usd`, and the comment at the top of its prices.json),
/// while yoyo's callers multiply cache tokens by the rate as given, so a `0.0`
/// is replaced by the input rate here to bill what yoagent bills.
pub(crate) fn cost_row(c: &yoagent::provider::CostConfig) -> (f64, f64, f64, f64) {
    let input = c.input_per_million;
    let or_input = |rate: f64| if rate == 0.0 { input } else { rate };
    (
        input,
        or_input(c.cache_write_per_million),
        or_input(c.cache_read_per_million),
        c.output_per_million,
    )
}

/// yoagent's bundled price for `id` as `/cost` bills it, for tests that pin a
/// yoagent-priced row: read from `PriceTable::builtin()`, never hand-typed, so
/// a yoagent upgrade that re-prices a row moves the expectation with it.
#[cfg(test)]
pub(crate) fn yoagent_builtin_row(id: &str) -> Option<(f64, f64, f64, f64)> {
    unique_cost_for_id(&yoagent::provider::PriceTable::builtin(), id).map(|c| cost_row(&c))
}

#[cfg(test)]
#[path = "yoagent_price_tests.rs"]
mod yoagent_price_tests;
