//! Tests for `config_get_unknown_key_message` (`yoyo config get <typo>`).
//! Lives in its own file because `commands_config.rs` sits at the module-size
//! gate's overshoot band.

use crate::commands_config_get::config_get_unknown_key_message;

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

// === shadowed_key_note (Day 253): `config get <key>` names a skipped file that sets it ===

/// A reader over an in-memory map of path -> file contents, so the table never
/// touches the real home directory.
fn reader<'a>(files: &'a [(&'a str, &'a str)]) -> impl Fn(&std::path::Path) -> Option<String> + 'a {
    move |p| {
        files
            .iter()
            .find(|(path, _)| std::path::Path::new(path) == p)
            .map(|(_, body)| body.to_string())
    }
}

#[test]
fn shadowed_key_note_table() {
    use crate::commands_config_get::shadowed_key_note;
    use std::path::{Path, PathBuf};
    let home = Path::new("/h/.yoyo.toml");
    let xdg = PathBuf::from("/h/.config/yoyo/config.toml");
    let files = [
        ("/h/.yoyo.toml", "model = \"m\"\n"),
        ("/h/.config/yoyo/config.toml", "thinking = \"low\"\n"),
    ];
    // (a) set only in the skipped XDG file: the note names it, its value, and the file read first.
    assert_eq!(
        shadowed_key_note("thinking", home, std::slice::from_ref(&xdg), reader(&files)),
        vec![
            "note: /h/.config/yoyo/config.toml sets thinking = low, but it is not read: \
             /h/.yoyo.toml is found first"
                .to_string()
        ]
    );
    // (b) set in the read file and nowhere skipped: no note (the caller only asks
    // when the key is absent from the read file, but the helper must not invent one).
    assert!(
        shadowed_key_note("model", home, std::slice::from_ref(&xdg), reader(&files)).is_empty()
    );
    // (c) set nowhere: no note.
    assert!(
        shadowed_key_note("provider", home, std::slice::from_ref(&xdg), reader(&files)).is_empty()
    );
    // (d) project shadowing home, where home sets the key: the note names the home file.
    let project = Path::new(".yoyo.toml");
    let files_d = [
        (".yoyo.toml", ""),
        ("/h/.yoyo.toml", "thinking = \"high\"\n"),
    ];
    assert_eq!(
        shadowed_key_note(
            "thinking",
            project,
            &[PathBuf::from("/h/.yoyo.toml")],
            reader(&files_d)
        ),
        vec![
            "note: /h/.yoyo.toml sets thinking = high, but it is not read: .yoyo.toml is found first"
                .to_string()
        ]
    );
    // An unreadable skipped file contributes nothing rather than a guess.
    assert!(
        shadowed_key_note("thinking", home, &[PathBuf::from("/gone")], reader(&files)).is_empty()
    );
    // A secret set in a skipped file is named but its value is not printed.
    let files_s = [("/h/.config/yoyo/config.toml", "api_key = \"sk-secret\"\n")];
    let note = shadowed_key_note(
        "api_key",
        home,
        std::slice::from_ref(&xdg),
        reader(&files_s),
    );
    assert_eq!(
        note,
        vec![
            "note: /h/.config/yoyo/config.toml sets api_key = ***, but it is not read: \
             /h/.yoyo.toml is found first"
                .to_string()
        ]
    );
}

// `yoyo config <word>` refusals: unknown words say unknown, REPL-only
// subcommands say interactive, and every suggested shell command runs.

use crate::commands_config_get::{config_shell_refusal, CONFIG_SHELL_FORMS};

const SHELL_TRY: &str = "Try: yoyo config show, yoyo config get <key> (work from the shell)";

#[test]
fn unknown_config_word_is_named_unknown_not_interactive() {
    let (headline, hint) = config_shell_refusal("bogus");
    assert_eq!(headline, "unknown config subcommand: `bogus`");
    assert_eq!(
        hint,
        format!("{SHELL_TRY}; inside yoyo: /config edit, /config set")
    );
    assert!(!headline.contains("interactive"), "{headline}");
}

#[test]
fn typo_of_real_config_subcommand_suggests_it() {
    let (headline, hint) = config_shell_refusal("shwo");
    assert_eq!(
        headline,
        "unknown config subcommand: `shwo` (did you mean `show`?)"
    );
    assert_eq!(
        hint,
        format!("{SHELL_TRY}; inside yoyo: /config edit, /config set")
    );
}

#[test]
fn repl_only_config_subcommands_keep_interactive_wording() {
    for sub in ["set", "edit"] {
        let (headline, hint) = config_shell_refusal(sub);
        assert_eq!(
            headline,
            format!(
                "`config {sub}` requires an interactive session: start yoyo and use /config {sub}"
            )
        );
        assert_eq!(hint, SHELL_TRY);
    }
}

/// The shell forms are a subset of the authority, and the dispatch arm runs
/// exactly them: each `yoyo config <x>` a refusal prints is a word the shell
/// arm handles (a round trip), and every other CONFIG_SUBCOMMANDS member is
/// refused with the interactive wording, never as unknown.
#[test]
fn suggested_config_commands_round_trip_to_the_shell_arm() {
    let src = include_str!("dispatch_sub.rs");
    let arm = src.find(concat!("\"config", "\" =>")).expect("config arm");
    let end = src[arm..]
        .find(concat!("\"permissions", "\" =>"))
        .expect("next arm");
    let body = &src[arm..arm + end];
    for (sub, form) in CONFIG_SHELL_FORMS {
        assert!(crate::commands::CONFIG_SUBCOMMANDS.contains(sub), "{sub}");
        assert!(
            body.contains(&format!("Some(\"{sub}\")")),
            "arm misses {sub}"
        );
        let words: Vec<&str> = form.split_whitespace().collect();
        assert_eq!(&words[..3], ["yoyo", "config", *sub], "{form}");
        assert!(SHELL_TRY.contains(form), "{form}");
    }
    for sub in crate::commands::CONFIG_SUBCOMMANDS {
        let on_shell = CONFIG_SHELL_FORMS.iter().any(|(s, _)| s == sub);
        assert_eq!(
            body.contains(&format!("Some(\"{sub}\")")),
            on_shell,
            "{sub}"
        );
        if !on_shell {
            assert!(config_shell_refusal(sub).0.contains("interactive session"));
        }
    }
}

// === #1172: `config get` says what the run does with a value it reads as unset ===

/// Every settable key is classified, so a new key cannot silently fall back to
/// "show the raw value" while the run ignores it. Iterates the authority.
#[test]
fn every_settable_key_has_an_unset_reading() {
    use crate::commands_config_get::run_reading;
    for (key, _) in crate::config::SETTABLE_KEYS {
        assert!(
            run_reading(key, "", "anthropic").is_some(),
            "{key} unclassified"
        );
    }
    assert_eq!(run_reading("not_a_key", "", "anthropic"), None);
}

/// `key = ""` for every settable key: the full printed line and exit code.
/// provider and thinking are the two the run does NOT read as unset
/// (provider "" is refused as unknown; thinking "" warns and uses medium),
/// and since #1195 the line says what the run does instead.
#[test]
fn empty_value_rows_say_what_the_run_does() {
    let src = "/x/.yoyo.toml";
    for (key, _) in crate::config::SETTABLE_KEYS {
        let mut cfg = std::collections::HashMap::new();
        cfg.insert(key.to_string(), String::new());
        let (code, text) = crate::commands_config_get::config_get_report(key, &cfg, src);
        assert_eq!(code, 0, "{key}");
        let expected = if *key == "provider" {
            format!("\x1b[2m  {key} =   ({src})  (read as an unknown provider; startup refuses it unless a base URL is given)\x1b[0m\n")
        } else if *key == "thinking" {
            format!("\x1b[2m  {key} =   ({src})  (read as medium)\x1b[0m\n")
        } else {
            format!(
                "\x1b[2m  {key} is not set: {src} has {key} = \"\", which yoyo reads as unset (using default)\x1b[0m\n"
            )
        };
        assert_eq!(strip_dim(&text), strip_dim(&expected), "{key}");
    }
}

/// The colour statics may be empty (no-color); compare without them.
fn strip_dim(s: &str) -> String {
    s.replace("\x1b[2m", "").replace("\x1b[0m", "")
}

/// Near misses: a real value, a value equal to the default, whitespace, garbage.
#[test]
fn unset_reading_near_misses() {
    let src = "/x/.yoyo.toml";
    let run = |key: &str, value: &str| {
        let mut cfg = std::collections::HashMap::new();
        cfg.insert(key.to_string(), value.to_string());
        let (code, text) = crate::commands_config_get::config_get_report(key, &cfg, src);
        (code, strip_dim(&text))
    };
    // A real model is shown as today.
    assert_eq!(run("model", "x"), (0, format!("  model = x  ({src})\n")));
    // Whitespace: the run trims the model, so it is unset too.
    assert_eq!(
        run("model", " "),
        (
            0,
            format!("  model is not set: {src} has model = \" \", which yoyo reads as unset (using default)\n")
        )
    );
    // A boolean equal to its default is still a value the run reads.
    assert_eq!(
        run("auto_continue", "true"),
        (0, format!("  auto_continue = true  ({src})\n"))
    );
    // A boolean the run does not recognise falls back to the default.
    assert_eq!(
        run("auto_edit", "maybe"),
        (
            0,
            format!("  auto_edit is not set: {src} has auto_edit = \"maybe\", which yoyo reads as unset (using default)\n")
        )
    );
    // A numeric that parses is shown; one that doesn't is unset.
    assert_eq!(
        run("max_turns", "20"),
        (0, format!("  max_turns = 20  ({src})\n"))
    );
    assert!(run("max_turns", "abc").1.contains("max_turns is not set"));
    // Absent: byte-for-byte as before.
    let empty = std::collections::HashMap::new();
    let (code, text) = crate::commands_config_get::config_get_report("model", &empty, src);
    assert_eq!(
        (code, strip_dim(&text)),
        (
            0,
            "  model is not set in config file (using default)\n".to_string()
        )
    );
}

/// `config show` lists the key and marks it the same way.
#[test]
fn config_show_marks_a_value_read_as_unset() {
    let mut cfg = std::collections::HashMap::new();
    cfg.insert("model".to_string(), String::new());
    cfg.insert("provider".to_string(), "openai".to_string());
    let out = crate::commands_config::format_config_output(
        &cfg,
        Some(std::path::Path::new("/x/.yoyo.toml")),
    );
    assert!(
        out.contains("  model     =   (read as unset; using default)\n"),
        "{out}"
    );
    assert!(out.contains("  provider  = openai\n"), "{out}");
}

// === #1195: `config get` / `config show` name the value the run uses ===

/// The census: every settable key (iterated from `SETTABLE_KEYS`, not typed)
/// with an unusual value, and what the run does with it. A key missing from
/// `rows` fails, so a new key must be read before it ships. Each row's value
/// was chosen to hit the key's reader at a boundary (clamp, fold, synonym).
/// No key is exempt: `max_tokens` above a model's ceiling is warned about at
/// build time but sent as written (agent_builder `max_tokens_ceiling_warning`),
/// so "as written" is the run's reading, not a default.
#[test]
fn run_reading_census_over_every_settable_key() {
    use crate::commands_config_get::{run_reading, RunReading::*};
    let as_ = |s: &str| As(s.to_string());
    let rows = [
        ("model", " ", Unset),
        ("provider", "OpenAI", as_("openai")),
        ("thinking", "max", as_("high")),
        ("temperature", "5", as_("1.0")),
        ("max_tokens", "999999999", AsWritten),
        ("max_turns", "0", AsWritten),
        ("auto_watch", "yes", as_("true")),
        ("auto_edit", "1", as_("true")),
        ("auto_commit", "off", as_("false")),
        ("auto_continue", "no", as_("false")),
        ("max_auto_continues", "50", as_("20")),
        ("continue_on_silence", "on", as_("true")),
        ("wait_for_reset", "0", as_("false")),
        ("retry_after_partial", "True", Unset),
        ("hints", "false", AsWritten),
        ("lite", "true", AsWritten),
        ("no_bell", "yes", as_("true")),
        ("quiet", "maybe", Unset),
        ("no_color", "1", as_("true")),
        ("notify_command", "  ", Unset),
        ("cost_warn_usd", "-1", Unset),
    ];
    for (key, _) in crate::config::SETTABLE_KEYS {
        let row = rows.iter().find(|(k, _, _)| k == key);
        let (_, value, expected) = row.unwrap_or_else(|| panic!("census has no row for {key}"));
        assert_eq!(
            run_reading(key, value, "anthropic").as_ref(),
            Some(expected),
            "{key} = {value:?}"
        );
    }
}

/// The ticket's own rows: thinking and temperature, each side of the change.
#[test]
fn run_reading_thinking_and_temperature_rows() {
    use crate::commands_config_get::{run_reading, RunReading::*};
    let as_ = |s: &str| Some(As(s.to_string()));
    for (value, expected) in [
        ("", as_("medium")),
        ("max", as_("high")),
        ("bogus", as_("medium")),
        ("med", as_("medium")),
        ("high", Some(AsWritten)),
        ("High", Some(AsWritten)),
        ("off", Some(AsWritten)),
        ("none", as_("off")),
    ] {
        assert_eq!(
            run_reading("thinking", value, "anthropic"),
            expected,
            "thinking {value:?}"
        );
    }
    for (value, expected) in [
        ("5", as_("1.0")),
        ("-1", as_("0.0")),
        ("0.7", Some(AsWritten)),
        ("1", Some(AsWritten)),
        ("hot", Some(Unset)),
    ] {
        assert_eq!(
            run_reading("temperature", value, "anthropic"),
            expected,
            "temperature {value:?}"
        );
    }
    // #1207: the bound is the provider's; openai documents 0–2.
    for (value, expected) in [
        ("1.5", Some(AsWritten)),
        ("2", Some(AsWritten)),
        ("2.5", as_("2.0")),
        ("5", as_("2.0")),
    ] {
        assert_eq!(
            run_reading("temperature", value, "openai"),
            expected,
            "openai temperature {value:?}"
        );
    }
    assert_eq!(run_reading("temperature", "1.5", "anthropic"), as_("1.0"));
    // Provider: the startup refusals are the run's reading.
    assert_eq!(
        run_reading("provider", "github", "anthropic"),
        as_("a retired provider; startup refuses it unless a base URL is given")
    );
    assert_eq!(
        run_reading("provider", "bogus", "anthropic"),
        as_("an unknown provider; startup refuses it unless a base URL is given")
    );
    assert_eq!(
        run_reading("provider", "openai", "anthropic"),
        Some(AsWritten)
    );
}

/// The printed lines for the ticket's two motivating configs, both doors.
#[test]
fn config_get_and_show_print_the_value_the_run_uses() {
    let src = "/x/.yoyo.toml";
    let mut cfg = std::collections::HashMap::new();
    cfg.insert("thinking".to_string(), String::new());
    cfg.insert("temperature".to_string(), "5".to_string());
    let get =
        |key: &str| strip_dim(&crate::commands_config_get::config_get_report(key, &cfg, src).1);
    assert_eq!(
        get("thinking"),
        format!("  thinking =   ({src})  (read as medium)\n")
    );
    assert_eq!(
        get("temperature"),
        format!("  temperature = 5  ({src})  (read as 1.0)\n")
    );
    let out = crate::commands_config::format_config_output(&cfg, Some(std::path::Path::new(src)));
    assert!(out.contains("  temperature  = 5  (read as 1.0)\n"), "{out}");
    assert!(
        out.contains("  thinking     =   (read as medium)\n"),
        "{out}"
    );
}

// === #1194: `is_secret_key` matches name segments, not substrings ===

/// Census over every key yoyo knows, from the two authorities (not typed):
/// the only one masked is `api_key`. `max_tokens` used to be masked too,
/// because its name contains `token` (#1194).
#[test]
fn only_api_key_is_masked_among_every_known_key() {
    use crate::config_secret::is_secret_key;
    let mut all: Vec<&str> = crate::config::SETTABLE_KEYS
        .iter()
        .map(|(k, _)| *k)
        .chain(crate::config_unknown_keys::TOP_LEVEL_KEYS.iter().copied())
        .collect();
    all.sort_unstable();
    all.dedup();
    assert!(all.contains(&"max_tokens"), "census lost max_tokens");
    let masked: Vec<&str> = all.iter().copied().filter(|k| is_secret_key(k)).collect();
    assert_eq!(masked, vec!["api_key"], "probed {} known keys", all.len());
}

#[test]
fn secret_key_rows_mask_credentials_and_show_counts() {
    use crate::config_secret::is_secret_key;
    for key in [
        "api_key",
        "apikey",
        "API_KEY",
        "GITHUB_TOKEN",
        "access_token",
        "AccessToken",
        "githubtoken",
        "client_secret",
        "db_password",
        "x-api-key",
        "aws.secret_access_key",
        "api_keys",
        "credentials",
    ] {
        assert!(is_secret_key(key), "{key} must be masked");
    }
    // #1197: yoyo reads none of these, so their value could be anything the
    // user typed; the pre-#1194 rule masked them and so does the current one.
    for key in ["MaxTokens", "max_output_tokens", "keymap"] {
        assert!(is_secret_key(key), "{key} is unknown, must be masked");
    }
    for key in ["max_tokens", "thinking", "model", "temperature"] {
        assert!(!is_secret_key(key), "{key} must be shown");
    }
}

/// The report path a user actually runs: `yoyo config get max_tokens`.
#[test]
fn config_get_max_tokens_prints_the_value() {
    let src = "/x/.yoyo.toml";
    let mut cfg = std::collections::HashMap::new();
    cfg.insert("max_tokens".to_string(), "2048".to_string());
    let (code, text) = crate::commands_config_get::config_get_report("max_tokens", &cfg, src);
    assert_eq!(
        (code, strip_dim(&text)),
        (0, format!("  max_tokens = 2048  ({src})\n"))
    );
    // And a real secret on the same path stays masked.
    let mut cfg = std::collections::HashMap::new();
    cfg.insert("api_key".to_string(), "sk-real".to_string());
    let (_, text) = crate::commands_config_get::config_get_report("api_key", &cfg, src);
    assert_eq!(strip_dim(&text), format!("  api_key = ***  ({src})\n"));
}

// === #1197: every name the pre-#1194 rule masked is masked again ===

/// The rule before 18296f08 (`git show 18296f08 -- src/commands_config.rs`),
/// copied as the reference for the differential: it is the old behaviour,
/// not an outside authority.
fn pre_1194_rule(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    ["key", "token", "secret", "password"]
        .iter()
        .any(|w| lower.contains(w))
}

/// The #1194 segment rule as it shipped in 18296f08, copied the same way.
fn rule_1194(key: &str) -> bool {
    const ENDINGS: &[&str] = &[
        "key",
        "keys",
        "token",
        "secret",
        "secrets",
        "password",
        "passwd",
        "credential",
        "credentials",
    ];
    let lower = key.to_ascii_lowercase();
    ["secret", "password", "apikey"]
        .iter()
        .any(|w| lower.contains(w))
        || lower
            .split(['_', '-', '.'])
            .any(|seg| ENDINGS.iter().any(|e| seg.ends_with(e)))
}

/// Generated names: credential word x prefix x suffix x case. Includes the
/// no-credential-word families #1197 called pre-existing (bearer, pat,
/// cookie, auth).
fn secret_name_corpus() -> Vec<String> {
    let words = [
        "token",
        "tokens",
        "key",
        "keys",
        "secret",
        "secrets",
        "password",
        "passwd",
        "apikey",
        "credential",
        "credentials",
        "bearer",
        "pat",
        "cookie",
        "auth",
    ];
    let prefixes = ["", "github_", "slack-", "my.", "X_", "max_"];
    let suffixes = ["", "2", "s", "value", "_old", "-backup"];
    let mut out = Vec::new();
    for w in words {
        for p in prefixes {
            for s in suffixes {
                let name = format!("{p}{w}{s}");
                out.push(name.to_ascii_uppercase());
                out.push(name);
            }
        }
    }
    out
}

#[test]
fn every_name_an_earlier_rule_masked_is_still_masked_unless_a_known_exemption() {
    use crate::config_secret::{is_secret_key, SHOWN_KNOWN_KEYS};
    let corpus = secret_name_corpus();
    let exempt = |k: &str| SHOWN_KNOWN_KEYS.iter().any(|(e, _)| *e == k);
    let leaked: Vec<&String> = corpus
        .iter()
        .filter(|k| (pre_1194_rule(k) || rule_1194(k)) && !exempt(k) && !is_secret_key(k))
        .collect();
    let old_masked = corpus
        .iter()
        .filter(|k| pre_1194_rule(k) || rule_1194(k))
        .count();
    assert!(
        leaked.is_empty(),
        "{} of {} names an earlier rule masked now print raw ({} names checked): {leaked:?}",
        leaked.len(),
        old_masked,
        corpus.len()
    );
    // The families #1197 called pre-existing are masked too now.
    for k in [
        "auth_bearer",
        "openai_pat",
        "session_cookie",
        "github.pat",
        "oauth",
    ] {
        assert!(is_secret_key(k), "{k} must be masked");
    }
    // `pat` is a whole segment only: these are not credentials.
    for k in ["path", "pattern", "system_file"] {
        assert!(!is_secret_key(k), "{k} must be shown");
    }
}

#[test]
fn issue_1197_repro_names_are_masked_and_max_tokens_is_not() {
    use crate::config_secret::is_secret_key;
    for k in ["github_token2", "slack_tokens", "tokenvalue"] {
        assert!(is_secret_key(k), "{k} must be masked (#1197)");
    }
    assert!(!is_secret_key("max_tokens"), "#1194 near-miss");
}

/// Exemptions are known yoyo keys only, each with a reason, and each one is
/// load-bearing (the word rule alone would mask it).
#[test]
fn shown_known_keys_are_known_and_each_exempts_something() {
    use crate::config_secret::SHOWN_KNOWN_KEYS;
    for (k, why) in SHOWN_KNOWN_KEYS {
        let known = crate::config::SETTABLE_KEYS.iter().any(|(s, _)| s == k)
            || crate::config_unknown_keys::TOP_LEVEL_KEYS.contains(k);
        assert!(known, "{k} is not a key yoyo reads; it may not be exempt");
        assert!(!why.trim().is_empty(), "{k} needs a reason");
        assert!(pre_1194_rule(k), "{k} exempts nothing");
    }
}

/// End to end through `config show`'s formatter: the raw value never prints.
#[test]
fn config_show_masks_github_token2_value() {
    let mut cfg = std::collections::HashMap::new();
    cfg.insert("github_token2".to_string(), "ghp_LEAK1".to_string());
    cfg.insert("slack_tokens".to_string(), "xoxb_LEAK2".to_string());
    cfg.insert("max_tokens".to_string(), "2048".to_string());
    let path = std::path::PathBuf::from("/x/.yoyo.toml");
    let out = crate::commands_config::format_config_output(&cfg, Some(&path));
    assert!(!out.contains("ghp_LEAK1"), "{out}");
    assert!(!out.contains("xoxb_LEAK2"), "{out}");
    assert!(out.contains("***"), "{out}");
    assert!(out.contains("2048"), "{out}");
    // And `config get` on the same key.
    let (_, text) =
        crate::commands_config_get::config_get_report("github_token2", &cfg, "/x/.yoyo.toml");
    assert!(!text.contains("ghp_LEAK1"), "{text}");
    assert!(text.contains("***"), "{text}");
}

// === #1199: a key yoyo does not read has its value masked (option A) ===

/// The #1199 file: known keys, a known secret, three unknown keys (two
/// credentials with no credential word, one typo) and an unknown key under
/// `[permissions]`.
const UNKNOWN_KEYS_FILE: &str = "model = \"claude-x\"\n\
max_tokens = 8000\n\
api_key = \"sk-abc\"\n\
openai = \"sk-abc\"\n\
dsn = \"postgres://u:pw@h/db\"\n\
modle = \"claude-x\"\n\
[permissions]\n\
allw = [\"sk-sec\"]\n";

/// `config show` on that file, line by line: known keys print raw, every
/// other value is `***`, and the section key (not in the flat map) never prints.
#[test]
fn config_show_masks_every_unknown_keys_value() {
    let cfg = crate::cli::parse_config_file(UNKNOWN_KEYS_FILE);
    let out = crate::commands_config::format_config_output(
        &cfg,
        Some(std::path::Path::new("/x/.yoyo.toml")),
    );
    assert_eq!(
        out,
        "Loaded config: /x/.yoyo.toml\n\n\
         \x20 api_key     = ***\n\
         \x20 dsn         = ***\n\
         \x20 max_tokens  = 8000\n\
         \x20 model       = claude-x\n\
         \x20 modle       = ***\n\
         \x20 openai      = ***\n"
    );
    assert!(!out.contains("sk-sec") && !out.contains("allw"), "{out}");
}

/// `config get` on the same keys: the unknown ones print `***`.
#[test]
fn config_get_masks_an_unknown_keys_value() {
    let cfg = crate::cli::parse_config_file(UNKNOWN_KEYS_FILE);
    let src = "/x/.yoyo.toml";
    let get = |k: &str| strip_dim(&crate::commands_config_get::config_get_report(k, &cfg, src).1);
    assert_eq!(get("openai"), format!("  openai = ***  ({src})\n"));
    assert_eq!(get("dsn"), format!("  dsn = ***  ({src})\n"));
    assert_eq!(get("modle"), format!("  modle = ***  ({src})\n"));
    assert_eq!(get("model"), format!("  model = claude-x  ({src})\n"));
    assert_eq!(get("max_tokens"), format!("  max_tokens = 8000  ({src})\n"));
    assert_eq!(get("api_key"), format!("  api_key = ***  ({src})\n"));
}

/// The warning still names each unknown key (and the typo's remedy), so
/// masking the value loses nothing a typo needs.
#[test]
fn unknown_key_warnings_survive_the_mask() {
    use crate::config_unknown_keys::{unknown_config_key_warnings, unknown_config_keys};
    let warnings =
        unknown_config_key_warnings("f", &unknown_config_keys(UNKNOWN_KEYS_FILE)).join("\n");
    assert_eq!(
        warnings,
        "f line 4: `openai` is not a key yoyo reads, so it has no effect\n\
         f line 5: `dsn` is not a key yoyo reads, so it has no effect\n\
         f line 6: `modle` is not a key yoyo reads, so it has no effect; did you mean `model`?\n\
         f line 8: `allw` (in [permissions]) is not a key yoyo reads, so it has no effect; did you mean `allow`?"
    );
}

/// The display and the warning share one authority: every flat-map key the
/// warning names is masked, and every key it is silent on is masked only if
/// its name is a credential's.
#[test]
fn masked_for_being_unknown_iff_the_warning_names_it() {
    use crate::config_secret::{is_masked_key, is_secret_key};
    use crate::config_unknown_keys::{unknown_config_key_warnings, unknown_config_keys};
    let file = UNKNOWN_KEYS_FILE.to_string();
    // Every top-level key yoyo reads, plus a hook, set at top level.
    let mut top: String = crate::config_unknown_keys::TOP_LEVEL_KEYS
        .iter()
        .map(|k| format!("{k} = \"v\"\n"))
        .collect();
    top.push_str("stripe_sk = \"sk_live_x\"\nhooks.pre.bash = \"echo\"\n");
    for contents in [top, file] {
        let warnings = unknown_config_key_warnings("f", &unknown_config_keys(&contents)).join("\n");
        let cfg = crate::cli::parse_config_file(&contents);
        assert!(cfg.len() > 5, "anti-vacuous: {cfg:?}");
        for key in cfg.keys() {
            // The finding's subject, not a did-you-mean naming it.
            let warned = warnings.contains(&format!(": `{key}`"));
            assert_eq!(
                is_masked_key(key),
                warned || is_secret_key(key),
                "{key}: masked and warned disagree\n{warnings}"
            );
        }
    }
}

/// #1207: `config get temperature` reads the bound of the file's own provider
/// (absent = the run's default, anthropic).
#[test]
fn config_get_temperature_reads_the_files_provider() {
    use crate::commands_config_get::config_get_report;
    let file = |pairs: &[(&str, &str)]| -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    let openai = file(&[("provider", "openai"), ("temperature", "1.5")]);
    let (_, text) = config_get_report("temperature", &openai, ".yoyo.toml");
    assert!(!text.contains("read as"), "{text}");
    for cfg in [
        file(&[("temperature", "1.5")]),
        file(&[("provider", "anthropic"), ("temperature", "1.5")]),
    ] {
        let (_, text) = config_get_report("temperature", &cfg, ".yoyo.toml");
        assert!(text.contains("(read as 1.0)"), "{text}");
    }
}
