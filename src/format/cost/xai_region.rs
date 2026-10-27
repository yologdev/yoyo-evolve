//! xAI's US regional endpoint premium (#1012, Day 241).
//!
//! docs.x.ai/docs/key-information/regions (fetched 2026-10-27): "The US
//! endpoint currently serves two models, grok-4.7 and grok-4.6 ... Token usage
//! on the US endpoint costs 10% more than on the global endpoint. The premium
//! applies to input, output, and cached input tokens, including long-context
//! rates." The global endpoint (`https://api.x.ai/v1`, yoyo's xai default)
//! bills the base rate.
//!
//! yoyo chooses the endpoint itself, so it does not have to guess: the
//! decision is [`is_xai_us_endpoint`] over the resolved `base_url`, made once
//! at startup in `cli.rs` and stored by [`set_xai_us_region`]. The price
//! lookups in `cost.rs` then scale grok-4.6 / grok-4.7 rows (base and >200K
//! tier) by [`xai_region_multiplier`]. A user `[model_pricing]` override is
//! never scaled: it returns before any lookup here runs.
//!
//! The decision is keyed on the HOST, not the provider name: a request to
//! `us.api.x.ai` is billed by xAI's US rate whichever provider arm sent it.

use std::sync::OnceLock;

/// The two models xAI's US endpoint serves, and so the only ids the premium
/// can apply to. Exact ids: an alias or a router-prefixed id is not a request
/// to `us.api.x.ai`.
const US_ENDPOINT_MODELS: &[&str] = &["grok-4.6", "grok-4.7"];

/// xAI's US-endpoint premium: "costs 10% more than on the global endpoint".
const US_ENDPOINT_PREMIUM: f64 = 1.1;

/// The one host that carries the premium.
const US_ENDPOINT_HOST: &str = "us.api.x.ai";

/// Whether this process talks to xAI's US endpoint. Set once at startup;
/// first write wins, unset reads as `false` (the global endpoint, yoyo's
/// default).
static XAI_US_REGION: OnceLock<bool> = OnceLock::new();

/// Record whether the session's endpoint is xAI's US regional endpoint.
pub fn set_xai_us_region(us_region: bool) {
    let _ = XAI_US_REGION.set(us_region);
}

#[cfg(test)]
thread_local! {
    /// Test seam: a per-thread region, so a test can price a request "on the US
    /// endpoint" through the real lookup path without racing other tests on the
    /// process-wide `OnceLock`. Only the one-line read of the stored bool is
    /// replaced; the decision ([`is_xai_us_endpoint`]) and the scaling
    /// ([`xai_region_multiplier`]) run unchanged in every test.
    static TEST_US_REGION: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Run `f` with this thread's xAI region forced to `us_region`.
#[cfg(test)]
pub(super) fn with_us_region<T>(us_region: bool, f: impl FnOnce() -> T) -> T {
    TEST_US_REGION.with(|c| c.set(Some(us_region)));
    let out = f();
    TEST_US_REGION.with(|c| c.set(None));
    out
}

/// The region the price lookups apply.
pub(super) fn xai_us_region() -> bool {
    #[cfg(test)]
    if let Some(forced) = TEST_US_REGION.with(|c| c.get()) {
        return forced;
    }
    XAI_US_REGION.get().copied().unwrap_or(false)
}

/// Is `base_url` xAI's US regional endpoint? `None` is yoyo's xai default,
/// the global `https://api.x.ai/v1`, so `false`. Compares the URL's HOST
/// exactly (case-insensitive), so a path or query that merely contains
/// `us.api.x.ai` does not count.
pub fn is_xai_us_endpoint(base_url: Option<&str>) -> bool {
    let Some(url) = base_url else {
        return false;
    };
    let url = url.trim();
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let host_port = authority.rsplit('@').next().unwrap_or_default();
    let host = host_port.split(':').next().unwrap_or_default();
    host.trim_end_matches('.')
        .eq_ignore_ascii_case(US_ENDPOINT_HOST)
}

/// Price multiplier for `model` on the given endpoint: 1.1 for grok-4.6 and
/// grok-4.7 on the US endpoint, otherwise 1.0.
pub(super) fn xai_region_multiplier(model: &str, us_region: bool) -> f64 {
    if us_region && US_ENDPOINT_MODELS.contains(&model) {
        US_ENDPOINT_PREMIUM
    } else {
        1.0
    }
}

/// Scale a `(input, cache_write, cache_read, output)` row for this process's
/// endpoint. Identity for every id but the two the premium covers.
pub(super) fn region_row(model: &str, row: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    let m = xai_region_multiplier(model, xai_us_region());
    if m == 1.0 {
        return row;
    }
    (row.0 * m, row.1 * m, row.2 * m, row.3 * m)
}

/// Scale a tiered `CostConfig` (base and every context tier) for this
/// process's endpoint. Identity for every id but the two the premium covers.
pub(super) fn region_cost(
    model: &str,
    mut cost: yoagent::provider::CostConfig,
) -> yoagent::provider::CostConfig {
    let m = xai_region_multiplier(model, xai_us_region());
    if m == 1.0 {
        return cost;
    }
    cost.input_per_million *= m;
    cost.output_per_million *= m;
    cost.cache_read_per_million *= m;
    cost.cache_write_per_million *= m;
    for tier in &mut cost.context_tiers {
        tier.input_per_million *= m;
        tier.output_per_million *= m;
        tier.cache_read_per_million *= m;
        tier.cache_write_per_million *= m;
    }
    cost
}

#[cfg(test)]
mod tests {
    use super::super::{
        estimate_cost_with, estimate_request_cost_with, model_pricing_with, ModelPricingOverrides,
    };
    use super::*;

    type Row = (f64, f64, f64, f64);

    fn close(a: Row, b: Row) -> bool {
        [(a.0, b.0), (a.1, b.1), (a.2, b.2), (a.3, b.3)]
            .iter()
            .all(|(x, y)| (x - y).abs() < 1e-9)
    }

    fn price(model: &str, us: bool) -> Row {
        let none = ModelPricingOverrides::default();
        with_us_region(us, || model_pricing_with(&none, model))
            .unwrap_or_else(|| panic!("{model} must be priced"))
    }

    #[test]
    fn us_endpoint_is_decided_by_the_url_host() {
        let cases: [(Option<&str>, bool); 11] = [
            (None, false), // yoyo's xai default: the global endpoint
            (Some("https://api.x.ai/v1"), false),
            (Some("https://us.api.x.ai/v1"), true),
            (Some("https://us.api.x.ai/v1/"), true),
            (Some("https://us.api.x.ai"), true),
            (Some("https://US.API.X.AI/v1"), true),
            (Some("https://us.api.x.ai:443/v1"), true),
            (Some("https://proxy.example.com/us.api.x.ai/v1"), false),
            (Some("https://example.com/v1?u=https://us.api.x.ai"), false),
            (Some("https://notus.api.x.ai/v1"), false),
            (Some("https://us.api.x.ai.evil.example/v1"), false),
        ];
        for (url, want) in cases {
            assert_eq!(is_xai_us_endpoint(url), want, "{url:?}");
        }
    }

    #[test]
    fn multiplier_covers_exactly_the_two_us_endpoint_models() {
        assert_eq!(xai_region_multiplier("grok-4.6", true), 1.1);
        assert_eq!(xai_region_multiplier("grok-4.7", true), 1.1);
        assert_eq!(xai_region_multiplier("grok-4.6", false), 1.0);
        assert_eq!(xai_region_multiplier("grok-4.7", false), 1.0);
        // Near-misses: models the US endpoint does not serve.
        for id in ["grok-4.5", "grok-4.3", "grok-4", "grok-4.6-latest"] {
            assert_eq!(xai_region_multiplier(id, true), 1.0, "{id}");
        }
    }

    /// Typed from docs.x.ai (2026-10-27): grok-4.6 and grok-4.7 bill
    /// 2.00 / 0.50 cached / 6.00 on the global endpoint, 10% more on the US one.
    #[test]
    fn grok_46_and_47_bill_the_endpoint_yoyo_calls() {
        let global: Row = (2.00, 0.0, 0.50, 6.00);
        let us: Row = (2.20, 0.0, 0.55, 6.60);
        for id in ["grok-4.6", "grok-4.7"] {
            assert!(
                close(price(id, false), global),
                "{id} global: {:?}",
                price(id, false)
            );
            assert!(close(price(id, true), us), "{id} US: {:?}", price(id, true));
        }
        // Near-miss: grok-4.5 is not served by the US endpoint; unchanged.
        assert_eq!(price("grok-4.5", true), price("grok-4.5", false));
        assert!(close(price("grok-4.5", true), (2.00, 0.0, 0.30, 6.00)));
    }

    #[test]
    fn a_user_override_is_never_scaled_by_the_region() {
        let ov = crate::config::parse_model_pricing_from_config(
            "[model_pricing.\"grok-4.6\"]\ninput = 1.0\noutput = 3.0\n",
        );
        let got = with_us_region(true, || model_pricing_with(&ov, "grok-4.6"));
        assert_eq!(got, Some((1.0, 0.0, 0.0, 3.0)));
        let usage = yoagent::Usage {
            input: 300_000,
            output: 1_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let req = with_us_region(true, || estimate_request_cost_with(&ov, &usage, "grok-4.6"));
        let want = (300_000.0 * 1.0 + 1_000.0 * 3.0) / 1_000_000.0;
        assert_eq!(req, Some((want, false)));
    }

    /// The >200K tier: xAI doubles every rate, and the US premium applies on
    /// top ("including long-context rates"). Global 4.00 / 1.00 / 12.00,
    /// US 4.40 / 1.10 / 13.20.
    #[test]
    fn the_long_context_tier_carries_the_premium_too() {
        let none = ModelPricingOverrides::default();
        let over = yoagent::Usage {
            input: 200_000,
            output: 3_000,
            cache_read: 50_000,
            cache_write: 0,
            total_tokens: 0,
        };
        let tier = |ti: f64, tcr: f64, to: f64| {
            (200_000.0 * ti + 50_000.0 * tcr + 3_000.0 * to) / 1_000_000.0
        };
        for id in ["grok-4.6", "grok-4.7"] {
            for (us, want) in [(false, tier(4.0, 1.0, 12.0)), (true, tier(4.4, 1.1, 13.2))] {
                let (cost, tiered) =
                    with_us_region(us, || estimate_request_cost_with(&none, &over, id)).unwrap();
                assert!(tiered, "{id} us={us}: a 250K prompt must be tiered");
                assert!((cost - want).abs() < 1e-9, "{id} us={us}: {cost} != {want}");
            }
            // Below the threshold the per-request path equals the flat estimate,
            // which itself carries the premium.
            let at = yoagent::Usage {
                input: 150_000,
                ..over.clone()
            };
            for us in [false, true] {
                let flat = with_us_region(us, || estimate_cost_with(&none, &at, id));
                let req = with_us_region(us, || estimate_request_cost_with(&none, &at, id));
                assert_eq!(req, flat.map(|c| (c, false)), "{id} us={us}");
            }
        }
        // Near-miss: grok-4.5's tier is the same on either endpoint.
        let a = with_us_region(false, || {
            estimate_request_cost_with(&none, &over, "grok-4.5")
        });
        let b = with_us_region(true, || {
            estimate_request_cost_with(&none, &over, "grok-4.5")
        });
        assert_eq!(a, b);
    }
}
