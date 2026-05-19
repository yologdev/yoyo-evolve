Title: Smart /init — detect existing AI tool instruction files and note them
Files: src/commands_project.rs
Issue: none

## What

When `/init` generates a `YOYO.md` file, it should detect if the project already has instruction files from other AI tools (`.cursorrules`, `AGENTS.md`, `.github/copilot-instructions.md`, `CLAUDE.md`) and:

1. **Print a notice** during generation: "Found existing AI tool configs: .cursorrules, AGENTS.md — yoyo will read these automatically"
2. **Add a section** to the generated `YOYO.md` noting which other instruction files exist, so the user knows yoyo is aware of them
3. **Avoid duplicating content** — don't copy content from other files into YOYO.md, just acknowledge their existence

## Implementation

### In `generate_init_content` (or nearby helper):

1. After scanning the project, check for the existence of these files relative to project root:
   - `CLAUDE.md`
   - `AGENTS.md`  
   - `.cursorrules`
   - `.github/copilot-instructions.md`

2. If any exist, add a section to the generated YOYO.md:
   ```markdown
   ## Other AI Tool Configs
   
   This project also has instruction files for other AI tools:
   - `.cursorrules` (Cursor)
   - `AGENTS.md` (Gemini / generic agents)
   
   yoyo reads these automatically for additional project context.
   ```

3. Print to stderr during `/init`:
   ```
   Found existing AI configs: .cursorrules, AGENTS.md — yoyo reads these automatically
   ```

### In `handle_init`:

No changes needed if `generate_init_content` handles the detection. Just ensure the function has access to the project root path (it should already, since it scans for Cargo.toml etc.).

### Tests

Add tests:
- `generate_init_content` in a temp dir with a `.cursorrules` file → output contains "Other AI Tool Configs" section
- `generate_init_content` in a temp dir with no other AI files → output does NOT contain "Other AI Tool Configs" section  
- `generate_init_content` in a temp dir with both `AGENTS.md` and `.cursorrules` → output lists both
- Test that `CLAUDE.md` is listed with label "(Claude Code)" and `AGENTS.md` with "(Gemini / generic agents)"

## Why

This completes the instruction file compatibility story started in Task 1. Task 1 makes yoyo *read* other tools' instruction files; this task makes yoyo *acknowledge* them visibly. Together, they create a seamless cross-tool experience where developers feel yoyo respects their existing setup.

First-contact impact (Day 64 learning): when a developer runs `/init` and sees "Found existing AI configs: .cursorrules — yoyo reads these automatically," it immediately signals that yoyo is a team player, not a replacement that ignores existing tooling.
