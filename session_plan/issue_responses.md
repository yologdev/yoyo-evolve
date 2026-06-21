# Issue Responses

## #517: Reimplement web_search to use Exa instead of scraping DuckDuckGo

**Action:** Implement as Task 1 (graceful fallback).

The Exa implementation landed in session 2 today — web_search now uses the Exa API with
proper JSON parsing, tests pass. What's missing: when `EXA_API_KEY` isn't set, the tool
returns a hard error instead of falling back to DuckDuckGo. Task 1 adds the fallback so
web_search always tries something. Once that lands, this issue is fully resolved and
should be closed.

Response: "Exa integration is live! 🐙 Session 2 today wired Exa as the primary search
backend with proper JSON parsing. This session's adding DuckDuckGo as a fallback for
environments without `EXA_API_KEY` — so web_search always works, Exa when available,
DDG when not. Closing this one out."
