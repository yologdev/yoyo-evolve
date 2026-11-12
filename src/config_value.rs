//! Reading one config value the way TOML does (#1177).
//!
//! yoyo reads its config files line by line, not with a TOML parser. Before
//! this module every reader cut the text after `=` by hand, so
//! `model = "m1" # c` sent `"m1" # c` (quotes and comment) as the model id,
//! `deny = ["/tmp"] # x` came back as an empty list, so the fence was never
//! built, and `model = """m1"""` read as `""m1""`.
//!
//! The rule now: a value is either read exactly as python3's `tomllib` reads
//! it, or **refused** (`Err(reason)`), never misread. A refused value is left
//! out by its reader, and [`warn_unreadable_values_once`] prints one warning
//! per run naming the key, so an unreadable `deny` list says so instead of
//! quietly becoming no fence. The differential test at the bottom checks
//! every row of a spec-shaped corpus against `tomllib` itself.
//!
//! Shapes that are refused: values spanning several lines (multi-line arrays,
//! `"""` strings with a line break), inline tables (`{ a = 1 }`, except the
//! `env` table of an `[mcp_servers.*]` section, which has its own reader),
//! nested arrays, and anything left over after a closing quote. Bare values
//! (numbers, booleans) are passed through as text, with TOML's integer forms
//! (`1_000`, `0x10`, `+5`) written as plain decimal so `parse::<u64>()` works.
//! A bare value that is not TOML at all (`model = m1`) is still passed through
//! as before: `tomllib` refuses the whole file there, and this module does
//! not make yoyo stricter than it was about input TOML itself rejects.

/// `raw` (the text after `=`) with a trailing `# comment` removed and
/// whitespace trimmed. A `#` inside a basic (`"…"`, with `\` escapes),
/// literal (`'…'`) or triple-quoted string is part of the string, not a
/// comment. An unterminated string keeps everything, so the reader refuses it.
pub(crate) fn strip_trailing_comment(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let Some((delim, escapes)) = string_opener(&bytes[i..]) else {
            if bytes[i] == b'#' {
                // ASCII `#`, so `i` is a char boundary.
                return raw[..i].trim();
            }
            i += 1;
            continue;
        };
        match closing_delimiter(&bytes[i + delim.len()..], delim, escapes) {
            Some(end) => i += delim.len() + end,
            None => return raw.trim(),
        }
    }
    raw.trim()
}

/// The string delimiter `s` starts with, longest first, and whether that
/// string kind has `\` escapes.
fn string_opener(s: &[u8]) -> Option<(&'static [u8], bool)> {
    let opens: [(&'static [u8], bool); 4] = [
        (b"\"\"\"", true),
        (b"'''", false),
        (b"\"", true),
        (b"'", false),
    ];
    opens.into_iter().find(|(d, _)| s.starts_with(d))
}

/// Byte offset just past the delimiter that closes a string whose body starts
/// at `body`. A triple-quoted string may end with up to two extra quote
/// characters, which belong to the content (`"""a""""` is `a"`). Works on
/// bytes; every offset it returns follows an ASCII quote, so it is a char
/// boundary.
fn closing_delimiter(body: &[u8], delim: &[u8], escapes: bool) -> Option<usize> {
    let quote = delim[0];
    let mut i = 0;
    while i < body.len() {
        if escapes && body[i] == b'\\' {
            i += 2;
            continue;
        }
        if body[i..].starts_with(delim) {
            let mut end = i + delim.len();
            if delim.len() == 3 {
                let mut extra = 0;
                while extra < 2 && body.get(end) == Some(&quote) {
                    end += 1;
                    extra += 1;
                }
            }
            return Some(end);
        }
        i += 1;
    }
    None
}

/// Read one single value (`raw` is the text after `=`, comment and all) as
/// TOML would. `Err` names the shape that cannot be read from one line.
pub(crate) fn read_scalar(raw: &str) -> Result<String, &'static str> {
    let v = strip_trailing_comment(raw);
    if v.starts_with('[') {
        return Err("an array where one value was expected");
    }
    if v.starts_with('{') {
        return Err("an inline table");
    }
    let Some((delim, escapes)) = string_opener(v.as_bytes()) else {
        return Ok(normalize_bare(v));
    };
    let body = &v[delim.len()..];
    let Some(end) = closing_delimiter(body.as_bytes(), delim, escapes) else {
        return Err(if delim.len() == 3 {
            "a string over several lines"
        } else {
            "an unterminated string"
        });
    };
    if end != body.len() {
        return Err("text after the closing quote");
    }
    let inner = &body[..end - delim.len()];
    Ok(if escapes {
        unescape_toml_basic_string(inner)
    } else {
        inner.to_string()
    })
}

/// Read a one-line TOML array of strings (or bare values) as TOML would.
/// Commas and `]` inside strings do not split or end it; a trailing comma is
/// allowed. `Err` for a multi-line array, a nested array or table, or
/// anything after the closing `]`.
pub(crate) fn read_array(raw: &str) -> Result<Vec<String>, &'static str> {
    let v = strip_trailing_comment(raw);
    let Some(mut rest) = v.strip_prefix('[') else {
        return Err("not an array");
    };
    let mut items = Vec::new();
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix(']') {
            return if after.trim().is_empty() {
                Ok(items)
            } else {
                Err("text after the closing ]")
            };
        }
        if rest.is_empty() {
            return Err("an array over several lines");
        }
        if rest.starts_with('[') || rest.starts_with('{') {
            return Err("a nested array or table");
        }
        // One item runs to the first `,` or `]` outside quotes.
        let item_len = item_length(rest).ok_or("an array over several lines")?;
        let item = rest[..item_len].trim();
        if item.is_empty() {
            return Err("an empty array item");
        }
        items.push(read_scalar(item)?);
        rest = rest[item_len..].trim_start();
        if let Some(after) = rest.strip_prefix(',') {
            rest = after;
        } else if !rest.starts_with(']') {
            return Err("an array over several lines");
        }
    }
}

/// Length of the array item at the start of `s`: up to the first `,` or `]`
/// outside a string. `None` if a string in it never closes.
fn item_length(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some((d, escapes)) = string_opener(&bytes[i..]) {
            i += d.len() + closing_delimiter(&bytes[i + d.len()..], d, escapes)?;
            continue;
        }
        if bytes[i] == b',' || bytes[i] == b']' {
            return Some(i);
        }
        i += 1;
    }
    Some(i)
}

/// A bare TOML value as text: integers in TOML's other spellings (`1_000`,
/// `0x10`, `0o17`, `0b11`, `+5`) become plain decimal; floats lose their `_`
/// separators. Anything else is returned as written.
fn normalize_bare(v: &str) -> String {
    let digits = v.replace('_', "");
    let radix = [("0x", 16), ("0o", 8), ("0b", 2)]
        .into_iter()
        .find_map(|(p, r)| digits.strip_prefix(p).map(|d| (d, r)));
    if let Some((d, r)) = radix {
        if let Ok(n) = i128::from_str_radix(d, r) {
            return n.to_string();
        }
    } else if let Ok(n) = digits.parse::<i128>() {
        return n.to_string();
    } else if v.contains('_') && digits.parse::<f64>().is_ok() {
        return digits;
    }
    v.to_string()
}

/// Undo TOML basic-string escapes (`\b \t \n \f \r \" \\ \uXXXX
/// \UXXXXXXXXX`). An escape TOML does not define (a Windows path written
/// `"C:\Users"`) is kept verbatim, backslash included, as yoyo always has.
pub(crate) fn unescape_toml_basic_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let simple = match chars.peek() {
            Some('\\') => Some('\\'),
            Some('"') => Some('"'),
            Some('n') => Some('\n'),
            Some('r') => Some('\r'),
            Some('t') => Some('\t'),
            Some('b') => Some('\u{8}'),
            Some('f') => Some('\u{c}'),
            _ => None,
        };
        if let Some(s) = simple {
            chars.next();
            out.push(s);
            continue;
        }
        let width = match chars.peek() {
            Some('u') => 4,
            Some('U') => 8,
            _ => 0,
        };
        let hex: String = chars.clone().skip(1).take(width).collect();
        let decoded = (width > 0 && hex.len() == width)
            .then(|| u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32))
            .flatten();
        match decoded {
            Some(ch) => {
                for _ in 0..=width {
                    chars.next();
                }
                out.push(ch);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Every `key = value` line in `content` whose value yoyo cannot read, as
/// `(1-based line, "[section] key", reason)`. Arrays are checked as arrays,
/// everything else as one value. The `env` inline table of an
/// `[mcp_servers.*]` section is skipped: it has its own reader.
pub(crate) fn unreadable_values(content: &str) -> Vec<(usize, String, &'static str)> {
    let mut out = Vec::new();
    let mut section = String::new();
    // Inside a value that spans lines (refused on its first line): skip until
    // the line that closes it, so its body is not read as keys.
    let mut open_until: Option<&str> = None;
    for (idx, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if let Some(close) = open_until {
            if line.contains(close) {
                open_until = None;
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(h) = crate::config_unknown_keys::toml_section_header(line) {
            section = h;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let v = strip_trailing_comment(value);
        // A top-level `directories.deny` is the fence list too (#1178).
        let dotted = ["directories", "permissions"].into_iter().find_map(|t| {
            crate::config_unknown_keys::key_in_table(None, key, t).map(|k| format!("[{t}] {k}"))
        });
        let key = crate::config_unknown_keys::config_key_name(key);
        if key == "env" && section.starts_with("mcp_servers.") && v.starts_with('{') {
            continue;
        }
        let result = if v.starts_with('[') {
            read_array(v).map(|_| ())
        } else {
            read_scalar(v).map(|_| ())
        };
        if let Err(reason) = result {
            let name = if section.is_empty() {
                dotted.unwrap_or(key)
            } else {
                format!("[{section}] {key}")
            };
            out.push((idx + 1, name, reason));
            open_until = match reason {
                "an array over several lines" => Some("]"),
                "a string over several lines" if v.starts_with("'''") => Some("'''"),
                "a string over several lines" => Some("\"\"\""),
                _ => None,
            };
        }
    }
    out
}

/// One warning line per unreadable value in `content` (read from `shown`).
pub(crate) fn unreadable_value_warnings(shown: &str, content: &str) -> Vec<String> {
    unreadable_values(content)
        .into_iter()
        .map(|(line, key, reason)| {
            let fence = if key.starts_with("[directories]") || key.starts_with("[permissions]") {
                " This rule is NOT in force."
            } else {
                ""
            };
            format!(
                "{shown}:{line}: could not read the value of '{key}' ({reason}); it was ignored.{fence} Write it on one line, as a quoted string or [\"a\", \"b\"]."
            )
        })
        .collect()
}

static UNREADABLE_WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Print [`unreadable_value_warnings`] to stderr, at most once per process
/// (`load_config_file` runs several times per run). Not quiet-gated: a value
/// that was ignored, perhaps a fence, is a fault, not startup chatter.
pub(crate) fn warn_unreadable_values_once(shown: &str, content: &str) {
    let warnings = unreadable_value_warnings(shown, content);
    if warnings.is_empty() || UNREADABLE_WARNED.swap(true, std::sync::atomic::Ordering::SeqCst) {
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

    /// Spec-shaped corpus: every row is TOML that `tomllib` reads (the
    /// differential test fails if one is not). The key is always `k`; yoyo's
    /// readers see the first line's text after `=`.
    const CORPUS: &[&str] = &[
        r##"k = "m1""##,
        r##"k = "m1" # c"##,
        r##"k = "m1"# c"##,
        r##"k = "a#b""##,
        r##"k = "a#b" # c"##,
        r##"k = 'a#b' # c"##,
        r##"k = 'm1'"##,
        r##"k = "a\"b" # c"##,
        r##"k = "a\\" # c"##,
        r##"k = """m1""""##,
        r##"k = '''m1'''"##,
        r##"k = """m1""" # c"##,
        r##"k = '''a#b''' # c"##,
        r##"k = """a"b""""##,
        r##"k = """a"""""##,
        r##"k = "tab\there""##,
        r##"k = "\u00e9""##,
        r##"k = "caf\u00E9 \U0001F600""##,
        r##"k = "C:\\Users\\me""##,
        r##"k = 'C:\Users\me'"##,
        r##"k = """##,
        r##"k = "" # c"##,
        r##"k = "ñ ✓" # c"##,
        r##"k = "m1" # it's "quoted""##,
        r##"k = 'a' # "b"##,
        r##"k = "a=b""##,
        "k = 4096 # c",
        "k = 4096",
        "k = 1_000",
        "k = 0x10",
        "k = 0o17",
        "k = 0b11",
        "k = +5",
        "k = -3",
        "k = 0.7 # c",
        "k = 1e3",
        "k = 1_000.5",
        "k = inf",
        "k = true # c",
        "k = false",
        r##"k = ["/tmp"] # x"##,
        r##"k = ["a#b", "c"]"##,
        r##"k = ["a]b"]"##,
        "k = ['x'] # y",
        "k = [] # empty",
        r##"k = ["a",]"##,
        r##"k = ["a", 'b' ,"c" ] # c"##,
        r##"k = ["a,b"]"##,
        r##"k = ["a\"b"]"##,
        r##"k = [""]"##,
        r##"k = ['''a,b''', """c]"""]"##,
        "k = [1, 2]",
        r##"k = [["a"]]"##,
        "k = { a = 1 }",
        "k = [\n  \"/tmp\",\n]",
        "k = [ # list\n  \"a\" ]",
        "k = \"\"\"\nm1\n\"\"\"",
        "k = '''\nm1'''",
    ];

    /// What `tomllib` read for each corpus row, as JSON tagged by type.
    fn oracle(rows: &[&str]) -> Vec<serde_json::Value> {
        const SCRIPT: &str = r#"
import json, sys, tomllib
def enc(v):
    if isinstance(v, bool): return {"bool": v}
    if isinstance(v, int): return {"int": str(v)}
    if isinstance(v, float): return {"float": repr(v)}
    if isinstance(v, str): return {"str": v}
    if isinstance(v, list): return {"list": [enc(x) for x in v]}
    return {"other": type(v).__name__}
out = []
for doc in json.load(sys.stdin):
    try: out.append(enc(tomllib.loads(doc)["k"]))
    except Exception as e: out.append({"error": str(e)})
print(json.dumps(out))
"#;
        use std::io::Write;
        let child = std::process::Command::new("python3")
            .args(["-c", SCRIPT])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();
        let mut child = child.unwrap_or_else(|e| {
            panic!("the TOML oracle is absent: could not run python3 ({e}); this test needs python3 >= 3.11 (tomllib)")
        });
        let input = serde_json::to_string(rows).expect("corpus to json");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes())
            .expect("write corpus to python3");
        let out = child.wait_with_output().expect("python3 output");
        assert!(
            out.status.success(),
            "the TOML oracle is absent or failed (python3 >= 3.11 with tomllib is required): {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("oracle json")
    }

    /// Does yoyo's text for one value match what tomllib read?
    fn scalar_agrees(want: &serde_json::Value, got: &str) -> bool {
        if let Some(s) = want.get("str").and_then(|s| s.as_str()) {
            return got == s;
        }
        if let Some(b) = want.get("bool").and_then(|b| b.as_bool()) {
            return got == b.to_string();
        }
        if let Some(n) = want.get("int").and_then(|n| n.as_str()) {
            return got == n;
        }
        if let Some(f) = want.get("float").and_then(|f| f.as_str()) {
            let (w, g) = (f.parse::<f64>(), got.parse::<f64>());
            return matches!((w, g), (Ok(w), Ok(g)) if w == g || (w.is_nan() && g.is_nan()));
        }
        false
    }

    /// yoyo's reading of one corpus row: `Ok(None)` = refused, `Err` = a
    /// disagreement with tomllib, described.
    fn compare(row: &str, want: &serde_json::Value) -> Result<Option<()>, String> {
        let first = row.lines().next().unwrap_or("");
        let raw = first.split_once('=').map(|(_, v)| v).unwrap_or("");
        let is_array = strip_trailing_comment(raw).starts_with('[');
        if is_array {
            match read_array(raw) {
                Err(_) => Ok(None),
                Ok(got) => {
                    let items = want.get("list").and_then(|l| l.as_array());
                    match items {
                        Some(items)
                            if items.len() == got.len()
                                && items.iter().zip(&got).all(|(w, g)| scalar_agrees(w, g)) =>
                        {
                            Ok(Some(()))
                        }
                        _ => Err(format!("yoyo read {got:?}, tomllib read {want}")),
                    }
                }
            }
        } else {
            match read_scalar(raw) {
                Err(_) => Ok(None),
                Ok(got) if scalar_agrees(want, &got) => Ok(Some(())),
                Ok(got) => Err(format!("yoyo read {got:?}, tomllib read {want}")),
            }
        }
    }

    /// #1177's done-when: over the whole corpus, yoyo reads every row the way
    /// tomllib does, or refuses it. Zero rows may disagree.
    #[test]
    fn every_corpus_row_agrees_with_tomllib_or_is_refused() {
        let wanted = oracle(CORPUS);
        assert_eq!(wanted.len(), CORPUS.len(), "oracle row count");
        let (mut agree, mut refused, mut disagree) = (0, 0, Vec::new());
        for (row, want) in CORPUS.iter().zip(&wanted) {
            assert!(
                want.get("error").is_none(),
                "corpus row {row:?} is not TOML per tomllib: {want}"
            );
            match compare(row, want) {
                Ok(Some(())) => agree += 1,
                Ok(None) => refused += 1,
                Err(why) => disagree.push(format!("{row:?}: {why}")),
            }
        }
        eprintln!(
            "{agree} rows agree, {refused} refused, {} disagree",
            disagree.len()
        );
        assert!(
            disagree.is_empty(),
            "rows that disagree with tomllib:\n{}",
            disagree.join("\n")
        );
        // Refusal is the escape hatch, not the answer: the shapes a user
        // writes daily must be read. Pin the refused count so widening the
        // refusals is a visible decision.
        assert_eq!(refused, 6, "refused rows changed");
    }

    /// The three motivating shapes of #1177, pinned without the oracle.
    #[test]
    fn motivating_shapes_read_exactly() {
        let map = crate::config::parse_config_file("model = \"m1\" # c\n");
        assert_eq!(map.get("model").map(String::as_str), Some("m1"));
        let map = crate::config::parse_config_file("model = \"\"\"m1\"\"\"\n");
        assert_eq!(map.get("model").map(String::as_str), Some("m1"));
        let dirs =
            crate::config::parse_directories_from_config("[directories]\ndeny = [\"/tmp\"] # x\n");
        assert_eq!(dirs.deny, vec!["/tmp".to_string()]);
        let perms =
            crate::config::parse_permissions_from_config("[permissions]\ndeny = [\"rm *\"] # no\n");
        assert_eq!(perms.deny, vec!["rm *".to_string()]);
    }

    #[test]
    fn unreadable_fence_list_is_named_not_silent() {
        let text = "model = \"m\"\n[directories]\ndeny = [\n  \"/tmp\",\n]\nallow = [\"/w\"]\n";
        let found = unreadable_values(text);
        assert_eq!(
            found,
            vec![(
                3,
                "[directories] deny".to_string(),
                "an array over several lines"
            )]
        );
        let w = unreadable_value_warnings(".yoyo.toml", text);
        assert_eq!(w.len(), 1);
        assert!(
            w[0].starts_with(".yoyo.toml:3: could not read the value of '[directories] deny'"),
            "{}",
            w[0]
        );
        assert!(
            w[0].contains("it was ignored. This rule is NOT in force."),
            "{}",
            w[0]
        );
    }

    /// #1178: a top-level dotted fence list is the fence, so an unreadable
    /// one says the rule is not in force, like the `[directories]` spelling.
    #[test]
    fn unreadable_dotted_fence_list_is_named_as_the_fence() {
        let text = "directories.deny = [\n  \"/etc\",\n]\n";
        let found = unreadable_values(text);
        assert_eq!(
            found,
            vec![(
                1,
                "[directories] deny".to_string(),
                "an array over several lines"
            )]
        );
        let w = unreadable_value_warnings(".yoyo.toml", text);
        assert!(w[0].contains("This rule is NOT in force."), "{}", w[0]);
    }

    #[test]
    fn readable_files_and_mcp_env_tables_raise_no_warning() {
        let text = "model = \"m1\" # c\nmax_tokens = 4096 # c\n[permissions]\nallow = [\"git *\"] # x\n\
                    [mcp_servers.fs]\ncommand = \"npx\"\nargs = [\"-y\", \"pkg\"]\nenv = { A = \"1\" }\n\
                    system = \"\"\"\nline = [\n\"\"\"\n";
        let found = unreadable_values(text);
        // Only the multi-line string is refused; its body is not read as keys.
        assert_eq!(
            found,
            vec![(
                9,
                "[mcp_servers.fs] system".to_string(),
                "a string over several lines"
            )]
        );
    }
}
