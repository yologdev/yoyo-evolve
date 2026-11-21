//! #1213: reading one `/spawn` flag value that may be quoted. `-o` and
//! `--system` share this one reader; split out of `commands_spawn.rs` for the
//! module-size gate.

/// Split off the first whitespace-separated word of `s`, skipping leading
/// whitespace. A blank `s` gives `("", "")`.
pub(super) fn next_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    s.split_once(char::is_whitespace).unwrap_or((s, ""))
}

/// Read one flag value from the front of `s`, returning it and the rest.
///
/// A value that opens with `"` or `'` runs to the matching close quote and
/// keeps the whitespace inside it; non-space text glued to the close quote
/// joins it (`"a b".md` is `a b.md`). Any other value is one bare word, and a
/// quote inside it is just a character (`don't.md`), as before #1213. There
/// are no escapes, so a Windows path's backslashes stay as typed.
///
/// `Err(quote)` when the opening quote is never closed.
fn take_flag_value(s: &str) -> Result<(String, &str), char> {
    let s = s.trim_start();
    let Some(q) = s.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
        let (word, rest) = next_word(s);
        return Ok((word.to_string(), rest));
    };
    let inner = s.strip_prefix(q).unwrap_or(s);
    let (quoted, after) = inner.split_once(q).ok_or(q)?;
    let (glued, rest) = after.split_once(char::is_whitespace).unwrap_or((after, ""));
    Ok((format!("{quoted}{glued}"), rest))
}

/// Read the value of `flag` (`-o` or `--system`) from `after`, the text that
/// follows it. `Err` is the user-facing refusal: a quote that is never
/// closed, or an `-o` whose quoted value is empty.
pub(super) fn read_flag_value<'a>(flag: &str, after: &'a str) -> Result<(String, &'a str), String> {
    let (what, eg) = match flag {
        "-o" => ("file name", "\"my notes.md\""),
        _ => ("system prompt", "\"You are a reviewer\""),
    };
    let (value, rest) = take_flag_value(after).map_err(|q| {
        format!(
            "{flag} {}: the {q} is never closed, so yoyo can't tell where the {what} ends. Nothing was run. Close the quote, e.g. /spawn {flag} {eg} <task>",
            after.trim()
        )
    })?;
    if flag == "-o" && value.is_empty() {
        return Err(format!(
            "-o {} names no file. Nothing was run. Give it a file name, e.g. /spawn -o {eg} <task>",
            next_word(after).0
        ));
    }
    Ok((value, rest))
}

/// Spell `path` so [`take_flag_value`] reads it back as one value.
pub(super) fn quote_for_flag(path: &str) -> String {
    if !path.contains(char::is_whitespace) && !path.starts_with(['"', '\'']) {
        return path.to_string();
    }
    let q = if path.contains('"') { '\'' } else { '"' };
    format!("{q}{path}{q}")
}
