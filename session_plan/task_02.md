Title: Add Lua and Zig language support to /map symbol extraction
Files: src/commands_map.rs
Issue: none

## What

yoyo's `/map` command currently supports 15 languages for symbol extraction. Aider supports 100+ via tree-sitter. Every language we add closes that gap. This task adds **Lua** and **Zig** — two popular languages not yet covered.

- **Lua** — Top-20 language by TIOBE, heavily used in game development (Love2D, Roblox), embedded scripting (Neovim, Redis), and config (Nginx).
- **Zig** — Rising systems language, growing community, used alongside C/C++ projects. Increasingly seen in open-source.

## Implementation

Both should use the **pattern-based extraction** approach (like Go, Ruby, Shell, C#, PHP, Kotlin) rather than dedicated `extract_*_symbols` functions — this is lighter and easier to maintain.

### Lua patterns

Lua symbols to detect:
- `function name(...)` — top-level functions
- `local function name(...)` — local functions  
- `function M.name(...)` or `function M:name(...)` — module methods
- `M.name = function(...)` — assigned functions
- No struct/class keywords, but detect common OOP patterns if straightforward

Create `LUA_PATTERNS` using `LanguagePatterns` struct with appropriate regex patterns. Key consideration: Lua uses `--` for line comments and `--[[ ]]` for block comments.

### Zig patterns

Zig symbols to detect:
- `pub fn name(...)` — public functions
- `fn name(...)` — private functions  
- `pub const Name = struct { ... }` — struct definitions
- `pub const Name = enum { ... }` — enum definitions
- `pub const Name = union { ... }` — union definitions
- `test "name"` — test blocks (skip these, similar to how Rust skips `#[cfg(test)]`)
- `const Name = struct { ... }` — private struct definitions

Create `ZIG_PATTERNS` using `LanguagePatterns` struct.

### detect_language updates

Add file extension mappings:
- `.lua` → `"lua"`
- `.zig` → `"zig"`

### extract_symbols updates

Add match arms:
```rust
"lua" => extract_symbols_from_patterns(code, &LUA_PATTERNS),
"zig" => extract_symbols_from_patterns(code, &ZIG_PATTERNS),
```

### Tests

Add comprehensive tests for both languages:

**Lua tests:**
- Basic function extraction (`function foo()`, `local function bar()`)
- Module method extraction (`function M.method()`, `function M:method()`)
- Skip comment lines
- `detect_language("script.lua")` returns `Some("lua")`

**Zig tests:**
- Public and private function extraction
- Struct, enum, union detection
- Skip test blocks
- `detect_language("main.zig")` returns `Some("zig")`

**Regression test:**
- Existing languages still work (spot-check Rust and Python extraction)

## Why

Closing the language gap against Aider (100+ languages). Lua and Zig are both popular, distinct from existing languages, and straightforward to add via the pattern-based approach. Each new language makes `/map` useful in more real projects.
