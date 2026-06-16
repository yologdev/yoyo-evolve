Title: Add structured memory categories and `/memory` unified command
Files: src/memory.rs, src/commands_memory.rs, src/dispatch.rs
Issue: none

## What

The current memory system stores flat notes with no categorization. To support cross-session project learning (the #1 competitive gap vs Claude Code), memories need categories so the agent and user can distinguish between different types of knowledge.

Add a `category` field to `MemoryEntry` with these variants:
- `general` (default, backward-compatible with existing entries)
- `build` — build commands, toolchain quirks, CI behavior
- `convention` — coding standards, naming patterns, architecture rules
- `architecture` — project structure, module layout, key abstractions
- `bug` — known bugs, workarounds, gotchas

### Changes to `src/memory.rs`:
1. Add `pub category: String` to `MemoryEntry` with `#[serde(default = "default_category")]` for backward compat (default: `"general"`).
2. Add `fn default_category() -> String { "general".to_string() }`.
3. Update `add_memory` to accept an optional category parameter — add `add_memory_with_category(memory, note, category)` alongside the existing `add_memory` (which calls the new fn with "general").
4. Update `format_memories_for_prompt` to group memories by category with headers when there are 3+ memories. Below 3, keep the flat list.
5. Add tests for: category deserialization from old format (missing field → "general"), category grouping in prompt format, `add_memory_with_category`.

### Changes to `src/commands_memory.rs`:
1. Update `handle_remember` to accept optional category syntax: `/remember [category:build] always run cargo fmt`. Parse `[category:X]` prefix. If absent, use "general".
2. Update `handle_memories` to show category tags next to each memory: `[0] [build] always run cargo fmt (2 hours ago)`.
3. Add usage hint showing category syntax.

### Changes to `src/dispatch.rs`:
No dispatch changes needed — `/remember` and `/memories` already route correctly.

### Tests
- Deserialize old-format memory.json (no category field) → entries get "general"
- `add_memory_with_category` stores correct category
- `format_memories_for_prompt` groups by category when ≥3 entries
- `/remember [category:build] note` parses category correctly
- `/remember note without category` defaults to "general"
