Title: Compact token stats — replace verbose dump with single dimmed line (Issue #180)
Files: src/format.rs, src/prompt.rs
Issue: #180

## Context

Issue #180 requests replacing the verbose token stats line:
```
tokens: 1119 in / 47 out  (session: 1119 in / 47 out)  cost: $0.020  total: $0.020  ⏱ 1.0s
```
with a compact format:
```
↳ 1.0s · 1119→47 tokens
```

When `--verbose` is on, show the full stats. By default, show the compact version.

## Implementation

### 1. Modify `print_usage()` in src/format.rs (around line 1342)

The current `print_usage()` function prints one long verbose line. Add a compact mode:

```rust
pub fn print_usage(
    usage: &yoagent::Usage,
    total: &yoagent::Usage,
    model: &str,
    elapsed: std::time::Duration,
) {
    if usage.input == 0 && usage.output == 0 {
        return;
    }

    if is_verbose() {
        // Existing verbose format — keep it exactly as is
        let cache_info = if usage.cache_read > 0 || usage.cache_write > 0 {
            format!(
                "  [cache: {} read, {} write]",
                usage.cache_read, usage.cache_write
            )
        } else {
            String::new()
        };
        let cost_info = estimate_cost(usage, model)
            .map(|c| format!("  cost: {}", format_cost(c)))
            .unwrap_or_default();
        let total_cost_info = estimate_cost(total, model)
            .map(|c| format!("  total: {}", format_cost(c)))
            .unwrap_or_default();
        let elapsed_str = format_duration(elapsed);
        println!(
            "\n{DIM}  tokens: {} in / {} out{cache_info}  (session: {} in / {} out){cost_info}{total_cost_info}  ⏱ {elapsed_str}{RESET}",
            usage.input, usage.output, total.input, total.output
        );
    } else {
        // Compact format: ↳ 1.0s · 1119→47 tokens
        let elapsed_str = format_duration(elapsed);
        let cost_suffix = estimate_cost(usage, model)
            .map(|c| format!(" · {}", format_cost(c)))
            .unwrap_or_default();
        println!(
            "\n{DIM}  ↳ {elapsed_str} · {}→{} tokens{cost_suffix}{RESET}",
            usage.input, usage.output
        );
    }
}
```

### 2. Import `is_verbose` in format.rs

Add `use crate::cli::is_verbose;` to the imports in format.rs if not already present.

### 3. Tests

Add tests in `src/format.rs`:
- `test_print_usage_compact_format` — verify that with verbose off, the compact format is used (capture stdout or test the format logic)
- `test_print_usage_zero_tokens_no_output` — verify no output when input=0 and output=0

Since `print_usage` uses `println!`, testing the exact output is tricky. Consider extracting the format string logic into a separate `fn format_usage_line(...)` that returns a String, then test that function. The `print_usage` function calls `format_usage_line` and prints it.

### 4. Verify

Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`
