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
    use crate::commands_config_get::run_reads_as_unset;
    for (key, _) in crate::config::SETTABLE_KEYS {
        assert!(run_reads_as_unset(key, "").is_some(), "{key} unclassified");
    }
    assert_eq!(run_reads_as_unset("not_a_key", ""), None);
}

/// `key = ""` for every settable key: the full printed line and exit code.
/// provider and thinking are the two the run does NOT read as unset
/// (provider "" is refused as unknown; thinking "" warns and uses medium).
#[test]
fn empty_value_rows_say_what_the_run_does() {
    let src = "/x/.yoyo.toml";
    for (key, _) in crate::config::SETTABLE_KEYS {
        let mut cfg = std::collections::HashMap::new();
        cfg.insert(key.to_string(), String::new());
        let (code, text) = crate::commands_config_get::config_get_report(key, &cfg, src);
        assert_eq!(code, 0, "{key}");
        let expected = if matches!(*key, "provider" | "thinking") {
            format!("\x1b[2m  {key} =   ({src})\x1b[0m\n")
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
    // (max_turns, not max_tokens: `is_secret_key` masks max_tokens, #1194.)
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
