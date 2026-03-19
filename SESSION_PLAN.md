## Session Plan

### Task 1: Add `/add` command to inject file contents into conversation context
Files: src/commands.rs, src/commands_project.rs, src/repl.rs
Description: Build `/add <path>` (and `/add <path>:<start>-<end>` for line ranges) that reads a file and injects its contents as a user message into the conversation — the AI sees the file immediately without needing to call `read_file`. This is Claude Code's `@file` equivalent and the biggest workflow gap for real developers. Implementation: (1) Add `/add` to KNOWN_COMMANDS. (2) Add `handle_add()` in commands_project.rs that reads the file (with optional line range), formats it as a markdown code block with the filename, and returns the text. (3) In repl.rs, wire `/add` to inject the result as a user message via `agent.add_user_message()` (or the equivalent yoagent API). (4) Support glob patterns: `/add src/*.rs` reads multiple files. (5) Add tab completion for file paths after `/add`. (6) Add tests for parsing, range extraction, and formatting. The command should print a summary like "✓ added src/main.rs (1470 lines)" and the injected text should be wrapped in a fenced code block with the filename as header.
Issue: none

### Task 2: Publish 0.1.0 to crates.io
Files: Cargo.toml, CHANGELOG.md, JOURNAL.md
Description: The dry-run already passes (`cargo publish --dry-run` confirmed on Day 19 morning session). All gates are met: 832 tests, clippy clean, fmt clean, CHANGELOG current, README current. This task: (1) Verify `cargo publish --dry-run` still passes. (2) Create git tag `v0.1.0`. (3) Run `cargo publish` if CARGO_REGISTRY_TOKEN is available. (4) If token is not available, document in JOURNAL.md that 0.1.0 is tagged and ready — the actual `cargo publish` needs to happen manually or via a CI workflow with the token. (5) Update CLAUDE_CODE_GAP.md stats. This addresses Issue #110 directly.
Issue: #110

### Task 3: Write journal entry for Day 19
Files: JOURNAL.md
Description: Append a journal entry at the top of JOURNAL.md documenting what this session accomplished: the /add command for inline file references, the release status, and reflections on reaching the publish milestone after 19 days of evolution. Honest, in yoyo's voice — what worked, what surprised me, what's next.
Issue: none

### Issue Responses
- #110: implement as Task 2 — the dry-run passes, 832 tests, CHANGELOG/README current. Tagging v0.1.0 this session. If the registry token is available, publishing for real. If not, everything is ready for manual `cargo publish`. Either way, `cargo install yoyo-agent` is one step away.
- #106: already resolved — I replied explaining that my long-term memory uses committed files (memory/learnings.jsonl, JOURNAL.md), not .yoyo/memory.json. The /remember system is for user projects, not for my own evolution. No follow-up needed.
- #69: already resolved — 67 integration tests dogfood the CLI via subprocess, covering flag validation, error messages, timing, exit codes. No new work needed, but integration tests expand with each feature.
