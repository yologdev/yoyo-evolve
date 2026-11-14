//! Warn when a loaded config file has a key yoyo never reads (#1171).
//!
//! `parse_config_file` puts every top-level key into a flat map and the
//! readers look up the names they know, so a typo (`modle = "gpt-4o"`) was
//! simply never looked up: the run went ahead on the default model and said
//! nothing. This module names those keys, once per process, on stderr. It
//! **warns, never refuses**: a key from a newer yoyo, or one the user's own
//! tooling reads, must not break the run.
//!
//! "Never reads" is defined by the readers, not by `/config set`'s list:
//! - the flat map is read by name (`file_config.get("model")`, `config_flag`,
//!   the numeric settings, `hooks.<phase>.<tool>` by prefix). Since #1184
//!   slice 2 the map holds TOML's top-level keys only (plus every spelling of
//!   `hooks`), so these names are read only before the first header (until
//!   then the map was flattened across sections);
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
    /// Valid TOML that names one of yoyo's tables in a spelling no reader
    /// reads (#1178): a dotted key, a sub-table, an array of tables. Not a
    /// typo, so no did-you-mean: the warning names the spelling yoyo reads.
    /// `header` findings collect the keys under them, like `Section`.
    Unread {
        line: usize,
        written: String,
        path: String,
        keys: Vec<String>,
        header: bool,
        spelling: String,
        fence: bool,
        /// For an inline table at a table name (#1180), the same table in
        /// the header form yoyo reads, built from the user's own line.
        rewrite: Option<String>,
    },
    /// A section no parser reads, with the keys under it that nothing reads
    /// either (a top-level name under it too, since #1184 slice 2).
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
    let best = closest_match(stem, &TABLE_STEMS, max_edits(stem))?;
    Some(match (best, rest) {
        ("mcp_servers" | "model_pricing", Some(r)) if !r.trim().is_empty() => {
            format!("[{best}.{r}]")
        }
        ("mcp_servers" | "model_pricing", _) => format!("[{best}.<name>]"),
        _ => format!("[{best}]"),
    })
}

/// The tables yoyo reads by header; a valid TOML name under one of them in
/// another spelling is [`UnknownKey::Unread`], not a typo.
const TABLE_STEMS: [&str; 4] = ["permissions", "directories", "mcp_servers", "model_pricing"];

/// For a TOML path (`parts`) under one of [`TABLE_STEMS`], the spelling yoyo
/// does read, and whether the table is a fence. `None` for any other path.
fn spelling_yoyo_reads(parts: &[String]) -> Option<(String, bool)> {
    let stem = parts.first()?.as_str();
    if !TABLE_STEMS.contains(&stem) {
        return None;
    }
    let part = |p: &String| toml_key_part(p);
    Some(match (stem, parts) {
        ("permissions" | "directories", [.., k]) if k == "allow" || k == "deny" => {
            (format!("`[{stem}]` then `{k} = [...]`"), true)
        }
        ("permissions" | "directories", _) => (
            format!("`[{stem}]` then `allow = [...]` / `deny = [...]`"),
            true,
        ),
        ("mcp_servers", [_, name, k]) => {
            (format!("`[{stem}.{}]` then `{k} = ...`", part(name)), false)
        }
        ("mcp_servers", _) => (format!("`[{stem}.<name>]` then `command = ...`"), false),
        ("model_pricing", [_, id, k]) => {
            (format!("`[{stem}.{}]` then `{k} = ...`", part(id)), false)
        }
        ("model_pricing", _) => (format!("`[{stem}.<id>]` then `input = ...`"), false),
        _ => return None,
    })
}

/// The tables `config_toml_read` reads through the `toml` crate (#1182), so
/// any TOML spelling of them is read; the detector judges a line in them by
/// the full path it sets and the value's type, never by its header form.
const PARSED_TABLES: [&str; 2] = ["mcp_servers", "model_pricing"];

fn is_parsed_table_path(path: &[String]) -> bool {
    path.first()
        .is_some_and(|s| PARSED_TABLES.contains(&s.as_str()))
}

/// The keys read inside one entry of a parsed table.
fn parsed_entry_keys(stem: &str) -> &'static [&'static str] {
    if stem == "mcp_servers" {
        &["command", "args", "env"]
    } else {
        &["input", "output"]
    }
}

/// A header under a parsed table that opens a table the reader reads:
/// `[t]`, `[t.<name>]` and `[mcp_servers.<name>.env]`.
fn parsed_header_is_read(path: &[String]) -> bool {
    match path {
        [_] | [_, _] => true,
        [stem, _, k] => stem == "mcp_servers" && k == "env",
        _ => false,
    }
}

/// What the reader needs at a parsed-table path, for the warning.
fn parsed_spelling(path: &[String]) -> String {
    let stem = path[0].as_str();
    let entry = match path.get(1) {
        Some(n) => format!("`[{stem}.{}]`", toml_key_part(n)),
        None => format!("`[{stem}.<name>]`"),
    };
    let what = match path.get(2).map(String::as_str) {
        Some("command") => "`command = \"<program>\"` (a string); the server is not started",
        Some("args") => "`args = [\"...\"]` (a list of strings); the server is not started",
        Some("env") => {
            "`env = { NAME = \"value\" }` (a table of strings); the server is not started"
        }
        Some("input" | "output") => {
            "`input = 1.5` / `output = 6.0` (numbers); the price is dropped"
        }
        _ if stem == "mcp_servers" => "`command = ...` (a server is a table)",
        _ => "`input = ...` / `output = ...` (a price is a table)",
    };
    format!("{entry} then {what}")
}

/// Judge the line at `line` that sets `path` (under a parsed table) to
/// `value` (`None` when the value alone is not readable, e.g. it spans
/// lines), recursing into inline tables, and push what the reader ignores.
fn judge_parsed_path(
    path: &[String],
    value: Option<&toml::Value>,
    line: usize,
    written: &str,
    out: &mut Vec<UnknownKey>,
) {
    use toml::Value;
    let unread = |out: &mut Vec<UnknownKey>| {
        out.push(UnknownKey::Unread {
            line,
            written: written.to_string(),
            path: path
                .iter()
                .map(|p| toml_key_part(p))
                .collect::<Vec<_>>()
                .join("."),
            keys: Vec::new(),
            header: false,
            spelling: parsed_spelling(path),
            fence: false,
            rewrite: None,
        })
    };
    let recurse = |t: &toml::Table, out: &mut Vec<UnknownKey>| {
        for (k, v) in t {
            let p: Vec<String> = path.iter().cloned().chain([k.clone()]).collect();
            judge_parsed_path(&p, Some(v), line, written, out);
        }
    };
    let Some(value) = value else { return };
    let stem = path[0].as_str();
    match path.len() {
        1 | 2 => match value {
            Value::Table(t) => recurse(t, out),
            _ => unread(out),
        },
        3 => {
            let k = path[2].as_str();
            let keys = parsed_entry_keys(stem);
            if !keys.contains(&k) {
                out.push(UnknownKey::Key {
                    line,
                    key: k.to_string(),
                    section: Some(format!("{stem}.{}", toml_key_part(&path[1]))),
                    suggestion: closest_match(k, keys, max_edits(k)).map(String::from),
                });
                return;
            }
            let ok = match (k, value) {
                ("command", Value::String(_)) => true,
                ("args", Value::Array(a)) => !a.iter().any(|v| v.is_array() || v.is_table()),
                ("env", Value::Table(t)) => return recurse(t, out),
                ("input" | "output", Value::Integer(_) | Value::Float(_)) => true,
                _ => false,
            };
            if !ok {
                unread(out);
            }
        }
        4 if stem == "mcp_servers" && path[2] == "env" => {
            if value.is_array() || value.is_table() {
                unread(out);
            }
        }
        _ => unread(out),
    }
}

/// One TOML key part as written in a header: bare when it can be, else quoted.
fn toml_key_part(p: &str) -> String {
    if toml_key_segments(p).is_some_and(|s| s.len() == 1 && s[0] == p) {
        p.to_string()
    } else {
        format!("\"{}\"", p.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// Byte offsets in `text` of each `stop` char outside quotes, `[]` and `{}`
/// (`None` if a quote or bracket is left open).
fn top_level_positions(text: &str, stop: char) -> Option<Vec<usize>> {
    let (mut depth, mut quote, mut escaped) = (0i32, None::<char>, false);
    let mut out = Vec::new();
    for (i, c) in text.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                _ if c == stop && depth == 0 => out.push(i),
                _ => {}
            },
        }
    }
    (quote.is_none() && depth == 0).then_some(out)
}

/// The inside of `value` when the whole value is one inline table closed on
/// this line (`{ ... }`, a trailing comment allowed).
fn inline_table_inner(value: &str) -> Option<&str> {
    let inner = value.strip_prefix('{')?;
    // The closing brace is the first top-level `}` once the `{` is counted.
    let (mut depth, mut quote, mut escaped) = (1i32, None::<char>, false);
    for (i, c) in inner.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '[' | '{' => depth += 1,
                ']' | '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let rest = inner[i + 1..].trim();
                        return (rest.is_empty() || rest.starts_with('#')).then_some(&inner[..i]);
                    }
                }
                _ => {}
            },
        }
    }
    None
}

/// The inline table `value` set at `path`, rewritten in the header form
/// yoyo's readers read (`[path]` then one key per line, sub-tables after),
/// keeping the user's own keys and values. `None` if it is not one inline
/// table on one line, so the warning falls back to naming the spelling.
fn inline_table_rewrite(path: &[String], value: &str) -> Option<String> {
    fn table(path: &[String], value: &str, out: &mut Vec<String>) -> Option<()> {
        let inner = inline_table_inner(value)?;
        let (mut keys, mut subs) = (Vec::new(), Vec::new());
        let mut start = 0;
        let commas = top_level_positions(inner, ',')?;
        for end in commas.into_iter().chain([inner.len()]) {
            let pair = inner[start..end].trim();
            start = (end + 1).min(inner.len());
            if pair.is_empty() {
                continue;
            }
            let eq = *top_level_positions(pair, '=')?.first()?;
            let (k, v) = (pair[..eq].trim(), pair[eq + 1..].trim());
            let segs = toml_key_segments(k)?;
            if v.starts_with('{') {
                subs.push((path.iter().cloned().chain(segs).collect::<Vec<_>>(), v));
            } else {
                keys.push(format!("{k} = {v}"));
            }
        }
        if !keys.is_empty() {
            let header: Vec<String> = path.iter().map(|p| toml_key_part(p)).collect();
            out.push(format!("[{}]", header.join(".")));
            out.append(&mut keys);
        }
        for (p, v) in subs {
            table(&p, v, out)?;
        }
        Some(())
    }
    let mut out = Vec::new();
    table(path, value, &mut out)?;
    (!out.is_empty()).then(|| out.join("\n"))
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

/// The one header reader every section parser of the config file goes through
/// (#1174): the text between the outer `[` and `]` of a header line, or `None`
/// for a line that is not a header. A trailing `# comment` is legal TOML and is
/// dropped first, but only outside quotes, so `[mcp_servers."a#b"]` keeps its
/// name. `[[t]]` yields `[t]`, as the old `ends_with(']')` readers saw it, and
/// `[x] y = 1` (not TOML) is still `None`. Before #1174 each parser required the
/// line to END in `]`, so `[directories] # keep out` was no header at all and
/// its `deny` list was dropped: a fence that failed open.
///
/// The name comes back the way TOML reads it (#1178): `[ directories ]`,
/// `["directories"]` and `[mcp_servers . "fs"]` are `directories` and
/// `mcp_servers.fs` (parts unquoted, joined by `.`). Text that is not a TOML
/// key comes back trimmed, so the unknown-section warning still names it.
pub(crate) fn toml_section_header(line: &str) -> Option<String> {
    let inner = header_inner(line)?;
    if inner.starts_with('[') && inner.ends_with(']') {
        // An array table: the readers have always seen `[t]`; unchanged.
        return Some(inner.to_string());
    }
    Some(toml_key(inner).unwrap_or_else(|| inner.trim().to_string()))
}

/// The text between a header line's brackets, as written.
fn header_inner(line: &str) -> Option<&str> {
    let line = line.trim();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut end = line.len();
    for (i, c) in line.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '#' => {
                    end = i;
                    break;
                }
                _ => {}
            },
        }
    }
    let head = line[..end].trim_end();
    if head.len() < 2 || !head.starts_with('[') || !head.ends_with(']') {
        return None;
    }
    Some(&head[1..head.len() - 1])
}

/// The parts of a TOML key as TOML reads them (#1178): bare
/// (`A-Za-z0-9_-`), basic (`"..."`, escapes undone) or literal (`'...'`)
/// parts joined by `.`, whitespace allowed around each. `None` if `s` is
/// not a TOML key.
pub(crate) fn toml_key_segments(s: &str) -> Option<Vec<String>> {
    let mut segs = Vec::new();
    let mut rest = s.trim();
    loop {
        let (seg, after) = if let Some(body) = rest.strip_prefix('"') {
            // `"` and `\` are ASCII, so every index compared here is a
            // char boundary.
            let bytes = body.as_bytes();
            let mut i = 0;
            while i < bytes.len() && bytes[i] != b'"' {
                i += if bytes[i] == b'\\' { 2 } else { 1 };
            }
            if i >= bytes.len() {
                return None;
            }
            let seg = crate::config_value::unescape_toml_basic_string(&body[..i]);
            (seg, &body[i + 1..])
        } else if let Some(body) = rest.strip_prefix('\'') {
            let end = body.find('\'')?;
            (body[..end].to_string(), &body[end + 1..])
        } else {
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(rest.len());
            if end == 0 {
                return None;
            }
            (rest[..end].to_string(), &rest[end..])
        };
        segs.push(seg);
        let after = after.trim_start();
        if after.is_empty() {
            return Some(segs);
        }
        rest = after.strip_prefix('.')?.trim_start();
    }
}

/// A TOML key (or header) as one name: its parts joined by `.`.
pub(crate) fn toml_key(s: &str) -> Option<String> {
    toml_key_segments(s).map(|segs| segs.join("."))
}

/// The name a `key = value` line (`raw` is the text before `=`) has in the
/// flat config map: a one-part key unquoted (`"model"` is `model`), anything
/// else as written. A dotted key keeps its spelling on purpose: the flat map
/// matches `hooks.pre.bash` as text, and `"hooks.pre.bash"` (one quoted part)
/// is not that key to TOML, so unquoting it would invent a hook.
pub(crate) fn config_key_name(raw: &str) -> String {
    match toml_key_segments(raw).as_deref() {
        Some([one]) if !one.contains('.') => one.clone(),
        _ => raw.trim().to_string(),
    }
}

/// The key a line sets inside `table`, TOML's way (#1178): under a
/// `[table]` header a one-part key, at top level a `table.key` dotted key.
/// `section` is the current header from [`toml_section_header`] (`None` at
/// top level). `None` when the line sets anything else.
pub(crate) fn key_in_table(section: Option<&str>, raw_key: &str, table: &str) -> Option<String> {
    let segs = toml_key_segments(raw_key)?;
    match (section, segs.as_slice()) {
        (Some(s), [k]) if s == table => Some(k.clone()),
        (None, [t, k]) if t == table => Some(k.clone()),
        _ => None,
    }
}

/// The header being read: its name, the keys its parser reads (`None` if no
/// parser reads it), and its parts as TOML reads them.
type OpenSection = (String, Option<&'static [&'static str]>, Vec<String>);

/// Every key (and section) in `contents` that no reader of the config file
/// looks at. Empty for a file that only uses keys yoyo reads.
pub(crate) fn unknown_config_keys(contents: &str) -> Vec<UnknownKey> {
    let mut out: Vec<UnknownKey> = Vec::new();
    // Current header (inner text) and whether a parser reads it.
    let mut section: Option<OpenSection> = None;
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
            // The section parsers' own reader (#1174). Anything else starting
            // with `[` is config_syntax's business, not a key.
            if let Some(header) = toml_section_header(line) {
                // An array table (`[[t]]`, read as `[t]`) is no table yoyo reads.
                let array = header.starts_with('[');
                let inner = header
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string();
                let raw = header_inner(line).unwrap_or_default();
                let raw = raw.strip_prefix('[').and_then(|r| r.strip_suffix(']'));
                let parts =
                    toml_key_segments(raw.unwrap_or(header_inner(line).unwrap_or_default()));
                let keys = match parts.as_deref() {
                    // #1182: these tables are read through the toml crate, so
                    // the header is judged by the path it opens.
                    Some(p) if is_parsed_table_path(p) => {
                        (!array && parsed_header_is_read(p)).then_some(&[][..])
                    }
                    _ if array => None,
                    // #1184: every TOML spelling of `hooks` reaches the hook
                    // reader, which judges its keys itself.
                    Some([stem, ..]) if stem == "hooks" => Some(&[][..]),
                    _ => section_keys(&inner),
                };
                open_section = None;
                if keys.is_none() {
                    let unread = parts.as_deref().and_then(spelling_yoyo_reads);
                    out.push(match unread {
                        Some((spelling, fence)) => UnknownKey::Unread {
                            line: idx + 1,
                            written: line.to_string(),
                            path: header.clone(),
                            keys: Vec::new(),
                            header: true,
                            spelling,
                            fence,
                            rewrite: None,
                        },
                        None => UnknownKey::Section {
                            line: idx + 1,
                            header: line.to_string(),
                            keys: Vec::new(),
                            suggestion: section_suggestion(&inner),
                        },
                    });
                    open_section = Some(out.len() - 1);
                }
                section = Some((inner, keys, parts.unwrap_or_default()));
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
        } else if value.starts_with('[') || value.starts_with('{') {
            // An inline table may hold a multi-line array too (#1180).
            array_depth = bracket_balance(value).max(0);
        }
        let name = config_key_name(key);
        // #1184: only a key before the first header is a top-level setting;
        // under a header it is that table's key, judged below.
        if key.is_empty() || (section.is_none() && is_top_level_key(&name)) {
            continue;
        }
        // The whole TOML path this line sets, when it is a TOML key.
        let segs = toml_key_segments(key);
        let here = section.as_ref().map_or(&[][..], |(_, _, p)| p.as_slice());
        let stem = here.first().or(segs.as_ref().and_then(|s| s.first()));
        let one_part = here.is_empty() && segs.as_ref().is_some_and(|s| s.len() == 1);
        if stem.is_some_and(|s| s == "hooks") && (!one_part || value.starts_with('{')) {
            continue; // a hook, in any spelling; the hook reader judges it
        }
        // #1182: under mcp_servers / model_pricing every TOML spelling is
        // read, so judge the full path and value, not the header form.
        let base = match &section {
            None => Some(Vec::new()),
            Some((_, Some(_), p)) if is_parsed_table_path(p) => Some(p.clone()),
            Some(_) => None,
        };
        if let (Some(base), Some(segs)) = (base, segs.as_ref()) {
            let path: Vec<String> = base.into_iter().chain(segs.iter().cloned()).collect();
            if is_parsed_table_path(&path) {
                let parsed = format!("v = {value}").parse::<toml::Table>().ok();
                let shown = format!("{key} = {value}");
                judge_parsed_path(
                    &path,
                    parsed.as_ref().and_then(|t| t.get("v")),
                    idx + 1,
                    &shown,
                    &mut out,
                );
                continue;
            }
        }
        // An inline table at one of yoyo's table names (#1180): valid TOML
        // the readers never see, so name it, and for a fence say it is off.
        let inline = section.is_none() && value.starts_with('{');
        if let Some(path) = segs.as_ref().filter(|_| inline) {
            if let Some((spelling, fence)) = spelling_yoyo_reads(path) {
                let shown: Vec<String> = path.iter().map(|p| toml_key_part(p)).collect();
                out.push(UnknownKey::Unread {
                    line: idx + 1,
                    written: format!("{key} = {{ ... }}"),
                    path: shown.join("."),
                    keys: Vec::new(),
                    header: false,
                    spelling,
                    fence,
                    rewrite: inline_table_rewrite(path, value),
                });
                continue;
            }
        }
        if let Some(segs) = segs.as_ref().filter(|s| s.len() > 1) {
            let here = section
                .as_ref()
                .map(|(_, _, p)| p.clone())
                .unwrap_or_default();
            let path: Vec<String> = here.into_iter().chain(segs.iter().cloned()).collect();
            let read_by_a_fence = section.is_none()
                && ["permissions", "directories"].into_iter().any(|t| {
                    key_in_table(None, key, t).is_some_and(|k| k == "allow" || k == "deny")
                });
            if read_by_a_fence {
                continue;
            }
            if let Some((spelling, fence)) = spelling_yoyo_reads(&path) {
                out.push(UnknownKey::Unread {
                    line: idx + 1,
                    written: key.to_string(),
                    path: path.join("."),
                    keys: Vec::new(),
                    header: false,
                    spelling,
                    fence,
                    rewrite: None,
                });
                continue;
            }
        }
        let key = name.as_str();
        match &section {
            None => out.push(UnknownKey::Key {
                line: idx + 1,
                key: key.to_string(),
                section: None,
                suggestion: closest_match(key, TOP_LEVEL_KEYS, max_edits(key)).map(String::from),
            }),
            Some((name, Some(keys), _)) => {
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
            Some((_, None, _)) => {
                if let Some(UnknownKey::Section { keys, .. } | UnknownKey::Unread { keys, .. }) =
                    open_section.and_then(|i| out.get_mut(i))
                {
                    keys.push(key.to_string());
                }
            }
        }
    }
    // An unread section with nothing unread under it costs the user nothing.
    out.retain(|f| {
        !matches!(f, UnknownKey::Section { keys, .. }
            | UnknownKey::Unread { keys, header: true, .. } if keys.is_empty())
    });
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
                    // #1184: a top-level key under a header is that table's key.
                    Some(s) if s == key && section.is_some() => {
                        "; it is a top-level key, so put it above the first `[section]` header"
                            .to_string()
                    }
                    Some(s) => format!("; did you mean `{s}`?"),
                    None => String::new(),
                };
                format!(
                    "{shown} line {line}: `{key}`{place} is not a key yoyo reads, so it has no effect{hint}"
                )
            }
            UnknownKey::Unread {
                line,
                written,
                path,
                keys,
                header,
                spelling,
                fence,
                rewrite,
            } => {
                let fence = if *fence {
                    " This rule is NOT in force."
                } else {
                    ""
                };
                let what = if *header {
                    format!("does not read `[{path}]`, so its keys ({}) have no effect", keys.join(", "))
                } else {
                    format!("does not read that spelling of `{path}`, so it has no effect")
                };
                let rewrite = match rewrite {
                    Some(r) => format!(" Write it as:\n    {}", r.replace('\n', "\n    ")),
                    None => String::new(),
                };
                format!("{shown} line {line}: `{written}` is valid TOML, but yoyo {what}.{fence} yoyo reads {spelling}.{rewrite}")
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
                UnknownKey::Unread { written, .. } => written,
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
    fn a_hooks_table_is_read_in_every_toml_spelling() {
        // Until #1184 slice 2 only the dotted `hooks.pre.bash` line was read
        // and `[hooks]` was warned as unread. The flat map is now built from
        // TOML's table, where every spelling is the same `hooks.pre.bash`.
        for doc in [
            "[hooks]\npre.bash = \"echo\"\n",
            "[hooks.pre]\nbash = \"echo\"\n",
            "hooks = { pre = { bash = \"echo\" } }\n",
            "\"hooks\".pre.bash = \"echo\"\n",
        ] {
            assert_eq!(keys(doc), Vec::<String>::new(), "{doc:?}");
            let map = crate::config::parse_config_file(doc);
            assert_eq!(map.get("hooks.pre.bash").map(String::as_str), Some("echo"));
        }
    }

    #[test]
    fn a_top_level_name_under_a_section_is_named_because_it_is_not_read() {
        // Until #1184 slice 2 the flat map took it (and it overrode the real
        // top-level key), so it was read and this list was empty.
        assert_eq!(
            keys("[whatever]\nmodel = \"x\"\n"),
            vec!["[whatever]".to_string()]
        );
        assert_eq!(
            keys("[permissions]\nmodel = \"x\"\n"),
            vec!["model".to_string()]
        );
        assert_eq!(
            keys("[x]\nhooks.pre.bash = \"x\"\n"),
            vec!["[x]".to_string()]
        );
        let found = unknown_config_keys("[permissions]\nprovider = \"openai\"\n");
        assert_eq!(
            unknown_config_key_warnings("cfg", &found),
            vec![
                "cfg line 2: `provider` (in [permissions]) is not a key yoyo reads, so it has \
                 no effect; it is a top-level key, so put it above the first `[section]` header"
                    .to_string()
            ]
        );
        for doc in [
            "[whatever]\nmodel = \"x\"\n",
            "[permissions]\nmodel = \"x\"\n",
        ] {
            assert_eq!(crate::config::parse_config_file(doc).get("model"), None);
        }
    }

    #[test]
    fn multi_line_arrays_and_strings_are_not_read_as_keys() {
        let text = "allow_x = 1\n\
                    mcp = [\n  \"FOO=bar cmd\",\n  \"x\",\n]\n\
                    system_prompt = \"\"\"\nmodle = inside a string\n\"\"\"\n";
        assert_eq!(keys(text), vec!["allow_x".to_string()]);
    }

    /// Inverted by #1174: this test used to be
    /// `a_header_with_a_trailing_comment_is_not_a_header_to_the_parsers` and
    /// pinned the parser bug (the `allow` reported as an unread top-level key).
    #[test]
    fn a_header_with_a_trailing_comment_is_a_header_to_the_parsers() {
        assert_eq!(
            unknown_config_keys("[permissions] # mine\nallow = [\"x\"]\n"),
            Vec::new()
        );
        assert_eq!(
            unknown_config_keys("[directories] # keep out\ndeny = [\"/tmp\"]\n"),
            Vec::new()
        );
        // The section is still checked: a typo under it is still named.
        assert!(matches!(
            &unknown_config_keys("[directories] # keep out\ndeyn = [\"/tmp\"]\n")[..],
            [UnknownKey::Key { key, section: Some(s), .. }] if key == "deyn" && s == "directories"
        ));
    }

    #[test]
    fn toml_section_header_table() {
        let rows: &[(&str, Option<&str>)] = &[
            ("[directories]", Some("directories")),
            ("  [directories]  ", Some("directories")),
            ("[directories] # keep out", Some("directories")),
            ("[directories]   #", Some("directories")),
            ("[directories]#tight", Some("directories")),
            // Old predicate: ends in `]`, so a header named `directories] # see [x`.
            ("[directories] # see [x]", Some("directories")),
            // The name as TOML reads it (#1178): parts unquoted and joined.
            ("[mcp_servers.\"a#b\"]", Some("mcp_servers.a#b")),
            ("[mcp_servers.\"a#b\"] # c", Some("mcp_servers.a#b")),
            ("[mcp_servers.'a#b'] # c", Some("mcp_servers.a#b")),
            (
                "[model_pricing.\"a\\\"#b\"] # c",
                Some("model_pricing.a\"#b"),
            ),
            ("[ directories ]", Some("directories")),
            ("[\tdirectories ] # c", Some("directories")),
            ("[\"directories\"]", Some("directories")),
            ("[ mcp_servers . 'my fs' ]", Some("mcp_servers.my fs")),
            // Not a TOML key: trimmed, so the warning can still name it.
            ("[ my table ]", Some("my table")),
            // Array tables: the old readers saw `[t]` as the inner text; unchanged.
            ("[[array]]", Some("[array]")),
            ("[[array]] # c", Some("[array]")),
            ("[x] y = 1", None),
            ("[x] = 1", None),
            ("# [directories]", None),
            ("deny = [\"[a]\"]", None),
            ("[", None),
            ("]", None),
            ("", None),
            ("[ñ] # ✓", Some("ñ")),
        ];
        for (line, want) in rows {
            assert_eq!(toml_section_header(line).as_deref(), *want, "line {line:?}");
        }
    }

    /// End to end through every section parser in config.rs (#1174): a
    /// trailing comment on the header no longer drops the section.
    #[test]
    fn every_section_parser_reads_a_header_with_a_trailing_comment() {
        use crate::config::*;
        let dirs = parse_directories_from_config("[directories] # keep out\ndeny = [\"/tmp\"]\n");
        assert_eq!(dirs.deny, vec!["/tmp".to_string()]);

        let perms = parse_permissions_from_config("[permissions] # mine\ndeny = [\"rm -rf *\"]\n");
        assert_eq!(perms.deny, vec!["rm -rf *".to_string()]);

        let mcp = parse_mcp_servers_from_config(
            "[mcp_servers.fs] # files\ncommand = \"npx\"\nargs = [\"x\"]\n",
        );
        assert_eq!(mcp.len(), 1, "{mcp:?}");
        assert_eq!(
            (mcp[0].name.as_str(), mcp[0].command.as_str()),
            ("fs", "npx")
        );

        let prices = parse_model_pricing_from_config(
            "[model_pricing.\"m\"] # mine\ninput = 1.5\noutput = 2\n",
        );
        let p = prices.lookup("m").expect("priced");
        assert_eq!((p.input_per_million, p.output_per_million), (1.5, 2.0));

        // The writer: before #1174 it did not see the commented header, so it
        // appended a second `[permissions]` table (invalid TOML).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".yoyo.toml");
        std::fs::write(&path, "[permissions] # mine\nallow = [\"a\"]\n").unwrap();
        append_allow_pattern_to("b", &path).unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, "[permissions] # mine\nallow = [\"a\", \"b\"]\n");
    }

    /// #1178, the writers: `append_allow_pattern_to` keeps a dotted
    /// `permissions` table dotted (a new `[permissions]` would declare the
    /// table twice, which TOML refuses), and `set_toml_key` replaces a quoted
    /// key instead of appending a duplicate.
    #[test]
    fn writers_keep_the_users_spelling_of_tables_and_keys() {
        use crate::config::*;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".yoyo.toml");
        for (before, after) in [
            (
                "permissions.deny = [\"rm *\"]\n",
                "permissions.deny = [\"rm *\"]\npermissions.allow = [\"b\"]\n",
            ),
            (
                "permissions.allow = [\"a\"]\n",
                "permissions.allow = [\"a\", \"b\"]\n",
            ),
            (
                "[ permissions ]\n\"allow\" = [\"a\"]\n",
                "[ permissions ]\nallow = [\"a\", \"b\"]\n",
            ),
        ] {
            std::fs::write(&path, before).unwrap();
            append_allow_pattern_to("b", &path).unwrap();
            let got = std::fs::read_to_string(&path).unwrap();
            assert_eq!(got, after, "from {before:?}");
            assert_eq!(
                parse_permissions_from_config(&got)
                    .allow
                    .last()
                    .map(String::as_str),
                Some("b")
            );
        }
        assert_eq!(
            set_toml_key("\"model\" = \"a\"\n", "model", "b"),
            "model = \"b\"\n"
        );
    }

    /// #1178: the flat map reads a quoted `"notify_command"` / `"mcp"` and a
    /// spaced `[ permissions ]` header, so the trust prompt must list them.
    #[test]
    fn trust_prompt_sees_every_spelling_the_readers_read() {
        let grants = |t: &str| crate::config_paths::project_trust_grants(t, false, false, false);
        let notify = "\"notify_command\" = \"touch /tmp/x\"\n";
        assert_eq!(
            crate::config::parse_config_file(notify)
                .get("notify_command")
                .map(String::as_str),
            Some("touch /tmp/x")
        );
        assert!(grants(notify)
            .iter()
            .any(|g| g.starts_with("notify_command")));
        assert!(grants("'mcp' = [\"npx x\"]\n")
            .iter()
            .any(|g| g.starts_with("MCP")));
        assert!(grants("[ mcp_servers . fs ]\ncommand = \"npx\"\n")
            .iter()
            .any(|g| g.starts_with("MCP")));
        assert!(grants("permissions.allow = [\"git *\"]\n")
            .iter()
            .any(|g| g.starts_with("auto-approved")));
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

    /// #1178's corpus: whole documents that vary the HEADER and KEY slots
    /// (the #1177 corpus in config_value.rs holds the key at `k` with no
    /// header, so it only ever certified values). Every row is valid TOML
    /// spelled with names yoyo uses, so no warning may say "did you mean".
    /// Slots varied: whitespace inside brackets, header comment, quoted and
    /// dotted headers, array tables, quoted keys, dotted keys (top level and in a table),
    /// whitespace around `=` and around the key, and (#1180) the key x value
    /// pair neither corpus had: a table name (bare, quoted, dotted) holding an
    /// inline table, nested, with quoted inner keys, a comma inside a string,
    /// a trailing comment and an array spanning lines. Held constant: outside
    /// those rows the value is a one-line string or array (config_value.rs
    /// varies values, but only under the key `k`, so it says nothing about a
    /// value at a name yoyo reads); an inline table never sits under a header
    /// (that slot is now covered for mcp_servers / model_pricing, #1182; a
    /// fence table under its own header holding an inline table is not); keys and
    /// headers are ASCII; no key contains `=`; no table is declared twice.
    const SHAPE_CORPUS: &[&str] = &[
        "[directories]\ndeny = [\"/etc\"]",
        "[ directories ]\ndeny = [\"/etc\"]",
        "[directories ]\ndeny = [\"/etc\"]",
        "[\tdirectories]\ndeny = [\"/etc\"]",
        "[directories] # c\ndeny = [\"/etc\"]",
        "[\"directories\"]\ndeny = [\"/etc\"]",
        "['directories']\ndeny = [\"/etc\"]",
        "[directories.x]\ndeny = [\"/etc\"]",
        "[directories]\n\"deny\" = [\"/etc\"]",
        "[directories]\n'deny' = [\"/etc\"]",
        "[directories]\ndeny=[\"/etc\"]",
        "[directories]\ndeny   =   [\"/etc\"]",
        "[directories]\n   deny   = [\"/etc\"]",
        "directories.deny = [\"/etc\"]",
        "directories . deny = [\"/etc\"]",
        "\"directories\".deny = [\"/etc\"]",
        "directories.'deny' = [\"/etc\"]",
        "\"directories.deny\" = [\"/etc\"]",
        "[directories]\nx.deny = [\"/etc\"]",
        "directories.allow = [\"/w\"]\ndirectories.deny = [\"/etc\"]",
        "[ permissions ]\ndeny = [\"rm *\"]",
        "[permissions]\n\"deny\" = [\"rm *\"]",
        "permissions.deny = [\"rm *\"]",
        "[permissions.x]\ndeny = [\"rm *\"]",
        "[[directories]]\ndeny = [\"/etc\"]",
        "[[ mcp_servers.fs ]]\ncommand = \"npx\"",
        "\"model\" = \"m1\"",
        "'model' = \"m1\"",
        "model= \"m1\"",
        "[ mcp_servers.fs ]\ncommand = \"npx\"",
        "[mcp_servers . fs]\n\"command\" = \"npx\"",
        "[mcp_servers.\"my server\"]\ncommand = \"npx\"",
        "mcp_servers.fs.command = \"npx\"",
        "[mcp_servers]\nfs.command = \"npx\"",
        "[ model_pricing.\"gpt-4.1\" ]\ninput = 1.0\noutput = 2.0",
        "[model_pricing.'m']\n\"input\" = 1.0\n\"output\" = 2.0",
        "model_pricing.m.input = 1.0\nmodel_pricing.m.output = 2.0",
        // #1180: a table-name key x an inline-table value.
        "directories = { deny = [\"/etc\"] }",
        "directories = { \"deny\" = [\"/etc\"] }",
        "directories={allow=[\"/w\"],deny=[\"/etc\"]} # c",
        "\"directories\" = { deny = [\"/etc\"] }",
        "directories = { deny = [\n  \"/etc\",\n] }",
        "permissions = { deny = [\"rm *\"] }",
        "permissions = { allow = [\"git *\"], 'deny' = [\"rm, *\"] }",
        "mcp_servers = { fs = { command = \"npx\" } }",
        "mcp_servers = { \"my server\" = { command = \"npx\", args = [\"-y\"] } }",
        "mcp_servers.fs = { command = \"npx\" }",
        "model_pricing = { m = { input = 1.0, output = 2.0 } }",
        "model_pricing.m = { input = 1.0, output = 2.0 }",
        // #1182: an inline table under a header naming one of yoyo's tables.
        "[mcp_servers]\nfs = { command = \"npx\" }",
        "[mcp_servers] # c\nfs = { command = \"npx\", args = [\"-y\"] } # c",
        "[mcp_servers]\n\"my server\" = { command = \"npx\", env = { A = \"b\" } }",
        "[mcp_servers]\nfs = { command = \"npx\", args = [\n  \"-y\",\n] }",
        "[mcp_servers.fs]\ncommand = \"npx\"\n[mcp_servers.fs.env]\nA = \"b\"",
        "[model_pricing]\nm = { input = 1.0, output = 2.0 }",
        "[model_pricing] # c\n\"gpt-4.1\" = { input = 1, output = 2.5 } # c",
    ];

    /// What tomllib reads for each document, projected onto the things
    /// yoyo's readers build (fence lists, the model, MCP commands, prices).
    fn shape_oracle(docs: &[&str]) -> Vec<serde_json::Value> {
        const SCRIPT: &str = r#"
import json, sys, tomllib
def proj(d):
    out = {}
    for t in ("directories", "permissions"):
        tab = d.get(t)
        if isinstance(tab, dict):
            for k in ("allow", "deny"):
                v = tab.get(k)
                if isinstance(v, list) and v: out[t + "." + k] = v
    if isinstance(d.get("model"), str): out["model"] = d["model"]
    ms = d.get("mcp_servers")
    if isinstance(ms, dict):
        for n, s in ms.items():
            if isinstance(s, dict) and isinstance(s.get("command"), str):
                out["mcp:" + n] = s["command"]
    mp = d.get("model_pricing")
    if isinstance(mp, dict):
        for n, s in mp.items():
            if isinstance(s, dict) and "input" in s and "output" in s:
                out["price:" + n] = [float(s["input"]), float(s["output"])]
    return out
res = []
for doc in json.load(sys.stdin):
    try: res.append(proj(tomllib.loads(doc)))
    except Exception as e: res.append({"error": str(e)})
print(json.dumps(res))
"#;
        use std::io::Write;
        let mut child = std::process::Command::new("python3")
            .args(["-c", SCRIPT])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| {
                panic!("the TOML oracle is absent: could not run python3 ({e}); this test needs python3 >= 3.11 (tomllib)")
            });
        let input = serde_json::to_string(docs).expect("corpus to json");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes())
            .expect("write corpus");
        let out = child.wait_with_output().expect("oracle ran");
        assert!(
            out.status.success(),
            "the TOML oracle failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("oracle json")
    }

    /// The same projection over what yoyo's readers build from `doc`.
    fn yoyo_view(doc: &str, want: &serde_json::Value) -> serde_json::Value {
        use crate::config::*;
        let mut m = serde_json::Map::new();
        let d = parse_directories_from_config(doc);
        let p = parse_permissions_from_config(doc);
        for (k, v) in [
            ("directories.allow", d.allow),
            ("directories.deny", d.deny),
            ("permissions.allow", p.allow),
            ("permissions.deny", p.deny),
        ] {
            if !v.is_empty() {
                m.insert(k.into(), serde_json::json!(v));
            }
        }
        if let Some(v) = parse_config_file(doc).get("model") {
            m.insert("model".into(), serde_json::json!(v));
        }
        for s in parse_mcp_servers_from_config(doc) {
            m.insert(format!("mcp:{}", s.name), serde_json::json!(s.command));
        }
        let prices = parse_model_pricing_from_config(doc);
        for key in want.as_object().into_iter().flat_map(|o| o.keys()) {
            if let Some(id) = key.strip_prefix("price:") {
                if let Some(pr) = prices.lookup(id) {
                    m.insert(
                        key.clone(),
                        serde_json::json!([pr.input_per_million, pr.output_per_million]),
                    );
                }
            }
        }
        serde_json::Value::Object(m)
    }

    /// #1178's done-when: every header/key shape is read the way tomllib
    /// reads it, or refused: yoyo builds nothing for it AND the warning says
    /// it is valid TOML yoyo does not read (NOT in force, for a fence). No
    /// row may be called a typo.
    #[test]
    fn every_header_and_key_shape_agrees_with_tomllib_or_is_refused() {
        let wanted = shape_oracle(SHAPE_CORPUS);
        assert_eq!(wanted.len(), SHAPE_CORPUS.len(), "oracle row count");
        let (mut agree, mut refused, mut disagree) = (0, 0, Vec::new());
        for (doc, want) in SHAPE_CORPUS.iter().zip(&wanted) {
            assert!(
                want.get("error").is_none(),
                "corpus row {doc:?} is not TOML per tomllib: {want}"
            );
            let got = yoyo_view(doc, want);
            let warnings =
                unknown_config_key_warnings(".yoyo.toml", &unknown_config_keys(doc)).join("\n");
            if warnings.contains("did you mean") {
                disagree.push(format!("{doc:?}: valid TOML called a typo: {warnings}"));
                continue;
            }
            if &got == want {
                // A row yoyo reads must not also be warned as unread (#1182:
                // checked by the words, so a non-fence table counts too).
                if warnings.contains("does not read") && !got.as_object().unwrap().is_empty() {
                    disagree.push(format!("{doc:?}: read, yet warned: {warnings}"));
                } else {
                    agree += 1;
                }
                continue;
            }
            let (g, w) = (got.as_object().unwrap(), want.as_object().unwrap());
            let read_other = g.iter().any(|(k, v)| w.get(k) != Some(v));
            let fence = w
                .keys()
                .any(|k| k.starts_with("directories.") || k.starts_with("permissions."));
            let named =
                warnings.contains("valid TOML") && (!fence || warnings.contains("NOT in force"));
            if !read_other && named {
                refused += 1;
            } else {
                disagree.push(format!(
                    "{doc:?}: yoyo read {got}, tomllib read {want}; warnings: {warnings:?}"
                ));
            }
        }
        eprintln!(
            "{agree} of {} shapes agree, {refused} refused, {} disagree",
            SHAPE_CORPUS.len(),
            disagree.len()
        );
        assert!(
            disagree.is_empty(),
            "shapes that disagree with tomllib:\n{}",
            disagree.join("\n")
        );
        // Refusal is the escape hatch: pin it so widening it is a decision.
        // 7 inline fence tables at a table name (#1180). Down from 15 at
        // #1182: mcp_servers / model_pricing are read through the toml crate,
        // so their 3 dotted spellings and 5 inline rows are now read.
        assert_eq!(refused, 7, "refused rows changed");
    }

    /// #1180: the rewrite an inline-table refusal prints is a command the
    /// user will paste, so it is run: put it in place of their line, read it
    /// with yoyo's readers, and it must build exactly what tomllib reads from
    /// the line they wrote, with no warning left.
    #[test]
    fn every_inline_table_rewrite_builds_what_tomllib_reads() {
        let rows: Vec<&str> = SHAPE_CORPUS
            .iter()
            .copied()
            .filter(|d| {
                !d.starts_with('[')
                    && d.split_once('=')
                        .is_some_and(|(_, v)| v.trim().starts_with('{'))
            })
            .filter(|d| !d.contains("mcp_servers") && !d.contains("model_pricing"))
            .collect();
        // The fence rows; mcp_servers / model_pricing ones are read (#1182).
        assert_eq!(rows.len(), 7, "the #1180 fence rows");
        let wanted = shape_oracle(&rows);
        let mut rewritten = 0;
        for (doc, want) in rows.iter().zip(&wanted) {
            let findings = unknown_config_keys(doc);
            let [UnknownKey::Unread { line, rewrite, .. }] = findings.as_slice() else {
                panic!("{doc:?}: expected one Unread finding, got {findings:?}");
            };
            let Some(rewrite) = rewrite else {
                // Only the inline table that spans lines gets no rewrite.
                assert!(
                    doc.contains('\n'),
                    "{doc:?}: one-line inline table, no rewrite"
                );
                continue;
            };
            let warning = unknown_config_key_warnings(".yoyo.toml", &findings).join("\n");
            assert!(
                warning.contains(&format!(
                    "Write it as:\n    {}",
                    rewrite.replace('\n', "\n    ")
                )),
                "{doc:?}: the warning does not print the rewrite: {warning}"
            );
            let mut lines: Vec<&str> = doc.lines().collect();
            lines[line - 1] = rewrite;
            let fixed = lines.join("\n");
            assert_eq!(
                &yoyo_view(&fixed, want),
                want,
                "{doc:?}: its rewrite {fixed:?} does not build what tomllib reads"
            );
            assert!(
                unknown_config_keys(&fixed).is_empty(),
                "{doc:?}: its rewrite {fixed:?} is warned about itself"
            );
            rewritten += 1;
        }
        assert_eq!(rewritten, 6, "rows with a rewrite");
    }

    #[test]
    fn an_inline_directories_table_is_a_fence_not_in_force_never_an_unknown_key() {
        let w = unknown_config_key_warnings(
            ".yoyo.toml",
            &unknown_config_keys("directories = { deny = [\"/etc\"] }"),
        );
        assert_eq!(
            w,
            vec![
                ".yoyo.toml line 1: `directories = { ... }` is valid TOML, but yoyo does not \
                  read that spelling of `directories`, so it has no effect. This rule is NOT in \
                  force. yoyo reads `[directories]` then `allow = [...]` / `deny = [...]`. \
                  Write it as:\n    [directories]\n    deny = [\"/etc\"]"
                    .to_string()
            ]
        );
        // #1182: the same shape for a non-fence table is read, so not warned.
        assert_eq!(
            unknown_config_keys(
                "mcp_servers = { \"my server\" = { command = \"npx\", args = [\"-y\"] } }",
            ),
            vec![]
        );
    }

    /// #1182: inside mcp_servers / model_pricing every spelling is read, so
    /// the detector judges the path and the value's type: a typo'd key is a
    /// typo (with its did-you-mean), a wrong type is named with what the
    /// reader needs, and a read shape is silent.
    #[test]
    fn parsed_tables_are_judged_by_path_and_type_not_by_header() {
        let w = |doc: &str| unknown_config_key_warnings(".yoyo.toml", &unknown_config_keys(doc));
        for doc in [
            "[mcp_servers]\nfs = { command = \"npx\" }",
            "[mcp_servers.fs.env]\nA = \"b\"",
            "[mcp_servers]\nfs.env.A = \"b\"",
            "[model_pricing]\nm.input = 1\nm.output = 2.0",
        ] {
            assert_eq!(w(doc), Vec::<String>::new(), "{doc:?}");
        }
        assert_eq!(
            w("[mcp_servers]\nfs = { comand = \"npx\" }"),
            vec![
                ".yoyo.toml line 2: `comand` (in [mcp_servers.fs]) is not a key yoyo reads, \
                  so it has no effect; did you mean `command`?"
                    .to_string()
            ]
        );
        assert_eq!(
            w("[mcp_servers.fs]\ncommand = 5"),
            vec![
                ".yoyo.toml line 2: `command = 5` is valid TOML, but yoyo does not read that \
                  spelling of `mcp_servers.fs.command`, so it has no effect. yoyo reads \
                  `[mcp_servers.fs]` then `command = \"<program>\"` (a string); the server is \
                  not started."
                    .to_string()
            ]
        );
        assert_eq!(
            w("[mcp_servers]\nfs = \"npx\""),
            vec![
                ".yoyo.toml line 2: `fs = \"npx\"` is valid TOML, but yoyo does not read that \
                  spelling of `mcp_servers.fs`, so it has no effect. yoyo reads \
                  `[mcp_servers.fs]` then `command = ...` (a server is a table)."
                    .to_string()
            ]
        );
        assert_eq!(
            w("[model_pricing]\nm = { input = \"1.0\", output = 2.0 }").len(),
            1,
            "a quoted rate is named"
        );
        // A table the reader never opens is still an unread header.
        assert!(
            w("[mcp_servers.fs.args]\nx = 1")[0].contains("does not read `[mcp_servers.fs.args]`")
        );
    }
}
