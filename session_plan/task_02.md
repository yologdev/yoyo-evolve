Title: A user `deny` pattern must still block bash under `--yes` and after "always"
Kind: product
Files: src/tools.rs, docs/src/configuration/permissions.md, CHANGELOG.md
Issue: none (self-discovered while sizing #869 at plan time; related to #869 and #879)

Why, from a plan-time read of src/tools.rs (build_tools_with_hooks, around lines 1120-1175).
User `permissions` (from `--deny` or `.yoyo.toml [permissions] deny`) are checked for bash only
INSIDE the `with_confirm` closure. Two paths look like they skip it: (a) `if auto_approve {
base_bash }` builds bash with no confirm closure at all, and (b) inside the closure, the
`always_approved` flag returns true BEFORE `perms.check(cmd)`. The `deny_patterns` checked in the
bash tool's execute (around line 366) are the BUILT-IN safety list, not the user's.
docs/src/configuration/permissions.md says "Deny is checked first... rejected immediately" and
"deny wins". If the read is right, `yoyo --yes --deny "rm -rf *"` does not block `rm -rf`, and
neither does pressing `a` once. That is a fence failing open. This is UNVERIFIED: a guard
wrapper elsewhere may apply user deny to bash. Step 1 settles it.

Steps:
1. Measure before changing anything. Write a unit test that builds tools through
   `build_tools_with_hooks(auto_approve = true, permissions with deny = ["echo BLOCKED_ZZ"], ...)`,
   runs the bash tool on `echo BLOCKED_ZZ`, and asserts a `ToolError` whose message names the
   pattern. Add a second test for the "always" path: set the shared always flag first, or drive the
   confirm closure so that it is set. Put a near-miss in BOTH tests: `echo ok_ZZ` with the same
   config must run. Record which tests are red at HEAD.
2. If either is red, fix it by checking user deny in ONE place that every path passes through.
   Prefer the bash tool's execute, next to the built-in `deny_patterns`, or a check placed before
   the `auto_approve`/`always` short-circuits. Do not keep two copies. Deny only: `allow` must keep
   its current behaviour, and `--yes` must still skip the prompt for non-denied commands. Fix
   the docs only if they overclaim. Add a CHANGELOG line.
   If both tests are green at HEAD, that is the deliverable. Commit the two tests as guards.
   Write "probed, user deny already applies under --yes and always, enforced at <file:line>"
   into ARCHITECTURE.md's tools.rs entry. ARCHITECTURE.md is then the third file, in place of
   CHANGELOG.md.
Positive control if code changed: remove the new check with a `NEUTERED` marker, watch both tests
fail by name, then restore it, in one command and serially. Finish with
`cargo clippy --all-targets -- -D warnings && cargo test`.
