Title: Part of #1127: shell `yoyo blame` and `yoyo ast` print `yoyo <verb>` usage, REPL keeps `/<verb>`
Kind: product
Files: src/commands_git_review.rs, src/commands_ast_grep.rs, src/dispatch_sub.rs (plus tests/integration.rs for the binary-level pin)
Issue: #1127
Dependency: none; checked the yoagent skill's tools section (this is yoyo's own CLI text, nothing upstream renders it)

## Why (user terms)
A user types `yoyo blame` in their shell and is told `Usage: /blame <file>` — a slash form their shell can't run. Exit codes are already right (2); only the text is wrong. Re-probed this morning on the built binary: still live for blame, docs, ast, risk, memories. This task takes two of them (blame, ast) whose shell paths already go through their own `*_status` functions, so the REPL path doesn't need touching. The REPL's `/blame` text is correct where it's shown, so it stays — that is a correctness reason, not a byte-identity reflex.

## Precedent to copy (read first)
`grep_usage(surface)` in src/commands_search.rs and its tests `grep_def_outline_usage_names_the_verb_as_typed_on_each_surface` / `grep_and_outline_repl_usage_is_byte_identical_to_before_1127` (commit 3ea76ec6); `commands_dev::command_form(DoctorSurface, verb)`; `DoctorSurface::Shell` is already imported in dispatch_sub.rs (l.13).

## Steps
1. `src/commands_git_review.rs` (~l.393): build the usage string from `command_form(surface, "blame")` for both mentions (`<file>` and `<file>:<start>-<end>`). Give `handle_blame_status` (shell path, dispatch_sub.rs ~l.616) a surface argument, or add a `_on(input, surface)` variant; the REPL's `handle_blame` passes `DoctorSurface::Repl` so `src/dispatch.rs` is untouched.
2. `src/commands_ast_grep.rs` (~l.203 and ~l.238, two copies of `Usage: /ast <pattern> [--lang <lang>] [--in <path>]`): one helper `ast_usage(surface)` used by both, shell path via `handle_ast_grep_status` (dispatch_sub.rs ~l.673). Grep the file for any other `/ast` text the shell can reach (e.g. "sg not installed" hints) and run it on both surfaces.
3. `src/dispatch_sub.rs` is at its recorded size (2663): change only the two call sites, no new lines; if rustfmt adds any, paste the new count into tests/module_size.rs.
4. Tests: unit test per verb naming both surfaces (`blame_and_ast_usage_names_the_verb_as_typed_on_each_surface`): Shell contains `yoyo blame` / `yoyo ast` and no `/blame` / `/ast`; Repl contains `/blame` / `/ast`. Update tests/integration.rs ~l.3950 (`"  ✗ Usage: /blame <file> ..."` pins the SHELL output with the bug) to the shell form, and grep tests/integration.rs for any `/ast` constant the same way.
5. Probe the built binary, empty temp dir, `</dev/null`: `yoyo blame`, `yoyo ast`, `yoyo ast --lang` — quote each output and exit code in the commit body; exit codes unchanged (2).
6. `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.

## Done / commit
Title `Part of #1127: ...` (docs, lint, risk, skill, map, memories remain). Comment on #1127 with first line: "Scope: still open = docs, lint, risk, skill, map, memories; done = grep, def, outline, blame, ast (sha)." If help.rs is touched at all (it shouldn't be), pre-pay its size entry ("src/help.rs", 2946) first.
