//! Day 215: the streamed (unreserved) stdout of the non-interactive doors —
//! plain `-p` and piped stdin without `--print` — stops leading with blank
//! lines.
//!
//! Day 212 fixed the two *reserved* doors (`--print`, `--output-format json`)
//! by passing the collected answer through `answer_payload`. The streamed door
//! writes deltas as they arrive, so it needs a streaming form of the same rule.
//! Measured from `/tmp`, no config, default model `claude-opus-4-6`:
//! `yoyo -p "Reply with exactly: PONG"` wrote `\n\n\nPONG\n\n`, where
//! `--output-format stream-json` shows the provider's first delta is
//! `"\n\nPONG"` — so two leading newlines were model output and the third was
//! yoyo's own "first text" framing newline (`write_stream_text("\n")` when
//! `in_text` flips). Both pass through the same stdout seam, so one filter
//! there drops both.
//!
//! There is exactly ONE statement of the rule: [`strip_leading_blank_lines`].
//! `main.rs`'s `answer_payload` delegates to it, and [`LeadingBlankFilter`]
//! applies it to a growing buffer of the whitespace prefix only.
//!
//! The filter is **off by default** (`pending: None` = passthrough), so the
//! interactive REPL, which never arms it, stays byte-identical.

use std::borrow::Cow;
use std::sync::Mutex;

/// Drop leading whitespace-only LINES and nothing else: the first line with
/// real content keeps its indentation, and everything after it — interior
/// blank lines, trailing newlines — is byte-identical. A trailing partial
/// whitespace run with no newline (`"  "`) is kept: it may be indentation.
pub(crate) fn strip_leading_blank_lines(text: &str) -> &str {
    let mut out = text;
    while let Some((line, after)) = out.split_once('\n') {
        if !line.trim().is_empty() {
            break;
        }
        out = after;
    }
    out
}

/// Streaming form of [`strip_leading_blank_lines`]. `pending: None` passes
/// everything through; `Some(buf)` holds the whitespace prefix seen so far.
/// Only whitespace is ever buffered — the delta carrying the first
/// non-whitespace character is released immediately, so real text is never
/// delayed.
#[derive(Debug, Default)]
pub(crate) struct LeadingBlankFilter {
    pending: Option<String>,
}

impl LeadingBlankFilter {
    /// A filter that strips until the first non-whitespace character.
    pub(crate) fn armed() -> Self {
        Self {
            pending: Some(String::new()),
        }
    }

    /// Bytes to write for this delta.
    pub(crate) fn feed<'a>(&mut self, s: &'a str) -> Cow<'a, str> {
        let Some(buf) = self.pending.as_mut() else {
            return Cow::Borrowed(s);
        };
        buf.push_str(s);
        let rest = strip_leading_blank_lines(buf);
        if rest.chars().any(|c| !c.is_whitespace()) {
            let out = rest.to_string();
            self.pending = None;
            Cow::Owned(out)
        } else {
            // `rest` is an unterminated whitespace run (complete blank lines
            // were already dropped): keep it in case it is indentation.
            let keep = rest.to_string();
            *buf = keep;
            Cow::Borrowed("")
        }
    }
}

/// The process's stdout filter. Unarmed (passthrough) unless a
/// non-interactive door calls [`arm`].
static STDOUT_FILTER: Mutex<LeadingBlankFilter> = Mutex::new(LeadingBlankFilter { pending: None });

/// Arm the filter for this process (non-interactive `-p` / piped, unreserved).
pub(crate) fn arm() {
    if let Ok(mut f) = STDOUT_FILTER.lock() {
        *f = LeadingBlankFilter::armed();
    }
}

/// Pass one stdout write through the process filter.
pub(crate) fn filter_stdout(s: &str) -> Cow<'_, str> {
    match STDOUT_FILTER.lock() {
        Ok(mut f) => match f.feed(s) {
            Cow::Borrowed(b) => Cow::Borrowed(b),
            Cow::Owned(o) => Cow::Owned(o),
        },
        // A poisoned lock must not swallow the answer: pass through.
        Err(_) => Cow::Borrowed(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Drive deltas through the filter into a byte buffer — the bytes a
    /// stdout would receive.
    fn written(filter: &mut LeadingBlankFilter, deltas: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for d in deltas {
            out.write_all(filter.feed(d).as_bytes()).unwrap();
        }
        out
    }

    #[test]
    fn armed_filter_drops_leading_blank_lines_split_across_deltas() {
        let cases: &[(&[&str], &str)] = &[
            // The measured shape: yoyo's framing "\n" then the model's "\n\nPONG".
            (&["\n", "\n\nPONG", "\n", "\n"], "PONG\n\n"),
            (&["\n", "\n", "PONG"], "PONG"),
            (&["\n\nPO", "NG\n"], "PONG\n"),
            (&["\r\n", " \t\n", "PONG"], "PONG"),
            // Whitespace split mid-line, then content: indentation survives.
            (&["\n", "  ", "  code"], "    code"),
        ];
        for (deltas, want) in cases {
            let mut f = LeadingBlankFilter::armed();
            assert_eq!(
                String::from_utf8(written(&mut f, deltas)).unwrap(),
                *want,
                "deltas {deltas:?}"
            );
        }
    }

    /// Near-miss: leading indentation and interior blank lines are kept.
    #[test]
    fn armed_filter_keeps_indentation_and_interior_blank_lines() {
        let mut f = LeadingBlankFilter::armed();
        assert_eq!(
            written(&mut f, &["  indented\n", "\nmore"]),
            b"  indented\n\nmore"
        );
    }

    /// Real text is released in the same delta that carries it — never held.
    #[test]
    fn armed_filter_never_delays_content() {
        let mut f = LeadingBlankFilter::armed();
        assert_eq!(f.feed("\n"), "");
        assert_eq!(f.feed("P"), "P");
        // Disarmed after content: later blank lines pass through untouched.
        assert_eq!(f.feed("\n\n"), "\n\n");
    }

    /// All-whitespace stream: writes nothing, does not panic or block.
    #[test]
    fn armed_filter_all_whitespace_writes_nothing() {
        let mut f = LeadingBlankFilter::armed();
        assert!(written(&mut f, &["\n", " \n", "\t", "\n", ""]).is_empty());
    }

    /// Near-miss: the default (unarmed) filter — the REPL — is byte-identical.
    #[test]
    fn unarmed_filter_is_byte_identical() {
        let mut f = LeadingBlankFilter::default();
        let deltas = ["\n", "\n\nPONG", "  x\n", "\n"];
        assert_eq!(written(&mut f, &deltas), deltas.concat().as_bytes());
    }
}
