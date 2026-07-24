//! Project-related command handlers: /context, /init, /docs.

use crate::cli;
use crate::commands_map::build_repo_map;
use crate::commands_search::is_binary_extension;
use crate::docs;
use crate::format::*;
use crate::symbols::{FileSymbols, SymbolKind};

// Re-export refactoring commands for backward compatibility
pub use crate::commands_move::handle_move;
pub use crate::commands_refactor::{handle_extract, handle_refactor};
pub use crate::commands_rename::{handle_rename, rename_in_project};

use yoagent::agent::Agent;

// ── /context ─────────────────────────────────────────────────────────────

/// Subcommands for /context.
const CONTEXT_SUBCOMMANDS: &[&str] = &["system", "tokens", "files", "relevant"];

pub fn context_subcommands() -> &'static [&'static str] {
    CONTEXT_SUBCOMMANDS
}

pub fn handle_context(input: &str, system_prompt: &str, agent: &Agent) {
    let args = input.strip_prefix("/context").unwrap_or("").trim();

    if args.starts_with("system") {
        show_system_prompt_sections(system_prompt);
    } else if args.starts_with("tokens") {
        show_context_tokens(system_prompt, agent);
    } else if args.starts_with("files") {
        show_context_files(agent);
    } else if args.starts_with("relevant") {
        let query = args.strip_prefix("relevant").unwrap_or("").trim();
        handle_context_relevant(query);
    } else {
        show_project_context_files();
    }
}

// ---------------------------------------------------------------------------
// /context files — show files the agent has interacted with
// ---------------------------------------------------------------------------

/// Categories of file interaction, ordered for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum FileAction {
    Read,
    Edited,
    Written,
    Listed,
    Searched,
}

impl FileAction {
    fn label(self) -> &'static str {
        match self {
            FileAction::Read => "Read",
            FileAction::Edited => "Edited",
            FileAction::Written => "Written",
            FileAction::Listed => "Listed",
            FileAction::Searched => "Searched",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            FileAction::Read => "📖",
            FileAction::Edited => "✏️ ",
            FileAction::Written => "📝",
            FileAction::Listed => "📂",
            FileAction::Searched => "🔍",
        }
    }
}

/// Extract file paths from agent messages, grouped by action type.
/// Returns a sorted `BTreeMap<FileAction, BTreeSet<String>>`.
fn extract_context_files(
    messages: &[yoagent::types::AgentMessage],
) -> std::collections::BTreeMap<FileAction, std::collections::BTreeSet<String>> {
    use std::collections::{BTreeMap, BTreeSet};
    use yoagent::types::{AgentMessage, Content, Message};

    let mut result: BTreeMap<FileAction, BTreeSet<String>> = BTreeMap::new();

    for msg in messages {
        let llm = match msg {
            AgentMessage::Llm(m) => m,
            _ => continue,
        };
        let content = match llm {
            Message::Assistant { content, .. } => content,
            _ => continue,
        };
        for block in content {
            if let Content::ToolCall {
                name, arguments, ..
            } = block
            {
                let action = match name.as_str() {
                    "read_file" => FileAction::Read,
                    "edit_file" => FileAction::Edited,
                    "write_file" => FileAction::Written,
                    "list_files" => FileAction::Listed,
                    "search" => FileAction::Searched,
                    _ => continue,
                };

                if let Some(path) = arguments.get("path").and_then(|v| v.as_str()) {
                    if !path.is_empty() {
                        result.entry(action).or_default().insert(path.to_string());
                    }
                }
            }
        }
    }

    result
}

fn show_context_files(agent: &Agent) {
    let files = extract_context_files(agent.messages());

    if files.is_empty() {
        println!("{DIM}  (no files referenced yet){RESET}");
        return;
    }

    println!("{DIM}  Files in this conversation:\n");
    for (action, paths) in &files {
        let paths_str: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        println!(
            "    {} {:<9} {}",
            action.icon(),
            format!("{}:", action.label()),
            paths_str.join(", ")
        );
    }
    println!("{RESET}");
}

fn show_context_tokens(system_prompt: &str, agent: &Agent) {
    let messages = agent.messages();
    let context_used = yoagent::context::total_tokens(messages) as u64;
    let context_max = cli::effective_context_tokens();

    // System prompt tokens
    let sys_tokens = estimate_tokens(system_prompt);
    println!("{DIM}  Context token budget:\n");
    println!(
        "    system prompt: ~{} tokens",
        format_token_count(sys_tokens as u64)
    );

    // Section breakdown (only if >1 section)
    let sections = parse_prompt_sections(system_prompt);
    if sections.len() > 1 {
        // Find the longest section name for alignment
        let max_name_len = sections
            .iter()
            .map(|s| s.name.len())
            .max()
            .unwrap_or(0)
            .min(30); // cap alignment width

        for section in &sections {
            let section_text = section.lines.join("\n");
            let full_text = format!("{}\n{}", section.name, section_text);
            let tokens = estimate_tokens(&full_text);
            let display_name = crate::format::truncate_with_ellipsis(&section.name, 30);
            println!(
                "      {:<width$}  ~{}",
                display_name,
                format_token_count(tokens as u64),
                width = max_name_len,
            );
        }
    }

    // Conversation
    println!(
        "    conversation:  {} message{}",
        messages.len(),
        if messages.len() == 1 { "" } else { "s" },
    );
    println!(
        "    context used:  {} / {} tokens",
        format_token_count(context_used),
        format_token_count(context_max),
    );

    // Percentage and remaining
    if context_max > 0 {
        let pct = ((context_used as f64 / context_max as f64) * 100.0) as u32;
        let color = context_usage_color(pct);
        let remaining = context_max.saturating_sub(context_used);
        println!("    usage:         {color}{pct}%{DIM}");
        println!(
            "    remaining:     ~{} tokens",
            format_token_count(remaining)
        );
    }
    println!("{RESET}");
}

fn show_project_context_files() {
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

/// A section parsed from a system prompt (split by markdown headers).
#[derive(Debug, Clone)]
pub struct PromptSection {
    pub name: String,
    pub header_level: usize,
    pub lines: Vec<String>,
}

/// Parse a system prompt into sections by splitting on markdown headers.
/// Each `# ` or `## ` header starts a new section. Content before the first
/// header becomes a "(preamble)" section.
pub fn parse_prompt_sections(prompt: &str) -> Vec<PromptSection> {
    let mut sections: Vec<PromptSection> = Vec::new();
    let mut current_name = "(preamble)".to_string();
    let mut current_level = 0usize;
    let mut current_lines: Vec<String> = Vec::new();

    for line in prompt.lines() {
        if let Some(rest) = line.strip_prefix("# ") {
            // Flush previous section
            if !current_lines.is_empty() || current_name != "(preamble)" {
                sections.push(PromptSection {
                    name: current_name,
                    header_level: current_level,
                    lines: current_lines,
                });
            }
            current_name = rest.trim().to_string();
            current_level = 1;
            current_lines = Vec::new();
        } else if let Some(rest) = line.strip_prefix("## ") {
            // Flush previous section
            if !current_lines.is_empty() || current_name != "(preamble)" {
                sections.push(PromptSection {
                    name: current_name,
                    header_level: current_level,
                    lines: current_lines,
                });
            }
            current_name = rest.trim().to_string();
            current_level = 2;
            current_lines = Vec::new();
        } else {
            current_lines.push(line.to_string());
        }
    }
    // Flush last section
    if !current_lines.is_empty() || current_name != "(preamble)" {
        sections.push(PromptSection {
            name: current_name,
            header_level: current_level,
            lines: current_lines,
        });
    }

    sections
}

/// Estimate token count from character count (rough approximation: chars / 4).
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

fn show_system_prompt_sections(prompt: &str) {
    if prompt.is_empty() {
        println!("{DIM}  System prompt is empty.{RESET}\n");
        return;
    }

    let sections = parse_prompt_sections(prompt);
    let total_lines: usize = sections.iter().map(|s| s.lines.len() + 1).sum(); // +1 for header
    let total_tokens = estimate_tokens(prompt);

    println!("{BOLD}  System prompt sections:{RESET}");
    println!();

    for section in &sections {
        let section_text = section.lines.join("\n");
        let tokens = estimate_tokens(&format!("{}\n{}", section.name, section_text));
        let line_count = section.lines.len();
        let prefix = if section.header_level <= 1 { "#" } else { "##" };
        let word = crate::format::pluralize(line_count, "line", "lines");

        println!(
            "{BOLD}  {prefix} {}{RESET}  {DIM}({line_count} {word}, ~{tokens} tokens){RESET}",
            section.name
        );

        // Print first 3 non-empty lines as preview
        let preview_lines: Vec<&String> = section
            .lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .take(3)
            .collect();
        for line in &preview_lines {
            let display = crate::format::truncate_with_ellipsis(line, 80);
            println!("{DIM}    {display}{RESET}");
        }
        if section
            .lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .count()
            > 3
        {
            println!("{DIM}    ...{RESET}");
        }
        println!();
    }

    println!("{DIM}  Total: {total_lines} lines, ~{total_tokens} tokens (estimated){RESET}\n");
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
        // Java
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        // Ruby
        "Gemfile",
        "Gemfile.lock",
        "Rakefile",
        ".rubocop.yml",
        // C/C++
        "CMakeLists.txt",
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
        ProjectType::Java => vec![("Build", "mvn compile"), ("Test", "mvn test")],
        ProjectType::Ruby => vec![
            ("Test", "bundle exec rake test"),
            ("Lint", "bundle exec rubocop"),
        ],
        ProjectType::Cpp => vec![
            ("Build", "cmake --build build"),
            ("Test", "ctest --test-dir build"),
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

/// AI tool instruction files that yoyo recognises.
/// Each entry is `(relative_path, label)`.
const AI_CONFIG_FILES: &[(&str, &str)] = &[
    ("CLAUDE.md", "Claude Code"),
    ("AGENTS.md", "Gemini / generic agents"),
    (".cursorrules", "Cursor"),
    (".github/copilot-instructions.md", "GitHub Copilot"),
];

/// Detect which other AI tool instruction files already exist in `dir`.
/// Returns a vec of `(path, label)` for each file found.
pub fn detect_ai_config_files(dir: &std::path::Path) -> Vec<(&'static str, &'static str)> {
    AI_CONFIG_FILES
        .iter()
        .filter(|(path, _)| dir.join(path).exists())
        .copied()
        .collect()
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

    // Other AI Tool Configs section (if any found)
    let ai_configs = detect_ai_config_files(dir);
    if !ai_configs.is_empty() {
        content.push_str("\n## Other AI Tool Configs\n\n");
        content.push_str("This project also has instruction files for other AI tools:\n");
        for (path, label) in &ai_configs {
            content.push_str(&format!("- `{path}` ({label})\n"));
        }
        content.push_str("\nyoyo reads these automatically for additional project context.\n");
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
        let ai_configs = detect_ai_config_files(&cwd);
        if !ai_configs.is_empty() {
            let names: Vec<&str> = ai_configs.iter().map(|(p, _)| *p).collect();
            eprintln!(
                "{DIM}  Found existing AI configs: {} — yoyo reads these automatically{RESET}",
                names.join(", ")
            );
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
    Java,
    Ruby,
    Cpp,
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
            ProjectType::Java => write!(f, "Java"),
            ProjectType::Ruby => write!(f, "Ruby"),
            ProjectType::Cpp => write!(f, "C/C++ (CMake)"),
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
    } else if dir.join("pom.xml").exists()
        || dir.join("build.gradle").exists()
        || dir.join("build.gradle.kts").exists()
    {
        ProjectType::Java
    } else if dir.join("Gemfile").exists() {
        ProjectType::Ruby
    } else if dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
        || dir.join("setup.cfg").exists()
    {
        ProjectType::Python
    } else if dir.join("go.mod").exists() {
        ProjectType::Go
    } else if dir.join("CMakeLists.txt").exists() {
        ProjectType::Cpp
    } else if dir.join("Makefile").exists() || dir.join("makefile").exists() {
        ProjectType::Make
    } else {
        ProjectType::Unknown
    }
}

// ── /plan ────────────────────────────────────────────────────────────────

/// Return short development convention hints for a given project type.
/// These are injected into project context when no explicit context file exists.
/// Returns None for Unknown project types.
pub fn project_type_hints(project_type: &ProjectType) -> Option<String> {
    let hints = match project_type {
        ProjectType::Rust => {
            "Build: `cargo build`\n\
             Test: `cargo test`\n\
             Lint: `cargo clippy --all-targets -- -D warnings`\n\
             Format: `cargo fmt`"
        }
        ProjectType::Node => {
            "Install: `npm install`\n\
             Test: `npm test`\n\
             Scripts: check `package.json` \"scripts\" for available commands"
        }
        ProjectType::Python => {
            "Test: `python -m pytest`\n\
             Lint: `ruff check .` or `flake8`\n\
             Install deps: `pip install -e .` or `poetry install`"
        }
        ProjectType::Go => {
            "Build: `go build ./...`\n\
             Test: `go test ./...`\n\
             Vet: `go vet ./...`"
        }
        ProjectType::Java => {
            "Build: `mvn compile` or `gradle build`\n\
             Test: `mvn test` or `gradle test`"
        }
        ProjectType::Ruby => {
            "Test: `bundle exec rake test` or `bundle exec rspec`\n\
             Lint: `bundle exec rubocop`\n\
             Install: `bundle install`"
        }
        ProjectType::Cpp => {
            "Configure: `cmake -B build`\n\
             Build: `cmake --build build`\n\
             Test: `ctest --test-dir build`"
        }
        ProjectType::Make => {
            "Build: `make`\n\
             Test: `make test`"
        }
        ProjectType::Unknown => return None,
    };
    Some(hints.to_string())
}

// ---------------------------------------------------------------------------
// /context relevant — auto-identify files relevant to a query
// ---------------------------------------------------------------------------

/// Decompose a snake_case or camelCase identifier into lowercase component words.
///
/// Examples:
/// - `agent_builder` → `["agent", "builder"]`
/// - `StreamingBashTool` → `["streaming", "bash", "tool"]`
/// - `auto_context_for_prompt` → `["auto", "context", "for", "prompt"]`
/// - `HTMLParser` → `["html", "parser"]`
/// - `getURLValue` → `["get", "url", "value"]`
fn decompose_identifier(ident: &str) -> Vec<String> {
    let mut words = Vec::new();
    // First split on underscores, hyphens, dots, slashes (common separators)
    for segment in ident.split(['_', '-', '.', '/']) {
        if segment.is_empty() {
            continue;
        }
        // Then split camelCase / PascalCase within each segment
        let mut current = String::new();
        let chars: Vec<char> = segment.chars().collect();
        for i in 0..chars.len() {
            let c = chars[i];
            if c.is_uppercase() {
                if !current.is_empty() {
                    // Check if we're in an all-caps run followed by lowercase
                    // e.g., "HTMLParser" — when we hit 'P' (uppercase after uppercase run),
                    // and the next char 'a' is lowercase, push the full acronym and start fresh
                    let prev_is_upper = i > 0 && chars[i - 1].is_uppercase();
                    let next_is_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();
                    if prev_is_upper && next_is_lower && current.len() > 1 {
                        // "HTML" is complete; 'P' starts new word "Parser"
                        words.push(current.to_lowercase());
                        current = String::new();
                        current.push(c);
                    } else if prev_is_upper {
                        // Still in an all-caps run (e.g., "HTM" in "HTML"), keep accumulating
                        current.push(c);
                    } else {
                        // Normal camelCase boundary (lowercase → uppercase)
                        words.push(current.to_lowercase());
                        current = String::new();
                        current.push(c);
                    }
                } else {
                    current.push(c);
                }
            } else {
                current.push(c);
            }
        }
        if !current.is_empty() {
            words.push(current.to_lowercase());
        }
    }
    // Filter out empty strings
    words.retain(|w| !w.is_empty());
    words
}

/// Common stop words filtered out of queries.
const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "to", "for", "in", "is", "are", "and", "or", "of", "with", "on", "it",
    "this", "that", "my", "do", "how",
];

/// Tokenize a natural-language query into keywords.
///
/// Splits on whitespace, lowercases, filters out common stop words, and
/// decomposes any snake_case/camelCase tokens into component words.
fn tokenize_query(query: &str) -> Vec<String> {
    let mut keywords = Vec::new();
    for word in query.split_whitespace() {
        // Decompose from the original casing (so camelCase boundaries are visible)
        let parts = decompose_identifier(word);
        if parts.len() > 1 {
            for part in parts {
                if !STOP_WORDS.contains(&part.as_str()) {
                    keywords.push(part);
                }
            }
        } else {
            // Single word or no decomposition — just lowercase and filter
            let lower = word.to_lowercase();
            if !STOP_WORDS.contains(&lower.as_str()) {
                keywords.push(lower);
            }
        }
    }
    keywords
}

/// A single file's relevance score and matching details.
#[derive(Debug)]
struct RelevanceResult {
    path: String,
    score: usize,
    matched_keywords: Vec<String>,
}

/// Score files from a repo map against a set of keywords.
///
/// Scoring:
///   - Filename component match (split by `/`, `_`, `.`): 3 points per keyword
///   - Symbol name match (function/struct/enum names): 2 points per keyword
///
/// Matches are case-insensitive substring matches.
fn score_files(files: &[FileSymbols], keywords: &[String]) -> Vec<RelevanceResult> {
    let mut results = Vec::new();

    for file in files {
        let mut score = 0usize;
        let mut matched = Vec::new();

        // Split filename into components by path separators, underscores, and dots
        let path_lower = file.path.to_lowercase();
        let path_components: Vec<&str> = path_lower.split(&['/', '\\', '_', '.'][..]).collect();

        for kw in keywords {
            let mut kw_matched = false;

            // Filename component matching (3x weight)
            for comp in &path_components {
                if comp.contains(kw.as_str()) {
                    score += 3;
                    kw_matched = true;
                    break;
                }
            }

            // Symbol name matching (2x weight)
            // Decompose camelCase/snake_case symbol names for better matching
            for sym in &file.symbols {
                let sym_lower = sym.name.to_lowercase();
                let sym_parts = decompose_identifier(&sym.name);
                let matched_via_decomposed = sym_parts.iter().any(|p| p == kw.as_str());
                if matched_via_decomposed || sym_lower.contains(kw.as_str()) {
                    score += 2;
                    kw_matched = true;
                    break;
                }
            }

            if kw_matched {
                matched.push(kw.clone());
            }
        }

        if score > 0 {
            results.push(RelevanceResult {
                path: file.path.clone(),
                score,
                matched_keywords: matched,
            });
        }
    }

    // Sort by score descending, then by path for stability
    results.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    results
}

/// Handle `/context relevant <query>`.
pub fn handle_context_relevant(query: &str) {
    let query = query.trim();
    if query.is_empty() {
        eprintln!(
            "{BOLD}Usage:{RESET} /context relevant <query>\n\
             Example: /context relevant web search fallback\n\n\
             Finds project files most relevant to a natural-language query\n\
             by matching keywords against filenames and symbol names."
        );
        return;
    }

    let keywords = tokenize_query(query);
    if keywords.is_empty() {
        eprintln!(
            "{YELLOW}No meaningful keywords{RESET} in query \"{query}\". Try more specific terms."
        );
        return;
    }

    let repo_map = build_repo_map(None, false);
    let results = score_files(&repo_map, &keywords);

    if results.is_empty() {
        eprintln!(
            "{YELLOW}No matching files{RESET} for keywords: {}. Try more specific or different terms.",
            keywords.join(", ")
        );
        return;
    }

    let top = &results[..results.len().min(10)];

    eprintln!("\n{BOLD_CYAN}Files relevant to \"{query}\":{RESET}\n");
    eprintln!("  {DIM}Keywords:{RESET} {}\n", keywords.join(", "));

    for (i, r) in top.iter().enumerate() {
        let rank = i + 1;
        eprintln!(
            "  {BOLD}{rank:>2}.{RESET} {GREEN}{}{RESET} {DIM}(score: {}, matched: {}){RESET}",
            r.path,
            r.score,
            r.matched_keywords.join(", "),
        );
    }

    if results.len() > 10 {
        eprintln!(
            "\n  {DIM}… and {} more files with matches{RESET}",
            results.len() - 10
        );
    }
    eprintln!();
}

/// Maximum number of files to auto-inject into a prompt.
const AUTO_CONTEXT_MAX_FILES: usize = 3;

/// Minimum relevance score for a file to be auto-injected.
const AUTO_CONTEXT_MIN_SCORE: usize = 5;

/// Score multiplier numerator for recently-edited files (×3/2 = 1.5×).
const RECENCY_BOOST_NUM: usize = 3;
/// Score multiplier denominator for recently-edited files.
const RECENCY_BOOST_DEN: usize = 2;

/// Maximum lines to read from a single file for auto-context.
const AUTO_CONTEXT_MAX_LINES: usize = 200;

/// Files longer than this get truncated with a note.
const AUTO_CONTEXT_LARGE_FILE: usize = 500;

/// Maximum chars for the compact signature block injected into auto-context.
const SIGNATURE_BLOCK_MAX_CHARS: usize = 2000;

/// Sentinel path used to identify the signature block entry in auto-context results.
const SIGNATURE_SENTINEL: &str = "[signatures]";

/// Truncate a file's content for auto-context injection.
///
/// Files longer than `AUTO_CONTEXT_MAX_LINES` are cut to the first
/// `AUTO_CONTEXT_MAX_LINES` lines with a note; files longer than
/// `AUTO_CONTEXT_LARGE_FILE` get a slightly different note. Shorter files are
/// returned unchanged.
///
/// The line slice is clamped with `.min(total_lines)` so it can never index
/// out of bounds — the previous inline version silently depended on the
/// invariant `AUTO_CONTEXT_LARGE_FILE >= AUTO_CONTEXT_MAX_LINES`, which is an
/// unstated relationship between two independently-tunable constants three
/// lines apart. Lowering `AUTO_CONTEXT_LARGE_FILE` below `AUTO_CONTEXT_MAX_LINES`
/// would have turned the "large file" branch into an out-of-bounds panic on any
/// file whose line count fell in the gap. The clamp removes that dependency.
fn truncate_file_content(content: String) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    // Clamp so `lines[..take]` is always in bounds regardless of constant values.
    let take = AUTO_CONTEXT_MAX_LINES.min(total_lines);

    if total_lines > AUTO_CONTEXT_LARGE_FILE {
        let mut text: String = lines[..take].join("\n");
        text.push_str(&format!(
            "\n\n[… truncated — file has {} lines, showing first {}]",
            total_lines, AUTO_CONTEXT_MAX_LINES
        ));
        text
    } else if total_lines > AUTO_CONTEXT_MAX_LINES {
        let mut text: String = lines[..take].join("\n");
        text.push_str(&format!(
            "\n\n[… truncated at {} of {} lines]",
            AUTO_CONTEXT_MAX_LINES, total_lines
        ));
        text
    } else {
        content
    }
}

/// Build a compact signature block for the given file paths from the repo map.
///
/// Lists function/struct/enum/trait/type signatures for each matched file,
/// capped at `SIGNATURE_BLOCK_MAX_CHARS`. Returns `None` if no symbols found.
fn build_signature_block(repo_map: &[FileSymbols], matched_paths: &[String]) -> Option<String> {
    if matched_paths.is_empty() {
        return None;
    }

    let mut output = String::new();

    for path in matched_paths {
        let entry = match repo_map.iter().find(|f| &f.path == path) {
            Some(e) => e,
            None => continue,
        };
        if entry.symbols.is_empty() {
            continue;
        }

        let mut file_block = format!("{}\n", entry.path);
        for sym in &entry.symbols {
            let kind_label = match sym.kind {
                SymbolKind::Function => "fn",
                SymbolKind::Struct => "struct",
                SymbolKind::Enum => "enum",
                SymbolKind::Trait => "trait",
                SymbolKind::Interface => "interface",
                SymbolKind::Class => "class",
                SymbolKind::Type => "type",
                SymbolKind::Const => "const",
                SymbolKind::Impl => "impl",
                SymbolKind::Module => "mod",
                SymbolKind::Macro => "macro",
                SymbolKind::Namespace => "namespace",
            };
            file_block.push_str(&format!("  {kind_label} {}\n", sym.name));
        }

        if output.len() + file_block.len() > SIGNATURE_BLOCK_MAX_CHARS {
            if output.is_empty() {
                // First file already exceeds the cap — truncate it to fit
                let mut truncated = String::new();
                for line in file_block.lines() {
                    if truncated.len() + line.len() + 1 > SIGNATURE_BLOCK_MAX_CHARS {
                        truncated.push_str("  …\n");
                        break;
                    }
                    truncated.push_str(line);
                    truncated.push('\n');
                }
                output = truncated;
            } else {
                output.push_str("  …\n");
            }
            break;
        }
        output.push_str(&file_block);
    }

    if output.is_empty() {
        None
    } else {
        Some(output.trim_end().to_string())
    }
}

/// Get the set of files changed in the last few git commits.
///
/// Uses `git diff --name-only HEAD~5` to find recently-edited files.
/// Falls back gracefully if git is unavailable or there are fewer than 5 commits.
fn get_recent_git_files() -> std::collections::HashSet<String> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD~5"])
        .stderr(std::process::Stdio::null())
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        }
        _ => std::collections::HashSet::new(),
    }
}

/// Automatically identify project files relevant to a user prompt.
///
/// Returns `(path, content)` pairs for the top files with score ≥ `AUTO_CONTEXT_MIN_SCORE`.
/// Skips binary files, caps content at `AUTO_CONTEXT_MAX_LINES` lines, and excludes
/// paths that already appear in `recent_context` (to avoid re-injecting files the
/// conversation has already seen).
///
/// Files that were recently edited (in the last ~5 commits) receive a 1.5× score
/// boost, making them more likely to clear the threshold and appear in results.
pub fn auto_context_for_prompt(prompt: &str, recent_context: &[String]) -> Vec<(String, String)> {
    // Gate: skip slash commands, @-mention prompts, and very short follow-ups
    if prompt.starts_with('/') || prompt.contains('@') || prompt.len() < 20 {
        return Vec::new();
    }

    let keywords = tokenize_query(prompt);
    if keywords.is_empty() {
        return Vec::new();
    }

    let repo_map = build_repo_map(None, false);
    let mut results = score_files(&repo_map, &keywords);

    // Apply recency boost: files changed in the last ~5 commits get 1.5× score
    let recent_files = get_recent_git_files();
    if !recent_files.is_empty() {
        for r in &mut results {
            if recent_files.contains(&r.path) {
                r.score = r.score * RECENCY_BOOST_NUM / RECENCY_BOOST_DEN;
            }
        }
        // Re-sort after boosting since relative order may have changed
        results.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    }

    let mut out = Vec::new();
    let mut matched_paths = Vec::new();

    for r in &results {
        if matched_paths.len() >= AUTO_CONTEXT_MAX_FILES {
            break;
        }
        if r.score < AUTO_CONTEXT_MIN_SCORE {
            break; // results are sorted descending, so all remaining are lower
        }

        // Skip files already in the conversation context
        if recent_context.iter().any(|ctx| ctx.contains(&r.path)) {
            continue;
        }

        // Skip binary files
        if is_binary_extension(&r.path) {
            continue;
        }

        matched_paths.push(r.path.clone());
    }

    // Build a compact signature block for matched files and prepend it
    if let Some(sig_block) = build_signature_block(&repo_map, &matched_paths) {
        out.push((SIGNATURE_SENTINEL.to_string(), sig_block));
    }

    // Read file contents for matched files
    for path in &matched_paths {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let truncated = truncate_file_content(content);

        out.push((path.clone(), truncated));
    }

    out
}

/// Format auto-context results into a prefix for the user prompt.
///
/// Returns `None` if `files` is empty.
pub fn format_auto_context(files: &[(String, String)], original_prompt: &str) -> Option<String> {
    if files.is_empty() {
        return None;
    }

    // Build a risk-score map for high-risk file annotations.
    // Only files in the top 25% of risk scores get annotated.
    let risk_map = build_risk_annotation_map();

    // Build an emerging-risk map for files trending toward fragility.
    // At most 2 annotations to avoid noisy prompts.
    let emerging_map = build_emerging_risk_map(2);

    let mut parts = Vec::new();
    parts.push(
        "[Auto-context: yoyo identified these files as relevant to your prompt]\n".to_string(),
    );

    // Emit the signature block first if present
    for (path, content) in files {
        if path == SIGNATURE_SENTINEL {
            parts.push("--- Relevant signatures ---".to_string());
            parts.push(content.clone());
            parts.push(String::new());
        }
    }

    // Then emit file contents (skip the signature sentinel entry)
    for (path, content) in files {
        if path == SIGNATURE_SENTINEL {
            continue;
        }
        if let Some(&score) = risk_map.get(path.as_str()) {
            parts.push(format!("--- {path} --- (\u{26a0} risk: {score:.2})"));
        } else {
            parts.push(format!("--- {path} ---"));
        }
        // Annotate emerging-risk files with anticipatory warning
        if let Some(&momentum) = emerging_map.get(path.as_str()) {
            parts.push(format!(
                "\u{26a1} Emerging risk: {path} — changing {momentum:.1}× faster than usual. Extra care advised."
            ));
        }
        parts.push(content.clone());
    }

    parts.push(String::new());
    parts.push("[Your prompt]:".to_string());
    parts.push(original_prompt.to_string());

    Some(parts.join("\n"))
}

/// Build a map of path → risk score for files in the top 25% of risk.
///
/// Used by `format_auto_context` to annotate high-risk file headers.
fn build_risk_annotation_map() -> std::collections::HashMap<String, f64> {
    let risks = crate::commands_risk::compute_file_risk_scores();
    if risks.is_empty() {
        return std::collections::HashMap::new();
    }
    // Top 25%: take the top quarter (at least 1)
    let cutoff_index = (risks.len() / 4).max(1);
    risks
        .into_iter()
        .take(cutoff_index)
        .map(|r| (r.path, r.score))
        .collect()
}

/// Build a map of path → momentum for files with accelerating risk.
///
/// Used by `format_auto_context` to annotate emerging-risk file headers.
/// Returns at most `max` entries to avoid noisy prompts.
fn build_emerging_risk_map(max: usize) -> std::collections::HashMap<String, f64> {
    let risks = crate::commands_risk::compute_file_risk_scores();
    if risks.is_empty() {
        return std::collections::HashMap::new();
    }
    let emerging = crate::commands_risk::detect_emerging_risks(&risks);
    emerging
        .into_iter()
        .take(max)
        .map(|e| (e.path, e.momentum))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn detect_project_type_java_maven() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pom.xml"), "<project/>").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Java);
    }

    #[test]
    fn detect_project_type_java_gradle() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("build.gradle"), "plugins {}").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Java);
    }

    #[test]
    fn detect_project_type_java_gradle_kts() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("build.gradle.kts"), "plugins {}").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Java);
    }

    #[test]
    fn detect_project_type_ruby() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Gemfile"), "source 'https://rubygems.org'").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Ruby);
    }

    #[test]
    fn detect_project_type_cpp_cmake() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.10)",
        )
        .unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Cpp);
    }

    #[test]
    fn detect_project_type_cmake_over_makefile() {
        // CMakeLists.txt should detect as Cpp, not Make (even if Makefile exists too)
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CMakeLists.txt"), "project(test)").unwrap();
        fs::write(dir.path().join("Makefile"), "all:").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Cpp);
    }

    #[test]
    fn test_project_type_hints_rust() {
        let hints = project_type_hints(&ProjectType::Rust).unwrap();
        assert!(hints.contains("cargo"));
    }

    #[test]
    fn test_project_type_hints_python() {
        let hints = project_type_hints(&ProjectType::Python).unwrap();
        assert!(hints.contains("pytest"));
    }

    #[test]
    fn test_project_type_hints_node() {
        let hints = project_type_hints(&ProjectType::Node).unwrap();
        assert!(hints.contains("npm") || hints.contains("package.json"));
    }

    #[test]
    fn test_project_type_hints_unknown() {
        assert!(project_type_hints(&ProjectType::Unknown).is_none());
    }

    #[test]
    fn test_project_type_hints_all_short() {
        let all_types = [
            ProjectType::Rust,
            ProjectType::Node,
            ProjectType::Python,
            ProjectType::Go,
            ProjectType::Java,
            ProjectType::Ruby,
            ProjectType::Cpp,
            ProjectType::Make,
        ];
        for pt in &all_types {
            let hints = project_type_hints(pt).unwrap();
            assert!(
                hints.len() < 500,
                "{:?} hints too long: {} chars",
                pt,
                hints.len()
            );
        }
    }

    // ── ProjectType Display ──────────────────────────────────────────

    #[test]
    fn project_type_display() {
        assert_eq!(format!("{}", ProjectType::Rust), "Rust (Cargo)");
        assert_eq!(format!("{}", ProjectType::Node), "Node.js (npm)");
        assert_eq!(format!("{}", ProjectType::Python), "Python");
        assert_eq!(format!("{}", ProjectType::Go), "Go");
        assert_eq!(format!("{}", ProjectType::Java), "Java");
        assert_eq!(format!("{}", ProjectType::Ruby), "Ruby");
        assert_eq!(format!("{}", ProjectType::Cpp), "C/C++ (CMake)");
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

    #[test]
    fn detect_ai_config_files_none() {
        let dir = TempDir::new().unwrap();
        let found = detect_ai_config_files(dir.path());
        assert!(found.is_empty());
    }

    #[test]
    fn detect_ai_config_files_cursorrules() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".cursorrules"), "some rules").unwrap();
        let found = detect_ai_config_files(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], (".cursorrules", "Cursor"));
    }

    #[test]
    fn detect_ai_config_files_multiple() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("AGENTS.md"), "# Agents").unwrap();
        fs::write(dir.path().join(".cursorrules"), "rules").unwrap();
        let found = detect_ai_config_files(dir.path());
        assert_eq!(found.len(), 2);
        let paths: Vec<&str> = found.iter().map(|(p, _)| *p).collect();
        assert!(paths.contains(&"AGENTS.md"));
        assert!(paths.contains(&".cursorrules"));
    }

    #[test]
    fn detect_ai_config_files_claude_md() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "# Claude instructions").unwrap();
        let found = detect_ai_config_files(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], ("CLAUDE.md", "Claude Code"));
    }

    #[test]
    fn detect_ai_config_files_copilot() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join(".github")).unwrap();
        fs::write(
            dir.path().join(".github/copilot-instructions.md"),
            "instructions",
        )
        .unwrap();
        let found = detect_ai_config_files(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1, "GitHub Copilot");
    }

    #[test]
    fn init_content_with_cursorrules_has_ai_config_section() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".cursorrules"), "rules").unwrap();
        let content = generate_init_content(dir.path());
        assert!(content.contains("## Other AI Tool Configs"));
        assert!(content.contains("`.cursorrules` (Cursor)"));
        assert!(content.contains("yoyo reads these automatically"));
    }

    #[test]
    fn init_content_no_ai_configs_omits_section() {
        let dir = TempDir::new().unwrap();
        let content = generate_init_content(dir.path());
        assert!(!content.contains("Other AI Tool Configs"));
    }

    #[test]
    fn init_content_with_multiple_ai_configs_lists_both() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("AGENTS.md"), "# Agents").unwrap();
        fs::write(dir.path().join(".cursorrules"), "rules").unwrap();
        let content = generate_init_content(dir.path());
        assert!(content.contains("## Other AI Tool Configs"));
        assert!(content.contains("`.cursorrules` (Cursor)"));
        assert!(content.contains("`AGENTS.md` (Gemini / generic agents)"));
    }

    #[test]
    fn init_content_claude_md_labeled_correctly() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "# Claude").unwrap();
        let content = generate_init_content(dir.path());
        assert!(content.contains("`CLAUDE.md` (Claude Code)"));
    }

    // ── parse_prompt_sections ──────────────────────────────────────────

    #[test]
    fn test_context_system_sections() {
        let prompt = "# System Instructions\nYou are helpful.\nBe concise.\n\n\
                      ## Tools\nYou have bash.\nYou have read_file.\nYou have write_file.\n\n\
                      # Project Context\nThis is a Rust project.\n";

        let sections = parse_prompt_sections(prompt);
        assert_eq!(sections.len(), 3);

        assert_eq!(sections[0].name, "System Instructions");
        assert_eq!(sections[0].header_level, 1);
        assert!(sections[0].lines.iter().any(|l| l.contains("helpful")));

        assert_eq!(sections[1].name, "Tools");
        assert_eq!(sections[1].header_level, 2);
        assert!(sections[1].lines.iter().any(|l| l.contains("bash")));

        assert_eq!(sections[2].name, "Project Context");
        assert_eq!(sections[2].header_level, 1);
        assert!(sections[2].lines.iter().any(|l| l.contains("Rust")));
    }

    #[test]
    fn test_context_system_empty_prompt() {
        let sections = parse_prompt_sections("");
        assert!(sections.is_empty());
    }

    #[test]
    fn test_context_system_no_headers() {
        let prompt = "Just some plain text\nwith multiple lines.\n";
        let sections = parse_prompt_sections(prompt);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].name, "(preamble)");
        assert_eq!(sections[0].header_level, 0);
        assert_eq!(sections[0].lines.len(), 2);
    }

    #[test]
    fn test_context_system_preamble_before_header() {
        let prompt = "Some preamble text.\n# First Section\nContent here.\n";
        let sections = parse_prompt_sections(prompt);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].name, "(preamble)");
        assert_eq!(sections[1].name, "First Section");
    }

    #[test]
    fn test_context_system_consecutive_headers() {
        let prompt = "# One\n# Two\nContent for two.\n";
        let sections = parse_prompt_sections(prompt);
        // "# One" creates section with empty lines, then "# Two" flushes it
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].name, "One");
        assert!(sections[0].lines.is_empty());
        assert_eq!(sections[1].name, "Two");
        assert!(!sections[1].lines.is_empty());
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        // Rough check: 400 chars ~= 100 tokens
        let text = "a".repeat(400);
        assert_eq!(estimate_tokens(&text), 100);
    }

    #[test]
    fn test_context_default_behavior() {
        // Verify handle_context with empty input doesn't panic
        // (it just calls show_project_context_files which prints)
        let agent = yoagent::Agent::from_provider(
            yoagent::provider::AnthropicProvider,
            yoagent::provider::ModelConfig::mock(),
        )
        .with_system_prompt("test")
        .with_api_key("test-key");
        handle_context("/context", "", &agent);
    }

    #[test]
    fn test_context_system_subcommand() {
        // Verify handle_context with "system" doesn't panic
        let agent = yoagent::Agent::from_provider(
            yoagent::provider::AnthropicProvider,
            yoagent::provider::ModelConfig::mock(),
        )
        .with_system_prompt("test")
        .with_api_key("test-key");
        handle_context("/context system", "# Test\nHello world.\n", &agent);
    }

    #[test]
    fn test_context_subcommands_list() {
        let subs = context_subcommands();
        assert!(subs.contains(&"system"));
        assert!(subs.contains(&"tokens"));
    }

    #[test]
    fn test_context_tokens_subcommand() {
        // Verify handle_context with "tokens" doesn't panic
        let agent = yoagent::Agent::from_provider(
            yoagent::provider::AnthropicProvider,
            yoagent::provider::ModelConfig::mock(),
        )
        .with_system_prompt("You are a test assistant.")
        .with_api_key("test-key");
        handle_context("/context tokens", "You are a test assistant.", &agent);
    }

    #[test]
    fn test_context_tokens_section_breakdown() {
        // Multi-section system prompt should show section breakdown without panic
        let prompt = "# Project context\nThis is the project.\nIt has details.\n\n\
                       ## Git status\nOn branch main\n\n\
                       ## Recently changed\nfile1.rs\nfile2.rs\n";
        let agent = yoagent::Agent::from_provider(
            yoagent::provider::AnthropicProvider,
            yoagent::provider::ModelConfig::mock(),
        )
        .with_system_prompt(prompt)
        .with_api_key("test-key");
        // Should not panic and should exercise the section breakdown path
        handle_context("/context tokens", prompt, &agent);
    }

    #[test]
    fn test_context_tokens_single_section_no_breakdown() {
        // Single-section prompt should NOT show breakdown (just the total)
        let prompt = "You are a helpful assistant.";
        let agent = yoagent::Agent::from_provider(
            yoagent::provider::AnthropicProvider,
            yoagent::provider::ModelConfig::mock(),
        )
        .with_system_prompt(prompt)
        .with_api_key("test-key");
        handle_context("/context tokens", prompt, &agent);
    }

    #[test]
    fn test_section_breakdown_token_counts() {
        // Verify section breakdown produces valid token estimates
        let prompt =
            "# Section A\nShort content.\n\n# Section B\nLonger content with more text here.\n";
        let sections = parse_prompt_sections(prompt);
        assert_eq!(sections.len(), 2);
        for section in &sections {
            let section_text = section.lines.join("\n");
            let full = format!("{}\n{}", section.name, section_text);
            let tokens = estimate_tokens(&full);
            assert!(tokens > 0, "Each section should have >0 tokens");
        }
        // Sum of section tokens should be roughly close to total
        let total = estimate_tokens(prompt);
        assert!(total > 0);
    }

    // ── tests migrated from commands.rs (Issue #260) ─────────────────

    #[test]
    fn test_detect_project_type_rust() {
        // Use CARGO_MANIFEST_DIR to avoid race with set_current_dir in other tests
        let cwd = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(detect_project_type(&cwd), ProjectType::Rust);
    }

    #[test]
    fn test_detect_project_type_node() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        std::fs::write(tmp.join("package.json"), "{}").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Node);
    }

    #[test]
    fn test_detect_project_type_python_pyproject() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        std::fs::write(tmp.join("pyproject.toml"), "[project]").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Python);
    }

    #[test]
    fn test_detect_project_type_python_setup_py() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        std::fs::write(tmp.join("setup.py"), "").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Python);
    }

    #[test]
    fn test_detect_project_type_go() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        std::fs::write(tmp.join("go.mod"), "module example.com/test").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Go);
    }

    #[test]
    fn test_detect_project_type_makefile() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        std::fs::write(tmp.join("Makefile"), "test:\n\techo ok").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Make);
    }

    #[test]
    fn test_detect_project_type_unknown() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        // Empty dir — no marker files
        assert_eq!(detect_project_type(&tmp), ProjectType::Unknown);
    }

    #[test]
    fn test_detect_project_type_priority_rust_over_makefile() {
        // If both Cargo.toml and Makefile exist, Rust wins
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        std::fs::write(tmp.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(tmp.join("Makefile"), "test:").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Rust);
    }

    #[test]
    fn test_project_type_display() {
        assert_eq!(format!("{}", ProjectType::Rust), "Rust (Cargo)");
        assert_eq!(format!("{}", ProjectType::Node), "Node.js (npm)");
        assert_eq!(format!("{}", ProjectType::Python), "Python");
        assert_eq!(format!("{}", ProjectType::Go), "Go");
        assert_eq!(format!("{}", ProjectType::Make), "Makefile");
        assert_eq!(format!("{}", ProjectType::Unknown), "Unknown");
    }

    #[test]
    fn test_scan_important_files_in_current_project() {
        let cwd = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let files = scan_important_files(&cwd);
        // This is a Rust project, so Cargo.toml should be found
        assert!(
            files.contains(&"Cargo.toml".to_string()),
            "Should find Cargo.toml: {files:?}"
        );
    }

    #[test]
    fn test_scan_important_files_empty_dir() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        let files = scan_important_files(&tmp);
        assert!(files.is_empty(), "Empty dir should have no important files");
    }

    #[test]
    fn test_scan_important_files_with_readme() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        std::fs::write(tmp.join("README.md"), "# Hello").unwrap();
        std::fs::write(tmp.join("package.json"), "{}").unwrap();
        let files = scan_important_files(&tmp);
        assert!(
            files.contains(&"README.md".to_string()),
            "Should find README.md"
        );
        assert!(
            files.contains(&"package.json".to_string()),
            "Should find package.json"
        );
    }

    #[test]
    fn test_scan_important_dirs_in_current_project() {
        let cwd = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let dirs = scan_important_dirs(&cwd);
        // This project has src/
        assert!(
            dirs.contains(&"src".to_string()),
            "Should find src/ dir: {dirs:?}"
        );
    }

    #[test]
    fn test_scan_important_dirs_empty_dir() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        let dirs = scan_important_dirs(&tmp);
        assert!(dirs.is_empty(), "Empty dir should have no important dirs");
    }

    #[test]
    fn test_scan_important_dirs_with_subdirs() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        let _ = std::fs::create_dir_all(tmp.join("src"));
        let _ = std::fs::create_dir_all(tmp.join("tests"));
        let _ = std::fs::create_dir_all(tmp.join("docs"));
        let dirs = scan_important_dirs(&tmp);
        assert!(dirs.contains(&"src".to_string()), "Should find src/");
        assert!(dirs.contains(&"tests".to_string()), "Should find tests/");
        assert!(dirs.contains(&"docs".to_string()), "Should find docs/");
    }

    #[test]
    fn test_build_commands_for_rust() {
        let cmds = build_commands_for_project(&ProjectType::Rust);
        assert!(!cmds.is_empty(), "Rust should have build commands");
        let labels: Vec<&str> = cmds.iter().map(|(l, _)| *l).collect();
        assert!(labels.contains(&"Build"), "Should have Build command");
        assert!(labels.contains(&"Test"), "Should have Test command");
        assert!(labels.contains(&"Lint"), "Should have Lint command");
    }

    #[test]
    fn test_build_commands_for_node() {
        let cmds = build_commands_for_project(&ProjectType::Node);
        assert!(!cmds.is_empty(), "Node should have build commands");
        let labels: Vec<&str> = cmds.iter().map(|(l, _)| *l).collect();
        assert!(labels.contains(&"Test"), "Should have Test command");
    }

    #[test]
    fn test_build_commands_for_unknown() {
        let cmds = build_commands_for_project(&ProjectType::Unknown);
        assert!(
            cmds.is_empty(),
            "Unknown project should have no build commands"
        );
    }

    #[test]
    fn test_detect_project_name_rust() {
        // Use CARGO_MANIFEST_DIR to avoid race with set_current_dir in other tests
        let cwd = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let name = detect_project_name(&cwd);
        assert_eq!(
            name, "yoyo-agent",
            "Should detect project name 'yoyo-agent' from Cargo.toml"
        );
    }

    #[test]
    fn test_detect_project_name_fallback_to_dir() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        let name = detect_project_name(&tmp);
        assert!(
            name.starts_with("yoyo_test_"),
            "Should fall back to directory name, got: {name}"
        );
    }

    #[test]
    fn test_detect_project_name_from_readme() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        std::fs::write(tmp.join("README.md"), "# My Awesome Project\n\nSome text.").unwrap();
        let name = detect_project_name(&tmp);
        assert_eq!(
            name, "My Awesome Project",
            "Should extract name from README title"
        );
    }

    #[test]
    fn test_detect_project_name_from_package_json() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        std::fs::write(
            tmp.join("package.json"),
            "{\n  \"name\": \"cool-app\",\n  \"version\": \"1.0.0\"\n}",
        )
        .unwrap();
        let name = detect_project_name(&tmp);
        assert_eq!(name, "cool-app", "Should extract name from package.json");
    }

    #[test]
    fn test_generate_init_content_rust_project() {
        let cwd = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let content = generate_init_content(&cwd);
        // Should contain project name
        assert!(
            content.contains("yoyo"),
            "Should contain project name: {}",
            crate::format::safe_truncate(&content, 200)
        );
        // Should detect Rust
        assert!(content.contains("Rust"), "Should mention Rust project type");
        // Should have build commands
        assert!(
            content.contains("cargo build"),
            "Should include cargo build command"
        );
        assert!(
            content.contains("cargo test"),
            "Should include cargo test command"
        );
        // Should have sections
        assert!(
            content.contains("## Build & Test"),
            "Should have Build & Test section"
        );
        assert!(
            content.contains("## Important Files"),
            "Should have Important Files section"
        );
        assert!(
            content.contains("## Coding Conventions"),
            "Should have Coding Conventions section"
        );
        // Should list Cargo.toml as important file
        assert!(
            content.contains("Cargo.toml"),
            "Should list Cargo.toml as important"
        );
        // Should list src/ as important dir
        assert!(
            content.contains("`src/`"),
            "Should list src/ as important dir"
        );
    }

    #[test]
    fn test_generate_init_content_empty_dir() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        let content = generate_init_content(&tmp);
        // Should still have sections even for empty/unknown project
        assert!(content.contains("# Project Context"));
        assert!(content.contains("## About This Project"));
        assert!(content.contains("## Build & Test"));
        assert!(content.contains("## Coding Conventions"));
        assert!(content.contains("## Important Files"));
    }

    #[test]
    fn test_generate_init_content_node_project() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("yoyo_test_")
            .tempdir()
            .unwrap();
        let tmp = tmp_dir.path().to_path_buf();
        std::fs::write(
            tmp.join("package.json"),
            "{\n  \"name\": \"my-app\",\n  \"version\": \"1.0.0\"\n}",
        )
        .unwrap();
        let _ = std::fs::create_dir_all(tmp.join("src"));
        let content = generate_init_content(&tmp);
        assert!(
            content.contains("my-app"),
            "Should detect project name from package.json"
        );
        assert!(content.contains("Node"), "Should detect Node project type");
        assert!(content.contains("npm"), "Should include npm commands");
    }

    // ── Tests moved from commands.rs — /docs command tests ──────────────

    #[test]
    fn test_docs_command_recognized() {
        use crate::commands::{is_unknown_command, KNOWN_COMMANDS};
        assert!(!is_unknown_command("/docs"));
        assert!(!is_unknown_command("/docs serde"));
        assert!(!is_unknown_command("/docs tokio"));
        assert!(
            KNOWN_COMMANDS.contains(&"/docs"),
            "/docs should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_docs_command_matching() {
        // /docs should match exact or with space, not /docstring etc.
        let docs_matches = |s: &str| s == "/docs" || s.starts_with("/docs ");
        assert!(docs_matches("/docs"));
        assert!(docs_matches("/docs serde"));
        assert!(docs_matches("/docs tokio-runtime"));
        assert!(!docs_matches("/docstring"));
        assert!(!docs_matches("/docsify"));
    }

    #[test]
    fn test_docs_crate_arg_extraction() {
        let input = "/docs serde";
        let crate_name = input.trim_start_matches("/docs ").trim();
        assert_eq!(crate_name, "serde");

        let input2 = "/docs tokio-runtime";
        let crate_name2 = input2.trim_start_matches("/docs ").trim();
        assert_eq!(crate_name2, "tokio-runtime");

        // Bare /docs has empty after stripping
        let input_bare = "/docs";
        assert_eq!(input_bare, "/docs");
        assert!(!input_bare.starts_with("/docs "));
    }

    #[test]
    fn test_context_files_subcommand_in_list() {
        assert!(
            CONTEXT_SUBCOMMANDS.contains(&"files"),
            "CONTEXT_SUBCOMMANDS should contain 'files'"
        );
    }

    #[test]
    fn test_show_context_files_no_panic() {
        // Smoke test: calling with an empty agent shouldn't panic
        let agent = yoagent::Agent::from_provider(
            yoagent::provider::AnthropicProvider,
            yoagent::provider::ModelConfig::mock(),
        )
        .with_system_prompt("test")
        .with_api_key("test-key");
        show_context_files(&agent);
    }

    #[test]
    fn test_context_files_dispatch() {
        // Verify handle_context routes "files" correctly (shouldn't panic)
        let agent = yoagent::Agent::from_provider(
            yoagent::provider::AnthropicProvider,
            yoagent::provider::ModelConfig::mock(),
        )
        .with_system_prompt("test")
        .with_api_key("test-key");
        handle_context("/context files", "", &agent);
    }

    #[test]
    fn test_extract_context_files_empty() {
        let messages: Vec<yoagent::types::AgentMessage> = vec![];
        let result = extract_context_files(&messages);
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_context_files_with_tool_calls() {
        use yoagent::types::*;

        let messages = vec![
            AgentMessage::Llm(
                Message::assistant(
                    vec![
                        Content::tool_call(
                            "1",
                            "read_file",
                            serde_json::json!({"path": "src/main.rs"}),
                        ),
                        Content::tool_call(
                            "2",
                            "edit_file",
                            serde_json::json!({"path": "src/tools.rs", "old_text": "a", "new_text": "b"}),
                        ),
                        Content::tool_call(
                            "3",
                            "write_file",
                            serde_json::json!({"path": "src/new.rs", "content": "fn main() {}"}),
                        ),
                    ],
                    StopReason::ToolUse,
                    "test",
                    "test",
                    Usage::default(),
                )
                .with_timestamp(0),
            ),
            AgentMessage::Llm(
                Message::assistant(
                    vec![
                        Content::tool_call(
                            "4",
                            "list_files",
                            serde_json::json!({"path": "src/"}),
                        ),
                        Content::tool_call(
                            "5",
                            "search",
                            serde_json::json!({"pattern": "TODO", "path": "src/"}),
                        ),
                        // Duplicate read — should be deduplicated
                        Content::tool_call(
                            "6",
                            "read_file",
                            serde_json::json!({"path": "src/main.rs"}),
                        ),
                    ],
                    StopReason::ToolUse,
                    "test",
                    "test",
                    Usage::default(),
                )
                .with_timestamp(0),
            ),
        ];

        let result = extract_context_files(&messages);

        // Check read files — deduplicated
        let read = result.get(&FileAction::Read).unwrap();
        assert_eq!(read.len(), 1);
        assert!(read.contains("src/main.rs"));

        // Check edited
        let edited = result.get(&FileAction::Edited).unwrap();
        assert!(edited.contains("src/tools.rs"));

        // Check written
        let written = result.get(&FileAction::Written).unwrap();
        assert!(written.contains("src/new.rs"));

        // Check listed
        let listed = result.get(&FileAction::Listed).unwrap();
        assert!(listed.contains("src/"));

        // Check searched
        let searched = result.get(&FileAction::Searched).unwrap();
        assert!(searched.contains("src/"));
    }

    #[test]
    fn test_extract_context_files_skips_non_file_tools() {
        use yoagent::types::*;

        let messages = vec![AgentMessage::Llm(
            Message::assistant(
                vec![
                    Content::tool_call("1", "bash", serde_json::json!({"command": "ls"})),
                    Content::tool_call("2", "todo", serde_json::json!({"action": "list"})),
                ],
                StopReason::Stop,
                "test",
                "test",
                Usage::default(),
            )
            .with_timestamp(0),
        )];

        let result = extract_context_files(&messages);
        assert!(result.is_empty(), "Non-file tools should be skipped");
    }

    #[test]
    fn test_extract_context_files_search_without_path() {
        use yoagent::types::*;

        // search tool call with no path (searches cwd) — should not add empty path
        let messages = vec![AgentMessage::Llm(
            Message::assistant(
                vec![Content::tool_call(
                    "1",
                    "search",
                    serde_json::json!({"pattern": "TODO"}),
                )],
                StopReason::ToolUse,
                "test",
                "test",
                Usage::default(),
            )
            .with_timestamp(0),
        )];

        let result = extract_context_files(&messages);
        // search without a path shouldn't produce an entry
        assert!(
            !result.contains_key(&FileAction::Searched),
            "search without path should not create entry"
        );
    }

    #[test]
    fn test_file_action_labels_and_icons() {
        assert_eq!(FileAction::Read.label(), "Read");
        assert_eq!(FileAction::Edited.label(), "Edited");
        assert_eq!(FileAction::Written.label(), "Written");
        assert_eq!(FileAction::Listed.label(), "Listed");
        assert_eq!(FileAction::Searched.label(), "Searched");

        // Icons should be non-empty
        assert!(!FileAction::Read.icon().is_empty());
        assert!(!FileAction::Edited.icon().is_empty());
        assert!(!FileAction::Written.icon().is_empty());
        assert!(!FileAction::Listed.icon().is_empty());
        assert!(!FileAction::Searched.icon().is_empty());
    }

    // --- /context relevant tests ---

    #[test]
    fn test_tokenize_query_filters_stop_words() {
        let tokens = tokenize_query("fix the web search");
        assert_eq!(tokens, vec!["fix", "web", "search"]);
    }

    #[test]
    fn test_tokenize_query_empty_after_stop_words() {
        // All stop words → empty result
        let tokens = tokenize_query("the a an to for");
        assert!(tokens.is_empty());

        // Completely empty query
        let tokens2 = tokenize_query("");
        assert!(tokens2.is_empty());
    }

    // --- decompose_identifier tests ---

    #[test]
    fn test_decompose_snake_case() {
        assert_eq!(
            decompose_identifier("agent_builder"),
            vec!["agent", "builder"]
        );
        assert_eq!(
            decompose_identifier("auto_context_for_prompt"),
            vec!["auto", "context", "for", "prompt"]
        );
    }

    #[test]
    fn test_decompose_camel_case() {
        assert_eq!(
            decompose_identifier("StreamingBashTool"),
            vec!["streaming", "bash", "tool"]
        );
        assert_eq!(decompose_identifier("AgentConfig"), vec!["agent", "config"]);
    }

    #[test]
    fn test_decompose_all_caps_acronym() {
        // "HTML" stays as one word
        assert_eq!(decompose_identifier("HTML"), vec!["html"]);
        // "HTMLParser" → "html", "parser"
        assert_eq!(decompose_identifier("HTMLParser"), vec!["html", "parser"]);
        // "getURLValue" → "get", "url", "value"
        assert_eq!(
            decompose_identifier("getURLValue"),
            vec!["get", "url", "value"]
        );
    }

    #[test]
    fn test_decompose_single_word() {
        assert_eq!(decompose_identifier("agent"), vec!["agent"]);
        assert_eq!(decompose_identifier("main"), vec!["main"]);
    }

    #[test]
    fn test_decompose_mixed_separators() {
        // Path-like input
        assert_eq!(
            decompose_identifier("src/agent_builder.rs"),
            vec!["src", "agent", "builder", "rs"]
        );
        // Hyphenated
        assert_eq!(decompose_identifier("web-search"), vec!["web", "search"]);
    }

    #[test]
    fn test_decompose_with_numbers() {
        assert_eq!(
            decompose_identifier("phase2Handler"),
            vec!["phase2", "handler"]
        );
    }

    #[test]
    fn test_decompose_empty() {
        let result = decompose_identifier("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_tokenize_query_decomposes_identifiers() {
        // A query containing a camelCase identifier should be decomposed
        let tokens = tokenize_query("fix StreamingBashTool");
        assert_eq!(tokens, vec!["fix", "streaming", "bash", "tool"]);

        // A query containing a snake_case identifier should be decomposed
        let tokens2 = tokenize_query("agent_builder config");
        assert_eq!(tokens2, vec!["agent", "builder", "config"]);
    }

    #[test]
    fn test_score_files_empty_keywords() {
        use crate::symbols::{Symbol, SymbolKind};
        let files = vec![FileSymbols {
            path: "src/main.rs".into(),
            lines: 100,
            symbols: vec![Symbol {
                name: "main".into(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            }],
        }];
        let results = score_files(&files, &[]);
        assert!(
            results.is_empty(),
            "Empty keywords should produce no matches"
        );
    }

    #[test]
    fn test_score_files_ranking() {
        use crate::symbols::{Symbol, SymbolKind};

        let files = vec![
            FileSymbols {
                path: "src/commands_web.rs".into(),
                lines: 200,
                symbols: vec![Symbol {
                    name: "web_search".into(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 10,
                }],
            },
            FileSymbols {
                path: "src/main.rs".into(),
                lines: 50,
                symbols: vec![Symbol {
                    name: "main".into(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 1,
                }],
            },
        ];

        let keywords = tokenize_query("web search");
        let results = score_files(&files, &keywords);

        // commands_web.rs should score higher: "web" matches path component (3x)
        // + "web" matches symbol "web_search" (2x) + "search" matches symbol (2x)
        // + "search" might match path component — either way it's higher than main.rs
        assert!(!results.is_empty());
        assert_eq!(results[0].path, "src/commands_web.rs");

        // main.rs should have score 0 (no keyword matches) → not in results,
        // or at minimum score lower
        if results.len() > 1 {
            assert!(
                results[0].score > results[1].score,
                "commands_web.rs should score higher than main.rs"
            );
        }
    }

    #[test]
    fn test_handle_context_relevant_no_panic() {
        // Running against the actual yoyo repo should not panic
        handle_context_relevant("web search");
    }

    // --- auto_context_for_prompt tests ---

    #[test]
    fn test_auto_context_web_search_returns_relevant_files() {
        // A prompt about "web search" should return web-related files from this repo.
        // With recency boosting, recently-edited files with matching symbols (e.g.,
        // tools.rs containing WebSearchTool) may rank higher than commands_web.rs.
        let results =
            auto_context_for_prompt("how does the web search tool work in this project", &[]);
        // Should return at least one file, and it should be web-related
        assert!(
            !results.is_empty(),
            "web search query should return relevant files"
        );
        let paths: Vec<&str> = results.iter().map(|r| r.0.as_str()).collect();
        let has_web_path = paths.iter().any(|p| p.contains("web"));
        // Also check the signature block for web-related symbols (e.g. WebSearchTool
        // in tools.rs), since recency boosting may promote files whose path doesn't
        // contain "web" but whose symbols do.
        let sig_has_web = results
            .iter()
            .find(|(p, _)| p == SIGNATURE_SENTINEL)
            .is_some_and(|(_, content)| content.to_lowercase().contains("web"));
        assert!(
            has_web_path || sig_has_web,
            "results should include a web-related file or web symbols in signatures, got: {:?}",
            paths
        );
        // Should return at most MAX_FILES file entries (plus optional signature block)
        let file_count = results
            .iter()
            .filter(|(p, _)| p != SIGNATURE_SENTINEL)
            .count();
        assert!(file_count <= AUTO_CONTEXT_MAX_FILES);
        // Each result should have non-empty content
        for (path, content) in &results {
            assert!(!path.is_empty());
            assert!(!content.is_empty());
        }
    }

    #[test]
    fn test_auto_context_empty_and_short_queries_return_empty() {
        // Empty prompt
        assert!(auto_context_for_prompt("", &[]).is_empty());
        // Very short prompt (< 20 chars)
        assert!(auto_context_for_prompt("fix the bug", &[]).is_empty());
        // Prompt of only stop words (tokens empty after filtering)
        assert!(auto_context_for_prompt("the a an to for with and or but", &[]).is_empty());
    }

    #[test]
    fn test_auto_context_slash_commands_return_empty() {
        // Slash commands should be skipped entirely
        assert!(auto_context_for_prompt("/help me with web search stuff", &[]).is_empty());
        assert!(auto_context_for_prompt("/add src/main.rs to context", &[]).is_empty());
        // @-mention prompts should also be skipped
        assert!(
            auto_context_for_prompt("look at @src/main.rs and fix the issue there", &[]).is_empty()
        );
    }

    #[test]
    fn test_auto_context_threshold_filtering() {
        // A query with a very obscure/nonsense keyword should return nothing
        // because no files will score >= AUTO_CONTEXT_MIN_SCORE
        let results =
            auto_context_for_prompt("xyzzyplugh frobnicate the glorpweasel machinery", &[]);
        assert!(
            results.is_empty(),
            "nonsense keywords should not match any files above threshold, got: {:?}",
            results.iter().map(|r| &r.0).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_auto_context_skips_files_in_context() {
        // First get results without context filtering
        let results_all =
            auto_context_for_prompt("how does the web search tool work in this project", &[]);
        if results_all.is_empty() {
            return; // skip if repo map doesn't find matches (unlikely in this repo)
        }
        // Find the first real file path (skip the signature sentinel)
        let first_file = results_all.iter().find(|(p, _)| p != SIGNATURE_SENTINEL);
        let first_path = match first_file {
            Some((p, _)) => p.clone(),
            None => return, // only signatures, no files to test
        };
        let recent = vec![format!("I already loaded {}", first_path)];
        let results_filtered =
            auto_context_for_prompt("how does the web search tool work in this project", &recent);
        // The first file should no longer appear
        let filtered_paths: Vec<&str> = results_filtered
            .iter()
            .filter(|(p, _)| p != SIGNATURE_SENTINEL)
            .map(|r| r.0.as_str())
            .collect();
        assert!(
            !filtered_paths.contains(&first_path.as_str()),
            "file already in context should be skipped: {}",
            first_path
        );
    }

    #[test]
    fn test_format_auto_context_empty() {
        assert!(format_auto_context(&[], "hello").is_none());
    }

    // --- truncate_file_content: panic-proofing the auto-context line slice ---
    //
    // Day 146 (dream chosen-experiment): guessed a multi-byte slice panic in the
    // auto-context path. The specific guess was wrong (the char/line iteration is
    // UTF-8 safe), but the read surfaced a latent slice-index fragility: the
    // `lines[..AUTO_CONTEXT_MAX_LINES]` slice was only in-bounds because of the
    // *unstated* invariant `AUTO_CONTEXT_LARGE_FILE >= AUTO_CONTEXT_MAX_LINES`
    // between two independently-tunable constants three lines apart. These tests
    // pin the now-clamped behavior so a future retune can't reintroduce the panic.

    #[test]
    fn test_truncate_file_content_short_file_unchanged() {
        let content = "line one\nline two\nline three".to_string();
        assert_eq!(truncate_file_content(content.clone()), content);
    }

    #[test]
    fn test_truncate_file_content_exactly_max_lines_unchanged() {
        // Exactly AUTO_CONTEXT_MAX_LINES lines: not "greater than", so unchanged.
        let content = (0..AUTO_CONTEXT_MAX_LINES)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = truncate_file_content(content.clone());
        assert_eq!(out, content);
        assert!(!out.contains("truncated"));
    }

    #[test]
    fn test_truncate_file_content_just_over_max_truncates() {
        let content = (0..AUTO_CONTEXT_MAX_LINES + 5)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = truncate_file_content(content);
        assert!(out.contains("truncated"));
        // Body keeps exactly the first MAX_LINES lines.
        assert!(out.starts_with("line 0\n"));
        assert!(out.contains(&format!("line {}", AUTO_CONTEXT_MAX_LINES - 1)));
        assert!(!out.contains(&format!("line {}\n", AUTO_CONTEXT_MAX_LINES)));
    }

    #[test]
    fn test_truncate_file_content_large_file_note() {
        let content = (0..AUTO_CONTEXT_LARGE_FILE + 10)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = truncate_file_content(content);
        assert!(out.contains("file has"));
        assert!(out.contains("showing first"));
    }

    #[test]
    fn test_truncate_file_content_multibyte_content_no_panic() {
        // Original Day-146 hypothesis, pinned: file content is arbitrary Unicode.
        // Build a long file whose truncation boundary sits amid multi-byte chars.
        let content = (0..AUTO_CONTEXT_MAX_LINES + 20)
            .map(|i| format!("✓ 世界 café — línea {i} 🐙"))
            .collect::<Vec<_>>()
            .join("\n");
        // Must not panic (line-based truncation is char-safe; the slice is clamped).
        let out = truncate_file_content(content);
        assert!(out.contains("truncated"));
        assert!(out.contains("🐙"));
    }

    #[test]
    fn test_truncate_file_content_empty_string() {
        // Edge: empty content -> zero lines -> clamp take = 0, no panic, unchanged.
        assert_eq!(truncate_file_content(String::new()), String::new());
    }

    #[test]
    fn test_format_auto_context_structure() {
        let files = vec![
            ("src/foo.rs".to_string(), "fn foo() {}".to_string()),
            ("src/bar.rs".to_string(), "fn bar() {}".to_string()),
        ];
        let result = format_auto_context(&files, "fix the bug").unwrap();
        assert!(result.contains("[Auto-context:"));
        assert!(result.contains("--- src/foo.rs ---"));
        assert!(result.contains("--- src/bar.rs ---"));
        assert!(result.contains("fn foo() {}"));
        assert!(result.contains("fn bar() {}"));
        assert!(result.contains("[Your prompt]:"));
        assert!(result.contains("fix the bug"));
    }

    #[test]
    fn test_build_signature_block_basic() {
        use crate::symbols::{Symbol, SymbolKind};

        let repo_map = vec![FileSymbols {
            path: "src/foo.rs".to_string(),
            lines: 100,
            symbols: vec![
                Symbol {
                    name: "do_stuff".to_string(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 10,
                },
                Symbol {
                    name: "MyStruct".to_string(),
                    kind: SymbolKind::Struct,
                    is_public: true,
                    line: 20,
                },
            ],
        }];
        let matched = vec!["src/foo.rs".to_string()];
        let block = build_signature_block(&repo_map, &matched).unwrap();
        assert!(block.contains("src/foo.rs"));
        assert!(block.contains("fn do_stuff"));
        assert!(block.contains("struct MyStruct"));
    }

    #[test]
    fn test_build_signature_block_empty_paths() {
        let repo_map: Vec<FileSymbols> = Vec::new();
        assert!(build_signature_block(&repo_map, &[]).is_none());
    }

    #[test]
    fn test_build_signature_block_no_symbols() {
        let repo_map = vec![FileSymbols {
            path: "src/empty.rs".to_string(),
            lines: 5,
            symbols: vec![],
        }];
        let matched = vec!["src/empty.rs".to_string()];
        assert!(build_signature_block(&repo_map, &matched).is_none());
    }

    #[test]
    fn test_build_signature_block_caps_at_limit() {
        use crate::symbols::{Symbol, SymbolKind};

        // Create a file with many symbols to test the char cap
        let mut symbols = Vec::new();
        for i in 0..200 {
            symbols.push(Symbol {
                name: format!("very_long_function_name_number_{i}"),
                kind: SymbolKind::Function,
                is_public: true,
                line: i,
            });
        }
        let repo_map = vec![
            FileSymbols {
                path: "src/big.rs".to_string(),
                lines: 5000,
                symbols: symbols.clone(),
            },
            FileSymbols {
                path: "src/big2.rs".to_string(),
                lines: 3000,
                symbols,
            },
        ];
        let matched = vec!["src/big.rs".to_string(), "src/big2.rs".to_string()];
        let block = build_signature_block(&repo_map, &matched).unwrap();
        // Should be capped near SIGNATURE_BLOCK_MAX_CHARS
        assert!(
            block.len() <= SIGNATURE_BLOCK_MAX_CHARS + 100,
            "signature block too large: {} chars",
            block.len()
        );
    }

    #[test]
    fn test_format_auto_context_with_signatures() {
        let files = vec![
            (
                SIGNATURE_SENTINEL.to_string(),
                "src/foo.rs\n  fn do_stuff\n  struct MyStruct".to_string(),
            ),
            ("src/foo.rs".to_string(), "fn do_stuff() {}".to_string()),
        ];
        let result = format_auto_context(&files, "explain do_stuff").unwrap();
        // Signature block should appear with its own header
        assert!(
            result.contains("--- Relevant signatures ---"),
            "should contain signature header"
        );
        assert!(
            result.contains("fn do_stuff\n  struct MyStruct"),
            "should contain signature content"
        );
        // File content should also appear
        assert!(result.contains("--- src/foo.rs ---"));
        assert!(result.contains("fn do_stuff() {}"));
        // Signature header should come before file content
        let sig_pos = result.find("--- Relevant signatures ---").unwrap();
        let file_pos = result.find("--- src/foo.rs ---").unwrap();
        assert!(
            sig_pos < file_pos,
            "signatures should appear before file contents"
        );
    }

    #[test]
    fn test_auto_context_includes_signatures() {
        // A prompt about "web search" should include a signature block
        let results =
            auto_context_for_prompt("how does the web search tool work in this project", &[]);
        if results.is_empty() {
            return; // skip if repo map doesn't find matches
        }
        let has_sig = results.iter().any(|(p, _)| p == SIGNATURE_SENTINEL);
        assert!(
            has_sig,
            "auto-context should include a signature block when files match"
        );
        // The signature content should contain symbol-like entries
        let sig_content = &results
            .iter()
            .find(|(p, _)| p == SIGNATURE_SENTINEL)
            .unwrap()
            .1;
        assert!(
            sig_content.contains("fn ") || sig_content.contains("struct "),
            "signature block should contain fn/struct entries, got: {}",
            &sig_content[..sig_content.len().min(200)]
        );
    }

    #[test]
    fn test_format_auto_context_risk_annotation_present() {
        // Use a known high-risk file from the actual codebase risk scores.
        // The top-risk file should get an annotation when it appears in auto-context.
        let top = crate::commands_risk::top_risk_files(1);
        if top.is_empty() {
            return; // skip if no risk data available
        }
        let high_risk_path = &top[0].0;
        let files = vec![(high_risk_path.clone(), "fn example() {}".to_string())];
        let result = format_auto_context(&files, "test prompt").unwrap();
        // The high-risk file should have the ⚠ risk annotation
        assert!(
            result.contains("\u{26a0} risk:"),
            "high-risk file '{}' should have risk annotation in auto-context, got:\n{}",
            high_risk_path,
            result
        );
        assert!(
            result.contains(&format!("--- {} ---", high_risk_path)),
            "should still contain the file path header"
        );
    }

    #[test]
    fn test_format_auto_context_low_risk_no_annotation() {
        // A file that doesn't exist in risk scores (or is very low risk) should not be annotated
        let files = vec![(
            "src/nonexistent_file_that_has_no_risk.rs".to_string(),
            "fn safe() {}".to_string(),
        )];
        let result = format_auto_context(&files, "test prompt").unwrap();
        assert!(
            !result.contains("\u{26a0} risk:"),
            "low-risk/unknown file should NOT have risk annotation, got:\n{}",
            result
        );
        assert!(result.contains("--- src/nonexistent_file_that_has_no_risk.rs ---"));
    }

    #[test]
    fn test_format_auto_context_emerging_risk_annotation() {
        // Use detect_emerging_risks on the live repo to find any emerging-risk files.
        // If one exists, verify that format_auto_context includes the ⚡ annotation.
        let risks = crate::commands_risk::compute_file_risk_scores();
        let emerging = crate::commands_risk::detect_emerging_risks(&risks);
        if emerging.is_empty() {
            // No emerging risks in the current repo — just verify no crash
            let files = vec![("src/main.rs".to_string(), "fn main() {}".to_string())];
            let result = format_auto_context(&files, "test prompt").unwrap();
            assert!(
                !result.contains("\u{26a1} Emerging risk:"),
                "should not have emerging risk annotation when none detected"
            );
            return;
        }
        // Use the first emerging-risk file
        let emerging_path = &emerging[0].path;
        let files = vec![(emerging_path.clone(), "fn example() {}".to_string())];
        let result = format_auto_context(&files, "test prompt").unwrap();
        assert!(
            result.contains("\u{26a1} Emerging risk:"),
            "emerging-risk file '{}' should have ⚡ annotation in auto-context, got:\n{}",
            emerging_path,
            result
        );
        assert!(
            result.contains("faster than usual"),
            "annotation should mention change rate acceleration"
        );
    }

    #[test]
    fn test_get_recent_git_files_returns_set() {
        // Running in the yoyo repo, git diff HEAD~5 should return *something*
        let files = get_recent_git_files();
        // We can't assert specific files (depends on git state), but the set
        // should either be non-empty (normal repo) or empty (shallow clone / no commits)
        // and should never panic.
        for f in &files {
            assert!(!f.is_empty(), "recent git file entry should not be empty");
        }
    }

    #[test]
    fn test_recency_boost_increases_score() {
        use crate::symbols::{Symbol, SymbolKind};

        // Two files with identical keyword scores
        let files = vec![
            FileSymbols {
                path: "src/alpha.rs".into(),
                lines: 100,
                symbols: vec![Symbol {
                    name: "search".into(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 1,
                }],
            },
            FileSymbols {
                path: "src/beta.rs".into(),
                lines: 100,
                symbols: vec![Symbol {
                    name: "search".into(),
                    kind: SymbolKind::Function,
                    is_public: true,
                    line: 1,
                }],
            },
        ];

        let keywords = vec!["search".to_string()];
        let mut results = score_files(&files, &keywords);

        // Both should have the same score initially
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].score, results[1].score);

        // Simulate recency boost for only alpha.rs
        let recent: std::collections::HashSet<String> =
            ["src/alpha.rs".to_string()].into_iter().collect();
        for r in &mut results {
            if recent.contains(&r.path) {
                r.score = r.score * RECENCY_BOOST_NUM / RECENCY_BOOST_DEN;
            }
        }
        results.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));

        // alpha.rs should now rank first with a higher score
        assert_eq!(results[0].path, "src/alpha.rs");
        assert!(
            results[0].score > results[1].score,
            "boosted file should have higher score: {} vs {}",
            results[0].score,
            results[1].score
        );
    }
}
