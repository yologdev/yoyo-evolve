Title: Add ast-grep integration as an optional structural search tool
Files: src/commands_project.rs, src/commands.rs, src/help.rs
Issue: #133

## Context

Issue #133 asks for language-specific high-level refactoring tools. @yuanhao specifically suggested [ast-grep](https://github.com/ast-grep/ast-grep) as a polyglot structural code search/rewrite engine. A previous attempt (Issue #170) to add this was reverted due to build failures — it was too ambitious (both REPL command AND agent tool, plus /doctor integration, all at once).

This time: **minimal viable integration** — just a `/ast` REPL command that shells out to `sg` (ast-grep's binary). No agent tool, no /doctor integration. Keep it simple so it builds.

We already have `/refactor` (umbrella), `/rename` (word-boundary rename), `/extract` (move code between files), `/move` (similar). Adding `/ast` gives users structural search when `sg` is installed, and gracefully degrades when it's not.

## Implementation

### 1. Detection function in `src/commands_project.rs`

```rust
/// Check if ast-grep's `sg` binary is available on PATH.
pub fn is_ast_grep_available() -> bool {
    std::process::Command::new("sg")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
```

### 2. Search function in `src/commands_project.rs`

```rust
/// Run ast-grep structural search.
/// Returns Ok(output) or Err(error message).
pub fn run_ast_grep_search(pattern: &str, lang: Option<&str>, path: Option<&str>) -> Result<String, String> {
    if !is_ast_grep_available() {
        return Err("ast-grep (sg) is not installed. Install from: https://ast-grep.github.io/".into());
    }
    let mut cmd = std::process::Command::new("sg");
    cmd.arg("--pattern").arg(pattern);
    if let Some(l) = lang { cmd.arg("--lang").arg(l); }
    if let Some(p) = path { cmd.arg(p); }
    match cmd.output() {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            if stdout.trim().is_empty() {
                Ok("No matches found.".into())
            } else {
                Ok(stdout)
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            Err(format!("ast-grep error: {}", stderr.trim()))
        }
        Err(e) => Err(format!("Failed to run sg: {e}")),
    }
}
```

### 3. REPL command handler `handle_ast_grep` in `src/commands_project.rs`

Parse `/ast <pattern> [--lang <lang>] [--in <path>]`:
- Extract the pattern (required, first non-flag argument)
- Parse `--lang <value>` and `--in <value>` flags
- Call `run_ast_grep_search` and display results
- If sg is not installed, show a helpful message with install link

### 4. Wire into commands

In `src/commands.rs`:
- Add `/ast` to `KNOWN_COMMANDS`
- Add arg completions for `/ast`: suggest `--lang`, `--in` as completions

In `src/help.rs`:
- Add help entry for `/ast`:
  ```
  /ast <pattern> [--lang <lang>] [--in <path>] — Structural code search using ast-grep
  
  Searches for AST patterns using ast-grep (sg). Requires `sg` to be installed.
  Pattern syntax: use $VAR for wildcards. E.g. $X.unwrap() matches any .unwrap() call.
  
  Install: https://ast-grep.github.io/
  
  Examples:
    /ast $X.unwrap()
    /ast $X.unwrap() --lang rust
    /ast "fn $NAME($$$ARGS)" --lang rust --in src/
  ```

### 5. Wire into REPL dispatch in `src/repl.rs`

Add the `/ast` command dispatch alongside other commands.

### 6. Tests

- `test_is_ast_grep_available_no_panic` — function runs without panicking even when sg isn't installed
- `test_ast_grep_search_no_sg` — returns appropriate error when sg not available
- `test_ast_in_known_commands` — /ast is in KNOWN_COMMANDS
- `test_ast_in_help_text` — /ast appears in help output
- `test_parse_ast_grep_args` — parse pattern, --lang, --in from input string

**Important:** Do NOT add an agent tool for this yet. Keep it as a REPL command only. The agent tool integration can come later once the basic command works reliably.
