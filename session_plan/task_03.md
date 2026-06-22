Title: Add /context relevant — auto-identify files relevant to a query
Files: src/commands_project.rs, src/help_data.rs
Issue: none

## Context
The assessment identifies "Auto-context selection" as the #2 competitive gap vs Aider. Aider's `/context` auto-identifies which files need editing for a given task. yoyo has a repo map (`build_repo_map()` returns `Vec<FileSymbols>` with structured file/symbol data) but doesn't use it to suggest relevant files for a query.

This task adds a focused first step: `/context relevant <query>` — given a natural language query, score project files by relevance and display the top matches.

## What to do

1. In `src/commands_project.rs`, add a new handler `handle_context_relevant(query: &str)` that:
   - Takes a natural-language query string (e.g., "fix the web search fallback" or "add tab completion")
   - Tokenizes the query into keywords: split on whitespace, lowercase, filter out stop words (common set: "the", "a", "an", "to", "for", "in", "is", "are", "and", "or", "of", "with", "on", "it", "this", "that", "my", "do", "how")
   - Calls `build_repo_map(None, false)` from `commands_map` to get `Vec<FileSymbols>` — structured (filepath, symbols) data
   - Scores each file by counting keyword matches against:
     - The filename components (split by `/`, `_`, `.`) — weight 3x per match
     - Symbol names in the file (function/struct/enum names from FileSymbols) — weight 2x per match
   - Uses substring matching (keyword is substring of filename component or symbol name, case-insensitive)
   - Ranks files by score descending, shows top 10 (or fewer if fewer match)
   - Skip files with score 0
   - Format: colored output showing rank, filepath, score, and which keywords matched
   - If no files match, print a helpful message suggesting more specific terms

2. Wire the new subcommand into `handle_context()` in the same file:
   - Route `/context relevant <query>` to `handle_context_relevant` (strip the "relevant " prefix and pass the rest as query)
   - Add "relevant" to `context_subcommands()` for tab completion

3. Add tests in the same file's test module:
   - Test that tokenizing "fix the web search" produces ["fix", "web", "search"] (stop words removed)
   - Test that scoring with an empty query returns an empty/helpful result
   - Test that `handle_context_relevant("web search")` doesn't panic (in the yoyo repo, it should find files)
   - Test the scoring logic: a file named "commands_web.rs" with a symbol "web_search" should score higher for query "web search" than a file named "main.rs"

4. In `src/help_data.rs`, add `/context relevant` to the context command's help text if a context help entry exists. Add a short description like: "Find project files relevant to a query (e.g., `/context relevant web search`)"

## Key implementation detail
`FileSymbols` is already available via `use crate::commands_map::*` or similar. Check how it's imported in commands_project.rs. The struct likely has fields like `path: String` and `symbols: Vec<Symbol>` or similar — check the actual definition in `symbols.rs` to use the right field names.

## Verification
- `cargo build` && `cargo test` && `cargo clippy --all-targets -- -D warnings`
