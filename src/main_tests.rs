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
        text_since_last_tool: String::new(),
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
        text_since_last_tool: String::new(),
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
        text_since_last_tool: String::new(),
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
        text_since_last_tool: String::new(),
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
        text_since_last_tool: String::new(),
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

    // Exactly 9 top-level keys (8 original + `external_servers`, #895).
    assert_eq!(
        obj.len(),
        9,
        "expected 9 top-level keys, got {:?}",
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
    assert!(obj.contains_key("external_servers"));

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
        text_since_last_tool: String::new(),
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
        text_since_last_tool: String::new(),
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
        text_since_last_tool: String::new(),
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
        text_since_last_tool: String::new(),
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
        text_since_last_tool: String::new(),
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
        text_since_last_tool: String::new(),
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
        text_since_last_tool: String::new(),
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
        text_since_last_tool: String::new(),
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

/// `emit_output` must record usage for **every** output mode, not just
/// `--output-format json`.
///
/// This is the shape of the 102-day defect (#848): the token counts and cost
/// were computed by `build_json_output` and reachable only under a flag this
/// repo's own harness never passes, while the human-facing route
/// (`print_usage`) is suppressed by quiet mode — which `cli.rs` auto-enables
/// whenever stdin and stdout are both non-terminal, i.e. every piped session.
///
/// Checked by control flow rather than by three end-to-end runs, and the
/// reason is a hazard rather than convenience: `audit_log_usage` reads the
/// process-global `AUDIT_ENABLED`, which `tests/global_state_races.rs` names
/// as a shared setter. A test that switched it on would race its ~5,000
/// siblings, and any tool call executing in that window would append to the
/// **real** `.yoyo/audit.jsonl`. So the emission point is asserted on real
/// bytes one layer down, in `prompt_budget`'s tempdir tests, and this pins the
/// one thing those cannot see: that the call sits *above* the mode branch.
///
/// Deliberately weak, and stated as such: it proves the call is present and
/// positioned, never that a given run wrote a line.
#[test]
fn emit_output_records_usage_before_branching_on_the_output_mode() {
    let src = include_str!("main.rs");
    let start = src
        .find("fn emit_output(")
        .expect("emit_output must exist in main.rs");
    let end = src[start..]
        .find("\n/// Handle `--prompt")
        .map(|i| start + i)
        .expect("emit_output must be followed by run_single_prompt's doc comment");
    let body = &src[start..end];

    // Needles assembled at runtime so this test cannot match itself.
    let call = format!("audit_log_{}(", "usage");
    let call_at = body
        .find(&call)
        .unwrap_or_else(|| panic!("emit_output must call {call} — see #848"));

    let branch = format!("if print_{} {{", "mode");
    let branch_at = body
        .find(&branch)
        .expect("emit_output must still branch on print_mode");

    assert!(
        call_at < branch_at,
        "the usage record must be written before the output-mode branch, or \
         print/json/default modes would not all reach it (#848)"
    );

    // ...and it must not be wrapped in a mode or quiet condition itself.
    //
    // Blind round 85 widened this from one gate to three. It used to name
    // `is_quiet()` alone while the comment above it promised "mode or quiet" —
    // so wrapping the call in `if json_output {` passed: the ordering assertion
    // still held, and `between` contained no `is_quiet()`. That is the #848
    // defect verbatim (usage recorded in one mode only), waved through by the
    // guard built to catch it.
    //
    // `if print_mode {` needs no entry here: `find` returns the *first*
    // occurrence, so a print_mode gate opened before the call would give
    // `branch_at < call_at` and fail the ordering assertion above.
    //
    // Each gate is matched as a **condition** (`if x {` / `f()`), never as a
    // bare identifier, because `between` starts at `fn emit_output(` and so
    // contains the whole parameter list — `json_output` and `print_mode` are
    // parameters, and a bare-name scan would fail on every build.
    let between = &body[..call_at];
    for gate in [
        format!("is_{}()", "quiet"),
        format!("is_plain_{}()", "output"),
        format!("if json_{} {{", "output"),
    ] {
        assert!(
            !between.contains(&gate),
            "the usage record must not be gated on `{gate}` — a gate here is the \
             102-day #848 defect exactly: usage recorded in one mode only, so the \
             dashboard figure stops growing rather than going to zero and no \
             non-zero check can see it. print, json and default must all reach it."
        );
    }
}

/// A minimal `PromptOutcome`/`Usage` pair for driving `build_json_output_with`.
fn json_fixture() -> (PromptOutcome, Usage) {
    (
        PromptOutcome {
            text: "ok".to_string(),
            text_since_last_tool: String::new(),
            last_tool_error: None,
            last_tool_name: None,
            was_overflow: false,
            last_api_error: None,
        },
        Usage {
            input: 1,
            output: 1,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 2,
        },
    )
}

fn json_for(external: &crate::agent_builder::ExternalServerReport) -> serde_json::Value {
    let (response, usage) = json_fixture();
    let out = build_json_output_with(
        &response,
        "m",
        &usage,
        false,
        &SessionChanges::new(),
        std::time::Duration::from_millis(1234),
        3,
        external,
    );
    // Round-tripping through the parser is what pins that the string is real
    // JSON escaped exactly once by serde_json, never hand-built with format!.
    serde_json::from_str(&out).expect("build_json_output must emit parseable JSON")
}

/// The near-miss guard, and it is the entire regression surface: a healthy run
/// emits every pre-existing key byte-identically and the new object is exactly
/// the all-zero/all-empty shape. Asserted with whole-value equality rather than
/// a `contains`, because this is every existing `--output-format json` user.
#[test]
fn healthy_run_json_is_byte_identical_plus_an_all_empty_external_servers() {
    let parsed = json_for(&crate::agent_builder::ExternalServerReport::default());

    // Every pre-existing key, whole-value.
    assert_eq!(parsed["response"], serde_json::json!("ok"));
    assert_eq!(parsed["model"], serde_json::json!("m"));
    assert_eq!(
        parsed["usage"],
        serde_json::json!({
            "input_tokens": 1,
            "output_tokens": 1,
            "cache_read_input_tokens": 0,
            "cache_creation_input_tokens": 0,
        })
    );
    assert_eq!(parsed["duration_ms"], serde_json::json!(1234));
    assert_eq!(parsed["num_turns"], serde_json::json!(3));
    assert_eq!(parsed["is_error"], serde_json::json!(false));

    // The new object, whole-value: present on a healthy run, all-empty.
    assert_eq!(
        parsed["external_servers"],
        serde_json::json!({
            "mcp_connected": 0,
            "mcp_failed": [],
            "openapi_connected": 0,
            "openapi_failed": [],
        }),
        "a healthy run must still carry the key, in the all-empty shape"
    );
}

/// A degraded run names the failed server verbatim in the right array, with the
/// right kind — an `openapi` failure must not report as `mcp`, the same rule
/// `connections_lost_note` follows (a note that misattributes its source is
/// worse than none). `is_error` deliberately stays false: a degraded run that
/// produced a correct answer is not an error.
#[test]
fn degraded_run_names_each_failed_server_under_its_own_kind() {
    let report = crate::agent_builder::ExternalServerReport {
        mcp_connected: 2,
        mcp_failed: vec!["npx -y @modelcontextprotocol/server-foo".to_string()],
        openapi_connected: 0,
        openapi_failed: vec!["/specs/petstore.yaml".to_string()],
    };
    // Anti-vacuous: the fixture must genuinely carry a failure on both axes, so
    // a broken reader cannot pass by both sides agreeing on nothing.
    assert!(!report.mcp_failed.is_empty() && !report.openapi_failed.is_empty());

    let parsed = json_for(&report);
    assert_eq!(
        parsed["external_servers"],
        serde_json::json!({
            "mcp_connected": 2,
            "mcp_failed": ["npx -y @modelcontextprotocol/server-foo"],
            "openapi_connected": 0,
            "openapi_failed": ["/specs/petstore.yaml"],
        })
    );
    // Not folded into is_error: flipping that flag would break every script
    // branching on it, and the run did produce an answer.
    assert_eq!(
        parsed["is_error"],
        serde_json::json!(false),
        "a degraded-but-successful run is not an error"
    );
}

/// Escaping is `serde_json`'s, not `sanitize_for_display`'s (#873 is the
/// *terminal* rule): a control byte in a server command is escaped exactly once
/// by the serializer and survives the round trip byte-identically.
#[test]
fn a_control_byte_in_a_server_command_is_escaped_once_by_serde() {
    let hostile = "npx \u{1b}[2K --spooky\nsecond line";
    let report = crate::agent_builder::ExternalServerReport {
        mcp_failed: vec![hostile.to_string()],
        ..Default::default()
    };
    assert!(
        hostile.bytes().any(|b| b == 0x1b),
        "anti-vacuous: the fixture must really carry a control byte"
    );
    let parsed = json_for(&report);
    assert_eq!(
        parsed["external_servers"]["mcp_failed"][0],
        serde_json::json!(hostile),
        "the command must round-trip verbatim, escaped exactly once"
    );
}

// ---------------------------------------------------------------------------
// Day 197: the process exit code for a finished non-interactive run.
//
// MEASURED first (branch A of the task's step 0). `run_single_prompt` and the
// piped path both exit 1 when the INITIAL turn dies on an API error — that was
// already correct. What was not: `should_exit_error` is computed once, before
// the auto-continue loop, and the loop overwrites `response.last_api_error`
// without revisiting it. So a piped run whose first turn succeeded and whose
// CONTINUATION turn was killed by a terminal rate limit exited 0, with
// `--output-format json` reporting `is_error: false` on the same breath.
// ---------------------------------------------------------------------------

/// The ladder, in both directions and at every boundary.
///
/// The success row is the entire regression surface — every user, every
/// session — and it is asserted with a whole-value `assert_eq!` against 0
/// rather than a `!= 1`, because "not 1" is satisfied by 2 as well.
#[test]
fn exit_code_for_outcome_table() {
    // The pass-through. This row is every successful run there has ever been.
    assert_eq!(
        exit_code_for_outcome(false, false, false),
        0,
        "a clean run must still exit 0, byte-identically"
    );

    // The fix: a failed run is 1, whatever else is true.
    assert_eq!(exit_code_for_outcome(true, false, false), 1);
    assert_eq!(exit_code_for_outcome(true, true, false), 1);

    // #766, unchanged: a failed --output write is a failed run on its own.
    assert_eq!(exit_code_for_outcome(false, true, false), 1);

    // Checkpoint keeps 2, and keeps LOSING to a failure — a checkpoint is a
    // request to resume and a dead turn has nothing to resume from.
    assert_eq!(exit_code_for_outcome(false, false, true), 2);
    assert_eq!(
        exit_code_for_outcome(true, false, true),
        1,
        "a failed run outranks a triggered checkpoint"
    );
    assert_eq!(
        exit_code_for_outcome(false, true, true),
        1,
        "a failed --output write outranks a triggered checkpoint"
    );
}

/// 1 and 2 mean different things in this binary and must not collide.
///
/// 2 is already spoken for twice — the Day-165 bare-word near-miss guard and
/// `yoyo gasp` on a default build — plus `CHECKPOINT_TRIGGERED` here. A failed
/// run must not be reported with a code a caller already reads as something
/// else.
#[test]
fn a_failed_run_does_not_collide_with_the_checkpoint_code() {
    let failed = exit_code_for_outcome(true, false, false);
    let checkpoint = exit_code_for_outcome(false, false, true);
    assert_ne!(
        failed, checkpoint,
        "a dead turn and a checkpoint must be distinguishable by exit code"
    );
    assert_eq!(failed, 1, "a failed run is the conventional 1");
}

/// The wiring guard: the piped path must decide from a value read AFTER the
/// auto-continue loop, not from the pre-loop `should_exit_error`.
///
/// Deliberately weak, and its limit is the point: this proves the value is
/// *positioned* after the loop, never that a process exited. `run_piped_mode`
/// is `async` and does network I/O, so no test drives it behaviourally — the
/// table above is what proves the ladder.
///
/// Every needle is assembled at runtime so the test cannot match its own
/// source, and the anti-vacuous assertions come FIRST: a slice that found
/// nothing satisfies every "expected N" by having nothing to count.
#[test]
fn the_piped_exit_decision_reads_the_post_loop_error_not_the_pre_loop_flag() {
    let src = include_str!("main.rs");
    let fn_marker = format!("async fn {}(", "run_piped_mode");
    let start = src
        .find(&fn_marker)
        .expect("anti-vacuous: run_piped_mode must exist in main.rs");
    let body = &src[start..];
    let end = body
        .find("\n}\n")
        .map(|i| i + 3)
        .expect("anti-vacuous: run_piped_mode's body must terminate");
    let body = &body[..end];

    // Anti-vacuous: the slice must really be the function we think it is.
    assert!(
        body.len() > 1000,
        "anti-vacuous: sliced body is too small to be run_piped_mode ({} bytes)",
        body.len()
    );

    let run_failed = format!("{}_{}", "run", "failed");
    let stale_flag = format!("{}_{}_{}", "should", "exit", "error");
    let post_loop_read = format!("{}.{}.is_some()", "response", "last_api_error");
    let ladder = format!("{}_{}_{}_{}(", "exit", "code", "for", "outcome");

    // Anti-vacuous: the pre-loop flag must still exist — this guard is about
    // where the DECISION reads from, not about deleting the flag.
    let stale_at = body
        .find(&stale_flag)
        .expect("anti-vacuous: should_exit_error must still be computed");

    // Direction 1: the one statement exists and folds in the post-loop error.
    let run_failed_at = body
        .find(&run_failed)
        .expect("the piped path must compute a single run_failed value");
    assert!(
        body.contains(&post_loop_read),
        "run_failed must fold in the value the auto-continue loop overwrites"
    );
    assert!(
        stale_at < run_failed_at,
        "run_failed must be computed AFTER the initial-turn flag it widens"
    );

    // Direction 2: the exit decision goes through the shared ladder, and the
    // stale flag is gone from everything AFTER the one statement. A guard
    // checking only the first passes on a tail carrying BOTH, which is how a
    // half-applied edit reads as done. The slice starts past the end of the
    // `run_failed` binding, because that binding's own right-hand side folds
    // the pre-loop flag in on purpose — that is the fix, not a leftover.
    let ladder_at = body
        .find(&ladder)
        .expect("the piped path must decide its exit code via the shared ladder");
    assert!(
        run_failed_at < ladder_at,
        "run_failed must be computed before it is handed to the ladder"
    );
    let binding_end = run_failed_at
        + body[run_failed_at..]
            .find(";\n")
            .expect("anti-vacuous: the run_failed binding must terminate")
        + 2;
    let tail = &body[binding_end..];
    assert!(
        !tail.contains(&stale_flag),
        "nothing after the run_failed binding may still read the pre-loop flag: {}",
        stale_flag
    );
}
