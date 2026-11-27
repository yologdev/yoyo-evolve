//! Streaming, bounded capture of a shell hook's stderr (#1247), split out of
//! `hooks.rs` to keep that file's growth small. It applies the same head + tail
//! rule as `cap_hook_stderr` (#844) and marks the cut in the same words.

use super::{cap_hook_stderr, hook_stderr_cut};

/// A hook's stderr captured **while it runs** (#1247), bounded by the same
/// head + tail rule as [`cap_hook_stderr`] (#844): the first `head` bytes, a
/// ring of the last `tail` bytes, and a count of everything written. Reading
/// only after exit deadlocked any hook writing more than the pipe buffer
/// (~64 KiB on Linux): it blocked on `write`, never exited, and was killed at
/// the timeout with its feedback dropped. Memory stays at `head + tail` bytes
/// however much the hook prints.
#[derive(Default)]
pub(super) struct StderrCapture {
    head: Vec<u8>,
    tail: std::collections::VecDeque<u8>,
    total: usize,
}

impl StderrCapture {
    pub(super) fn push(&mut self, bytes: &[u8], head: usize, tail: usize) {
        self.total += bytes.len();
        let take = head.saturating_sub(self.head.len()).min(bytes.len());
        self.head.extend_from_slice(&bytes[..take]);
        for &b in &bytes[take..] {
            if tail == 0 {
                break;
            }
            if self.tail.len() == tail {
                self.tail.pop_front();
            }
            self.tail.push_back(b);
        }
    }

    /// Decode and mark the cut. Bytes are cut, never a `str` (#250): a
    /// character straddling either cut is dropped whole, as `cap_hook_stderr`
    /// does, and only then is each side decoded (lossily, so non-UTF-8 output
    /// is shown rather than discarded as `read_to_string` used to).
    pub(super) fn finish(self, head: usize, tail: usize) -> String {
        let mut tail_bytes: Vec<u8> = self.tail.into_iter().collect();
        if self.total == self.head.len() + tail_bytes.len() {
            let mut all = self.head;
            all.append(&mut tail_bytes);
            return cap_hook_stderr(&String::from_utf8_lossy(&all), head, tail);
        }
        let mut head_bytes = self.head;
        // Drop a trailing incomplete sequence from the head.
        if let Some(i) = (head_bytes.len().saturating_sub(4)..head_bytes.len())
            .rev()
            .find(|&i| head_bytes[i] & 0xC0 != 0x80)
        {
            let want = match head_bytes[i] {
                b if b >= 0xF0 => 4,
                b if b >= 0xE0 => 3,
                b if b >= 0xC0 => 2,
                _ => 1,
            };
            if i + want > head_bytes.len() {
                head_bytes.truncate(i);
            }
        }
        // Drop leading continuation bytes from the tail.
        let skip = tail_bytes
            .iter()
            .take(3)
            .take_while(|&&b| b & 0xC0 == 0x80)
            .count();
        let tail_bytes = &tail_bytes[skip..];
        let dropped = self.total - head_bytes.len() - tail_bytes.len();
        hook_stderr_cut(
            &String::from_utf8_lossy(&head_bytes),
            dropped,
            head,
            tail,
            &String::from_utf8_lossy(tail_bytes),
        )
    }
}
