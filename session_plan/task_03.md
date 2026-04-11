Title: Enhance /history with token counts and turn numbering for /drop integration
Files: src/commands_session.rs, src/help.rs
Issue: none (pairs with /drop for context management workflow)

## Problem

The `/history` command shows messages as a flat list with sequential numbering:
```
  1. [user] What is...
  2. [assistant] Here's...
  3. [user] Now do...
  4. [assistant] Done...
```

This doesn't show:
1. How many tokens each message uses (critical for knowing what's eating your context window)
2. Turn numbers that align with `/drop` (so users can identify which turn to drop)

## What to Build

Enhance `/history` to show token-aware, turn-grouped output:

```
  Conversation (4 messages, 2 turns, ~12,450 tokens):
  Turn 1 (~3,200 tokens)
    1. [user] What is...             ~120 tokens
    2. [assistant] Here's...         ~3,080 tokens
  Turn 2 (~9,250 tokens)  
    3. [user] Now do...              ~250 tokens
    4. [assistant] Done...           ~9,000 tokens
```

This makes it easy to see:
- Which turns are eating the most context
- What turn number to pass to `/drop` to free up space

## Implementation

### In `commands_session.rs`:

Modify `handle_history` to:
1. Group messages into turns (a turn = consecutive user message(s) + consecutive assistant message(s))
2. For each message, estimate tokens using a simple heuristic: `text.len() / 4` (or use `total_tokens` if available per-message — check what yoagent's Message type exposes for token counts)
3. Display turn headers with cumulative token count
4. Display per-message token estimates
5. Show total summary at the top

Also add a helper function `fn group_into_turns(messages: &[Message]) -> Vec<(usize, Vec<(usize, &Message)>)>` that returns turn groups. This can be reused by `/drop` in Task 2.

### In `help.rs`:

Update the `/history` help text to mention token counts and turn grouping.

## Tests

Add tests:
- `test_history_turn_grouping` — verify messages are correctly grouped into turns
- `test_history_token_estimation` — verify token estimates are reasonable
- `test_history_empty` — empty conversation case still works

## Verification

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```
