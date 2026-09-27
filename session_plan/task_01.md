Title: /retry uses the tool name the prompt loop already carries, instead of string-scanning the error (#742)
Kind: product
Files: src/commands_retry.rs, src/dispatch.rs, src/repl.rs
Issue: #742

## The defect (verified by grep in planning, not recalled)

`handle_retry` (src/commands_retry.rs:111) works out which tool failed by text-scanning the
error string (`extract_tool_name_from_error`, :55, walking a hand-typed `KNOWN_TOOL_NAMES`).
Meanwhile `PromptOutcome.last_tool_name` (src/prompt.rs:190) already carries the real name, and
prompt.rs itself already uses it for auto-retry (`build_auto_retry_prompt(input, err,
outcome.last_tool_name.as_deref(), attempt)`, prompt.rs:1490). So the REPL's `/retry` guesses at
a value the auto-retry path gets for free. An error message that mentions a different tool
(e.g. an edit_file failure whose text quotes "bash") makes the guess wrong.

## The blocker that stopped the last two attempts, named

#773 and #779 both died with "agent exited without changing anything" / module-size failure.
The reason is plumbing, not difficulty: `last_tool_name` dies at the prompt -> REPL boundary.
The REPL stores only `last_error: Option<String>` (repl.rs:963, carried by reference in the
REPL ctx at repl.rs:681 and in the dispatch ctx at dispatch.rs:400), so `handle_retry` has no
way to see the name. The fix is a SIBLING field that travels exactly where `last_error` travels.
Do NOT touch src/prompt.rs (it is not needed: the outcome already has the field) and do NOT
touch src/dispatch_sub.rs.

## Steps (ORDER MATTERS — commit before any cargo invocation)

1. Pure helper first, in src/commands_retry.rs:
   `pub(crate) fn resolve_retry_tool(carried: Option<&str>, error: Option<&str>) -> Option<String>`
   — returns `carried` when it is Some and non-empty, else falls back to the existing
   `extract_tool_name_from_error(error)`. Keep the string-scan as the fallback (do not delete it —
   it still serves paths that do not carry a name, e.g. the watch-fix result). Change
   `handle_retry` to take a new `last_tool: Option<&str>` parameter and call the helper.
   Add the table test in the same edit:
   - carried Some("edit_file"), error text mentioning "bash" -> "edit_file" (the bug)
   - carried None, same error -> byte-identical to what the old scan returns (near-miss guard:
     every path that carries no name must behave exactly as before; assert_eq!)
   - carried Some(""), error mentioning "read_file" -> the scan result (empty is not a name)
   - carried None, error None -> None
2. Plumb: add `last_error_tool: Option<String>` next to `last_error` in repl.rs's session
   state, and a `last_error_tool: &'a mut Option<String>` next to `last_error` in both ctx
   structs. Everywhere `*ctx.last_error = <x>` is assigned, assign the sibling in the same
   place: from a PromptOutcome use `outcome.last_tool_name.clone()` when `last_tool_error` is
   Some, else None; where no tool name exists (watch result, `= None` clears, handle_retry's own
   return) set None. Grep `last_error` in both files and treat every hit as a site — a missed
   site leaves a STALE name paired with a new error, which is worse than the guess.
   Pass `ctx.last_error_tool.as_deref()` into `handle_retry` at dispatch.rs:1167.
3. `git add -A && git commit -m "WIP #742: plumb last_tool_name to /retry"` — BEFORE cargo.
4. `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.
   If tests/module_size.rs warns that repl.rs/dispatch.rs grew past their recorded entries,
   paste the new (path, N) over the entry as the message says; if dispatch.rs is not
   grandfathered and goes >50 past the 2000 cap, move the helper test or code rather than
   registering. Commit again.

## Docs
One short entry in ARCHITECTURE.md under src/commands_retry.rs: the carried name wins, the
string scan is now the fallback only, and why (two prior no-progress reverts were the missing
REPL-side field). Not CLAUDE.md.

## Done when
The table test above is green, `handle_retry` has no path that ignores a carried name, and the
four checks pass. Positive control (serial, atomic mutate->run->restore): make
`resolve_retry_tool` ignore `carried` and watch the "edit_file vs bash" row fail by name.
