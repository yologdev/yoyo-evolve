Title: Reimplement web_search tool on Exa API (fix broken core tool)
Files: src/commands_web.rs, src/tools.rs
Issue: #517

## Problem

The native `web_search` tool scrapes `html.duckduckgo.com`, which returns CAPTCHAs to CI/server IPs. The parser sees no results, returns "no search results found", and the agent answers from training memory instead of the web. This is the most embarrassing capability gap — a broken core tool.

The `research` skill was already rerouted to Exa (commit `5b6a738`), but the native `WebSearchTool` (used by the agent itself and by `/web search`) still uses the dead DuckDuckGo path.

## What to do

### In `src/commands_web.rs`:

1. **Add Exa search function** — `exa_search(query: &str, max_results: usize) -> Result<Vec<WebSearchResult>, String>`:
   - `POST https://api.exa.ai/search`
   - Headers: `x-api-key: $EXA_API_KEY`, `Content-Type: application/json`
   - Body: `{"query": <q>, "type": "auto", "numResults": <n>, "contents": {"text": {"maxCharacters": 2000}, "highlights": true}}`
   - Parse `.results[]` — each has `.title`, `.url`, `.text`, `.highlights[]`
   - Map to existing `WebSearchResult` struct

2. **Rewrite `web_search()` function** — call `exa_search` instead of DuckDuckGo scraping. If `EXA_API_KEY` is not set, return a clear error: "web_search requires EXA_API_KEY environment variable" (never "no results found").

3. **Rewrite `web_search_and_read()`** — call `exa_search` which already returns page text in `.text` field (Exa returns search + content in one call). No need for a separate fetch step.

4. **Keep the DuckDuckGo functions** (`extract_ddg_url`, `parse_ddg_results`) for now — they can be removed in a follow-up. Just stop calling them from the main paths.

5. **Add pure tests** (no network):
   - `test_parse_exa_response` — parse a mocked Exa JSON response, assert titles/urls/text extracted correctly
   - `test_exa_missing_key` — ensure clear error when `EXA_API_KEY` is unset
   - `test_exa_api_error` — ensure clear error on API error JSON

### In `src/tools.rs`:

No structural changes needed — `WebSearchTool` already calls `commands_web::web_search_and_read()`. The tool name, schema, and wiring stay the same. Just verify the existing tests still pass.

## Verification

- `cargo build && cargo test`
- `cargo clippy --all-targets -- -D warnings`
- The tool should return a clear "EXA_API_KEY not set" error instead of silently failing with "no results"
