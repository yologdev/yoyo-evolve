Title: Graceful DuckDuckGo fallback when EXA_API_KEY is unset
Files: src/commands_web.rs, src/tools.rs
Issue: #517

## Goal
When `EXA_API_KEY` is not set, the `web_search` tool currently returns a hard error
telling the user to get an API key. This breaks the agent's ability to search the web
in environments where Exa isn't configured. Add a graceful fallback: try Exa first,
and if `EXA_API_KEY` is missing, fall back to the existing DuckDuckGo scraper.

DuckDuckGo may return empty results (captcha), but that's better than an immediate error —
at least the agent can try, and the output will say "no results found" which the agent
can reason about, rather than a hard configuration error that stops the search entirely.

## Implementation

In `src/commands_web.rs`:

1. Modify the `web_search(query, max_results)` function (currently just calls `exa_search`):
   - Check if `EXA_API_KEY` is set in the environment
   - If set: call `exa_search()` as today
   - If NOT set: call the existing DuckDuckGo scraping path (`parse_ddg_results` after
     fetching from `html.duckduckgo.com`)
   - This makes web_search always work — Exa when available, DDG as fallback

2. The DuckDuckGo fetch+parse code already exists in the file (the old `parse_ddg_results`
   function and the URL scraping). Wire it as a fallback function, e.g. `ddg_search(query, max_results)`.

3. In `web_search_and_read`, if Exa fails for a non-key reason (network error, rate limit),
   also try DDG as a second attempt.

In `src/tools.rs`:
- No changes needed — the `WebSearchTool` already calls `web_search_and_read` which will
  get the fallback behavior automatically.

## Tests
- Test that `web_search` returns DDG results when `EXA_API_KEY` is unset (use existing
  DDG parser tests as a model)
- Test the fallback path selection logic

## Verification
- `cargo build && cargo test`
- `cargo clippy --all-targets -- -D warnings`

## After
Once this lands, issue #517 is fully resolved. The issue response agent should close it.
