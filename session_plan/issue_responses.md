# Issue Responses

## #212: About /map — ast-grep instead of regex

Implement as Task 1. The `/map` command currently uses hand-written regex extractors which work well for common patterns but can miss edge cases (symbols in macros, complex generics, etc.). ast-grep would give more accurate structural parsing. The plan: when `sg` is installed, use it as the primary backend for `/map` symbol extraction; when it's not, fall back to the existing regex extractors. Adding a `--regex` flag to force the regex backend for comparison. This keeps `/map` working for everyone while giving users with ast-grep better accuracy.

## #205: --fallback CLI flag for provider failover

Acknowledging @BenjaminBilbro's excellent point — they already handle fallbacks via LiteLLM, pointing yoyo to that endpoint. This is a valid architecture: the fallback logic lives in a proxy layer rather than in the client. After four implementation attempts that all reverted due to test failures, and given that external proxy solutions (LiteLLM, OpenRouter) already handle this cleanly, the honest assessment is:

1. The four failed attempts suggest the in-process approach has real complexity (provider trait differences, streaming state, error propagation)
2. External solutions already exist and work
3. The ROI of building this natively is lower than it seemed

Response: Comment acknowledging the LiteLLM approach, note that users can already point `--base-url` at a LiteLLM endpoint for provider failover today. Keep the issue open for potential future implementation but de-prioritize — practical users have a working solution.

## #156: Submit to coding agent benchmarks

No new action needed. Community members (@BenjaminBilbro) have volunteered to run benchmarks with local models. @yuanhao's note says "for your information only." The right move is to stay quiet and let the community-driven effort happen naturally.
