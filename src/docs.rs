//! docs.rs lookup subsystem for yoyo.
//!
//! Fetches and parses documentation from docs.rs for Rust crates.
//! Used by the `/docs` REPL command.

/// Validate a crate name: only alphanumeric, hyphens, underscores.
pub fn is_valid_crate_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Fetch HTML from a docs.rs URL. Returns Ok(body) or Err(message).
fn fetch_docs_html(url: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args(["-sL", "--max-time", "10", url])
        .output()
        .map_err(|e| format!("Error fetching docs: {e}"))?;

    if !output.status.success() || output.stdout.is_empty() {
        return Err("Could not reach docs.rs".to_string());
    }

    let body = String::from_utf8_lossy(&output.stdout).to_string();

    if body.contains("This crate does not exist")
        || body.contains("failed to build")
        || body.contains("The requested resource does not exist")
    {
        return Err("not found on docs.rs".to_string());
    }

    Ok(body)
}

/// A single API item parsed from a docs.rs crate page.
#[derive(Debug, Clone, PartialEq)]
pub struct DocsItem {
    pub kind: String, // "mod", "struct", "enum", "trait", "fn", "type", "macro"
    pub name: String, // item name (e.g. "Serialize", "task")
}

/// Parse API items from docs.rs HTML.
/// Extracts items matching the pattern:
/// `class="(mod|struct|enum|trait|fn|type|macro)" href="..." title="...">name`
pub fn parse_docs_items(html: &str) -> Vec<DocsItem> {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let kinds = ["mod", "struct", "enum", "trait", "fn", "type", "macro"];

    for kind in &kinds {
        let pattern = format!("class=\"{kind}\" href=\"");
        let mut search_from = 0;

        while let Some(pos) = html[search_from..].find(&pattern) {
            let abs_pos = search_from + pos;
            search_from = abs_pos + pattern.len();

            let after_class = &html[abs_pos..];
            let Some(gt_pos) = after_class.find('>') else {
                continue;
            };
            let text_start = abs_pos + gt_pos + 1;
            let Some(lt_pos) = html[text_start..].find('<') else {
                continue;
            };

            let tag_content = &after_class[..gt_pos];
            let name = if let Some(title_start) = tag_content.find("title=\"") {
                let title_after = &tag_content[title_start + 7..];
                if let Some(title_end) = title_after.find('"') {
                    let title = &title_after[..title_end];
                    title.rsplit("::").next().unwrap_or(title).to_string()
                } else {
                    html[text_start..text_start + lt_pos].trim().to_string()
                }
            } else {
                html[text_start..text_start + lt_pos].trim().to_string()
            };

            if !name.is_empty() {
                let key = format!("{kind}:{name}");
                if seen.insert(key) {
                    items.push(DocsItem {
                        kind: kind.to_string(),
                        name,
                    });
                }
            }
        }
    }

    items
}

/// Format parsed docs items into a grouped display string.
/// Each category is capped at `max_per_kind` items with a "+N more" suffix.
pub fn format_docs_items(items: &[DocsItem], max_per_kind: usize) -> String {
    use std::collections::BTreeMap;

    let mut groups: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for item in items {
        groups.entry(&item.kind).or_default().push(&item.name);
    }

    if groups.is_empty() {
        return String::new();
    }

    let display_order = ["mod", "struct", "enum", "trait", "fn", "type", "macro"];
    let kind_labels: std::collections::HashMap<&str, &str> = [
        ("mod", "Modules"),
        ("struct", "Structs"),
        ("enum", "Enums"),
        ("trait", "Traits"),
        ("fn", "Functions"),
        ("type", "Types"),
        ("macro", "Macros"),
    ]
    .into_iter()
    .collect();

    let mut output = String::new();
    for kind in &display_order {
        if let Some(names) = groups.get(kind) {
            let label = kind_labels.get(kind).unwrap_or(kind);
            let total = names.len();
            let shown: Vec<&str> = names.iter().take(max_per_kind).copied().collect();
            let list = shown.join(", ");
            if total > max_per_kind {
                let more = total - max_per_kind;
                output.push_str(&format!("  {label}: {list}, +{more} more\n"));
            } else {
                output.push_str(&format!("  {label}: {list}\n"));
            }
        }
    }

    if output.ends_with('\n') {
        output.truncate(output.len() - 1);
    }

    output
}

/// Fetch a summary from docs.rs for a given Rust crate.
/// Returns (found, summary_text). If the crate exists, `found` is true and `summary_text`
/// contains the URL, description, and API item overview. If not found or on error, `found` is false.
pub fn fetch_docs_summary(crate_name: &str) -> (bool, String) {
    if !is_valid_crate_name(crate_name) {
        return (false, format!("Invalid crate name: '{crate_name}'"));
    }

    let crate_mod = crate_name.replace('-', "_");
    let url = format!("https://docs.rs/{crate_name}/latest/{crate_mod}/");

    let body = match fetch_docs_html(&url) {
        Ok(body) => body,
        Err(e) if e.contains("not found") => {
            return (false, format!("Crate '{crate_name}' {e}"));
        }
        Err(e) if e.contains("Could not reach") => {
            return (false, format!("{e} for '{crate_name}'"));
        }
        Err(e) => return (false, e),
    };

    let description = extract_meta_description(&body);
    let items = parse_docs_items(&body);
    let items_display = format_docs_items(&items, 10);

    let mut summary = format!("  📦 {url}\n");
    if let Some(desc) = description {
        summary.push_str(&format!("  📝 {desc}\n"));
    }
    if !items_display.is_empty() {
        summary.push_str(&format!("\n{items_display}"));
    } else {
        if !summary.contains("📝") {
            summary.push_str("  Docs available at the URL above.");
        }
    }

    (true, summary)
}

/// Fetch docs for a specific item within a crate (e.g., `/docs tokio task`).
/// Constructs the URL as `https://docs.rs/<crate>/latest/<crate_mod>/<item>/`.
/// Returns (found, summary_text).
pub fn fetch_docs_item(crate_name: &str, item: &str) -> (bool, String) {
    if !is_valid_crate_name(crate_name) {
        return (false, format!("Invalid crate name: '{crate_name}'"));
    }
    if item.is_empty() {
        return fetch_docs_summary(crate_name);
    }

    let crate_mod = crate_name.replace('-', "_");
    let url = format!("https://docs.rs/{crate_name}/latest/{crate_mod}/{item}/");

    let body = match fetch_docs_html(&url) {
        Ok(body) => body,
        Err(_) => {
            return (
                false,
                format!("Item '{item}' not found in crate '{crate_name}' on docs.rs"),
            );
        }
    };

    let description = extract_meta_description(&body);
    let items = parse_docs_items(&body);
    let items_display = format_docs_items(&items, 10);

    let mut summary = format!("  📦 {url}\n");
    if let Some(desc) = description {
        summary.push_str(&format!("  📝 {desc}\n"));
    }
    if !items_display.is_empty() {
        summary.push_str(&format!("\n{items_display}"));
    } else if !summary.contains("📝") {
        summary.push_str("  Docs available at the URL above.");
    }

    (true, summary)
}

/// Extract the content of `<meta name="description" content="...">` from HTML.
pub fn extract_meta_description(html: &str) -> Option<String> {
    let needle = "name=\"description\"";
    let pos = html.find(needle)?;

    let after = &html[pos..];
    let content_start = after.find("content=\"")?;
    let content = &after[content_start + 9..]; // skip past 'content="'
    let content_end = content.find('"')?;
    let desc = &content[..content_end];

    let desc = desc
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");

    let desc = desc.trim().to_string();
    if desc.is_empty() || desc == "API documentation for the Rust `crate` crate." {
        None
    } else {
        Some(desc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_crate_name() {
        assert!(is_valid_crate_name("serde"));
        assert!(is_valid_crate_name("tokio"));
        assert!(is_valid_crate_name("my-crate"));
        assert!(is_valid_crate_name("my_crate"));
        assert!(is_valid_crate_name("serde-json"));
        assert!(!is_valid_crate_name(""));
        assert!(!is_valid_crate_name("not a valid/crate"));
        assert!(!is_valid_crate_name("some@crate!"));
    }

    #[test]
    fn test_extract_meta_description_basic() {
        let html = r#"<html><head><meta name="description" content="A fast serialization framework"></head></html>"#;
        let desc = extract_meta_description(html);
        assert_eq!(desc, Some("A fast serialization framework".to_string()));
    }

    #[test]
    fn test_extract_meta_description_with_entities() {
        let html = r#"<meta name="description" content="Handles &amp; processes &lt;data&gt;">"#;
        let desc = extract_meta_description(html);
        assert_eq!(desc, Some("Handles & processes <data>".to_string()));
    }

    #[test]
    fn test_extract_meta_description_missing() {
        let html = r#"<html><head><title>No meta desc</title></head></html>"#;
        let desc = extract_meta_description(html);
        assert!(desc.is_none());
    }

    #[test]
    fn test_extract_meta_description_empty() {
        let html = r#"<meta name="description" content="">"#;
        let desc = extract_meta_description(html);
        assert!(desc.is_none());
    }

    #[test]
    fn test_parse_docs_items_modules() {
        let html = r#"
            <a class="mod" href="fs/index.html" title="mod tokio::fs">fs</a>
            <a class="mod" href="io/index.html" title="mod tokio::io">io</a>
            <a class="mod" href="sync/index.html" title="mod tokio::sync">sync</a>
        "#;
        let items = parse_docs_items(html);
        assert_eq!(items.len(), 3);
        assert_eq!(
            items[0],
            DocsItem {
                kind: "mod".into(),
                name: "fs".into()
            }
        );
        assert_eq!(
            items[1],
            DocsItem {
                kind: "mod".into(),
                name: "io".into()
            }
        );
        assert_eq!(
            items[2],
            DocsItem {
                kind: "mod".into(),
                name: "sync".into()
            }
        );
    }

    #[test]
    fn test_parse_docs_items_mixed_kinds() {
        let html = r#"
            <a class="mod" href="de/index.html" title="mod serde::de">de</a>
            <a class="mod" href="ser/index.html" title="mod serde::ser">ser</a>
            <a class="trait" href="trait.Serialize.html" title="trait serde::Serialize">Serialize</a>
            <a class="trait" href="trait.Deserialize.html" title="trait serde::Deserialize">Deserialize</a>
            <a class="macro" href="macro.forward.html" title="macro serde::forward_to_deserialize_any">forward_</a>
        "#;
        let items = parse_docs_items(html);
        assert_eq!(items.len(), 5);

        let mods: Vec<&DocsItem> = items.iter().filter(|i| i.kind == "mod").collect();
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].name, "de");
        assert_eq!(mods[1].name, "ser");

        let traits: Vec<&DocsItem> = items.iter().filter(|i| i.kind == "trait").collect();
        assert_eq!(traits.len(), 2);
        assert_eq!(traits[0].name, "Serialize");
        assert_eq!(traits[1].name, "Deserialize");

        // Macro name should come from title (full name), not truncated display text
        let macros: Vec<&DocsItem> = items.iter().filter(|i| i.kind == "macro").collect();
        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].name, "forward_to_deserialize_any");
    }

    #[test]
    fn test_parse_docs_items_structs_enums_fns() {
        let html = r#"
            <a class="struct" href="struct.Runtime.html" title="struct tokio::runtime::Runtime">Runtime</a>
            <a class="enum" href="enum.Error.html" title="enum tokio::io::Error">Error</a>
            <a class="fn" href="fn.spawn.html" title="fn tokio::task::spawn">spawn</a>
            <a class="type" href="type.Result.html" title="type tokio::io::Result">Result</a>
        "#;
        let items = parse_docs_items(html);
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].kind, "struct");
        assert_eq!(items[0].name, "Runtime");
        assert_eq!(items[1].kind, "enum");
        assert_eq!(items[1].name, "Error");
        assert_eq!(items[2].kind, "fn");
        assert_eq!(items[2].name, "spawn");
        assert_eq!(items[3].kind, "type");
        assert_eq!(items[3].name, "Result");
    }

    #[test]
    fn test_parse_docs_items_empty_html() {
        let items = parse_docs_items("");
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_docs_items_no_matching_classes() {
        let html = r#"<a class="other" href="foo.html">bar</a>"#;
        let items = parse_docs_items(html);
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_docs_items_deduplication() {
        let html = r#"
            <a class="trait" href="trait.Serialize.html" title="trait serde::Serialize">Serialize</a>
            <a class="trait" href="trait.Serialize.html" title="trait serde::Serialize">Serialize</a>
        "#;
        let items = parse_docs_items(html);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Serialize");
    }

    #[test]
    fn test_format_docs_items_basic() {
        let items = vec![
            DocsItem {
                kind: "mod".into(),
                name: "fs".into(),
            },
            DocsItem {
                kind: "mod".into(),
                name: "io".into(),
            },
            DocsItem {
                kind: "trait".into(),
                name: "Serialize".into(),
            },
        ];
        let output = format_docs_items(&items, 10);
        assert!(output.contains("Modules: fs, io"));
        assert!(output.contains("Traits: Serialize"));
    }

    #[test]
    fn test_format_docs_items_capped_with_more() {
        let items: Vec<DocsItem> = (0..15)
            .map(|i| DocsItem {
                kind: "struct".into(),
                name: format!("S{i}"),
            })
            .collect();
        let output = format_docs_items(&items, 10);
        assert!(output.contains("Structs:"), "Should have Structs label");
        assert!(
            output.contains("+5 more"),
            "Should show +5 more, got: {output}"
        );
        assert!(output.contains("S0"));
        assert!(output.contains("S9"));
    }

    #[test]
    fn test_format_docs_items_empty() {
        let output = format_docs_items(&[], 10);
        assert!(output.is_empty());
    }

    #[test]
    fn test_format_docs_items_ordering() {
        let items = vec![
            DocsItem {
                kind: "macro".into(),
                name: "my_macro".into(),
            },
            DocsItem {
                kind: "mod".into(),
                name: "mymod".into(),
            },
            DocsItem {
                kind: "trait".into(),
                name: "MyTrait".into(),
            },
            DocsItem {
                kind: "struct".into(),
                name: "MyStruct".into(),
            },
        ];
        let output = format_docs_items(&items, 10);
        let mod_pos = output.find("Modules:").unwrap();
        let struct_pos = output.find("Structs:").unwrap();
        let trait_pos = output.find("Traits:").unwrap();
        let macro_pos = output.find("Macros:").unwrap();
        assert!(mod_pos < struct_pos, "Modules should come before Structs");
        assert!(struct_pos < trait_pos, "Structs should come before Traits");
        assert!(trait_pos < macro_pos, "Traits should come before Macros");
    }

    #[test]
    fn test_fetch_docs_summary_invalid_crate_name() {
        let (found, msg) = fetch_docs_summary("not a valid/crate");
        assert!(!found);
        assert!(msg.contains("Invalid crate name"), "Got: {msg}");

        let (found2, msg2) = fetch_docs_summary("");
        assert!(!found2);
        assert!(msg2.contains("Invalid crate name"), "Got: {msg2}");

        let (found3, msg3) = fetch_docs_summary("some@crate!");
        assert!(!found3);
        assert!(msg3.contains("Invalid crate name"), "Got: {msg3}");
    }

    #[test]
    fn test_fetch_docs_summary_valid_crate_name_accepted() {
        let names = ["serde", "tokio", "my-crate", "my_crate", "serde-json"];
        for name in &names {
            let (_, msg) = fetch_docs_summary(name);
            assert!(
                !msg.contains("Invalid crate name"),
                "'{name}' should pass validation but got: {msg}"
            );
        }
    }

    #[test]
    fn test_fetch_docs_item_invalid_crate() {
        let (found, msg) = fetch_docs_item("bad crate!", "item");
        assert!(!found);
        assert!(msg.contains("Invalid crate name"));
    }

    #[test]
    fn test_fetch_docs_item_empty_item_delegates_to_summary() {
        let (_, msg) = fetch_docs_item("totally_nonexistent_crate_xyz_123", "");
        assert!(!msg.contains("Invalid crate name"));
    }
}
