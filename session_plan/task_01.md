Title: Proactive context compaction before prompt attempts
Files: src/prompt.rs, src/commands_session.rs
Issue: #173, #175

## Context

Issue #173 documents that evolution sessions hit 400 Bad Request because the agent accumulates tool outputs across turns, eventually exceeding the 200K token limit. The previous attempt (#175) failed because it tried to restructure PromptResult/PromptOutcome with new fields, breaking pattern matches.

This attempt uses a **minimal approach**: add a proactive compact check at the START of `run_prompt_with_changes`, before the retry loop even begins.

## Implementation

### 1. Add `proactive_compact_if_needed()` to `src/commands_session.rs`

Add a new function alongside the existing `auto_compact_if_needed()`:

```rust
/// Proactively compact conversation if context usage exceeds the proactive threshold.
/// This runs BEFORE a prompt attempt (not after) to prevent overflow during agentic execution.
/// Uses a tighter threshold (0.70) than the post-turn auto-compact (0.80).
/// Returns true if compaction was performed.
pub fn proactive_compact_if_needed(agent: &mut Agent) -> bool {
    let messages = agent.messages().to_vec();
    let used = total_tokens(&messages) as u64;
    let ratio = used as f64 / MAX_CONTEXT_TOKENS as f64;

    const PROACTIVE_THRESHOLD: f64 = 0.70;

    if ratio > PROACTIVE_THRESHOLD {
        if let Some((before_count, before_tokens, after_count, after_tokens)) = compact_agent(agent) {
            eprintln!(
                "{DIM}  ⚡ proactive compact: {before_count} → {after_count} messages, ~{} → ~{} tokens{RESET}",
                format_token_count(before_tokens),
                format_token_count(after_tokens)
            );
            return true;
        }
    }
    false
}
```

### 2. Wire proactive compact into `run_prompt_with_changes()` in `src/prompt.rs`

At the very start of `run_prompt_with_changes`, before the retry loop, add:

```rust
// Proactive compact: if context is already near the limit, compact before attempting
crate::commands_session::proactive_compact_if_needed(agent);
```

This is a single line addition. It runs before `saved_state` is captured, so the saved state will be the already-compacted version — clean and simple.

### 3. Tests

Add to `commands_session.rs` tests:
- `test_proactive_compact_threshold_lower_than_auto` — verify PROACTIVE_THRESHOLD < AUTO_COMPACT_THRESHOLD conceptually (document the relationship in a comment-test)
- `test_proactive_compact_returns_false_when_empty` — call on a fresh agent-like scenario (empty messages), should return false

Note: We can't easily create a real `Agent` in unit tests, but we can test the threshold logic and document the design.

### 4. Why this approach won't fail

The previous attempt (#175) failed because it:
- Added fields to PromptResult enum variants (breaking every `match` arm)
- Added fields to PromptOutcome struct (breaking every construction site)
- Tried to estimate tokens from tool results (new utility function with type issues)

This approach:
- Adds ONE new function to commands_session.rs (no existing code modified)
- Adds ONE line to run_prompt_with_changes (before existing code, no structural changes)
- Changes no enum variants, no struct fields, no function signatures
- The proactive_compact_if_needed function uses only existing imports and functions
