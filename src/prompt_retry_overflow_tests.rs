//! #1167: `is_overflow_error` reads overflow the way yoagent does, not from a
//! hand copy of yoagent's private phrase list. The copy drifted twice: it kept
//! the broad `"exceeds the maximum"` (which matches Azure's peak-load rate limit)
//! after yoagent narrowed it, and it never gained
//! `"model_context_window_exceeded"`. Every expectation below is checked
//! against yoagent itself, so the pin is to the dependency's behaviour and not
//! to a list typed here.

use super::is_overflow_error;
use yoagent::provider::ProviderError;
use yoagent::{Message, StopReason, Usage};

/// yoagent's own verdict on an assistant error message carrying `msg`: the
/// shape `error_message` has when prompt.rs reads it.
fn yoagent_reads_overflow(msg: &str) -> bool {
    Message::assistant(vec![], StopReason::Error, "m", "p", Usage::default())
        .with_error_message(msg)
        .is_context_overflow()
}

/// Azure OpenAI's mid-stream capacity error, copied verbatim from yoagent
/// 0.27.7's `AZURE_NO_CAPACITY` fixture (src/provider/traits.rs:736).
const AZURE_NO_CAPACITY: &str = r#"{"type":"error","error":{"type":"too_many_requests","code":"no_capacity","message":"The request exceeds the maximum usage size allowed during peak load. Please retry later.","param":null},"sequence_number":3}"#;

#[test]
fn azure_peak_load_rate_limit_is_not_an_overflow() {
    // Anti-vacuous: this is the text the old broad phrase matched.
    assert!(AZURE_NO_CAPACITY.contains("exceeds the maximum"));
    assert!(!yoagent_reads_overflow(AZURE_NO_CAPACITY));
    assert!(!ProviderError::classify(400, AZURE_NO_CAPACITY).is_context_overflow());
    assert!(
        !is_overflow_error(AZURE_NO_CAPACITY),
        "a peak-load rate limit must not be compacted as a context overflow"
    );
}

#[test]
fn anthropic_in_stream_stop_reason_is_an_overflow() {
    let m = "model_context_window_exceeded";
    assert!(yoagent_reads_overflow(m));
    assert!(is_overflow_error(m));
}

/// What a `ProviderError::ContextOverflow` with an empty body (Cerebras /
/// Mistral 400/413) displays as, i.e. what lands in `error_message`. No phrase
/// matches it; yoagent recognises it by its display prefix.
#[test]
fn empty_body_overflow_display_is_an_overflow() {
    let shown = ProviderError::classify(413, "").to_string();
    assert!(ProviderError::classify(413, "").is_context_overflow());
    assert!(yoagent_reads_overflow(&shown), "display: {shown:?}");
    assert!(is_overflow_error(&shown), "display: {shown:?}");
}

/// One real overflow message per provider, as yoagent's list comments and
/// classify tests name them (src/provider/traits.rs:528-550, 649-693).
const PROVIDER_OVERFLOWS: &[(&str, &str)] = &[
    (
        "anthropic",
        "prompt is too long: 213462 tokens > 200000 maximum",
    ),
    (
        "openai",
        "Your input exceeds the context window of this model",
    ),
    (
        "google",
        "The input token count (1196265) exceeds the maximum number of tokens allowed",
    ),
    ("bedrock", "input is too long for requested model"),
    (
        "xai",
        "This model's maximum prompt length is 131072 but request contains 537812 tokens",
    ),
    (
        "groq",
        "Please reduce the length of the messages or completion",
    ),
    (
        "openrouter",
        "This endpoint's maximum context length is 200000 tokens",
    ),
    (
        "llama.cpp",
        "the request exceeds the available context size, try increasing it",
    ),
    ("openai code", "context_length_exceeded"),
];

#[test]
fn each_provider_overflow_is_read_as_yoagent_reads_it() {
    for (provider, m) in PROVIDER_OVERFLOWS {
        assert!(is_overflow_error(m), "{provider}: {m}");
        assert_eq!(
            is_overflow_error(m),
            ProviderError::classify(400, m).is_context_overflow(),
            "{provider}: yoyo and yoagent's classify disagree on {m:?}"
        );
        assert_eq!(
            is_overflow_error(m),
            yoagent_reads_overflow(m),
            "{provider}: yoyo and yoagent's Message reader disagree on {m:?}"
        );
    }
}

#[test]
fn near_misses_are_not_overflows() {
    for m in [
        "Rate limit reached for requests",
        "Request exceeds the maximum allowed size",
        "invalid request format",
    ] {
        assert!(!is_overflow_error(m), "{m}");
        assert_eq!(is_overflow_error(m), yoagent_reads_overflow(m), "{m}");
    }
    // Empty and whitespace-only text: yoagent's *status* rule reads an empty
    // 400/413 body as overflow, but a bare message string carries no status,
    // so yoyo reads it as not-an-overflow (as yoagent's Message reader does).
    assert!(!is_overflow_error(""));
    assert!(!is_overflow_error("   "));
    assert!(!yoagent_reads_overflow("   "));
}
