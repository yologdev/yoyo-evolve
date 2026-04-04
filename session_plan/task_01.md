Title: Add --print-system-prompt flag for prompt transparency
Files: src/cli.rs, src/main.rs
Issue: none

## What

Add a `--print-system-prompt` CLI flag that prints the full assembled system prompt (including
project context, repo map, skills, and any custom system prompt overrides) to stdout and exits.
This is a transparency/debugging feature that Claude Code provides — users need to see exactly
what context the model is receiving.

## Why

Users can't debug prompt issues or understand model behavior without seeing the actual system
prompt. Currently `/config` shows a truncated one-liner. `--print-system-prompt` gives users
the complete picture. This is table stakes for a serious coding agent CLI.

## Implementation

### In `src/cli.rs`:
1. Add `print_system_prompt: bool` field to `Config` struct
2. Add `"--print-system-prompt"` to `KNOWN_FLAGS` array
3. Parse it in `parse_args()` — set to true when flag is present
4. Add it to the `print_help()` output under the flags section
5. Write tests: flag parsing, help text inclusion

### In `src/main.rs`:
1. After `parse_args()` returns the config, check `config.print_system_prompt`
2. If true, print the full `config.system_prompt` to stdout and return/exit(0)
3. This must happen AFTER project context and repo map are assembled into the system prompt
   (so the user sees the complete prompt), but BEFORE agent construction
4. Do NOT print any banner, welcome text, or ANSI colors — just raw prompt text

### Tests to add:
- `test_print_system_prompt_flag_parsed` — verify the flag sets the bool
- `test_print_system_prompt_flag_default_false` — verify default is false
- Verify `--print-system-prompt` is in KNOWN_FLAGS

### Edge cases:
- Should work with `--system "custom prompt"` and `--system-file` to show the resolved prompt
- Should work with `--no-project-context` to show what happens without project context
- Output to stdout (not stderr) so it can be piped/redirected
