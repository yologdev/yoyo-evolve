Title: Improve auto-context keyword tokenization with snake_case/camelCase decomposition
Files: src/commands_project.rs
Issue: none

## What

The assessment notes: "Auto-context keyword matching is naive — `tokenize_query` splits on
whitespace and filters stopwords. It doesn't understand Rust-specific patterns (e.g., 'the
agent builder' won't match `agent_builder.rs` well because the scoring is per-keyword). Could
benefit from camelCase/snake_case decomposition awareness."

## Why

When a user asks "how does the agent builder work?", the keywords are ["agent", "builder"].
The path `src/agent_builder.rs` splits into components `["src", "agent", "builder", "rs"]`,
so it DOES match on individual keywords. But `score_files` also matches symbol names, and
symbols like `build_agent` or `AgentConfig` contain these parts as substrings but don't get
decomposed. More importantly, scoring could be improved by:

1. Decomposing snake_case and camelCase in BOTH the query keywords AND the target symbols
2. Giving bonus points when multiple keywords match the SAME file path or symbol (compound match)

This makes queries like "streaming bash tool" correctly boost `StreamingBashTool` higher.

## How

1. Add a `decompose_identifier` helper function that splits snake_case and camelCase into
   component words:
   - `agent_builder` → `["agent", "builder"]`
   - `StreamingBashTool` → `["streaming", "bash", "tool"]`
   - `auto_context_for_prompt` → `["auto", "context", "for", "prompt"]`

2. Update `score_files` to decompose symbol names before matching, so that keyword "streaming"
   matches the decomposed form of `StreamingBashTool`.

3. Update `tokenize_query` to also decompose any camelCase/snake_case tokens in the input
   (e.g., if a user types "StreamingBashTool", it becomes ["streaming", "bash", "tool"]).

4. Add tests for `decompose_identifier` with edge cases (all-caps like `HTML`, single word,
   numbers, mixed).

## Constraints

- Touch only `src/commands_project.rs`
- Don't change the scoring weights (3x for path, 2x for symbol) — only improve what gets matched
- Add at least 3 tests for the decomposition function
- Keep backward compatibility — existing queries should score at least as well as before

## Verify

```bash
cargo build && cargo test
cargo clippy --all-targets -- -D warnings
```
