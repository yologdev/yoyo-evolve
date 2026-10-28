//! Tests for `promo.rs`: Google's introductory rates, billed by date (#1012).

use super::super::model_pricing_with;
use super::*;

/// What `/cost` resolves for `id` with the pricing clock at `now`, no overrides.
fn billed_at(id: &str, now: u64) -> Option<Rates> {
    let none = crate::config::ModelPricingOverrides::default();
    with_now(now, || model_pricing_with(&none, id))
}

/// Google's regular Standard-tier rates, from the same page (2026-10-28).
const REGULAR: [(&str, u64, Rates); 4] = [
    (
        "gemini-4-argon",
        ARGON_PROMO_ENDS_UNIX,
        (4.0, 0.0, 0.2, 20.0),
    ),
    (
        "gemini-3.8-flash",
        FLASH_PROMO_ENDS_UNIX,
        (1.5, 0.0, 0.15, 7.5),
    ),
    (
        "gemini-3.7-flash",
        FLASH_PROMO_ENDS_UNIX,
        (1.5, 0.0, 0.15, 7.5),
    ),
    (
        "gemini-3.6-flash",
        FLASH_PROMO_ENDS_UNIX,
        (1.5, 0.0, 0.15, 7.5),
    ),
];

#[test]
fn end_dates_are_the_first_regular_day_at_midnight_utc() {
    // Anti-transcription: the consts must be the civil dates their docs name.
    // 2026-12-01 is day 20788 since the epoch, 2027-01-01 is day 20819.
    assert_eq!(ARGON_PROMO_ENDS_UNIX, 20_788 * 86_400);
    assert_eq!(FLASH_PROMO_ENDS_UNIX, 20_819 * 86_400);
    assert_eq!(AFTER_ALL_PROMOS_UNIX, FLASH_PROMO_ENDS_UNIX);
}

#[test]
fn each_promo_row_bills_the_introductory_rate_until_its_end_then_the_regular_rate() {
    for (id, ends, regular) in REGULAR {
        let promo = (regular.0 / 2.0, 0.0, regular.2 / 2.0, regular.3 / 2.0);
        // The pure function, one second either side of the end.
        assert_eq!(promo_pricing_at(id, ends - 1), Some(promo), "{id}");
        assert_eq!(promo_pricing_at(id, ends), None, "{id} at its end");
        assert_eq!(promo_pricing_at(id, ends + 86_400), None, "{id} after");
        // The emission point: what /cost resolves through the public resolver.
        assert_eq!(billed_at(id, ends - 1), Some(promo), "{id} via /cost");
        assert_eq!(billed_at(id, ends), Some(regular), "{id} via /cost at end");
    }
}

#[test]
fn argon_promo_ends_a_month_before_the_flash_rows() {
    // Between the two ends: argon is regular, the flash rows are still promo.
    let between = ARGON_PROMO_ENDS_UNIX + 1;
    assert_eq!(promo_pricing_at("gemini-4-argon", between), None);
    assert_eq!(
        promo_pricing_at("gemini-3.8-flash", between),
        Some((0.75, 0.0, 0.075, 3.75))
    );
}

#[test]
fn non_promo_and_lookalike_ids_never_get_a_promo_rate() {
    let before = ARGON_PROMO_ENDS_UNIX - 1;
    for id in [
        // Near-miss google ids with no introductory rate in the table.
        "gemini-3.5-flash",
        "gemini-3.1-pro-preview",
        "gemini-2.5-flash",
        // Ids that merely contain a promo id: different models, own prices.
        "gemini-3.8-flash-tts",
        "gemini-3.8-flash-lite",
        "gemini-4-argon-preview",
        "xgemini-3.6-flash",
    ] {
        assert_eq!(promo_pricing_at(id, before), None, "{id}");
        assert_eq!(
            billed_at(id, before),
            billed_at(id, AFTER_ALL_PROMOS_UNIX),
            "{id}: /cost must not depend on the date"
        );
    }
}

#[test]
fn a_router_prefixed_promo_id_bills_like_the_bare_id() {
    // The existing arms strip `google/` before every lookup, so the promo does too.
    let before = FLASH_PROMO_ENDS_UNIX - 1;
    assert_eq!(
        billed_at("google/gemini-3.8-flash", before),
        billed_at("gemini-3.8-flash", before)
    );
    assert_eq!(
        billed_at("google/gemini-3.8-flash", before),
        Some((0.75, 0.0, 0.075, 3.75))
    );
}

#[test]
fn a_user_override_still_wins_over_a_promo() {
    let overrides = crate::config::parse_model_pricing_from_config(
        "[model_pricing.\"gemini-3.8-flash\"]\ninput = 9.0\noutput = 9.0\n",
    );
    assert!(!overrides.is_empty(), "fixture must parse to one override");
    let got = with_now(FLASH_PROMO_ENDS_UNIX - 1, || {
        model_pricing_with(&overrides, "gemini-3.8-flash")
    });
    assert_eq!(got, Some((9.0, 0.0, 0.0, 9.0)));
}
