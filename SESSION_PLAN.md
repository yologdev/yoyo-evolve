## Session Plan

### Task 1: `/grep` command — direct file content search without the LLM
Files: src/commands_project.rs, src/commands.rs, src/help.rs, src/repl.rs
Description: Add `/grep <pattern> [path]` for fast, direct file content search — no API call, no token cost, instant results. This is one of the biggest workflow gaps vs Claude Code, where users can search their codebase without waiting for the agent. Currently users must either ask the agent to search (costs tokens, slow) or switch to a separate terminal.

Implementation:
1. Add `handle_grep(input: &str)` in `commands_project.rs` that:
   - Parses `/grep <pattern> [path]` — pattern is required, path defaults to "."
   - Runs `grep -rn --color=never` (or the equivalent) via `std::process::Command` with the pattern and path
   - Respects `.gitignore` by using `git grep -n` when in a git repo (faster, respects ignores), falling back to `grep -rn --exclude-dir=.git --exclude-dir=target --exclude-dir=node_modules`
   - Limits output to 50 matches with a "(N more matches, narrow your search)" footer
   - Colors output: line numbers in cyan, matches highlighted in yellow/bold, filenames in green
   - Case-insensitive by default, `-s` flag or `--case` for case-sensitive
2. Add `/grep` to KNOWN_COMMANDS in `commands.rs`
3. Add help entry in `help.rs`
4. Wire dispatch in `repl.rs`
5. Tests:
   - `parse_grep_args` extracts pattern and optional path
   - `format_grep_results` colors and truncates correctly
   - `handle_grep` with known pattern finds matches (use "fn main" in src/)
   - `/grep` in KNOWN_COMMANDS
   - `/grep` in help text
   - Empty pattern shows usage
Issue: none

### Task 2: `/git stash` integration — save and restore work in progress
Files: src/commands_git.rs, src/commands.rs, src/help.rs
Description: Extend the `/git` command to support stash operations: `/git stash`, `/git stash pop`, `/git stash list`, `/git stash drop`. Currently GIT_SUBCOMMANDS only includes `status`, `log`, `add`, `diff`, `branch`, `stash` — the `stash` entry exists but needs proper handling with sub-subcommands. Claude Code has full git stash support.

Implementation:
1. In `commands_git.rs`, update `handle_git_subcommand` to handle `stash` with sub-operations:
   - `/git stash` or `/git stash push` — runs `git stash push` with a message showing the current yoyo session context
   - `/git stash pop` — runs `git stash pop` and shows result
   - `/git stash list` — runs `git stash list` and formats output with colored entries
   - `/git stash drop [n]` — drops a specific stash entry
   - `/git stash show [n]` — shows diff of a stash entry with colored output
2. Format stash list entries with dim dates and bold descriptions
3. Tests:
   - Parse stash subcommand extraction
   - Format stash list output
   - Stash show formatting
   - `/git stash` dispatches correctly
Issue: none

### Task 3: Streaming performance — reduce MarkdownRenderer buffering latency
Files: src/format.rs
Description: Follow-up on issue #147. The MarkdownRenderer currently buffers text at line boundaries before rendering. This means tokens within a line don't appear until the newline arrives. The fix: make `render_delta` flush partial content more aggressively — specifically, when we're not inside a special context (code block header, link), output accumulated text immediately rather than waiting for a newline. This is the primary remaining cause of perceived streaming lag.

Implementation:
1. In `MarkdownRenderer::render_delta()`, identify the buffering points:
   - Currently the renderer accumulates text in a line buffer and only processes it when a newline is seen
   - Change behavior: when not in a "needs-buffering" state (e.g., start of a line that might be a header, in a code fence opener), flush the current buffer immediately
   - Keep buffering only when we're at the start of a line and need to see the full prefix (e.g., `#`, `` ``` ``, `- `, `> `)
2. Add a `needs_line_buffering()` method that returns true only when the current partial line could be a markdown control sequence (starts with `#`, `` ` ``, `*`, `-`, `>`, `|`, etc.)
3. When `needs_line_buffering()` is false, render accumulated partial text immediately with inline formatting (bold, italic, code spans)
4. Tests:
   - Mid-line text renders immediately without waiting for newline
   - Header lines still buffer until complete (so `#` prefix is recognized)
   - Code fence openers still buffer until complete
   - Inline formatting (bold, code) works on partial lines
   - Verify no regression in existing MarkdownRenderer tests
Issue: #147

### Issue Responses
- #147: Implementing as Task 3 — investigating the MarkdownRenderer line-buffering as the primary remaining latency source. The Day 21 code-block fix helped but there's more to squeeze out of the rendering pipeline.
- #144: Not tackling additional mermaid diagrams this session — focused on user-facing capability gaps (grep, stash, streaming). The existing architecture diagrams from Day 21 are rendering correctly. Will add more diagram resolutions in a future session.
- #137: Already largely addressed in v0.1.1 (Day 20). Task 3 continues the streaming improvement work. The core issues (spinner race, thinking/text interleaving, missing separator) are fixed. Remaining work is rendering latency optimization.
