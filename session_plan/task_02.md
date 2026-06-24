Title: Warn clearly when web_search returns no results due to missing API key
Files: src/commands_web.rs, src/tools.rs
Issue: none

## What

The assessment identifies a UX bug: when Exa API key is unset, `web_search` falls back to
`ddg_search`, which scrapes DuckDuckGo HTML. But DDG serves captcha walls to automated scrapers,
so the fallback almost certainly returns empty results silently. The user gets no search results
and no error — just silence.

## Why

Silent failure is the worst kind of failure. A user who types a search query and gets nothing
back has no idea whether the query was bad, the service is down, or they're missing a config.
This is basic UX — tell people what happened.

## How

1. In `web_search()` in `src/commands_web.rs`: after the DDG fallback returns empty results,
   add a clear warning message to the output explaining that:
   - No results were found
   - The Exa API key (`EXA_API_KEY`) is not set
   - Setting it would enable reliable web search
   - Brief instruction: `export EXA_API_KEY=your-key`

2. In the `WebSearchTool` implementation in `src/tools.rs`: ensure the warning propagates
   to the tool output so the agent (and user) see it.

3. Don't remove the DDG fallback code — it still has value if DDG ever stops blocking scrapers.
   Just make the empty-result case informative instead of silent.

## Constraints

- Touch at most `src/commands_web.rs` and `src/tools.rs`
- Don't remove existing DDG code
- Add a test that verifies the warning message appears when results are empty and EXA_API_KEY is unset
- Keep the warning concise — 2-3 lines max

## Verify

```bash
cargo build && cargo test
cargo clippy --all-targets -- -D warnings
```
