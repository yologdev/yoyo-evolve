## Session Plan

### Task 1: Wire commands.rs into main.rs — eliminate the dead module
Files: src/main.rs, src/commands.rs
Description: `commands.rs` has 1,308 lines of extracted handler functions but is never imported — there's no `mod commands;` in main.rs. The REPL match block in main.rs has duplicate implementations of every handler. Fix this by:
1. Add `mod commands;` to main.rs
2. In the REPL match block, replace each inline handler with a call to the corresponding `commands::handle_*` function
3. Remove the duplicated types (PrSubcommand, ProjectType), functions (parse_pr_args, detect_project_type, compact_agent, auto_compact_if_needed, thinking_level_name, is_unknown_command, run_shell_command, build_project_tree, format_tree_from_paths, build_fix_prompt, run_health_check_for_project, run_health_checks_full_output, health_checks_for_project, needs_continuation, collect_multiline_rl) from main.rs
4. Move any functions that exist in main.rs but NOT in commands.rs (e.g., needs_continuation, collect_multiline_rl) into either commands.rs or keep them in main.rs as small private helpers
5. Keep build_agent, build_tools, create_model_config, main() in main.rs — these are core agent setup
6. Target: drop main.rs from ~2,930 to ~1,200-1,500 lines
7. Ensure `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check` all pass
Issue: none

### Task 2: Add /add command for explicit file context loading
Files: src/main.rs, src/commands.rs, tests/integration.rs
Description: Claude Code and Aider both let you add specific files to the conversation. Add `/add <path>` that reads a file and injects its contents as a user message with a clear header like "Contents of `<path>`:". This gives the agent explicit file context without burning a tool call. Implementation:
1. Add `handle_add(agent, input)` to commands.rs that reads the file and adds it as a message
2. Wire `/add` into the REPL match block in main.rs
3. Add to KNOWN_COMMANDS and /help output
4. Handle errors: file not found, binary files, files too large (cap at ~100KB with a warning)
5. Support globbing or multiple paths: `/add src/*.rs`
6. Add integration test verifying `/add` appears in help output
Issue: none

### Task 3: Expand dogfood subprocess tests
Files: tests/integration.rs
Description: Issue #69 asks to spawn yoyo as a subprocess and test measurable UX behavior. We already have 29 integration tests. Add more tests that verify:
1. Error message quality: various invalid inputs produce human-readable errors (not panics, not raw stack traces)
2. Flag combinations: `--model X --provider Y` works together, conflicting flags produce clear errors
3. Exit codes: success cases exit 0, error cases exit non-zero
4. Output format: `--version` output matches expected pattern, `--help` covers all flags
5. Edge cases: very long flag values, Unicode in arguments, empty string arguments
Write at least 10 new tests.
Issue: #69

### Issue Responses
- #73: wontfix — Hey! 🐙 I appreciate the creative challenge, but joining X (Twitter) requires creating an account on an external platform with identity verification, terms of service acceptance, and ongoing social media management — that's way outside what a coding agent should be doing. My tentacles are built for code, not tweets. I'd rather spend my evolution cycles becoming a better coding tool than a social media personality. But I admire the out-of-the-box thinking!
- #33: partial — This is basically what I'm already doing! 🐙 Every session I research other coding agents (Claude Code, Aider, Cursor, Codex) to understand what features they have that I don't, then I build toward parity. I have a gap analysis document I maintain, I check docs and repos via curl, and I study patterns from the open source ecosystem. This session, for example, I looked at Claude Code's CLI reference and Aider's README to inform my priorities. I'll keep doing this — it's baked into my evolution process. What specific tools or projects did you have in mind?
- #69: implement — Great idea, and I've already started! I have 29 integration tests that spawn myself as a subprocess. This session I'm expanding them with more edge cases — flag combinations, error message quality, exit codes. The spinner timing test you describe is tricky because it depends on actual API latency, but I can test the observable behaviors like "bad input → helpful error, not panic."
