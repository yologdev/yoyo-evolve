//! Fence the RESULTS of a recursive reader, not just its path argument (#1022).
//!
//! `GuardedTool` / `ArcGuardedTool` check a tool's `path` argument against the
//! directory fence (`--allow-dir` / `--deny-dir` / `[directories]`, plus the
//! `/cd`-added deny list). For `read_file` that argument IS the file read. For
//! `search` and `list_files` it is a ROOT: `search CANARY .` passed the check on
//! `.` and then the inner yoagent tool walked straight into `./secrets/` and
//! handed the model `./secrets/key.txt:1:token=CANARY`. This module filters the
//! text those two tools return, using the caller's own refusal predicate, so
//! there is one predicate for the argument and the results, not two.
//!
//! **Byte-identical when nothing is withheld** — that is every user without a
//! fence and every fenced search that stayed outside the denied subtree, i.e.
//! the whole regression surface. When something is withheld, one glyph-free
//! line says how many, so the model learns results were removed rather than
//! concluding they never existed.
//!
//! **Fail closed on lines it cannot attribute (#1023).** While a fence is active
//! and the root is a directory, a line before yoagent's own footer is kept only
//! if its first `path:N:` candidate names a path under the searched root (and
//! holds no NUL). Anything else is dropped and counted in its own note. The
//! property is "no line reaches the model unless the fence could name the file
//! it came from", decided by where a line's path came from — no list of rg
//! flags or output formats is consulted.
//!
//! A user's search config is no longer one of the reasons: yoagent's `search`
//! passes `--no-config` to `rg` and removes `RIPGREP_CONFIG_PATH` and
//! `GREP_OPTIONS` from the child's environment, for `rg` and the grep fallback
//! alike <!-- yoagent-version-claim: 0.25 -->. The shapes that can still reach
//! the drop path, stated rather than implied: a file NAME containing a newline
//! (measured, GNU grep 3.11: `d/a\nb.txt` printed as `d/a` and `b.txt:1:tok`,
//! neither naming a file under `d`); a grep that prints its binary-match notice
//! on stdout (GNU grep did before 3.5, per its NEWS: `Binary file FOO matches`);
//! an `rg` or `grep` on `PATH` that is a wrapper adding its own flags; and any
//! future output form this parser does not know. Before yoagent 0.25.5 the
//! main reason was a ripgrep config (measured on ripgrep 14.1.0, Day 229:
//! `--no-filename`, `--null`, `--json` and `--path-separator=X` each passed a
//! denied file's line as "not a result line"); those fixtures stay in the tests
//! below because a wrapper on `PATH` can still produce them.
//!
//! **The tool's own count agrees with what is shown (#1023).** When anything is
//! withheld, yoagent's `(N matches)` / `(N files)` footer (and its truncated
//! forms) is re-counted to the result lines actually kept, and `details`'
//! `matches` / `total` is lowered by the same amount. A footer this module
//! does not recognise is left alone, never invented.
//!
//! Limits, stated rather than implied: `find` and `rg` warning lines after the
//! footer are kept verbatim even if they name a denied path. A single-FILE root is not put
//! in fail-closed mode: `rg` prints that file's lines without a path, and the
//! root itself already passed the argument check.

/// Tools whose `path` argument is a root they walk below, so their output can
/// name files the argument check never saw.
const RECURSIVE_READERS: &[&str] = &["search", "list_files"];

/// The whole text yoagent's `search` returns when the subprocess printed
/// nothing; such output cannot carry a result line.
const NO_MATCHES_PREFIX: &str = "No matches found for '";

/// Every candidate file path a `search` result line could start with: the
/// prefix before each `:<digits>:` occurrence, first one first. A line with no
/// such occurrence (a header, the `(N matches)` footer, a blank, a single-file
/// search's bare `N:text`) yields nothing and is kept by the filter.
///
/// All splits are returned, not just the first, because a directory or file
/// name may itself contain `:<digits>:`; the filter withholds a line if ANY
/// candidate is refused, so an odd name fails closed rather than leaking.
fn search_path_candidates(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for (i, _) in line.match_indices(':') {
        let rest = &line[i + 1..];
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        // `digits` counts ASCII chars, so it is also a byte length here.
        if i > 0 && digits > 0 && rest[digits..].starts_with(':') {
            out.push(&line[..i]);
        }
    }
    out
}

/// The path a `search` result line names: the prefix before its FIRST
/// `:<digits>:`. `None` when the line is not a result line.
fn search_result_path(line: &str) -> Option<&str> {
    search_path_candidates(line).into_iter().next()
}

/// Whether a `search` line can be attributed to a file under `root`: its first
/// `path:N:` candidate exists, starts with the root exactly as it was handed to
/// the subprocess (that is how both `rg` and `grep -r` print it), and holds no
/// NUL (no path does). A reshaped line fails this, whatever reshaped it.
fn attributable_to_root(line: &str, root: &str) -> bool {
    search_result_path(line).is_some_and(|p| p.starts_with(root) && !p.contains('\0'))
}

/// One of the lines yoagent's `search` appends after the matches. Everything
/// from the LAST such line on is the tool's own tail (footer, then any
/// `Warnings:` block); picking the last one means a footer-shaped line earlier
/// in the output can only cause more dropping, never less.
fn is_search_footer(line: &str) -> bool {
    recount_footer("search", line, 0).is_some()
}

/// A non-empty run of ASCII digits, as a count.
fn parse_count(s: &str) -> Option<usize> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// yoagent's truncated-`search` footer, split around its count.
const SEARCH_MORE_PREFIX: &str = "... (showing the first ";
const SEARCH_MORE_SUFFIX: &str = " matches; there are more. Narrow the pattern, path or include.)";

/// If `line` is the count footer yoagent's `tool` appends (spellings copied
/// from yoagent 0.25.7's `tools/search.rs` and `tools/list.rs`
/// <!-- yoagent-version-claim: 0.25 -->), return it re-counted to `shown`
/// result lines, plus the count the original claimed for its shown lines.
/// `None` for anything else, so an unknown footer is left alone, never
/// invented. For a truncated `list_files` footer (`T files, showing first M`)
/// the total drops by exactly the shown entries that were withheld; entries
/// past the cap were never in the output and were not checked.
fn recount_footer(tool: &str, line: &str, shown: usize) -> Option<(String, usize)> {
    match tool {
        "search" => {
            if let Some(n) = line
                .strip_prefix('(')
                .and_then(|r| r.strip_suffix(" matches)"))
                .and_then(parse_count)
            {
                return Some((format!("({shown} matches)"), n));
            }
            let n = line
                .strip_prefix(SEARCH_MORE_PREFIX)
                .and_then(|r| r.strip_suffix(SEARCH_MORE_SUFFIX))
                .and_then(parse_count)?;
            Some((
                format!("{SEARCH_MORE_PREFIX}{shown}{SEARCH_MORE_SUFFIX}"),
                n,
            ))
        }
        "list_files" => {
            if let Some(n) = line
                .strip_prefix('(')
                .and_then(|r| r.strip_suffix(" files)"))
                .and_then(parse_count)
            {
                return Some((format!("({shown} files)"), n));
            }
            let (t, m) = line
                .strip_prefix("... (")
                .and_then(|r| r.strip_suffix(')'))
                .and_then(|r| r.split_once(" files, showing first "))?;
            let (t, m) = (parse_count(t)?, parse_count(m)?);
            let total = t.saturating_sub(m.saturating_sub(shown));
            Some((format!("... ({total} files, showing first {shown})"), m))
        }
        _ => None,
    }
}

/// The in-band note appended when `count > 0`. Glyph-free on purpose.
fn withheld_note(count: usize) -> String {
    let noun = if count == 1 { "result" } else { "results" };
    format!(
        "[{count} {noun} withheld: inside a directory the fence refuses (--deny-dir / --allow-dir / [directories])]"
    )
}

/// The note for lines dropped because no file could be named for them.
fn unattributed_note(count: usize) -> String {
    let noun = if count == 1 { "line" } else { "lines" };
    format!(
        "[{count} {noun} withheld: a directory fence is active and the line does not name a file under the searched path in the usual path:line: form, so the fence could not check it and dropped it rather than risk a leak]"
    )
}

/// Filter one tool's text output. Returns the kept text and how many lines were
/// withheld; with `0` withheld the input comes back byte-identical.
///
/// `strict_root` is `Some(root)` only while a fence is active and a `search`
/// walked the directory `root`: then a line before the tool's footer that is
/// not [`attributable_to_root`] is dropped too (#1023). `None` keeps the
/// pre-#1023 behaviour exactly.
#[cfg(test)]
fn filter_fenced_lines(
    tool: &str,
    output: &str,
    refused: &dyn Fn(&str) -> bool,
    strict_root: Option<&str>,
) -> (String, usize) {
    let f = filter_fenced(tool, output, refused, strict_root);
    (f.text, f.withheld)
}

/// The header yoagent's `list_files` puts above `find`'s stderr, after its
/// footer, and the line its 2000-char cap ends that stderr with (spellings
/// copied from yoagent 0.25 `src/tools/list.rs`) <!-- yoagent-version-claim: 0.25 -->.
const FIND_WARNINGS_HEADER: &str = "Warnings from find (the listing may be incomplete):";
/// `list_files`' whole text when `find` printed no file.
const NO_FILES_PREFIX: &str = "No files found in ";
/// The prefix of yoagent's `list_files` error when `find` failed and printed
/// no file: the rest is `find`'s (capped) stderr.
const LISTING_ERROR_PREFIX: &str = "Listing error: ";

/// For `list_files`: where its body (the file lines `find` printed) ends, and
/// the index of the find-warnings header, if any. Everything from the body's
/// end up to the header is yoagent's own (blank line, footer). Other tools:
/// the whole output is body, no header.
fn list_files_regions(lines: &[&str], output: &str) -> (usize, Option<usize>) {
    if !output.starts_with(NO_FILES_PREFIX) && !lines.contains(&FIND_WARNINGS_HEADER) {
        let footer = lines
            .iter()
            .rposition(|l| recount_footer("list_files", l, 0).is_some());
        return (footer.unwrap_or(lines.len()), None);
    }
    let header = lines.iter().rposition(|l| *l == FIND_WARNINGS_HEADER);
    let before = header.unwrap_or(lines.len());
    let body_end = if output.starts_with(NO_FILES_PREFIX) {
        0
    } else {
        lines[..before]
            .iter()
            .rposition(|l| recount_footer("list_files", l, 0).is_some())
            .unwrap_or(before)
    };
    (body_end, header)
}

/// Every path a `find` warning line could name: GNU's `find: ‘<p>’: <reason>`
/// (UTF-8 locale), `find: '<p>': <reason>` (C locale, an inner quote escaped
/// as `\'`), or BSD's unquoted `find: <p>: <reason>`. A name containing the
/// separator yields several candidates and any refused one withholds the line
/// (fails closed, like [`search_path_candidates`]). A line yoagent's cap cut
/// mid-name has no separator, so its whole remainder is the candidate.
/// Candidates without a `/` are dropped: every path below the root has one,
/// so such a name is the root itself (already past the argument check) or a
/// word like `warning`. Not a `find:` line: no candidates, the line stays.
fn find_warning_paths(line: &str) -> Vec<String> {
    let Some(rest) = line.trim_end_matches('\r').strip_prefix("find: ") else {
        return Vec::new();
    };
    let (body, sep) = if let Some(b) = rest.strip_prefix('\u{2018}') {
        (b, "\u{2019}: ")
    } else if let Some(b) = rest.strip_prefix('\'') {
        (b, "': ")
    } else {
        (rest, ": ")
    };
    let mut out: Vec<String> = body
        .match_indices(sep)
        .map(|(i, _)| body[..i].to_string())
        .collect();
    if out.is_empty() {
        out.push(body.to_string());
    }
    if sep == "': " {
        for p in out.iter_mut() {
            *p = p.replace("\\'", "'");
        }
    }
    out.retain(|p| p.contains('/'));
    out
}

/// Fence yoagent's `list_files` error text too: when `find` fails having
/// printed no file, its warnings come back as `Err("Listing error: ...")`,
/// which never reaches [`fence_tool_result`]. Warning lines naming a refused
/// path are withheld the same way; anything else is returned unchanged.
pub(crate) fn fence_tool_error(
    tool: &str,
    err: yoagent::types::ToolError,
    refused: &dyn Fn(&str) -> bool,
) -> yoagent::types::ToolError {
    let yoagent::types::ToolError::Failed(msg) = &err else {
        return err;
    };
    let Some(w) = msg.strip_prefix(LISTING_ERROR_PREFIX) else {
        return err;
    };
    if tool != "list_files" {
        return err;
    }
    let f = filter_fenced(
        "list_files",
        &format!("{FIND_WARNINGS_HEADER}\n{w}"),
        refused,
        None,
    );
    if f.withheld == 0 {
        return err;
    }
    let rest = f
        .text
        .strip_prefix(FIND_WARNINGS_HEADER)
        .map_or(f.text.as_str(), |r| r.trim_start_matches('\n'));
    yoagent::types::ToolError::Failed(format!("{LISTING_ERROR_PREFIX}{rest}"))
}

/// What [`filter_fenced`] produced: the text, how many lines it withheld, and
/// how far it lowered the footer's count (`0` when it found no footer).
struct Filtered {
    text: String,
    withheld: usize,
    recounted_by: usize,
}

/// The filter itself; see [`filter_fenced_lines`] for the contract.
fn filter_fenced(
    tool: &str,
    output: &str,
    refused: &dyn Fn(&str) -> bool,
    strict_root: Option<&str>,
) -> Filtered {
    if !RECURSIVE_READERS.contains(&tool) {
        return Filtered {
            text: output.to_string(),
            withheld: 0,
            recounted_by: 0,
        };
    }
    let lines: Vec<&str> = output.split('\n').collect();
    // Lines at or after `tail_start` are yoagent's own; only the body before it
    // must be attributable. With no footer the whole output is body, unless it
    // is the tool's "No matches" message, which carries no subprocess line.
    let strict_root = match strict_root {
        Some(_) if tool != "search" || output.starts_with(NO_MATCHES_PREFIX) => None,
        other => other,
    };
    let tail_start = lines
        .iter()
        .rposition(|l| is_search_footer(l))
        .unwrap_or(lines.len());
    let (body_end, warn_header) = list_files_regions(&lines, output);
    let mut keep = vec![false; lines.len()];
    let mut denied = 0usize;
    let mut unattributed = 0usize;
    for (idx, line) in lines.iter().enumerate() {
        let deny = match tool {
            "list_files" if idx < body_end => {
                let p = line.trim_end_matches('\r');
                !p.is_empty() && refused(p)
            }
            // yoagent's own lines (footer, header, cap) are never paths; under
            // an allow list a relative `(1 files)` resolved against the cwd
            // was refused and miscounted as a withheld result (#1023).
            "list_files" => {
                warn_header.is_some_and(|h| idx > h)
                    && find_warning_paths(line).iter().any(|p| refused(p))
            }
            _ => search_path_candidates(line).into_iter().any(refused),
        };
        if deny {
            denied += 1;
        } else if strict_root
            .is_some_and(|root| idx < tail_start && !attributable_to_root(line, root))
        {
            unattributed += 1;
        } else {
            keep[idx] = true;
        }
    }
    // A warnings header with nothing left under it would be a dangling claim;
    // the withheld note covers what was under it.
    if let Some(h) = warn_header {
        keep[h] = keep[h + 1..].iter().any(|&k| k);
    }
    let withheld = denied + unattributed;
    if withheld == 0 {
        return Filtered {
            text: output.to_string(),
            withheld: 0,
            recounted_by: 0,
        };
    }
    // Re-count yoagent's footer to the result lines actually shown, so the
    // model is not handed a count that includes what was withheld (#1023).
    let footer = lines
        .iter()
        .rposition(|l| recount_footer(tool, l, 0).is_some())
        .and_then(|f| {
            let shown = (0..f)
                .filter(|&i| keep[i] && !lines[i].trim_end_matches('\r').is_empty())
                .count();
            recount_footer(tool, lines[f], shown).map(|(new, old)| (f, new, old - old.min(shown)))
        });
    let recounted_by = footer.as_ref().map_or(0, |(_, _, by)| *by);
    let mut kept: Vec<&str> = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        match &footer {
            Some((f, new, _)) if *f == idx => kept.push(new),
            _ if keep[idx] => kept.push(line),
            _ => {}
        }
    }
    let mut text = kept.join("\n");
    for note in [
        (denied > 0).then(|| withheld_note(denied)),
        (unattributed > 0).then(|| unattributed_note(unattributed)),
    ]
    .into_iter()
    .flatten()
    {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&note);
    }
    Filtered {
        text,
        withheld,
        recounted_by,
    }
}

/// Apply the filter to every text block of a successful tool result. Non-text
/// blocks and the `details` JSON pass through untouched; tools other than
/// `search` / `list_files` are returned unchanged.
///
/// `fence_active` must be the UNION of every fence source (startup restrictions
/// and the `/cd` lists), and `root` the `path` the tool walked (`.` when the
/// argument was absent, as yoagent defaults it). Fail-closed mode is decided
/// here, inside the one function every guarded search result passes through,
/// so no caller can forget it.
pub(crate) fn fence_tool_result(
    tool: &str,
    mut result: yoagent::types::ToolResult,
    refused: &dyn Fn(&str) -> bool,
    fence_active: bool,
    root: &str,
) -> yoagent::types::ToolResult {
    if !RECURSIVE_READERS.contains(&tool) {
        return result;
    }
    let strict_root = (fence_active && !std::path::Path::new(root).is_file()).then_some(root);
    let mut recounted_by = 0usize;
    for block in result.content.iter_mut() {
        if let yoagent::types::Content::Text { text } = block {
            let f = filter_fenced(tool, text, refused, strict_root);
            if f.withheld > 0 {
                *text = f.text;
                recounted_by += f.recounted_by;
            }
        }
    }
    // The same count lives in `details` (`matches` for search, `total` for
    // list_files); lower it by exactly what the footer was lowered by, so the
    // two never disagree. A missing or non-numeric field is left alone.
    let key = if tool == "search" { "matches" } else { "total" };
    if recounted_by > 0 {
        if let Some(n) = result.details.get(key).and_then(|v| v.as_u64()) {
            result.details[key] = serde_json::json!(n.saturating_sub(recounted_by as u64));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DirectoryRestrictions;

    const CANARY: &str = "CANARY_Q7Z";

    /// A tempdir holding `secrets/key.txt` (the canary) and `src/a.txt`, plus a
    /// predicate refusing everything under `<tmp>/secrets` via the REAL
    /// `DirectoryRestrictions::check_path` (absolute paths, so cwd is not read).
    fn fixture() -> (tempfile::TempDir, String, DirectoryRestrictions) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().to_string();
        std::fs::create_dir_all(dir.path().join("secrets")).unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("secrets/key.txt"),
            format!("token={CANARY}\n"),
        )
        .unwrap();
        std::fs::write(dir.path().join("src/a.txt"), format!("ok {CANARY}\n")).unwrap();
        let r = DirectoryRestrictions {
            allow: vec![],
            deny: vec![format!("{root}/secrets")],
        };
        (dir, root, r)
    }

    #[test]
    fn search_result_naming_a_denied_file_is_withheld_and_sibling_survives() {
        let (dir, root, r) = fixture();
        // Anti-vacuous: the canary really is on disk inside the denied dir.
        let on_disk = std::fs::read_to_string(dir.path().join("secrets/key.txt")).unwrap();
        assert!(on_disk.contains(CANARY));
        let sibling = format!("{root}/src/a.txt:1:ok {CANARY}");
        let output = format!("{root}/secrets/key.txt:1:token={CANARY}\n{sibling}\n(2 matches)");
        let refused = |p: &str| r.check_path(p).is_err();
        let (out, n) = filter_fenced_lines("search", &output, &refused, None);
        assert_eq!(n, 1);
        assert!(!out.contains("token="), "canary leaked: {out}");
        assert!(
            !out.contains("secrets/key.txt"),
            "denied path leaked: {out}"
        );
        assert!(out.contains(&sibling), "allowed sibling lost: {out}");
        assert!(out.contains("[1 result withheld:"), "note missing: {out}");
    }

    #[test]
    fn list_files_entry_inside_denied_dir_is_withheld() {
        let (_dir, root, r) = fixture();
        let output = format!("{root}/secrets/key.txt\n{root}/src/a.txt\n\n(2 files)");
        let refused = |p: &str| r.check_path(p).is_err();
        let (out, n) = filter_fenced_lines("list_files", &output, &refused, None);
        assert_eq!(n, 1);
        assert!(!out.contains("secrets/key.txt"), "{out}");
        assert!(out.contains(&format!("{root}/src/a.txt")), "{out}");
        assert!(out.ends_with(&withheld_note(1)), "{out}");
    }

    #[test]
    fn output_with_nothing_denied_is_byte_identical() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        for (tool, output) in [
            (
                "search",
                format!("{root}/src/a.txt:1:ok {CANARY}\n(1 matches)"),
            ),
            ("list_files", format!("{root}/src/a.txt\n\n(1 files)")),
            ("search", "No matches found for 'x'".to_string()),
        ] {
            let (out, n) = filter_fenced_lines(tool, &output, &refused, None);
            assert_eq!(n, 0);
            assert_eq!(out, output);
        }
    }

    #[test]
    fn other_tools_are_never_filtered() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        let output = format!("{root}/secrets/key.txt:1:token={CANARY}");
        let (out, n) = filter_fenced_lines("read_file", &output, &refused, None);
        assert_eq!((out.as_str(), n), (output.as_str(), 0));
    }

    #[test]
    fn search_path_splits_on_the_first_colon_digits_colon() {
        assert_eq!(
            search_result_path("src/a.rs:3:let x = \"b:12:c\""),
            Some("src/a.rs")
        );
        assert_eq!(search_result_path("(3 matches)"), None);
        assert_eq!(search_result_path("12:bare single-file hit"), None);
        assert_eq!(search_result_path("a:b:c"), None);
        assert_eq!(search_result_path(""), None);
        // Multi-byte text must not panic (no byte-indexing past a boundary).
        assert_eq!(search_result_path("ü/✓.rs:7:✓:9:✓"), Some("ü/✓.rs"));
    }

    #[test]
    fn a_name_containing_colon_digits_fails_closed() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        // First split lands at `weird` (allowed); a later split names the
        // denied file — the line is withheld rather than leaked.
        let line = format!("{root}/weird:1:x/../secrets/key.txt:4:token");
        let (out, n) = filter_fenced_lines("search", &line, &refused, None);
        assert_eq!(n, 1, "{out}");
    }

    /// Bodies `search` returned through the real guarded yoagent tool with
    /// `RIPGREP_CONFIG_PATH` naming a one-flag config (ripgrep 14.1.0, Day 229
    /// probe, `{R}` = the searched root, `secrets/` denied). Every row named
    /// the denied file or carried its line, and every row passed the
    /// pre-#1023 filter untouched. Since yoagent 0.25.5 a config can no longer
    /// produce these, but a wrapper `rg` on `PATH` still can, so they stay.
    fn reshaped_bodies(root: &str) -> Vec<(&'static str, String)> {
        vec![
            ("--no-filename", format!("1:token={CANARY}\n1:ok {CANARY}\n(2 matches)")),
            (
                "--null",
                format!("{root}/secrets/key.txt\u{0}1:token={CANARY}\n{root}/src/a.txt\u{0}1:ok {CANARY}\n(2 matches)"),
            ),
            (
                "--json",
                format!("{{\"type\":\"match\",\"data\":{{\"path\":{{\"text\":\"{root}/secrets/key.txt\"}},\"lines\":{{\"text\":\"token={CANARY}\\n\"}},\"line_number\":1}}}}\n(1 matches)"),
            ),
            (
                "--path-separator=X",
                format!("{}Xsecrets/key.txt:1:token={CANARY}\n(1 matches)", root.replace('/', "X")),
            ),
            (
                "-C1 context line",
                format!("{root}/secrets/key.txt-2-token={CANARY}\n--\n{root}/src/a.txt:1:ok {CANARY}\n(2 matches)"),
            ),
        ]
    }

    #[test]
    fn unattributable_lines_under_an_active_fence_are_dropped() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        for (flag, body) in reshaped_bodies(&root) {
            // Anti-vacuous: the fixture really carries the denied file's line.
            assert!(body.contains(&format!("token={CANARY}")), "{flag}");
            let (out, n) = filter_fenced_lines("search", &body, &refused, Some(&root));
            assert!(n > 0, "{flag}: nothing withheld: {out}");
            assert!(!out.contains("token="), "{flag}: canary leaked: {out}");
            assert!(
                !out.contains("secrets"),
                "{flag}: denied path leaked: {out}"
            );
            assert!(out.contains("withheld"), "{flag}: no note: {out}");
        }
    }

    #[test]
    fn reshaped_bodies_without_a_fence_are_byte_identical() {
        let (_dir, root, _r) = fixture();
        // No fence: nothing is refused and strict mode is off.
        let refused = |_: &str| false;
        for (flag, body) in reshaped_bodies(&root) {
            let (out, n) = filter_fenced_lines("search", &body, &refused, None);
            assert_eq!((out.as_str(), n), (body.as_str(), 0), "{flag}");
        }
    }

    #[test]
    fn ordinary_results_under_an_active_fence_pass_byte_identically() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        for (root_arg, body) in [
            (
                root.clone(),
                format!("{root}/src/a.txt:1:ok {CANARY}\n{root}/src/a.txt:2:x:7:y\n(2 matches)"),
            ),
            (
                root.clone(),
                format!("{root}/src/a.txt:1:ok\n... (showing the first 1 matches; there are more. Narrow the pattern, path or include.)\nWarnings:\nrg: {root}/src/x: Permission denied (os error 13)"),
            ),
            (".".to_string(), "./src/a.txt:1:ok\n(1 matches)".to_string()),
            (root.clone(), format!("No matches found for '{CANARY}'")),
        ] {
            let (out, n) = filter_fenced_lines("search", &body, &refused, Some(&root_arg));
            assert_eq!((out.as_str(), n), (body.as_str(), 0));
        }
    }

    #[test]
    fn a_single_file_root_is_not_put_in_fail_closed_mode() {
        let (dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        let file = dir.path().join("src/a.txt").to_string_lossy().to_string();
        let text = format!("1:ok {CANARY}\n(1 matches)");
        let mk = || yoagent::types::ToolResult {
            content: vec![yoagent::types::Content::Text { text: text.clone() }],
            details: serde_json::json!({}),
        };
        assert_eq!(
            text_of(&fence_tool_result("search", mk(), &refused, true, &file)),
            text
        );
        // Near-miss: the same bare line under a DIRECTORY root is dropped.
        let dropped = text_of(&fence_tool_result("search", mk(), &refused, true, &root));
        assert!(!dropped.contains(CANARY), "{dropped}");
        assert!(dropped.contains("[1 line withheld:"), "{dropped}");
    }

    fn text_of(res: &yoagent::types::ToolResult) -> String {
        res.content
            .iter()
            .filter_map(|b| match b {
                yoagent::types::Content::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// End to end through the REAL wrappers and the REAL yoagent tools: the
    /// argument (the tempdir root) is allowed, the walk reaches `secrets/`, and
    /// the result the model would receive must not carry the canary. Both the
    /// Box path (main agent) and the Arc path (sub-agents) are driven.
    #[tokio::test]
    async fn real_guarded_search_and_list_files_withhold_the_denied_subtree() {
        use yoagent::types::ToolContext;
        let (_dir, root, r) = fixture();
        let search = crate::tool_wrappers::maybe_guard(
            Box::new(yoagent::tools::search::SearchTool::default()),
            &r,
        );
        let res = search
            .execute(
                serde_json::json!({"pattern": CANARY, "path": root}),
                ToolContext::new("c1", "search"),
            )
            .await
            .expect("search over an allowed root succeeds");
        let out = text_of(&res);
        assert!(!out.contains("token="), "canary leaked via search: {out}");
        assert!(out.contains("src/a.txt"), "allowed hit lost: {out}");
        assert!(out.contains("[1 result withheld:"), "{out}");
        // The REAL footer is recounted: 2 hits on disk, 1 shown (#1023).
        assert!(
            out.contains("\n(1 matches)\n"),
            "footer not recounted: {out}"
        );
        assert_eq!(res.details["matches"], serde_json::json!(1), "{out}");
        // The fence is active here, so fail-closed mode is on: yoagent's own
        // footer must still be recognised, or this note would appear (#1023).
        assert!(
            !out.contains("line withheld"),
            "footer not recognised: {out}"
        );
        assert!(
            !out.contains("lines withheld"),
            "footer not recognised: {out}"
        );

        let list = crate::tool_wrappers::maybe_guard_arc(
            std::sync::Arc::new(yoagent::tools::list::ListFilesTool::default()),
            &r,
        );
        let res = list
            .execute(
                serde_json::json!({"path": root}),
                ToolContext::new("c2", "list_files"),
            )
            .await
            .expect("list_files over an allowed root succeeds");
        let out = text_of(&res);
        assert!(
            !out.contains("secrets/key.txt"),
            "listed denied file: {out}"
        );
        assert!(out.contains("src/a.txt"), "{out}");
        assert!(out.contains("[1 result withheld:"), "{out}");
        assert!(out.contains("\n(1 files)\n"), "footer not recounted: {out}");
        assert_eq!(res.details["total"], serde_json::json!(1), "{out}");
    }

    #[test]
    fn fence_tool_result_rewrites_text_blocks_of_search_only() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        let text = format!("{root}/secrets/key.txt:1:token={CANARY}\n(1 matches)");
        let mk = || yoagent::types::ToolResult {
            content: vec![yoagent::types::Content::Text { text: text.clone() }],
            details: serde_json::json!({}),
        };
        let get = |res: &yoagent::types::ToolResult| match &res.content[0] {
            yoagent::types::Content::Text { text } => text.clone(),
            _ => panic!("not text"),
        };
        let fenced = fence_tool_result("search", mk(), &refused, false, &root);
        assert!(!get(&fenced).contains(CANARY));
        let untouched = fence_tool_result("bash", mk(), &refused, true, &root);
        assert_eq!(get(&untouched), text);
    }

    /// The note the MODEL receives for an unattributable line, pinned at the
    /// emission point (`fence_tool_result`'s text block). The fixture is the
    /// shape measured on GNU grep 3.11 for a file named `a\nb.txt` (Day 230):
    /// the name's newline splits the result into two lines, neither naming a
    /// file under the root. It must not blame a ripgrep config: yoagent 0.25.5
    /// passes `--no-config` and clears `RIPGREP_CONFIG_PATH`, so the old
    /// parenthetical told the model a cause that can no longer happen.
    #[test]
    fn unattributed_note_names_no_cause_the_code_cannot_know() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        let body = format!("{root}/src/a.txt:1:ok {CANARY}\n{root}/src/x\ny.txt:1:ok\n(2 matches)");
        // Anti-vacuous: the split line really is unattributable on its own.
        assert!(!attributable_to_root("y.txt:1:ok", &root));
        let res = yoagent::types::ToolResult {
            content: vec![yoagent::types::Content::Text { text: body }],
            details: serde_json::json!({}),
        };
        let out = text_of(&fence_tool_result("search", res, &refused, true, &root));
        assert_eq!(
            out,
            format!(
                "{root}/src/a.txt:1:ok {CANARY}\n(1 matches)\n[2 lines withheld: a directory fence is active and the line does not name a file under the searched path in the usual path:line: form, so the fence could not check it and dropped it rather than risk a leak]"
            )
        );
        for stale in ["RIPGREP_CONFIG_PATH", "ripgrep config"] {
            assert!(!out.contains(stale), "note names a dead cause: {out}");
        }
        assert_eq!(
            unattributed_note(1),
            "[1 line withheld: a directory fence is active and the line does not name a file under the searched path in the usual path:line: form, so the fence could not check it and dropped it rather than risk a leak]"
        );
    }

    /// #1023 item 2, at the emission point: the footer the MODEL receives
    /// counts only the result lines it can see, and `details` agrees.
    #[test]
    fn search_footer_counts_only_the_lines_the_fence_let_through() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        let text = format!(
            "{root}/secrets/key.txt:1:token={CANARY}\n{root}/src/a.txt:1:ok {CANARY}\n{root}/src/b.txt:2:ok\n(3 matches)"
        );
        // Anti-vacuous: the fixture carries the footer spelling yoagent emits
        // (`format!("{}\n({} matches)", ...)` in yoagent's tools/search.rs).
        assert!(text.ends_with("\n(3 matches)"));
        assert!(is_search_footer("(3 matches)"));
        let res = yoagent::types::ToolResult {
            content: vec![yoagent::types::Content::Text { text }],
            details: serde_json::json!({"matches": 3, "truncated": false}),
        };
        let out = fence_tool_result("search", res, &refused, true, &root);
        assert_eq!(
            text_of(&out),
            format!(
                "{root}/src/a.txt:1:ok {CANARY}\n{root}/src/b.txt:2:ok\n(2 matches)\n{}",
                withheld_note(1)
            )
        );
        assert_eq!(
            out.details,
            serde_json::json!({"matches": 2, "truncated": false})
        );
    }

    #[test]
    fn truncated_search_and_list_footers_are_recounted() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        let more = format!(
            "{root}/secrets/key.txt:1:x\n{root}/src/a.txt:1:y\n... (showing the first 2 matches; there are more. Narrow the pattern, path or include.)"
        );
        let (out, n) = filter_fenced_lines("search", &more, &refused, None);
        assert_eq!(n, 1);
        assert_eq!(
            out,
            format!(
                "{root}/src/a.txt:1:y\n... (showing the first 1 matches; there are more. Narrow the pattern, path or include.)\n{}",
                withheld_note(1)
            )
        );
        let list = format!("{root}/secrets/key.txt\n{root}/src/a.txt\n\n(2 files)");
        let res = yoagent::types::ToolResult {
            content: vec![yoagent::types::Content::Text { text: list }],
            details: serde_json::json!({"total": 2}),
        };
        let out = fence_tool_result("list_files", res, &refused, true, &root);
        assert_eq!(
            text_of(&out),
            format!("{root}/src/a.txt\n\n(1 files)\n{}", withheld_note(1))
        );
        assert_eq!(out.details, serde_json::json!({"total": 1}));
        let capped =
            format!("{root}/secrets/key.txt\n{root}/src/a.txt\n\n... (9 files, showing first 2)");
        let (out, _) = filter_fenced_lines("list_files", &capped, &refused, None);
        assert_eq!(
            out,
            format!(
                "{root}/src/a.txt\n\n... (8 files, showing first 1)\n{}",
                withheld_note(1)
            )
        );
    }

    /// Near-miss: nothing withheld -> text AND details byte-identical.
    #[test]
    fn footer_and_details_untouched_when_nothing_is_withheld() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        let text = format!("{root}/src/a.txt:1:ok\n{root}/src/b.txt:2:ok\n(2 matches)");
        let details = serde_json::json!({"matches": 2, "truncated": false});
        let res = yoagent::types::ToolResult {
            content: vec![yoagent::types::Content::Text { text: text.clone() }],
            details: details.clone(),
        };
        let out = fence_tool_result("search", res, &refused, true, &root);
        assert_eq!(text_of(&out), text);
        assert_eq!(out.details, details);
    }

    /// No recognisable footer -> none is invented, and details stay put.
    #[test]
    fn an_absent_or_unparseable_footer_is_left_alone() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        for tail in ["", "\n(many matches)", "\n(3 hits)"] {
            let text = format!("{root}/secrets/key.txt:1:x\n{root}/src/a.txt:1:y{tail}");
            let res = yoagent::types::ToolResult {
                content: vec![yoagent::types::Content::Text { text }],
                details: serde_json::json!({"matches": 2}),
            };
            let out = fence_tool_result("search", res, &refused, false, &root);
            assert_eq!(
                text_of(&out),
                format!("{root}/src/a.txt:1:y{tail}\n{}", withheld_note(1))
            );
            assert_eq!(out.details, serde_json::json!({"matches": 2}));
        }
    }

    /// `list_files` output in yoagent's exact shape: format strings copied from
    /// yoagent 0.25 `src/tools/list.rs` (`"{}\n\n({} files)"`, then
    /// `"\nWarnings from find (the listing may be incomplete):\n{w}"`).
    fn list_output(files: &[String], warnings: Option<&str>) -> String {
        let mut text = format!("{}\n\n({} files)", files.join("\n"), files.len());
        if let Some(w) = warnings {
            text.push_str(&format!(
                "\nWarnings from find (the listing may be incomplete):\n{w}"
            ));
        }
        text
    }

    #[test]
    fn find_warning_naming_a_denied_path_is_withheld_and_header_dropped() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        let files = vec![format!("{root}/src/a.txt")];
        // GNU find's UTF-8-locale quoting, as `find` on this runner prints it.
        let w = format!("find: \u{2018}{root}/secrets/keys\u{2019}: Permission denied");
        let output = list_output(&files, Some(&w));
        let (out, n) = filter_fenced_lines("list_files", &output, &refused, None);
        assert_eq!(n, 1, "{out}");
        assert_eq!(
            out,
            format!("{root}/src/a.txt\n\n(1 files)\n{}", withheld_note(1)),
            "denied warning withheld, header dropped, footer count unchanged"
        );
    }

    #[test]
    fn find_warning_naming_an_allowed_path_is_kept_byte_identically() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        let files = vec![format!("{root}/src/a.txt")];
        let w = format!("find: \u{2018}{root}/src/locked\u{2019}: Permission denied");
        let output = list_output(&files, Some(&w));
        // Anti-vacuous: the fixture really carries the header and a warning.
        assert!(output.contains("Warnings from find") && output.contains("src/locked"));
        let (out, n) = filter_fenced_lines("list_files", &output, &refused, None);
        assert_eq!(n, 0);
        assert_eq!(out, output);
    }

    #[test]
    fn mixed_find_warnings_keep_header_and_allowed_line_only() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        let files = vec![format!("{root}/src/a.txt")];
        let allowed = format!("find: '{root}/src/locked': Permission denied");
        let w = format!(
            "find: '{root}/secrets/k\\'s: x': Permission denied\n{allowed}\nfind: {root}/secrets/bsd: Permission denied"
        );
        let output = list_output(&files, Some(&w));
        let (out, n) = filter_fenced_lines("list_files", &output, &refused, None);
        assert_eq!(n, 2, "{out}");
        assert_eq!(
            out,
            format!(
                "{root}/src/a.txt\n\n(1 files)\nWarnings from find (the listing may be incomplete):\n{allowed}\n{}",
                withheld_note(2)
            )
        );
    }

    #[test]
    fn a_warning_cut_mid_name_by_the_cap_is_still_checked() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        let files = vec![format!("{root}/src/a.txt")];
        let w = format!("find: \u{2018}{root}/secrets/ke\n... (more warnings not shown)");
        let output = list_output(&files, Some(&w));
        let (out, n) = filter_fenced_lines("list_files", &output, &refused, None);
        assert_eq!(n, 1, "{out}");
        assert!(!out.contains("secrets"), "{out}");
        // The cap line survives, so the header above it does too.
        assert!(
            out.contains("incomplete):\n... (more warnings not shown)\n"),
            "{out}"
        );
    }

    #[test]
    fn non_path_find_warnings_are_never_checked() {
        let root = "/r";
        assert!(find_warning_paths("find: warning: odd thing").is_empty());
        assert!(find_warning_paths("not find output").is_empty());
        assert_eq!(
            find_warning_paths(&format!("find: '{root}/a\\'b': Permission denied")),
            vec![format!("{root}/a'b")]
        );
    }

    #[test]
    fn list_files_own_lines_are_not_path_checked_under_an_allow_list() {
        // Under an allow list, a relative `(1 files)` resolved against the cwd
        // is "not under any allowed directory": it must not count as withheld.
        let (_dir, root, _r) = fixture();
        let r = DirectoryRestrictions {
            allow: vec![format!("{root}/src")],
            deny: vec![],
        };
        let refused = |p: &str| r.check_path(p).is_err();
        // Anti-vacuous: the footer really is refused as a path here.
        assert!(refused("(1 files)") && refused("No files found in x"));
        let files = vec![format!("{root}/src/a.txt")];
        let w = format!("find: \u{2018}{root}/src/locked\u{2019}: Permission denied");
        for output in [
            list_output(&files, None),
            list_output(&files, Some(&w)),
            format!("No files found in {root}/src"),
        ] {
            let (out, n) = filter_fenced_lines("list_files", &output, &refused, None);
            assert_eq!((n, out.as_str()), (0, output.as_str()));
        }
    }

    #[test]
    fn search_output_mentioning_find_warnings_is_unaffected() {
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        let output = format!(
            "{root}/src/a.txt:1:Warnings from find (the listing may be incomplete):\n{root}/src/a.txt:2:find: '{root}/secrets/x': y\n(2 matches)"
        );
        let (out, n) = filter_fenced_lines("search", &output, &refused, None);
        assert_eq!((n, out.as_str()), (0, output.as_str()));
    }

    #[test]
    fn listing_error_warnings_naming_a_denied_path_are_withheld() {
        use yoagent::types::ToolError;
        let (_dir, root, r) = fixture();
        let refused = |p: &str| r.check_path(p).is_err();
        let msg = |w: &str| format!("Listing error: {w}");
        let denied = format!("find: \u{2018}{root}/secrets\u{2019}: Permission denied");
        let allowed = format!("find: \u{2018}{root}/src\u{2019}: Permission denied");
        let text = |e: ToolError| match e {
            ToolError::Failed(m) => m,
            other => panic!("{other:?}"),
        };
        let all = text(fence_tool_error(
            "list_files",
            ToolError::Failed(msg(&denied)),
            &refused,
        ));
        assert_eq!(all, msg(&withheld_note(1)));
        let mixed = text(fence_tool_error(
            "list_files",
            ToolError::Failed(msg(&format!("{denied}\n{allowed}"))),
            &refused,
        ));
        assert_eq!(mixed, msg(&format!("{allowed}\n{}", withheld_note(1))));
        // Nothing denied, another tool, another error: untouched.
        for (tool, m) in [
            ("list_files", msg(&allowed)),
            ("search", msg(&denied)),
            ("list_files", format!("Directory not found: {root}/secrets")),
        ] {
            assert_eq!(
                text(fence_tool_error(
                    tool,
                    ToolError::Failed(m.clone()),
                    &refused
                )),
                m
            );
        }
    }

    /// The `--allow-dir` probe (#1023): through the REAL wrapper and the REAL
    /// yoagent tools, a root above the allowed dir is refused at the argument
    /// check (nothing walked, nothing to leak), and listing the allowed dir
    /// itself from a cwd outside it withholds nothing.
    #[tokio::test]
    async fn allow_dir_refuses_a_parent_root_and_lists_the_allowed_one_cleanly() {
        use yoagent::types::ToolContext;
        let (_dir, root, _r) = fixture();
        let r = DirectoryRestrictions {
            allow: vec![format!("{root}/src")],
            deny: vec![],
        };
        for tool in ["list_files", "search"] {
            let inner: Box<dyn yoagent::types::AgentTool> = if tool == "search" {
                Box::new(yoagent::tools::search::SearchTool::default())
            } else {
                Box::new(yoagent::tools::list::ListFilesTool::default())
            };
            let t = crate::tool_wrappers::maybe_guard(inner, &r);
            let args = serde_json::json!({"pattern": CANARY, "path": root});
            let err = t
                .execute(args, ToolContext::new("c", tool))
                .await
                .expect_err("a parent of the allowed dir is refused");
            assert!(
                err.to_string().contains("not under any allowed directory"),
                "{tool}: {err}"
            );
        }
        let list = crate::tool_wrappers::maybe_guard(
            Box::new(yoagent::tools::list::ListFilesTool::default()),
            &r,
        );
        let res = list
            .execute(
                serde_json::json!({"path": format!("{root}/src")}),
                ToolContext::new("c", "list_files"),
            )
            .await
            .expect("the allowed dir lists");
        let out = text_of(&res);
        assert_eq!(out, format!("{root}/src/a.txt\n\n(1 files)"));
    }
}
