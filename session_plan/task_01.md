Title: Enhance /map with optional ast-grep backend (Issue #212)
Files: src/commands_search.rs, src/help.rs, docs/src/usage/commands.md
Issue: #212

## Context

Issue #212 asks if `/map` can use ast-grep instead of regex for symbol extraction. Currently, `/map` uses hand-written regex extractors per language (Rust, Python, JS, TS, Go, Java). The `/ast` command already wraps ast-grep for pattern search. ast-grep can also output structured node information that could power `/map`.

## What to do

Add an ast-grep backend for `/map` that activates when `sg` is available on PATH. When ast-grep is installed, use it for more accurate symbol extraction. When it's not, fall back to the existing regex extractors (the current behavior).

### Implementation plan

1. **Write tests first** — add tests in `commands_search.rs` that verify:
   - `extract_symbols_ast_grep()` returns symbols in the expected format for a simple Rust file
   - Graceful fallback when `sg` is not installed (returns empty, falls through to regex)
   - The `/map` output format doesn't change regardless of backend

2. **Add `extract_symbols_ast_grep(path, language)` function** that:
   - Calls `sg scan --json` on a file with rules that match function/struct/enum/trait/class definitions
   - Parses the JSON output into the existing `Vec<Symbol>` format
   - Returns `None` if `sg` is not available or the call fails (so callers can fall back)

3. **Wire it into `build_repo_map()`** — for each file:
   - Try `extract_symbols_ast_grep()` first (if available)
   - Fall back to `extract_symbols()` (regex) if ast-grep fails or isn't installed
   - The check for `is_ast_grep_available()` already exists, so reuse it

4. **Add a `--regex` flag** to `/map` that forces the regex backend even when ast-grep is available (useful for comparison/debugging)

5. **Show which backend is being used** — when `/map` runs, display a dim note like "using ast-grep" or "using regex (install sg for better accuracy)"

6. **Update help text** in `help.rs` for `/map` to mention the ast-grep enhancement

7. **Update docs** in `docs/src/usage/commands.md` for the `/map` entry

### ast-grep approach

The simplest approach: use `sg scan` with inline rules per language. For Rust:
```
sg scan --json --inline-rules '{ id: rust-fn, language: rust, rule: { kind: function_item } }' file.rs
```

This gives structured JSON output with node text, start/end positions. Parse the function/struct/enum names from the matched nodes.

Key languages to support: Rust, Python, JavaScript/TypeScript, Go, Java (matching the existing regex support).

### Important notes

- The regex backend must remain fully functional — ast-grep is optional
- Don't break existing `/map` behavior for users without ast-grep
- Run `cargo test` after implementation to verify all existing map tests still pass
- Keep the `extract_symbols()` regex functions unchanged — they're the fallback
