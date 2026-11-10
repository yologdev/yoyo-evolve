//! Warn when a loaded config file has a key yoyo never reads (#1171).
//!
//! `parse_config_file` puts every `key = value` line into a flat map and the
//! readers look up the names they know, so a typo (`modle = "gpt-4o"`) was
//! simply never looked up: the run went ahead on the default model and said
//! nothing. This module names those keys, once per process, on stderr. It
//! **warns, never refuses**: a key from a newer yoyo, or one the user's own
//! tooling reads, must not break the run.
//!
//! "Never reads" is defined by the readers, not by `/config set`'s list:
//! - the flat map is read by name (`file_config.get("model")`, `config_flag`,
//!   the numeric settings, `hooks.<phase>.<tool>` by prefix). The map is
//!   flattened across sections, so these names are read wherever they appear;
//! - four section parsers read their own keys inside their own headers
//!   (`[permissions]`, `[directories]`, `[mcp_servers.<name>]`,
//!   `[model_pricing.<name>]`), using the same header rule they use:
//!   a line that starts with `[` and ends with `]`.
//!
//! The census of `TOP_LEVEL_KEYS` was taken from those readers (a grep of
//! `.get("…")` / `.contains_key("…")` / `config_flag(config, "…")` /
//! `&file_config, "…"` over non-test `src/`), and
//! `every_flat_map_reader_key_is_known` re-runs that grep so a new reader whose
//! key is missing here fails a test by name.

use crate::commands::closest_match;

/// Every top-level key some reader of the flat config map looks up.
pub(crate) const TOP_LEVEL_KEYS: &[&str] = &[
    "api_key",
    "audit",
    "auto_commit",
    "auto_continue",
    "auto_edit",
    "auto_watch",
    "base_url",
    "context_window",
    "continue_on_silence",
    "cost_warn_usd",
    "fallback",
    "hints",
    "lite",
    "max_auto_continues",
    "max_tokens",
    "max_turns",
    "mcp",
    "model",
    "no_bell",
    "no_color",
    "notify_command",
    "provider",
    "quiet",
    "retry_after_partial",
    "sub_agent_model",
    "system_file",
    "system_prompt",
    "temperature",
    "thinking",
    "wait_for_reset",
];

/// Hook keys are `hooks.<phase>.<tool>`; `hooks::parse_hooks_from_config`
/// owns judging their shape (and already warns on a malformed one).
const HOOK_KEY_PREFIX: &str = "hooks.";

/// The sections whose parsers read keys under their header: `(name, is a
/// `name<id>` prefix, keys read)`. Mirrors `parse_permissions_from_config`,
/// `parse_directories_from_config`, `parse_mcp_servers_from_config` and
/// `parse_model_pricing_from_config`.
const SECTIONS: &[(&str, bool, &[&str])] = &[
    ("permissions", false, &["allow", "deny"]),
    ("directories", false, &["allow", "deny"]),
    ("mcp_servers.", true, &["command", "args", "env"]),
    ("model_pricing.", true, &["input", "output"]),
];

/// One thing in a config file that no reader looks at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnknownKey {
    /// A key no reader looks up. `section` is the header it sits under (as
    /// written, without brackets), `None` at top level.
    Key {
        line: usize,
        key: String,
        section: Option<String>,
        suggestion: Option<String>,
    },
    /// A section no parser reads, with the keys under it that nothing reads
    /// either (a top-level name under it is still read: the map is flat).
    Section {
        line: usize,
        header: String,
        keys: Vec<String>,
        suggestion: Option<String>,
    },
}

/// Is `key` a flat-map name some reader looks up?
fn is_top_level_key(key: &str) -> bool {
    TOP_LEVEL_KEYS.contains(&key)
        || key
            .strip_prefix(HOOK_KEY_PREFIX)
            .is_some_and(|rest| !rest.is_empty())
}

/// The keys a section header's parser reads, or `None` if no parser reads it.
fn section_keys(section: &str) -> Option<&'static [&'static str]> {
    SECTIONS.iter().find_map(|&(name, is_prefix, keys)| {
        let hit = if is_prefix {
            section
                .strip_prefix(name)
                .is_some_and(|id| !id.trim().is_empty())
        } else {
            section == name
        };
        hit.then_some(keys)
    })
}

/// Edit budget for a did-you-mean: one edit for short names, two otherwise
/// (`modle` → `model` is two, a transposition).
fn max_edits(word: &str) -> usize {
    if word.chars().count() <= 4 {
        1
    } else {
        2
    }
}

/// A did-you-mean for an unread section header, rendered as a header.
fn section_suggestion(section: &str) -> Option<String> {
    let (stem, rest) = match section.split_once('.') {
        Some((s, r)) => (s, Some(r)),
        None => (section, None),
    };
    let stems = ["permissions", "directories", "mcp_servers", "model_pricing"];
    let best = closest_match(stem, &stems, max_edits(stem))?;
    Some(match (best, rest) {
        ("mcp_servers" | "model_pricing", Some(r)) if !r.trim().is_empty() => {
            format!("[{best}.{r}]")
        }
        ("mcp_servers" | "model_pricing", _) => format!("[{best}.<name>]"),
        _ => format!("[{best}]"),
    })
}

/// Net `[` minus `]` outside quoted strings on `text`, used to skip the
/// continuation lines of a multi-line array (their `"A=B",` items are not keys).
fn bracket_balance(text: &str) -> i32 {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '#' => break,
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            },
        }
    }
    depth
}

/// Every key (and section) in `contents` that no reader of the config file
/// looks at. Empty for a file that only uses keys yoyo reads.
pub(crate) fn unknown_config_keys(contents: &str) -> Vec<UnknownKey> {
    let mut out: Vec<UnknownKey> = Vec::new();
    // Current header (inner text) and whether a parser reads it.
    let mut section: Option<(String, Option<&'static [&'static str]>)> = None;
    // Index into `out` of the open unread-section finding, if any.
    let mut open_section: Option<usize> = None;
    let mut open_multiline: Option<&str> = None;
    let mut array_depth: i32 = 0;

    for (idx, raw) in contents.lines().enumerate() {
        let line = raw.trim();
        if let Some(delim) = open_multiline {
            if line.contains(delim) {
                open_multiline = None;
            }
            continue;
        }
        if array_depth > 0 {
            array_depth += bracket_balance(line);
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            // The section parsers' own rule: a header ends with `]`. Anything
            // else starting with `[` is config_syntax's business, not a key.
            if line.ends_with(']') {
                let inner = line
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string();
                let keys = section_keys(&inner);
                open_section = None;
                if keys.is_none() {
                    out.push(UnknownKey::Section {
                        line: idx + 1,
                        header: line.to_string(),
                        keys: Vec::new(),
                        suggestion: section_suggestion(&inner),
                    });
                    open_section = Some(out.len() - 1);
                }
                section = Some((inner, keys));
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if let Some(delim) = ["\"\"\"", "'''"]
            .into_iter()
            .find(|d| value.starts_with(d) && !value[d.len()..].contains(d))
        {
            open_multiline = Some(delim);
        } else if value.starts_with('[') {
            array_depth = bracket_balance(value).max(0);
        }
        if key.is_empty() || is_top_level_key(key) {
            continue;
        }
        match &section {
            None => out.push(UnknownKey::Key {
                line: idx + 1,
                key: key.to_string(),
                section: None,
                suggestion: closest_match(key, TOP_LEVEL_KEYS, max_edits(key)).map(String::from),
            }),
            Some((name, Some(keys))) => {
                if !keys.contains(&key) {
                    let suggestion = closest_match(key, keys, max_edits(key))
                        .or_else(|| closest_match(key, TOP_LEVEL_KEYS, max_edits(key)))
                        .map(String::from);
                    out.push(UnknownKey::Key {
                        line: idx + 1,
                        key: key.to_string(),
                        section: Some(name.clone()),
                        suggestion,
                    });
                }
            }
            Some((_, None)) => {
                if let Some(UnknownKey::Section { keys, .. }) =
                    open_section.and_then(|i| out.get_mut(i))
                {
                    keys.push(key.to_string());
                }
            }
        }
    }
    // An unread section with nothing unread under it costs the user nothing.
    out.retain(|f| !matches!(f, UnknownKey::Section { keys, .. } if keys.is_empty()));
    out
}

/// One warning line per finding, naming the file it came from (`shown`).
pub(crate) fn unknown_config_key_warnings(shown: &str, findings: &[UnknownKey]) -> Vec<String> {
    findings
        .iter()
        .map(|f| match f {
            UnknownKey::Key {
                line,
                key,
                section,
                suggestion,
            } => {
                let place = match section {
                    Some(s) => format!(" (in [{s}])"),
                    None => String::new(),
                };
                let hint = match suggestion {
                    Some(s) => format!("; did you mean `{s}`?"),
                    None => String::new(),
                };
                format!(
                    "{shown} line {line}: `{key}`{place} is not a key yoyo reads, so it has no effect{hint}"
                )
            }
            UnknownKey::Section {
                line,
                header,
                keys,
                suggestion,
            } => {
                let hint = match suggestion {
                    Some(s) => format!("; did you mean `{s}`?"),
                    None => String::new(),
                };
                format!(
                    "{shown} line {line}: `{header}` is not a section yoyo reads, so its keys ({}) have no effect{hint}",
                    keys.join(", ")
                )
            }
        })
        .collect()
}

/// Set once the warnings have printed: `load_config_file` runs several times
/// per process (argument parsing, subcommands, the REPL) on the same file.
static UNKNOWN_KEYS_WARNED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Print the unknown-key warnings for `contents` (read from `shown`) to
/// stderr, at most once per process. Not quiet-gated, like the #1034 syntax
/// warning: a key that silently does nothing is a fault, not chatter.
pub(crate) fn warn_unknown_config_keys_once(shown: &str, contents: &str) {
    let warnings = unknown_config_key_warnings(shown, &unknown_config_keys(contents));
    if warnings.is_empty() || UNKNOWN_KEYS_WARNED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    use crate::format::{RESET, YELLOW};
    for w in warnings {
        eprintln!("{YELLOW}warning:{RESET} {w}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(contents: &str) -> Vec<String> {
        unknown_config_keys(contents)
            .into_iter()
            .map(|f| match f {
                UnknownKey::Key { key, .. } => key,
                UnknownKey::Section { header, .. } => header,
            })
            .collect()
    }

    #[test]
    fn modle_is_warned_with_model_as_the_suggestion() {
        let found = unknown_config_keys("modle = \"gpt-4o\"\n");
        assert_eq!(
            found,
            vec![UnknownKey::Key {
                line: 1,
                key: "modle".into(),
                section: None,
                suggestion: Some("model".into()),
            }]
        );
        let lines = unknown_config_key_warnings(".yoyo.toml", &found);
        assert_eq!(
            lines,
            vec![
                ".yoyo.toml line 1: `modle` is not a key yoyo reads, so it has no effect; \
                 did you mean `model`?"
                    .to_string()
            ]
        );
    }

    #[test]
    fn every_known_key_and_section_key_is_silent() {
        let mut text = String::new();
        for k in TOP_LEVEL_KEYS {
            text.push_str(&format!("{k} = \"x\"\n"));
        }
        text.push_str("hooks.pre.bash = \"echo hi\"\n");
        text.push_str("[permissions]\nallow = [\"cargo *\"]\ndeny = [\"rm *\"]\n");
        text.push_str("[directories]\nallow = [\"src\"]\ndeny = [\"secret\"]\n");
        text.push_str(
            "[mcp_servers.fs]\ncommand = \"npx\"\nargs = [\"-y\", \"x\"]\nenv = { A = \"1\" }\n",
        );
        text.push_str("[model_pricing.\"my-model\"]\ninput = 1.0\noutput = 2.0\n");
        assert_eq!(keys(&text), Vec::<String>::new());
    }

    #[test]
    fn set_keys_are_all_known() {
        for (k, _) in crate::config::SETTABLE_KEYS {
            assert!(
                TOP_LEVEL_KEYS.contains(k),
                "`/config set` writes `{k}`, which TOP_LEVEL_KEYS calls unread"
            );
        }
    }

    #[test]
    fn comments_blank_lines_and_empty_values_are_silent() {
        let text = "# modle = \"x\"\n\n   \nmodel =\nprovider = \"\"\n";
        assert_eq!(keys(text), Vec::<String>::new());
    }

    #[test]
    fn a_typo_inside_a_known_section_is_warned_with_the_section_key() {
        let found = unknown_config_keys("[permissions]\nalow = [\"cargo *\"]\n");
        assert_eq!(
            found,
            vec![UnknownKey::Key {
                line: 2,
                key: "alow".into(),
                section: Some("permissions".into()),
                suggestion: Some("allow".into()),
            }]
        );
    }

    #[test]
    fn a_misspelled_section_is_one_finding_listing_its_keys() {
        let found = unknown_config_keys("[permisions]\nallow = [\"a\"]\ndeny = [\"b\"]\n");
        assert_eq!(
            found,
            vec![UnknownKey::Section {
                line: 1,
                header: "[permisions]".into(),
                keys: vec!["allow".into(), "deny".into()],
                suggestion: Some("[permissions]".into()),
            }]
        );
        let lines = unknown_config_key_warnings("cfg", &found);
        assert_eq!(
            lines,
            vec![
                "cfg line 1: `[permisions]` is not a section yoyo reads, so its keys \
                 (allow, deny) have no effect; did you mean `[permissions]`?"
                    .to_string()
            ]
        );
    }

    #[test]
    fn a_hooks_table_is_unread_because_hooks_are_dotted_top_level_keys() {
        // `[hooks]` + `pre.bash` flattens to `pre.bash`, which the hook parser
        // never sees: it reads `hooks.pre.bash`.
        assert_eq!(
            keys("[hooks]\npre.bash = \"echo\"\n"),
            vec!["[hooks]".to_string()]
        );
    }

    #[test]
    fn a_top_level_name_under_any_section_is_read_because_the_map_is_flat() {
        assert_eq!(keys("[whatever]\nmodel = \"x\"\n"), Vec::<String>::new());
        assert_eq!(keys("[permissions]\nmodel = \"x\"\n"), Vec::<String>::new());
    }

    #[test]
    fn multi_line_arrays_and_strings_are_not_read_as_keys() {
        let text = "allow_x = 1\n\
                    mcp = [\n  \"FOO=bar cmd\",\n  \"x\",\n]\n\
                    system_prompt = \"\"\"\nmodle = inside a string\n\"\"\"\n";
        assert_eq!(keys(text), vec!["allow_x".to_string()]);
    }

    #[test]
    fn a_header_with_a_trailing_comment_is_not_a_header_to_the_parsers() {
        // parse_permissions_from_config needs `]` at the end, so this `allow`
        // is read by nothing and is reported at top level.
        let found = unknown_config_keys("[permissions] # mine\nallow = [\"x\"]\n");
        assert!(matches!(
            &found[..],
            [UnknownKey::Key { key, section: None, .. }] if key == "allow"
        ));
    }

    #[test]
    fn non_ascii_keys_do_not_panic_and_get_no_suggestion() {
        let found = unknown_config_keys("modèle = \"x\"\n✓✓ = 1\n[ñ]\nü = 1\n");
        assert_eq!(found.len(), 3);
        let _ = unknown_config_key_warnings("cfg", &found);
    }

    #[test]
    fn far_off_keys_get_no_suggestion() {
        let found = unknown_config_keys("colour_scheme = \"dark\"\n");
        assert!(matches!(
            &found[..],
            [UnknownKey::Key {
                suggestion: None,
                ..
            }]
        ));
    }

    /// Near-miss: this repository's own config uses only keys yoyo reads.
    #[test]
    fn this_repos_yoyo_toml_has_no_unknown_keys() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/.yoyo.toml"))
            .expect("repo .yoyo.toml");
        assert_eq!(unknown_config_keys(&text), Vec::new());
    }

    /// The census, re-run: every key a non-test reader of the flat config map
    /// looks up by name must be in TOP_LEVEL_KEYS, or a real key would be
    /// reported as unread.
    #[test]
    fn every_flat_map_reader_key_is_known() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut found: Vec<(String, String)> = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".rs") || name.ends_with("_tests.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let body = text.split("#[cfg(test)]\nmod tests").next().unwrap();
            // Join lines so a `.get("x")` chained onto the next line is seen.
            let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
            for (var, pat) in [
                ("file_config", ".get(\""),
                ("file_config", ".contains_key(\""),
                ("repl_file_config", ".get(\""),
                ("config", ".get(\""),
                ("map", ".get(\""),
            ] {
                for needle in [format!("{var}{pat}"), format!("{var} {pat}")] {
                    for (i, _) in flat.match_indices(needle.as_str()) {
                        // whole-word variable name
                        if flat[..i]
                            .chars()
                            .last()
                            .is_some_and(|c| c.is_alphanumeric() || c == '_')
                        {
                            continue;
                        }
                        let rest = &flat[i + needle.len()..];
                        if let Some(end) = rest.find('"') {
                            found.push((name.clone(), rest[..end].to_string()));
                        }
                    }
                }
            }
            for needle in ["config_flag(config, \"", "&file_config, \""] {
                for (i, _) in flat.match_indices(needle) {
                    let rest = &flat[i + needle.len()..];
                    if let Some(end) = rest.find('"') {
                        found.push((name.clone(), rest[..end].to_string()));
                    }
                }
            }
        }
        // `config.get(..)` and `map.get(..)` are also used on JSON values and
        // other maps; only keys that look like config names are candidates,
        // and those maps' keys are listed here by file so they cannot hide a
        // real config key elsewhere.
        let config_files = [
            "cli.rs",
            "config.rs",
            "dispatch_sub.rs",
            "repl.rs",
            "setup.rs",
            "cd_config_note.rs",
            "watch_shell.rs",
            "hooks.rs",
        ];
        let missing: Vec<_> = found
            .iter()
            .filter(|(file, _)| config_files.contains(&file.as_str()))
            .filter(|(_, key)| !is_top_level_key(key))
            .collect();
        assert!(
            found.iter().any(|(_, k)| k == "model") && found.iter().any(|(_, k)| k == "audit"),
            "the census found nothing (anti-vacuous): {found:?}"
        );
        assert!(
            missing.is_empty(),
            "config readers look up keys TOP_LEVEL_KEYS does not list: {missing:?}"
        );
    }
}
