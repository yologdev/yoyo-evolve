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

/// Google's real reply to a bad key, observed on the wire in the Day 254
/// assessment (#1163): `yoyo --provider google --model gemini-2.5-flash -p hi`
/// with GEMINI_API_KEY/GOOGLE_API_KEY unset and ANTHROPIC_API_KEY set got
/// `400 Bad Request` with `"API key not valid. Please pass a valid API key."`
/// and `"reason": "API_KEY_INVALID"`, and yoyo printed raw JSON with no hint.
/// Wrapped the way yoagent 0.27.5's google provider builds it
/// (`"Google API error {status}: {body}"`, classified as `Api`, not `Auth`,
/// because yoagent's classifier maps only 401/403 to `Auth`).
const GOOGLE_BAD_KEY: &str = r#"Google API error 400 Bad Request: {
  "error": {
    "code": 400,
    "message": "API key not valid. Please pass a valid API key.",
    "status": "INVALID_ARGUMENT",
    "details": [
      {
        "@type": "type.googleapis.com/google.rpc.ErrorInfo",
        "reason": "API_KEY_INVALID",
        "domain": "googleapis.com",
        "metadata": { "service": "generativelanguage.googleapis.com" }
      }
    ]
  }
}"#;

#[test]
fn google_400_api_key_invalid_reaches_the_auth_hint() {
    let diag = super::diagnose_api_error_for(GOOGLE_BAD_KEY, "gemini-2.5-flash", true)
        .expect("Google's bad-key 400 must get a diagnosis");
    assert!(
        diag.starts_with("Authentication failed for provider 'google'."),
        "{diag}"
    );
}

#[test]
fn each_google_bad_key_marker_alone_is_an_auth_failure() {
    // Either half of Google's body is enough: the message text and the reason
    // code each identify a rejected key, and neither is a bare status.
    for err in [
        "Google API error 400 Bad Request: API key not valid. Please pass a valid API key.",
        r#"Google API error 400 Bad Request: {"reason": "API_KEY_INVALID"}"#,
    ] {
        let diag = super::diagnose_api_error_for(err, "gemini-2.5-flash", true);
        assert!(
            diag.as_deref()
                .is_some_and(|d| d.starts_with("Authentication failed")),
            "{err} -> {diag:?}"
        );
    }
}

#[test]
fn google_400_that_is_not_about_the_key_gets_no_auth_hint() {
    // Near miss: same status, same INVALID_ARGUMENT, a bad field instead of a
    // bad key. Matching on `400` or `invalid_argument` would mislabel it.
    let err = r#"Google API error 400 Bad Request: {
  "error": {
    "code": 400,
    "message": "Invalid JSON payload received. Unknown name \"tempurature\" at 'generation_config': Cannot find field.",
    "status": "INVALID_ARGUMENT"
  }
}"#;
    let diag = super::diagnose_api_error_for(err, "gemini-2.5-flash", true);
    assert!(
        !diag
            .as_deref()
            .is_some_and(|d| d.contains("Authentication failed")),
        "{diag:?}"
    );
}

#[test]
fn google_cross_provider_fallback_names_both_unset_google_vars() {
    // The assessment's exact case: both of google's own variables unset, the
    // generic fallback sent ANTHROPIC_API_KEY. The hint must not say only
    // GOOGLE_API_KEY is unset when GEMINI_API_KEY (read first) is unset too.
    let got = auth_failure_message(
        "google",
        "GOOGLE_API_KEY",
        &KeySource::Env("ANTHROPIC_API_KEY".into()),
    );
    assert_eq!(
        got,
        format!(
            "Authentication failed for provider 'google'.\n  Neither GEMINI_API_KEY nor GOOGLE_API_KEY is set, so the key from ANTHROPIC_API_KEY was sent instead, and the API rejected it.\nSet the right one with: export GOOGLE_API_KEY=<your-key>\n{CONFIG_HINT}"
        )
    );
    // Remedy check (Day 252): the variable the hint tells the user to export
    // is one google's key lookup actually reads, and every variable it calls
    // unset is one too, so following the advice works.
    let vars = crate::providers::provider_api_key_vars("google");
    let export = got
        .lines()
        .find_map(|l| l.strip_prefix("Set the right one with: export "))
        .and_then(|rest| rest.split('=').next())
        .expect("hint has an export line");
    assert!(vars.contains(&export), "{export} not in {vars:?}");
    for v in vars {
        assert!(got.contains(v), "hint omits {v}: {got}");
    }
}

/// xAI's real reply to a bad key, captured from the built binary on Day 254
/// (`yoyo --provider xai` with a junk key): a 400, not a 401, and its
/// `invalid-argument` code is the same one xAI uses for any bad field (#1165).
const XAI_BAD_KEY: &str = r#"HTTP 400 Bad Request: {"code":"invalid-argument","error":"Incorrect API key provided. You can obtain an API key from https://console.x.ai."}"#;

#[test]
fn xai_400_incorrect_api_key_reaches_the_auth_hint() {
    let diag = super::diagnose_api_error_for(XAI_BAD_KEY, "grok-4", true)
        .expect("xAI's bad-key 400 must get a diagnosis");
    assert!(
        diag.starts_with("Authentication failed for provider 'xai'."),
        "{diag}"
    );
}

#[test]
fn xai_400_that_is_not_about_the_key_gets_no_auth_hint() {
    // Near miss: same status, same `invalid-argument` code, a bad field
    // instead of a bad key. Keying on `400` or `invalid-argument` would
    // mislabel it.
    let err = r#"HTTP 400 Bad Request: {"code":"invalid-argument","error":"Invalid request content: temperature must be <= 2"}"#;
    let diag = super::diagnose_api_error_for(err, "grok-4", true);
    assert!(
        !diag
            .as_deref()
            .is_some_and(|d| d.contains("Authentication failed")),
        "{diag:?}"
    );
}
