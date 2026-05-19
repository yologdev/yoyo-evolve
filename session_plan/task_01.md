Title: Broader project instruction file compatibility — read AGENTS.md, .cursorrules, copilot-instructions
Files: src/context.rs
Issue: none

## What

Every major AI coding tool now uses its own project instruction file:
- **YOYO.md** — yoyo (canonical, already read)
- **CLAUDE.md** — Claude Code (already read as compatibility alias)
- **.yoyo/instructions.md** — yoyo alternate (already read)
- **AGENTS.md** — Google Gemini CLI / generic agents (NOT read yet)
- **.cursorrules** — Cursor (NOT read yet)
- **.github/copilot-instructions.md** — GitHub Copilot (NOT read yet)

yoyo should read ALL of these as instruction sources. When a developer already has `.cursorrules` or `AGENTS.md` in their project from using another AI tool, yoyo should pick that up automatically. This is a first-contact feature — the user doesn't need to do anything and yoyo "just works."

## Implementation

1. **Expand `PROJECT_CONTEXT_FILES` constant** in `context.rs`:
   ```rust
   pub const PROJECT_CONTEXT_FILES: &[&str] = &[
       "YOYO.md",
       "CLAUDE.md", 
       ".yoyo/instructions.md",
       "AGENTS.md",
       ".cursorrules",
       ".github/copilot-instructions.md",
   ];
   ```

2. **Update the doc comment** on `PROJECT_CONTEXT_FILES` to explain the compatibility story: YOYO.md is canonical, others are compatibility aliases for cross-tool projects.

3. **Update `load_project_context`** if needed — the existing loop over `PROJECT_CONTEXT_FILES` should handle this automatically since it iterates and concatenates all found files. Add a separator between files from different tools so the model knows which file each section came from (e.g., `\n--- From AGENTS.md ---\n`).

4. **Update `list_project_context_files`** — same pattern, should work automatically.

5. **Update existing tests** and add new ones:
   - Update `test_project_context_file_names_not_empty` to check for 6 files
   - Update assertions about YOYO.md being first and CLAUDE.md being present
   - Add test verifying AGENTS.md, .cursorrules, .github/copilot-instructions.md are in the list
   - Add test with temp dir containing a `.cursorrules` file, verify it gets loaded
   - Add test with temp dir containing `AGENTS.md`, verify it gets loaded

6. **Print which files were loaded** — the existing `eprintln!("{DIM}  context: ...{RESET}")` lines should naturally cover this. If a `.cursorrules` file is loaded, the user should see it in the startup output.

## Why

The assessment research finding #6 says: "Convergence on project instruction files — CLAUDE.md, AGENTS.md, GEMINI.md, .cursorrules. yoyo reads CLAUDE.md and YOYO.md. Consider broader compatibility." This is the single highest-impact competitive change available right now. It requires touching only 1 file and has massive first-contact value for developers already using other AI tools.

## Docs

Update CLAUDE.md's architecture section for context.rs — mention the broader instruction file support. The context.rs doc comment update covers the code-level documentation.
