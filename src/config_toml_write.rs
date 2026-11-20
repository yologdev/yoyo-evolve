//! Writers for yoyo's config files, through `toml_edit` (#1184 step 2).
//!
//! `/config set` ([`set_toml_key`]) and the "always allow" prompt
//! ([`set_permissions_allow`]) edit the parsed document, not lines, so a
//! user's comments (above and beside keys), key order, key spelling (quoted,
//! dotted) and multi-line arrays survive a write. Before this, the line
//! editors rewrote only the opening line of a multi-line `allow = [` (leaving
//! the old elements dangling: invalid TOML), dropped a comment beside the
//! value they replaced, and appended a top-level key at end of file, which in
//! any file with a `[section]` put it inside that section.
//!
//! A file that is not valid TOML is refused, unchanged: these return `Err`
//! with the parser's line and column, and the callers in `config.rs` write
//! nothing (the #1216 class: never parse-fail-to-empty-then-write).

use toml_edit::{Array, DocumentMut, Item, Table, Value};

/// Parse `content` for editing, or say why it can't be edited.
fn parse_for_edit(content: &str) -> Result<DocumentMut, String> {
    content.parse::<DocumentMut>().map_err(|e| {
        format!(
            "is not valid TOML, so yoyo left it unchanged: {}",
            e.to_string().trim_end()
        )
    })
}

/// The TOML value `/config set` writes for what the user typed: an integer,
/// a finite float or a boolean goes unquoted, anything else is a string
/// (escaped by `toml_edit`, so the reader gives back exactly `value`).
fn toml_value_for(value: &str) -> Value {
    if let Ok(i) = value.parse::<i64>() {
        return i.into();
    }
    if let Some(f) = value.parse::<f64>().ok().filter(|f| f.is_finite()) {
        return f.into();
    }
    match value {
        "true" => true.into(),
        "false" => false.into(),
        _ => value.into(),
    }
}

/// Set top-level `key = value`, keeping everything else byte-identical.
///
/// An existing top-level `key` (however spelled, e.g. `"model"`) gets the new
/// value with its old surrounding whitespace and beside-comment; a missing
/// one goes after the last top-level key, never inside a `[section]`. A key
/// that names a table (`[model]`) is refused rather than replaced.
pub fn set_toml_key(content: &str, key: &str, value: &str) -> Result<String, String> {
    let mut doc = parse_for_edit(content)?;
    let mut new = toml_value_for(value);
    match doc.get_mut(key) {
        Some(Item::Value(old)) => {
            *new.decor_mut() = old.decor().clone();
            *old = new;
        }
        Some(Item::None) | None => {
            doc.insert(key, Item::Value(new));
        }
        Some(_) => {
            return Err(format!(
                "has `{key}` as a table, not a value, so yoyo left it unchanged"
            ))
        }
    }
    Ok(with_trailing_newline(doc.to_string()))
}

/// Make every one of `patterns` an element of `[permissions]`'s `allow`
/// array, appending the missing ones in order. Existing elements, their
/// comments and their layout stay as they are; in a multi-line array a new
/// element takes the previous element's line indent. The table is created
/// if absent, and kept in the user's spelling if present (`[permissions]`,
/// dotted `permissions.allow`, inline `permissions = { ... }`).
pub fn set_permissions_allow(content: &str, patterns: &[String]) -> Result<String, String> {
    let mut doc = parse_for_edit(content)?;
    if !doc.contains_key("permissions") {
        doc.insert("permissions", Item::Table(Table::new()));
    }
    let Some(table) = doc.get_mut("permissions").and_then(Item::as_table_like_mut) else {
        return Err("has `permissions` that is not a table, so yoyo left it unchanged".into());
    };
    if !table.contains_key("allow") {
        table.insert("allow", Item::Value(Value::Array(Array::new())));
    }
    let Some(allow) = table.get_mut("allow").and_then(Item::as_array_mut) else {
        return Err(
            "has `permissions.allow` that is not an array, so yoyo left it unchanged".into(),
        );
    };
    for pattern in patterns {
        if allow.iter().any(|v| v.as_str() == Some(pattern.as_str())) {
            continue;
        }
        push_keeping_layout(allow, pattern);
    }
    Ok(with_trailing_newline(doc.to_string()))
}

/// Push `pattern` onto `array`; in a multi-line array the new element copies
/// the last element's indent, so it lands on its own line.
fn push_keeping_layout(array: &mut Array, pattern: &str) {
    let prefix = array
        .iter()
        .last()
        .and_then(|v| v.decor().prefix())
        .and_then(|p| p.as_str())
        // The prefix holds the end of the previous line too (its comment),
        // so only the newline and the indent after it are copied.
        .and_then(|p| p.rfind('\n').map(|i| p[i..].to_string()));
    array.push(pattern);
    if let (Some(prefix), Some(last)) = (prefix, array.iter_mut().last()) {
        last.decor_mut().set_prefix(prefix);
    }
}

fn with_trailing_newline(mut s: String) -> String {
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allow(content: &str, pattern: &str) -> Result<String, String> {
        set_permissions_allow(content, &[pattern.to_string()])
    }

    /// The before/after corpus posted on #1184: every row keeps what it
    /// didn't ask to change, byte for byte.
    #[test]
    fn set_toml_key_corpus_keeps_comments_order_and_sections() {
        let v = "say \"hi\" C:\\tmp é";
        let rows: &[(&str, &str, &str)] = &[
            (
                "comments above and beside",
                "# top comment\nmodel = \"a\" # beside\n# between\nprovider = \"anthropic\"\n",
                "# top comment\nmodel = 'say \"hi\" C:\\tmp é' # beside\n# between\nprovider = \"anthropic\"\n",
            ),
            (
                "non-alphabetical order",
                "thinking = \"low\"\nmodel = \"a\"\nauto_commit = true\n",
                "thinking = \"low\"\nmodel = 'say \"hi\" C:\\tmp é'\nauto_commit = true\n",
            ),
            (
                "absent key, file with a section: lands top-level",
                "provider = \"x\"\n[permissions]\nallow = [\"ls *\"]\n",
                "provider = \"x\"\nmodel = 'say \"hi\" C:\\tmp é'\n[permissions]\nallow = [\"ls *\"]\n",
            ),
            (
                "same key under a header is not the top-level key",
                "[permissions]\nmodel = \"x\"\n",
                "model = 'say \"hi\" C:\\tmp é'\n[permissions]\nmodel = \"x\"\n",
            ),
            (
                "quoted key keeps its spelling",
                "\"model\" = \"a\"\n",
                "\"model\" = 'say \"hi\" C:\\tmp é'\n",
            ),
            ("empty file", "", "model = 'say \"hi\" C:\\tmp é'\n"),
        ];
        for (why, before, after) in rows {
            let got = set_toml_key(before, "model", v).unwrap();
            assert_eq!(&got, after, "{why}");
            assert_eq!(
                crate::config::parse_config_file(&got)
                    .get("model")
                    .map(String::as_str),
                Some(v),
                "{why}: the run reads back what was set"
            );
        }
    }

    #[test]
    fn set_toml_key_value_types() {
        assert_eq!(
            set_toml_key("", "max_tokens", "8192").unwrap(),
            "max_tokens = 8192\n"
        );
        assert_eq!(
            set_toml_key("", "temperature", "0.5").unwrap(),
            "temperature = 0.5\n"
        );
        assert_eq!(
            set_toml_key("", "auto_commit", "true").unwrap(),
            "auto_commit = true\n"
        );
        // Not finite: a string, never an unquoted `NaN` the parser refuses.
        assert_eq!(
            set_toml_key("", "model", "NaN").unwrap(),
            "model = \"NaN\"\n"
        );
    }

    #[test]
    fn set_toml_key_refuses_a_table_of_that_name() {
        let before = "[model]\nid = \"a\"\n";
        let err = set_toml_key(before, "model", "b").unwrap_err();
        assert!(err.contains("as a table"), "{err}");
    }

    /// The multi-line row is the defect the line editor had: it rewrote
    /// only `allow = [` and left the old elements dangling.
    #[test]
    fn allow_corpus_keeps_layout_and_appends_as_an_element() {
        let rows: &[(&str, &str, &str)] = &[
            (
                "one-line array",
                "[permissions]\nallow = [\"ls *\"]\ndeny = [\"rm *\"]\n",
                "[permissions]\nallow = [\"ls *\", \"git status\"]\ndeny = [\"rm *\"]\n",
            ),
            (
                "multi-line array, trailing comma, comment on an element",
                "[permissions]\nallow = [\n  \"ls *\", # listing\n  \"cat *\",\n]\ndeny = [\"rm *\"]\n",
                "[permissions]\nallow = [\n  \"ls *\", # listing\n  \"cat *\",\n  \"git status\",\n]\ndeny = [\"rm *\"]\n",
            ),
            (
                "no [permissions] table",
                "# mine\nmodel = \"a\" # beside\n",
                "# mine\nmodel = \"a\" # beside\n\n[permissions]\nallow = [\"git status\"]\n",
            ),
            (
                "table without allow",
                "[permissions]\ndeny = [\"rm *\"]\n",
                "[permissions]\ndeny = [\"rm *\"]\nallow = [\"git status\"]\n",
            ),
            (
                "inline table",
                "permissions = { allow = [\"ls *\"] }\n",
                "permissions = { allow = [\"ls *\", \"git status\"] }\n",
            ),
            (
                "already present: unchanged",
                "[permissions]\nallow = [\n  \"git status\", # kept\n]\n",
                "[permissions]\nallow = [\n  \"git status\", # kept\n]\n",
            ),
        ];
        for (why, before, after) in rows {
            let got = allow(before, "git status").unwrap();
            assert_eq!(&got, after, "{why}");
            assert!(
                crate::config::parse_permissions_from_config(&got)
                    .allow
                    .iter()
                    .any(|p| p == "git status"),
                "{why}: the fence reads the new pattern"
            );
        }
    }

    #[test]
    fn allow_pattern_with_quote_and_backslash_round_trips() {
        let p = r#"echo "a\b""#;
        let got = allow("", p).unwrap();
        let read = crate::config::parse_permissions_from_config(&got).allow;
        assert_eq!(read, vec![p.to_string()], "{got}");
    }

    /// #1216's class: a file that doesn't parse is refused with the parser's
    /// location, and the writer produces nothing to write.
    #[test]
    fn invalid_toml_is_refused_with_a_location() {
        let broken = "model = \"a\nprovider = \n";
        for err in [
            set_toml_key(broken, "model", "b").unwrap_err(),
            allow(broken, "ls *").unwrap_err(),
        ] {
            assert!(err.contains("not valid TOML"), "{err}");
            assert!(err.contains("line 1"), "{err}");
        }
    }

    /// The same at the file seams `/config set` and "always allow" call:
    /// the broken file is left byte-identical and the error names it.
    #[test]
    fn invalid_toml_file_is_left_byte_identical_by_both_writers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".yoyo.toml");
        let broken = "# mine\nmodel = \"a\nprovider = \n";
        std::fs::write(&path, broken).unwrap();
        for err in [
            crate::config::write_config_value_to("model", "b", &path).unwrap_err(),
            crate::config::append_allow_pattern_to("ls *", &path).unwrap_err(),
        ] {
            assert!(err.contains(&path.display().to_string()), "{err}");
            assert!(err.contains("line 2"), "{err}");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
        }
    }

    #[test]
    fn allow_that_is_not_an_array_is_refused() {
        let err = allow("[permissions]\nallow = \"ls *\"\n", "x").unwrap_err();
        assert!(err.contains("not an array"), "{err}");
    }
}
