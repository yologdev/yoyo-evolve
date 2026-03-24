Title: Suppress partial tool output in piped/CI mode
Files: src/prompt.rs
Issue: #172

## Context

Issue #172: In CI/piped mode, `ToolExecutionUpdate` events print partial tool output lines (`┆` prefix) via `format_partial_tail()`. In a terminal, ANSI cursor-up sequences overwrite previous lines — but in piped mode there's no cursor control, so every partial update becomes a permanent log line. This inflates CI logs from ~800 to ~9500 lines.

## Implementation

In `src/prompt.rs`, inside `handle_prompt_events()`, find the `AgentEvent::ToolExecutionUpdate` handler (around line 905). Gate the entire partial output rendering block behind a TTY check:

```rust
AgentEvent::ToolExecutionUpdate { tool_call_id, partial_result, .. } => {
    // Update line count on the progress timer if active
    let line_count = count_result_lines(&partial_result);
    if let Some(timer) = tool_progress_timers.get(&tool_call_id) {
        timer.set_line_count(line_count);
    }

    // Only show partial output in interactive (terminal) mode.
    // In piped/CI mode, cursor-up sequences don't work and every
    // partial update becomes a permanent log line, inflating output.
    if io::stdout().is_terminal() {
        let text = extract_result_text(&partial_result);
        if !text.is_empty() {
            let tail = format_partial_tail(&text, 3);
            if !tail.is_empty() {
                println!();
                println!("{tail}");
                io::stdout().flush().ok();
            }
        }
    }
}
```

You'll need to add `use std::io::IsTerminal;` at the top of the file if it's not already imported (check — `std::io` is already imported but `IsTerminal` may not be).

## Tests

1. `test_format_partial_tail_still_works` — verify `format_partial_tail` function itself still works (existing tests cover this, just confirm they pass).
2. No new unit test needed for the TTY check itself (it's a runtime environment check), but add an integration test:
   - `test_piped_mode_no_partial_output_symbols` — run yoyo in piped mode with a prompt that triggers tool use and verify no `┆` characters appear in output. This may need to be `#[ignore]` if it requires an API key.

Actually, the simplest verification is: cargo build && cargo test pass. The behavioral change is testable by running in CI — which is exactly where we'll see it.

## Verification

After implementing, run:
```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```
