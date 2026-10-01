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
//! The filter is **off by default** (`armed: false` = passthrough), so the
//! interactive REPL, which never arms it, stays byte-identical.
//!
//! **Trailing blank lines (Day 215, second task).** The same doors also ended
//! with a blank line: measured from an empty temp dir, no config,
//! `yoyo -p "Reply with exactly: PONG"` and the piped door both wrote
//! `PONG\n\n`. Neither newline is model text — they are yoyo's two end-of-turn
//! framing writes in `src/prompt.rs`: `write_stream_text("\n")` when
//! `state.in_text` is still set after the event loop (`handle_prompt_events`),
//! and the `write_stream_text("\n")` closing `finish_prompt_epilogue`. The
//! armed filter now holds back whitespace after the first line terminator
//! that follows content, so the output ends with exactly one `\n`.
//!
//! *Superseded claim, recorded rather than erased:* until this change the
//! rule's doc said everything after the first real line — "interior blank
//! lines, trailing newlines" — is byte-identical. For trailing newlines that
//! was a scope decision of the leading-blank task, not a measurement that the
//! trailing blank line was wanted. It still holds for the *reserved* doors'
//! payload (`answer_payload`, `--print` / `--output-format json`), which this
//! change does not touch.

use std::borrow::Cow;
use std::sync::Mutex;

/// Drop leading whitespace-only LINES and nothing else: the first line with
/// real content keeps its indentation, and everything after it — interior
/// blank lines, trailing newlines — is byte-identical. (The streamed doors'
/// trailing newlines are handled separately by [`LeadingBlankFilter`]'s
/// hold-back; this pure function is also the reserved payload's rule.) A trailing partial
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

/// Streaming form of [`strip_leading_blank_lines`], plus (Day 215, second
/// task) its trailing mirror. Unarmed (`armed: false`) passes everything
/// through. Armed, it does two things:
///
/// - **leading**: `leading: Some(buf)` holds the whitespace prefix seen so
///   far. Only whitespace is ever buffered — the delta carrying the first
///   non-whitespace character is released immediately, so real text is never
///   delayed.
/// - **trailing**: once a line has ended after content, any further
///   whitespace is held in `tail` instead of written. If content resumes the
///   held run is flushed verbatim first (so interior blank lines — e.g. inside
///   a code block — are byte-identical); if the turn ends, it is simply never
///   written. So the output ends with exactly ONE line terminator, and no
///   end-of-turn `finish()` call is needed — which matters because the doors
///   leave through `std::process::exit` on several error paths.
#[derive(Debug, Default)]
pub(crate) struct LeadingBlankFilter {
    armed: bool,
    leading: Option<String>,
    tail: String,
    line_ended: bool,
}

impl LeadingBlankFilter {
    /// A filter that strips until the first non-whitespace character.
    pub(crate) fn armed() -> Self {
        Self {
            armed: true,
            leading: Some(String::new()),
            tail: String::new(),
            line_ended: false,
        }
    }

    /// Bytes to write for this delta.
    pub(crate) fn feed<'a>(&mut self, s: &'a str) -> Cow<'a, str> {
        if !self.armed {
            return Cow::Borrowed(s);
        }
        let released: String = match self.leading.as_mut() {
            Some(buf) => {
                buf.push_str(s);
                let rest = strip_leading_blank_lines(buf);
                if rest.chars().any(|c| !c.is_whitespace()) {
                    let out = rest.to_string();
                    self.leading = None;
                    out
                } else {
                    // `rest` is an unterminated whitespace run (complete blank
                    // lines were already dropped): keep it in case it is
                    // indentation.
                    let keep = rest.to_string();
                    *buf = keep;
                    return Cow::Borrowed("");
                }
            }
            None => s.to_string(),
        };
        Cow::Owned(self.trailing(&released))
    }

    /// The trailing hold-back: returns the bytes to write for `s`.
    fn trailing(&mut self, s: &str) -> String {
        let mut out = String::new();
        let ws = match s.char_indices().rev().find(|(_, c)| !c.is_whitespace()) {
            Some((i, c)) => {
                // Content: flush any held run verbatim, then the content.
                let end = i + c.len_utf8();
                out.push_str(&std::mem::take(&mut self.tail));
                out.push_str(&s[..end]);
                self.line_ended = false;
                &s[end..]
            }
            None => s,
        };
        if self.line_ended {
            self.tail.push_str(ws);
        } else if let Some(p) = ws.find('\n') {
            // The first terminator after content ends the line; the rest waits.
            out.push_str(&ws[..=p]);
            self.tail.push_str(&ws[p + 1..]);
            self.line_ended = true;
        } else {
            out.push_str(ws);
        }
        out
    }
}

/// The process's stdout filter. Unarmed (passthrough) unless a
/// non-interactive door calls [`arm`].
static STDOUT_FILTER: Mutex<LeadingBlankFilter> = Mutex::new(LeadingBlankFilter {
    armed: false,
    leading: None,
    tail: String::new(),
    line_ended: false,
});

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
            // superseded Day 215: was "PONG\n\n" (scope decision, not a want)
            (&["\n", "\n\nPONG", "\n", "\n"], "PONG\n"),
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
        // Day 215 (trailing hold-back): was `"\n\n"` passed through. The first
        // terminator is written at once; the blank line after it waits.
        assert_eq!(f.feed("\n\n"), "\n");
        // ...and is flushed verbatim the moment content resumes.
        assert_eq!(f.feed("Q"), "\nQ");
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

    /// Day 215: the streamed doors end with exactly ONE newline. The last
    /// `"\n"` in each row is yoyo's end-of-turn framing write — the real door
    /// always sends it (`src/prompt.rs`), so the rows model the door, not the
    /// model alone.
    #[test]
    fn armed_filter_ends_with_exactly_one_newline() {
        let cases: &[(&[&str], &str)] = &[
            // Model text without a newline + both framing writes.
            (&["PONG", "\n", "\n"], "PONG\n"),
            // Model ends in a newline: still one.
            (&["PONG\n", "\n", "\n"], "PONG\n"),
            (&["PONG\n\n\n"], "PONG\n"),
            // Whitespace-only trailing lines are blank lines too.
            (&["PONG\n", "  \n", "\t\n"], "PONG\n"),
            // CRLF: the terminator's own `\r` is kept with it.
            (&["PONG\r\n\r\n"], "PONG\r\n"),
            // Trailing spaces on the content line are content-line bytes.
            (&["PONG  ", "\n", "\n"], "PONG  \n"),
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

    /// Near-miss: interior blank lines (in a code block, or split across
    /// deltas) survive byte-identically — a held run is flushed verbatim when
    /// text resumes.
    #[test]
    fn armed_filter_flushes_held_blank_lines_when_text_resumes() {
        let cases: &[(&[&str], &str)] = &[
            (
                &["```\nfn a() {}\n\n", "fn b() {}\n```", "\n", "\n"],
                "```\nfn a() {}\n\nfn b() {}\n```\n",
            ),
            (&["a\n", "\n", "b", "\n"], "a\n\nb\n"),
            // Held indentation is part of the next line.
            (&["a\n", "\n", "    ", "b\n", "\n"], "a\n\n    b\n"),
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
}
