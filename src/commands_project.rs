//! Project-related command handlers: /context, /init, /docs.

use crate::cli;
use crate::docs;
use crate::format::*;

// Re-export refactoring commands for backward compatibility
pub use crate::commands_move::handle_move;
pub use crate::commands_refactor::{handle_extract, handle_refactor};
pub use crate::commands_rename::{handle_rename, rename_in_project};

use yoagent::agent::Agent;

// ── /context ─────────────────────────────────────────────────────────────

/// Subcommands for /context.
const CONTEXT_SUBCOMMANDS: &[&str] = &["system", "tokens", "files"];

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
        let agent = yoagent::Agent::new(yoagent::provider::AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");
        handle_context("/context", "", &agent);
    }

    #[test]
    fn test_context_system_subcommand() {
        // Verify handle_context with "system" doesn't panic
        let agent = yoagent::Agent::new(yoagent::provider::AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
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
        let agent = yoagent::Agent::new(yoagent::provider::AnthropicProvider)
            .with_system_prompt("You are a test assistant.")
            .with_model("test-model")
            .with_api_key("test-key");
        handle_context("/context tokens", "You are a test assistant.", &agent);
    }

    #[test]
    fn test_context_tokens_section_breakdown() {
        // Multi-section system prompt should show section breakdown without panic
        let prompt = "# Project context\nThis is the project.\nIt has details.\n\n\
                       ## Git status\nOn branch main\n\n\
                       ## Recently changed\nfile1.rs\nfile2.rs\n";
        let agent = yoagent::Agent::new(yoagent::provider::AnthropicProvider)
            .with_system_prompt(prompt)
            .with_model("test-model")
            .with_api_key("test-key");
        // Should not panic and should exercise the section breakdown path
        handle_context("/context tokens", prompt, &agent);
    }

    #[test]
    fn test_context_tokens_single_section_no_breakdown() {
        // Single-section prompt should NOT show breakdown (just the total)
        let prompt = "You are a helpful assistant.";
        let agent = yoagent::Agent::new(yoagent::provider::AnthropicProvider)
            .with_system_prompt(prompt)
            .with_model("test-model")
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
        let tmp = std::env::temp_dir().join("yoyo_test_node");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("package.json"), "{}").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Node);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_python_pyproject() {
        let tmp = std::env::temp_dir().join("yoyo_test_python_pyproject");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("pyproject.toml"), "[project]").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Python);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_python_setup_py() {
        let tmp = std::env::temp_dir().join("yoyo_test_python_setup");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("setup.py"), "").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Python);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_go() {
        let tmp = std::env::temp_dir().join("yoyo_test_go");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("go.mod"), "module example.com/test").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Go);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_makefile() {
        let tmp = std::env::temp_dir().join("yoyo_test_make");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("Makefile"), "test:\n\techo ok").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Make);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_unknown() {
        let tmp = std::env::temp_dir().join("yoyo_test_unknown");
        let _ = std::fs::create_dir_all(&tmp);
        // Empty dir — no marker files
        assert_eq!(detect_project_type(&tmp), ProjectType::Unknown);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_priority_rust_over_makefile() {
        // If both Cargo.toml and Makefile exist, Rust wins
        let tmp = std::env::temp_dir().join("yoyo_test_priority");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(tmp.join("Makefile"), "test:").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Rust);
        let _ = std::fs::remove_dir_all(&tmp);
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
        let tmp = std::env::temp_dir().join("yoyo_test_init_empty");
        let _ = std::fs::create_dir_all(&tmp);
        let files = scan_important_files(&tmp);
        assert!(files.is_empty(), "Empty dir should have no important files");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_scan_important_files_with_readme() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_readme");
        let _ = std::fs::create_dir_all(&tmp);
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
        let _ = std::fs::remove_dir_all(&tmp);
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
        let tmp = std::env::temp_dir().join("yoyo_test_init_dirs_empty");
        let _ = std::fs::create_dir_all(&tmp);
        let dirs = scan_important_dirs(&tmp);
        assert!(dirs.is_empty(), "Empty dir should have no important dirs");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_scan_important_dirs_with_subdirs() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_subdirs");
        let _ = std::fs::create_dir_all(tmp.join("src"));
        let _ = std::fs::create_dir_all(tmp.join("tests"));
        let _ = std::fs::create_dir_all(tmp.join("docs"));
        let dirs = scan_important_dirs(&tmp);
        assert!(dirs.contains(&"src".to_string()), "Should find src/");
        assert!(dirs.contains(&"tests".to_string()), "Should find tests/");
        assert!(dirs.contains(&"docs".to_string()), "Should find docs/");
        let _ = std::fs::remove_dir_all(&tmp);
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
        let tmp = std::env::temp_dir().join("yoyo_test_name_fallback");
        let _ = std::fs::create_dir_all(&tmp);
        let name = detect_project_name(&tmp);
        assert_eq!(
            name, "yoyo_test_name_fallback",
            "Should fall back to directory name"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_name_from_readme() {
        let tmp = std::env::temp_dir().join("yoyo_test_name_readme");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("README.md"), "# My Awesome Project\n\nSome text.").unwrap();
        let name = detect_project_name(&tmp);
        assert_eq!(
            name, "My Awesome Project",
            "Should extract name from README title"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_name_from_package_json() {
        let tmp = std::env::temp_dir().join("yoyo_test_name_pkg");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(
            tmp.join("package.json"),
            "{\n  \"name\": \"cool-app\",\n  \"version\": \"1.0.0\"\n}",
        )
        .unwrap();
        let name = detect_project_name(&tmp);
        assert_eq!(name, "cool-app", "Should extract name from package.json");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_generate_init_content_rust_project() {
        let cwd = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let content = generate_init_content(&cwd);
        // Should contain project name
        assert!(
            content.contains("yoyo"),
            "Should contain project name: {}",
            &content[..200.min(content.len())]
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
        let tmp = std::env::temp_dir().join("yoyo_test_init_gen_empty");
        let _ = std::fs::create_dir_all(&tmp);
        let content = generate_init_content(&tmp);
        // Should still have sections even for empty/unknown project
        assert!(content.contains("# Project Context"));
        assert!(content.contains("## About This Project"));
        assert!(content.contains("## Build & Test"));
        assert!(content.contains("## Coding Conventions"));
        assert!(content.contains("## Important Files"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_generate_init_content_node_project() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_gen_node");
        let _ = std::fs::create_dir_all(&tmp);
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
        let _ = std::fs::remove_dir_all(&tmp);
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
        let agent = yoagent::Agent::new(yoagent::provider::AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");
        show_context_files(&agent);
    }

    #[test]
    fn test_context_files_dispatch() {
        // Verify handle_context routes "files" correctly (shouldn't panic)
        let agent = yoagent::Agent::new(yoagent::provider::AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
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
            AgentMessage::Llm(Message::Assistant {
                content: vec![
                    Content::ToolCall {
                        id: "1".into(),
                        name: "read_file".into(),
                        arguments: serde_json::json!({"path": "src/main.rs"}),
                        provider_metadata: None,
                    },
                    Content::ToolCall {
                        id: "2".into(),
                        name: "edit_file".into(),
                        arguments: serde_json::json!({"path": "src/tools.rs", "old_text": "a", "new_text": "b"}),
                        provider_metadata: None,
                    },
                    Content::ToolCall {
                        id: "3".into(),
                        name: "write_file".into(),
                        arguments: serde_json::json!({"path": "src/new.rs", "content": "fn main() {}"}),
                        provider_metadata: None,
                    },
                ],
                stop_reason: StopReason::ToolUse,
                model: "test".into(),
                provider: "test".into(),
                usage: Usage::default(),
                timestamp: 0,
                error_message: None,
            }),
            AgentMessage::Llm(Message::Assistant {
                content: vec![
                    Content::ToolCall {
                        id: "4".into(),
                        name: "list_files".into(),
                        arguments: serde_json::json!({"path": "src/"}),
                        provider_metadata: None,
                    },
                    Content::ToolCall {
                        id: "5".into(),
                        name: "search".into(),
                        arguments: serde_json::json!({"pattern": "TODO", "path": "src/"}),
                        provider_metadata: None,
                    },
                    // Duplicate read — should be deduplicated
                    Content::ToolCall {
                        id: "6".into(),
                        name: "read_file".into(),
                        arguments: serde_json::json!({"path": "src/main.rs"}),
                        provider_metadata: None,
                    },
                ],
                stop_reason: StopReason::ToolUse,
                model: "test".into(),
                provider: "test".into(),
                usage: Usage::default(),
                timestamp: 0,
                error_message: None,
            }),
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

        let messages = vec![AgentMessage::Llm(Message::Assistant {
            content: vec![
                Content::ToolCall {
                    id: "1".into(),
                    name: "bash".into(),
                    arguments: serde_json::json!({"command": "ls"}),
                    provider_metadata: None,
                },
                Content::ToolCall {
                    id: "2".into(),
                    name: "todo".into(),
                    arguments: serde_json::json!({"action": "list"}),
                    provider_metadata: None,
                },
            ],
            stop_reason: StopReason::Stop,
            model: "test".into(),
            provider: "test".into(),
            usage: Usage::default(),
            timestamp: 0,
            error_message: None,
        })];

        let result = extract_context_files(&messages);
        assert!(result.is_empty(), "Non-file tools should be skipped");
    }

    #[test]
    fn test_extract_context_files_search_without_path() {
        use yoagent::types::*;

        // search tool call with no path (searches cwd) — should not add empty path
        let messages = vec![AgentMessage::Llm(Message::Assistant {
            content: vec![Content::ToolCall {
                id: "1".into(),
                name: "search".into(),
                arguments: serde_json::json!({"pattern": "TODO"}),
                provider_metadata: None,
            }],
            stop_reason: StopReason::ToolUse,
            model: "test".into(),
            provider: "test".into(),
            usage: Usage::default(),
            timestamp: 0,
            error_message: None,
        })];

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
}
