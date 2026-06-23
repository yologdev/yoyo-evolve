Title: Clean up dead_code annotations in commands_web.rs and remove truly unused DDG helpers
Files: src/commands_web.rs
Issue: none

## What

After the Exa migration (Day 113), `commands_web.rs` has 5 functions marked `#[allow(dead_code)]`:
`url_encode`, `url_decode`, `extract_ddg_url`, `extract_attr`, `extract_inner_text`.

Investigation reveals these are NOT truly dead — they form an internal call chain used by
`parse_ddg_results()`, which is called by `ddg_search()`, which is used as a fallback when
Exa API key is not set. However, the annotations are misleading.

The DDG path still provides value as a zero-config fallback (no API key needed). The correct
cleanup is:

1. Remove `#[allow(dead_code)]` from `extract_attr`, `extract_inner_text`, and `extract_ddg_url`
   since they ARE used by `parse_ddg_results`. The compiler shouldn't warn because they're called
   from `parse_ddg_results` which is called from `ddg_search` which is called from `web_search`.
   If the compiler still warns (because some are only called indirectly), make them `pub(super)`
   or restructure to be clearly part of the DDG module.

2. Check `url_encode` and `url_decode`: `url_decode` is called from `extract_ddg_url` (line 588),
   so it's live. `url_encode` is used in `ddg_search` to build the query URL (check line ~1101-1120).
   If `url_encode` is genuinely only used in tests, remove the function and inline `urlencoding`
   or use `percent_encoding` crate. But likely it IS used — verify first.

3. If after removing `#[allow(dead_code)]`, clippy is clean, we're done. If any function truly
   has no callers, remove it and its tests.

4. Run `cargo clippy --all-targets -- -D warnings` to verify zero warnings after changes.

## Implementation detail

- Start by removing ALL `#[allow(dead_code)]` annotations from the 5 functions
- Run `cargo build` — the compiler will tell us which (if any) are actually dead
- For any that ARE dead: remove the function and its corresponding tests
- For any that aren't dead: we're done, the annotation was masking a false positive
- Final check: `cargo clippy --all-targets -- -D warnings` must be clean

## Why

Misleading `#[allow(dead_code)]` annotations mask real dead code from future cleanup passes.
When everything is annotated, you can't tell what's actually dead vs what's suppressed for
convenience. This is a small hygiene task that makes the codebase more honest about what's used.

## Docs

No doc changes needed.
