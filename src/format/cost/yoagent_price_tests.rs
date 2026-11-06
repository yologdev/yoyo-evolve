//! Tests for `yoagent_price.rs`: `/cost` prices through yoagent's table first,
//! and yoyo keeps no row yoagent shadows (#1012 slice 1, Day 251).

/// Every exact model id in yoyo's own price table (`builtin_model_pricing`'s
/// `==` and match-arm literals), read off the source
/// text with `//` comments dropped. `contains(..)` / `starts_with(..)` literals
/// are patterns, not ids, so they are not returned.
fn yoyo_exact_price_ids() -> Vec<String> {
    let cost_src = include_str!("../cost.rs");
    let start = cost_src
        .find("fn builtin_model_pricing(")
        .expect("builtin_model_pricing is in cost.rs");
    let body = &cost_src[start..];
    let body = &body[..body.find("\n}\n").expect("the fn ends")];
    let mut ids = Vec::new();
    for line in body.lines() {
        let code = line.split("//").next().unwrap_or("");
        let mut rest = code;
        while let Some(open) = rest.find('"') {
            let before = rest[..open].trim_end();
            let after_open = &rest[open + 1..];
            let Some(close) = after_open.find('"') else {
                break;
            };
            let lit = &after_open[..close];
            let pattern_or_call = ["contains(", "starts_with(", "yoagent_price_row("];
            if !pattern_or_call.iter().any(|p| before.ends_with(p)) {
                ids.push(lit.to_string());
            }
            rest = &after_open[close + 1..];
        }
    }
    ids
}

/// #1012 slice 1 (Day 251): yoagent's price wins for every id it prices, so a
/// yoyo row keyed on such an id exactly can never be reached and must be
/// deleted, not left to drift. Read from `PriceTable::builtin()`, so a yoagent
/// upgrade that starts pricing one of yoyo's ids reddens this test by name.
/// An id yoagent lists under several providers at different rates is not
/// shadowed (`unique_cost_for_id` falls through), and is skipped here too.
#[test]
fn no_yoyo_price_row_is_shadowed_by_yoagent() {
    let ids = yoyo_exact_price_ids();
    // Anti-vacuous: the scan must see the exact ids yoyo is known to keep.
    for kept in ["glm-4.7", "grok-4.6", "gemini-3.7-flash"] {
        assert!(ids.iter().any(|i| i == kept), "scan missed {kept}: {ids:?}");
    }
    assert!(
        !ids.iter().any(|i| i == "gpt-5.5"),
        "a starts_with literal is a pattern, not an id"
    );
    let shadowed: Vec<&String> = ids
        .iter()
        .filter(|id| super::yoagent_builtin_row(id).is_some())
        .collect();
    assert!(
        shadowed.is_empty(),
        "yoagent prices these ids, so yoyo's rows for them are unreachable; delete them: {shadowed:?}"
    );
}

/// #1012 slice 1 (Day 251): precedence is user override > yoagent's table >
/// yoyo's table. Expectations come from `PriceTable::builtin()`, not typed.
#[test]
fn cost_prices_through_yoagent_first_and_yoyo_only_for_the_rest() {
    let none = crate::config::ModelPricingOverrides::default();
    let table = yoagent::provider::PriceTable::builtin();
    // Every unambiguous id yoagent prices bills yoagent's row (bare and with a
    // router prefix yoyo strips). Count asserted, so this cannot pass on nothing.
    let mut checked = 0;
    for (_, id, _) in table.iter() {
        let Some(want) = super::yoagent_builtin_row(id) else {
            continue;
        };
        assert_eq!(
            crate::format::model_pricing_with(&none, id),
            Some(want),
            "{id}"
        );
        checked += 1;
    }
    assert!(checked >= 30, "only {checked} yoagent ids checked");
    assert_eq!(
        crate::format::model_pricing_with(&none, "deepseek/deepseek-v4-pro"),
        super::yoagent_builtin_row("deepseek-v4-pro")
    );
    // An id yoagent does not price still gets yoyo's own row.
    assert!(table.iter().all(|(_, id, _)| id != "glm-4.7"));
    assert_eq!(
        crate::format::model_pricing_with(&none, "glm-4.7"),
        Some((0.60, 0.0, 0.11, 2.20))
    );
    // A user override still beats yoagent.
    let overrides = crate::config::parse_model_pricing_from_config(
        "[model_pricing.\"deepseek-flash\"]\ninput = 9.0\noutput = 9.0\n",
    );
    assert!(!overrides.is_empty(), "fixture must parse to one override");
    assert_eq!(
        crate::format::model_pricing_with(&overrides, "deepseek-flash"),
        Some((9.0, 0.0, 0.0, 9.0))
    );
}

/// yoagent keys by (provider, id), yoyo by id: an id listed under two providers
/// at different rates is not guessed at. On yoagent 0.27.1 that is `gpt-6-luna`
/// (`openai` and `openai-decisions`); it falls through to yoyo's preset path.
#[test]
fn an_id_yoagent_prices_differently_per_provider_is_not_guessed() {
    use yoagent::provider::{CostConfig, PriceEntry, PriceTable};
    let mut t = PriceTable::new();
    t.insert("a", "m", PriceEntry::new(CostConfig::new(1.0, 2.0)))
        .expect("a valid fixture row");
    t.insert("b", "m", PriceEntry::new(CostConfig::new(1.0, 2.0)))
        .expect("a valid fixture row");
    assert_eq!(
        super::unique_cost_for_id(&t, "m"),
        Some(CostConfig::new(1.0, 2.0)),
        "equal rates under two providers are one price"
    );
    t.insert("c", "m", PriceEntry::new(CostConfig::new(3.0, 2.0)))
        .expect("a valid fixture row");
    assert_eq!(super::unique_cost_for_id(&t, "m"), None);
    assert_eq!(super::unique_cost_for_id(&t, "absent"), None);
    // The live specimen, so the rule is not only pinned on a fixture.
    let builtin = PriceTable::builtin();
    let luna = builtin
        .iter()
        .filter(|(_, id, _)| *id == "gpt-6-luna")
        .count();
    assert!(luna >= 2, "gpt-6-luna is no longer multi-provider: {luna}");
    assert_eq!(super::yoagent_builtin_row("gpt-6-luna"), None);
    let none = crate::config::ModelPricingOverrides::default();
    assert!(crate::format::model_pricing_with(&none, "gpt-6-luna").is_some());
}

/// yoagent reads a zero cache rate as "bill at input"; yoyo's callers multiply
/// the rate as given, so the conversion substitutes the input rate.
#[test]
fn a_zero_yoagent_cache_rate_bills_at_the_input_rate() {
    let c = yoagent::provider::CostConfig::new(1.5, 7.5).with_cache_read(0.15);
    assert_eq!(super::cost_row(&c), (1.5, 1.5, 0.15, 7.5));
    // The real row with a zero cache_write (Google's flash), against yoagent's own bill.
    let entry = yoagent::provider::PriceTable::builtin()
        .cost("google", "gemini-3.8-flash")
        .expect("yoagent prices gemini-3.8-flash");
    assert_eq!(entry.cache_write_per_million, 0.0);
    let usage = yoagent::Usage {
        input: 0,
        output: 0,
        cache_read: 0,
        cache_write: 1_000_000,
        total_tokens: 0,
    };
    let ours = crate::format::estimate_cost(&usage, "gemini-3.8-flash").unwrap();
    assert!((ours - entry.cost_usd(&usage)).abs() < 1e-9, "{ours}");
}
