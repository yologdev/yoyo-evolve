//! Error diagnosis and retry logic for prompt execution.
//!
//! Extracted from `prompt.rs` (Day 64) — these functions handle retry prompt
//! construction, error classification, exponential backoff, and API error
//! diagnosis. They have no dependency on the prompt execution machinery itself.

use crate::format::safe_truncate;
use std::time::Duration;

/// Build a retry prompt that includes error context from a previous failed attempt.
///
/// If `last_error` is `Some`, prepends an error context note to help the model
/// avoid repeating the same mistake. When `tool_name` is provided, a
/// tool-specific recovery hint (from [`tool_recovery_hint`]) is appended so the
/// agent can make a more informed retry decision instead of repeating the same
/// failing approach. If `None`, returns the input unchanged.
pub fn build_retry_prompt(
    input: &str,
    last_error: &Option<String>,
    tool_name: Option<&str>,
) -> String {
    match last_error {
        Some(err) => {
            // Truncate very long errors to keep the prompt focused
            let summary = if err.len() > 200 {
                format!("{}…", safe_truncate(err, 200))
            } else {
                err.clone()
            };
            match tool_name {
                Some(name) => {
                    let hint = tool_recovery_hint(name, 1);
                    format!(
                        "[Previous attempt failed — {name} error: {summary}. {hint}]\n\n{input}"
                    )
                }
                None => {
                    format!(
                        "[Previous attempt failed: {summary}. Try a different approach.]\n\n{input}"
                    )
                }
            }
        }
        None => input.to_string(),
    }
}

/// Maximum retries for transient API errors (rate limits, 5xx, overload).
/// Total wall-clock budget with the capped-exponential-backoff-plus-jitter
/// policy in `retry_delay`: roughly 5 × ~avg(cap/2) = up to ~150s, which
/// comfortably covers normal Anthropic overload windows (30s–2min).
pub(crate) const MAX_RETRIES: u32 = 5;

/// Maximum number of automatic retries when a tool execution fails during a
/// natural-language prompt. The agent re-runs with error context appended so
/// it can self-correct without the user having to `/retry` manually.
pub const MAX_AUTO_RETRIES: u32 = 2;

/// Build a prompt for automatic retry after a tool error.
/// Includes the original input plus context about what went wrong,
/// encouraging the agent to try a different approach. When `tool_name` is
/// provided, the retry prompt names the specific tool and includes a
/// tool-specific recovery hint so the agent can make informed retry decisions.
pub fn build_auto_retry_prompt(
    original_input: &str,
    tool_error: &str,
    tool_name: Option<&str>,
    attempt: u32,
) -> String {
    let summary = if tool_error.len() > 300 {
        format!("{}…", safe_truncate(tool_error, 300))
    } else {
        tool_error.to_string()
    };
    match tool_name {
        Some(name) => {
            let hint = tool_recovery_hint(name, attempt);
            format!(
                "[Auto-retry {attempt}/{MAX_AUTO_RETRIES}: {name} failed with: {summary}. \
                 {hint}]\n\n{original_input}"
            )
        }
        None => {
            format!(
                "[Auto-retry {attempt}/{MAX_AUTO_RETRIES}: a tool failed with: {summary}. \
                 Try a different approach or fix the error.]\n\n{original_input}"
            )
        }
    }
}

/// Return a tool-specific recovery hint for the given tool name.
///
/// Hints escalate based on the retry attempt number:
/// - **Attempt 1**: diagnostic advice (fix the immediate error, same tool)
/// - **Attempt 2+**: concrete alternative tool suggestions (switch tools entirely)
///
/// This prevents premature tool-switching on transient failures while ensuring
/// the agent doesn't get stuck retrying the same failing approach.
pub fn tool_recovery_hint(tool_name: &str, attempt: u32) -> &'static str {
    if attempt >= 2 {
        // Escalate: suggest a concrete alternative tool
        match tool_name {
            "edit_file" => {
                "Try write_file instead: use read_file to get the full current contents, \
                 apply your edit to the full text, then use write_file to replace the entire file."
            }
            "read_file" => {
                "Try bash instead: use `cat <path>` or `head -n 100 <path>` to read \
                 the file contents directly."
            }
            "search" => {
                "Try bash instead: use `grep -rn '<pattern>' <path>` for regex search, \
                 or `find . -name '<pattern>'` for file name search."
            }
            "write_file" => {
                "Try bash instead: use `cat > <path> << 'HEREDOC'` with a heredoc to \
                 write the file contents, or check directory permissions with `ls -la`."
            }
            "rename_symbol" => {
                "Try search + edit_file instead: use search to find all occurrences of \
                 the symbol, then use edit_file on each file to replace them."
            }
            "bash" => {
                "Try a simpler command: break the command into smaller steps, \
                 check if the binary exists with `which <cmd>`, or try an alternative \
                 tool (e.g., read_file instead of cat, search instead of grep)."
            }
            _ => "The tool call failed again. Try a completely different tool or approach.",
        }
    } else {
        // Attempt 1: diagnostic hint (fix the immediate error)
        match tool_name {
            "bash" => {
                "The shell command failed. Check if the command exists, \
                 try a simpler version, or use a different approach."
            }
            "edit_file" => {
                "The edit failed (likely old_text mismatch). Use read_file to see \
                 current contents, then retry with the exact text."
            }
            "write_file" => {
                "The file write failed. Check that the path exists and you have \
                 the right permissions."
            }
            "read_file" => {
                "The file read failed. Use list_files to verify the path, or \
                 search for the file."
            }
            "search" => "The search failed. Try a simpler pattern or check the path.",
            "rename_symbol" => "The rename failed. Verify the symbol exists with search first.",
            _ => "The tool call failed. Try a different approach.",
        }
    }
}

/// Known phrases that indicate context overflow across LLM providers.
/// Mirrors the upstream yoagent patterns so we can detect overflow from
/// error *strings* (e.g., in RetriableError messages or raw API output)
/// even when the structured `ProviderError::ContextOverflow` isn't available.
const OVERFLOW_PHRASES: &[&str] = &[
    "prompt is too long",
    "input is too long",
    "exceeds the context window",
    "exceeds the maximum",
    "maximum prompt length",
    "reduce the length of the messages",
    "maximum context length",
    "exceeds the limit of",
    "exceeds the available context size",
    "greater than the context length",
    "context window exceeds limit",
    "exceeded model token limit",
    "context length exceeded",
    "context_length_exceeded",
    "too many tokens",
    "token limit exceeded",
];

/// Check if an error message indicates a context overflow / prompt-too-long error.
/// Uses phrase matching against `OVERFLOW_PHRASES` since we often only have the raw
/// structured `ProviderError`. Case-insensitive.
pub fn is_overflow_error(msg: &str) -> bool {
    if msg.is_empty() {
        return false;
    }
    let lower = msg.to_lowercase();
    OVERFLOW_PHRASES.iter().any(|phrase| lower.contains(phrase))
}

/// Build a retry prompt after auto-compacting due to context overflow.
/// Tells the model the context was compacted so it can re-orient.
pub fn build_overflow_retry_prompt(original_input: &str) -> String {
    format!(
        "[Context was auto-compacted because the conversation exceeded the model's token limit. \
         Earlier messages have been summarized. Please continue with the task.]\n\n{original_input}"
    )
}

/// Calculate exponential backoff delay with a 60s cap and ±50% jitter.
///
/// The function clamps the exponent so that even pathological `attempt`
/// values (e.g., `u32::MAX`) stay bounded: `2^min(attempt-1, 6)` gives a
/// maximum base of 64 s, capped to `CAP_SECS` = 60 s.
///
/// Jitter is ±50 % of the capped base, derived from nanosecond-precision
/// wall-clock entropy — cheap, deterministic-enough for our use case, and
/// avoids adding `rand` as a direct dependency.
///
/// Why 60 s cap?  Day 33 taught us that uncapped exponential backoff on an
/// Anthropic `overloaded_error` cost an entire session — see journal.
pub fn retry_delay(attempt: u32) -> Duration {
    const CAP_SECS: u64 = 60;
    // Clamp the shift so 2^n can't overflow u64 for pathological inputs.
    let shift = attempt.saturating_sub(1).min(6); // 2^6 = 64 ≥ CAP
    let base = 1u64 << shift;
    let capped = base.min(CAP_SECS);
    // Cheap entropy for ±50% jitter without pulling in `rand` as a direct dep.
    // Nanoseconds-since-epoch provide enough spread for thundering-herd avoidance.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let jitter_bp = (nanos % 1000) as u64; // 0..=999 basis points
    let factor_bp = 500 + jitter_bp; // 500..=1499 → 0.5x..~1.5x
    let jittered_ms = capped * factor_bp; // capped(sec) * factor_bp == capped*1000*factor_bp/1000 (ms)
    Duration::from_millis(jittered_ms.max(500))
}

/// Classify whether an API error message looks transient (worth retrying).
/// Retries: rate limits (429), server errors (5xx), network/connection issues, overloaded.
/// Does NOT retry: auth errors (401/403), invalid requests (400), permission denied,
/// billing/quota exhaustion.
pub fn is_retriable_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();

    // Don't retry auth, client, or billing/quota errors
    let non_retriable = [
        "401",
        "403",
        "400",
        "authentication",
        "unauthorized",
        "forbidden",
        "invalid api key",
        "invalid request",
        "permission denied",
        "invalid_api_key",
        "not_found",
        "404",
        // Billing / quota exhaustion — retrying won't help
        "insufficient_quota",
        "insufficient quota",
        "billing_hard_limit_reached",
        "billing hard limit",
        "credit balance",
        "out of credits",
        "plan limit",
        "spending limit",
        "budget exceeded",
        "quota exceeded",
        "payment required",
        "402",
    ];
    for keyword in &non_retriable {
        if lower.contains(keyword) {
            return false;
        }
    }

    // Retry on transient errors
    let retriable = [
        "429",
        "rate limit",
        "rate_limit",
        "too many requests",
        "500",
        "502",
        "503",
        "504",
        "internal server error",
        "bad gateway",
        "service unavailable",
        "gateway timeout",
        "overloaded",
        "connection",
        "timeout",
        "timed out",
        "network",
        "temporarily",
        "retry",
        "capacity",
        "server error",
        "stream closed",
        "unexpected eof",
        "broken pipe",
        "reset by peer",
        "incomplete",
    ];
    for keyword in &retriable {
        if lower.contains(keyword) {
            return true;
        }
    }

    false
}

/// Diagnose a non-retriable API error and return a user-friendly message
/// with actionable suggestions. Returns `None` for errors that don't match
/// known patterns.
///
/// Recognises five broad classes:
/// 1. **Auth failures** (401/403/unauthorized) — checks the relevant env var
/// 2. **Billing/quota exhaustion** (402/insufficient_quota/credit balance) — actionable billing advice
/// 3. **Rate limits** (429/too many requests) — explains auto-retry behaviour
/// 4. **Network errors** (connection refused/reset/timeout) — provider-specific hints
/// 5. **Model not found** (404/invalid model) — suggests known models for the provider
pub fn diagnose_api_error(error: &str, model: &str) -> Option<String> {
    let lower = error.to_lowercase();
    let provider = infer_provider_from_model(model);

    if lower.contains("401")
        || lower.contains("unauthorized")
        || lower.contains("invalid api key")
        || lower.contains("invalid_api_key")
        || lower.contains("authentication")
    {
        let env_var = crate::cli::provider_api_key_env(&provider).unwrap_or("ANTHROPIC_API_KEY");
        let key_set = std::env::var(env_var).is_ok();
        let config_hint = "Or add api_key to .yoyo.toml, or use --api-key <key>.";
        let status = if key_set {
            format!("  {env_var} is set but the API rejected it — check the key value.")
        } else {
            format!("  {env_var} is not set.")
        };
        return Some(format!(
            "Authentication failed for provider '{provider}'.\n\
             {status}\n\
             Set it with: export {env_var}=<your-key>\n\
             {config_hint}"
        ));
    }

    // Billing / quota exhaustion — not retriable, needs user action
    if lower.contains("insufficient_quota")
        || lower.contains("insufficient quota")
        || lower.contains("billing_hard_limit_reached")
        || lower.contains("billing hard limit")
        || lower.contains("credit balance")
        || lower.contains("out of credits")
        || lower.contains("plan limit")
        || lower.contains("spending limit")
        || lower.contains("budget exceeded")
        || lower.contains("quota exceeded")
        || lower.contains("payment required")
        || lower.contains("402")
    {
        let dashboard = match provider.as_str() {
            "anthropic" => "https://console.anthropic.com/settings/billing",
            "openai" => "https://platform.openai.com/account/billing",
            "google" => "https://console.cloud.google.com/billing",
            "deepseek" => "https://platform.deepseek.com/top_up",
            _ => "",
        };
        let mut msg = format!(
            "Billing/quota limit reached for provider '{provider}'.\n\
             Your API key has exhausted its available credits or hit a spending cap.\n\
             This is not a transient error — retrying won't help."
        );
        if !dashboard.is_empty() {
            msg.push_str(&format!("\n  Check your balance: {dashboard}"));
        }
        msg.push_str(&format!(
            "\n  Options:\n\
             \x20   • Add credits or raise your spending limit in the {provider} dashboard\n\
             \x20   • Switch to a different provider: /provider <name>\n\
             \x20   • Use a local model: /model ollama/llama3"
        ));
        return Some(msg);
    }

    // Rate limits — retriable (handled by auto-retry) but give the user context
    if lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("too many requests")
    {
        return Some(format!(
            "Rate limited by provider '{provider}'.\n\
             yoyo will auto-retry with exponential backoff (up to {MAX_RETRIES} attempts).\n\
             If this persists, you may be on a low-tier plan with strict rate limits.\n\
             Consider upgrading your API plan or switching to a different model."
        ));
    }

    if lower.contains("not_found")
        || lower.contains("model not found")
        || lower.contains("does not exist")
        || lower.contains("model_not_found")
        || lower.contains("invalid model")
        || lower.contains("no such model")
    {
        let known = crate::cli::known_models_for_provider(&provider);
        let mut msg = format!("Model '{model}' was not found by provider '{provider}'.");
        if !known.is_empty() {
            msg.push_str("\nAvailable models for this provider:");
            for m in known {
                msg.push_str(&format!("\n  • {m}"));
            }
            msg.push_str(&format!(
                "\nSwitch with: /model {} or --model {}",
                known[0], known[0]
            ));
        }
        return Some(msg);
    }

    if lower.contains("connection refused")
        || lower.contains("connection reset")
        || lower.contains("connection closed")
        || lower.contains("dns resolution failed")
        || lower.contains("name resolution failed")
        || lower.contains("no route to host")
    {
        let mut msg = String::from("Network error — could not reach the API.\n");
        if provider == "ollama" {
            msg.push_str("  Is Ollama running? Try: ollama serve\n");
        } else if provider == "custom" {
            msg.push_str("  Check your --base-url value.\n");
        } else {
            msg.push_str(&format!(
                "  Check your internet connection and that {provider}'s API is reachable.\n"
            ));
        }
        msg.push_str("  You can retry with /retry.");
        return Some(msg);
    }

    if lower.contains("403") || lower.contains("forbidden") || lower.contains("permission denied") {
        return Some(format!(
            "Access forbidden (403) from provider '{provider}'.\n\
             This usually means your API key doesn't have access to model '{model}'.\n\
             Check your plan/tier with {provider}, or try a different model."
        ));
    }

    if lower.contains("stream ended") {
        return Some(
            "The API stream ended without the expected termination signal.\n\
             This is common with some providers (e.g. MiniMax) whose SSE format \n\
             differs slightly from the OpenAI standard. The response was likely \n\
             delivered in full — check the output above. Not retrying."
                .to_string(),
        );
    }

    if lower.contains("stream closed")
        || lower.contains("unexpected eof")
        || lower.contains("broken pipe")
        || lower.contains("incomplete")
    {
        return Some(
            "The API stream was interrupted before the response completed.\n\
             This is usually a transient network issue — yoyo will auto-retry.\n\
             If it persists, check your internet connection or try a different model."
                .to_string(),
        );
    }

    None
}

/// Infer the provider name from a model identifier.
/// Used by `diagnose_api_error` so it doesn't need `provider` threaded through every caller.
fn infer_provider_from_model(model: &str) -> String {
    let m = model.to_lowercase();
    if m.contains("claude") || m.contains("opus") || m.contains("sonnet") || m.contains("haiku") {
        "anthropic".into()
    } else if m.starts_with("gpt-") || m.starts_with("o3") || m.starts_with("o4") {
        "openai".into()
    } else if m.contains("gemini") {
        "google".into()
    } else if m.contains("grok") {
        "xai".into()
    } else if m.contains("deepseek") {
        "deepseek".into()
    } else if m.contains("mistral") || m.contains("codestral") {
        "mistral".into()
    } else if m.contains("llama") || m.contains("mixtral") || m.contains("gemma") {
        // Could be groq, ollama, or cerebras — default to groq for hosted
        "groq".into()
    } else if m.contains("glm") {
        "zai".into()
    } else {
        "anthropic".into() // safe default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_delay_exponential_backoff_ranges() {
        // Post-Day-47 policy: cap + ±50% jitter. Assertions are ranges, not
        // exact values, so the test doesn't flake on the jitter RNG.
        // Attempt 1 ideal=1s → [0.5s, 1.5s]
        let d1 = retry_delay(1);
        assert!(
            d1 >= Duration::from_millis(500) && d1 <= Duration::from_millis(1500),
            "attempt 1 out of range: {d1:?}"
        );
        // Attempt 2 ideal=2s → [1s, 3s]
        let d2 = retry_delay(2);
        assert!(
            d2 >= Duration::from_secs(1) && d2 <= Duration::from_secs(3),
            "attempt 2 out of range: {d2:?}"
        );
        // Attempt 3 ideal=4s → [2s, 6s]
        let d3 = retry_delay(3);
        assert!(
            d3 >= Duration::from_secs(2) && d3 <= Duration::from_secs(6),
            "attempt 3 out of range: {d3:?}"
        );
    }

    #[test]
    fn test_retry_delay_capped_at_60s() {
        // Very high attempt numbers must be capped (jitter can push up to ~90s,
        // but never the pathological 2^20 seconds the old pure-exponential would).
        let d = retry_delay(20);
        assert!(d <= Duration::from_secs(90), "not capped: {d:?}");
        assert!(d >= Duration::from_secs(30), "cap too aggressive: {d:?}");
    }

    #[test]
    fn test_retry_delay_zero_attempt_floor() {
        // Edge case: attempt 0 with saturating_sub should still yield the floor
        // and land in the attempt-1 jitter window.
        let d = retry_delay(0);
        assert!(d >= Duration::from_millis(500), "below floor: {d:?}");
        assert!(
            d <= Duration::from_millis(1500),
            "above attempt-1 range: {d:?}"
        );
    }

    #[test]
    fn test_is_retriable_rate_limit() {
        assert!(is_retriable_error("429 Too Many Requests"));
        assert!(is_retriable_error("rate limit exceeded"));
        assert!(is_retriable_error("Rate_limit_error: too many requests"));
        assert!(is_retriable_error("too many requests, please slow down"));
    }

    #[test]
    fn test_is_retriable_server_errors() {
        assert!(is_retriable_error("500 Internal Server Error"));
        assert!(is_retriable_error("502 Bad Gateway"));
        assert!(is_retriable_error("503 Service Unavailable"));
        assert!(is_retriable_error("504 Gateway Timeout"));
        assert!(is_retriable_error("the server is overloaded"));
        assert!(is_retriable_error("Server error occurred"));
    }

    #[test]
    fn test_is_retriable_network_errors() {
        assert!(is_retriable_error("connection reset by peer"));
        assert!(is_retriable_error("network error: connection refused"));
        assert!(is_retriable_error("request timed out"));
        assert!(is_retriable_error("timeout waiting for response"));
    }

    #[test]
    fn test_is_not_retriable_auth_errors() {
        assert!(!is_retriable_error("401 Unauthorized"));
        assert!(!is_retriable_error("403 Forbidden"));
        assert!(!is_retriable_error("authentication failed"));
        assert!(!is_retriable_error("invalid api key"));
        assert!(!is_retriable_error("Invalid_api_key: check your key"));
        assert!(!is_retriable_error("permission denied"));
    }

    #[test]
    fn test_is_not_retriable_client_errors() {
        assert!(!is_retriable_error("400 Bad Request"));
        assert!(!is_retriable_error("invalid request body"));
        assert!(!is_retriable_error("404 not_found"));
    }

    #[test]
    fn test_is_not_retriable_billing_quota_errors() {
        // Billing/quota exhaustion — retrying won't help, needs user action
        assert!(!is_retriable_error("insufficient_quota"));
        assert!(!is_retriable_error("Insufficient quota remaining"));
        assert!(!is_retriable_error("billing_hard_limit_reached"));
        assert!(!is_retriable_error("You exceeded your billing hard limit"));
        assert!(!is_retriable_error("Your credit balance is too low"));
        assert!(!is_retriable_error("out of credits"));
        assert!(!is_retriable_error("Plan limit exceeded"));
        assert!(!is_retriable_error("spending limit reached"));
        assert!(!is_retriable_error("Budget exceeded for this month"));
        assert!(!is_retriable_error("quota exceeded"));
        assert!(!is_retriable_error("402 Payment Required"));
        assert!(!is_retriable_error("payment required"));
    }

    #[test]
    fn test_is_not_retriable_unknown_error() {
        // Unknown errors without retriable keywords should NOT be retried
        assert!(!is_retriable_error("something went wrong"));
        assert!(!is_retriable_error("unexpected error"));
    }

    #[test]
    fn test_is_retriable_stream_errors() {
        // "stream ended" is NOT retriable — the response was likely complete
        // (see Issue #222: MiniMax SSE format causes false retries)
        assert!(!is_retriable_error("Stream ended"));

        // Other stream interruptions ARE retriable
        assert!(is_retriable_error("stream closed unexpectedly"));
        assert!(is_retriable_error("unexpected eof while reading"));
        assert!(is_retriable_error("broken pipe"));
        assert!(is_retriable_error("connection reset by peer"));
        assert!(is_retriable_error("incomplete response from server"));
    }

    #[test]
    fn test_stream_ended_not_retriable() {
        // Issue #222: MiniMax's SSE stream doesn't send `data: [DONE]` in the
        // expected format. yoagent reports "stream ended" but the response was
        // already complete. Retrying causes 4x duplicated output.
        assert!(!is_retriable_error("stream ended"));
        assert!(!is_retriable_error("Stream ended"));
        assert!(!is_retriable_error("stream ended unexpectedly"));
        assert!(!is_retriable_error("Stream ended: no more data"));
    }

    #[test]
    fn test_diagnose_stream_ended() {
        // "stream ended" now gets a distinct message (not retriable, Issue #222)
        let diag = diagnose_api_error("error: Stream ended", "claude-sonnet-4-20250514");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("stream ended"));
        assert!(msg.contains("delivered in full"));
        assert!(msg.contains("Not retrying"));
    }

    #[test]
    fn test_diagnose_stream_closed() {
        let diag = diagnose_api_error("stream closed unexpectedly", "gpt-4o");
        assert!(diag.is_some());
        assert!(diag.unwrap().contains("interrupted"));
    }

    #[test]
    fn test_diagnose_unexpected_eof() {
        let diag = diagnose_api_error("unexpected eof", "claude-sonnet-4-20250514");
        assert!(diag.is_some());
        assert!(diag.unwrap().contains("interrupted"));
    }

    #[test]
    fn test_diagnose_broken_pipe() {
        let diag = diagnose_api_error("broken pipe while writing", "claude-sonnet-4-20250514");
        assert!(diag.is_some());
        assert!(diag.unwrap().contains("interrupted"));
    }

    #[test]
    fn test_diagnose_incomplete() {
        let diag = diagnose_api_error("incomplete response", "claude-sonnet-4-20250514");
        assert!(diag.is_some());
        assert!(diag.unwrap().contains("interrupted"));
    }

    #[test]
    fn test_max_auto_retries_constant() {
        assert_eq!(MAX_AUTO_RETRIES, 2);
    }

    #[test]
    fn test_is_overflow_error_anthropic() {
        assert!(is_overflow_error(
            "prompt is too long: 213462 tokens > 200000 maximum"
        ));
    }

    #[test]
    fn test_is_overflow_error_openai() {
        assert!(is_overflow_error(
            "Your input exceeds the context window of this model"
        ));
    }

    #[test]
    fn test_is_overflow_error_google() {
        assert!(is_overflow_error(
            "The input token count (1196265) exceeds the maximum number of tokens allowed"
        ));
    }

    #[test]
    fn test_is_overflow_error_generic_too_many_tokens() {
        assert!(is_overflow_error("too many tokens in request"));
    }

    #[test]
    fn test_is_overflow_error_context_length_exceeded() {
        assert!(is_overflow_error("context length exceeded"));
        assert!(is_overflow_error("context_length_exceeded"));
    }

    #[test]
    fn test_is_overflow_error_max_token_exceeded() {
        assert!(is_overflow_error(
            "exceeded model token limit for this request"
        ));
        assert!(is_overflow_error("token limit exceeded"));
    }

    #[test]
    fn test_is_overflow_error_case_insensitive() {
        assert!(is_overflow_error("PROMPT IS TOO LONG"));
        assert!(is_overflow_error("Too Many Tokens"));
        assert!(is_overflow_error("CONTEXT LENGTH EXCEEDED"));
    }

    #[test]
    fn test_is_overflow_error_bedrock() {
        assert!(is_overflow_error("input is too long for requested model"));
    }

    #[test]
    fn test_is_overflow_error_groq() {
        assert!(is_overflow_error(
            "Please reduce the length of the messages or completion"
        ));
    }

    #[test]
    fn test_is_overflow_error_xai() {
        assert!(is_overflow_error(
            "This model's maximum prompt length is 131072 but request contains 537812 tokens"
        ));
    }

    #[test]
    fn test_is_not_overflow_error() {
        assert!(!is_overflow_error("invalid api key"));
        assert!(!is_overflow_error("rate limit exceeded"));
        assert!(!is_overflow_error("500 Internal Server Error"));
        assert!(!is_overflow_error("connection reset"));
        assert!(!is_overflow_error("bad request"));
        assert!(!is_overflow_error(""));
    }

    #[test]
    fn test_build_overflow_retry_prompt() {
        let prompt = build_overflow_retry_prompt("explain the code");
        assert!(prompt.contains("explain the code"));
        assert!(prompt.contains("auto-compacted"));
    }

    #[test]
    fn test_tool_recovery_hint_bash_attempt1() {
        let hint = tool_recovery_hint("bash", 1);
        assert!(
            hint.contains("command") || hint.contains("shell"),
            "bash hint should mention command or shell: {hint}"
        );
        assert!(
            hint.contains("failed") || hint.contains("check") || hint.contains("different"),
            "bash hint should suggest diagnosis or alternative: {hint}"
        );
    }

    #[test]
    fn test_tool_recovery_hint_bash_attempt2() {
        let hint = tool_recovery_hint("bash", 2);
        assert!(
            hint.contains("simpler") || hint.contains("break") || hint.contains("alternative"),
            "bash escalated hint should suggest simplification or alternative: {hint}"
        );
        assert!(
            hint.contains("which") || hint.contains("exists") || hint.contains("check"),
            "bash escalated hint should suggest verifying the command: {hint}"
        );
    }

    #[test]
    fn test_tool_recovery_hint_edit_file_attempt1() {
        let hint = tool_recovery_hint("edit_file", 1);
        assert!(
            hint.contains("edit") || hint.contains("mismatch") || hint.contains("old_text"),
            "edit_file hint should mention the edit failure mode: {hint}"
        );
        assert!(
            hint.contains("read_file") || hint.contains("current contents"),
            "edit_file hint should suggest reading current contents: {hint}"
        );
    }

    #[test]
    fn test_tool_recovery_hint_edit_file_attempt2() {
        let hint = tool_recovery_hint("edit_file", 2);
        assert!(
            hint.contains("write_file") || hint.contains("replace"),
            "edit_file escalated hint should suggest write_file or full replacement: {hint}"
        );
        assert!(
            hint.contains("read_file")
                || hint.contains("current contents")
                || hint.contains("full"),
            "edit_file escalated hint should mention getting file contents: {hint}"
        );
    }

    #[test]
    fn test_tool_recovery_hint_read_file_attempt2() {
        let hint = tool_recovery_hint("read_file", 2);
        assert!(
            hint.contains("bash") || hint.contains("cat") || hint.contains("head"),
            "read_file escalated hint should suggest a shell alternative: {hint}"
        );
    }

    #[test]
    fn test_tool_recovery_hint_search_attempt2() {
        let hint = tool_recovery_hint("search", 2);
        assert!(
            hint.contains("bash") || hint.contains("grep") || hint.contains("find"),
            "search escalated hint should suggest a shell search alternative: {hint}"
        );
    }

    #[test]
    fn test_tool_recovery_hint_write_file_attempt2() {
        let hint = tool_recovery_hint("write_file", 2);
        assert!(
            hint.contains("bash")
                || hint.contains("cat")
                || hint.contains("heredoc")
                || hint.contains("HEREDOC"),
            "write_file escalated hint should suggest a shell write alternative: {hint}"
        );
    }

    #[test]
    fn test_tool_recovery_hint_rename_symbol_attempt2() {
        let hint = tool_recovery_hint("rename_symbol", 2);
        assert!(
            hint.contains("search") || hint.contains("find") || hint.contains("occurrences"),
            "rename_symbol escalated hint should suggest finding occurrences: {hint}"
        );
        assert!(
            hint.contains("edit_file") || hint.contains("replace"),
            "rename_symbol escalated hint should suggest manual replacement: {hint}"
        );
    }

    #[test]
    fn test_tool_recovery_hint_unknown() {
        let hint = tool_recovery_hint("some_unknown_tool", 1);
        assert!(
            hint.contains("different") || hint.contains("approach") || hint.contains("try"),
            "unknown tool hint should suggest a different approach: {hint}"
        );
    }

    #[test]
    fn test_tool_recovery_hint_unknown_attempt2() {
        let hint = tool_recovery_hint("some_unknown_tool", 2);
        assert!(
            hint.contains("different") || hint.contains("alternative") || hint.contains("approach"),
            "unknown tool escalated hint should suggest trying something different: {hint}"
        );
    }

    #[test]
    fn test_tool_recovery_hint_known_tools_both_attempts() {
        // All known tools should return specific (non-default) hints at both attempt levels
        for tool in &[
            "bash",
            "edit_file",
            "write_file",
            "read_file",
            "search",
            "rename_symbol",
        ] {
            let hint1 = tool_recovery_hint(tool, 1);
            assert!(
                !hint1.contains("The tool call failed"),
                "{tool} attempt 1 should have a specific hint, got default: {hint1}"
            );
            let hint2 = tool_recovery_hint(tool, 2);
            assert!(
                !hint2.contains("The tool call failed"),
                "{tool} attempt 2 should have a specific hint, got default: {hint2}"
            );
            // Escalated hint should differ from diagnostic hint
            assert_ne!(
                hint1, hint2,
                "{tool} should have different hints for attempt 1 vs 2"
            );
        }
    }

    #[test]
    fn test_all_recovery_hints_are_actionable() {
        let tools = [
            "bash",
            "edit_file",
            "write_file",
            "read_file",
            "search",
            "rename_symbol",
            "unknown_tool",
        ];
        let action_words = [
            "try",
            "use",
            "check",
            "verify",
            "retry",
            "break",
            "approach",
            "alternative",
            "different",
        ];
        for tool in &tools {
            for attempt in [1, 2] {
                let hint = tool_recovery_hint(tool, attempt);
                assert!(
                    !hint.is_empty(),
                    "{tool} attempt {attempt} should have a hint"
                );
                let has_action = action_words.iter().any(|w| hint.to_lowercase().contains(w));
                assert!(
                    has_action,
                    "{tool} attempt {attempt} hint should contain an actionable word: {hint}"
                );
            }
        }
    }

    #[test]
    fn test_build_auto_retry_prompt_with_tool_name() {
        let prompt = build_auto_retry_prompt("fix the bug", "file not found", Some("read_file"), 1);
        assert!(
            prompt.contains("read_file"),
            "retry prompt should include tool name: {prompt}"
        );
        assert!(
            prompt.contains("fix the bug"),
            "retry prompt should include original input: {prompt}"
        );
        assert!(
            prompt.contains("file not found"),
            "retry prompt should include error summary: {prompt}"
        );
        // Attempt 1: should contain the diagnostic hint for read_file
        assert!(
            prompt.contains("list_files"),
            "retry prompt should include read_file diagnostic hint: {prompt}"
        );
    }

    #[test]
    fn test_build_auto_retry_prompt_escalates_on_attempt2() {
        let prompt = build_auto_retry_prompt("fix the bug", "file not found", Some("read_file"), 2);
        assert!(
            prompt.contains("read_file"),
            "retry prompt should include tool name: {prompt}"
        );
        // Attempt 2: should contain the escalated alternative tool hint
        assert!(
            prompt.contains("bash"),
            "retry prompt attempt 2 should suggest bash alternative: {prompt}"
        );
        assert!(
            prompt.contains("cat"),
            "retry prompt attempt 2 should suggest cat command: {prompt}"
        );
    }

    #[test]
    fn test_build_auto_retry_prompt_edit_escalation() {
        let prompt =
            build_auto_retry_prompt("refactor code", "old_text not found", Some("edit_file"), 2);
        assert!(
            prompt.contains("write_file"),
            "edit_file attempt 2 should suggest write_file: {prompt}"
        );
    }

    #[test]
    fn test_build_auto_retry_prompt_without_tool_name() {
        let prompt = build_auto_retry_prompt("fix the bug", "something broke", None, 1);
        assert!(
            prompt.contains("a tool failed"),
            "retry prompt without tool name should say 'a tool failed': {prompt}"
        );
        assert!(
            prompt.contains("fix the bug"),
            "retry prompt should include original input: {prompt}"
        );
        assert!(
            prompt.contains("something broke"),
            "retry prompt should include error summary: {prompt}"
        );
    }

    // ---------------------------------------------------------------
    // build_retry_prompt tests
    // ---------------------------------------------------------------

    #[test]
    fn test_build_retry_prompt_no_error() {
        let result = build_retry_prompt("hello world", &None, None);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_build_retry_prompt_short_error() {
        let err = Some("file not found".to_string());
        let result = build_retry_prompt("fix the bug", &err, None);
        assert!(
            result.contains("[Previous attempt failed: file not found. Try a different approach.]"),
            "should wrap error: {result}"
        );
        assert!(
            result.contains("fix the bug"),
            "should include original input: {result}"
        );
    }

    #[test]
    fn test_build_retry_prompt_long_error_truncated() {
        // Create an error longer than 200 chars
        let long_err = "a".repeat(300);
        let err = Some(long_err.clone());
        let result = build_retry_prompt("try again", &err, None);
        // Should contain the truncation marker
        assert!(
            result.contains('…'),
            "long error should be truncated with ellipsis: {result}"
        );
        // Should NOT contain the full 300-char error
        assert!(
            !result.contains(&long_err),
            "should not contain full long error"
        );
        // Should contain the original input
        assert!(result.contains("try again"));
        // Should contain the prefix
        assert!(result.contains("[Previous attempt failed:"));
    }

    #[test]
    fn test_build_retry_prompt_exactly_200_chars_not_truncated() {
        let err_200 = "b".repeat(200);
        let err = Some(err_200.clone());
        let result = build_retry_prompt("input", &err, None);
        // 200 chars is not > 200, so no truncation
        assert!(
            result.contains(&err_200),
            "200-char error should appear in full: {result}"
        );
        assert!(
            !result.contains('…'),
            "200-char error should not have ellipsis"
        );
    }

    #[test]
    fn test_build_retry_prompt_201_chars_truncated() {
        let err_201 = "c".repeat(201);
        let err = Some(err_201);
        let result = build_retry_prompt("input", &err, None);
        assert!(
            result.contains('…'),
            "201-char error should be truncated: {result}"
        );
    }

    #[test]
    fn test_build_retry_prompt_with_tool_name() {
        let err = Some("old_text not found in file".to_string());
        let result = build_retry_prompt("fix it", &err, Some("edit_file"));
        assert!(
            result.contains("edit_file error:"),
            "should mention tool: {result}"
        );
        assert!(
            result.contains("old_text not found"),
            "should include error: {result}"
        );
        // Should include some recovery guidance (semantic check, not exact wording)
        let has_recovery_hint = result.contains("read_file")
            || result.contains("current contents")
            || result.contains("verify")
            || result.contains("mismatch")
            || result.contains("retry");
        assert!(
            has_recovery_hint,
            "should include recovery guidance for edit_file: {result}"
        );
        assert!(
            result.contains("fix it"),
            "should include original input: {result}"
        );
    }

    #[test]
    fn test_build_retry_prompt_with_bash_tool() {
        let err = Some("command not found".to_string());
        let result = build_retry_prompt("run tests", &err, Some("bash"));
        assert!(
            result.contains("bash error:"),
            "should mention bash: {result}"
        );
        // bash hint should include some recovery guidance (resilient to hint text changes)
        let has_recovery_hint = result.contains("command")
            || result.contains("approach")
            || result.contains("simpler")
            || result.contains("try")
            || result.contains("retry");
        assert!(
            has_recovery_hint,
            "should include recovery guidance for bash: {result}"
        );
    }

    #[test]
    fn test_build_retry_prompt_tool_name_no_error() {
        // When there's no error, tool_name is irrelevant — should return input unchanged
        let result = build_retry_prompt("hello", &None, Some("bash"));
        assert_eq!(result, "hello");
    }

    // ---------------------------------------------------------------
    // infer_provider_from_model tests
    // ---------------------------------------------------------------

    #[test]
    fn test_infer_provider_claude_models() {
        assert_eq!(
            infer_provider_from_model("claude-sonnet-4-20250514"),
            "anthropic"
        );
        assert_eq!(infer_provider_from_model("claude-opus-4-6"), "anthropic");
        assert_eq!(infer_provider_from_model("claude-haiku-4-5"), "anthropic");
    }

    #[test]
    fn test_infer_provider_opus_sonnet_haiku_keywords() {
        // These keywords alone should map to anthropic
        assert_eq!(infer_provider_from_model("my-opus-model"), "anthropic");
        assert_eq!(infer_provider_from_model("custom-sonnet"), "anthropic");
        assert_eq!(infer_provider_from_model("haiku-latest"), "anthropic");
    }

    #[test]
    fn test_infer_provider_openai_models() {
        assert_eq!(infer_provider_from_model("gpt-4o"), "openai");
        assert_eq!(infer_provider_from_model("gpt-4o-mini"), "openai");
        assert_eq!(infer_provider_from_model("o3"), "openai");
        assert_eq!(infer_provider_from_model("o4-mini"), "openai");
    }

    #[test]
    fn test_infer_provider_google() {
        assert_eq!(infer_provider_from_model("gemini-2.5-pro"), "google");
        assert_eq!(infer_provider_from_model("gemini-2.0-flash"), "google");
    }

    #[test]
    fn test_infer_provider_xai() {
        assert_eq!(infer_provider_from_model("grok-3"), "xai");
        assert_eq!(infer_provider_from_model("grok-4"), "xai");
    }

    #[test]
    fn test_infer_provider_deepseek() {
        assert_eq!(infer_provider_from_model("deepseek-chat"), "deepseek");
        assert_eq!(infer_provider_from_model("deepseek-reasoner"), "deepseek");
    }

    #[test]
    fn test_infer_provider_mistral() {
        assert_eq!(infer_provider_from_model("mistral-large"), "mistral");
        assert_eq!(infer_provider_from_model("codestral-latest"), "mistral");
    }

    #[test]
    fn test_infer_provider_groq_family() {
        assert_eq!(infer_provider_from_model("llama-3.3-70b"), "groq");
        assert_eq!(infer_provider_from_model("mixtral-8x7b"), "groq");
        assert_eq!(infer_provider_from_model("gemma-7b"), "groq");
    }

    #[test]
    fn test_infer_provider_zai() {
        assert_eq!(infer_provider_from_model("glm-4-plus"), "zai");
    }

    #[test]
    fn test_infer_provider_unknown_defaults_to_anthropic() {
        assert_eq!(infer_provider_from_model("my-custom-model"), "anthropic");
        assert_eq!(infer_provider_from_model("unknown-xyz"), "anthropic");
    }

    #[test]
    fn test_infer_provider_case_insensitive() {
        assert_eq!(infer_provider_from_model("Claude-Opus-4-6"), "anthropic");
        assert_eq!(infer_provider_from_model("GPT-4o"), "openai");
        assert_eq!(infer_provider_from_model("GEMINI-2.5-PRO"), "google");
    }

    // ---------------------------------------------------------------
    // diagnose_api_error tests — auth branch
    // ---------------------------------------------------------------

    #[test]
    fn test_diagnose_auth_401() {
        let diag = diagnose_api_error("401 Unauthorized", "claude-sonnet-4-20250514");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("Authentication failed"), "msg: {msg}");
        assert!(msg.contains("anthropic"), "should mention provider: {msg}");
        assert!(
            msg.contains("ANTHROPIC_API_KEY"),
            "should mention env var: {msg}"
        );
    }

    #[test]
    fn test_diagnose_auth_invalid_api_key() {
        let diag = diagnose_api_error("invalid api key provided", "gpt-4o");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("Authentication failed"));
        assert!(msg.contains("openai"), "should mention openai: {msg}");
        assert!(
            msg.contains("OPENAI_API_KEY"),
            "should mention env var: {msg}"
        );
    }

    #[test]
    fn test_diagnose_auth_key_not_set() {
        // Ensure the env var is not set for this test
        let key = "DEEPSEEK_API_KEY";
        let prev = std::env::var(key).ok();
        std::env::remove_var(key);
        let diag = diagnose_api_error("401 unauthorized", "deepseek-chat");
        let msg = diag.unwrap();
        assert!(
            msg.contains("is not set"),
            "should say key is not set: {msg}"
        );
        // Restore
        if let Some(v) = prev {
            std::env::set_var(key, v);
        }
    }

    #[test]
    fn test_diagnose_auth_key_is_set() {
        let key = "XAI_API_KEY";
        let prev = std::env::var(key).ok();
        std::env::set_var(key, "fake-key");
        let diag = diagnose_api_error("unauthorized", "grok-3");
        let msg = diag.unwrap();
        assert!(
            msg.contains("is set but the API rejected it"),
            "should say key is set but rejected: {msg}"
        );
        // Restore
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    // ---------------------------------------------------------------
    // diagnose_api_error tests — model not found branch
    // ---------------------------------------------------------------

    #[test]
    fn test_diagnose_model_not_found() {
        let diag = diagnose_api_error("model not found: claude-99", "claude-99");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("not found"), "msg: {msg}");
        assert!(msg.contains("claude-99"), "should mention the model: {msg}");
        assert!(
            msg.contains("Available models"),
            "should list alternatives: {msg}"
        );
    }

    #[test]
    fn test_diagnose_model_not_found_openai() {
        let diag = diagnose_api_error("model_not_found", "gpt-99");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("gpt-99"));
        assert!(msg.contains("openai"), "should mention openai: {msg}");
        // Should list at least one known openai model
        assert!(msg.contains("gpt-"), "should list gpt models: {msg}");
    }

    #[test]
    fn test_diagnose_model_does_not_exist() {
        let diag = diagnose_api_error("The model does not exist", "gemini-99");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("gemini-99"));
        assert!(msg.contains("google"));
    }

    // ---------------------------------------------------------------
    // diagnose_api_error tests — connection refused branch
    // ---------------------------------------------------------------

    #[test]
    fn test_diagnose_connection_refused_ollama() {
        let diag = diagnose_api_error("connection refused", "llama-3.3-70b");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("Network error"), "msg: {msg}");
        // llama maps to groq, not ollama — so it won't say "Is Ollama running?"
        // but the network error message should still be there
        assert!(msg.contains("/retry"), "should suggest retry: {msg}");
    }

    #[test]
    fn test_diagnose_connection_refused_generic() {
        let diag = diagnose_api_error("connection refused", "gpt-4o");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("Network error"));
        assert!(msg.contains("openai"), "should mention provider: {msg}");
    }

    #[test]
    fn test_diagnose_dns_resolution_failed() {
        let diag = diagnose_api_error("dns resolution failed", "claude-sonnet-4-20250514");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("Network error"));
    }

    #[test]
    fn test_diagnose_connection_reset() {
        let diag = diagnose_api_error("connection reset by peer", "gemini-2.5-pro");
        assert!(diag.is_some());
        assert!(diag.unwrap().contains("Network error"));
    }

    // ---------------------------------------------------------------
    // diagnose_api_error tests — billing/quota exhaustion branch
    // ---------------------------------------------------------------

    #[test]
    fn test_diagnose_insufficient_quota_anthropic() {
        let diag = diagnose_api_error("insufficient_quota", "claude-sonnet-4-20250514");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(
            msg.contains("Billing/quota limit"),
            "should mention billing: {msg}"
        );
        assert!(msg.contains("anthropic"), "should mention provider: {msg}");
        assert!(
            msg.contains("console.anthropic.com"),
            "should include Anthropic dashboard link: {msg}"
        );
    }

    #[test]
    fn test_diagnose_billing_hard_limit_openai() {
        let diag = diagnose_api_error("billing_hard_limit_reached", "gpt-4o");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("Billing/quota limit"), "msg: {msg}");
        assert!(
            msg.contains("platform.openai.com"),
            "should include OpenAI dashboard link: {msg}"
        );
    }

    #[test]
    fn test_diagnose_402_payment_required() {
        let diag = diagnose_api_error("402 Payment Required", "claude-sonnet-4-20250514");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("Billing/quota limit"), "msg: {msg}");
        assert!(
            msg.contains("credits or hit a spending cap"),
            "should explain the issue: {msg}"
        );
        assert!(
            msg.contains("/provider"),
            "should suggest switching provider: {msg}"
        );
    }

    #[test]
    fn test_diagnose_out_of_credits() {
        let diag = diagnose_api_error("out of credits on your account", "deepseek-chat");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("Billing/quota limit"), "msg: {msg}");
        assert!(
            msg.contains("platform.deepseek.com"),
            "should include DeepSeek dashboard link: {msg}"
        );
    }

    #[test]
    fn test_diagnose_quota_exceeded_unknown_provider() {
        let diag = diagnose_api_error("quota exceeded", "some-unknown-model");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("Billing/quota limit"), "msg: {msg}");
        // Unknown provider should still give generic advice
        assert!(
            msg.contains("Add credits"),
            "should suggest adding credits: {msg}"
        );
    }

    // ---------------------------------------------------------------
    // diagnose_api_error tests — 403 forbidden branch
    // ---------------------------------------------------------------

    #[test]
    fn test_diagnose_403_forbidden() {
        let diag = diagnose_api_error("403 Forbidden", "claude-sonnet-4-20250514");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("Access forbidden"), "msg: {msg}");
        assert!(msg.contains("anthropic"), "should mention provider: {msg}");
        assert!(
            msg.contains("claude-sonnet-4-20250514"),
            "should mention model: {msg}"
        );
    }

    #[test]
    fn test_diagnose_permission_denied() {
        let diag = diagnose_api_error("permission denied for this model", "gpt-4o");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("Access forbidden"));
    }

    // ---------------------------------------------------------------
    // diagnose_api_error tests — unrecognized error
    // ---------------------------------------------------------------

    #[test]
    fn test_diagnose_unrecognized_error_returns_none() {
        assert!(
            diagnose_api_error("something weird happened", "claude-sonnet-4-20250514").is_none()
        );
        assert!(diagnose_api_error("bad request body", "gpt-4o").is_none());
        assert!(diagnose_api_error("", "claude-sonnet-4-20250514").is_none());
    }
}
