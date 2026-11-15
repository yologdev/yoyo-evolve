//! Which config keys hold credentials, so `config show` / `config get` print
//! `***` instead of their value. Moved out of `commands_config.rs`, which sits
//! at the module-size gate.
//!
//! The error directions are not symmetric. Masking a setting costs a `***`
//! where a number should be; failing to mask a credential prints it to a
//! terminal, a screen share or a pasted bug report. And `config show` prints
//! keys yoyo does not read too, so the names that can hold a secret are
//! whatever a user types. Hence: match credential words anywhere in the name
//! (the rule before #1194), and un-mask only named keys that yoyo itself reads
//! and knows to be settings (#1197).

/// Known yoyo keys whose names contain a credential word but whose values are
/// settings, so they print raw. Only keys yoyo reads (`SETTABLE_KEYS` ∪
/// `TOP_LEVEL_KEYS`) may be here: an unknown key's value could be anything.
/// Matched exactly (TOML keys are case-sensitive; `MAX_TOKENS` is unknown).
pub(crate) const SHOWN_KNOWN_KEYS: &[(&str, &str)] = &[(
    "max_tokens",
    "the cap on reply length, a number like 8192 (#1194)",
)];

/// A name containing any of these anywhere (case-insensitive) is masked.
const CREDENTIAL_WORDS: &[&str] = &[
    "key",
    "token",
    "secret",
    "password",
    "passwd",
    "credential",
    "bearer",
    "cookie",
    "auth",
];

/// Masked only as a whole `_ - .` segment: as a substring these sit inside
/// ordinary words (`path`, `pattern`).
const CREDENTIAL_SEGMENTS: &[&str] = &["pat"];

/// Return `true` if a config key looks like a secret and its value should be
/// masked in any user-visible output.
pub(crate) fn is_secret_key(key: &str) -> bool {
    if SHOWN_KNOWN_KEYS.iter().any(|(k, _)| *k == key) {
        return false;
    }
    let lower = key.to_ascii_lowercase();
    CREDENTIAL_WORDS.iter().any(|w| lower.contains(w))
        || lower
            .split(['_', '-', '.'])
            .any(|seg| CREDENTIAL_SEGMENTS.contains(&seg))
}
