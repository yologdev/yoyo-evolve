//! Unit tests for `main.rs`.
//!
//! `#[cfg(test)]`-only sibling of `src/main.rs`, declared there as
//! `#[cfg(test)] mod main_tests;` so it never reaches a release build. This is a
//! **pure move** (Day 172): same tests, same names, same assertions — `main.rs`
//! sat at 1999 lines against a `MAX_MODULE_LINES = 2000` gate with one line of
//! room, and every recent task lands in it. Same form as the two precedents,
//! `src/help_data_guards.rs` (Day 166) and `src/commands_risk_epistemic_tests.rs`
//! (Day 169).
//!
//! `main_tests` is a child of the crate root, so `use super::*;` resolves to
//! exactly what it did inside `main.rs` and the import header is unchanged.
//! `include_str!("main.rs")` is resolved relative to *this* file's directory,
//! which is still `src/`, so the source-level tests still read `src/main.rs` —
//! and now the needle they search for lives in a different file than the text
//! they search, which makes the self-match guard stronger, not weaker.

use super::*;
use serial_test::serial;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[test]
fn looks_like_slash_command_detects_leading_slash() {
    assert!(looks_like_slash_command("/doctor"));
    assert!(looks_like_slash_command("/help"));
    assert!(looks_like_slash_command("/"));
}

#[test]
fn looks_like_slash_command_handles_leading_whitespace() {
    // The caller already trims, but we should be robust to \n/doctor\n etc.
    assert!(looks_like_slash_command("  /doctor"));
    assert!(looks_like_slash_command("\n/doctor\n"));
    assert!(looks_like_slash_command("\t/status"));
}

#[test]
fn looks_like_slash_command_rejects_mid_string_slash() {
    // A slash that isn't the first non-whitespace character must NOT trigger.
    assert!(!looks_like_slash_command("what does /doctor do?"));
    assert!(!looks_like_slash_command("explain /help to me"));
    assert!(!looks_like_slash_command("path: a/b/c"));
}

#[test]
fn looks_like_slash_command_rejects_non_slash_input() {
    assert!(!looks_like_slash_command("hello"));
    assert!(!looks_like_slash_command(""));
    assert!(!looks_like_slash_command("   "));
    assert!(!looks_like_slash_command("-flag"));
}

#[test]
fn test_always_approve_flag_starts_false() {
    // The "always" flag should start as false
    let flag = Arc::new(AtomicBool::new(false));
    assert!(!flag.load(Ordering::Relaxed));
}

#[test]
fn test_checkpoint_triggered_flag_starts_false() {
    // CHECKPOINT_TRIGGERED should default to false
    assert!(!CHECKPOINT_TRIGGERED.load(Ordering::SeqCst));
}

#[test]
fn test_always_approve_flag_persists_across_clones() {
    // Simulates the confirm closure: flag is shared via Arc
    let always_approved = Arc::new(AtomicBool::new(false));
    let flag_clone = Arc::clone(&always_approved);

    // Initially not set
    assert!(!flag_clone.load(Ordering::Relaxed));

    // User answers "always" — set the flag
    always_approved.store(true, Ordering::Relaxed);

    // The clone sees the update (simulates next confirm call)
    assert!(flag_clone.load(Ordering::Relaxed));
}

#[test]
fn test_always_approve_response_matching() {
    // Verify the response matching logic for "always" variants
    let responses_that_approve = ["y", "yes", "a", "always"];
    let responses_that_deny = ["n", "no", "", "maybe", "nope"];

    for r in &responses_that_approve {
        let normalized = r.trim().to_lowercase();
        assert!(
            matches!(normalized.as_str(), "y" | "yes" | "a" | "always"),
            "Expected '{}' to be approved",
            r
        );
    }

    for r in &responses_that_deny {
        let normalized = r.trim().to_lowercase();
        assert!(
            !matches!(normalized.as_str(), "y" | "yes" | "a" | "always"),
            "Expected '{}' to be denied",
            r
        );
    }
}

#[test]
fn test_always_approve_only_on_a_or_always() {
    // Only "a" and "always" should set the persist flag, not "y" or "yes"
    let always_responses = ["a", "always"];
    let single_responses = ["y", "yes"];

    for r in &always_responses {
        let normalized = r.trim().to_lowercase();
        assert!(
            matches!(normalized.as_str(), "a" | "always"),
            "Expected '{}' to trigger always-approve",
            r
        );
    }

    for r in &single_responses {
        let normalized = r.trim().to_lowercase();
        assert!(
            !matches!(normalized.as_str(), "a" | "always"),
            "Expected '{}' NOT to trigger always-approve",
            r
        );
    }
}

#[test]
fn test_always_approve_flag_used_in_confirm_simulation() {
    // End-to-end simulation of the confirm flow with "always"
    let always_approved = Arc::new(AtomicBool::new(false));

    // Simulate three bash commands in sequence
    let commands = ["ls", "echo hello", "cat file.txt"];
    let user_responses = ["a", "", ""]; // user answers "always" first time

    for (i, cmd) in commands.iter().enumerate() {
        let approved = if always_approved.load(Ordering::Relaxed) {
            // Auto-approved — no prompt needed
            true
        } else {
            let response = user_responses[i].trim().to_lowercase();
            let result = matches!(response.as_str(), "y" | "yes" | "a" | "always");
            if matches!(response.as_str(), "a" | "always") {
                always_approved.store(true, Ordering::Relaxed);
            }
            result
        };

        match i {
            0 => assert!(
                approved,
                "First command '{}' should be approved via 'a'",
                cmd
            ),
            1 => assert!(approved, "Second command '{}' should be auto-approved", cmd),
            2 => assert!(approved, "Third command '{}' should be auto-approved", cmd),
            _ => unreachable!(),
        }
    }
}

/// Helper to create a default AgentConfig for tests.
fn test_agent_config(provider: &str, model: &str) -> AgentConfig {
    AgentConfig {
        model: model.to_string(),
        api_key: "test-key".to_string(),
        provider: provider.to_string(),
        base_url: None,
        skills: yoagent::skills::SkillSet::empty(),
        system_prompt: "Test prompt.".to_string(),
        thinking: ThinkingLevel::Off,
        max_tokens: None,
        temperature: None,
        max_turns: None,
        auto_approve: true,
        auto_commit: false,
        permissions: cli::PermissionConfig::default(),
        dir_restrictions: cli::DirectoryRestrictions::default(),
        context_strategy: cli::ContextStrategy::default(),
        context_window: None,
        shell_hooks: vec![],
        fallback_provider: None,
        fallback_model: None,
        auto_watch: true,
        allowed_tools: vec![],
        disallowed_tools: vec![],
        no_tools: false,
        lite: false,
        bash_cwd: None,
    }
}

#[test]
fn test_build_json_output_valid_json_with_expected_keys() {
    let response = PromptOutcome {
        text: "Hello, world!".to_string(),
        last_tool_error: None,
        last_tool_name: None,
        was_overflow: false,
        last_api_error: None,
    };
    let usage = Usage {
        input: 100,
        output: 50,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 150,
    };
    let result = build_json_output(
        &response,
        "claude-sonnet-4-20250514",
        &usage,
        false,
        &SessionChanges::new(),
        std::time::Duration::from_millis(1234),
        3,
    );

    // Must be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("build_json_output should produce valid JSON");

    // Check all expected keys exist
    assert_eq!(parsed["response"], "Hello, world!");
    assert_eq!(parsed["model"], "claude-sonnet-4-20250514");
    assert_eq!(parsed["is_error"], false);
    assert!(parsed["usage"].is_object());
    assert_eq!(parsed["usage"]["input_tokens"], 100);
    assert_eq!(parsed["usage"]["output_tokens"], 50);
    assert_eq!(parsed["usage"]["cache_read_input_tokens"], 0);
    assert_eq!(parsed["usage"]["cache_creation_input_tokens"], 0);
    assert!(parsed["cost_usd"].is_number());
    assert_eq!(parsed["duration_ms"], 1234);
    assert_eq!(parsed["num_turns"], 3);
}

#[test]
fn test_build_json_output_error_mode() {
    let response = PromptOutcome {
        text: "Something went wrong".to_string(),
        last_tool_error: None,
        last_tool_name: None,
        was_overflow: false,
        last_api_error: Some("API error".to_string()),
    };
    let usage = Usage {
        input: 10,
        output: 5,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 15,
    };
    let result = build_json_output(
        &response,
        "claude-sonnet-4-20250514",
        &usage,
        true,
        &SessionChanges::new(),
        std::time::Duration::from_millis(1234),
        3,
    );

    let parsed: serde_json::Value = serde_json::from_str(&result)
        .expect("build_json_output should produce valid JSON even in error mode");

    assert_eq!(parsed["response"], "Something went wrong");
    assert_eq!(parsed["is_error"], true);
    assert!(parsed["usage"].is_object());
    assert!(parsed["cost_usd"].is_number());
    assert_eq!(parsed["duration_ms"], 1234);
    assert_eq!(parsed["num_turns"], 3);
    assert_eq!(parsed["usage"]["cache_read_input_tokens"], 0);
    assert_eq!(parsed["usage"]["cache_creation_input_tokens"], 0);
}

#[test]
fn bedrock_credentials_noop_for_non_bedrock() {
    let mut config = test_agent_config("anthropic", "test-model");
    config.api_key = "sk-test".to_string();
    apply_bedrock_credentials(&mut config);
    assert_eq!(config.api_key, "sk-test");
}

#[test]
fn bedrock_credentials_noop_when_already_combined() {
    let mut config = test_agent_config("bedrock", "test-model");
    config.api_key = "access:secret".to_string();
    apply_bedrock_credentials(&mut config);
    assert_eq!(config.api_key, "access:secret");
}

#[test]
#[serial]
fn bedrock_credentials_combines_access_and_secret() {
    // SAFETY: test runs serially, no concurrent readers
    unsafe {
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "my-secret");
        std::env::remove_var("AWS_SESSION_TOKEN");
    }
    let mut config = test_agent_config("bedrock", "test-model");
    config.api_key = "my-access".to_string();
    apply_bedrock_credentials(&mut config);
    assert_eq!(config.api_key, "my-access:my-secret");
    unsafe {
        std::env::remove_var("AWS_SECRET_ACCESS_KEY");
    }
}

#[test]
#[serial]
fn bedrock_credentials_includes_session_token() {
    // SAFETY: test runs serially, no concurrent readers
    unsafe {
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "my-secret");
        std::env::set_var("AWS_SESSION_TOKEN", "my-token");
    }
    let mut config = test_agent_config("bedrock", "test-model");
    config.api_key = "my-access".to_string();
    apply_bedrock_credentials(&mut config);
    assert_eq!(config.api_key, "my-access:my-secret:my-token");
    unsafe {
        std::env::remove_var("AWS_SECRET_ACCESS_KEY");
        std::env::remove_var("AWS_SESSION_TOKEN");
    }
}

// --- build_json_output tests ---

#[test]
fn test_build_json_output_empty_text() {
    let response = PromptOutcome {
        text: String::new(),
        last_tool_error: None,
        last_tool_name: None,
        was_overflow: false,
        last_api_error: None,
    };
    let usage = Usage {
        input: 0,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 0,
    };
    let result = build_json_output(
        &response,
        "test-model",
        &usage,
        false,
        &SessionChanges::new(),
        std::time::Duration::from_millis(1234),
        3,
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("empty text should produce valid JSON");
    assert_eq!(parsed["response"], "");
    assert_eq!(parsed["is_error"], false);
}

#[test]
fn test_build_json_output_special_characters() {
    // Quotes, newlines, unicode — all must be properly escaped in JSON
    let response = PromptOutcome {
        text: "He said \"hello\"\nnew line\ttab\u{2713} checkmark".to_string(),
        last_tool_error: None,
        last_tool_name: None,
        was_overflow: false,
        last_api_error: None,
    };
    let usage = Usage {
        input: 10,
        output: 20,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 30,
    };
    let result = build_json_output(
        &response,
        "test-model",
        &usage,
        false,
        &SessionChanges::new(),
        std::time::Duration::from_millis(1234),
        3,
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("special chars should produce valid JSON");
    // The response field should contain the original text with special chars intact
    assert!(parsed["response"].as_str().unwrap().contains("\"hello\""));
    assert!(parsed["response"].as_str().unwrap().contains('\n'));
    assert!(parsed["response"].as_str().unwrap().contains('\u{2713}'));
}

#[test]
fn test_build_json_output_structure_completeness() {
    // Verify that all and only the expected top-level keys are present
    let response = PromptOutcome {
        text: "test".to_string(),
        last_tool_error: None,
        last_tool_name: None,
        was_overflow: false,
        last_api_error: None,
    };
    let usage = Usage {
        input: 1,
        output: 1,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 2,
    };
    let result = build_json_output(
        &response,
        "m",
        &usage,
        false,
        &SessionChanges::new(),
        std::time::Duration::from_millis(1234),
        3,
    );
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let obj = parsed.as_object().unwrap();

    // Exactly 8 top-level keys
    assert_eq!(
        obj.len(),
        8,
        "expected 8 top-level keys, got {:?}",
        obj.keys().collect::<Vec<_>>()
    );
    assert!(obj.contains_key("response"));
    assert!(obj.contains_key("model"));
    assert!(obj.contains_key("usage"));
    assert!(obj.contains_key("cost_usd"));
    assert!(obj.contains_key("is_error"));
    assert!(obj.contains_key("session"));
    assert!(obj.contains_key("duration_ms"));
    assert!(obj.contains_key("num_turns"));

    // usage sub-object has exactly 4 keys
    let usage_obj = parsed["usage"].as_object().unwrap();
    assert_eq!(usage_obj.len(), 4);
    assert!(usage_obj.contains_key("input_tokens"));
    assert!(usage_obj.contains_key("output_tokens"));
    assert!(usage_obj.contains_key("cache_read_input_tokens"));
    assert!(usage_obj.contains_key("cache_creation_input_tokens"));
}

#[test]
fn test_build_json_output_cost_is_non_negative() {
    let response = PromptOutcome {
        text: "x".to_string(),
        last_tool_error: None,
        last_tool_name: None,
        was_overflow: false,
        last_api_error: None,
    };
    let usage = Usage {
        input: 1000,
        output: 500,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 1500,
    };
    let result = build_json_output(
        &response,
        "claude-sonnet-4-20250514",
        &usage,
        false,
        &SessionChanges::new(),
        std::time::Duration::from_millis(1234),
        3,
    );
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let cost = parsed["cost_usd"].as_f64().unwrap();
    assert!(cost >= 0.0, "cost should be non-negative, got {}", cost);
}

#[test]
fn test_build_json_output_unknown_model_still_valid() {
    // Even with an unknown model (where cost estimation may return 0), JSON is valid
    let response = PromptOutcome {
        text: "result".to_string(),
        last_tool_error: None,
        last_tool_name: None,
        was_overflow: false,
        last_api_error: None,
    };
    let usage = Usage {
        input: 50,
        output: 25,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 75,
    };
    let result = build_json_output(
        &response,
        "unknown-model-xyz",
        &usage,
        false,
        &SessionChanges::new(),
        std::time::Duration::from_millis(1234),
        3,
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("unknown model should still produce valid JSON");
    assert_eq!(parsed["model"], "unknown-model-xyz");
}

#[test]
fn test_build_json_output_includes_session_changes() {
    let response = PromptOutcome {
        text: "done".to_string(),
        last_tool_error: None,
        last_tool_name: None,
        was_overflow: false,
        last_api_error: None,
    };
    let usage = Usage {
        input: 100,
        output: 50,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 150,
    };
    let changes = SessionChanges::new();
    changes.record("src/main.rs", session::ChangeKind::Write);
    changes.record("src/cli.rs", session::ChangeKind::Edit);

    let result = build_json_output(
        &response,
        "test-model",
        &usage,
        false,
        &changes,
        std::time::Duration::from_millis(1234),
        3,
    );
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    // session key must exist
    assert!(
        parsed["session"].is_object(),
        "expected 'session' key in JSON output"
    );
    assert_eq!(parsed["session"]["files_changed"], 2);

    let arr = parsed["session"]["changes"].as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["path"], "src/main.rs");
    assert_eq!(arr[0]["kind"], "write");
    assert_eq!(arr[1]["path"], "src/cli.rs");
    assert_eq!(arr[1]["kind"], "edit");
}

#[test]
fn test_build_json_output_empty_session_changes() {
    let response = PromptOutcome {
        text: "nothing changed".to_string(),
        last_tool_error: None,
        last_tool_name: None,
        was_overflow: false,
        last_api_error: None,
    };
    let usage = Usage {
        input: 10,
        output: 5,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 15,
    };
    let changes = SessionChanges::new();

    let result = build_json_output(
        &response,
        "test-model",
        &usage,
        false,
        &changes,
        std::time::Duration::from_millis(1234),
        3,
    );
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert_eq!(parsed["session"]["files_changed"], 0);
    assert!(parsed["session"]["changes"].as_array().unwrap().is_empty());
}

/// #678 / #700: `prompt.rs` exposes wrapper pairs, and one of each pair builds a
/// *throwaway* `SessionChanges` tracker. When `main.rs` calls the throwaway flavour it
/// still owns a `session_changes` that nothing ever writes to — so the post-prompt watch
/// cycle sees "no files changed this turn" and skips, and `--output-format json` reports
/// an empty session summary, for a run that demonstrably edited files.
///
/// There is no unit-testable seam here (the real check needs a live `Agent`), so this is
/// pinned at the source level instead. The needles are assembled at runtime so that the
/// literals never appear in this file — otherwise the test would match itself and pass
/// (or fail) for the wrong reason.
#[test]
fn test_main_never_calls_the_tracker_less_prompt_wrappers() {
    let src = include_str!("main.rs");
    for stem in ["run_prompt", "run_prompt_with_content"] {
        let needle = format!("{}{}", stem, "(");
        assert!(
            !src.contains(&needle),
            "src/main.rs calls the tracker-less `{needle})` wrapper, which builds a \
             throwaway SessionChanges. Call the *_with_changes / *_and_changes sibling \
             and pass the caller's tracker — watch gating (should_run_watch_after_prompt) \
             and --output-format json both read it."
        );
    }
}

// --- piped-mode auto-continue gate (#794 half (b)) ---

/// Every stop condition, tested independently: each `false`/exceeded input
/// alone must stop the loop, and all-true must continue. This is the only
/// part of the piped auto-continue loop that can be driven by a test — the
/// loop body is async and does network I/O.
#[test]
fn piped_should_continue_requires_every_condition() {
    // All conditions met -> continue.
    assert!(piped_should_continue(true, 0, 5, false, false, true));
    assert!(piped_should_continue(true, 4, 5, false, false, true));

    // Not opted in -> never continues, whatever else is true.
    assert!(!piped_should_continue(false, 0, 5, false, false, true));

    // Budget spent: count == max stops, and so does count > max.
    assert!(!piped_should_continue(true, 5, 5, false, false, true));
    assert!(!piped_should_continue(true, 6, 5, false, false, true));
    // max == 0 (auto_continue disabled in config) stops immediately.
    assert!(!piped_should_continue(true, 0, 0, false, false, true));

    // An error on the previous turn stops.
    assert!(!piped_should_continue(true, 0, 5, true, false, true));

    // A spent session wall-clock budget stops.
    assert!(!piped_should_continue(true, 0, 5, false, true, true));

    // The shared predicate says there's nothing pending -> stop.
    assert!(!piped_should_continue(true, 0, 5, false, false, false));
}

/// Source-level connectivity check: the piped path must still consult the
/// shared `should_auto_continue` decision.
///
/// **This is a weak test and says so.** It only proves the identifier
/// appears in `run_piped_mode`'s body — it cannot prove the call is
/// reached, correctly gated, or correctly bounded. It exists to catch
/// silent deletion of the wiring (the exact failure #794 documented: an
/// `AtomicBool` set and never read), nothing more. The needle is built at
/// runtime so this test's own source can't satisfy it.
#[test]
fn test_piped_mode_consults_the_shared_auto_continue_decision() {
    let src = include_str!("main.rs");
    let start = src
        .find("async fn run_piped_mode(")
        .expect("run_piped_mode should exist in main.rs");
    // Bound the search at the next top-level `\n}` so we're reading this
    // function's body and not the whole file.
    let body = &src[start..];
    let end = body.find("\n}\n").map(|i| i + 2).unwrap_or(body.len());
    let body = &body[..end];

    let needle = format!("{}{}", "should_auto_continue", "(");
    assert!(
        body.contains(&needle),
        "run_piped_mode no longer calls `{needle})`. Piped mode is the path \
         every evolve-loop agent takes (stdin is not a terminal), so without \
         this call `--continue-on-silence` is a flag that is set and never \
         read (#794)."
    );
}

// --- looks_like_slash_command edge case tests ---

#[test]
fn looks_like_slash_command_slash_followed_by_numbers() {
    // /123 is technically a slash command (starts with /)
    assert!(looks_like_slash_command("/123"));
    assert!(looks_like_slash_command("/42foo"));
}

#[test]
fn looks_like_slash_command_only_whitespace_before_slash() {
    assert!(looks_like_slash_command("   /test"));
    assert!(looks_like_slash_command("\t\t/test"));
    assert!(looks_like_slash_command(" \n \t /test"));
}

#[test]
fn looks_like_slash_command_empty_and_whitespace() {
    assert!(!looks_like_slash_command(""));
    assert!(!looks_like_slash_command("   "));
    assert!(!looks_like_slash_command("\n\t\n"));
}

#[test]
fn looks_like_slash_command_slash_only() {
    // A single "/" should still be detected as a slash command
    assert!(looks_like_slash_command("/"));
    assert!(looks_like_slash_command("  /"));
}

#[test]
fn looks_like_slash_command_unicode_after_slash() {
    assert!(looks_like_slash_command("/café"));
    assert!(looks_like_slash_command("/日本語"));
}

// --- apply_config_flags tests ---

/// Helper to build a minimal Config for testing apply_config_flags.
fn test_config() -> Config {
    Config {
        model: String::new(),
        api_key: String::new(),
        provider: String::new(),
        base_url: None,
        skills: yoagent::skills::SkillSet::empty(),
        system_prompt: String::new(),
        thinking: ThinkingLevel::Off,
        max_tokens: None,
        temperature: None,
        max_turns: None,
        continue_session: false,
        output_path: None,
        prompt_arg: None,
        image_path: None,
        verbose: false,
        mcp_servers: vec![],
        mcp_server_configs: vec![],
        openapi_specs: vec![],
        auto_approve: false,
        auto_edit: false,
        auto_commit: false,
        permissions: cli::PermissionConfig::default(),
        dir_restrictions: cli::DirectoryRestrictions::default(),
        context_strategy: cli::ContextStrategy::default(),
        context_window: None,
        shell_hooks: vec![],
        fallback_provider: None,
        fallback_model: None,
        no_update_check: false,
        json_output: false,
        output_format: cli::OutputFormat::Text,
        audit: false,
        print_system_prompt: false,
        print_mode: false,
        auto_watch: true,
        allowed_tools: vec![],
        disallowed_tools: vec![],
        no_tools: false,
        lite: false,
        safe_mode: false,
    }
}

#[test]
fn test_apply_config_flags_default_returns_true() {
    // Default config (all false) should return true (continue execution)
    let config = test_config();
    assert!(apply_config_flags(&config));
}

#[test]
fn test_apply_config_flags_print_system_prompt_returns_false() {
    // When print_system_prompt is true, function should return false (early exit)
    // We can't easily capture stdout here, but we can verify the return value.
    // This test will print to stdout as a side effect, which is acceptable.
    let mut config = test_config();
    config.print_system_prompt = true;
    config.system_prompt = "test system prompt".to_string();
    assert!(!apply_config_flags(&config));
}

// --- apply_cli_flags tests ---

#[test]
fn test_apply_cli_flags_unknown_flags_ignored() {
    // Unknown flags should not panic or cause errors
    let args = vec![
        "yoyo".to_string(),
        "--unknown-flag".to_string(),
        "--another".to_string(),
    ];
    apply_cli_flags(&args); // should not panic
}

#[test]
fn test_apply_cli_flags_empty_args() {
    // Empty args list should not panic
    let args: Vec<String> = vec![];
    apply_cli_flags(&args); // should not panic
}

#[test]
fn test_apply_cli_flags_mixed_known_and_unknown() {
    // Mix of known and unknown flags should process known ones without error
    let args = vec![
        "yoyo".to_string(),
        "--no-bell".to_string(),
        "--unknown".to_string(),
        "--no-notify".to_string(),
    ];
    apply_cli_flags(&args); // should not panic
}

/// #766 (b): `--print` used to be an early branch in `emit_output` that
/// returned before `write_output_file` was ever reached, so
/// `yoyo --print -p "…" -o file` exited 0 with no file and *nothing* on
/// stderr. `-o` means "also save this", never "instead of printing", so the
/// write must happen in every mode. Asserted at the emission point —
/// `emit_output` itself, not a helper one layer below it.
#[test]
fn emit_output_writes_the_output_file_in_print_mode() {
    let tmp = tempfile::Builder::new()
        .prefix("yoyo_emit_print")
        .tempdir()
        .unwrap();
    let path = tmp.path().join("pm_out.txt");
    let response = PromptOutcome {
        text: "ok".to_string(),
        ..Default::default()
    };
    let failed = emit_output(
        &response,
        "test-model",
        &Usage::default(),
        false,
        &SessionChanges::new(),
        &Some(path.to_string_lossy().to_string()),
        false,
        true, // print_mode
        std::time::Duration::from_secs(0),
        1,
    );
    assert!(!failed, "a successful write must not report failure");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "ok",
        "--print must still honour -o"
    );
}

/// The same decision for `--output-format json`: the middle branch of
/// `emit_output` must not swallow `-o` either.
#[test]
fn emit_output_writes_the_output_file_in_json_mode() {
    let tmp = tempfile::Builder::new()
        .prefix("yoyo_emit_json")
        .tempdir()
        .unwrap();
    let path = tmp.path().join("json_out.txt");
    let response = PromptOutcome {
        text: "ok".to_string(),
        ..Default::default()
    };
    let failed = emit_output(
        &response,
        "test-model",
        &Usage::default(),
        false,
        &SessionChanges::new(),
        &Some(path.to_string_lossy().to_string()),
        true, // json_output
        false,
        std::time::Duration::from_secs(0),
        1,
    );
    assert!(!failed);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "ok");
}

/// #766 (a): a failed `--output` write must reach the caller so the caller
/// can set a non-zero exit code. The helper never terminates the process
/// itself; `emit_output` reports, `run_single_prompt` decides.
#[test]
fn emit_output_reports_a_failed_write_to_its_caller() {
    let tmp = tempfile::Builder::new()
        .prefix("yoyo_emit_fail")
        .tempdir()
        .unwrap();
    // Directory does not exist, so the write cannot succeed.
    let path = tmp.path().join("no_such_dir").join("out.txt");
    let response = PromptOutcome {
        text: "ok".to_string(),
        ..Default::default()
    };
    let failed = emit_output(
        &response,
        "test-model",
        &Usage::default(),
        false,
        &SessionChanges::new(),
        &Some(path.to_string_lossy().to_string()),
        false,
        false,
        std::time::Duration::from_secs(0),
        1,
    );
    assert!(
        failed,
        "a failed -o write must be reported so the caller can exit non-zero"
    );
    assert!(!path.exists());
}

/// No `-o` given: nothing is written and nothing is reported as failed.
#[test]
fn emit_output_without_output_path_reports_no_failure() {
    let response = PromptOutcome {
        text: "ok".to_string(),
        ..Default::default()
    };
    let failed = emit_output(
        &response,
        "test-model",
        &Usage::default(),
        false,
        &SessionChanges::new(),
        &None,
        false,
        true,
        std::time::Duration::from_secs(0),
        1,
    );
    assert!(!failed);
}

#[test]
#[serial]
fn test_apply_cli_flags_no_rtk_via_env() {
    // --no-rtk should also be settable via YOYO_NO_RTK=1 env var
    unsafe {
        std::env::set_var("YOYO_NO_RTK", "1");
    }
    let args = vec!["yoyo".to_string()];
    apply_cli_flags(&args); // should trigger rtk disable via env
    unsafe {
        std::env::remove_var("YOYO_NO_RTK");
    }
}
