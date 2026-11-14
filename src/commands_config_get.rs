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
        // #1172: the run reads this value as if the key were absent; say so.
        Some(value) if run_reads_as_unset(key, value) == Some(true) => {
            let shown = if is_secret_key(key) && !value.trim().is_empty() {
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

/// #1172: `Some(true)` when the run reads `value` for `key` exactly as it reads
/// the key being absent (the default is used), `Some(false)` when it reads the
/// value, `None` for a key that is not settable. Every arm asks the run's own
/// reader; none restates its rule. Not unset: `provider` ("" is refused as an
/// unknown provider) and `thinking` ("" warns and runs at medium).
pub(crate) fn run_reads_as_unset(key: &str, value: &str) -> Option<bool> {
    use crate::config::*;
    let one: std::collections::HashMap<String, String> =
        [(key.to_string(), value.to_string())].into();
    let numeric_none = |flag: &str| match key {
        "temperature" => crate::cli_config::resolve_numeric_setting::<f32>(&[], flag, &one, key)
            .0
            .is_none(),
        "max_turns" => crate::cli_config::resolve_numeric_setting::<usize>(&[], flag, &one, key)
            .0
            .is_none(),
        _ => crate::cli_config::resolve_numeric_setting::<u32>(&[], flag, &one, key)
            .0
            .is_none(),
    };
    Some(match key {
        "model" => crate::config_value::named_model(value).is_none(),
        "provider" | "thinking" => false,
        "temperature" => numeric_none("--temperature"),
        "max_tokens" => numeric_none("--max-tokens"),
        "max_turns" => numeric_none("--max-turns"),
        "max_auto_continues" => parse_max_auto_continues_from_config(&one).is_none(),
        "notify_command" => parse_notify_command_from_config(&one).is_none(),
        "cost_warn_usd" => {
            crate::prompt_budget::parse_cost_threshold(parse_cost_warn_from_config(&one).as_deref())
                .is_none()
        }
        // Booleans go through `config_flag`: a value it does not recognise is
        // the one whose result depends on the default, i.e. the default is used.
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
        | "no_color" => config_flag(&one, key, false) != config_flag(&one, key, true),
        _ => return None,
    })
}

/// `config show`'s marker for a value the run reads as unset (#1172).
pub(crate) fn unset_reading_note(key: &str, value: &str) -> &'static str {
    if run_reads_as_unset(key, value) == Some(true) {
        "  (read as unset; using default)"
    } else {
        ""
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
            let shown = if is_secret_key(key) {
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
