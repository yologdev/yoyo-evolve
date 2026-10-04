//! Tests for `config_get_unknown_key_message` (`yoyo config get <typo>`).
//! Lives in its own file because `commands_config.rs` sits at the module-size
//! gate's overshoot band.

use crate::commands_config::config_get_unknown_key_message;

fn known() -> Vec<&'static str> {
    crate::config::SETTABLE_KEYS
        .iter()
        .map(|(k, _)| *k)
        .collect()
}

#[test]
fn typo_is_named_unknown_and_suggests_nearest_key() {
    assert_eq!(
        config_get_unknown_key_message("modle", &known()),
        Some(
            "modle is not a settable config key, and it is not set in the config file \
             (did you mean model?)"
                .to_string()
        )
    );
}

#[test]
fn unknown_key_without_neighbour_gets_no_suggestion() {
    let msg = config_get_unknown_key_message("zzzzqqq", &known()).expect("unknown key");
    assert_eq!(
        msg,
        "zzzzqqq is not a settable config key, and it is not set in the config file"
    );
    assert!(!msg.contains("did you mean"));
    assert!(!msg.contains("using default"));
}

/// Near miss: every key in the real authority stays on the old
/// "not set (using default)" path, including keys added later.
#[test]
fn every_settable_key_is_known() {
    let known = known();
    assert!(
        !known.is_empty(),
        "anti-vacuous: SETTABLE_KEYS must be non-empty"
    );
    for key in &known {
        assert_eq!(config_get_unknown_key_message(key, &known), None, "{key}");
    }
}

#[test]
fn message_is_glyph_free() {
    for key in ["modle", "zzzzqqq"] {
        let msg = config_get_unknown_key_message(key, &known()).unwrap();
        assert!(msg.is_ascii(), "{msg}");
    }
}

// #982: the exit code travels with the text, from one pure core.

#[test]
fn unknown_key_reports_exit_2_with_the_message_unchanged() {
    let empty = std::collections::HashMap::new();
    let (code, text) = crate::commands_config_get::config_get_report("modle", &empty, "defaults");
    assert_eq!(code, 2);
    let msg = config_get_unknown_key_message("modle", &known()).unwrap();
    assert!(text.contains(&msg), "{text}");
    assert!(!text.contains("using default"), "{text}");
}

/// Near miss and the regression surface: a REAL key that is merely unset is an
/// honest answer, not a failure, so it exits 0.
#[test]
fn real_but_unset_key_reports_exit_0() {
    let empty = std::collections::HashMap::new();
    let (code, text) = crate::commands_config_get::config_get_report("model", &empty, "defaults");
    assert_eq!(code, 0);
    assert!(
        text.contains("model is not set in config file (using default)"),
        "{text}"
    );
}

#[test]
fn set_key_reports_exit_0_with_its_value_and_source() {
    let mut cfg = std::collections::HashMap::new();
    cfg.insert("model".to_string(), "m-1".to_string());
    let (code, text) =
        crate::commands_config_get::config_get_report("model", &cfg, "/x/.yoyo.toml");
    assert_eq!(code, 0);
    assert!(text.contains("model = m-1  (/x/.yoyo.toml)"), "{text}");
}
