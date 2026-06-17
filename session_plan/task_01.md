Title: Add /goal verify — save and run verification commands for goals
Files: src/commands_goal.rs, src/help_data.rs
Issue: none

## What to do

Add a `/goal verify <command>` subcommand that associates a verification command with the current goal. When `/goal check` is run and a verify command exists, it automatically runs the verification command first and includes the output in the AI's evaluation prompt.

### New subcommands

1. **`/goal verify <command>`** — Save a verification command alongside the goal in `.yoyo/goal_verify.md` (separate file, not in goal.md). Print confirmation.
   - Example: `/goal verify cargo test --test auth`
   - Example: `/goal verify curl -s http://localhost:8080/health | grep ok`

2. **`/goal verify`** (no args) — Show the current verify command, or say none is set.

3. **`/goal verify clear`** — Remove the verify command.

### Modified behavior

4. **`/goal check`** — When a verify command exists:
   - Run the verify command via `std::process::Command` (capture stdout+stderr, limit to 2000 chars)
   - Include the verification output in the prompt sent to the AI:
     ```
     My current goal is:
     <goal text>
     
     Verification command: <command>
     Verification output:
     <stdout/stderr>
     Exit code: <code>
     
     Based on the conversation history and verification results, evaluate...
     ```
   - When no verify command exists, behavior is unchanged (current behavior)

5. **`/goal clear`** — Also remove the verify command file when clearing the goal.

6. **`/goal show`** — Also show the verify command if one is set.

### Implementation details

- Store verify command in `.yoyo/goal_verify.md` (simple text file, one line)
- Add functions: `save_verify_command()`, `load_verify_command()`, `clear_verify_command()`, `run_verify_command()`
- `run_verify_command()` should use `std::process::Command::new("sh").args(["-c", &cmd])` to run the command, capture output, and return `(exit_code, output_text)`
- Truncate verify output to 2000 chars using `safe_truncate` from format module
- Update the usage help text in the `else` branch of `handle_goal`

### Tests to add

- `test_save_and_load_verify_command` — save/load roundtrip in temp dir
- `test_clear_verify_removes_file` — verify file gone after clear
- `test_goal_clear_also_clears_verify` — clearing goal removes verify
- `test_run_verify_command_captures_output` — run `echo hello` and check output
- `test_run_verify_command_captures_exit_code` — run `false` and check exit code is non-zero
- `test_handle_goal_verify_no_args_shows_current` — display current verify command
- `test_handle_goal_show_includes_verify` — `/goal show` displays verify command

### Help update

In `src/help_data.rs`, update the "goal" entry to include the new verify subcommand:
```
/goal verify <cmd>  Set a verification command
/goal verify        Show current verify command
/goal verify clear  Remove verify command
```

Also add a note: "When a verify command is set, /goal check runs it first and includes the result in the AI's evaluation."
