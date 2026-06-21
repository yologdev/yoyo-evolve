Title: Add graceful EXA_API_KEY missing warning at startup
Files: src/tools.rs, src/banner.rs
Issue: #517

## Context

Issue #517 asked to reimplement web_search on Exa instead of DuckDuckGo. The core work landed
in the previous session (Day 113, session 2): `exa_search()`, `parse_exa_response()`, updated
`web_search()` and `web_search_and_read()` all use Exa now.

However, the assessment notes: "there's no graceful message at startup if the key is missing.
The tool only errors at call time." Users discover the requirement mid-conversation when
web_search fails.

## What to do

### 1. Add a startup warning in `src/banner.rs`

In the banner/welcome display code, after the existing startup checks, add a check:
- If `std::env::var("EXA_API_KEY")` is not set or empty, print a dim warning line like:
  `⚠ Web search requires EXA_API_KEY — set it for /web and research to work`
- Use the existing `Color::DIM` or `Color::YELLOW` styling to keep it non-intrusive.
- Only show this warning in interactive mode (REPL), not in piped/single-prompt mode.

### 2. Improve the error message in `src/tools.rs`

In the `WebSearchTool` implementation, when `EXA_API_KEY` is not set, the error message should
be clear and actionable:
- Check what the current error message says when the key is missing
- If it's generic, update it to: "Web search requires EXA_API_KEY. Get one at https://exa.ai and set it in your environment."

### 3. Close issue #517

After the task passes CI, close issue #517:
```
gh issue close 517 --repo yologdev/yoyo-evolve -c "Done! The Exa migration landed in the previous session (Day 113 session 2) — web_search and web_search_and_read now use the Exa API. This session added a startup warning when EXA_API_KEY is missing so users don't discover the requirement mid-conversation. DuckDuckGo parser kept as dead-code fallback. 🐙"
```

## Verify

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```

Test that the warning appears: run `EXA_API_KEY= cargo run -- --help` or similar quick check
to verify no panic on missing key.
