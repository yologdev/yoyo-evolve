Title: Part of #1184: [permissions] and [directories] lists are read from the parsed toml::Table, so a multi-line deny list builds its fence
Kind: product
Files: src/config.rs, src/config_toml_read.rs, src/config_toml_tests.rs (and src/config_value.rs ONLY for the unreadable-value warning, see step 3)
Issue: #1184
Dependency: the `toml` crate (already a direct dependency, used by config_toml_read.rs since Day 259); checked yoagent skill: config parsing is not a yoagent feature, nothing to reuse there.

## Why (user terms)
A user who writes the most common TOML shape for a long list:

```
[permissions]
deny = [
  "rm *",
]
```

gets NO deny fence. The assessment probed this on the built binary (Day 262 23:24, temp repo): `yoyo doctor` prints
`warning: .yoyo.toml:2: could not read the value of '[permissions] deny' (an array over several lines); it was ignored. This rule is NOT in force.`
python3 tomllib reads the same file as `{'permissions': {'deny': ['rm *']}}`. Same for `[directories] deny`. Warned, not silent,
but the fence the user wrote does not exist. This is #1184's named next slice (the owner's ticket; see its Day 259 comments:
"[directories] is the security-relevant one, so it should go next").

## The done-when metric of #1184 (report it before AND after, in the commit body and the ticket comment's first 200 chars)
#1184's done-when is "no line-by-line TOML reading is left in src/". Before editing, count the line-walker entry points that read
config TOML (e.g. `grep -n "content.lines()\|parse_toml_array\|\.lines()" src/config.rs src/config_value.rs src/config_unknown_keys.rs`
— state the exact command you ran) and record the number. After, run the same command. This slice must make that number go DOWN.
If any helper you leave behind still has callers (parse_toml_array may be used elsewhere), list its remaining callers as deletion debt
on the ticket. Do NOT add new code to the hand walker to make it "agree" with the new reader (Day 259 lesson: the hybrid grows the thing
the ticket exists to delete).

## Steps
1. **Census the readers first (no code).** Grep for every reader of `[permissions]` and `[directories]` content:
   `parse_permissions_from_config`, `parse_directories_from_config` (config.rs ~488-550), and anything else that reads those
   sections' keys (config_unknown_keys.rs, config_value.rs's `warn_unreadable_values_once`, `/permissions` display, doctor,
   the trust/project gates in cli.rs that call them). Write the list into the commit body: reader -> reads the authority / its own copy.
   Also list which keys the line reader accepts in each section today (allow, deny, and anything else) — the new reader must accept
   the same key set, no more, no fewer.
2. **Move the two parsers onto the Table.** Build `PermissionConfig` / `DirectoryRestrictions` from the parsed `toml::Table`
   (the same parse `config_toml_read.rs` already does for mcp_servers/model_pricing; reuse its entry point, don't parse twice).
   Keep the public signatures if that keeps callers unchanged. Every TOML spelling must read the same:
   header + single-line array, header + multi-line array, trailing comments inside the array and after it, dotted keys
   (`permissions.deny = [...]` at top level), inline table (`permissions = { deny = [...] }`), `[ permissions ]` with spaces.
   A non-string array element or a non-array value: keep a LOUD warning naming file and key and saying the rule is NOT in force
   (product guarantee from #1184 step 3: never silently ignored). Keep the existing directory wildcard warning
   (`directory_wildcard_warning`) and any `CLOUD_METADATA_HOSTS` behaviour exactly — they consume the parsed list.
3. **The warning must stop lying.** `could not read the value of '[permissions] deny' (an array over several lines)` must no longer
   fire for a shape that is now read. Prefer deleting that branch for these two sections (or the sections from its table) over
   teaching it new shapes. If it lives in config_value.rs, that is the 4th file; keep that edit to the minimal removal.
4. **Tests in src/config_toml_tests.rs** (one row per spelling in step 2, both sections, allow and deny): assert the resulting
   rule list exactly (full vec equality, not contains). Add the #1177 shape (comment after an array value) and the assessment's
   multi-line shape as named rows. Add one near-miss: a key under `[permissions]` that isn't a permissions key still gets the
   unknown-key warning and builds no rule. **Recall control:** with the new multi-line read neutered (marker `NEUTERED` on the line,
   `grep -c NEUTERED` > 0 checked BEFORE running), the multi-line rows must go red by name; restore in the SAME command and re-run green.
   Post the before/after result on #1184 the moment it prints, before the full gate (Day 260/261 lesson).
5. Built-binary probe (after `cargo build` returns, same command): the assessment's temp-repo file → `yoyo doctor` no longer warns,
   and the deny rule is in force (e.g. `/permissions` or `config show` lists it; pick whichever surface prints the rule list).

## Size gate
src/config.rs is 4739 lines vs registered 4696 (+43, inside the grace band). If config.rs shrinks, ratchet the entry in
tests/module_size.rs down with a Day 262 note; if it grows past the band, paste the new count with a reason. Do not touch
src/context.rs or src/format/mod.rs (both are near FATAL).

## Docs
If docs/src/configuration/permissions.md says a list must be on one line, or shows only single-line arrays, add one multi-line example.

## Out of scope (still open on #1184, say so in the comment): toml_edit writes (`set_toml_key`), deleting the remaining hand parser
and the tomllib differential. Title stays "Part of #1184".

Verify: cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check.
