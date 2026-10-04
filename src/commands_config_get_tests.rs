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
