Title: Investigate and fix write_file empty content behavior (Issues #218, #219)
Files: src/main.rs, src/prompt.rs
Issue: #218, #219

## Problem

Two related reports:
- Issue #218: write_file called with empty `content` field
- Issue #219: write_file not being called at all despite user requests

These may be model-level behavior issues (the model choosing not to call the tool or losing the 
content parameter in long conversations), but we should add defensive handling in yoyo.

## Investigation steps

1. Check the `write_file` tool implementation — does it validate that `content` is non-empty?
   Search for the write_file tool in the default tools from yoagent:
   ```bash
   grep -r "write_file" ~/.cargo/registry/src/*/yoagent-*/src/ | head -20
   ```

2. Check if there's any content truncation happening in our tool output formatting that could 
   affect the model's ability to see write_file results.

## Fix

### Defensive guard in file operation confirmation
In `confirm_file_operation()` (around line 268 of main.rs), add a warning when write_file is 
called with empty content:
- If `content` is empty string, print a warning: "⚠ write_file called with empty content — 
  this would create/overwrite with an empty file. Allow?" 
- This gives the user a chance to reject the operation.

### Improved tool result feedback  
In `prompt.rs` event handling for `ToolExecutionEnd`, when the tool is `write_file` and the 
result indicates 0 bytes written, display a visible warning so the user (and model via tool result)
knows something may be wrong.

### What NOT to do
- Don't silently block empty write_file — there are legitimate use cases (creating empty files,
  clearing file contents)
- Don't modify yoagent's built-in tool — that's upstream

### What to verify:
- `cargo build && cargo test`
- write_file with empty content shows a clear warning
- write_file with valid content works normally
