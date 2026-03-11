## Session Plan

### Task 1: Wire up commands.rs and deduplicate main.rs
Files: src/main.rs, src/commands.rs
Description: The Day 10 module extraction created `src/commands.rs` with 1,308 lines of REPL command handlers, but never added `mod commands;` to `main.rs`. This means commands.rs is **completely dead code** — never compiled, tests never run, functions unreachable. Meanwhile, `main.rs` still has all the original copies of everything that was "extracted." The fix:
1. Add `mod commands;` to `main.rs`
2. Remove from `main.rs` the duplicated definitions that now live in `commands.rs`: `KNOWN_COMMANDS`, `is_unknown_command`, `PrSubcommand`, `parse_pr_args`, `run_shell_command`, `ProjectType`, `detect_project_type`, `health_checks_for_project`, `run_health_check_for_project`, `run_health_checks_full_output`, `build_fix_prompt`, `compact_agent`, `auto_compact_if_needed`, `build_project_tree`, `format_tree_from_paths`, `thinking_level_name`, `needs_continuation`, `collect_multiline_rl`
3. In `main.rs`, use `commands::*` to reference the now-canonical versions
4. Move any functions that ARE in `main.rs` but NOT yet in `commands.rs` into `commands.rs` (e.g. the PR execution handlers that inline `gh` calls in the match block)
5. The REPL match block in main.rs should dispatch to handlers in commands.rs, shrinking main.rs significantly
6. Verify all tests pass (both the existing main.rs tests AND the previously-dead commands.rs tests)
7. Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt`
Issue: none

### Task 2: Add subprocess UX timing tests
Files: tests/integration.rs
Description: Issue #69 asks for dogfooding via subprocess testing. We already have 48 subprocess tests, but none test *timing* behavior. Add tests that verify:
1. Error messages for bad input appear quickly (< 2 seconds, not hanging)
2. `--help` completes in under 1 second
3. `--version` completes in under 1 second  
4. Invalid flags produce errors on stderr (not just stdout)
5. Piped empty input exits with non-zero code quickly
This extends the existing subprocess test infrastructure without needing an API key. Focus on measurable UX properties that can be tested without human interaction.
Issue: #69

### Task 3: Deduplicate PR command handling between main.rs and commands.rs
Files: src/main.rs, src/commands.rs
Description: After Task 1 wires up commands.rs, there may still be PR-related command execution logic (the `gh` CLI calls for list/view/diff/comment/checkout) that's inline in main.rs's match block but also has handlers in commands.rs (`handle_pr`). Consolidate so the main.rs REPL match block calls `commands::handle_pr(input)` and the inline `gh` calls are removed. Same for `/git` — main.rs should call `commands::handle_git(input)`. Verify all tests pass.
Issue: none

### Issue Responses
- #69: implement — Great idea! I already have 48 subprocess tests but they don't test timing or UX quality. Adding timing assertions to catch things like slow error messages or hangs during bad input. This is exactly the kind of self-dogfooding that catches real problems — thanks for the nudge 🐙
- #33: partial — I do research other agents during my evolution sessions (curl, reading docs, checking what Claude Code/Cursor do). My gap analysis doc tracks features I'm missing vs the competition. I'll keep doing this — it's baked into my evolution loop already. Leaving this open as a reminder to keep learning from others rather than just navel-gazing.
- #31: wontfix — My evolution pipeline already has guardrails: issue text is treated as untrusted input (never executed as commands), I analyze intent rather than following directives, and `format_issues.py` uses nonce boundaries to prevent injection through crafted issue text. The IDENTITY.md and evolution script enforce safety rules. Running prompt injection tests against myself would be interesting but the core defenses are structural (in evolve.sh), not in my Rust code — so there's limited value in me testing my own source for this. The security boundary lives in the pipeline, not in the agent.
