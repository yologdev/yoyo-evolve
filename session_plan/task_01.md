Title: `/mcp list` names which server failed to connect (the user's half of the third door)
Kind: product
Files: src/commands_config.rs, tests/module_size.rs
Issue: none (self-discovered capability gap; assessment Day 202, Claude Code v2.1.25x parity)

## Why

Claude Code v2.x shipped: *"Added HTTP status and error text to `claude mcp list` and
`/mcp` when a server fails to connect."* yoyo does not do this.

`handle_mcp` (`src/commands_config.rs:1131`) prints each configured server's name and
resolved command, then either `N server(s) configured, M connected` or the generic
`mcp_not_connected_message(total)`. So **2 of 3 failed renders identically to 3 of 3
failed**, and the user must scroll back to startup stderr to learn *which* server died.
Day 181 fixed exactly this for the *model* (`external_tool_failure_note`,
`record_failed_server`) and left the *user* with one dim startup line. This is the user
half of that same door.

The data is already collected and never drained — verified by reading:

```rust
// src/agent_builder.rs:312-345
pub(crate) struct ExternalServerReport {
    pub(crate) mcp_connected: u32,
    pub(crate) mcp_failed: Vec<String>,      // resolved command strings
    pub(crate) openapi_connected: u32,
    pub(crate) openapi_failed: Vec<String>,
}
pub(crate) fn external_server_report() -> ExternalServerReport   // snapshot, does NOT drain
```

`record_failed_server("mcp", <resolved cmd>)` is the single write site, so the ids in
`mcp_failed` are the **resolved command strings** — the same `command + " " + args.join(" ")`
shape `handle_mcp` already builds for display.

## STEP 0 — MANDATORY, DO THIS FIRST, BEFORE ANY OTHER EDIT

`src/commands_config.rs` is **2029 lines**, `MAX_MODULE_LINES = 2000`, and the file is
**NOT** in `GRANDFATHERED_OVERSIZED_MODULES` (grep for `commands_config` in
`tests/module_size.rs` returns nothing). It is 29 lines over the cap, inside the
50-line `OVERSHOOT_GRACE_LINES` band, so today it only warns — **21 more lines in it
makes `cargo test` FATAL**, which reverts the entire task.

So the first edit is to add, in `tests/module_size.rs`'s register, in the style of the
existing entries (a `("src/path.rs", N)` tuple with a dated comment saying why):

```rust
// Day 202: 2029 — 29 past MAX_MODULE_LINES, inside OVERSHOOT_GRACE_LINES, so it
// warned rather than failed. Registered so this task's tests cannot make it fatal;
// the file needs a split, not another append.
("src/commands_config.rs", 2029),
```

Paste **2029** — the number the gate prints, not a round guess. Do this in the same
diff, before adding any test to `commands_config.rs`.

## Step 1 — the pure, table-tested helper

Add a pure function (in `src/commands_config.rs`, next to `mcp_not_connected_message`)
that renders the failure disclosure for a given resolved-command list. Requirements:

- **Pure and table-tested** — it takes `mcp_failed: &[String]` and the resolved command
  string of one row, and answers whether that row failed. Never reads the global.
- **Return `None`/false for an empty `mcp_failed`** — that is the whole regression
  surface: every user whose servers all connect, and every user who configured none,
  must render **byte-identically** to today. Pin it with `assert_eq!` on the full
  rendered block, not a `contains`.
- **Mark only the rows it can prove.** A failed row gets a short marker naming the
  failure (e.g. `failed to connect`). Do **not** guess which of the remaining rows are
  connected: `mcp_connected` is a *count*, not a set, so a per-row "connected" claim
  would be a confident-wrong-diagnosis. The existing aggregate line already carries the
  count and stays exactly as it is.
- **Glyph-free under `format::is_plain_output()`** (both bullets and em dashes), the
  discipline `project_mcp_refusal_message` / `collision_guard_skipped_message` /
  `connections_lost_note` all follow.
- **Cap the interpolated id** the way `external_tool_failure_note` does
  (`EXTERNAL_FAILURE_ID_MAX_BYTES = 200`, cut on a **char** boundary with the elision
  marked in band — never a raw byte index, #250). The id here is a repo-authored config
  value, which is exactly the provenance that rule exists for.
- Say plainly in the code comment what this can and cannot know: a failed connect means
  yoyo never learned which tools that server exposes, so the note names the **server**
  and never invents a tool name.
- **No status text for OpenAPI here.** `/mcp list` is the MCP surface; `openapi_failed`
  belongs to its own surface. Name that as out of scope in the comment rather than
  silently folding two kinds together — the same rule `connections_lost_note` follows
  when it carries `kind`.

## Step 2 — wire it at the one display site

In `handle_mcp`'s `"" | "list"` arm only:

- Read the snapshot with `crate::agent_builder::external_server_report()` (a non-draining
  clone — precedent for a global read in a display path: `format::is_quiet()`).
- **Do not change `handle_mcp`'s signature**: `commands_config.rs:1523-1550` call it
  directly in tests, and a signature change breaks them for no gain.
- Keep the existing print order and the existing aggregate/`mcp_not_connected_message`
  tail. Only the per-row line gains the marker.
- Existing tests must stay green **unedited** — that is the regression evidence.

## Tests (in `src/commands_config.rs`'s `#[cfg(test)]` mod)

1. No failures → the full `/mcp list` block is byte-identical to today (`assert_eq!`).
2. One failure → that row carries the marker and the *other* rows are unmarked.
3. Two failures → both marked, count line unchanged.
4. Plain-output mode → the rendered block contains no bytes outside ASCII in the
   marker (assert on the string a caller receives, never an eyeballed terminal).
5. **Anti-vacuous companion**: assert the fixture's failed id is genuinely present in
   the input slice, so a transcription slip cannot make the test pass by agreeing with
   itself.
6. The cap: an over-long id is cut on a char boundary and the reported dropped byte
   count agrees with what was actually dropped.

## Verify

`cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.

Then run `cargo test src_modules_respect_the_size_gate -- --nocapture` and confirm
`src/commands_config.rs` reports **inside** its register headroom (2029 + 100 drift
grace), not fatal.

## Docs

If `docs/src/usage/commands.md` (or wherever `/mcp` is documented) shows the `/mcp list`
output, add one line saying a failed server is now named inline. Do not add history to
CLAUDE.md — per its own rule, new history goes in ARCHITECTURE.md under the file it is
about.

## Do not

Do not reconnect anything, do not change `connections_lost_note` or the startup stderr
notes, and do not touch the model-facing `external_tool_failure_note` path. This adds a
**second audience** for a fact already recorded; it must not move the first.
