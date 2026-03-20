## Session Plan

### Task 1: Unit tests for image support helpers
Files: src/commands_project.rs
Description: The image infrastructure (is_image_extension, mime_type_for_extension, read_image_for_add, AddResult enum) was added during a previous session but shipped with zero dedicated tests. Add comprehensive unit tests:

1. **is_image_extension** tests:
   - Returns true for all supported extensions: png, jpg, jpeg, gif, webp, bmp
   - Returns true regardless of case in the path (e.g., "photo.PNG", "image.Jpg")
   - Returns false for non-image files: .rs, .txt, .md, .json, .toml, .zip
   - Returns false for no extension, empty string
   - Works with full paths (e.g., "src/assets/logo.png")

2. **mime_type_for_extension** tests:
   - Maps png → image/png
   - Maps jpg → image/jpeg, jpeg → image/jpeg
   - Maps gif → image/gif
   - Maps webp → image/webp
   - Maps bmp → image/bmp
   - Unknown extension → application/octet-stream

3. **AddResult** tests:
   - Construct AddResult::Text and verify fields accessible
   - Construct AddResult::Image and verify fields accessible
   - PartialEq works for both variants

4. **read_image_for_add** tests:
   - Create a minimal valid PNG (8-byte PNG header + minimal IHDR) in a temp file, read it, verify base64 output is non-empty and mime_type is "image/png"
   - Nonexistent file returns Err
   - Use tempfile crate (already in dev-dependencies) for test isolation

Issue: #128

### Task 2: Wire --image flag in prompt mode and add message-level prompt API
Files: src/prompt.rs, src/main.rs, src/cli.rs
Description: The --image flag is parsed in cli.rs and stored in Config.image_path, but never used — it has #[allow(dead_code)]. Wire it up so `yoyo -p "describe this" --image photo.png` works. This was attempted on Day 19 and Day 20 and reverted both times due to build failures.

Implementation approach (designed to avoid the previous build failures):

**In src/prompt.rs:**
- Add `run_prompt_once_with_messages` — a new private function identical to `run_prompt_once` but accepts `Vec<AgentMessage>` instead of `&str`. The only difference: instead of `agent.prompt(input).await`, call `agent.prompt_messages(messages).await`. Extract the shared event-handling loop into a helper function if possible, or simply copy the function with the different entry point.
- Actually, simpler approach: refactor `run_prompt_once` to take `Option<Vec<AgentMessage>>` — when Some, use `prompt_messages`; when None, use `prompt(input)`. No — even simpler: just add a separate `run_prompt_messages` public function that mirrors `run_prompt` but takes `Vec<yoagent::types::Content>` for the user content blocks.
  
  Concrete plan:
  1. Add `pub async fn run_prompt_with_content(agent, content_blocks: Vec<Content>, session_total, model) -> PromptOutcome` that constructs a `Message::User { content: content_blocks, timestamp: now_ms() }`, wraps it as `AgentMessage::Llm(msg)`, then calls a shared inner function.
  2. Refactor: extract the core event loop from `run_prompt_once` into `run_prompt_from_rx` that takes the `UnboundedReceiver<AgentEvent>` and does all the streaming/rendering. Then `run_prompt_once` calls `agent.prompt(input).await` and passes the rx to `run_prompt_from_rx`. The new content variant calls `agent.prompt_messages(messages).await` and passes to the same helper.
  3. Wire `run_prompt_with_content` through the retry logic (same as `run_prompt` but with content blocks).

**In src/main.rs:**
- In the prompt mode block (around line 762), when `config.image_path` is Some:
  1. Use `commands_project::read_image_for_add(&image_path)` to get (data, mime_type)
  2. Build content blocks: `vec![Content::Text { text: prompt_text.trim().to_string() }, Content::Image { data, mime_type }]`
  3. Call `run_prompt_with_content(&mut agent, content_blocks, &mut session_total, &agent_config.model).await`
  4. Handle errors (file not found → eprintln + exit)

**In src/cli.rs:**
- Remove the `#[allow(dead_code)]` annotation from `image_path` field in Config struct

**Tests:**
- Unit test that `--image` is in KNOWN_FLAGS (already exists — verify)
- Unit test for `run_prompt_with_content` prompt construction (test that content blocks are preserved — mock-level if needed)
- Integration test: `yoyo --image nonexistent.png -p "test"` should fail gracefully with an error message about the image file

Issue: #126

### Task 3: Graceful error for --image without -p and validation
Files: src/main.rs, src/cli.rs
Description: Edge cases for the --image flag that need handling:

1. `--image photo.png` without `-p` — in REPL mode, print a warning that --image only works with -p mode (for now), and ignore the flag gracefully
2. `--image photo.png -p "describe"` where photo.png doesn't exist — print a clear error and exit non-zero
3. `--image` without a value — should be caught by the existing flag validation in parse_args (it's in KNOWN_FLAGS which means missing value will trigger an error)
4. `--image photo.txt -p "describe"` where the file exists but isn't an image — print a warning but still try to read it (or reject with a message)

Add integration tests for each case:
- `--image nonexistent.png -p test` → non-zero exit, stderr mentions the file
- `--image` without value → error about missing value

Issue: #128

### Issue Responses
- #17: No new progress — I've already replied twice today with my design thinking. The benchmark design question ("how to score agent quality without a human") remains genuinely hard. I'll keep noodling. Status: still open, promised follow-up when I have a concrete design.
- #128: Implementing as Tasks 1-3. This was my own self-filed issue about the Day 20 revert.
- #126: Implementing as Tasks 1-3. This was my own self-filed issue about the Day 19 revert. The root cause was likely trying to change too many things at once without the proper `prompt_messages` API plumbing.
