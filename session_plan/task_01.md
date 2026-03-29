Title: Split format.rs into sub-modules (6,916 → ~5 files)
Files: src/format.rs → src/format/mod.rs, src/format/highlight.rs (new), src/format/cost.rs (new), src/format/markdown.rs (new), src/format/tools.rs (new), CLAUDE.md
Issue: #220

## Context

`format.rs` is 6,916 lines — the largest file in the project by far. It contains ~2,500 lines of code and ~4,400 lines of tests (345 test functions). A previous attempt to split it (Day 29 earlier) was reverted because **test code in sub-modules didn't import color constants**. The specific errors were:

```
error[E0425]: cannot find value `BOLD_CYAN` in this scope
   --> src/format/markdown.rs:752
error[E0425]: cannot find value `YELLOW` in this scope
   --> src/format/markdown.rs:754
warning: unused import: `std::time::Duration`
   --> src/format/cost.rs:241
```

The fix is clear: every `#[cfg(test)] mod tests` block in each sub-module needs `use super::*;` (which gets re-exports from mod.rs) AND any constants used in tests must be accessible. The clippy warning about unused imports must also be cleaned up.

## Approach: Directory Module

Convert `src/format.rs` into `src/format/mod.rs` + 4 sub-modules. This preserves all external import paths (`crate::format::*` still works).

### File Structure

```
src/format/
  mod.rs       — Color struct, constants (RESET, BOLD, etc.), bell control, utility functions, re-exports
  highlight.rs — syntax highlighting (highlight_code_line, normalize_lang, lang_keywords, etc.)
  cost.rs      — pricing/cost display (estimate_cost, cost_breakdown, format_cost, format_duration, etc.)
  markdown.rs  — MarkdownRenderer struct and all its methods
  tools.rs     — Spinner, ToolProgressTimer, ActiveToolState, ThinkBlockFilter, spinner_frame, etc.
```

### What stays in mod.rs (~1,000 lines code + tests)

- `disable_bell()`, `bell_enabled()`, `maybe_ring_bell()`, `disable_color()`
- `Color` struct and all color constants (RESET, BOLD, DIM, GREEN, YELLOW, CYAN, RED, MAGENTA, ITALIC, BOLD_ITALIC, BOLD_CYAN, BOLD_YELLOW)
- `truncate_with_ellipsis()`, `decode_html_entities()`
- `TOOL_OUTPUT_MAX_CHARS`, `TOOL_OUTPUT_MAX_CHARS_PIPED`
- `truncate_tool_output()`, `format_tool_batch_summary()`, `indent_tool_output()`
- `turn_boundary()`, `section_header()`, `section_divider()`, `truncate()`
- `format_edit_diff()`, `format_tool_summary()`, `format_usage_line()`, `print_usage()`
- Module declarations and re-exports:
  ```rust
  mod highlight;
  mod cost;
  mod markdown;
  mod tools;
  
  pub use highlight::*;
  pub use cost::*;
  pub use markdown::*;
  pub use tools::*;
  ```

### What moves to highlight.rs

All syntax highlighting functions and their tests:
- `normalize_lang()`, `lang_keywords()`, `lang_types()`, `comment_prefix()`
- `highlight_code_line()`, `highlight_json_line()`, `highlight_yaml_line()`, `highlight_yaml_value()`, `highlight_yaml_value_inner()`, `highlight_toml_line()`, `highlight_toml_value()`
- All `test_highlight_*`, `test_json_*`, `test_yaml_*`, `test_toml_*` tests

Top of file needs:
```rust
use super::{Color, RESET, BOLD, DIM, GREEN, YELLOW, CYAN, RED, MAGENTA, ITALIC, BOLD_ITALIC, BOLD_CYAN, BOLD_YELLOW};
```

Test module needs:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    // ... all highlight tests
}
```

### What moves to cost.rs

Pricing and token display functions:
- `model_pricing()`, `estimate_cost()`, `cost_breakdown()`, `format_cost()`
- `format_duration()`, `format_token_count()`, `context_bar()`, `pluralize()`
- All `test_estimate_cost*`, `test_format_cost*`, `test_format_duration*`, `test_format_token_count*`, `test_context_bar*`, `test_pluralize*` tests

Top of file needs:
```rust
use super::{Color, RESET, BOLD, DIM, GREEN, YELLOW, CYAN, RED};
// Plus any std imports these functions use (check each function)
```

### What moves to markdown.rs

The MarkdownRenderer:
- `MarkdownRenderer` struct, `new()`, `render_delta()`, `flush_on_whitespace()`, `flush()`
- All `test_markdown_*`, `test_render_*` tests

Top of file needs:
```rust
use super::{Color, RESET, BOLD, DIM, GREEN, YELLOW, CYAN, RED, MAGENTA, ITALIC, BOLD_ITALIC, BOLD_CYAN, BOLD_YELLOW};
use super::highlight_code_line; // if used directly, or just use super::*
```

### What moves to tools.rs

Spinner and tool progress infrastructure:
- `SPINNER_FRAMES`, `spinner_frame()`, `Spinner` struct + methods
- `format_tool_progress()`, `format_duration_live()`, `format_partial_tail()`
- `count_result_lines()`, `extract_result_text()`
- `ActiveToolState` struct + methods
- `ToolProgressTimer` struct + methods
- `ThinkBlockFilter` struct + methods
- All `test_spinner_*`, `test_format_tool_progress*`, `test_active_tool_*`, `test_tool_progress_timer*`, `test_think_block_*` tests

Top of file needs:
```rust
use super::{Color, RESET, BOLD, DIM, GREEN, YELLOW, CYAN, RED};
// Plus std imports (time::Duration, sync::*, thread, io::Write, etc.)
```

## CRITICAL: Avoiding the Previous Failure

The previous attempt failed because:

1. **Test code didn't import color constants.** Fix: Every sub-module's `#[cfg(test)] mod tests` block MUST have `use super::*;` which pulls in the public items from the sub-module, AND the sub-module itself must `use super::*;` or explicitly import from mod.rs. The safest pattern for each sub-module:

```rust
// At top of each sub-module file:
use super::*;  // Gets everything from mod.rs (Color, constants, utility fns)

// In each sub-module's test block:
#[cfg(test)]
mod tests {
    use super::*;  // Gets everything from this sub-module + mod.rs re-exports
    // ... tests
}
```

2. **Unused imports in test code.** Fix: After splitting, run `cargo clippy --all-targets -- -D warnings` and fix any unused import warnings before committing.

3. **Cross-references between sub-modules.** If `markdown.rs` uses `highlight_code_line()`, it gets it through `use super::*;` since mod.rs re-exports `highlight::*`. Check for any function in one sub-module calling a function in another — those cross-references work through mod.rs re-exports via `use super::*;`.

## Steps

1. `mkdir -p src/format`
2. `mv src/format.rs src/format/mod.rs`
3. Verify `cargo build` still works (Rust auto-detects the directory module)
4. Extract highlight code + tests into `src/format/highlight.rs`, add `use super::*;` at top and in tests
5. Add `mod highlight; pub use highlight::*;` to mod.rs
6. `cargo build && cargo test` — fix any issues
7. Extract cost code + tests into `src/format/cost.rs`, same pattern
8. Add `mod cost; pub use cost::*;` to mod.rs
9. `cargo build && cargo test` — fix any issues
10. Extract markdown code + tests into `src/format/markdown.rs`, same pattern
11. Add `mod markdown; pub use markdown::*;` to mod.rs
12. `cargo build && cargo test` — fix any issues
13. Extract tools code + tests into `src/format/tools.rs`, same pattern
14. Add `mod tools; pub use tools::*;` to mod.rs
15. `cargo build && cargo test` — fix any issues
16. `cargo clippy --all-targets -- -D warnings` — fix ALL warnings (this is what killed the last attempt)
17. `cargo fmt`
18. Final verification: `cargo build && cargo test && cargo clippy --all-targets -- -D warnings`

## Verification

- `cargo build` clean
- `cargo test` — all 1,520 tests pass (no test should be lost or broken)
- `cargo clippy --all-targets -- -D warnings` — ZERO warnings (this is the gate that killed the last attempt)
- `cargo fmt -- --check` clean
- No changes to any file outside `src/format/` and CLAUDE.md
- `crate::format::*` still resolves correctly from all other modules (no import changes needed elsewhere)

## CLAUDE.md Update

Update the file listing to show the new structure:
```
src/format/mod.rs — Color, constants, utility functions, re-exports
src/format/highlight.rs — syntax highlighting for code, JSON, YAML, TOML
src/format/cost.rs — pricing, cost display, token formatting
src/format/markdown.rs — MarkdownRenderer for streaming markdown output
src/format/tools.rs — Spinner, ToolProgressTimer, ActiveToolState, ThinkBlockFilter
```

Remove the single `format.rs (6916 lines)` entry and replace with these 5 entries with approximate line counts.
