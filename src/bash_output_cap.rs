//! Head+tail output cap for `StreamingBashTool` (#1159 item 1).
//!
//! A long command's verdict comes LAST (`test result:`, the compiler's
//! `error:` summary, a script's final exit message). A head-only cap keeps the
//! first `cap` bytes and drops all of that. This keeps the first half and the
//! most recent half, and says exactly how many bytes were dropped between.
//!
//! Mirrors yoagent's own `bash` (`Capture` in `src/tools/bash.rs`:
//! `head_cap = cap / 2`, `tail_cap = cap - head_cap`). That struct is
//! private, so this is a hand copy, but the copy is pinned to yoagent's
//! public `BashTool` by `head_tail_and_marker_match_yoagent_bash_tool`, which
//! runs both on the same output: a yoagent change to the cut points or the
//! marker text fails it. Unlike yoagent (bytes, lossy
//! decode), this works on `&str` lines and never cuts inside a character;
//! bytes dropped by a boundary adjustment are counted in the omitted number,
//! so `kept + omitted == total` holds exactly.

/// Bounded accumulator: first `cap/2` bytes, plus the newest bytes up to the
/// cap, plus the exact count of bytes dropped between them.
pub(crate) struct HeadTailCap {
    cap: usize,
    head: String,
    /// Once something has been pushed past the head, the head never grows again.
    head_closed: bool,
    /// Holds up to twice its limit between trims, so trimming is amortised
    /// O(1) per byte rather than a memmove of the whole tail on every line.
    tail: String,
    /// Bytes trimmed off the front of `tail` so far.
    omitted: usize,
}

impl HeadTailCap {
    pub(crate) fn new(cap: usize) -> Self {
        Self {
            cap,
            head: String::new(),
            head_closed: false,
            tail: String::new(),
            omitted: 0,
        }
    }

    /// Bytes the tail may keep: whatever the head left of the cap, so input
    /// under the cap is never trimmed even when the head cut short of `cap/2`
    /// at a character boundary.
    fn tail_limit(&self) -> usize {
        self.cap - self.head.len()
    }

    /// Smallest char boundary of `s` at or after `at`.
    fn ceil_boundary(s: &str, mut at: usize) -> usize {
        while at < s.len() && !s.is_char_boundary(at) {
            at += 1;
        }
        at.min(s.len())
    }

    pub(crate) fn push(&mut self, s: &str) {
        let mut rest = s;
        if !self.head_closed {
            let room = self.cap / 2 - self.head.len();
            if rest.len() <= room {
                self.head.push_str(rest);
                return;
            }
            let mut cut = room;
            while cut > 0 && !rest.is_char_boundary(cut) {
                cut -= 1;
            }
            self.head.push_str(&rest[..cut]);
            rest = &rest[cut..];
            self.head_closed = true;
        }
        self.tail.push_str(rest);
        let limit = self.tail_limit();
        if self.tail.len() > limit.saturating_mul(2).max(limit + 4096) {
            let start = Self::ceil_boundary(&self.tail, self.tail.len() - limit);
            self.tail.drain(..start);
            self.omitted += start;
        }
    }

    /// Append one output line, newline-joined like the old accumulator.
    pub(crate) fn push_line(&mut self, line: &str) {
        if !self.head.is_empty() || !self.tail.is_empty() || self.omitted > 0 {
            self.push("\n");
        }
        self.push(line);
    }

    /// The text the model gets: byte-identical to the input when nothing was
    /// dropped, otherwise yoagent's `head \n... (output truncated: N bytes
    /// omitted) ...\n tail` shape.
    pub(crate) fn render(&self) -> String {
        let limit = self.tail_limit();
        let start = if self.tail.len() > limit {
            Self::ceil_boundary(&self.tail, self.tail.len() - limit)
        } else {
            0
        };
        let omitted = self.omitted + start;
        if omitted == 0 {
            return format!("{}{}", self.head, self.tail);
        }
        format!(
            "{}\n... (output truncated: {} bytes omitted) ...\n{}",
            self.head,
            omitted,
            &self.tail[start..]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capped(cap: usize, input: &str) -> String {
        let mut c = HeadTailCap::new(cap);
        c.push(input);
        c.render()
    }

    /// Parse `(kept_head, omitted, kept_tail)` back out of a rendered result.
    fn split(out: &str) -> (&str, usize, &str) {
        let open = "\n... (output truncated: ";
        let i = out.find(open).expect("marker present");
        let after = &out[i + open.len()..];
        let n_end = after.find(' ').unwrap();
        let n: usize = after[..n_end].parse().unwrap();
        let close = " bytes omitted) ...\n";
        let j = after.find(close).unwrap();
        (&out[..i], n, &after[j + close.len()..])
    }

    #[test]
    fn under_cap_is_byte_identical() {
        for input in ["", "x", "line one\nline two", &"a".repeat(100)] {
            assert_eq!(capped(100, input), input);
        }
    }

    #[test]
    fn under_cap_is_byte_identical_even_when_the_head_cuts_short_at_a_boundary() {
        // 49 ASCII bytes then a 3-byte ✓ straddling the head's 50-byte edge:
        // the head stops at 49, the tail must still take the rest unharmed.
        let input = format!("{}✓{}", "a".repeat(49), "b".repeat(48));
        assert_eq!(input.len(), 100);
        assert_eq!(capped(100, &input), input);
    }

    #[test]
    fn long_output_keeps_the_first_line_and_the_verdict_line() {
        let mut c = HeadTailCap::new(200);
        c.push_line("running 3 tests");
        for i in 0..10_000 {
            c.push_line(&format!("noise line {i}"));
        }
        c.push_line("test result: ok. 3 passed");
        let out = c.render();
        assert!(out.starts_with("running 3 tests\n"), "{out}");
        assert!(out.ends_with("test result: ok. 3 passed"), "{out}");
        assert!(!out.contains("noise line 5000\n"), "middle dropped: {out}");
    }

    #[test]
    fn marker_names_the_exact_omitted_count() {
        let input: String = (0..500).map(|i| format!("row {i}\n")).collect();
        let out = capped(100, &input);
        let (head, omitted, tail) = split(&out);
        assert_eq!(head.len(), 50);
        assert_eq!(tail.len(), 50);
        assert_eq!(head.len() + omitted + tail.len(), input.len());
        assert!(input.starts_with(head) && input.ends_with(tail));
    }

    /// Runs yoagent's own public `BashTool` and this cap over the same bytes
    /// and requires the same text, so the copy is pinned to its authority
    /// rather than to a string typed here. `BashTool`'s content is
    /// `"Exit code: N\n"` + the stdout capture's text when stderr is empty
    /// (`render_output` in yoagent's src/tools/bash.rs); the header is
    /// stripped and only the capped body is compared: kept head, marker line
    /// with its omitted count, kept tail. ASCII only: yoagent cuts bytes and
    /// this cuts at char boundaries, which agree exactly on ASCII.
    #[tokio::test]
    async fn head_tail_and_marker_match_yoagent_bash_tool() {
        use yoagent::types::{AgentTool, Content, ToolContext};
        let command = "seq 1 2000";
        let full = std::process::Command::new("bash")
            .args(["-c", command])
            .output()
            .expect("run seq");
        let full = String::from_utf8(full.stdout).unwrap();
        assert!(full.len() > 1000, "input must overflow the cap");
        for cap in [100, 101, 257] {
            let tool = yoagent::tools::bash::BashTool {
                max_output_bytes: cap,
                ..Default::default()
            };
            let result = tool
                .execute(
                    serde_json::json!({ "command": command }),
                    ToolContext::new("t", "bash"),
                )
                .await
                .expect("yoagent bash ran");
            let Content::Text { text } = &result.content[0] else {
                panic!("expected text content");
            };
            let body = text
                .strip_prefix("Exit code: 0\n")
                .unwrap_or_else(|| panic!("unexpected wrapper: {text:?}"));
            let ours = capped(cap, &full);
            assert!(ours.contains("\n... (output truncated: "), "{ours}");
            assert_eq!(ours, body, "cap {cap}");
        }
    }

    #[test]
    fn multibyte_input_never_panics_and_counts_stay_exact() {
        for cap in [0, 1, 2, 3, 5, 7, 10, 64, 101] {
            for unit in ["✓", "🐙", "é", "a✓🐙"] {
                let input = unit.repeat(300);
                let out = capped(cap, &input);
                if out == input {
                    assert!(input.len() <= cap);
                    continue;
                }
                let (head, omitted, tail) = split(&out);
                assert_eq!(
                    head.len() + omitted + tail.len(),
                    input.len(),
                    "cap {cap} unit {unit}"
                );
                assert!(head.len() + tail.len() <= cap, "cap {cap} unit {unit}");
                assert!(input.starts_with(head) && input.ends_with(tail));
            }
        }
    }

    #[test]
    fn many_small_pushes_match_one_big_push() {
        let pieces: Vec<String> = (0..3000).map(|i| format!("{i}✓🐙 line\n")).collect();
        let whole: String = pieces.concat();
        for cap in [0, 9, 100, 1000, 10_000, 1_000_000] {
            let mut c = HeadTailCap::new(cap);
            for p in &pieces {
                c.push(p);
            }
            assert_eq!(c.render(), capped(cap, &whole), "cap {cap}");
        }
    }
}
