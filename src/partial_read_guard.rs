//! `write_file` refuses to overwrite a file the session has only partly read.
//!
//! yoagent's `read_file` pages: with no `limit` it returns at most
//! `DEFAULT_READ_MAX_LINES` (500) lines under a header like
//! `[Lines 1-500 of 2000 — use offset/limit to read the rest]`, and
//! `write_file` overwrites unconditionally. So a model that reads page 1 of a
//! 2,000-line file and then writes "the whole file" back silently destroys
//! lines 501-2000. This module closes that hole.
//!
//! **What it records.** For every successful `read_file`, the line range the
//! header reports (a `[N lines]` header means the whole file) is added to a
//! store keyed by canonical path. A successful `write_file` marks the path
//! fully covered at its new length (the model wrote every line). A successful
//! `edit_file` on a fully covered file keeps it fully covered at the new
//! length; on a partly covered one it leaves the ranges alone (their numbering
//! may now be stale, which can only make the guard refuse, never allow).
//!
//! **What it refuses.** A `write_file` whose target (a) exists as a regular
//! file, (b) has a recorded read this session, and (c) has lines on disk NOW
//! that no recorded read covered. The decision is keyed on state (the line
//! count on disk at write time against the lines seen), not on how the read
//! happened, so a file that grew after a whole read is refused too.
//! Lines are counted the way yoagent counts them: `str::lines()` on the text.
//!
//! **The refusal** names the path, the line count and the lines read, and gives
//! the two legal moves: read the rest with `read_file` offset/limit, or change
//! the file in place with `edit_file`. It carries [`REFUSAL_STEM_PARTIAL_READ`],
//! which `prompt_retry::is_deterministic_tool_error` recognises, so the
//! recovery-hint wrapper does not append "try a simpler version" and the
//! refusal does not make the prompt eligible for auto-retry (#1068's class).
//!
//! **Not in this slice, named rather than implied (all four tracked in #1094):**
//! - An existing file that was **never read** is not refused (Claude Code
//!   requires a read before any overwrite; that is a broader product decision).
//! - The store is **process-wide**: a sub-agent's read counts as coverage for
//!   the parent and vice versa.
//! - Coverage is taken from the header yoagent returns, **inside** yoyo's
//!   output-truncation wrapper. If `TruncatingTool` cuts the middle out of a
//!   long page, the header still claims those lines and the guard counts them
//!   as seen, so a write after a truncated read is allowed (no worse than
//!   before this guard existed).
//! - A file changed by `bash` (or another process) is judged only by its line
//!   count: a shrink or a same-length rewrite is not detected.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use yoagent::types::{ToolError, ToolResult};

/// Stem every partial-read refusal carries; matched as a substring by
/// `prompt_retry::is_deterministic_tool_error`, like the `REFUSAL_STEM_*`
/// consts in `tool_wrappers.rs`.
pub const REFUSAL_STEM_PARTIAL_READ: &str = " but this session has only read ";

/// Line ranges (1-indexed, inclusive) seen per canonical path.
pub(crate) type Store = Mutex<HashMap<PathBuf, Vec<(usize, usize)>>>;

fn global_store() -> &'static Store {
    static STORE: OnceLock<Store> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Parse yoagent `read_file`'s header line into `(first, last, total)`.
/// `[Lines a-b of N — …]` is a page; `[N lines]` is the whole file.
/// Anything else (an image, an unknown shape) is `None` and records nothing.
pub(crate) fn parse_read_header(text: &str) -> Option<(usize, usize, usize)> {
    let first = text.lines().next()?.trim();
    let inner = first.strip_prefix('[')?;
    if let Some(rest) = inner.strip_prefix("Lines ") {
        let (range, rest) = rest.split_once(" of ")?;
        let (a, b) = range.split_once('-')?;
        let total: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        return Some((a.parse().ok()?, b.parse().ok()?, total.parse().ok()?));
    }
    let n = inner.strip_suffix(" lines]")?.parse().ok()?;
    Some((1, n, n))
}

/// Count lines the way yoagent's `read_file` does (`str::lines()`).
pub(crate) fn count_lines(bytes: &[u8]) -> usize {
    String::from_utf8_lossy(bytes).lines().count()
}

/// Merge ranges (dropping empty ones such as an offset past the end).
fn merged(ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut v: Vec<(usize, usize)> = ranges.iter().copied().filter(|(a, b)| a <= b).collect();
    v.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (a, b) in v {
        match out.last_mut() {
            Some(last) if a <= last.1.saturating_add(1) => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// True when the ranges cover every line `1..=total`.
pub(crate) fn covers_all(ranges: &[(usize, usize)], total: usize) -> bool {
    if total == 0 {
        return true;
    }
    matches!(merged(ranges).first(), Some(&(1, b)) if b >= total)
}

/// The decision, pure: refuse overwriting `path` (shown to the model) when the
/// ranges seen do not cover its current `current_total` lines.
pub(crate) fn overwrite_refusal(
    path: &str,
    covered: &[(usize, usize)],
    current_total: usize,
) -> Option<String> {
    if covers_all(covered, current_total) {
        return None;
    }
    let seen: Vec<String> = merged(covered)
        .iter()
        .map(|(a, b)| format!("{a}-{b}"))
        .collect();
    let seen = if seen.is_empty() {
        "none of its lines".to_string()
    } else {
        format!("lines {}", seen.join(", "))
    };
    Some(format!(
        "Refusing to overwrite '{}': it has {} lines{}{}, so writing it whole would destroy \
         the lines you have not seen. Read the rest with read_file offset/limit, or change \
         it in place with edit_file.",
        crate::cli::sanitize_for_display(path),
        current_total,
        REFUSAL_STEM_PARTIAL_READ,
        seen
    ))
}

fn canonical(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(Path::new(path)).ok()
}

fn lines_on_disk(p: &Path) -> Option<usize> {
    let meta = std::fs::metadata(p).ok()?;
    if !meta.is_file() {
        return None;
    }
    std::fs::read(p).ok().map(|b| count_lines(&b))
}

/// What to record once the inner tool succeeds.
pub(crate) struct Pending {
    kind: Kind,
    path: String,
}

enum Kind {
    Read,
    Write,
    Edit { was_full: bool },
}

/// Before the tool runs: refuse a partial-read overwrite, or return what to
/// record after it succeeds. Tools other than read/write/edit pass through.
pub(crate) fn gate_with(
    store: &Store,
    tool: &str,
    path: Option<&str>,
) -> Result<Option<Pending>, ToolError> {
    let Some(path) = path else { return Ok(None) };
    let recorded = |p: &Path| store.lock().ok().and_then(|s| s.get(p).cloned());
    let kind = match tool {
        "read_file" => Kind::Read,
        "write_file" => {
            if let Some(c) = canonical(path) {
                if let (Some(ranges), Some(n)) = (recorded(&c), lines_on_disk(&c)) {
                    if let Some(r) = overwrite_refusal(path, &ranges, n) {
                        return Err(ToolError::Failed(r));
                    }
                }
            }
            Kind::Write
        }
        "edit_file" => {
            let was_full = canonical(path)
                .and_then(|c| Some(covers_all(&recorded(&c)?, lines_on_disk(&c)?)))
                .unwrap_or(false);
            Kind::Edit { was_full }
        }
        _ => return Ok(None),
    };
    Ok(Some(Pending {
        kind,
        path: path.to_string(),
    }))
}

/// After the tool succeeded: record what it showed or wrote.
pub(crate) fn record_with(store: &Store, pending: Option<Pending>, result: &ToolResult) {
    let Some(p) = pending else { return };
    let Some(c) = canonical(&p.path) else { return };
    let Ok(mut s) = store.lock() else { return };
    match p.kind {
        Kind::Read => {
            let text = result.content.iter().find_map(|c| match c {
                yoagent::Content::Text { text } => Some(text.as_str()),
                _ => None,
            });
            if let Some((a, b, _)) = text.and_then(parse_read_header) {
                let e = s.entry(c).or_default();
                e.push((a, b));
                *e = merged(e);
            }
        }
        Kind::Write | Kind::Edit { was_full: true } => {
            if let Some(n) = lines_on_disk(&c) {
                s.insert(c, vec![(1, n)]);
            }
        }
        Kind::Edit { was_full: false } => {}
    }
}

/// [`gate_with`] on the process-wide store (the `GuardedTool` call site).
pub(crate) fn gate(tool: &str, path: Option<&str>) -> Result<Option<Pending>, ToolError> {
    gate_with(global_store(), tool, path)
}

/// [`record_with`] on the process-wide store (the `GuardedTool` call site).
pub(crate) fn record(pending: Option<Pending>, result: &ToolResult) {
    record_with(global_store(), pending, result)
}

#[cfg(test)]
#[path = "partial_read_guard_tests.rs"]
mod partial_read_guard_tests;
