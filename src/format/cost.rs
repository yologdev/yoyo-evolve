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
        if model.contains("4-5")
            || model.contains("4-6")
            || model.contains("4-7")
            || model.contains("4.5")
            || model.contains("4.6")
            || model.contains("4.7")
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
    // OpenAI Codex-mini (estimated, code-focused model similar to gpt-4.1-mini)
    if model.contains("codex-mini") {
        return Some((0.40, 0.0, 0.0, 1.60));
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

    // GPT-5 family (estimated, based on comparable model tiers)
    // Note: gpt-5.5 must be checked before gpt-5 since "gpt-5.5".starts_with("gpt-5") is true
    if model.starts_with("gpt-5.5") {
        if model.contains("mini") {
            return Some((0.40, 0.0, 0.0, 1.60)); // gpt-5.5-mini (estimated)
        } else {
            return Some((5.00, 0.0, 0.0, 20.00)); // gpt-5.5 (estimated)
        }
    }
    if model.starts_with("gpt-5") {
        if model.contains("mini") {
            return Some((0.40, 0.0, 0.0, 1.60)); // gpt-5-mini (estimated)
        } else {
            return Some((2.00, 0.0, 0.0, 8.00)); // gpt-5 (estimated)
        }
    }

    // ── Google Gemini ─────────────────────────────────────────────────
    // https://ai.google.dev/pricing
    // Gemini 3.x (estimated, based on 2.5 pricing tiers)
    if model.contains("gemini-3") {
        if model.contains("pro") {
            return Some((1.25, 0.0, 0.0, 10.00)); // gemini-3.0-pro (estimated)
        }
        // flash variants
        return Some((0.15, 0.0, 0.0, 0.60)); // gemini-3.0-flash (estimated)
    }
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
    if model.contains("deepseek-reasoner")
        || model.contains("deepseek-r1")
        || model.contains("deepseek-r2")
    {
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
    if model.contains("grok-4") {
        if model.contains("mini") {
            return Some((0.60, 0.0, 0.0, 3.00)); // grok-4-mini (estimated, between grok-3-mini and grok-4)
        }
        return Some((3.00, 0.0, 0.0, 15.00)); // grok-4 (estimated)
    }
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
    // Llama 4 on Groq (estimated, similar to llama-3.3-70b pricing tier)
    if model.contains("llama-4") {
        return Some((0.59, 0.0, 0.0, 0.79));
    }
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
/// Format cache statistics for display. Returns `None` if there's no caching activity.
/// Example output: `"Cache: 85% hit rate (150.2k read, 12.0k written)"`
pub fn format_cache_stats(usage: &yoagent::Usage) -> Option<String> {
    if usage.cache_read == 0 && usage.cache_write == 0 {
        return None;
    }
    let rate = usage.cache_hit_rate();
    let pct = (rate * 100.0) as u32;
    Some(format!(
        "Cache: {}% hit rate ({} read, {} written)",
        pct,
        format_token_count(usage.cache_read),
        format_token_count(usage.cache_write),
    ))
}

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
    let pct_int = (pct * 100.0) as u32;
    // Issue #263: integer truncation rendered tiny non-zero usage as "0%".
    // Show "<1%" so the user can tell tokens were actually consumed.
    let label = if used > 0 && pct_int == 0 {
        "<1%".to_string()
    } else {
        format!("{pct_int}%")
    };
    format!("{bar} {label}")
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

// ── Per-turn cost breakdown ─────────────────────────────────────────────

/// Per-turn cost information extracted from conversation messages.
pub struct TurnCost {
    pub turn_number: usize,
    pub usage: yoagent::Usage,
    pub cost_usd: Option<f64>,
}

/// Extract per-turn costs from a conversation message list.
/// Each Assistant message counts as one turn.
pub fn extract_turn_costs(messages: &[yoagent::AgentMessage], model: &str) -> Vec<TurnCost> {
    let mut turns = Vec::new();
    let mut turn_number = 0;
    for msg in messages {
        if let yoagent::AgentMessage::Llm(yoagent::Message::Assistant { usage, .. }) = msg {
            turn_number += 1;
            turns.push(TurnCost {
                turn_number,
                usage: usage.clone(),
                cost_usd: estimate_cost(usage, model),
            });
        }
    }
    turns
}

/// Format per-turn costs as a compact table for display.
pub fn format_turn_costs(costs: &[TurnCost]) -> String {
    if costs.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    lines.push("    Per-turn breakdown:".to_string());
    lines.push("      Turn   Input    Output   Cost".to_string());

    let mut total_input: u64 = 0;
    let mut total_output: u64 = 0;
    let mut total_cost: f64 = 0.0;
    let mut has_cost = false;

    for tc in costs {
        total_input = total_input.saturating_add(tc.usage.input);
        total_output = total_output.saturating_add(tc.usage.output);
        let cost_str = match tc.cost_usd {
            Some(c) => {
                has_cost = true;
                total_cost += c;
                format_cost(c)
            }
            None => "—".to_string(),
        };
        lines.push(format!(
            "      {:>4}   {:>7}  {:>7}  {}",
            tc.turn_number,
            format_token_count(tc.usage.input),
            format_token_count(tc.usage.output),
            cost_str,
        ));
    }

    lines.push("      ─────────────────────────────────".to_string());
    let total_cost_str = if has_cost {
        format_cost(total_cost)
    } else {
        "—".to_string()
    };
    lines.push(format!(
        "      Total  {:>7}  {:>7}  {}",
        format_token_count(total_input),
        format_token_count(total_output),
        total_cost_str,
    ));

    lines.join("\n")
}

/// Format a context breakdown table with colors and percentages.
///
/// Each category is shown with its token count and percentage of total.
/// The largest category is highlighted in bold. If tool results exceed 50%,
/// a suggestion to `/compact` is appended.
pub fn format_context_breakdown(breakdown: &crate::commands_info::ContextBreakdown) -> String {
    use super::{BOLD, DIM, RESET, YELLOW};

    let total = breakdown.total.max(1); // avoid division by zero
    let categories: &[(&str, usize)] = &[
        ("system prompt", breakdown.system_estimate),
        ("user messages", breakdown.user_messages),
        ("assistant", breakdown.assistant_messages),
        ("tool calls", breakdown.tool_calls),
        ("tool results", breakdown.tool_results),
        ("thinking", breakdown.thinking),
    ];

    // Find the largest non-zero category
    let max_val = categories.iter().map(|(_, v)| *v).max().unwrap_or(0);

    let mut lines = Vec::new();
    lines.push(format!("{DIM}  Context breakdown:"));

    for &(label, value) in categories {
        if value == 0 {
            continue;
        }
        let pct = (value as f64 / total as f64) * 100.0;
        let tok_str = format_token_count(value as u64);
        let is_max = value == max_val && value > 0;
        if is_max {
            lines.push(format!(
                "    {BOLD}{:<16} {:>7} tokens  ({:.0}%){RESET}{DIM}",
                label, tok_str, pct
            ));
        } else {
            lines.push(format!(
                "    {:<16} {:>7} tokens  ({:.0}%)",
                label, tok_str, pct
            ));
        }
    }

    lines.push(format!("    {}", "─".repeat(38)));
    lines.push(format!(
        "    {:<16} {:>7} tokens",
        "total",
        format_token_count(total as u64),
    ));

    // Advice if tool results dominate
    let tool_pct = (breakdown.tool_results as f64 / total as f64) * 100.0;
    if tool_pct > 50.0 {
        lines.push(format!(
            "    {YELLOW}💡 Tool results are {:.0}% of context — consider /compact.{RESET}{DIM}",
            tool_pct
        ));
    }

    lines.join("\n")
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

    // Issue #263: tiny non-zero usage was rendering as "0%" due to integer
    // truncation, making the bar look broken even when tokens had been spent.
    #[test]
    fn context_bar_shows_less_than_one_percent_for_tiny_usage() {
        let bar = context_bar(500, 200_000);
        assert!(!bar.contains(" 0%"), "expected non-0% label, got: {bar}");
        assert!(bar.contains("<1%"), "expected <1% label, got: {bar}");
    }

    #[test]
    fn context_bar_zero_usage_still_shows_zero() {
        let bar = context_bar(0, 200_000);
        assert!(
            bar.contains("0%"),
            "expected literal 0% for zero usage, got: {bar}"
        );
        assert!(!bar.contains("<1%"));
    }

    #[test]
    fn context_bar_normal_usage_unchanged() {
        let bar = context_bar(50_000, 200_000);
        assert!(bar.contains("25%"), "expected 25%, got: {bar}");
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

    #[test]
    fn test_estimate_cost_gpt5() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gpt-5: $2.00/MTok input, $8.00/MTok output
        let cost = estimate_cost(&usage, "gpt-5").unwrap();
        assert!((cost - 10.0).abs() < 0.01, "gpt-5 cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_gpt5_mini() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gpt-5-mini: $0.40/MTok input, $1.60/MTok output
        let cost = estimate_cost(&usage, "gpt-5-mini").unwrap();
        assert!((cost - 2.0).abs() < 0.01, "gpt-5-mini cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_gpt55() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gpt-5.5: $5.00/MTok input, $20.00/MTok output
        let cost = estimate_cost(&usage, "gpt-5.5").unwrap();
        assert!((cost - 25.0).abs() < 0.01, "gpt-5.5 cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_gpt55_mini() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // gpt-5.5-mini: $0.40/MTok input, $1.60/MTok output
        let cost = estimate_cost(&usage, "gpt-5.5-mini").unwrap();
        assert!((cost - 2.0).abs() < 0.01, "gpt-5.5-mini cost: {cost}");
    }

    #[test]
    fn test_estimate_cost_grok4() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // grok-4: $3.00/MTok input, $15.00/MTok output
        let cost = estimate_cost(&usage, "grok-4").unwrap();
        assert!((cost - 18.0).abs() < 0.01, "grok-4 cost: {cost}");
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

    // ── Per-turn cost tests ───────────────────────────────────────────

    #[test]
    fn test_extract_turn_costs_empty() {
        let messages: Vec<yoagent::AgentMessage> = vec![];
        let costs = extract_turn_costs(&messages, "claude-sonnet-4-20250514");
        assert!(costs.is_empty());
    }

    #[test]
    fn test_extract_turn_costs_skips_non_assistant() {
        use yoagent::{AgentMessage, Content, Message};

        let messages = vec![AgentMessage::Llm(Message::User {
            content: vec![Content::Text {
                text: "hello".into(),
            }],
            timestamp: 0,
        })];
        let costs = extract_turn_costs(&messages, "claude-sonnet-4-20250514");
        assert!(costs.is_empty());
    }

    #[test]
    fn test_extract_turn_costs_single_assistant() {
        use yoagent::{AgentMessage, Content, Message, StopReason, Usage};

        let messages = vec![AgentMessage::Llm(Message::Assistant {
            content: vec![Content::Text { text: "hi".into() }],
            stop_reason: StopReason::Stop,
            model: "claude-sonnet-4-20250514".into(),
            provider: "anthropic".into(),
            usage: Usage {
                input: 1000,
                output: 500,
                cache_read: 0,
                cache_write: 0,
                total_tokens: 1500,
            },
            timestamp: 0,
            error_message: None,
        })];
        let costs = extract_turn_costs(&messages, "claude-sonnet-4-20250514");
        assert_eq!(costs.len(), 1);
        assert_eq!(costs[0].turn_number, 1);
        assert_eq!(costs[0].usage.input, 1000);
        assert_eq!(costs[0].usage.output, 500);
        assert!(costs[0].cost_usd.is_some());
    }

    #[test]
    fn test_extract_turn_costs_multiple() {
        use yoagent::{AgentMessage, Content, Message, StopReason, Usage};

        let make_assistant = |input: u64, output: u64| {
            AgentMessage::Llm(Message::Assistant {
                content: vec![Content::Text { text: "hi".into() }],
                stop_reason: StopReason::Stop,
                model: "claude-sonnet-4-20250514".into(),
                provider: "anthropic".into(),
                usage: Usage {
                    input,
                    output,
                    cache_read: 0,
                    cache_write: 0,
                    total_tokens: input + output,
                },
                timestamp: 0,
                error_message: None,
            })
        };
        let user_msg = AgentMessage::Llm(Message::User {
            content: vec![Content::Text { text: "q".into() }],
            timestamp: 0,
        });

        let messages = vec![
            user_msg.clone(),
            make_assistant(1000, 500),
            user_msg.clone(),
            make_assistant(2000, 800),
            user_msg,
            make_assistant(3000, 1200),
        ];
        let costs = extract_turn_costs(&messages, "claude-sonnet-4-20250514");
        assert_eq!(costs.len(), 3);
        assert_eq!(costs[0].turn_number, 1);
        assert_eq!(costs[1].turn_number, 2);
        assert_eq!(costs[2].turn_number, 3);
        assert_eq!(costs[2].usage.input, 3000);
    }

    #[test]
    fn test_format_turn_costs_empty() {
        let result = format_turn_costs(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_turn_costs_single() {
        let costs = vec![TurnCost {
            turn_number: 1,
            usage: yoagent::Usage {
                input: 1200,
                output: 500,
                cache_read: 0,
                cache_write: 0,
                total_tokens: 1700,
            },
            cost_usd: Some(0.0111),
        }];
        let output = format_turn_costs(&costs);
        assert!(output.contains("Per-turn breakdown:"));
        assert!(output.contains("Turn"));
        assert!(output.contains("1.2k"));
        assert!(output.contains("500"));
        assert!(output.contains("Total"));
    }

    #[test]
    fn test_format_turn_costs_multiple() {
        let costs = vec![
            TurnCost {
                turn_number: 1,
                usage: yoagent::Usage {
                    input: 1200,
                    output: 500,
                    cache_read: 0,
                    cache_write: 0,
                    total_tokens: 1700,
                },
                cost_usd: Some(0.003),
            },
            TurnCost {
                turn_number: 2,
                usage: yoagent::Usage {
                    input: 1500,
                    output: 800,
                    cache_read: 0,
                    cache_write: 0,
                    total_tokens: 2300,
                },
                cost_usd: Some(0.005),
            },
        ];
        let output = format_turn_costs(&costs);
        assert!(output.contains("Per-turn breakdown:"));
        // Both turns should appear
        assert!(output.contains("1.2k"));
        assert!(output.contains("1.5k"));
        // Total line should appear
        assert!(output.contains("Total"));
    }

    #[test]
    fn test_format_turn_costs_unknown_model() {
        let costs = vec![TurnCost {
            turn_number: 1,
            usage: yoagent::Usage {
                input: 1000,
                output: 500,
                cache_read: 0,
                cache_write: 0,
                total_tokens: 1500,
            },
            cost_usd: None,
        }];
        let output = format_turn_costs(&costs);
        // Should show dash for unknown cost
        assert!(output.contains("—"));
    }

    #[test]
    fn test_format_cache_stats_no_activity() {
        let usage = yoagent::Usage {
            input: 10_000,
            output: 5_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 15_000,
        };
        assert!(format_cache_stats(&usage).is_none());
    }

    #[test]
    fn test_format_cache_stats_with_reads() {
        let usage = yoagent::Usage {
            input: 10_000,
            output: 5_000,
            cache_read: 150_000,
            cache_write: 0,
            total_tokens: 165_000,
        };
        let result = format_cache_stats(&usage).unwrap();
        // cache_hit_rate = 150k / (10k + 150k + 0k) = 150/160 = 93.75% → 93%
        assert!(result.contains("93%"), "got: {result}");
        assert!(result.contains("hit rate"));
        assert!(result.contains("150.0k read"));
    }

    #[test]
    fn test_format_cache_stats_with_writes_only() {
        let usage = yoagent::Usage {
            input: 50_000,
            output: 10_000,
            cache_read: 0,
            cache_write: 12_000,
            total_tokens: 72_000,
        };
        let result = format_cache_stats(&usage).unwrap();
        // cache_hit_rate = 0 / (50k + 0 + 12k) = 0%
        assert!(result.contains("0%"), "got: {result}");
        assert!(result.contains("12.0k written"));
    }

    #[test]
    fn test_format_cache_stats_mixed() {
        let usage = yoagent::Usage {
            input: 20_000,
            output: 5_000,
            cache_read: 80_000,
            cache_write: 10_000,
            total_tokens: 115_000,
        };
        let result = format_cache_stats(&usage).unwrap();
        // cache_hit_rate = 80k / (20k + 80k + 10k) = 80/110 ≈ 72.7% → 72%
        assert!(result.contains("72%"), "got: {result}");
        assert!(result.contains("80.0k read"));
        assert!(result.contains("10.0k written"));
    }

    // === Day 76: Tests for new model pricing entries ===

    #[test]
    fn test_pricing_gemini_30_pro() {
        assert!(model_pricing("gemini-3.0-pro").is_some());
        let (inp, _, _, out) = model_pricing("gemini-3.0-pro").unwrap();
        assert!((inp - 1.25).abs() < 0.001);
        assert!((out - 10.00).abs() < 0.001);
    }

    #[test]
    fn test_pricing_gemini_30_flash() {
        assert!(model_pricing("gemini-3.0-flash").is_some());
        let (inp, _, _, out) = model_pricing("gemini-3.0-flash").unwrap();
        assert!((inp - 0.15).abs() < 0.001);
        assert!((out - 0.60).abs() < 0.001);
    }

    #[test]
    fn test_pricing_codex_mini() {
        assert!(model_pricing("codex-mini").is_some());
        let (inp, _, _, out) = model_pricing("codex-mini").unwrap();
        assert!((inp - 0.40).abs() < 0.001);
        assert!((out - 1.60).abs() < 0.001);
    }

    #[test]
    fn test_pricing_grok4_mini() {
        assert!(model_pricing("grok-4-mini").is_some());
        let (inp, _, _, out) = model_pricing("grok-4-mini").unwrap();
        assert!((inp - 0.60).abs() < 0.001);
        assert!((out - 3.00).abs() < 0.001);
    }

    #[test]
    fn test_pricing_grok4_still_works() {
        // Ensure grok-4 (non-mini) still gets full pricing
        let (inp, _, _, out) = model_pricing("grok-4").unwrap();
        assert!((inp - 3.00).abs() < 0.001);
        assert!((out - 15.00).abs() < 0.001);
    }

    #[test]
    fn test_pricing_deepseek_r2() {
        assert!(model_pricing("deepseek-r2").is_some());
        let (inp, _, _, out) = model_pricing("deepseek-r2").unwrap();
        assert!((inp - 0.55).abs() < 0.001);
        assert!((out - 2.19).abs() < 0.001);
    }

    #[test]
    fn test_pricing_llama4_maverick() {
        assert!(model_pricing("llama-4-maverick-17b").is_some());
        let (inp, _, _, out) = model_pricing("llama-4-maverick-17b").unwrap();
        assert!((inp - 0.59).abs() < 0.001);
        assert!((out - 0.79).abs() < 0.001);
    }

    #[test]
    fn test_pricing_llama4_scout() {
        assert!(model_pricing("llama-4-scout-17b").is_some());
        let (inp, _, _, out) = model_pricing("llama-4-scout-17b").unwrap();
        assert!((inp - 0.59).abs() < 0.001);
        assert!((out - 0.79).abs() < 0.001);
    }

    #[test]
    fn test_pricing_claude_sonnet_47() {
        // claude-sonnet-4-7 should hit the existing sonnet branch
        assert!(model_pricing("claude-sonnet-4-7").is_some());
        let (inp, _, _, out) = model_pricing("claude-sonnet-4-7").unwrap();
        assert!((inp - 3.00).abs() < 0.001);
        assert!((out - 15.00).abs() < 0.001);
    }

    #[test]
    fn test_pricing_o4_mini_high() {
        // o4-mini-high should hit the existing o4-mini branch
        assert!(model_pricing("o4-mini-high").is_some());
        let (inp, _, _, out) = model_pricing("o4-mini-high").unwrap();
        assert!((inp - 1.10).abs() < 0.001);
        assert!((out - 4.40).abs() < 0.001);
    }
}
