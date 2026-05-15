Title: Project-type-aware context hints — auto-inject development conventions
Files: src/context.rs, src/commands_project.rs
Issue: none

## What

When yoyo starts in a project, auto-detect the project type (Rust, Python, Node, Go, Java, Ruby, C/C++) and inject development convention hints into the project context. This makes yoyo immediately useful in any recognized project without requiring the user to write a YOYO.md file.

Currently, if there's no YOYO.md/CLAUDE.md, the agent gets file listings and git status but zero guidance about how to build, test, or lint the project. A Rust developer gets no "use `cargo test`" hint. A Python developer gets no "use `pytest`" hint. This is a real gap — Claude Code infers this kind of context automatically.

## Current State

- `detect_project_type()` in `commands_project.rs` already detects Rust, Node, Python, Go, Java, Ruby, C/C++, Make, and Unknown
- `build_commands_for_project()` maps project types to build/test commands (but only for `/init`)
- `load_project_context()` in `context.rs` loads YOYO.md/CLAUDE.md + file listing + recent changes + git status
- **No project-type hints are injected into context**

## Implementation

### In `commands_project.rs`:

1. **Add `project_type_hints(project_type: &ProjectType) -> Option<String>`** function:
   - Returns a short (3-5 line) string with development conventions for the project type
   - For Rust: build/test/lint commands, common patterns
   - For Python: pytest, pip/poetry, venv conventions
   - For Node: npm/yarn, package.json scripts
   - For Go: go build/test, module conventions
   - For Java: Maven/Gradle commands
   - For Ruby: bundle, rake
   - For C/C++: cmake, make conventions
   - For Unknown: return None
   - Keep each hint SHORT — max 5 lines. This is ambient context, not a tutorial.
   - Make the function `pub` so `context.rs` can call it.

### In `context.rs`:

2. **Modify `load_project_context()`** to detect project type and append hints:
   - Call `detect_project_type(".")` 
   - If hints exist AND no project context file (YOYO.md, CLAUDE.md, .yoyo/instructions.md) was found, append the hints as a `## Development Conventions` section
   - If a context file IS present, skip the auto-hints (the user's explicit instructions take priority)
   - Show `{DIM}  context: {project_type} conventions{RESET}` in stderr when hints are injected

### Tests to add:

In `commands_project.rs`:
- `test_project_type_hints_rust` — Rust hints mention cargo
- `test_project_type_hints_python` — Python hints mention pytest
- `test_project_type_hints_node` — Node hints mention npm/package.json
- `test_project_type_hints_unknown` — Unknown returns None
- `test_project_type_hints_all_short` — All hints are under 500 chars

In `context.rs`:
- `test_project_context_includes_conventions` — When no YOYO.md exists, conventions appear
- `test_project_context_skips_conventions_with_context_file` — When YOYO.md exists, no conventions

### What NOT to do:
- Don't make the hints long — max 5 lines each, concise
- Don't override user-provided context files — if YOYO.md exists, trust the user
- Don't try to detect specific tooling (poetry vs pip) — keep it generic
- Don't add this to the system prompt constant — inject it dynamically via project context
