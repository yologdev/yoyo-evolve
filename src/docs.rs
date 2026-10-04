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

/// What a docs.rs response means. Three outcomes, kept apart on purpose: "could not
/// check" (`Unreachable`) must never read as "does not exist" (`NotFound`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocsLookup {
    Found,
    NotFound,
    Unreachable,
}

/// Classify a docs.rs response by its HTTP status first; the body is only a secondary
/// signal. Before Day 218 only body strings decided, and docs.rs's current 404 page
/// ("The requested crate does not exist") matched none of them, so every 404 read as found.
/// `status == 0` means curl failed or gave no code.
fn classify_docs_response(status: u16, body: &str) -> DocsLookup {
    match status {
        404 | 410 => DocsLookup::NotFound,
        200..=299 => {
            if body.trim().is_empty() {
                DocsLookup::Unreachable
            } else if body.contains("This crate does not exist")
                || body.contains("failed to build")
                || body.contains("The requested resource does not exist")
            {
                DocsLookup::NotFound
            } else {
                DocsLookup::Found
            }
        }
        _ => DocsLookup::Unreachable,
    }
}

/// Split curl's `-w '\n%{http_code}'` trailer off its output. Uses `rsplit_once`, never
/// a byte index. Missing or unparseable trailer → status 0 (unreachable), never a guess.
fn split_status_trailer(raw: &str) -> (&str, u16) {
    match raw.rsplit_once('\n') {
        Some((body, code)) => (body, code.trim().parse::<u16>().unwrap_or(0)),
        None => (raw, 0),
    }
}

/// Turn a classified response into the body (found) or the error message the callers
/// already understand ("not found ..." / "Could not reach ...").
fn docs_body_for(status: u16, body: &str) -> Result<String, String> {
    match classify_docs_response(status, body) {
        DocsLookup::Found => Ok(body.to_string()),
        DocsLookup::NotFound if status == 404 || status == 410 => {
            Err(format!("not found on docs.rs (HTTP {status})"))
        }
        DocsLookup::NotFound => Err("not found on docs.rs".to_string()),
        DocsLookup::Unreachable if (200..=299).contains(&status) || status == 0 => {
            Err("Could not reach docs.rs".to_string())
        }
        DocsLookup::Unreachable => Err(format!(
            "Could not reach docs.rs (HTTP {status}: lookup not completed)"
        )),
    }
}

/// Fetch HTML from a docs.rs URL. Returns Ok(body) or Err(message).
fn fetch_docs_html(url: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args(["-sL", "--max-time", "10", "-w", "\n%{http_code}", url])
        .output()
        .map_err(|e| format!("Error fetching docs: {e}"))?;

    let raw = String::from_utf8_lossy(&output.stdout);
    let (body, status) = split_status_trailer(&raw);
    // A curl failure (DNS, timeout mid-body) is "could not check", whatever code it saw.
    let status = if output.status.success() { status } else { 0 };
    docs_body_for(status, body)
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

/// Build the display output for a docs.rs page given its URL, description, and item listing.
/// Shared by `fetch_docs_summary` and `fetch_docs_item`.
fn build_docs_display(url: &str, description: Option<String>, items_display: &str) -> String {
    let mut summary = format!("  📦 {url}\n");
    if let Some(desc) = description {
        summary.push_str(&format!("  📝 {desc}\n"));
    }
    if !items_display.is_empty() {
        summary.push_str(&format!("\n{items_display}"));
    } else if !summary.contains("📝") {
        summary.push_str("  Docs available at the URL above.");
    }
    summary
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

    summary_outcome(crate_name, &url, fetch_docs_html(&url))
}

/// Pure half of `fetch_docs_summary`: what the caller receives for a fetch result.
/// `found == false` is what keeps the ✓ off the screen in `handle_docs`.
fn summary_outcome(crate_name: &str, url: &str, fetched: Result<String, String>) -> (bool, String) {
    let body = match fetched {
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

    (true, build_docs_display(url, description, &items_display))
}

/// Build the user-facing message for a failed item fetch.
///
/// A transport failure and a genuine "item not found" are different facts and must not
/// reach the user as one sentence: telling someone their item does not exist in the crate
/// when the machine is offline (or `curl` is missing, or docs.rs returned a 500) sends them
/// to check their spelling for a problem that is not theirs. `fetch_docs_summary` already
/// separates the two; this is the same separation for the item path.
fn item_fetch_error_message(err: &str, crate_name: &str, item: &str) -> String {
    if err.contains("Could not reach") || err.contains("Error fetching docs") {
        format!("{err} (looking up '{item}' in '{crate_name}')")
    } else {
        // Carry the status suffix (" (HTTP 404)") when the status decided; the legacy
        // body-match error has none, so its message stays byte-identical.
        let status = err.strip_prefix("not found on docs.rs").unwrap_or("");
        format!("Item '{item}' not found in crate '{crate_name}' on docs.rs{status}")
    }
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

    item_outcome(crate_name, item, &url, fetch_docs_html(&url))
}

/// Pure half of `fetch_docs_item`: what the caller receives for a fetch result.
fn item_outcome(
    crate_name: &str,
    item: &str,
    url: &str,
    fetched: Result<String, String>,
) -> (bool, String) {
    let body = match fetched {
        Ok(body) => body,
        Err(e) => return (false, item_fetch_error_message(&e, crate_name, item)),
    };

    let description = extract_meta_description(&body);
    let items = parse_docs_items(&body);
    let items_display = format_docs_items(&items, 10);

    (true, build_docs_display(url, description, &items_display))
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

    let desc = crate::format::decode_html_entities(desc);

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

    // ── build_docs_display ──────────────────────────────────────────

    #[test]
    fn test_build_docs_display_with_desc_and_items() {
        let result = build_docs_display(
            "https://docs.rs/serde/latest/serde/",
            Some("A serialization framework".to_string()),
            "  Modules: de, ser",
        );
        assert!(result.contains("📦 https://docs.rs/serde/latest/serde/"));
        assert!(result.contains("📝 A serialization framework"));
        assert!(result.contains("Modules: de, ser"));
    }

    #[test]
    fn test_build_docs_display_with_desc_no_items() {
        let result = build_docs_display(
            "https://docs.rs/serde/latest/serde/",
            Some("A serialization framework".to_string()),
            "",
        );
        assert!(result.contains("📝 A serialization framework"));
        assert!(!result.contains("Docs available at the URL above."));
    }

    #[test]
    fn test_build_docs_display_no_desc_no_items() {
        let result = build_docs_display("https://docs.rs/serde/latest/serde/", None, "");
        assert!(result.contains("📦"));
        assert!(result.contains("Docs available at the URL above."));
    }

    #[test]
    fn test_build_docs_display_no_desc_with_items() {
        let result = build_docs_display(
            "https://docs.rs/serde/latest/serde/",
            None,
            "  Structs: Foo",
        );
        assert!(!result.contains("📝"));
        assert!(result.contains("Structs: Foo"));
        assert!(!result.contains("Docs available at the URL above."));
    }

    // The two tests below pin `item_fetch_error_message`, which is the whole of what
    // `fetch_docs_item` returns to its caller on the error path (the call site is
    // `Err(e) => return (false, item_fetch_error_message(&e, crate_name, item))` and does
    // nothing else to the string). Stated plainly: they do NOT drive a live fetch — that
    // needs the network — so they assert the message a caller receives, one call short of
    // the process boundary.

    #[test]
    fn test_item_fetch_transport_failure_is_not_reported_as_a_missing_item() {
        // Offline / DNS failure / 5xx: `fetch_docs_html` returns "Could not reach docs.rs".
        let msg = item_fetch_error_message("Could not reach docs.rs", "tokio", "task");
        assert!(
            !msg.contains("not found in crate"),
            "a transport failure must not tell the user the item does not exist: {msg}"
        );
        assert_eq!(
            msg, "Could not reach docs.rs (looking up 'task' in 'tokio')",
            "the reachability failure and the crate/item being looked up are both named"
        );

        // curl missing entirely: the spawn error is transport too, not a missing item.
        let spawn = item_fetch_error_message(
            "Error fetching docs: No such file or directory (os error 2)",
            "tokio",
            "task",
        );
        assert!(!spawn.contains("not found in crate"), "{spawn}");
        assert!(spawn.contains("Error fetching docs"), "{spawn}");
    }

    #[test]
    fn test_item_fetch_genuine_not_found_keeps_its_message() {
        // The pass-through half: a real "not found" is byte-identical to the pre-fix message,
        // so separating transport failures did not cost the common case its wording.
        let msg = item_fetch_error_message("not found on docs.rs", "serde", "Serialize");
        assert_eq!(
            msg,
            "Item 'Serialize' not found in crate 'serde' on docs.rs"
        );
    }

    // ── HTTP status, not page prose (Day 218) ─────────────────────────────────
    // Before this, `fetch_docs_html` ran `curl -sL` with no status check and recognised
    // not-found only by three body strings. docs.rs's current 404 page says "The requested
    // crate does not exist", which matches none of them, so every 404 printed a green ✓.

    const CURRENT_DOCSRS_404_BODY: &str =
        "<html><head><title>The requested crate does not exist</title></head>\
         <body><h1>The requested crate does not exist</h1></body></html>";
    const FOUND_PAGE: &str = "<html><head>\
        <meta name=\"description\" content=\"A generic serialization framework\">\
        </head><body>\
        <a class=\"mod\" href=\"de/index.html\" title=\"mod serde::de\">de</a>\
        <a class=\"trait\" href=\"trait.Serialize.html\" title=\"trait serde::Serialize\">Serialize</a>\
        </body></html>\n";

    #[test]
    fn test_classify_docs_response_table() {
        // Anti-vacuous: the current 404 body really matches none of the legacy strings,
        // so the 404 row below is decided by status and not by a body match.
        for legacy in [
            "This crate does not exist",
            "failed to build",
            "The requested resource does not exist",
        ] {
            assert!(!CURRENT_DOCSRS_404_BODY.contains(legacy), "{legacy}");
        }
        let rows: &[(u16, &str, DocsLookup)] = &[
            (404, CURRENT_DOCSRS_404_BODY, DocsLookup::NotFound),
            (404, "", DocsLookup::NotFound),
            (410, "gone", DocsLookup::NotFound),
            // Near miss: a normal crate page must stay Found.
            (200, FOUND_PAGE, DocsLookup::Found),
            // Legacy body path, unchanged.
            (
                200,
                "<p>This crate does not exist</p>",
                DocsLookup::NotFound,
            ),
            (
                200,
                "<p>docs for this crate failed to build</p>",
                DocsLookup::NotFound,
            ),
            (
                200,
                "<p>The requested resource does not exist</p>",
                DocsLookup::NotFound,
            ),
            // Could not check is not the same as not found.
            (0, "", DocsLookup::Unreachable),
            (0, FOUND_PAGE, DocsLookup::Unreachable),
            (500, CURRENT_DOCSRS_404_BODY, DocsLookup::Unreachable),
            (503, "", DocsLookup::Unreachable),
            (429, "slow down", DocsLookup::Unreachable),
            (200, "", DocsLookup::Unreachable),
        ];
        for (status, body, want) in rows {
            assert_eq!(
                classify_docs_response(*status, body),
                *want,
                "status {status}, body {body:?}"
            );
        }
    }

    #[test]
    fn test_split_status_trailer() {
        // The body is exactly what the server sent; only the "\n<code>" curl appended is removed.
        assert_eq!(
            split_status_trailer("<html>é</html>\n\n200"),
            ("<html>é</html>\n", 200)
        );
        assert_eq!(split_status_trailer("\n404"), ("", 404));
        assert_eq!(split_status_trailer("\n000"), ("", 0));
        // No trailer or garbage: status 0, never a guessed success.
        assert_eq!(split_status_trailer(""), ("", 0));
        assert_eq!(split_status_trailer("body\nnotanumber"), ("body", 0));
    }

    #[test]
    fn test_crate_404_is_a_refusal_naming_the_crate_and_the_status() {
        let url = "https://docs.rs/zzqq/latest/zzqq/";
        let (found, msg) =
            summary_outcome("zzqq", url, docs_body_for(404, CURRENT_DOCSRS_404_BODY));
        assert!(
            !found,
            "a 404 must not be reported as found (that is what prints the ✓)"
        );
        assert_eq!(msg, "Crate 'zzqq' not found on docs.rs (HTTP 404)");
        assert!(!msg.contains("Docs available"), "{msg}");
    }

    #[test]
    fn test_item_404_is_a_refusal_naming_crate_item_and_status() {
        let url = "https://docs.rs/serde/latest/serde/NoSuchItemXyz/";
        let (found, msg) = item_outcome("serde", "NoSuchItemXyz", url, docs_body_for(404, ""));
        assert!(!found);
        assert_eq!(
            msg,
            "Item 'NoSuchItemXyz' not found in crate 'serde' on docs.rs (HTTP 404)"
        );
    }

    #[test]
    fn test_unreachable_is_not_reported_as_not_found() {
        let url = "https://docs.rs/serde/latest/serde/";
        let (found, msg) = summary_outcome("serde", url, docs_body_for(0, ""));
        assert!(!found);
        assert_eq!(msg, "Could not reach docs.rs for 'serde'");

        let (found, msg) = summary_outcome("serde", url, docs_body_for(503, ""));
        assert!(!found);
        assert_eq!(
            msg,
            "Could not reach docs.rs (HTTP 503: lookup not completed) for 'serde'"
        );
        assert!(!msg.contains("not found"), "{msg}");

        let (found, msg) = item_outcome("serde", "de", url, docs_body_for(500, ""));
        assert!(!found);
        assert!(!msg.contains("not found in crate"), "{msg}");
        assert_eq!(
            msg,
            "Could not reach docs.rs (HTTP 500: lookup not completed) (looking up 'de' in 'serde')"
        );
    }

    #[test]
    fn test_found_page_renders_byte_identically() {
        let url = "https://docs.rs/serde/latest/serde/";
        let (found, msg) = summary_outcome("serde", url, docs_body_for(200, FOUND_PAGE));
        assert!(found);
        // Full string: the found path is the regression surface and must not drift.
        assert_eq!(
            msg,
            "  📦 https://docs.rs/serde/latest/serde/\n  📝 A generic serialization framework\n\n  Modules: de\n  Traits: Serialize"
        );
        let (found_item, item_msg) =
            item_outcome("serde", "de", url, docs_body_for(200, FOUND_PAGE));
        assert!(found_item);
        assert_eq!(item_msg, msg);
    }

    #[test]
    fn test_legacy_body_not_found_keeps_its_old_message() {
        let url = "https://docs.rs/serde/latest/serde/";
        let (found, msg) = summary_outcome(
            "serde",
            url,
            docs_body_for(200, "<p>This crate does not exist</p>"),
        );
        assert!(!found);
        assert_eq!(msg, "Crate 'serde' not found on docs.rs");
    }
}
