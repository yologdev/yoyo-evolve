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

    // Fleet models (claude-fable-5, claude-opus-4-8, claude-sonnet-5,
    // claude-haiku-4-5): read pricing from yoagent 0.9's preset ModelConfig at
    // runtime — the preset is the source of truth, not this table. This must
    // run BEFORE the substring matching below, otherwise e.g. claude-opus-4-8
    // would fall into the legacy opus branch and show the wrong price.
    if let Some(preset) = crate::agent_builder::anthropic_preset(model) {
        return Some((
            preset.cost.input_per_million,
            preset.cost.cache_write_per_million,
            preset.cost.cache_read_per_million,
            preset.cost.output_per_million,
        ));
    }

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
    if model.contains("deepseek-v4-pro") || model.contains("deepseek-v3") {
        return Some((0.27, 0.0, 0.0, 1.10));
    }
    if model.contains("deepseek-v4-flash") || model.contains("deepseek-r1") {
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

/// Format a duration for display (e.g., "350ms", "1.2s", "2m 15s", "1h 5m", "2d 3h").
pub fn format_duration(d: std::time::Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        return format!("{ms}ms");
    }
    if ms < 59_950 {
        // Upper bound is 59_950, not 60_000: values in 59_950..60_000 round to
        // "60.0s" at one decimal place, so roll them over to the "m s" form.
        return format!("{:.1}s", ms as f64 / 1000.0);
    }
    // From here up we render whole seconds, so round to the nearest second
    // first — otherwise a value like 59_999ms truncates to "0m 59s" instead of
    // rolling over to "1m 0s" (the same rollover lie the seconds branch avoids).
    let total_secs = (ms + 500) / 1000;
    if total_secs < 3600 {
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{mins}m {secs}s")
    } else if total_secs < 86_400 {
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        format!("{hours}h {mins}m")
    } else {
        let days = total_secs / 86_400;
        let hours = (total_secs % 86_400) / 3600;
        format!("{days}d {hours}h")
    }
}

/// Format a token count for display (e.g., 1500 -> "1.5k", 1000000 -> "1.0M").
pub fn format_token_count(count: u64) -> String {
    if count < 1000 {
        format!("{count}")
    } else if count < 999_950 {
        // Upper bound is 999_950, not 1_000_000: values in 999_950..1_000_000
        // round to "1000.0k" at one decimal place, so roll them over to "M".
        format!("{:.1}k", count as f64 / 1000.0)
    } else if count < 999_950_000 {
        // Same rollover discipline: values in 999_950_000..1_000_000_000 round
        // to "1000.0M" at one decimal place, so roll them over to "B".
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else {
        format!("{:.1}B", count as f64 / 1_000_000_000.0)
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
    // Clamped fraction drives the bar fill (never over-draws past full).
    let pct = if max == 0 {
        0.0
    } else {
        (used as f64 / max as f64).min(1.0)
    };
    let width = 20;
    let filled = (pct * width as f64).round() as usize;
    let empty = width - filled;
    let bar: String = "█".repeat(filled) + &"░".repeat(empty);
    // The label uses the *true* (unclamped) percentage so an over-budget
    // context reads ">100%" instead of a misleading flat "100%".
    let true_pct = if max == 0 {
        0.0
    } else {
        used as f64 / max as f64
    };
    let pct_int = (true_pct * 100.0) as u32;
    // Issue #263: integer truncation rendered tiny non-zero usage as "0%".
    // Show "<1%" so the user can tell tokens were actually consumed.
    let label = if used > 0 && pct_int == 0 {
        "<1%".to_string()
    } else {
        format!("{pct_int}%")
    };
    format!("{bar} {label}")
}

/// Estimate how many more turns fit before hitting the context limit.
///
/// Returns `None` if fewer than 2 assistant turns have occurred (not enough
/// data for a meaningful average). Otherwise returns
/// `Some((remaining_turns, avg_tokens_per_turn))`.
pub fn estimate_remaining_turns(
    messages: &[yoagent::AgentMessage],
    max_context: u64,
) -> Option<(usize, f64)> {
    // Count assistant turns
    let turn_count = messages
        .iter()
        .filter(|m| {
            matches!(
                m,
                yoagent::AgentMessage::Llm(yoagent::Message::Assistant { .. })
            )
        })
        .count();

    if turn_count < 2 {
        return None;
    }

    let context_used = yoagent::context::total_tokens(messages) as u64;
    if context_used == 0 {
        return None;
    }

    let avg_per_turn = context_used as f64 / turn_count as f64;
    let remaining_capacity = max_context.saturating_sub(context_used);
    let remaining_turns = (remaining_capacity as f64 / avg_per_turn).floor() as usize;

    Some((remaining_turns, avg_per_turn))
}

/// Format estimated remaining turns for display.
///
/// Uses yellow for ≤3 remaining turns and red when context is nearly full.
pub fn format_remaining_turns(remaining: usize, avg_per_turn: f64) -> String {
    use super::{RED, RESET, YELLOW};

    let avg_str = format_token_count(avg_per_turn as u64);
    if remaining == 0 {
        format!("{RED}⚠ Context nearly full (~{avg_str}/turn avg){RESET}")
    } else if remaining <= 3 {
        format!(
            "{YELLOW}~{remaining} {} remaining (~{avg_str}/turn avg){RESET}",
            if remaining == 1 { "turn" } else { "turns" }
        )
    } else {
        format!("~{remaining} turns remaining (~{avg_str}/turn avg)")
    }
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

// ---------------------------------------------------------------------------
// Per-tool call summary
// ---------------------------------------------------------------------------

/// Summary of tool usage during a session.
pub struct ToolCallSummary {
    pub name: String,
    pub calls: usize,
    pub errors: usize,
}

/// Extract per-tool call counts from conversation messages.
///
/// Iterates over all `ToolResult` messages, counts calls per tool name,
/// and tracks error counts. Returns sorted by call count descending,
/// then alphabetically by name for ties.
pub fn extract_tool_call_summary(messages: &[yoagent::AgentMessage]) -> Vec<ToolCallSummary> {
    use std::collections::HashMap;

    let mut counts: HashMap<String, (usize, usize)> = HashMap::new();

    for msg in messages {
        if let yoagent::AgentMessage::Llm(yoagent::Message::ToolResult {
            tool_name,
            is_error,
            ..
        }) = msg
        {
            let entry = counts.entry(tool_name.clone()).or_insert((0, 0));
            entry.0 += 1;
            if *is_error {
                entry.1 += 1;
            }
        }
    }

    let mut result: Vec<ToolCallSummary> = counts
        .into_iter()
        .map(|(name, (calls, errors))| ToolCallSummary {
            name,
            calls,
            errors,
        })
        .collect();

    // Sort by call count descending, then alphabetically for ties
    result.sort_by(|a, b| b.calls.cmp(&a.calls).then_with(|| a.name.cmp(&b.name)));
    result
}

/// Format tool call summary as a compact table.
///
/// Output looks like:
/// ```text
///     Tool usage:
///       bash           12 calls
///       edit_file       8 calls (1 error)
///       read_file       5 calls
/// ```
pub fn format_tool_call_summary(summary: &[ToolCallSummary]) -> String {
    if summary.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    lines.push("    Tool usage:".to_string());

    // Find max tool name length for alignment
    let max_name_len = summary.iter().map(|s| s.name.len()).max().unwrap_or(0);

    let total_calls: usize = summary.iter().map(|s| s.calls).sum();
    let total_errors: usize = summary.iter().map(|s| s.errors).sum();

    for s in summary {
        let error_str = if s.errors > 0 {
            format!(" ({} {})", s.errors, pluralize(s.errors, "error", "errors"))
        } else {
            String::new()
        };
        lines.push(format!(
            "      {:<width$}  {:>3} {}{}",
            s.name,
            s.calls,
            pluralize(s.calls, "call", "calls"),
            error_str,
            width = max_name_len,
        ));
    }

    // Total line
    let total_error_str = if total_errors > 0 {
        format!(
            " ({} {})",
            total_errors,
            pluralize(total_errors, "error", "errors")
        )
    } else {
        String::new()
    };
    lines.push(format!(
        "      {:<width$}  {:>3} total{}",
        "—",
        total_calls,
        total_error_str,
        width = max_name_len,
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
        // Rounding boundary: values that round up to the next magnitude should
        // roll over (999_950 rounds to 1000.0k → must show "1.0M" instead).
        assert_eq!(format_token_count(999_950), "1.0M");
        assert_eq!(format_token_count(999_999), "1.0M");
        assert_eq!(format_token_count(999_949), "999.9k");
        // Billion tier: values >= 1e9 roll over to "B" instead of "1000.0M".
        assert_eq!(format_token_count(999_949_999), "999.9M");
        assert_eq!(format_token_count(999_950_000), "1.0B");
        assert_eq!(format_token_count(1_000_000_000), "1.0B");
        assert_eq!(format_token_count(2_500_000_000), "2.5B");
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

    // When usage exceeds max (over budget), the old code clamped the label to a
    // flat "100%", hiding the fact that the context is genuinely over the limit.
    // Show the true percentage (>100%) so the user can tell they've blown past it.
    #[test]
    fn context_bar_over_budget_shows_true_percentage() {
        let bar = context_bar(300_000, 200_000);
        assert!(
            bar.contains("150%"),
            "expected true over-budget percentage, got: {bar}"
        );
        // The bar itself stays fully filled (clamped), never over-drawn.
        assert!(bar.contains('█'), "expected filled bar, got: {bar}");
        assert!(
            !bar.contains('░'),
            "over-budget bar should be full, got: {bar}"
        );
    }

    // Exactly at the limit is still a clean "100%", not ">100%".
    #[test]
    fn context_bar_exactly_full_is_100() {
        let bar = context_bar(200_000, 200_000);
        assert!(bar.contains("100%"), "expected 100%, got: {bar}");
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
        // Rollover discipline: 59_950..60_000 ms rounds to "60.0s" at one
        // decimal place, which is a lie — it must roll over to "1m 0s".
        assert_eq!(
            format_duration(std::time::Duration::from_millis(59_949)),
            "59.9s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(59_950)),
            "1m 0s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(59_999)),
            "1m 0s"
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
    fn test_format_duration_hours() {
        // Exactly one hour.
        assert_eq!(
            format_duration(std::time::Duration::from_secs(3600)),
            "1h 0m"
        );
        // 1h 5m 30s -> seconds dropped once we're in the hours range.
        assert_eq!(
            format_duration(std::time::Duration::from_secs(3600 + 5 * 60 + 30)),
            "1h 5m"
        );
        // 2h 5m — the old code would have shown "125m 3s".
        assert_eq!(
            format_duration(std::time::Duration::from_secs(2 * 3600 + 5 * 60 + 3)),
            "2h 5m"
        );
        // Just under an hour still uses the minutes format.
        assert_eq!(
            format_duration(std::time::Duration::from_secs(59 * 60 + 59)),
            "59m 59s"
        );
    }

    #[test]
    fn test_format_duration_days() {
        // Exactly one day.
        assert_eq!(
            format_duration(std::time::Duration::from_secs(24 * 3600)),
            "1d 0h"
        );
        // 2d 3h — minutes dropped once we're in the days range.
        assert_eq!(
            format_duration(std::time::Duration::from_secs(
                2 * 24 * 3600 + 3 * 3600 + 42 * 60
            )),
            "2d 3h"
        );
        // Just under a day still uses the hours format.
        assert_eq!(
            format_duration(std::time::Duration::from_secs(23 * 3600 + 59 * 60)),
            "23h 59m"
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
    fn test_estimate_cost_fable_5() {
        // Pricing comes from yoagent 0.9's claude_fable_5() preset:
        // input $10/MTok, output $50/MTok, cache read $1/MTok, cache write $12.5/MTok
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-fable-5").unwrap();
        // 1M * 10/M + 100k * 50/M = 10.0 + 5.0 = 15.0
        assert!((cost - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_cost_breakdown_fable_5_cache() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 500_000,
            cache_write: 200_000,
            total_tokens: 0,
        };
        let (input, cw, cr, output) = cost_breakdown(&usage, "claude-fable-5").unwrap();
        // input: 1M * 10/M = 10.0
        assert!((input - 10.0).abs() < 0.001);
        // output: 100k * 50/M = 5.0
        assert!((output - 5.0).abs() < 0.001);
        // cache_read: 500k * 1.0/M = 0.5
        assert!((cr - 0.5).abs() < 0.001);
        // cache_write: 200k * 12.5/M = 2.5
        assert!((cw - 2.5).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_opus_4_8_uses_preset_not_legacy() {
        // claude-opus-4-8 must resolve to the yoagent 0.9 preset ($5/$25),
        // NOT the legacy opus branch ($15/$75). Regression guard for #568.
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-opus-4-8").unwrap();
        // 1M * 5/M + 100k * 25/M = 5.0 + 2.5 = 7.5
        assert!((cost - 7.5).abs() < 0.001);
        // Explicitly not the legacy price: 1M * 15/M + 100k * 75/M = 22.5
        assert!((cost - 22.5).abs() > 1.0);
    }

    #[test]
    fn test_estimate_cost_sonnet_5_preset() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        // Sonnet 5 preset: input $3/MTok, output $15/MTok
        let cost = estimate_cost(&usage, "claude-sonnet-5").unwrap();
        assert!((cost - 4.5).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_fleet_model_openrouter_prefix() {
        // Provider prefixes are stripped before the preset lookup
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let prefixed = estimate_cost(&usage, "anthropic/claude-fable-5").unwrap();
        let direct = estimate_cost(&usage, "claude-fable-5").unwrap();
        assert!((prefixed - direct).abs() < 0.001);
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
        // deepseek-v4-pro: $0.27/MTok input, $1.10/MTok output
        let cost = estimate_cost(&usage, "deepseek-v4-pro").unwrap();
        assert!((cost - 1.37).abs() < 0.001, "deepseek-v4-pro cost: {cost}");
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
        // deepseek-v4-flash: $0.55/MTok input, $2.19/MTok output
        let cost = estimate_cost(&usage, "deepseek-v4-flash").unwrap();
        assert!(
            (cost - 2.74).abs() < 0.001,
            "deepseek-v4-flash cost: {cost}"
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

        let messages = vec![AgentMessage::Llm(
            Message::assistant(
                vec![Content::Text { text: "hi".into() }],
                StopReason::Stop,
                "claude-sonnet-4-20250514",
                "anthropic",
                Usage {
                    input: 1000,
                    output: 500,
                    cache_read: 0,
                    cache_write: 0,
                    total_tokens: 1500,
                },
            )
            .with_timestamp(0),
        )];
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
            AgentMessage::Llm(
                Message::assistant(
                    vec![Content::Text { text: "hi".into() }],
                    StopReason::Stop,
                    "claude-sonnet-4-20250514",
                    "anthropic",
                    Usage {
                        input,
                        output,
                        cache_read: 0,
                        cache_write: 0,
                        total_tokens: input + output,
                    },
                )
                .with_timestamp(0),
            )
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
    fn test_pricing_deepseek_v4_flash() {
        assert!(model_pricing("deepseek-v4-flash").is_some());
        let (inp, _, _, out) = model_pricing("deepseek-v4-flash").unwrap();
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

    // -----------------------------------------------------------------------
    // Tool call summary tests
    // -----------------------------------------------------------------------

    fn make_tool_result(tool_name: &str, is_error: bool) -> yoagent::AgentMessage {
        use yoagent::{AgentMessage, Content, Message};
        AgentMessage::Llm(Message::ToolResult {
            tool_call_id: "test-id".into(),
            tool_name: tool_name.into(),
            content: vec![Content::Text {
                text: "result".into(),
            }],
            is_error,
            timestamp: 0,
        })
    }

    #[test]
    fn test_extract_tool_call_summary_empty() {
        let messages: Vec<yoagent::AgentMessage> = vec![];
        let summary = extract_tool_call_summary(&messages);
        assert!(summary.is_empty());
    }

    #[test]
    fn test_extract_tool_call_summary_counts() {
        let messages = vec![
            make_tool_result("bash", false),
            make_tool_result("bash", false),
            make_tool_result("bash", false),
            make_tool_result("edit_file", false),
            make_tool_result("edit_file", false),
            make_tool_result("read_file", false),
        ];
        let summary = extract_tool_call_summary(&messages);
        assert_eq!(summary.len(), 3);
        assert_eq!(summary[0].name, "bash");
        assert_eq!(summary[0].calls, 3);
        assert_eq!(summary[0].errors, 0);
        assert_eq!(summary[1].name, "edit_file");
        assert_eq!(summary[1].calls, 2);
        assert_eq!(summary[2].name, "read_file");
        assert_eq!(summary[2].calls, 1);
    }

    #[test]
    fn test_extract_tool_call_summary_errors() {
        let messages = vec![
            make_tool_result("bash", false),
            make_tool_result("bash", true),
            make_tool_result("edit_file", true),
            make_tool_result("edit_file", true),
        ];
        let summary = extract_tool_call_summary(&messages);
        assert_eq!(summary.len(), 2);
        // Both have 2 calls each, sorted alphabetically for ties
        assert_eq!(summary[0].name, "bash");
        assert_eq!(summary[0].calls, 2);
        assert_eq!(summary[0].errors, 1);
        assert_eq!(summary[1].name, "edit_file");
        assert_eq!(summary[1].calls, 2);
        assert_eq!(summary[1].errors, 2);
    }

    #[test]
    fn test_extract_tool_call_summary_sorted() {
        let messages = vec![
            make_tool_result("search", false),
            make_tool_result("bash", false),
            make_tool_result("bash", false),
            make_tool_result("bash", false),
            make_tool_result("edit_file", false),
            make_tool_result("edit_file", false),
        ];
        let summary = extract_tool_call_summary(&messages);
        assert_eq!(summary.len(), 3);
        // Sorted by call count descending
        assert_eq!(summary[0].name, "bash");
        assert_eq!(summary[0].calls, 3);
        assert_eq!(summary[1].name, "edit_file");
        assert_eq!(summary[1].calls, 2);
        assert_eq!(summary[2].name, "search");
        assert_eq!(summary[2].calls, 1);
    }

    #[test]
    fn test_extract_tool_call_summary_ignores_non_tool_messages() {
        use yoagent::{AgentMessage, Content, Message};
        let messages = vec![
            AgentMessage::Llm(Message::User {
                content: vec![Content::Text {
                    text: "hello".into(),
                }],
                timestamp: 0,
            }),
            make_tool_result("bash", false),
        ];
        let summary = extract_tool_call_summary(&messages);
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].name, "bash");
        assert_eq!(summary[0].calls, 1);
    }

    #[test]
    fn test_format_tool_call_summary_empty() {
        let output = format_tool_call_summary(&[]);
        assert!(output.is_empty());
    }

    #[test]
    fn test_format_tool_call_summary() {
        let summary = vec![
            ToolCallSummary {
                name: "bash".into(),
                calls: 12,
                errors: 0,
            },
            ToolCallSummary {
                name: "edit_file".into(),
                calls: 8,
                errors: 1,
            },
            ToolCallSummary {
                name: "search".into(),
                calls: 3,
                errors: 0,
            },
        ];
        let output = format_tool_call_summary(&summary);
        assert!(output.contains("Tool usage:"));
        assert!(output.contains("bash"));
        assert!(output.contains("12 calls"));
        assert!(output.contains("edit_file"));
        assert!(output.contains("8 calls (1 error)"));
        assert!(output.contains("search"));
        assert!(output.contains("3 calls"));
        assert!(output.contains("23 total"));
        // Total should show the 1 error
        assert!(output.contains("(1 error)"));
    }

    #[test]
    fn test_format_tool_call_summary_multiple_errors() {
        let summary = vec![ToolCallSummary {
            name: "bash".into(),
            calls: 5,
            errors: 3,
        }];
        let output = format_tool_call_summary(&summary);
        assert!(output.contains("(3 errors)"));
    }

    // ── estimate_remaining_turns / format_remaining_turns ──

    #[test]
    fn test_estimate_remaining_turns_empty() {
        let messages: Vec<yoagent::AgentMessage> = vec![];
        assert!(estimate_remaining_turns(&messages, 200_000).is_none());
    }

    #[test]
    fn test_estimate_remaining_turns_one_turn() {
        use yoagent::{AgentMessage, Content, Message, StopReason, Usage};

        let messages = vec![AgentMessage::Llm(
            Message::assistant(
                vec![Content::Text { text: "hi".into() }],
                StopReason::Stop,
                "test",
                "test",
                Usage {
                    input: 1000,
                    output: 500,
                    cache_read: 0,
                    cache_write: 0,
                    total_tokens: 1500,
                },
            )
            .with_timestamp(0),
        )];
        // Only 1 turn — not enough data
        assert!(estimate_remaining_turns(&messages, 200_000).is_none());
    }

    #[test]
    fn test_estimate_remaining_turns_basic() {
        use yoagent::{AgentMessage, Content, Message, StopReason, Usage};

        let make_assistant = |input: u64, output: u64| {
            AgentMessage::Llm(
                Message::assistant(
                    vec![Content::Text { text: "hi".into() }],
                    StopReason::Stop,
                    "test",
                    "test",
                    Usage {
                        input,
                        output,
                        cache_read: 0,
                        cache_write: 0,
                        total_tokens: input + output,
                    },
                )
                .with_timestamp(0),
            )
        };
        let user_msg = AgentMessage::Llm(Message::User {
            content: vec![Content::Text { text: "q".into() }],
            timestamp: 0,
        });

        // 3 assistant turns, each with ~1500 total tokens in usage
        // total_tokens() estimates based on content, not usage fields,
        // so the actual context size depends on message_tokens().
        let messages = vec![
            user_msg.clone(),
            make_assistant(1000, 500),
            user_msg.clone(),
            make_assistant(1000, 500),
            user_msg.clone(),
            make_assistant(1000, 500),
        ];

        let result = estimate_remaining_turns(&messages, 200_000);
        assert!(result.is_some());
        let (remaining, avg) = result.unwrap();
        // With 3 assistant turns and small messages, plenty of room left
        assert!(remaining > 0, "expected remaining > 0, got {remaining}");
        assert!(avg > 0.0, "expected avg > 0, got {avg}");
    }

    #[test]
    fn test_format_remaining_turns_normal() {
        let output = format_remaining_turns(12, 4200.0);
        assert!(
            output.contains("~12 turns remaining"),
            "expected turn count in output: {output}"
        );
        assert!(
            output.contains("~4.2k/turn avg"),
            "expected avg in output: {output}"
        );
    }

    #[test]
    fn test_format_remaining_turns_low() {
        let output = format_remaining_turns(2, 5000.0);
        assert!(
            output.contains("~2 turns remaining"),
            "expected turn count: {output}"
        );
        // Should contain YELLOW ANSI escape for warning
        assert!(
            output.contains("\x1b["),
            "expected ANSI color code for low remaining: {output}"
        );
    }

    #[test]
    fn test_format_remaining_turns_zero() {
        let output = format_remaining_turns(0, 8000.0);
        assert!(
            output.contains("nearly full"),
            "expected 'nearly full' warning: {output}"
        );
        // Should contain RED ANSI escape
        assert!(
            output.contains("\x1b["),
            "expected ANSI color code for zero remaining: {output}"
        );
    }
}
