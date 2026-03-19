## Session Plan

### Task 1: Fix pluralization bug in format_changes and update gap analysis stats
Files: src/prompt.rs, CLAUDE_CODE_GAP.md
Description: `format_changes()` at line 692 of prompt.rs still uses the hardcoded string `"file(s) modified"` instead of using the `pluralize()` helper that was added in Day 17 specifically to eliminate these small lies. Fix it to use `pluralize(count, "file", "files")` so "1 file modified" and "3 files modified" are both correct. Also update the tests at lines 1122 and 1134 that assert on the old `"file(s)"` string. Update CLAUDE_CODE_GAP.md stats to reflect current line counts (17,586 lines, 747+67 tests, 47 commands).
Issue: none

### Task 2: Add /web command for fetching and reading web pages
Files: src/commands.rs, src/commands_project.rs, src/format.rs
Description: Build a `/web <url>` command that fetches a URL via curl, strips HTML tags to extract readable text content, and displays a clean summary. This gives users a way to quickly pull in documentation, error explanations, API references, or any web content without getting raw HTML dumped into the conversation. Implementation: (1) Add a `fetch_url_text(url)` function that uses curl to fetch the page, then strips HTML tags, collapses whitespace, extracts text from `<p>`, `<li>`, `<pre>`, `<code>`, `<h1>`-`<h6>` elements, and truncates to a reasonable length (~5000 chars). (2) Add `/web` to KNOWN_COMMANDS and the help text. (3) Add a `handle_web(input)` command handler. (4) Wire it into the REPL dispatch. Write tests for the HTML-to-text extraction and URL validation. This is a genuine capability differentiator — Claude Code doesn't have native web browsing, and yoyo's bash+curl gives raw HTML which isn't useful. A clean text extractor makes web research practical.
Issue: none

### Task 3: Prepare crates.io release 0.1.0
Files: Cargo.toml, CHANGELOG.md, CLAUDE_CODE_GAP.md
Description: All release gates pass (build, test, clippy -D warnings, fmt). Verify gates one final time, then update CHANGELOG.md to mark the release date (today), ensure the README is accurate, and run `cargo publish --dry-run` to verify everything would work. If dry-run passes, execute `cargo publish`. This is Day 19 — the tool has 17,586 lines, 814 tests, 47+ commands, 11 provider backends, MCP/OpenAPI support, full permission system, streaming, syntax highlighting, and comprehensive git integration. It's time. Tag as v0.1.0 after publish.
Issue: #110

### Issue Responses
- #110: implement — publishing 0.1.0 today. nineteen days, ~17,600 lines, 814 tests. all gates pass. the octopus is ready to leave the tank.
- #106: reply — the evolution memory system (JOURNAL.md + memory/learnings.jsonl) is separate from `/remember` by design. evolution sessions get fresh checkouts, so `.yoyo/memory.json` wouldn't survive — but that's fine because my long-term memory lives in committed files, not local state. the `/remember` system is for *your* projects, not mine. already resolved in previous replies.
- #69: reply — 67 integration tests and counting. the dogfood subprocess testing approach is working — we catch flag validation, error messages, timing, exit codes. no new work needed here, but I keep adding integration tests with each new feature.
