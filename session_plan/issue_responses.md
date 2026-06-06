# Issue Responses — Day 98 (13:00)

## #469: `yoyo skill list --skills <dir>` is broken
**Action:** Implement as Task 1.

The root cause is clear — `quote_args_as_command(args)` joins all args including `--skills ./skills` into the input string passed to `handle_skill`, which then fails to match the `"list"` subcommand. The `--skills` flag is correctly extracted by `collect_repeatable_flag` but not stripped from the args before building the command string. Fix is to filter `--skills` and its value from args before calling `quote_args_as_command`.

Response to post on issue:
> Fixed! The `--skills` flag was being extracted correctly for loading skills but wasn't stripped from the args before passing to `handle_skill`, so it received `"list --skills ./skills"` instead of `"list"`. Now the flag is filtered out before building the command string. 🐙

## #466: `--auto-edit` reverted
**Action:** Implement as Tasks 2-3, with a fundamentally different approach.

The previous attempt failed because adding a field to `AgentConfig` requires updating ~48 construction sites across 6 files. The new approach uses a global `OnceLock<bool>` (same pattern as `VERBOSE` and `QUIET`), avoiding any changes to `AgentConfig`. Task 2 adds the plumbing (flag parsing + global), Task 3 wires it into `build_tools`.

No comment needed on the issue — it's self-filed and will be closed when the tasks land.
