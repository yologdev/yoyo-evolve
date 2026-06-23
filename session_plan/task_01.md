Title: Auto-context file injection for prompts (biggest UX gap vs Claude Code)
Files: src/commands_project.rs, src/repl.rs
Issue: none

## What

The single largest UX gap vs Claude Code is that yoyo doesn't automatically identify and inject
relevant files when the user types a natural-language prompt. The infrastructure already exists:
`score_files()` in `commands_project.rs` scores files against keywords from a query, and
`expand_file_mentions()` in `commands_file.rs` shows how file content gets injected. The missing
piece is *wiring them together* in the REPL prompt flow.

## Implementation

1. **Add a public function `auto_context_for_prompt()` in `commands_project.rs`** that:
   - Takes a user prompt string
   - Tokenizes it into keywords via `tokenize_query()`
   - Calls `build_repo_map(None, false)` and `score_files()` to find relevant files
   - Returns a `Vec<(String, String)>` of `(path, content)` for the top 3 files with score ≥ 5
     (threshold ensures only strongly relevant files are included)
   - Reads each file (up to 200 lines to avoid context bloat), returns the content
   - Skip files already in the conversation (check if the path appears in recent messages)
   - Skip binary files, very large files (> 500 lines — just return first 200 with a note)

2. **Wire it into the REPL prompt flow in `repl.rs`** (around line 854, after `expand_file_mentions`):
   - Call `auto_context_for_prompt(&effective_input)`
   - If it returns any files, prepend them to the effective_input as:
     ```
     [Auto-context: yoyo identified these files as relevant to your prompt]
     
     --- src/foo.rs ---
     <content>
     --- src/bar.rs ---
     <content>
     
     [Your prompt]:
     <original prompt>
     ```
   - Print a brief note to stderr: `"  📎 Auto-attached: src/foo.rs, src/bar.rs (use --no-auto-context to disable)"`
   - Gate this behind a check: skip if the prompt starts with `/` (commands don't need auto-context),
     skip if the prompt contains `@` mentions (user is already specifying files manually),
     skip if the prompt is very short (< 20 chars — likely a follow-up, not a new task)

3. **Add tests** in `commands_project.rs`:
   - Test `auto_context_for_prompt` with a query like "web search" returns web-related files
   - Test that empty/short queries return empty vec
   - Test that slash commands return empty vec
   - Test threshold filtering (low-score files excluded)

## Why

This is the #1 capability gap identified in the assessment. Claude Code does this automatically.
Users shouldn't have to manually `/add` files — the agent should figure out what's relevant.
The conservative thresholds (score ≥ 5, max 3 files, max 200 lines each) prevent context bloat
while providing meaningful automatic context for most prompts.

## Docs

Update CLAUDE.md architecture section for `commands_project.rs` to mention `auto_context_for_prompt`.
