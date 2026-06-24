Title: Wire repo map signatures into auto-context injection
Files: src/commands_project.rs
Issue: none

## What

The assessment identifies this as the single biggest capability gap vs Aider: yoyo has a
sophisticated tree-sitter symbol extractor (`symbols.rs`) and a repo map generator
(`commands_map.rs`), but auto-context (`auto_context_for_prompt`) only injects raw file
contents. It doesn't inject a compact signature map showing function/struct/type signatures
of relevant files.

## Why

Aider's killer feature is sending a compact "repo map" (function signatures) with every prompt
so the model knows what exists in the codebase without reading full files. yoyo already builds
this map (`generate_repo_map_for_prompt_with_limit`) but only uses it for the `/map` command
and system prompt. Auto-context should include it.

## How

In `auto_context_for_prompt` (src/commands_project.rs), after scoring files and selecting
the top matches, also build a compact signature block for those files using the repo map data
that's already been fetched (the `build_repo_map` call is already there).

Specifically:

1. After the `score_files` call, build a compact "Relevant signatures" block from the matched
   files' symbols. For each matched file, list its function/struct/type signatures (one line
   each, like `fn auto_context_for_prompt(prompt: &str, recent_context: &[String]) -> Vec<...>`).

2. Inject this compact signature block as the FIRST entry in the auto-context output, before
   any file contents. This gives the model a structural overview before diving into specific files.

3. Keep the existing file-content injection as-is — the signatures complement, not replace, the
   full file reads.

4. Cap the signature block at ~2000 chars to keep token usage reasonable.

5. Update `format_auto_context` to include the signature section in its output.

## Constraints

- Touch only `src/commands_project.rs`
- The `build_repo_map` call is already in `auto_context_for_prompt` — reuse that data
- Add at least one test showing that auto-context now includes signature info
- The signature block should be concise — symbol names only, not full implementations

## Verify

```bash
cargo build && cargo test
cargo clippy --all-targets -- -D warnings
```
