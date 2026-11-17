Title: Fixes #1196: a double-quoted mention of a dangerous command is judged the same whether or not the quotes also hold a $(...)
Kind: product
Files: src/safety.rs, src/safety_quoted_subst_tests.rs (or a new src/safety_quoted_mention_tests.rs if that file is about something else), tests/module_size.rs (only if safety.rs's line count moves)
Issue: #1196 (Part of #1202 too — say so in the body)
Dependency: none; checked yoagent skill "tools / policies": yoagent has no bash safety classifier, this is yoyo's own safety.rs.

## Why (user terms)
`echo "kill -9 1 at $(date)"` is refused ("Killing PID 1 (init process)") while `echo "kill -9 1"` and `echo 'kill -9 1'` pass.
The words `kill -9 1` are inside quotes in all three — they're text echo prints, not a command. Adding `$(date)` to the quotes
makes yoyo treat the whole quoted string as a command. Users writing log lines, commit messages (`git commit -m "... $(date)"`)
or echo-ed notes hit false refusals. Day 260's comment on #1196: the tree pass (bash_parse) already reads it right (extracts
only `echo` and `date`); the refusal comes from the RAW-TEXT pass in analyze_bash_command, kept as a second net until #1202 step 3.

## Steps
1. **Census, posted before any code edit.** For each of the SAFETY_CHECKS triggers (iterate the const, don't type a list: for each
   check, take one trigger string from its own existing tests), run `analyze_bash_command` on three forms:
   `echo "T"`, `echo 'T'`, `echo "T at $(date)"`. Do this as a temporary `#[test]` printing a table under `--nocapture` (or a
   permanent table test — see step 3). Record: how many checks flip between form 1 and form 3. Post the table's summary on #1196
   as a comment whose first 200 characters state "N of M checks refuse the $(...) form but not the plain quoted form".
2. **Find the one mechanism, fix it there.** Expected (UNVERIFIED — read the code to confirm): the quote-stripping step
   (`strip_quoted_regions` or the per-check splitting at `$(`, SUBSHELL_BREAKS from #1193) keeps a double-quoted region unstripped,
   or splits it at `$(`, when it contains a substitution, so the literal text around the substitution reaches the checks as if it
   were command text. The fix: the literal parts of a double-quoted string are treated as mentions, while the CONTENTS of every
   `$(...)` / backtick inside it are still checked as commands. Fix the mechanism once, not per check.
   **Must-still-refuse rows (do not weaken these):** `echo "$(rm -rf /)"`, `echo "x $(kill -9 1) y"`, `` echo "`kill -9 1`" ``,
   `bash -c "kill -9 1"`, `sh -c "rm -rf / $(date)"`, `eval "kill -9 1 $(date)"` (eval/-c bodies are commands, not mentions —
   check how the plain `bash -c "kill -9 1"` is handled today and keep that verdict), and every row of #1189's / #1193's existing
   tests (src/safety_quoted_subst_tests.rs, src/safety_git_glued_tests.rs SEGMENT_ROWS). Never delete or edit an existing test's expectation.
3. **Pin it.** A permanent table test over every SAFETY_CHECKS member: the `"T at $(date)"` form gets the same verdict as `"T"`
   (assert equality of the Option, full value). Plus the must-still-refuse rows above, each asserted Some(..).
4. **Differential (this narrows a detector — Day 259 lesson).** Run old vs new `analyze_bash_command` over the whole existing safety
   corpus plus a generated set (each trigger × {inside "..." with $(date) before/after/both sides, inside $(...) inside "...", glued
   with ; && | after the quotes}). List every flip in BOTH directions in the commit body. Every flip toward "stops refusing" needs a
   reason (it was a mention) or gets reverted. Expected: only mention-inside-quotes rows flip to pass.
5. **Recall control:** neuter the fix with a `NEUTERED` marker (check `grep -c NEUTERED` > 0 before running), watch the step-3 table go
   red by name, restore in the SAME command, green again. Post the result on #1196 when it prints, before the full gate.

## Closing
If step 1 shows every flipping check shares the mechanism you fixed and step 3 passes for all of them: `Fixes #1196`, `Part of #1202`.
If any check still differs after your fix, title it `Part of #1196` and list the remaining checks on the ticket (scope first).
Before `Fixes #1196`, `git grep -n '#1196' src docs` and retarget any comment that says "tracked in #1196" (SEGMENT_ROWS has a note
about it being unpinned — update that note).

## Size gate
src/safety.rs is registered at 4814 and is at 4814. Keep production lines in safety.rs minimal; tests go in the tests file.
If safety.rs's count changes, paste the new count over its entry in tests/module_size.rs with a Day 262 (23:24) note.

Verify: cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check.
