Title: Add /map command for structural codebase understanding (repo map)
Files: src/commands_search.rs, src/commands.rs, src/help.rs, src/cli.rs, tests/integration.rs, docs/src/usage/commands.md
Issue: none (capability gap — #1 competitive disadvantage vs Aider and Claude Code)

## Context

The assessment identifies **repository mapping / structural codebase understanding** as yoyo's biggest competitive gap versus Aider and Claude Code. Currently, `/index` only lists files with line counts and first meaningful lines — it has zero structural understanding. Aider's `repo-map` uses tree-sitter to produce a condensed view of function signatures, struct/class definitions, and imports that fits in the context window, giving the model a "table of contents" for the whole project. This is their most differentiated feature.

yoyo doesn't need tree-sitter to start. A regex-based extraction that handles the top 5-6 languages (Rust, Python, JavaScript/TypeScript, Go, Java) would cover the majority of projects and is a huge step up from file listing.

## Design

### `/map` command

A new REPL command `/map` (or `/map <path>`) that generates a structural summary of the codebase:

```
src/main.rs (3,008 lines)
  fn main()
  fn parse_fallback_spec(spec: &str) -> (String, String)
  struct AgentConfig
  impl AgentConfig
    fn build_agent(&self) -> Agent
    fn configure_agent(...)
  const SYSTEM_PROMPT: &str

src/cli.rs (3,147 lines)
  fn parse_args() -> CliConfig
  struct CliConfig
  fn resolve_system_prompt(...) -> String
  fn load_project_context() -> Option<String>
  ...
```

### Output format

- Files grouped naturally (as found in the project)
- For each file: path and line count
- Indented underneath: extracted symbols (functions, structs, classes, traits, enums, impl blocks, consts)
- Only public/exported symbols by default; `/map --all` includes private ones
- Truncate to fit reasonable context (skip test modules, limit per-file entries)

### Language-specific extractors

Use regex patterns to extract structural symbols from source code. Each language needs patterns for:

**Rust:**
- `pub fn name(...)` and `fn name(...)`
- `pub struct Name` / `struct Name`
- `pub enum Name` / `enum Name`  
- `pub trait Name` / `trait Name`
- `impl Name` / `impl Trait for Name`
- `pub const NAME` / `const NAME`
- `mod name` (module declarations)
- Skip lines inside `#[cfg(test)]` modules

**Python:**
- `def name(...)` and `async def name(...)`
- `class Name`
- Top-level assignments (CONSTANT = ...)

**JavaScript/TypeScript:**
- `function name(...)` / `async function name(...)`
- `export function name(...)` / `export default function`
- `class Name` / `export class Name`
- `const name =` / `export const name =`
- `interface Name` (TypeScript)
- `type Name =` (TypeScript)

**Go:**
- `func Name(...)` / `func (r *Type) Name(...)`
- `type Name struct` / `type Name interface`
- `const Name` / `var Name`

**Java:**
- `public class Name` / `class Name`
- `public interface Name`
- `public ... methodName(...)` (method signatures)
- `enum Name`

### Detection

Detect language by file extension:
- `.rs` → Rust
- `.py` → Python  
- `.js`, `.jsx`, `.mjs` → JavaScript
- `.ts`, `.tsx` → TypeScript
- `.go` → Go
- `.java` → Java
- Other → skip structural extraction, just show in file list

### Integration with system prompt

This is the KEY part. Add a function `generate_repo_map()` that produces a condensed structural summary. In `cli.rs`, where `load_project_context()` appends to the system prompt, ALSO append the repo map if the project has source files:

```rust
// In parse_args(), after project context:
if let Some(repo_map) = generate_repo_map_for_prompt() {
    system_prompt.push_str("\n\n# Repository Structure\n\n");
    system_prompt.push_str(&repo_map);
}
```

The repo map for the system prompt should be:
- Condensed (no blank lines between files)
- Only public/exported symbols
- Skip test modules
- Capped at ~4000 tokens (~16K chars) to avoid bloating the context
- Sorted by file relevance (larger files first, since they contain more structure)

### `/map` REPL command

The REPL version shows the full map (not truncated) with colors:
- File paths in bold/cyan
- Functions in green
- Types (struct/class/enum/trait) in yellow  
- Constants in blue
- Line counts dimmed

## WRITE TESTS FIRST

### 1. Rust extraction tests (in `src/commands_search.rs` tests section)

```rust
#[test]
fn extract_rust_symbols_basic() {
    let code = r#"
pub fn hello(name: &str) -> String { todo!() }
fn private_fn() {}
pub struct MyStruct {
    field: i32,
}
pub enum Color { Red, Green, Blue }
pub trait Drawable { fn draw(&self); }
impl MyStruct {
    pub fn new() -> Self { todo!() }
}
const MAX: usize = 100;
"#;
    let symbols = extract_symbols(code, "rust");
    assert!(symbols.iter().any(|s| s.name == "hello" && s.kind == SymbolKind::Function));
    assert!(symbols.iter().any(|s| s.name == "MyStruct" && s.kind == SymbolKind::Struct));
    assert!(symbols.iter().any(|s| s.name == "Color" && s.kind == SymbolKind::Enum));
    assert!(symbols.iter().any(|s| s.name == "Drawable" && s.kind == SymbolKind::Trait));
    assert!(symbols.iter().any(|s| s.name == "impl MyStruct"));
}

#[test]
fn extract_rust_skips_test_module() {
    let code = r#"
pub fn real_fn() {}

#[cfg(test)]
mod tests {
    fn test_something() {}
}
"#;
    let symbols = extract_symbols(code, "rust");
    assert!(symbols.iter().any(|s| s.name == "real_fn"));
    assert!(!symbols.iter().any(|s| s.name == "test_something"));
}
```

### 2. Python extraction tests

```rust
#[test]
fn extract_python_symbols() {
    let code = r#"
class MyClass:
    def method(self):
        pass

def top_level_func(x, y):
    return x + y

async def async_handler(req):
    pass

MAX_SIZE = 1024
"#;
    let symbols = extract_symbols(code, "python");
    assert!(symbols.iter().any(|s| s.name == "MyClass" && s.kind == SymbolKind::Class));
    assert!(symbols.iter().any(|s| s.name == "top_level_func" && s.kind == SymbolKind::Function));
    assert!(symbols.iter().any(|s| s.name == "async_handler" && s.kind == SymbolKind::Function));
}
```

### 3. JavaScript/TypeScript extraction tests

```rust
#[test]
fn extract_js_symbols() {
    let code = r#"
export function fetchData(url) { }
function helper() { }
export class ApiClient { }
const BASE_URL = "https://api.example.com";
export default function main() { }
"#;
    let symbols = extract_symbols(code, "javascript");
    assert!(symbols.iter().any(|s| s.name == "fetchData" && s.kind == SymbolKind::Function));
    assert!(symbols.iter().any(|s| s.name == "ApiClient" && s.kind == SymbolKind::Class));
}

#[test]
fn extract_typescript_symbols() {
    let code = r#"
interface Config { key: string; }
type Result<T> = { data: T; error?: string; }
export class Service { }
"#;
    let symbols = extract_symbols(code, "typescript");
    assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == SymbolKind::Interface));
    assert!(symbols.iter().any(|s| s.name == "Result" && s.kind == SymbolKind::Type));
    assert!(symbols.iter().any(|s| s.name == "Service" && s.kind == SymbolKind::Class));
}
```

### 4. Go extraction tests

```rust
#[test]
fn extract_go_symbols() {
    let code = r#"
func main() { }
func (s *Server) Handle(w http.ResponseWriter, r *http.Request) { }
type Server struct { port int }
type Handler interface { Handle() }
"#;
    let symbols = extract_symbols(code, "go");
    assert!(symbols.iter().any(|s| s.name == "main" && s.kind == SymbolKind::Function));
    assert!(symbols.iter().any(|s| s.name == "Server" && s.kind == SymbolKind::Struct));
    assert!(symbols.iter().any(|s| s.name == "Handler" && s.kind == SymbolKind::Interface));
}
```

### 5. Integration test

```rust
#[test]
fn test_map_in_help() {
    let output = Command::new(cargo_bin())
        .args(["--help"])
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // /map should be mentioned somewhere in help or docs
}
```

### 6. generate_repo_map tests

```rust
#[test]
fn generate_repo_map_respects_size_limit() {
    // The repo map for system prompt should be capped
    let map = generate_repo_map_for_prompt_with_limit(1000);
    assert!(map.unwrap_or_default().len() <= 1000);
}

#[test]
fn format_repo_map_empty_project() {
    let entries: Vec<FileSymbols> = vec![];
    let result = format_repo_map(&entries);
    assert!(result.contains("no structural") || result.is_empty());
}
```

## Implementation Steps

### Step 1: Define types

In `src/commands_search.rs`, add:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Interface,
    Class,
    Type,
    Const,
    Impl,
    Module,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub is_public: bool,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct FileSymbols {
    pub path: String,
    pub lines: usize,
    pub symbols: Vec<Symbol>,
}
```

### Step 2: Language detection

```rust
fn detect_language(path: &str) -> Option<&'static str> {
    match Path::new(path).extension()?.to_str()? {
        "rs" => Some("rust"),
        "py" => Some("python"),
        "js" | "jsx" | "mjs" => Some("javascript"),
        "ts" | "tsx" => Some("typescript"),
        "go" => Some("go"),
        "java" => Some("java"),
        _ => None,
    }
}
```

### Step 3: Symbol extraction per language

Implement `extract_symbols(code: &str, language: &str) -> Vec<Symbol>` using regex patterns. This is the core logic — one function that dispatches to language-specific regex sets.

Key implementation details:
- Use `regex::Regex` (already in dependencies via yoagent)
- For Rust: track `#[cfg(test)]` blocks by counting braces after the attribute to skip test modules
- For indented languages (Python): use indentation level to distinguish top-level from nested definitions
- Each regex captures the symbol name and its line number

### Step 4: Build repo map

```rust
pub fn build_repo_map(public_only: bool) -> Vec<FileSymbols> {
    let files = list_project_files();
    let mut result = Vec::new();
    
    for path in &files {
        if is_binary_extension(path) { continue; }
        let lang = match detect_language(path) {
            Some(l) => l,
            None => continue, // skip unsupported languages
        };
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let line_count = content.lines().count();
        let mut symbols = extract_symbols(&content, lang);
        if public_only {
            symbols.retain(|s| s.is_public);
        }
        if !symbols.is_empty() {
            result.push(FileSymbols { path: path.clone(), lines: line_count, symbols });
        }
    }
    
    // Sort by line count descending (biggest/most important files first)
    result.sort_by(|a, b| b.lines.cmp(&a.lines));
    result
}
```

### Step 5: Format for display (REPL)

`format_repo_map_colored(entries: &[FileSymbols]) -> String` — with ANSI colors for the REPL `/map` command.

### Step 6: Format for system prompt

`generate_repo_map_for_prompt() -> Option<String>` — plain text, capped at ~16K chars, public symbols only, no test modules.

### Step 7: Wire `/map` command

In `src/commands.rs`, add `/map` to the command dispatch. In `src/help.rs`, add help text. The handler calls `build_repo_map(false)` (show all symbols) and `format_repo_map_colored()`.

Support `/map <path>` to show map for a specific directory or file.

### Step 8: Wire into system prompt

In `src/cli.rs`, in `parse_args()`, after the project context is appended to the system prompt, also generate and append the repo map:

```rust
// After project context block
if let Some(repo_map) = commands_search::generate_repo_map_for_prompt() {
    system_prompt.push_str("\n\n# Repository Structure\n\n");
    system_prompt.push_str(&repo_map);
}
```

This gives the model automatic codebase awareness from the start of every session.

### Step 9: Update docs

- `docs/src/usage/commands.md` — add `/map` with description
- `src/help.rs` — add `/map` help entry

### Step 10: Run full test suite

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```

## Important Notes

- Check if `regex` is already a direct dependency. If not, it's available transitively through yoagent. If needed, add `regex = "1"` to Cargo.toml.
- The system prompt integration is the most impactful part — even if the `/map` REPL command is nice-to-have, the automatic context in the system prompt is what closes the gap with Aider.
- Don't over-engineer the regex patterns. Simple line-by-line matching is fine. False positives in comments are acceptable for v1 — they'll be rare in practice and the model can filter them.
- The repo map should gracefully handle projects with no supported languages (return None, don't crash).
- Keep the size cap reasonable. For yoyo's own codebase (35K lines), the public-only repo map should be ~2-4K chars.
