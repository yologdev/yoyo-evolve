//! Tests for `needs_setup_from` and the `--api-key` input it takes (#1062).
//! A sibling file because setup.rs sits at its module-size register entry.

use super::needs_setup_from;
use serial_test::serial;
use std::collections::HashMap;

/// The decision matrix for "can yoyo actually start?" — one row per
/// (provider, key_from_cli, key_from_env, key_in_config) shape that reaches
/// the wizard gate. The enumeration lives here in code, not in my memory.
#[test]
fn test_needs_setup_from_matrix() {
    // (provider, cli, env, config, expected_needs_setup, why)
    let cases: &[(&str, bool, bool, bool, bool, &str)] = &[
        // Local / self-hosted providers never need a key — never prompt them.
        ("ollama", false, false, false, false, "ollama needs no key"),
        ("custom", false, false, false, false, "custom needs no key"),
        ("ollama", false, true, false, false, "ollama with a key"),
        // #1062: `--api-key` on argv is a key. Before, the wizard opened and
        // Enter fell back to Anthropic over an argv that named both.
        (
            "openai",
            true,
            false,
            false,
            false,
            "key from --api-key only",
        ),
        ("anthropic", true, false, false, false, "cli key, anthropic"),
        ("openai", true, true, true, false, "key from all three"),
        // A reachable key from env or config means yoyo can start (unchanged).
        ("anthropic", false, true, false, false, "key from env only"),
        (
            "anthropic",
            false,
            false,
            true,
            false,
            "key from config only",
        ),
        (
            "openai",
            false,
            true,
            true,
            false,
            "key from env and config",
        ),
        // No reachable key: the wizard must offer itself. The presence of a
        // config file is irrelevant — a half-configured file (provider +
        // model, no key) used to lock the user out entirely (#628).
        (
            "anthropic",
            false,
            false,
            false,
            true,
            "keyless config: wizard",
        ),
        ("openai", false, false, false, true, "no key anywhere"),
        ("google", false, false, false, true, "no key, google"),
    ];
    for (provider, cli, env, cfg, expected, why) in cases {
        assert_eq!(
            needs_setup_from(provider, *cli, *env, *cfg),
            *expected,
            "provider={provider} cli={cli} env={env} config={cfg}: {why}"
        );
    }
}

/// Where `key_from_cli` comes from: the parsed `Config`, decided by the same
/// branch that decides to USE the argv key. Not read from the
/// `set_key_source` global, which is skipped under `cfg!(test)`.
fn cli_key_flag(argv: &[&str]) -> bool {
    let args: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    // Empty config passed in, so the repo's own .yoyo.toml cannot leak in.
    crate::cli::parse_args_with_config(&args, HashMap::new(), String::new())
        .expect("should parse")
        .api_key_from_cli
}

#[test]
#[serial]
fn api_key_from_cli_is_set_only_by_a_non_empty_argv_key() {
    // An env key so parse_args has something to start with in every row.
    std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    assert!(cli_key_flag(&["yoyo", "--api-key", "sk-dummy"]));
    assert!(cli_key_flag(&[
        "yoyo",
        "--provider",
        "openai",
        "--model",
        "gpt-4o",
        "--api-key",
        "dummy"
    ]));
    // Near-miss: no flag at all, the key came from env → not a cli key.
    assert!(!cli_key_flag(&["yoyo"]));
    std::env::remove_var("ANTHROPIC_API_KEY");
}

/// `--api-key ""` is not a key, and it never reaches the wizard at all:
/// parse_args refuses it with exit 1 ("requires a non-empty value") before
/// any setup decision, so it cannot skip the wizard on a key never used.
/// Whitespace-only is the same. Pinned at the check that refuses it, since
/// the refusal is a process::exit an in-process test cannot survive.
#[test]
fn an_empty_api_key_is_refused_before_the_wizard_decides() {
    use crate::cli::{check_flag_values, FlagIssue, FLAGS_NEEDING_VALUES};
    for empty in ["", "   "] {
        let args: Vec<String> = ["yoyo", "--api-key", empty]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            check_flag_values(&args, FLAGS_NEEDING_VALUES),
            vec![FlagIssue::Empty {
                flag: "--api-key".to_string()
            }],
            "--api-key {empty:?}"
        );
    }
}
