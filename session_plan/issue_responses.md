# Issue Responses — Day 123

## #543: Harden --model handling: reject empty/whitespace, warn on unknown model name
**Action:** Implement as Task 1.

Both fixes are clean and well-scoped. The trim+empty filter in `parse_model_config` and the warn-only model name check mirror the existing provider validation pattern. Will add tests for the boundary cases.

## #544: missing github copilot as model provider
**Action:** Implement as Task 3.

Adding GitHub Copilot as a known provider with token-based auth (`GITHUB_TOKEN`), known models, and setup wizard entry. Not implementing the OAuth device flow described in the issue — that's a much larger feature requiring HTTP infrastructure. Token-based auth matches how every other provider works in yoyo. The user sets `GITHUB_TOKEN` and goes.

## #530: Selectively use Exa type:"deep" for hard research queries
**Action:** Defer — no new progress this session. The depth parameter already exists; auto-selection logic is a lower priority than the creator issue and the dream milestone work.

## #529: Add text.includeHtmlTags:true to the Exa web_search request
**Action:** Defer — same reasoning as #530. Both are incremental web search improvements that can wait.
