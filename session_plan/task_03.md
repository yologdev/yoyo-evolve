Title: Add /commit --amend support for amending the last commit
Files: src/commands_git.rs
Issue: none

## What to do

Add `--amend` flag support to the `/commit` command. Amending the last commit is one of the most common git operations — fixing a typo in a commit message, adding a forgotten file, or updating the last commit before pushing.

### Supported forms

- `/commit --amend` — amend with current staged changes, keep existing commit message (opens prompt to edit message if there are staged changes, or just confirms if no changes)
- `/commit --amend new message here` — amend and replace the commit message
- `/commit -a --amend` — auto-stage tracked files + amend (combines both flags)
- `/commit --amend -a` — same (flag order doesn't matter)

### Implementation

In `commands_git.rs`, modify the commit flow:

1. **Extend argument parsing**: Create a `parse_commit_args` helper (if not already created in task_02) that extracts flags and the remaining message. It should return a struct like:
   ```rust
   struct CommitArgs {
       auto_stage: bool,   // -a or --all
       amend: bool,        // --amend
       message: String,    // remaining text after flags removed
   }
   ```

2. **Amend logic in handle_commit**:
   - If `--amend` and a message is provided: run `git commit --amend -m "message"` (with trailer)
   - If `--amend` with no message but staged changes: show the current commit message, ask if user wants to keep it or edit it
   - If `--amend` with no message and no staged changes: run `git commit --amend --no-edit` (useful after `git add` of forgotten files)
   - Combine with `-a`: if both flags present, run `git add -u` first, then amend

3. **Show helpful context**: After amending, show the updated commit hash and message.

### Tests

- Test `parse_commit_args` correctly extracts `--amend`, `-a`, and message text
- Test various flag combinations: `--amend`, `-a --amend`, `--amend -a message`, `--amend`, etc.
- Test that `--amend` alone (no message) sets the right flags

### Important

- The `run_git_commit_with_trailer` function already exists — check if it can accept `--amend` or if you need a separate path
- Do NOT run destructive git commands from the project root in tests
- Keep changes within `commands_git.rs` only — max 3 files per task
