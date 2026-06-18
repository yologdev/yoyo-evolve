Title: Consolidate raw git calls in commands_file.rs, commands_skill.rs, and 3 small files
Files: src/commands_file.rs, src/commands_skill.rs, src/git.rs
Issue: none (code quality — assessment finding #1, continued)

## Context

Continuation of the raw git call consolidation arc. This task covers the remaining
production-code offenders. After tasks 2 and 3, all production source files will use
centralized git helpers.

**Production code calls to replace:**

### commands_file.rs (3 production calls)

1. **Line ~526** — `apply_patch()`: `git apply --stat <path>`
   → Replace with `run_git_output(&["apply", "--stat", path])`

2. **Line ~551** — `apply_patch()`: `git apply [--check] [--3way] <path>`
   → Replace with `run_git_output(&["apply", ...args, path])`

3. **Line ~585** — `apply_patch()`: `git apply --3way <path>` (fallback for error info)
   → Replace with `run_git_output(&["apply", "--3way", path])`

Note: All three are in `apply_patch()` and deal with `git apply` which needs raw
Output for both stdout and stderr. Use `run_git_output` for all three.

### commands_skill.rs (1 production call)

Find the single production `Command::new("git")` call and replace with the appropriate
`run_git` or `run_git_output` helper. (The other 5 calls are in test code.)

### Small files (1 production call each — commands_rename.rs, commands_move.rs, commands_map.rs)

- **commands_rename.rs line ~177** — `git ls-files` to find tracked files
  → Replace with `run_git(&["ls-files"])` 

- **commands_move.rs line ~618** — `git ls-files` to find tracked files
  → Replace with `run_git(&["ls-files"])`

- **commands_map.rs line ~173** — `git ls-files` to find tracked files  
  → Replace with `run_git(&["ls-files"])`

Note: touching 5 source files here, but commands_rename.rs, commands_move.rs, and
commands_map.rs each have exactly ONE line change (a mechanical replacement), so the
real complexity is in commands_file.rs and commands_skill.rs. If the implementation
agent needs to stay within the 3-file limit, prioritize commands_file.rs and
commands_skill.rs (the larger changes) and leave the three 1-line files for next session.

## Implementation notes

- Same pattern as task 02: import helpers, replace Command::new calls, keep test code untouched
- For `apply_patch()` in commands_file.rs, the function needs both stdout and stderr from
  git apply, so use `run_git_output` which returns the raw `std::process::Output`
- If any replacement changes the error message format slightly, that's fine — the goal is
  consistency, not identical strings

## Tests

- Existing tests should continue to pass unchanged
- No new tests needed — mechanical refactor

## Verification

After changes: `cargo build && cargo test && cargo clippy --all-targets -- -D warnings`
