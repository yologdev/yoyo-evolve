//! #1184 slice 2: the flat config map holds TOML's top-level keys only.
//!
//! Before this slice `parse_config_file` read the file line by line and put
//! every `k = v` into one map whatever header it sat under, last one winning,
//! so `model` under `[mcp_servers.fs]` replaced the top-level `model` and was
//! the model the request went out on. Each row says what the map holds for
//! the key AND that the section's copy did not reach it.

use super::parse_config_file;
use std::collections::HashMap;

/// HEAD's line loop, kept verbatim as the baseline the near-miss rows must
/// still match (the reader it described is gone from production).
fn legacy_flat_map(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = crate::config_unknown_keys::config_key_name(key);
            let value = crate::config_value::strip_trailing_comment(value);
            let read = if value.starts_with('[') {
                Ok(value.to_string())
            } else {
                crate::config_value::read_scalar(value)
            };
            let Ok(value) = read else { continue };
            map.insert(key, value);
        }
    }
    map
}

fn get(content: &str, key: &str) -> Option<String> {
    parse_config_file(content).get(key).cloned()
}

#[test]
fn a_key_under_a_section_never_overrides_the_top_level_one() {
    // The assessment's repro: HEAD read `claude-b` and sent it.
    let doc = "model = \"claude-a\"\n[mcp_servers.fs]\ncommand = \"true\"\nmodel = \"claude-b\"\n";
    assert_eq!(get(doc, "model").as_deref(), Some("claude-a"));
    // The section's own keys are not top-level keys either.
    assert_eq!(get(doc, "command"), None);
    assert_eq!(legacy_flat_map(doc)["model"], "claude-b", "HEAD's reading");
}

#[test]
fn a_key_only_under_a_header_is_absent() {
    for (doc, key) in [
        ("[permissions]\nprovider = \"openai\"\n", "provider"),
        (
            "[whatever]\nbase_url = \"http://evil.example\"\n",
            "base_url",
        ),
        ("[a.b]\nmodel = \"x\"\n", "model"),
        ("[[servers]]\nmodel = \"x\"\n", "model"),
    ] {
        assert_eq!(get(doc, key), None, "{doc:?}");
        assert!(
            legacy_flat_map(doc).contains_key(key),
            "HEAD leaked {doc:?}"
        );
    }
}

#[test]
fn a_dotted_or_inline_key_is_not_a_top_level_key() {
    assert_eq!(get("a.model = \"x\"\n", "model"), None);
    assert_eq!(get("a.model = \"x\"\n", "a.model"), None);
    let doc = "model = \"top\"\nx = { model = \"y\" }\n";
    assert_eq!(get(doc, "model").as_deref(), Some("top"));
    assert_eq!(get(doc, "x"), None, "an inline table is no scalar");
}

#[test]
fn plain_top_level_keys_read_as_they_did_at_head() {
    let doc = "provider = \"anthropic\" # c\n\
               model = \"claude-opus-5-5\"\n\
               base_url = 'http://127.0.0.1:9/v1'\n\
               max_tokens = 4096\n\
               temperature = 0.5\n\
               auto_watch = true\n\
               api_key = \"sk-\\u0041\"\n\
               fallback = \"\"\n\
               mcp = [\"npx a\", \"b, c\"] # servers\n";
    let new = parse_config_file(doc);
    let old = legacy_flat_map(doc);
    for key in [
        "provider",
        "model",
        "base_url",
        "max_tokens",
        "temperature",
        "auto_watch",
        "api_key",
        "fallback",
    ] {
        assert_eq!(new.get(key), old.get(key), "{key}");
    }
    assert_eq!(
        new["fallback"], "",
        "empty stays empty (config get reports it as unset, #1172)"
    );
    // The array is re-rendered, so compare what its reader makes of it.
    assert_eq!(
        crate::config::parse_toml_array(&new["mcp"]),
        crate::config::parse_toml_array(&old["mcp"])
    );
    assert_eq!(
        crate::config::parse_toml_array(&new["mcp"]),
        vec!["npx a".to_string(), "b, c".to_string()]
    );
}

#[test]
fn an_array_with_quotes_and_backslashes_survives_its_reader() {
    let doc = "mcp = [\"say \\\"hi\\\"\", 'C:\\tmp', \"\"]\n";
    let new = parse_config_file(doc);
    assert_eq!(
        crate::config::parse_toml_array(&new["mcp"]),
        vec![
            "say \"hi\"".to_string(),
            "C:\\tmp".to_string(),
            String::new()
        ]
    );
}

#[test]
fn hook_keys_reach_the_hook_reader_in_every_toml_spelling() {
    let want = Some("echo hi".to_string());
    for doc in [
        "hooks.pre.bash = \"echo hi\"\n",
        "[hooks]\npre.bash = \"echo hi\"\n",
        "[hooks.pre]\nbash = \"echo hi\"\n",
        "hooks = { pre = { bash = \"echo hi\" } }\n",
    ] {
        assert_eq!(get(doc, "hooks.pre.bash"), want, "{doc:?}");
    }
    // HEAD read the documented spelling the same way.
    assert_eq!(
        legacy_flat_map("hooks.pre.bash = \"echo hi\"\n").get("hooks.pre.bash"),
        want.as_ref()
    );
    // A hook-looking key under another section is that section's key.
    assert_eq!(
        get("[x]\nhooks.pre.bash = \"echo hi\"\n", "hooks.pre.bash"),
        None
    );
}

#[test]
fn the_repos_own_config_reads_identically() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/.yoyo.toml"))
        .expect("the repo's .yoyo.toml");
    assert_eq!(parse_config_file(&text), legacy_flat_map(&text));
    assert!(parse_config_file(&text).contains_key("model"));
}

#[test]
fn content_that_is_not_toml_configures_nothing() {
    // The loader refuses such a file first; any other caller gets nothing,
    // never the line loop's partial guess.
    assert!(parse_config_file("model = m1\nprovider = \"openai\"\n").is_empty());
}

#[test]
fn the_trust_prompt_names_a_hook_in_every_spelling_the_map_reads() {
    // A spelling the map newly reads must not run without the prompt naming
    // it: the inline table is no `[hooks` line and no `hooks.<phase>.` line.
    for doc in [
        "hooks = { pre = { bash = \"echo hi\" } }\n",
        "\"hooks\".pre.bash = \"echo hi\"\n",
        "hooks.pre.bash = \"echo hi\"\n",
    ] {
        let grants = crate::config_paths::project_trust_grants(doc, false, false, false);
        assert!(
            grants.iter().any(|g| g.starts_with("shell hooks")),
            "{doc:?}: {grants:?}"
        );
    }
}
