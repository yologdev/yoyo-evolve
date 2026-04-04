Title: Enhance /context to show system prompt sections with token estimates
Files: src/commands_project.rs, src/help.rs, src/repl.rs
Issue: none

## What

Enhance the `/context` command to accept a `system` subcommand (`/context system`) that
displays the full system prompt broken into sections with approximate token counts for each.
This complements `--print-system-prompt` (Task 1) for interactive use — users can inspect
what the model sees without restarting yoyo.

## Why

Currently `/context` only shows which project context files were loaded and their line counts.
Users can't see the actual assembled system prompt — the base instructions, project context,
repo map, skills, custom overrides — or how much of their context window each section uses.
For debugging prompt issues or understanding model behavior mid-session, this is essential.

## Implementation

### In `src/commands_project.rs`:
1. Modify `handle_context()` to accept arguments: `pub fn handle_context(input: &str, system_prompt: &str)`
2. When `input` is empty or no subcommand: show existing behavior (project context files list)
3. When `input.trim()` starts with "system": call new `show_system_prompt_sections(system_prompt)`
4. `show_system_prompt_sections(prompt: &str)`:
   - Split the prompt into sections by looking for markdown headers (`# `, `## `)
   - For each section, show:
     - Section name (from the header)
     - Line count
     - Approximate token count (chars / 4 as rough estimate)
   - Show total at the bottom
   - Print the first 3 lines of each section as preview (dimmed)
5. Add tests for section parsing and display

### In `src/repl.rs`:
1. Update the `/context` dispatch to pass the input arguments and system_prompt reference
   to `handle_context()`. The system_prompt is already available in the REPL loop scope.

### In `src/help.rs`:
1. Update `/context` help to document the `system` subcommand
2. Add to command_arg_completions for tab completion of "system" after "/context"

### Tests to add:
- `test_context_system_sections` — verify section parsing from a sample prompt
- `test_context_system_empty_prompt` — verify graceful handling of empty prompt
- `test_context_default_behavior` — verify no-arg still shows file list

### Token estimation approach:
Use chars/4 as a rough approximation. Don't bring in a tokenizer dependency — approximate
is fine for debugging. Label it as "~N tokens (estimated)".

### Important:
- The system_prompt string needs to be passed through from the REPL loop. Check how
  other commands that need session state receive their parameters.
- If passing system_prompt through is too invasive, an alternative is to store it in
  a thread-local at startup (similar to watch_command pattern in prompt.rs).
