//! `/config get <key>`'s pure core (#982): the printed text and the shell exit
//! code from one function. Split out of `commands_config.rs`, which sits at the
//! module-size gate.

use crate::commands_config::settable_keys_list;
use crate::config_secret::is_masked_key;
use crate::format::{DIM, RESET, YELLOW};

/// Pure core of `/config get <key>`: the exact text printed and the shell exit
/// code. A real key that is merely unset ("using default") is an honest answer
/// and exits 0; only an unknown key exits 2.
pub(crate) fn config_get_report(
    key: &str,
    config: &std::collections::HashMap<String, String>,
    source: &str,
) -> (i32, String) {
    let provider = run_provider(config);
    match config.get(key) {
        // #1172: the run reads this value as if the key were absent; say so.
        Some(value) if run_reading(key, value, &provider) == Some(RunReading::Unset) => {
            let shown = if is_masked_key(key) && !value.trim().is_empty() {
                "\"***\"".to_string()
            } else {
                format!("{value:?}")
            };
            (
                0,
                format!(
                    "{DIM}  {key} is not set: {source} has {key} = {shown}, which yoyo reads as unset (using default){RESET}\n"
                ),
            )
        }
        Some(value) => {
            let display = if is_masked_key(key) {
                "***".to_string()
            } else {
                value.clone()
            };
            // #1195: the run uses a different value than the text; name it.
            let note = match run_reading(key, value, &provider) {
                Some(RunReading::As(used)) => format!("  (read as {used})"),
                _ => String::new(),
            };
            (
                0,
                format!("{DIM}  {key} = {display}  ({source}){note}{RESET}\n"),
            )
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

/// What the run does with `value` for `key` (#1172, #1195).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RunReading {
    /// The run uses the value as written.
    AsWritten,
    /// The run reads it as if the key were absent (the default is used).
    Unset,
    /// The run uses something else, named here: a level, a clamped number, a
    /// canonical spelling, or a startup refusal.
    As(String),
}

/// #1172/#1195: what the run does with `value` for `key`, or `None` for a key
/// that is not settable. Every arm asks the run's own reader (the warning-free
/// halves of `cli::parse_thinking_level` / `clamp_temperature`, the startup
/// provider refusals, `config_flag`, ...); none restates its rule.
/// The provider a run on this file uses (its `provider`, else the run's
/// default `anthropic`), which sets the temperature bound (#1207).
pub(crate) fn run_provider(config: &std::collections::HashMap<String, String>) -> String {
    config
        .get("provider")
        .map(|p| p.trim().to_lowercase())
        .unwrap_or_else(|| "anthropic".to_string())
}

pub(crate) fn run_reading(key: &str, value: &str, provider: &str) -> Option<RunReading> {
    use crate::config::*;
    use RunReading::*;
    let one: std::collections::HashMap<String, String> =
        [(key.to_string(), value.to_string())].into();
    let unset_if = |unset: bool| if unset { Unset } else { AsWritten };
    let numeric_none = |flag: &str| match key {
        "max_turns" => crate::cli_config::resolve_numeric_setting::<usize>(&[], flag, &one, key)
            .0
            .is_none(),
        _ => crate::cli_config::resolve_numeric_setting::<u32>(&[], flag, &one, key)
            .0
            .is_none(),
    };
    Some(match key {
        "model" => unset_if(crate::config_value::named_model(value).is_none()),
        "provider" => provider_reading(value),
        "thinking" => {
            let level = crate::cli::thinking_level_from_str(value)
                .unwrap_or(yoagent::ThinkingLevel::Medium);
            let used = crate::commands::thinking_level_name(level);
            if value.to_lowercase() == used {
                AsWritten
            } else {
                As(used.to_string())
            }
        }
        "temperature" => {
            let parsed =
                crate::cli_config::resolve_numeric_setting::<f32>(&[], "--temperature", &one, key)
                    .0;
            match parsed {
                None => Unset,
                Some(t) if t.is_nan() || crate::cli::clamped_temperature(provider, t) == t => {
                    AsWritten
                }
                Some(t) => As(format!(
                    "{:?}",
                    crate::cli::clamped_temperature(provider, t)
                )),
            }
        }
        "max_tokens" => unset_if(numeric_none("--max-tokens")),
        "max_turns" => unset_if(numeric_none("--max-turns")),
        "max_auto_continues" => match parse_max_auto_continues_from_config(&one) {
            None => Unset,
            Some(n) if value.parse::<u32>().ok() == Some(n) => AsWritten,
            Some(n) => As(n.to_string()),
        },
        "notify_command" => unset_if(parse_notify_command_from_config(&one).is_none()),
        "cost_warn_usd" => unset_if(
            crate::prompt_budget::parse_cost_threshold(
                parse_cost_warn_from_config(&one).as_deref(),
            )
            .is_none(),
        ),
        // Booleans go through `config_flag`: a value it does not recognise is
        // the one whose result depends on the default, i.e. the default is used.
        // A synonym it does recognise (`yes`, `1`, `on`) is named as true/false.
        "auto_watch"
        | "auto_edit"
        | "auto_commit"
        | "auto_continue"
        | "continue_on_silence"
        | "wait_for_reset"
        | "retry_after_partial"
        | "hints"
        | "lite"
        | "no_bell"
        | "quiet"
        | "no_color" => {
            let used = config_flag(&one, key, false);
            if used != config_flag(&one, key, true) {
                Unset
            } else if value == used.to_string() {
                AsWritten
            } else {
                As(used.to_string())
            }
        }
        _ => return None,
    })
}

/// The `provider` row of [`run_reading`]: startup lowercases the name, then
/// refuses a retired or unknown one unless a base URL is given (`--base-url`
/// or `base_url`, which this key alone cannot see, so the note says "unless").
fn provider_reading(value: &str) -> RunReading {
    let lower = value.to_lowercase();
    let unless = "startup refuses it unless a base URL is given";
    if crate::providers::retired_provider_refusal(&lower).is_some() {
        RunReading::As(format!("a retired provider; {unless}"))
    } else if crate::providers::unknown_provider_refusal(&lower, false).is_some() {
        RunReading::As(format!("an unknown provider; {unless}"))
    } else if lower != value {
        RunReading::As(lower)
    } else {
        RunReading::AsWritten
    }
}

/// `config show`'s marker for a value the run does not use as written (#1172,
/// #1195): "read as unset" or "read as <what it uses>", else empty.
pub(crate) fn unset_reading_note(key: &str, value: &str, provider: &str) -> String {
    match run_reading(key, value, provider) {
        Some(RunReading::Unset) => "  (read as unset; using default)".to_string(),
        Some(RunReading::As(used)) => format!("  (read as {used})"),
        _ => String::new(),
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

/// What `config get` with no key prints (#1154/#1155): the usage line in the
/// form `surface` can run, then the settable keys. The caller exits 2.
pub(crate) fn config_get_usage(surface: crate::commands_dev::DoctorSurface) -> String {
    let config = crate::commands_dev::command_form(surface, "config");
    format!(
        "{YELLOW}  usage: {config} get <key>{RESET}\n{DIM}  settable keys: {}{RESET}\n",
        settable_keys_list()
    )
}

/// The lost-settings hint for `config get <key>` (Day 253): one line per
/// skipped config file (`skipped`, from `config_paths::skipped_config_sources`)
/// that sets `key`, saying it is not read because `loaded` is found first.
/// yoyo reads one config file, first found wins, never merged, so a key set
/// only in a shadowed file reads as "not set" — true of the file read, and
/// silent about the file the user actually wrote it in. Plain text (no
/// colour) so the table can compare whole strings; the caller styles it.
/// `read` returns a file's contents, or `None` when it cannot be read (no
/// guess is made about an unreadable file). Each file is parsed by
/// `cli::parse_config_file`, the same rule `config get` applies to `loaded`.
pub(crate) fn shadowed_key_note(
    key: &str,
    loaded: &std::path::Path,
    skipped: &[std::path::PathBuf],
    read: impl Fn(&std::path::Path) -> Option<String>,
) -> Vec<String> {
    skipped
        .iter()
        .filter_map(|path| {
            let value = crate::cli::parse_config_file(&read(path)?).remove(key)?;
            let shown = if is_masked_key(key) {
                "***".to_string()
            } else {
                value
            };
            Some(format!(
                "note: {} sets {key} = {shown}, but it is not read: {} is found first",
                path.display(),
                loaded.display()
            ))
        })
        .collect()
}

/// [`shadowed_key_note`] over the real disk, styled for printing: empty unless
/// `key` is absent from the loaded file (`config`) and some skipped file sets
/// it. Reads skipped files with `read_regular_to_string`, which refuses a FIFO
/// rather than blocking on it and prints nothing (#1047).
pub(crate) fn shadowed_key_note_on_disk(
    key: &str,
    config: &std::collections::HashMap<String, String>,
    loaded: Option<&std::path::Path>,
) -> String {
    let Some(loaded) = loaded.filter(|_| !config.contains_key(key)) else {
        return String::new();
    };
    let chain = crate::config_paths::existing_config_paths();
    let skipped = crate::config_paths::skipped_config_sources(&chain, Some(loaded));
    shadowed_key_note(key, loaded, &skipped, |p| {
        crate::non_regular_read::read_regular_to_string(p).ok()
    })
    .iter()
    .map(|line| format!("  {YELLOW}{line}{RESET}\n"))
    .collect()
}

/// The `CONFIG_SUBCOMMANDS` that `yoyo config <sub>` runs from the shell, each
/// in the form a shell user types. Every other member needs the REPL. The
/// dispatch arm in `dispatch_sub.rs` matches exactly these words; a test pins
/// both directions.
pub(crate) const CONFIG_SHELL_FORMS: &[(&str, &str)] = &[
    ("show", "yoyo config show"),
    ("get", "yoyo config get <key>"),
];

/// What `yoyo config <word>` prints when `word` does not run from the shell,
/// as (headline, hint), unstyled. A real REPL-only subcommand (`set`, `edit`)
/// says it needs an interactive session; any other word is unknown, with a
/// did-you-mean from `CONFIG_SUBCOMMANDS`, because claiming the REPL would run
/// it is false. Both hints list only commands that work from the shell. The
/// caller exits 2 either way (#982).
pub(crate) fn config_shell_refusal(word: &str) -> (String, String) {
    let shell: Vec<&str> = CONFIG_SHELL_FORMS.iter().map(|(_, form)| *form).collect();
    let try_line = format!("Try: {} (work from the shell)", shell.join(", "));
    if crate::commands::CONFIG_SUBCOMMANDS.contains(&word) {
        return (
            format!("`config {word}` requires an interactive session: start yoyo and use /config {word}"),
            try_line,
        );
    }
    let mut headline = format!("unknown config subcommand: `{word}`");
    if let Some(near) = crate::commands::closest_match(word, crate::commands::CONFIG_SUBCOMMANDS, 2)
    {
        headline.push_str(&format!(" (did you mean `{near}`?)"));
    }
    let repl: Vec<String> = crate::commands::CONFIG_SUBCOMMANDS
        .iter()
        .filter(|sub| !CONFIG_SHELL_FORMS.iter().any(|(s, _)| s == *sub))
        .map(|sub| format!("/config {sub}"))
        .collect();
    (
        headline,
        format!("{try_line}; inside yoyo: {}", repl.join(", ")),
    )
}
