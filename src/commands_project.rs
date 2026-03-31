//! Project-related command handlers: /todo, /context, /init, /docs, /plan.

use crate::cli;
use crate::commands::auto_compact_if_needed;
use crate::docs;
use crate::format::*;
use crate::prompt::*;

// Re-export refactoring commands for backward compatibility
pub use crate::commands_refactor::{
    handle_extract, handle_move, handle_refactor, handle_rename, rename_in_project,
};

use std::sync::RwLock;

use yoagent::agent::Agent;
use yoagent::*;

// ── /todo ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Done,
}

impl std::fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TodoStatus::Pending => write!(f, "[ ]"),
            TodoStatus::InProgress => write!(f, "[~]"),
            TodoStatus::Done => write!(f, "[✓]"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TodoItem {
    pub id: usize,
    pub description: String,
    pub status: TodoStatus,
}

static TODO_LIST: RwLock<Vec<TodoItem>> = RwLock::new(Vec::new());
static TODO_NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

/// Add a todo item, return its ID.
pub fn todo_add(description: &str) -> usize {
    let id = TODO_NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let item = TodoItem {
        id,
        description: description.to_string(),
        status: TodoStatus::Pending,
    };
    TODO_LIST.write().unwrap().push(item);
    id
}

/// Update the status of a todo item by ID.
pub fn todo_update(id: usize, status: TodoStatus) -> Result<(), String> {
    let mut list = TODO_LIST.write().unwrap();
    match list.iter_mut().find(|item| item.id == id) {
        Some(item) => {
            item.status = status;
            Ok(())
        }
        None => Err(format!("No todo item with ID {id}")),
    }
}

/// Return a snapshot of all todo items.
pub fn todo_list() -> Vec<TodoItem> {
    TODO_LIST.read().unwrap().clone()
}

/// Clear all todo items and reset the ID counter.
pub fn todo_clear() {
    TODO_LIST.write().unwrap().clear();
    TODO_NEXT_ID.store(1, std::sync::atomic::Ordering::SeqCst);
}

/// Remove a single todo item by ID.
pub fn todo_remove(id: usize) -> Result<TodoItem, String> {
    let mut list = TODO_LIST.write().unwrap();
    let pos = list
        .iter()
        .position(|item| item.id == id)
        .ok_or_else(|| format!("No todo item with ID {id}"))?;
    Ok(list.remove(pos))
}

/// Format the todo list with status checkboxes.
pub fn format_todo_list(items: &[TodoItem]) -> String {
    if items.is_empty() {
        return "  No tasks. Use /todo add <description> to add one.".to_string();
    }
    let mut out = String::new();
    for item in items {
        out.push_str(&format!(
            "  {} #{} {}\n",
            item.status, item.id, item.description
        ));
    }
    // Remove trailing newline
    if out.ends_with('\n') {
        out.truncate(out.len() - 1);
    }
    out
}

/// Handle the /todo command and its subcommands. Returns a string to print.
pub fn handle_todo(input: &str) -> String {
    let arg = input.strip_prefix("/todo").unwrap_or("").trim();

    if arg.is_empty() {
        // Show all tasks
        let items = todo_list();
        return format_todo_list(&items);
    }

    if arg == "clear" {
        todo_clear();
        return format!("{GREEN}  ✓ Cleared all tasks{RESET}");
    }

    if let Some(desc) = arg.strip_prefix("add ") {
        let desc = desc.trim();
        if desc.is_empty() {
            return "  Usage: /todo add <description>".to_string();
        }
        let id = todo_add(desc);
        return format!("{GREEN}  ✓ Added task #{id}: {desc}{RESET}");
    }
    if arg == "add" {
        return "  Usage: /todo add <description>".to_string();
    }

    if let Some(id_str) = arg.strip_prefix("done ") {
        let id_str = id_str.trim();
        match id_str.parse::<usize>() {
            Ok(id) => match todo_update(id, TodoStatus::Done) {
                Ok(()) => return format!("{GREEN}  ✓ Marked #{id} as done{RESET}"),
                Err(e) => return format!("{RED}  {e}{RESET}"),
            },
            Err(_) => return format!("{RED}  Invalid ID: {id_str}{RESET}"),
        }
    }

    if let Some(id_str) = arg.strip_prefix("wip ") {
        let id_str = id_str.trim();
        match id_str.parse::<usize>() {
            Ok(id) => match todo_update(id, TodoStatus::InProgress) {
                Ok(()) => return format!("{GREEN}  ✓ Marked #{id} as in-progress{RESET}"),
                Err(e) => return format!("{RED}  {e}{RESET}"),
            },
            Err(_) => return format!("{RED}  Invalid ID: {id_str}{RESET}"),
        }
    }

    if let Some(id_str) = arg.strip_prefix("remove ") {
        let id_str = id_str.trim();
        match id_str.parse::<usize>() {
            Ok(id) => match todo_remove(id) {
                Ok(item) => {
                    return format!("{GREEN}  ✓ Removed #{id}: {}{RESET}", item.description)
                }
                Err(e) => return format!("{RED}  {e}{RESET}"),
            },
            Err(_) => return format!("{RED}  Invalid ID: {id_str}{RESET}"),
        }
    }

    // Unknown subcommand — show usage
    "  Usage:\n\
     \x20 /todo                    Show all tasks\n\
     \x20 /todo add <description>  Add a new task\n\
     \x20 /todo done <id>          Mark task as done\n\
     \x20 /todo wip <id>           Mark as in-progress\n\
     \x20 /todo remove <id>        Remove a task\n\
     \x20 /todo clear              Clear all tasks"
        .to_string()
}

// ── /context ─────────────────────────────────────────────────────────────

pub fn handle_context() {
    let files = cli::list_project_context_files();
    if files.is_empty() {
        println!("{DIM}  No project context files found.");
        println!("  Create a YOYO.md to give yoyo project context.");
        println!("  Also supports: CLAUDE.md (compatibility alias), .yoyo/instructions.md");
        println!("  Run /init to create a starter YOYO.md.{RESET}\n");
    } else {
        println!("{DIM}  Project context files:");
        for (name, lines) in &files {
            let word = crate::format::pluralize(*lines, "line", "lines");
            println!("    {name} ({lines} {word})");
        }
        println!("{RESET}");
    }
}

// ── /init ────────────────────────────────────────────────────────────────

/// Scan the project directory and find important files (README, config, CI, etc.).
/// Returns a list of file paths that exist.
pub fn scan_important_files(dir: &std::path::Path) -> Vec<String> {
    let candidates = [
        "README.md",
        "README",
        "readme.md",
        "LICENSE",
        "LICENSE.md",
        "CHANGELOG.md",
        "CONTRIBUTING.md",
        ".gitignore",
        ".editorconfig",
        // Rust
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        // Node
        "package.json",
        "package-lock.json",
        "tsconfig.json",
        ".eslintrc.json",
        ".eslintrc.js",
        ".prettierrc",
        // Python
        "pyproject.toml",
        "setup.py",
        "setup.cfg",
        "requirements.txt",
        "Pipfile",
        "tox.ini",
        // Go
        "go.mod",
        "go.sum",
        // Build/CI
        "Makefile",
        "Dockerfile",
        "docker-compose.yml",
        "docker-compose.yaml",
        ".dockerignore",
        // CI configs
        ".github/workflows",
        ".gitlab-ci.yml",
        ".circleci/config.yml",
        ".travis.yml",
        "Jenkinsfile",
    ];
    candidates
        .iter()
        .filter(|f| dir.join(f).exists())
        .map(|f| f.to_string())
        .collect()
}

/// Detect key directories in the project (src, tests, docs, etc.).
/// Returns a list of directory names that exist.
pub fn scan_important_dirs(dir: &std::path::Path) -> Vec<String> {
    let candidates = [
        "src",
        "lib",
        "tests",
        "test",
        "docs",
        "doc",
        "examples",
        "benches",
        "scripts",
        ".github",
        ".vscode",
        "config",
        "public",
        "static",
        "assets",
        "migrations",
    ];
    candidates
        .iter()
        .filter(|d| dir.join(d).is_dir())
        .map(|d| d.to_string())
        .collect()
}

/// Get build/test/lint commands for a project type.
pub fn build_commands_for_project(project_type: &ProjectType) -> Vec<(&'static str, &'static str)> {
    match project_type {
        ProjectType::Rust => vec![
            ("Build", "cargo build"),
            ("Test", "cargo test"),
            ("Lint", "cargo clippy --all-targets -- -D warnings"),
            ("Format check", "cargo fmt -- --check"),
            ("Format", "cargo fmt"),
        ],
        ProjectType::Node => vec![
            ("Install", "npm install"),
            ("Test", "npm test"),
            ("Lint", "npx eslint ."),
        ],
        ProjectType::Python => vec![
            ("Test", "python -m pytest"),
            ("Lint", "ruff check ."),
            ("Type check", "python -m mypy ."),
        ],
        ProjectType::Go => vec![
            ("Build", "go build ./..."),
            ("Test", "go test ./..."),
            ("Vet", "go vet ./..."),
        ],
        ProjectType::Make => vec![("Build", "make"), ("Test", "make test")],
        ProjectType::Unknown => vec![],
    }
}

/// Extract the project name from a README.md title line (# Title).
/// Returns None if no README or no title found.
fn extract_project_name_from_readme(dir: &std::path::Path) -> Option<String> {
    let readme_names = ["README.md", "readme.md", "README"];
    for name in &readme_names {
        if let Ok(content) = std::fs::read_to_string(dir.join(name)) {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(title) = trimmed.strip_prefix("# ") {
                    let title = title.trim();
                    if !title.is_empty() {
                        return Some(title.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Extract the project name from Cargo.toml [package] name field.
fn extract_name_from_cargo_toml(dir: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("name") {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix('=') {
                let val = rest.trim().trim_matches('"').trim_matches('\'');
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Extract the project name from package.json "name" field.
fn extract_name_from_package_json(dir: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(dir.join("package.json")).ok()?;
    // Simple JSON parsing — find "name": "value"
    for line in content.lines() {
        let trimmed = line.trim().trim_end_matches(',');
        if let Some(rest) = trimmed.strip_prefix("\"name\"") {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix(':') {
                let val = rest.trim().trim_matches('"');
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Best-effort project name detection. Tries multiple sources.
pub fn detect_project_name(dir: &std::path::Path) -> String {
    // Try Cargo.toml name
    if let Some(name) = extract_name_from_cargo_toml(dir) {
        return name;
    }
    // Try package.json name
    if let Some(name) = extract_name_from_package_json(dir) {
        return name;
    }
    // Try README title
    if let Some(name) = extract_project_name_from_readme(dir) {
        return name;
    }
    // Fall back to directory name
    dir.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "my-project".to_string())
}

/// Generate a complete YOYO.md context file by scanning the project.
pub fn generate_init_content(dir: &std::path::Path) -> String {
    let project_type = detect_project_type(dir);
    let project_name = detect_project_name(dir);
    let important_files = scan_important_files(dir);
    let important_dirs = scan_important_dirs(dir);
    let build_commands = build_commands_for_project(&project_type);

    let mut content = String::new();

    // Header
    content.push_str("# Project Context\n\n");
    content.push_str("<!-- YOYO.md — generated by `yoyo /init`. Edit to customize. -->\n");
    content.push_str("<!-- Also works as CLAUDE.md for compatibility with other tools. -->\n\n");

    // About section
    content.push_str("## About This Project\n\n");
    content.push_str(&format!("**{project_name}**"));
    if project_type != ProjectType::Unknown {
        content.push_str(&format!(" — {project_type} project"));
    }
    content.push_str("\n\n");
    content.push_str("<!-- Add a description of what this project does. -->\n\n");

    // Build & Test section
    content.push_str("## Build & Test\n\n");
    if build_commands.is_empty() {
        content.push_str("<!-- Add build, test, and run commands for this project. -->\n\n");
    } else {
        content.push_str("```bash\n");
        for (label, cmd) in &build_commands {
            content.push_str(&format!("{cmd:<50} # {label}\n"));
        }
        content.push_str("```\n\n");
    }

    // Coding Conventions section
    content.push_str("## Coding Conventions\n\n");
    content.push_str(
        "<!-- List any coding standards, naming conventions, or patterns to follow. -->\n\n",
    );

    // Important Files section
    content.push_str("## Important Files\n\n");
    if important_files.is_empty() && important_dirs.is_empty() {
        content.push_str("<!-- List key files and directories the agent should know about. -->\n");
    } else {
        if !important_dirs.is_empty() {
            content.push_str("Key directories:\n");
            for d in &important_dirs {
                content.push_str(&format!("- `{d}/`\n"));
            }
            content.push('\n');
        }
        if !important_files.is_empty() {
            content.push_str("Key files:\n");
            for f in &important_files {
                content.push_str(&format!("- `{f}`\n"));
            }
            content.push('\n');
        }
    }

    content
}

pub fn handle_init() {
    let path = "YOYO.md";
    if std::path::Path::new(path).exists() {
        println!("{DIM}  {path} already exists — not overwriting.{RESET}\n");
    } else if std::path::Path::new("CLAUDE.md").exists() {
        println!("{DIM}  CLAUDE.md already exists — yoyo reads it as a compatibility alias.");
        println!("  Rename it to YOYO.md when you're ready: mv CLAUDE.md YOYO.md{RESET}\n");
    } else {
        let cwd = std::env::current_dir().unwrap_or_default();
        let project_type = detect_project_type(&cwd);
        println!("{DIM}  Scanning project...{RESET}");
        if project_type != ProjectType::Unknown {
            println!("{DIM}  Detected: {project_type}{RESET}");
        }
        let content = generate_init_content(&cwd);
        match std::fs::write(path, &content) {
            Ok(_) => {
                let line_count = content.lines().count();
                let word = crate::format::pluralize(line_count, "line", "lines");
                println!("{GREEN}  ✓ Created {path} ({line_count} {word}) — edit it to add project context.{RESET}");
                println!("{DIM}  Tip: Use /remember to save project-specific notes that persist across sessions.{RESET}\n");
            }
            Err(e) => eprintln!("{RED}  error creating {path}: {e}{RESET}\n"),
        }
    }
}

// ── /docs ────────────────────────────────────────────────────────────────

pub fn handle_docs(input: &str) {
    if input == "/docs" {
        println!("{DIM}  usage: /docs <crate> [item]");
        println!("  Look up docs.rs documentation for a Rust crate.");
        println!("  Examples: /docs serde, /docs tokio task{RESET}\n");
        return;
    }
    let args = input.trim_start_matches("/docs ").trim();
    if args.is_empty() {
        println!("{DIM}  usage: /docs <crate> [item]{RESET}\n");
        return;
    }
    let parts: Vec<&str> = args.splitn(2, char::is_whitespace).collect();
    let crate_name = parts[0].trim();
    let item_name = parts.get(1).map(|s| s.trim()).unwrap_or("");

    let (found, summary) = if item_name.is_empty() {
        docs::fetch_docs_summary(crate_name)
    } else {
        docs::fetch_docs_item(crate_name, item_name)
    };
    if found {
        let label = if item_name.is_empty() {
            crate_name.to_string()
        } else {
            format!("{crate_name}::{item_name}")
        };
        println!("{GREEN}  ✓ {label}{RESET}");
        println!("{DIM}{summary}{RESET}\n");
    } else {
        println!("{RED}  ✗ {summary}{RESET}\n");
    }
}

// ── /health ──────────────────────────────────────────────────────────────

/// Detected project type based on marker files in the working directory.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Make,
    Unknown,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectType::Rust => write!(f, "Rust (Cargo)"),
            ProjectType::Node => write!(f, "Node.js (npm)"),
            ProjectType::Python => write!(f, "Python"),
            ProjectType::Go => write!(f, "Go"),
            ProjectType::Make => write!(f, "Makefile"),
            ProjectType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Detect project type by checking for marker files in the given directory.
pub fn detect_project_type(dir: &std::path::Path) -> ProjectType {
    if dir.join("Cargo.toml").exists() {
        ProjectType::Rust
    } else if dir.join("package.json").exists() {
        ProjectType::Node
    } else if dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
        || dir.join("setup.cfg").exists()
    {
        ProjectType::Python
    } else if dir.join("go.mod").exists() {
        ProjectType::Go
    } else if dir.join("Makefile").exists() || dir.join("makefile").exists() {
        ProjectType::Make
    } else {
        ProjectType::Unknown
    }
}

// ── /plan ────────────────────────────────────────────────────────────────

/// Parse a `/plan` command and extract the task description.
/// Returns None if no task was provided.
pub fn parse_plan_task(input: &str) -> Option<String> {
    let task = input.strip_prefix("/plan").unwrap_or("").trim().to_string();
    if task.is_empty() {
        None
    } else {
        Some(task)
    }
}

/// Build a planning-mode prompt that asks the agent to create a structured plan
/// WITHOUT executing any tools. This is the "architect mode" equivalent.
pub fn build_plan_prompt(task: &str) -> String {
    format!(
        r#"Create a detailed step-by-step plan for the following task. Do NOT execute any tools — this is planning only.

## Task
{task}

## Instructions
Analyze the task and produce a structured plan covering:

1. **Files to examine** — which existing files need to be read to understand the current state
2. **Files to modify** — which files will be created or changed, and what changes
3. **Step-by-step approach** — ordered list of concrete implementation steps
4. **Tests to write** — what tests should be added or updated
5. **Potential risks** — what could go wrong, edge cases, backwards compatibility concerns
6. **Verification** — how to confirm the changes work correctly

Be specific: mention file paths, function names, and concrete code changes where possible.
Keep the plan actionable — someone (or you, in the next step) should be able to execute it directly."#
    )
}

/// Handle the `/plan` command: create a structured plan for a task without executing tools.
/// The plan gets injected into conversation context so the user can review and say "go ahead."
/// Returns Some(plan_prompt) if a plan was requested, None otherwise.
pub async fn handle_plan(
    input: &str,
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let task = match parse_plan_task(input) {
        Some(t) => t,
        None => {
            println!("{DIM}  usage: /plan <task description>{RESET}");
            println!("{DIM}  Creates a step-by-step plan without executing any tools.{RESET}");
            println!("{DIM}  Review the plan, then say \"go ahead\" to execute it.{RESET}\n");
            return None;
        }
    };

    println!("{DIM}  📋 Planning: {task}{RESET}\n");

    let plan_prompt = build_plan_prompt(&task);
    run_prompt(agent, &plan_prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    println!(
        "\n{DIM}  💡 Review the plan above. Say \"go ahead\" to execute it, or refine it.{RESET}\n"
    );

    Some(plan_prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::KNOWN_COMMANDS;
    use crate::help::help_text;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    // ── detect_project_type ──────────────────────────────────────────

    #[test]
    fn detect_project_type_rust() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Rust);
    }

    #[test]
    fn detect_project_type_node() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Node);
    }

    #[test]
    fn detect_project_type_python_pyproject() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pyproject.toml"), "[tool]").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Python);
    }

    #[test]
    fn detect_project_type_python_setup_py() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("setup.py"), "").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Python);
    }

    #[test]
    fn detect_project_type_python_setup_cfg() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("setup.cfg"), "").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Python);
    }

    #[test]
    fn detect_project_type_go() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("go.mod"), "module example").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Go);
    }

    #[test]
    fn detect_project_type_make() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Makefile"), "all:").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Make);
    }

    #[test]
    fn detect_project_type_make_lowercase() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("makefile"), "all:").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Make);
    }

    #[test]
    fn detect_project_type_unknown_empty_dir() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Unknown);
    }

    #[test]
    fn detect_project_type_priority_rust_over_make() {
        // Cargo.toml should win even if Makefile also exists
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(dir.path().join("Makefile"), "all:").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Rust);
    }

    // ── ProjectType Display ──────────────────────────────────────────

    #[test]
    fn project_type_display() {
        assert_eq!(format!("{}", ProjectType::Rust), "Rust (Cargo)");
        assert_eq!(format!("{}", ProjectType::Node), "Node.js (npm)");
        assert_eq!(format!("{}", ProjectType::Python), "Python");
        assert_eq!(format!("{}", ProjectType::Go), "Go");
        assert_eq!(format!("{}", ProjectType::Make), "Makefile");
        assert_eq!(format!("{}", ProjectType::Unknown), "Unknown");
    }

    // ── scan_important_files ─────────────────────────────────────────

    #[test]
    fn scan_important_files_finds_known_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.md"), "# Hello").unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(dir.path().join(".gitignore"), "target/").unwrap();
        let found = scan_important_files(dir.path());
        assert!(found.contains(&"README.md".to_string()));
        assert!(found.contains(&"Cargo.toml".to_string()));
        assert!(found.contains(&".gitignore".to_string()));
    }

    #[test]
    fn scan_important_files_empty_dir() {
        let dir = TempDir::new().unwrap();
        let found = scan_important_files(dir.path());
        assert!(found.is_empty());
    }

    #[test]
    fn scan_important_files_ignores_unknown() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("random.txt"), "stuff").unwrap();
        let found = scan_important_files(dir.path());
        assert!(found.is_empty());
    }

    // ── scan_important_dirs ──────────────────────────────────────────

    #[test]
    fn scan_important_dirs_finds_known_dirs() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::create_dir(dir.path().join("tests")).unwrap();
        fs::create_dir(dir.path().join("docs")).unwrap();
        let found = scan_important_dirs(dir.path());
        assert!(found.contains(&"src".to_string()));
        assert!(found.contains(&"tests".to_string()));
        assert!(found.contains(&"docs".to_string()));
    }

    #[test]
    fn scan_important_dirs_empty_dir() {
        let dir = TempDir::new().unwrap();
        let found = scan_important_dirs(dir.path());
        assert!(found.is_empty());
    }

    #[test]
    fn scan_important_dirs_ignores_files() {
        let dir = TempDir::new().unwrap();
        // Create a file named "src" — not a directory
        fs::write(dir.path().join("src"), "not a dir").unwrap();
        let found = scan_important_dirs(dir.path());
        assert!(!found.contains(&"src".to_string()));
    }

    // ── detect_project_name ──────────────────────────────────────────

    #[test]
    fn detect_project_name_from_cargo_toml() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"my-crate\"",
        )
        .unwrap();
        assert_eq!(detect_project_name(dir.path()), "my-crate");
    }

    #[test]
    fn detect_project_name_from_package_json() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            "{\n  \"name\": \"my-app\",\n  \"version\": \"1.0.0\"\n}",
        )
        .unwrap();
        assert_eq!(detect_project_name(dir.path()), "my-app");
    }

    #[test]
    fn detect_project_name_from_readme() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.md"), "# Cool Project\n\nSome text").unwrap();
        assert_eq!(detect_project_name(dir.path()), "Cool Project");
    }

    #[test]
    fn detect_project_name_cargo_over_readme() {
        // Cargo.toml should win over README
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"cargo-name\"",
        )
        .unwrap();
        fs::write(dir.path().join("README.md"), "# README Title").unwrap();
        assert_eq!(detect_project_name(dir.path()), "cargo-name");
    }

    #[test]
    fn detect_project_name_fallback_to_dir_name() {
        let dir = TempDir::new().unwrap();
        // No marker files — should fall back to the dir name
        let name = detect_project_name(dir.path());
        // TempDir creates something like /tmp/.tmpXXXXXX — just check it's not empty
        assert!(!name.is_empty());
    }

    // ── extract_project_name_from_readme ─────────────────────────────

    #[test]
    fn extract_readme_skips_blank_lines() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.md"), "\n\n  \n# Title After Blanks").unwrap();
        assert_eq!(detect_project_name(dir.path()), "Title After Blanks");
    }

    #[test]
    fn extract_readme_empty_title_skipped() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.md"), "#  \n# Real Title").unwrap();
        assert_eq!(detect_project_name(dir.path()), "Real Title");
    }

    // ── extract_name_from_cargo_toml edge cases ──────────────────────

    #[test]
    fn cargo_toml_name_with_single_quotes() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = 'quoted'").unwrap();
        assert_eq!(detect_project_name(dir.path()), "quoted");
    }

    #[test]
    fn cargo_toml_name_with_spaces_around_equals() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname   =   \"spaced\"",
        )
        .unwrap();
        assert_eq!(detect_project_name(dir.path()), "spaced");
    }

    // ── build_commands_for_project ───────────────────────────────────

    #[test]
    fn build_commands_rust() {
        let cmds = build_commands_for_project(&ProjectType::Rust);
        assert!(!cmds.is_empty());
        assert!(cmds.iter().any(|(label, _)| *label == "Build"));
        assert!(cmds.iter().any(|(label, _)| *label == "Test"));
    }

    #[test]
    fn build_commands_unknown_empty() {
        let cmds = build_commands_for_project(&ProjectType::Unknown);
        assert!(cmds.is_empty());
    }

    #[test]
    fn build_commands_node() {
        let cmds = build_commands_for_project(&ProjectType::Node);
        assert!(cmds.iter().any(|(_, cmd)| *cmd == "npm install"));
    }

    #[test]
    fn build_commands_python() {
        let cmds = build_commands_for_project(&ProjectType::Python);
        assert!(cmds.iter().any(|(_, cmd)| *cmd == "python -m pytest"));
    }

    #[test]
    fn build_commands_go() {
        let cmds = build_commands_for_project(&ProjectType::Go);
        assert!(cmds.iter().any(|(_, cmd)| *cmd == "go build ./..."));
    }

    // ── generate_init_content ────────────────────────────────────────

    #[test]
    fn generate_init_content_rust_project() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test-proj\"",
        )
        .unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

        let content = generate_init_content(dir.path());
        assert!(content.contains("# Project Context"));
        assert!(content.contains("test-proj"));
        assert!(content.contains("Rust (Cargo)"));
        assert!(content.contains("cargo build"));
        assert!(content.contains("cargo test"));
    }

    #[test]
    fn generate_init_content_unknown_project() {
        let dir = TempDir::new().unwrap();
        let content = generate_init_content(dir.path());
        assert!(content.contains("# Project Context"));
        // Should not contain a project type label
        assert!(!content.contains("Rust"));
        assert!(!content.contains("Node"));
        // Should have placeholder for build commands
        assert!(content.contains("Add build, test, and run commands"));
    }

    #[test]
    fn generate_init_content_includes_dirs_and_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.md"), "# My Project").unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();

        let content = generate_init_content(dir.path());
        assert!(content.contains("`src/`"));
        assert!(content.contains("`README.md`"));
    }

    // ── parse_plan_task tests ────────────────────────────────────────────

    #[test]
    fn parse_plan_task_with_description() {
        let result = parse_plan_task("/plan add error handling to the parser");
        assert_eq!(result, Some("add error handling to the parser".to_string()));
    }

    #[test]
    fn parse_plan_task_empty() {
        let result = parse_plan_task("/plan");
        assert!(result.is_none(), "Empty /plan should return None");
    }

    #[test]
    fn parse_plan_task_whitespace_only() {
        let result = parse_plan_task("/plan   ");
        assert!(result.is_none(), "Whitespace-only /plan should return None");
    }

    #[test]
    fn parse_plan_task_preserves_full_description() {
        let result = parse_plan_task("/plan refactor main.rs into smaller modules with tests");
        assert_eq!(
            result,
            Some("refactor main.rs into smaller modules with tests".to_string())
        );
    }

    // ── build_plan_prompt tests ─────────────────────────────────────────

    #[test]
    fn build_plan_prompt_contains_task() {
        let prompt = build_plan_prompt("add a /plan command");
        assert!(
            prompt.contains("add a /plan command"),
            "Plan prompt should contain the task"
        );
    }

    #[test]
    fn build_plan_prompt_contains_no_tools_instruction() {
        let prompt = build_plan_prompt("something");
        assert!(
            prompt.contains("Do NOT execute any tools"),
            "Plan prompt should instruct not to use tools"
        );
    }

    #[test]
    fn build_plan_prompt_contains_structure_sections() {
        let prompt = build_plan_prompt("add feature X");
        assert!(
            prompt.contains("Files to examine"),
            "Should mention files to examine"
        );
        assert!(
            prompt.contains("Files to modify"),
            "Should mention files to modify"
        );
        assert!(
            prompt.contains("Step-by-step"),
            "Should mention step-by-step approach"
        );
        assert!(prompt.contains("Tests to write"), "Should mention tests");
        assert!(prompt.contains("Potential risks"), "Should mention risks");
        assert!(
            prompt.contains("Verification"),
            "Should mention verification"
        );
    }

    // ── /todo tests ──────────────────────────────────────────────────────

    #[test]
    #[serial]
    fn test_todo_add_returns_incrementing_ids() {
        todo_clear();
        let id1 = todo_add("first task");
        let id2 = todo_add("second task");
        assert!(id2 > id1, "IDs should increment: {id1} < {id2}");
        let items = todo_list();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].description, "first task");
        assert_eq!(items[1].description, "second task");
    }

    #[test]
    #[serial]
    fn test_todo_update_status() {
        todo_clear();
        let id = todo_add("update me");
        assert_eq!(todo_list()[0].status, TodoStatus::Pending);

        todo_update(id, TodoStatus::InProgress).unwrap();
        assert_eq!(todo_list()[0].status, TodoStatus::InProgress);

        todo_update(id, TodoStatus::Done).unwrap();
        assert_eq!(todo_list()[0].status, TodoStatus::Done);
    }

    #[test]
    #[serial]
    fn test_todo_update_invalid_id() {
        todo_clear();
        let result = todo_update(99999, TodoStatus::Done);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("99999"));
    }

    #[test]
    #[serial]
    fn test_todo_remove() {
        todo_clear();
        let id = todo_add("remove me");
        assert_eq!(todo_list().len(), 1);

        let removed = todo_remove(id).unwrap();
        assert_eq!(removed.description, "remove me");
        assert!(todo_list().is_empty());
    }

    #[test]
    #[serial]
    fn test_todo_remove_invalid_id() {
        todo_clear();
        let result = todo_remove(99998);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("99998"));
    }

    #[test]
    #[serial]
    fn test_todo_clear() {
        todo_clear();
        todo_add("one");
        todo_add("two");
        assert_eq!(todo_list().len(), 2);

        todo_clear();
        assert!(todo_list().is_empty());
    }

    #[test]
    #[serial]
    fn test_todo_list_empty() {
        todo_clear();
        assert!(todo_list().is_empty());
    }

    #[test]
    #[serial]
    fn test_format_todo_list() {
        todo_clear();
        let id1 = todo_add("pending task");
        let id2 = todo_add("wip task");
        let id3 = todo_add("done task");
        todo_update(id2, TodoStatus::InProgress).unwrap();
        todo_update(id3, TodoStatus::Done).unwrap();

        let items = todo_list();
        let formatted = format_todo_list(&items);
        assert!(formatted.contains("[ ]"), "Should contain pending checkbox");
        assert!(
            formatted.contains("[~]"),
            "Should contain in-progress checkbox"
        );
        assert!(formatted.contains("[✓]"), "Should contain done checkbox");
        assert!(formatted.contains(&format!("#{id1}")));
        assert!(formatted.contains("pending task"));
        assert!(formatted.contains("wip task"));
        assert!(formatted.contains("done task"));
    }

    #[test]
    fn test_format_todo_list_empty() {
        let formatted = format_todo_list(&[]);
        assert!(formatted.contains("No tasks"));
    }

    #[test]
    #[serial]
    fn test_handle_todo_add() {
        todo_clear();
        let result = handle_todo("/todo add write tests");
        assert!(result.contains("Added task"));
        assert!(result.contains("write tests"));
        assert_eq!(todo_list().len(), 1);
    }

    #[test]
    #[serial]
    fn test_handle_todo_show_empty() {
        todo_clear();
        let result = handle_todo("/todo");
        assert!(result.contains("No tasks"));
    }

    #[test]
    #[serial]
    fn test_handle_todo_done() {
        todo_clear();
        let id = todo_add("finish me");
        let result = handle_todo(&format!("/todo done {id}"));
        assert!(result.contains("done"));
        assert_eq!(todo_list()[0].status, TodoStatus::Done);
    }

    #[test]
    #[serial]
    fn test_handle_todo_wip() {
        todo_clear();
        let id = todo_add("start me");
        let result = handle_todo(&format!("/todo wip {id}"));
        assert!(result.contains("in-progress"));
        assert_eq!(todo_list()[0].status, TodoStatus::InProgress);
    }

    #[test]
    #[serial]
    fn test_handle_todo_remove_via_command() {
        todo_clear();
        let id = todo_add("delete me");
        let result = handle_todo(&format!("/todo remove {id}"));
        assert!(result.contains("Removed"));
        assert!(todo_list().is_empty());
    }

    #[test]
    #[serial]
    fn test_handle_todo_clear_via_command() {
        todo_clear();
        todo_add("one");
        todo_add("two");
        let result = handle_todo("/todo clear");
        assert!(result.contains("Cleared"));
        assert!(todo_list().is_empty());
    }

    #[test]
    fn test_handle_todo_unknown_subcommand() {
        let result = handle_todo("/todo badcmd");
        assert!(result.contains("Usage"));
    }

    #[test]
    #[serial]
    fn test_handle_todo_add_empty_description() {
        let result = handle_todo("/todo add");
        assert!(result.contains("Usage"));
        let result2 = handle_todo("/todo add   ");
        assert!(result2.contains("Usage"));
    }

    #[test]
    fn test_todo_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/todo"),
            "/todo should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_todo_help_exists() {
        let help = crate::help::command_help("todo");
        assert!(help.is_some(), "todo should have help text");
        let text = help.unwrap();
        assert!(text.contains("/todo add"));
        assert!(text.contains("/todo done"));
        assert!(text.contains("/todo clear"));
    }

    #[test]
    fn test_todo_in_help_text() {
        let text = help_text();
        assert!(text.contains("/todo"), "/todo should appear in help text");
    }
}
