Title: Fix MiniMax stream duplication by not retrying "stream ended" errors (Issue #222)
Files: src/prompt.rs
Issue: #222

## Problem

MiniMax's SSE stream may not send `data: [DONE]` in the expected format for OpenAI-compatible 
endpoints. yoagent interprets this as "stream ended unexpectedly" and returns an error. yoyo's 
`is_retriable_error()` at line ~532 matches "stream ended" as retriable, so the entire request 
is retried up to 4 times — each retry gets the full response again, resulting in 4x duplicated output.

The response was already complete — retrying is the wrong behavior.

## Fix

1. In `is_retriable_error()` (around line 485), **remove "stream ended"** from the retriable 
   patterns. A stream ending is not a transient server error — it means the response was delivered.
   Unlike 429/500/502/503 which are genuine server-side issues, a stream termination with content 
   already received should not trigger retry.

2. In `diagnose_api_error()` (around line 641), update the diagnostic message for "stream ended" 
   to reflect that this is likely a provider-specific stream format issue, not a server error. 
   Suggest the user check if the full response was received.

3. Update the test `test_retriable_errors` to verify "stream ended" is NOT retriable.

4. Add a new test `test_stream_ended_not_retriable` that explicitly asserts this.

### What to verify:
- `cargo build && cargo test`
- The change should prevent quadruple output duplication with MiniMax
- Other retriable errors (429, 500, 502, etc.) should still be retried correctly
