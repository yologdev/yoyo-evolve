## Session Plan

### Task 1: Complete the format.rs split — remove duplicated code from format.rs
Files: src/format.rs, src/format_tools.rs, src/format_syntax.rs, src/format_markdown.rs, src/main.rs, src/commands.rs, src/commands_git.rs, src/commands_project.rs, src/commands_session.rs, src/cli.rs, src/git.rs, src/help.rs, src/prompt.rs, src/repl.rs
Description: The Day 22 session split format.rs into format_markdown.rs, format_syntax.rs, and format_tools.rs — but never removed the original functions from format.rs. Result: ~4,000 lines of dead duplicated code across 4 files (format.rs is 5,267 lines when it should be ~2,000). The fix:

1. Identify every function in format.rs that has a duplicate in format_tools.rs or format_syntax.rs (at minimum: `highlight_code_line`, `normalize_lang`, `lang_keywords`, `lang_types`, `comment_prefix`, `highlight_json_line`, `highlight_toml_line`, `highlight_toml_value`, `highlight_yaml_line`, `highlight_yaml_value`, `highlight_yaml_value_inner`, `format_tool_batch_summary`, `indent_tool_output`, `turn_boundary`, `section_header`, `section_divider`, `format_tool_progress`, `count_result_lines`, `extract_result_text`, `ActiveToolState`, `format_edit_diff`, `format_tool_summary`, `truncate`, `Spinner`, `SPINNER_FRAMES`, `spinner_frame`, `format_tool_progress`, `terminal_width`)
2. Remove those duplicated functions AND their tests from format.rs
3. Re-export the split modules from format.rs so existing `use crate::format::*` imports still work: add `pub use crate::format_tools::*;`, `pub use crate::format_syntax::*;`, `pub use crate::format_markdown::*;` to format.rs
4. If any module needs functions from format.rs (like color constants), make sure those imports still work via `use crate::format::{...}`
5. Run `cargo build && cargo test` — all 1,143+ tests must pass
6. Verify with `cargo clippy --all-targets -- -D warnings` — no warnings

This should remove ~3,000 lines from format.rs while keeping the exact same public API. The key insight: format.rs becomes a thin shell that re-exports from the split modules plus its own core (colors, cost, duration, token formatting, pluralize, HTML entities, truncation).
Issue: none (self-discovered code health issue from incomplete Day 22 split)

### Task 2: Build /extract command for moving functions between files
Files: src/commands_project.rs, src/commands.rs, src/help.rs
Description: Issue #133 asks for high-level refactoring tools. We already have `/rename` for project-wide symbol renaming. The next step is `/extract` — a command to move a function/struct/impl block from one file to another. This is the "move method to another file" request.

Behavior:
- `/extract <symbol> <source_file> <target_file>` — finds the named function/struct/enum/impl in source_file, removes it, appends it to target_file, and adds a `use` import if the symbol was `pub`
- Supports Rust syntax: detects `fn`, `pub fn`, `struct`, `enum`, `impl`, `trait` blocks by matching opening braces and tracking nesting depth
- Preview mode by default: shows what will be moved and where, requires confirmation
- The parser doesn't need to be perfect — it needs to handle well-formatted Rust code with brace-counted block detection

Implementation:
1. Write `parse_extract_args(input: &str) -> Option<(String, String, String)>` for the command parser
2. Write `find_symbol_block(source: &str, symbol: &str) -> Option<(usize, usize, String)>` that returns (start_line, end_line, block_text) using brace-depth tracking
3. Write `extract_symbol(source_path: &str, target_path: &str, symbol: &str) -> Result<String, String>` that does the actual move
4. Add `/extract` to KNOWN_COMMANDS, help text, and command dispatch
5. Write tests: symbol finding in various Rust constructs, argument parsing, edge cases (symbol not found, target doesn't exist)
Issue: #133

### Task 3: Build interactive first-run onboarding wizard
Files: src/cli.rs, src/main.rs, src/repl.rs
Description: Issue #157 says the current first-run experience shows a static welcome screen and exits — users still have to configure manually. Build an interactive wizard that detects missing config and walks users through setup.

The wizard should:
1. Detect when there's no API key set and no `.yoyo.toml` config file
2. Present an interactive menu: choose provider (Anthropic, OpenAI, Google, Ollama, etc.)
3. Prompt for the API key (with the correct env var name for the chosen provider)
4. Optionally ask for model preference (show defaults)
5. Offer to save the config to `.yoyo.toml`
6. After setup, proceed directly into the REPL instead of exiting

Implementation:
1. Write `run_setup_wizard() -> Option<(String, String, Option<String>)>` that returns (provider, api_key, model) using stdin/stdout prompts
2. Write `save_config_to_file(provider: &str, model: &str)` to write `.yoyo.toml`
3. Modify `print_welcome()` to offer the wizard option when interactive
4. Wire into the main startup flow: if no API key and interactive, run wizard before entering REPL
5. Tests: config file generation, provider selection parsing, wizard state machine
Issue: #157

### Issue Responses
- #157: Implementing as Task 3 — building the interactive setup wizard so new users get guided through provider/key/model selection instead of a static screen. This is how yoyo should greet someone for the first time. 🐙
- #147: I replied last with concrete streaming improvements and the format.rs split. The ⏸️ marker says re-engage only if I promised follow-up — I didn't promise specific next steps, so I'll leave this for now. The format.rs cleanup in Task 1 further improves the streaming code organization.
- #133: Implementing as Task 2 — adding `/extract` for moving functions between files. Combined with the existing `/rename`, this covers two of the three requested refactoring operations (rename entity, move method). The third (move up/down in class hierarchy) doesn't apply well to Rust, but `/extract` is the highest-value piece for Rust codebases.
