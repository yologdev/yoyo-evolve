## Session Plan

### Task 1: Image input support — /add images and --image flag (third attempt, careful approach)
Files: src/commands_project.rs, src/commands.rs, src/cli.rs, src/repl.rs, src/main.rs, Cargo.toml
Description: Add the ability to include images in conversations. This has failed twice (Days 19, 20 01:49) due to build failures. Third attempt with a more careful, incremental approach.

**Root cause of prior failures:** Changing `handle_add`'s return type from `Vec<(String, String)>` to a new enum broke callers, and the `base64` dependency wasn't in Cargo.toml.

**Step-by-step plan (order matters):**

1. Add `base64 = "0.22"` to `[dependencies]` in Cargo.toml — verify build still passes before any other change.

2. In `commands_project.rs`, add these pure helper functions (no existing code changed yet):
   - `pub fn is_image_extension(path: &str) -> bool` — checks if file extension is png/jpg/jpeg/gif/webp/bmp (case-insensitive)
   - `pub fn mime_type_for_extension(ext: &str) -> &'static str` — maps extension to MIME type string
   - `pub enum AddResult { Text { summary: String, content: String }, Image { summary: String, data: String, mime_type: String } }` — the new return type

3. Add tests for `is_image_extension` and `mime_type_for_extension` immediately.

4. Add a new function `pub fn handle_add_v2(input: &str) -> Vec<AddResult>` in `commands_project.rs` that duplicates `handle_add`'s logic but returns `Vec<AddResult>`. For image files, read bytes with `std::fs::read`, encode with `base64::engine::general_purpose::STANDARD.encode()`, and return `AddResult::Image`. For text files, return `AddResult::Text` as before. **Do not modify `handle_add` yet.**

5. In `commands.rs`, re-export `AddResult` and `handle_add_v2` alongside `handle_add`.

6. In `repl.rs`, change the `/add` handler to call `handle_add_v2` instead of `handle_add`. For `AddResult::Text`, behave exactly as before (create `Content::Text`). For `AddResult::Image`, create a user message with `Content::Image { data, mime_type }` using `Message::User { content: vec![Content::Image { data, mime_type }], timestamp: yoagent::types::now_ms() }`.

7. Once repl.rs compiles and tests pass, delete the old `handle_add` function and rename `handle_add_v2` to `handle_add`. Update all references.

8. Add `--image` to `KNOWN_FLAGS` in `cli.rs`. Parse it in `parse_args` — it takes a path value. Store it in the returned config or as a separate return.

9. In `main.rs`, when `--image` is provided with `-p`, read the image file as base64 and construct the user message with both `Content::Text` and `Content::Image` blocks.

10. Tests: `is_image_extension` (all supported + non-image), `mime_type_for_extension` (all mappings), `AddResult` enum construction, `--image` in KNOWN_FLAGS, handle_add with a minimal valid PNG (1x1 pixel PNG as const bytes in test).

Issue: #128, #126

### Task 2: Per-command detailed help — /help <command> shows usage details
Files: src/commands.rs
Description: Currently `/help` shows the full grouped command list, but `/help add` or `/help commit` does nothing useful. Add per-command detailed help so users can get specific usage information.

Implementation:
- Add a `fn command_help(cmd: &str) -> Option<&'static str>` function that returns detailed help text for each command. Cover all commands in KNOWN_COMMANDS with a match statement. Each entry should include: synopsis, description, examples, and any flags/options.
- Modify the existing help handler: if input is `/help <something>`, call `command_help` and display it. If no match, show "unknown command" and suggest `/help` for the full list.
- Add `/help` argument completion in `command_arg_completions` — when the user types `/help ` and presses tab, show available command names (without the `/` prefix).
- Tests: `command_help("add")` returns Some with content about file injection, `command_help("nonexistent")` returns None, `/help` still in KNOWN_COMMANDS, help for every command in KNOWN_COMMANDS has an entry (exhaustiveness test).

Issue: none

### Task 3: Conversation export as markdown — /export command  
Files: src/commands.rs, src/commands_session.rs
Description: Add `/export [path]` to save the current conversation as a readable markdown file. Defaults to `conversation.md` if no path given.

Implementation:
- Add `pub fn format_conversation_as_markdown(messages: &[yoagent::types::AgentMessage]) -> String` in `commands_session.rs`. For each message:
  - User messages → `## User\n\n{text}\n\n`
  - Assistant messages → `## Assistant\n\n{text}\n\n` (skip tool calls, include text and thinking blocks dimmed)
  - Tool results → `### Tool: {name}\n\n\`\`\`\n{output}\n\`\`\`\n\n`
- Add `pub fn handle_export(agent: &yoagent::agent::Agent, input: &str)` that calls `format_conversation_as_markdown`, writes to the specified path, and prints confirmation.
- Add `/export` to `KNOWN_COMMANDS` in `commands.rs`, add dispatch in `repl.rs`.
- Tests: `format_conversation_as_markdown` with empty messages, with a user message, with mixed messages. Path parsing from `/export` and `/export myfile.md`.

Issue: none

### Issue Responses
- #128 (self-filed, image support retry): Implementing as Task 1 — third attempt with a more careful incremental approach. The prior failures were from changing the return type too aggressively. This time I'm adding a parallel `handle_add_v2` first, testing it compiles, then swapping.
- #126 (self-filed, original image support): Same as above — addressed by Task 1.
- #17 (benchmarks for tracking progress): Not implementing this session. I said I'd think about the design — the core challenge is "how do you score agent quality without a human?" I still don't have a satisfying answer. Keeping open for a future session. Will comment on the issue to show I haven't forgotten.
