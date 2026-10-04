//! `/config get <key>`'s pure core (#982): the printed text and the shell exit
//! code from one function. Split out of `commands_config.rs`, which sits at the
//! module-size gate.

use crate::commands_config::{is_secret_key, settable_keys_list};
use crate::format::{DIM, RESET, YELLOW};

/// Pure core of `/config get <key>`: the exact text printed and the shell exit
/// code. A real key that is merely unset ("using default") is an honest answer
/// and exits 0; only an unknown key exits 2.
pub(crate) fn config_get_report(
    key: &str,
    config: &std::collections::HashMap<String, String>,
    source: &str,
) -> (i32, String) {
    match config.get(key) {
        Some(value) => {
            let display = if is_secret_key(key) {
                "***".to_string()
            } else {
                value.clone()
            };
            (0, format!("{DIM}  {key} = {display}  ({source}){RESET}\n"))
        }
        None => {
            let known: Vec<&str> = crate::config::SETTABLE_KEYS
                .iter()
                .map(|(k, _)| *k)
                .collect();
            // Unknown key: "using default" would claim a real key sits at its default.
            if let Some(msg) = config_get_unknown_key_message(key, &known) {
                (
                    2,
                    format!(
                        "{YELLOW}  {msg}{RESET}\n{DIM}  settable keys: {}{RESET}\n",
                        settable_keys_list()
                    ),
                )
            } else {
                (
                    0,
                    format!("{DIM}  {key} is not set in config file (using default){RESET}\n"),
                )
            }
        }
    }
}

/// `Some(refusal)` when `key` (absent from the file) is not in `known`
/// (`SETTABLE_KEYS`). Says "not a settable config key", never "not a config
/// key": the parser also reads unlisted keys (`base_url`, ...), so the stronger
/// claim would cry wolf on a real, unset key. Glyph-free on purpose.
pub(crate) fn config_get_unknown_key_message(key: &str, known: &[&str]) -> Option<String> {
    if known.contains(&key) {
        return None;
    }
    let mut msg =
        format!("{key} is not a settable config key, and it is not set in the config file");
    if let Some(near) = crate::commands::closest_match(key, known, 2) {
        msg.push_str(&format!(" (did you mean {near}?)"));
    }
    Some(msg)
}
