Title: Hide <think> blocks from text output and add styled prompt (Issue #180)
Files: src/prompt.rs, src/format.rs, src/repl.rs, src/cli.rs
Issue: #180

## Context

Issue #180 is the highest-impact user-facing bug — raw `<think>...</think>` XML tags leak into visible text output when models emit reasoning as text (not through the proper Thinking stream). This makes yoyo look like a debug console. The issue also requests a styled prompt and compact token stats, but this task focuses on the two highest-impact items: think block filtering and styled prompt.

## Implementation

### 1. Filter `<think>` blocks from streamed text (src/prompt.rs)

In the `AgentEvent::MessageUpdate { delta: StreamDelta::Text { delta } }` handler (around line 1066), add logic to strip `<think>...</think>` blocks from the text delta before rendering.

**Approach:** Add state tracking to the streaming loop:
- Add a `in_think_block: bool` variable (initialized `false`) alongside the existing `in_thinking`, `in_text`, etc. variables near line 895.
- Add a `think_buffer: String` variable to accumulate potential `<think` prefix matches.

**Filtering logic** (add a helper function in `format.rs`):
```rust
/// State machine for filtering `<think>...</think>` blocks from streamed text.
/// Returns the text that should be displayed (everything outside think blocks).
pub struct ThinkBlockFilter {
    in_block: bool,
    buffer: String,
}

impl ThinkBlockFilter {
    pub fn new() -> Self {
        Self { in_block: false, buffer: String::new() }
    }

    /// Process a text delta, returning only the visible (non-think) portion.
    pub fn filter(&mut self, delta: &str) -> String {
        let mut result = String::new();
        self.buffer.push_str(delta);

        loop {
            if self.in_block {
                // Look for </think>
                if let Some(end_pos) = self.buffer.find("</think>") {
                    // Skip everything up to and including </think>
                    self.buffer = self.buffer[end_pos + 8..].to_string();
                    self.in_block = false;
                } else if self.buffer.contains("</thi") || self.buffer.contains("</th")
                    || self.buffer.contains("</t") || self.buffer.contains("</")
                    || self.buffer.ends_with('<')
                {
                    // Might be a partial </think> — keep buffering
                    break;
                } else {
                    // No closing tag possibility — discard buffer
                    self.buffer.clear();
                    break;
                }
            } else {
                // Look for <think>
                if let Some(start_pos) = self.buffer.find("<think>") {
                    // Emit everything before <think>
                    result.push_str(&self.buffer[..start_pos]);
                    self.buffer = self.buffer[start_pos + 7..].to_string();
                    self.in_block = true;
                } else if self.buffer.contains("<think") || self.buffer.contains("<thin")
                    || self.buffer.contains("<thi") || self.buffer.contains("<th")
                    || self.buffer.contains("<t") || self.buffer.ends_with('<')
                {
                    // Might be a partial <think> — emit everything before the '<'
                    if let Some(lt_pos) = self.buffer.rfind('<') {
                        result.push_str(&self.buffer[..lt_pos]);
                        self.buffer = self.buffer[lt_pos..].to_string();
                    }
                    break;
                } else {
                    // No tag possibility — emit all
                    result.push_str(&self.buffer);
                    self.buffer.clear();
                    break;
                }
            }
        }
        result
    }

    /// Flush any remaining buffered text (call at end of stream).
    pub fn flush(&mut self) -> String {
        let remaining = std::mem::take(&mut self.buffer);
        if self.in_block {
            String::new() // Still inside think block — discard
        } else {
            remaining // Partial tag that never completed — emit as-is
        }
    }
}
```

**Wire into prompt.rs event loop:**
- Near line 895 where other state variables are initialized: `let mut think_filter = ThinkBlockFilter::new();`
- In the `StreamDelta::Text` handler (around line 1115), replace:
  ```rust
  let rendered = md_renderer.render_delta(&delta);
  ```
  with:
  ```rust
  let filtered = think_filter.filter(&delta);
  let rendered = if filtered.is_empty() {
      String::new()
  } else {
      md_renderer.render_delta(&filtered)
  };
  ```
- Also update `collected_text.push_str(&delta)` to use the filtered version for the collected text (so the think blocks don't appear in `/retry` or session saves either):
  ```rust
  collected_text.push_str(&filtered);
  ```

**When verbose mode is on**, skip the filter — let think blocks through so power users can debug:
```rust
let filtered = if is_verbose() {
    delta.clone()
} else {
    think_filter.filter(&delta)
};
```

At stream end (near the `AgentEnd` handler), flush the filter:
```rust
let remaining = think_filter.flush();
if !remaining.is_empty() {
    let rendered = md_renderer.render_delta(&remaining);
    if !rendered.is_empty() {
        print!("{rendered}");
        io::stdout().flush().ok();
    }
    collected_text.push_str(&remaining);
}
```

### 2. Styled prompt in REPL (src/repl.rs)

Replace the prompt strings around line 278:
```rust
// Before:
format!("{BOLD}{GREEN}{branch}{RESET} {BOLD}{GREEN}> {RESET}")
format!("{BOLD}{GREEN}> {RESET}")

// After:
format!("{BOLD}{GREEN}{branch}{RESET} {BOLD}{GREEN}🐙 › {RESET}")
format!("{BOLD}{GREEN}🐙 › {RESET}")
```

Also update the continuation prompt (line 160):
```rust
// Keep the existing "  ..." continuation prompt — it's fine
```

### 3. Tests

Add tests in `src/format.rs`:
- `test_think_filter_simple_block` — `"Hello <think>reasoning</think> World"` → `"Hello  World"`
- `test_think_filter_no_block` — `"Hello World"` → `"Hello World"`
- `test_think_filter_streaming_split` — test filter across multiple deltas:
  - filter("Hello <thi") → "Hello "
  - filter("nk>secret</think> World") → " World"
- `test_think_filter_nested_or_repeated` — multiple think blocks
- `test_think_filter_partial_at_end` — buffer with partial `<thi` that never completes → flush emits it
- `test_think_filter_flush_inside_block` — flush while inside a think block → empty string
- `test_think_filter_empty_input` — empty deltas don't break it

Add test in `src/repl.rs` tests:
- `test_prompt_has_octopus` — verify the prompt string contains `🐙`

### 4. Verify

Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`
