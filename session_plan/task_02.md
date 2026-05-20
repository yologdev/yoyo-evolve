Title: TypeScript and Python structured error parsing for watch fix prompts
Files: src/watch.rs
Issue: none

Day 79 added structured Rust compiler error parsing (`CompilerError`, `parse_rust_errors`, 
`categorize_error_code`, `categorize_message`, `build_error_summary`) that produces richer, 
category-specific fix prompts for Rust projects. Non-Rust projects still get a generic 
"Please fix the issues" prompt with raw output — no structured parsing, no category hints.

This task extends structured error parsing to TypeScript and Python, the two most common
non-Rust languages yoyo users are likely to work with. This directly closes the "improved 
error recovery limited to Rust" gap identified in the assessment.

**What to implement:**

1. **TypeScript error parser** — `parse_typescript_errors(output: &str) -> Vec<CompilerError>`
   Parse `tsc` output format: `src/file.ts(line,col): error TS2345: Argument of type...`
   Also handle `eslint` format: `src/file.ts:line:col: error ...`
   Categories: type errors (TS2xxx type family), import errors (TS2307, TS2305), 
   syntax errors (TS1xxx), and unused (no-unused-vars, TS6133).
   Category hints: similar to Rust but TypeScript-specific advice.

2. **Python error parser** — `parse_python_errors(output: &str) -> Vec<CompilerError>`
   Parse pytest output: `FAILED tests/test_foo.py::test_bar - AssertionError: ...`
   Parse mypy output: `src/foo.py:42: error: Incompatible types ...`
   Parse Python tracebacks: `File "foo.py", line 42, in func_name` → extract last frame.
   Categories: type (mypy errors), test assertion, import (ModuleNotFoundError, ImportError),
   syntax (SyntaxError, IndentationError).

3. **Update `build_watch_fix_prompt`** to try TypeScript/Python parsing when Rust parsing 
   returns empty results. Detection heuristic: check `watch_cmd` for `tsc`, `npm`, `npx`,
   `eslint`, `jest`, `vitest` → TypeScript; `pytest`, `python`, `mypy`, `ruff` → Python.
   Fall through to generic prompt if neither matches.

4. **Tests** (at least 8):
   - `parse_typescript_errors` with tsc output (type error, import error)
   - `parse_typescript_errors` with eslint output
   - `parse_python_errors` with pytest failures
   - `parse_python_errors` with mypy output
   - `parse_python_errors` with traceback
   - `build_watch_fix_prompt` with `npm test` and TS errors → structured output
   - `build_watch_fix_prompt` with `pytest` and Python errors → structured output
   - Non-matching output still falls through to generic prompt

**Important:** Reuse the existing `CompilerError` struct and `build_error_summary` function.
The new parsers should populate the same struct so the summary builder works unchanged.
Keep `ErrorCategory` — add new variants only if needed (the existing ones cover most cases:
Type, Import, Syntax, TestAssertion, Unused, Other).

**Do NOT** touch `parse_rust_errors` or existing Rust tests. This is purely additive.
