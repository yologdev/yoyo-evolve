//! Pricing, cost display, token formatting, and context bar.

fn model_pricing(model: &str) -> Option<(f64, f64, f64, f64)> {
    // Returns (input_per_MTok, cache_write_per_MTok, cache_read_per_MTok, output_per_MTok)
    // For providers without caching, cache_write and cache_read are set to 0.0.

    // Strip common OpenRouter prefixes (e.g. "anthropic/claude-sonnet-4-20250514")
    let model = model
        .strip_prefix("anthropic/")
        .or_else(|| model.strip_prefix("openai/"))
        .or_else(|| model.strip_prefix("google/"))
        .or_else(|| model.strip_prefix("deepseek/"))
        .or_else(|| model.strip_prefix("mistralai/"))
        .or_else(|| model.strip_prefix("x-ai/"))
        .or_else(|| model.strip_prefix("meta-llama/"))
        .unwrap_or(model);

    // ── Anthropic ─────────────────────────────────────────────────────
    // https://docs.anthropic.com/en/about-claude/pricing
    if model.contains("opus") {
        if model.contains("4-6")
            || model.contains("4-5")
            || model.contains("4.6")
            || model.contains("4.5")
        {
            return Some((5.0, 6.25, 0.50, 25.0));
        } else {
            return Some((15.0, 18.75, 1.50, 75.0));
        }
    }
    if model.contains("sonnet") {
        return Some((3.0, 3.75, 0.30, 15.0));
    }
    if model.contains("haiku") {
        if model.contains("4-5") || model.contains("4.5") {
            return Some((1.0, 1.25, 0.10, 5.0));
        } else {
            return Some((0.80, 1.0, 0.08, 4.0));
        }
    }

    // ── OpenAI ────────────────────────────────────────────────────────
    // https://platform.openai.com/docs/pricing
    if model.starts_with("gpt-4.1") {
        if model.contains("mini") {
            return Some((0.40, 0.0, 0.0, 1.60)); // gpt-4.1-mini
        } else if model.contains("nano") {
            return Some((0.10, 0.0, 0.0, 0.40)); // gpt-4.1-nano
        } else {
            return Some((2.00, 0.0, 0.0, 8.00)); // gpt-4.1
        }
    }
    if model.starts_with("gpt-4o") {
        if model.contains("mini") {
            return Some((0.15, 0.0, 0.0, 0.60)); // gpt-4o-mini
        } else {
            return Some((2.50, 0.0, 0.0, 10.00)); // gpt-4o
        }
    }
    if model.starts_with("o4-mini") {
        return Some((1.10, 0.0, 0.0, 4.40));
    }
    if model.starts_with("o3-mini") {
        return Some((1.10, 0.0, 0.0, 4.40));
    }
    if model == "o3" {
        return Some((2.00, 0.0, 0.0, 8.00));
    }

    // ── Google Gemini ─────────────────────────────────────────────────
    // https://ai.google.dev/pricing
    if model.contains("gemini-2.5-pro") {
        return Some((1.25, 0.0, 0.0, 10.00));
    }
    if model.contains("gemini-2.5-flash") {
        return Some((0.15, 0.0, 0.0, 0.60));
    }
    if model.contains("gemini-2.0-flash") {
        return Some((0.10, 0.0, 0.0, 0.40));
    }

    // ── DeepSeek ──────────────────────────────────────────────────────
    // https://platform.deepseek.com/api-docs/pricing/
    if model.contains("deepseek-chat") || model.contains("deepseek-v3") {
        return Some((0.27, 0.0, 0.0, 1.10));
    }
    if model.contains("deepseek-reasoner") || model.contains("deepseek-r1") {
        return Some((0.55, 0.0, 0.0, 2.19));
    }

    // ── Mistral ───────────────────────────────────────────────────────
    // https://mistral.ai/products#pricing
    if model.contains("mistral-large") {
        return Some((2.00, 0.0, 0.0, 6.00));
    }
    if model.contains("mistral-small") || model.contains("mistral-latest") {
        return Some((0.10, 0.0, 0.0, 0.30));
    }
    if model.contains("codestral") {
        return Some((0.30, 0.0, 0.0, 0.90));
    }

    // ── xAI (Grok) ───────────────────────────────────────────────────
    // https://docs.x.ai/docs/models#models-and-pricing
    if model.contains("grok-3") {
        if model.contains("mini") {
            return Some((0.30, 0.0, 0.0, 0.50));
        } else {
            return Some((3.00, 0.0, 0.0, 15.00));
        }
    }
    if model.contains("grok-2") {
        return Some((2.00, 0.0, 0.0, 10.00));
    }

    // ── ZAI (Zhipu AI / z.ai) ────────────────────────────────────────
    // https://open.bigmodel.cn/pricing — prices converted from CNY to USD
    if model.contains("glm-4-plus") || model.contains("glm-4.7") {
        return Some((0.70, 0.0, 0.0, 0.70));
    }
    if model.contains("glm-4-air") || model.contains("glm-4.5-air") {
        return Some((0.07, 0.0, 0.0, 0.07));
    }
    if model.contains("glm-4-flash") || model.contains("glm-4.5-flash") {
        return Some((0.01, 0.0, 0.0, 0.01));
    }
    if model.contains("glm-4-long") {
        return Some((0.14, 0.0, 0.0, 0.14));
    }
    if model.contains("glm-5") {
        return Some((0.70, 0.0, 0.0, 0.70));
    }

    // ── Groq (hosted models) ─────────────────────────────────────────
    // https://groq.com/pricing/
    if model.contains("llama-3.3-70b") || model.contains("llama3-70b") {
        return Some((0.59, 0.0, 0.0, 0.79));
    }
    if model.contains("llama-3.1-8b") || model.contains("llama3-8b") {
        return Some((0.05, 0.0, 0.0, 0.08));
    }
    if model.contains("mixtral-8x7b") {
        return Some((0.24, 0.0, 0.0, 0.24));
    }
    if model.contains("gemma2-9b") {
        return Some((0.20, 0.0, 0.0, 0.20));
    }

    None
}

/// Estimate cost in USD for a given usage and model.
/// Returns None if the model pricing is unknown.
pub fn estimate_cost(usage: &yoagent::Usage, model: &str) -> Option<f64> {
    let (input_cost, cw_cost, cr_cost, output_cost) = cost_breakdown(usage, model)?;
    Some(input_cost + cw_cost + cr_cost + output_cost)
}

/// Get individual cost components for a usage and model.
/// Returns (input_cost, cache_write_cost, cache_read_cost, output_cost) or None if model unknown.
pub fn cost_breakdown(usage: &yoagent::Usage, model: &str) -> Option<(f64, f64, f64, f64)> {
    let (input_per_m, cache_write_per_m, cache_read_per_m, output_per_m) = model_pricing(model)?;

    let input_cost = usage.input as f64 * input_per_m / 1_000_000.0;
    let cache_write_cost = usage.cache_write as f64 * cache_write_per_m / 1_000_000.0;
    let cache_read_cost = usage.cache_read as f64 * cache_read_per_m / 1_000_000.0;
    let output_cost = usage.output as f64 * output_per_m / 1_000_000.0;

    Some((input_cost, cache_write_cost, cache_read_cost, output_cost))
}

/// Format a cost in USD for display (e.g., "$0.0042", "$1.23").
pub fn format_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("${:.4}", cost)
    } else if cost < 1.0 {
        format!("${:.3}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

/// Format a duration for display (e.g., "1.2s", "350ms", "2m 15s").
pub fn format_duration(d: std::time::Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1000;
        format!("{mins}m {secs}s")
    }
}

/// Format a token count for display (e.g., 1500 -> "1.5k", 1000000 -> "1.0M").
pub fn format_token_count(count: u64) -> String {
    if count < 1000 {
        format!("{count}")
    } else if count < 1_000_000 {
        format!("{:.1}k", count as f64 / 1000.0)
    } else {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    }
}

/// Build a context usage bar (e.g., "████████░░░░░░░░░░░░ 40%").
pub fn context_bar(used: u64, max: u64) -> String {
    let pct = if max == 0 {
        0.0
    } else {
        (used as f64 / max as f64).min(1.0)
    };
    let width = 20;
    let filled = (pct * width as f64).round() as usize;
    let empty = width - filled;
    let bar: String = "█".repeat(filled) + &"░".repeat(empty);
    format!("{bar} {:.0}%", pct * 100.0)
}

/// Truncate a string with an ellipsis if it exceeds `max` characters.
/// Return the correct singular or plural form of a word based on count.
///
/// `pluralize(1, "line", "lines")` → `"line"`
/// `pluralize(3, "line", "lines")` → `"lines"`
pub fn pluralize<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 {
        singular
    } else {
        plural
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_token_count() {
        assert_eq!(format_token_count(0), "0");
        assert_eq!(format_token_count(999), "999");
        assert_eq!(format_token_count(1000), "1.0k");
        assert_eq!(format_token_count(1500), "1.5k");
        assert_eq!(format_token_count(10000), "10.0k");
        assert_eq!(format_token_count(150000), "150.0k");
        assert_eq!(format_token_count(1000000), "1.0M");
        assert_eq!(format_token_count(2500000), "2.5M");
    }

    #[test]
    fn test_context_bar() {
        let bar = context_bar(50000, 200000);
        assert!(bar.contains('█'));
        assert!(bar.contains("25%"));

        let bar_empty = context_bar(0, 200000);
        assert!(bar_empty.contains("0%"));

        let bar_full = context_bar(200000, 200000);
        assert!(bar_full.contains("100%"));
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(format_cost(0.0001), "$0.0001");
        assert_eq!(format_cost(0.0042), "$0.0042");
        assert_eq!(format_cost(0.05), "$0.050");
        assert_eq!(format_cost(0.123), "$0.123");
        assert_eq!(format_cost(1.5), "$1.50");
        assert_eq!(format_cost(12.345), "$12.35");
    }

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(
            format_duration(std::time::Duration::from_millis(50)),
            "50ms"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(999)),
            "999ms"
        );
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(
            format_duration(std::time::Duration::from_millis(1000)),
            "1.0s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(1500)),
            "1.5s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(30000)),
            "30.0s"
        );
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(
            format_duration(std::time::Duration::from_millis(60000)),
            "1m 0s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(90000)),
            "1m 30s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(125000)),
            "2m 5s"
        );
    }

    #[test]
    fn test_estimate_cost_opus() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-opus-4-6").unwrap();
        assert!((cost - 7.5).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_sonnet() {
        let usage = yoagent::Usage {
            input: 500_000,
            output: 50_000,
            cache_read: 200_000,
            cache_write: 100_000,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-sonnet-4-6").unwrap();
        assert!((cost - 2.685).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_haiku() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 500_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-haiku-4-5").unwrap();
        assert!((cost - 3.5).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_unknown_model() {
        let usage = yoagent::Usage {
            input: 1000,
            output: 1000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // A truly unknown model should return None
        assert!(estimate_cost(&usage, "unknown-model-xyz").is_none());
    }

    #[test]
    fn test_cost_breakdown_opus() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 500_000,
            cache_write: 200_000,
            total_tokens: 0,
        };
        let (input, cw, cr, output) = cost_breakdown(&usage, "claude-opus-4-6").unwrap();
        // input: 1M * 5/M = 5.0
        assert!((input - 5.0).abs() < 0.001);
        // output: 100k * 25/M = 2.5
        assert!((output - 2.5).abs() < 0.001);
        // cache_read: 500k * 0.50/M = 0.25
        assert!((cr - 0.25).abs() < 0.001);
        // cache_write: 200k * 6.25/M = 1.25
        assert!((cw - 1.25).abs() < 0.001);
        // Total should match estimate_cost
        let total = input + cw + cr + output;
        let expected = estimate_cost(&usage, "claude-opus-4-6").unwrap();
        assert!((total - expected).abs() < 0.001);
    }

    #[test]
    fn test_cost_breakdown_unknown_model() {
        let usage = yoagent::Usage {
            input: 1000,
            output: 1000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        assert!(cost_breakdown(&usage, "unknown-model-xyz").is_none());
    }

    // ── OpenAI model pricing tests ───────────────────────────────────

    #[test]
    fn test_estimate_cost_gpt4o() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gpt-4o: $2.50/MTok input, $10.00/MTok output
        let cost = estimate_cost(&usage, "gpt-4o").unwrap();
        assert!((cost - 3.5).abs() < 0.001, "gpt-4o cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_gpt4o_mini() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gpt-4o-mini: $0.15/MTok input, $0.60/MTok output
        let cost = estimate_cost(&usage, "gpt-4o-mini").unwrap();
        assert!((cost - 0.75).abs() < 0.001, "gpt-4o-mini cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_gpt41() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gpt-4.1: $2.00/MTok input, $8.00/MTok output
        let cost = estimate_cost(&usage, "gpt-4.1").unwrap();
        assert!((cost - 2.8).abs() < 0.001, "gpt-4.1 cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_gpt41_mini() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gpt-4.1-mini: $0.40/MTok input, $1.60/MTok output
        let cost = estimate_cost(&usage, "gpt-4.1-mini").unwrap();
        assert!((cost - 2.0).abs() < 0.001, "gpt-4.1-mini cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_o3() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // o3: $2.00/MTok input, $8.00/MTok output
        let cost = estimate_cost(&usage, "o3").unwrap();
        assert!((cost - 2.8).abs() < 0.001, "o3 cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_o4_mini() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // o4-mini: $1.10/MTok input, $4.40/MTok output
        let cost = estimate_cost(&usage, "o4-mini").unwrap();
        assert!((cost - 1.54).abs() < 0.001, "o4-mini cost: {cost}");
    }

    // ── Google Gemini pricing tests ──────────────────────────────────

    #[test]
    fn test_estimate_cost_gemini_25_pro() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gemini-2.5-pro: $1.25/MTok input, $10.00/MTok output
        let cost = estimate_cost(&usage, "gemini-2.5-pro").unwrap();
        assert!((cost - 2.25).abs() < 0.001, "gemini-2.5-pro cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_gemini_25_flash() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gemini-2.5-flash: $0.15/MTok input, $0.60/MTok output
        let cost = estimate_cost(&usage, "gemini-2.5-flash").unwrap();
        assert!((cost - 0.75).abs() < 0.001, "gemini-2.5-flash cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_gemini_20_flash() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gemini-2.0-flash: $0.10/MTok input, $0.40/MTok output
        let cost = estimate_cost(&usage, "gemini-2.0-flash").unwrap();
        assert!((cost - 0.50).abs() < 0.001, "gemini-2.0-flash cost: {cost}");
    }

    // ── DeepSeek pricing tests ───────────────────────────────────────

    #[test]
    fn test_estimate_cost_deepseek_chat() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // deepseek-chat: $0.27/MTok input, $1.10/MTok output
        let cost = estimate_cost(&usage, "deepseek-chat").unwrap();
        assert!((cost - 1.37).abs() < 0.001, "deepseek-chat cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_deepseek_reasoner() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // deepseek-reasoner: $0.55/MTok input, $2.19/MTok output
        let cost = estimate_cost(&usage, "deepseek-reasoner").unwrap();
        assert!(
            (cost - 2.74).abs() < 0.001,
            "deepseek-reasoner cost: {cost}"
        );
    }

    // ── Mistral pricing tests ────────────────────────────────────────

    #[test]
    fn test_estimate_cost_mistral_large() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // mistral-large: $2.00/MTok input, $6.00/MTok output
        let cost = estimate_cost(&usage, "mistral-large-latest").unwrap();
        assert!((cost - 2.6).abs() < 0.001, "mistral-large cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_mistral_small() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // mistral-small: $0.10/MTok input, $0.30/MTok output
        let cost = estimate_cost(&usage, "mistral-small-latest").unwrap();
        assert!((cost - 0.40).abs() < 0.001, "mistral-small cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_codestral() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // codestral: $0.30/MTok input, $0.90/MTok output
        let cost = estimate_cost(&usage, "codestral-latest").unwrap();
        assert!((cost - 1.20).abs() < 0.001, "codestral cost: {cost}");
    }

    // ── xAI (Grok) pricing tests ─────────────────────────────────────

    #[test]
    fn test_estimate_cost_grok3() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // grok-3: $3.00/MTok input, $15.00/MTok output
        let cost = estimate_cost(&usage, "grok-3").unwrap();
        assert!((cost - 4.5).abs() < 0.001, "grok-3 cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_grok3_mini() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // grok-3-mini: $0.30/MTok input, $0.50/MTok output
        let cost = estimate_cost(&usage, "grok-3-mini").unwrap();
        assert!((cost - 0.80).abs() < 0.001, "grok-3-mini cost: {cost}");
    }

    // ── Groq pricing tests ───────────────────────────────────────────

    #[test]
    fn test_estimate_cost_groq_llama70b() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // llama-3.3-70b on Groq: $0.59/MTok input, $0.79/MTok output
        let cost = estimate_cost(&usage, "llama-3.3-70b-versatile").unwrap();
        assert!((cost - 1.38).abs() < 0.001, "llama-3.3-70b cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_groq_llama8b() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // llama-3.1-8b on Groq: $0.05/MTok input, $0.08/MTok output
        let cost = estimate_cost(&usage, "llama-3.1-8b-instant").unwrap();
        assert!((cost - 0.13).abs() < 0.001, "llama-3.1-8b cost: {cost}");
    }

    // ── ZAI (Zhipu AI) pricing tests ─────────────────────────────────

    #[test]
    fn test_estimate_cost_glm4_plus() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // glm-4-plus: $0.70/MTok input, $0.70/MTok output
        let cost = estimate_cost(&usage, "glm-4-plus").unwrap();
        assert!((cost - 1.40).abs() < 0.001, "glm-4-plus cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_glm4_air() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // glm-4-air: $0.07/MTok input, $0.07/MTok output
        let cost = estimate_cost(&usage, "glm-4-air").unwrap();
        assert!((cost - 0.14).abs() < 0.001, "glm-4-air cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_glm4_flash() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // glm-4-flash: $0.01/MTok input, $0.01/MTok output
        let cost = estimate_cost(&usage, "glm-4-flash").unwrap();
        assert!((cost - 0.02).abs() < 0.001, "glm-4-flash cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_glm5() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // glm-5: $0.70/MTok input, $0.70/MTok output
        let cost = estimate_cost(&usage, "glm-5").unwrap();
        assert!((cost - 1.40).abs() < 0.001, "glm-5 cost: {cost}");
    }

    // ── OpenRouter prefix stripping tests ────────────────────────────

    #[test]
    fn test_estimate_cost_openrouter_anthropic_prefix() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // OpenRouter uses "anthropic/claude-sonnet-4-20250514" format
        let cost = estimate_cost(&usage, "anthropic/claude-sonnet-4-20250514").unwrap();
        let direct_cost = estimate_cost(&usage, "claude-sonnet-4-20250514").unwrap();
        assert!(
            (cost - direct_cost).abs() < 0.001,
            "OpenRouter prefix should resolve to same pricing"
        );
    }

    #[test]
    fn test_estimate_cost_openrouter_openai_prefix() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "openai/gpt-4o").unwrap();
        let direct_cost = estimate_cost(&usage, "gpt-4o").unwrap();
        assert!(
            (cost - direct_cost).abs() < 0.001,
            "OpenRouter openai/ prefix should resolve to same pricing"
        );
    }

    #[test]
    fn test_estimate_cost_openrouter_google_prefix() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "google/gemini-2.0-flash").unwrap();
        let direct_cost = estimate_cost(&usage, "gemini-2.0-flash").unwrap();
        assert!(
            (cost - direct_cost).abs() < 0.001,
            "OpenRouter google/ prefix should resolve to same pricing"
        );
    }

    // ── Non-caching provider zero cache costs ────────────────────────

    #[test]
    fn test_non_anthropic_providers_zero_cache_costs() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 500_000,
            cache_write: 200_000,
            total_tokens: 0,
        };
        // For non-Anthropic models, cache_write and cache_read rates are 0
        // so even with cache_read/cache_write tokens, those don't add to cost
        let (_, cw, cr, _) = cost_breakdown(&usage, "gpt-4o").unwrap();
        assert!(
            cw.abs() < 0.001 && cr.abs() < 0.001,
            "Non-Anthropic models should have zero cache costs: cw={cw}, cr={cr}"
        );
    }

    #[test]
    fn test_pluralize_singular() {
        assert_eq!(pluralize(1, "line", "lines"), "line");
        assert_eq!(pluralize(1, "file", "files"), "file");
    }

    #[test]
    fn test_pluralize_plural() {
        assert_eq!(pluralize(0, "line", "lines"), "lines");
        assert_eq!(pluralize(2, "line", "lines"), "lines");
        assert_eq!(pluralize(100, "file", "files"), "files");
    }

    // --- truncate_tool_output tests ---
}
