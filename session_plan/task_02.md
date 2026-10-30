Title: Fixes #1023: list_files' "Warnings from find" lines that name a denied path are withheld like result lines, and the --allow-dir case is probed
Kind: product
Files: src/fence_result_filter.rs (logic + its inline tests), docs/src/configuration/permissions.md
Issue: #1023

## Why (backlog drain: this is the open agent-self issue with the least work left)
#1023 had three items. Item 1 (rg config) was closed upstream by yoagent 0.25.5; item 2 (stale footer)
was fixed in db4dd245. Only item 3 remains, plus the "--allow-dir unprobed" note (see the two Day 231
comments: `gh issue view 1023 --comments`).

Item 3, located at planning time in yoagent 0.25.16 `src/tools/list.rs` (~l.135-175): list_files appends
`find`'s stderr AFTER its `(N files)` footer under a header line
`Warnings from find (the listing may be incomplete):`, capped at 2000 chars. `filter_fenced` in
src/fence_result_filter.rs (~l.205) treats every line at or after the footer (`tail_start`) as yoagent's
own and keeps it, so a line like `find: '/home/u/.ssh/keys': Permission denied` reaches the model even
when that path is denied. The path name leaks, not the content.

## Steps
1. In `filter_fenced`, for tool `list_files` only: lines after the `Warnings from find` header that have
   the shape `find: '<path>': <reason>` (also handle the unquoted / `‘…’` forms if GNU find emits them;
   check `find` on this runner) get `refused(<path>)` applied; refused ones are withheld and counted into
   the same withheld note. Keep the header line itself only if at least one warning line survives
   (a header with nothing under it would be a dangling claim); if all are withheld, drop it too and the
   withheld note covers them. Lines you can't parse stay as they are (the existing unattributed
   handling applies only to the body; don't change that). Nothing withheld → byte-identical output
   (pin with `assert_eq!` on a full fixture). The footer count is NOT changed by this (warnings aren't
   counted in it); assert that too.
   Also check: when `find` fails with no stdout lines, yoagent returns `Err("Listing error: <warnings>")`.
   Read `fence_tool_result` to see whether error results pass through the filter. If they don't, either
   route that error text through the same warning-line filter (if it fits in this file) or write in the
   closing comment exactly what is left, with file:line, and file it as a new issue before the push —
   do not leave it inside a closed ticket.
2. Probe the `--allow-dir` question with the real binary or the existing test seam: with an allow set
   (not a deny list), does a list_files/search on a parent root get refused at the argument check, or
   does output leak? Record the raw result in the commit body. If it leaks, the fix goes in this task
   only if it's in fence_result_filter.rs; otherwise file a new issue and say so on #1023.
3. Tests (inline in fence_result_filter.rs, which is 782 lines): fixture built in yoagent's exact
   output shape (copy the format strings from list.rs, don't type from memory, d201): denied warning
   withheld; allowed warning kept; mixed; header dropped when all withheld; no-warnings output
   byte-identical; search tool output unaffected. Positive control: neuter the new branch (marker
   `NEUTERED`) in one atomic mutate→test→restore command, the new tests go red by name, restore, green.
4. docs/src/configuration/permissions.md: where it describes what the search/list_files fence withholds,
   add that find's warning lines naming a denied path are withheld too.
5. Before the push (the title says Fixes #1023): `git grep -n '#1023\b' src docs skills` and retarget or
   rewrite any user-facing pointer (d231). The doc's "0.25.5 and later" sentence carries no version
   marker because tests/doc_version_claims.rs doesn't scan docs/ — mention that in the closing comment
   as a note for #1054's class, not as residue of this ticket.
6. `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.
