//! #1184 slice 1: the `toml` crate is the gate for every loaded config file.
//! These are gate tests only. What each reader does with a valid file stays
//! pinned by the readers' own tests in `config.rs`.

use super::toml_parse_error;

/// The refusal for `content`, which these tests expect to exist.
fn refusal(content: &str) -> String {
    toml_parse_error(".yoyo.toml", content)
        .unwrap_or_else(|| panic!("expected a refusal for {content:?}"))
}

#[test]
fn bare_value_is_refused_with_path_line_and_column() {
    // Before #1184 this ran as model `m1`: the hand parser accepted it.
    let msg = refusal("model = m1\n");
    assert!(
        msg.starts_with(".yoyo.toml:1:9: config file is not valid TOML: "),
        "{msg}"
    );
    assert!(msg.contains("string values must be quoted"), "{msg}");
    // The offending line and a caret under the value.
    assert!(msg.contains("1 | model = m1"), "{msg}");
    assert!(msg.contains("  |         ^^"), "{msg}");
    assert!(msg.contains("nothing ran and nothing was sent"), "{msg}");
    assert!(
        msg.contains("To fix: correct line 1 of .yoyo.toml"),
        "{msg}"
    );
}

#[test]
fn duplicate_key_is_refused_not_last_wins() {
    // Named behaviour change: the hand parser silently took the last value.
    let msg = refusal("model = \"a\"\nmodel = \"b\"\n");
    assert!(msg.starts_with(".yoyo.toml:2:1: "), "{msg}");
    assert!(msg.contains("duplicate key"), "{msg}");
    assert!(msg.contains("2 | model = \"b\""), "{msg}");
}

#[test]
fn shapes_the_old_syntax_check_caught_are_refused_too() {
    // #1034/#1036 shapes: an unterminated quote and an unclosed header.
    let msg = refusal("provider = \"anthropic\"\nmodel = \"claude-haiku-5-5\n");
    assert!(msg.starts_with(".yoyo.toml:2:"), "{msg}");
    let msg = refusal("model = \"x\"\n\n[permissions\nallow = [\"a\"]\n");
    assert!(msg.starts_with(".yoyo.toml:3:"), "{msg}");
}

#[test]
fn column_counts_characters_not_bytes() {
    // é and ✓ are 2 and 3 bytes; the column must still be the 11th char.
    let msg = refusal("k = \"é✓ ab\n");
    assert!(msg.starts_with(".yoyo.toml:1:11: "), "{msg}");
}

#[test]
fn line_col_at_never_splits_a_char() {
    // Offsets inside a multi-byte char clamp down to its start.
    let text = "a\n✓b";
    assert_eq!(super::line_col_at(text, 0), (1, 1));
    assert_eq!(super::line_col_at(text, 2), (2, 1));
    assert_eq!(super::line_col_at(text, 3), (2, 1));
    assert_eq!(super::line_col_at(text, 5), (2, 2));
    assert_eq!(super::line_col_at(text, 99), (2, 3));
}

#[test]
fn path_and_echoed_line_are_sanitized() {
    let msg = toml_parse_error("evil\x1b[31m.toml", "model = m1\x1b\n").unwrap();
    assert!(!msg.contains('\x1b'), "{msg:?}");
    assert!(msg.starts_with("evil\\x1b[31m.toml:1:"), "{msg}");
}

#[test]
fn valid_shapes_from_the_closed_tickets_pass_the_gate() {
    let valid = [
        // #1174: header with a trailing comment.
        "[permissions] # mine\nallow = [\"git *\"]\n",
        // #1177: array value with a trailing comment.
        "[permissions]\ndeny = [\"rm -rf *\"] # never\n",
        // #1178: spaced header and a dotted key.
        "[ directories ]\ndeny = [\"secrets\"]\n",
        "directories.deny = [\"secrets\"]\n",
        // #1180: inline table.
        "directories = { deny = [\"secrets\"] }\n",
        // #1182: an mcp server as an inline table under [mcp_servers].
        "[mcp_servers]\nfs = { command = \"npx\" }\n",
        // Empty and comment-only files.
        "",
        "# nothing here\n",
    ];
    for content in valid {
        assert_eq!(toml_parse_error(".yoyo.toml", content), None, "{content:?}");
    }
}

#[test]
fn the_repos_own_config_passes_the_gate() {
    // The protected harness scripts read this file; it must stay loadable.
    let own = include_str!("../.yoyo.toml");
    assert_eq!(toml_parse_error(".yoyo.toml", own), None);
}
