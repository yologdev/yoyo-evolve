//! Tests for `auth_failure_message` (#1062): the 401 hint names where the
//! session's key actually came from. The source is a PARAMETER here, so no
//! test reads or writes the process-global `KEY_SOURCE` and none can race.

use super::{auth_failure_message, KeySource};

const CONFIG_HINT: &str = "Or add api_key to .yoyo.toml, or use --api-key <key>.";

#[test]
fn missing_key_is_byte_identical_to_the_old_hint() {
    // The whole regression surface: no key anywhere → today's text exactly.
    let got = auth_failure_message("openai", "OPENAI_API_KEY", &KeySource::Missing);
    let want = format!(
        "Authentication failed for provider 'openai'.\n  OPENAI_API_KEY is not set.\nSet it with: export OPENAI_API_KEY=<your-key>\n{CONFIG_HINT}"
    );
    assert_eq!(got, want);
}

#[test]
fn own_env_var_is_byte_identical_to_the_old_hint() {
    let got = auth_failure_message(
        "openai",
        "OPENAI_API_KEY",
        &KeySource::Env("OPENAI_API_KEY".into()),
    );
    let want = format!(
        "Authentication failed for provider 'openai'.\n  OPENAI_API_KEY is set but the API rejected it — check the key value.\nSet it with: export OPENAI_API_KEY=<your-key>\n{CONFIG_HINT}"
    );
    assert_eq!(got, want);
}

#[test]
fn argv_key_names_the_flag_and_never_says_not_set() {
    let got = auth_failure_message("openai", "OPENAI_API_KEY", &KeySource::Argv);
    assert_eq!(
        got,
        "Authentication failed for provider 'openai'.\n  The key passed with --api-key was rejected by the API — check the key value.\n--api-key takes precedence over OPENAI_API_KEY and api_key in a config file, so the value you pass is the one to fix."
    );
    // Anti-vacuous: the measured defect was "OPENAI_API_KEY is not set." plus
    // an export line, said to a user who had passed a key on the command line.
    assert!(!got.contains("is not set"), "{got}");
    assert!(!got.contains("export "), "{got}");
}

#[test]
fn config_file_key_names_the_config_file() {
    let got = auth_failure_message("deepseek", "DEEPSEEK_API_KEY", &KeySource::ConfigFile);
    assert_eq!(
        got,
        "Authentication failed for provider 'deepseek'.\n  The api_key from your yoyo config file was rejected by the API — check the key value.\nIt was used because neither --api-key nor DEEPSEEK_API_KEY is set; either one would take precedence."
    );
    assert!(!got.contains("is not set."), "{got}");
}

#[test]
fn provider_alias_env_var_is_named_instead_of_the_canonical_one() {
    // google reads GEMINI_API_KEY before GOOGLE_API_KEY (yoagent's order, #1064),
    // while the canonical name the old hint printed is GOOGLE_API_KEY.
    let got = auth_failure_message(
        "google",
        "GOOGLE_API_KEY",
        &KeySource::Env("GEMINI_API_KEY".into()),
    );
    assert_eq!(
        got,
        "Authentication failed for provider 'google'.\n  GEMINI_API_KEY is set but the API rejected it — check the key value.\nGEMINI_API_KEY is where this session's key came from; fix its value there, or pass --api-key <key>."
    );
    assert!(!got.contains("GOOGLE_API_KEY is not set"), "{got}");
}

#[test]
fn cross_provider_fallback_says_which_key_was_sent_instead() {
    // CLAUDE.md's provider-key bullet: with the provider's own variable unset,
    // the lookup falls back to ANTHROPIC_API_KEY and a 401 follows.
    let got = auth_failure_message(
        "openai",
        "OPENAI_API_KEY",
        &KeySource::Env("ANTHROPIC_API_KEY".into()),
    );
    assert_eq!(
        got,
        format!(
            "Authentication failed for provider 'openai'.\n  OPENAI_API_KEY is not set, so the key from ANTHROPIC_API_KEY was sent instead, and the API rejected it.\nSet the right one with: export OPENAI_API_KEY=<your-key>\n{CONFIG_HINT}"
        )
    );
}

#[test]
fn generic_fallback_without_provider_vars_does_not_invent_a_right_one() {
    // `custom` has no variables of its own; API_KEY reached it via the generic
    // fallback, so there is no "right" variable to point at.
    let got = auth_failure_message(
        "custom",
        "ANTHROPIC_API_KEY",
        &KeySource::Env("API_KEY".into()),
    );
    assert_eq!(
        got,
        "Authentication failed for provider 'custom'.\n  API_KEY is set but the API rejected it — check the key value.\nAPI_KEY is where this session's key came from; fix its value there, or pass --api-key <key>."
    );
}
