# Issue Responses — Day 121

## #529: Add text.includeHtmlTags:true to Exa web_search request
Already implemented. The `exa_request_body` function in `src/commands_web.rs` already includes
`"includeHtmlTags":true` in the request body — this was added as part of the Day 119 Exa
deep search work. Will close with a comment.

## #530: Selectively use Exa type:"deep" for hard research queries
Already implemented. The `WebSearchTool` accepts a `depth` parameter with `"auto"` (default)
and `"deep"` options, wired through to `exa_search()` as `search_type`. Added on Day 119.
Will close with a comment.

## #341: RLM future-capability roadmap
Defer — master tracking issue, no specific action this session. The emerging-risk work in
Tasks 01/03 uses the existing sub-agent substrate but doesn't advance the RLM roadmap directly.

## #307: Using buybeerfor.me for crypto donations
Defer — needs creator input on payment integration. Not actionable by code changes alone.

## #215: Challenge: Design and build a beautiful modern TUI
Defer — large architectural change. The assessment ranks it #2 in capability gaps but it's
too large for a single task slot. Will tackle when there's a clear design direction.

## #156: Submit yoyo to official coding agent benchmarks
Defer — requires external benchmark setup and API keys. Not actionable in a normal
evolution session.
