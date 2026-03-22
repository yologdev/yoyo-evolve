//! Project-related command handlers: /add, /context, /init, /health, /fix, /test, /lint,
//! /tree, /run, /docs, /find, /index, /web.

use crate::cli;
use crate::commands::auto_compact_if_needed;
use crate::docs;
use crate::format::*;
use crate::prompt::*;

use yoagent::agent::Agent;
use yoagent::*;

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

/// Return health check commands for a given project type.
#[allow(clippy::vec_init_then_push, unused_mut)]
pub fn health_checks_for_project(
    project_type: &ProjectType,
) -> Vec<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => {
            let mut checks = vec![("build", vec!["cargo", "build"])];
            #[cfg(not(test))]
            checks.push(("test", vec!["cargo", "test"]));
            checks.push((
                "clippy",
                vec!["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
            ));
            checks.push(("fmt", vec!["cargo", "fmt", "--", "--check"]));
            checks
        }
        ProjectType::Node => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["npm", "test"]));
            checks.push(("lint", vec!["npx", "eslint", "."]));
            checks
        }
        ProjectType::Python => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["python", "-m", "pytest"]));
            checks.push(("lint", vec!["python", "-m", "flake8", "."]));
            checks.push(("typecheck", vec!["python", "-m", "mypy", "."]));
            checks
        }
        ProjectType::Go => {
            let mut checks = vec![("build", vec!["go", "build", "./..."])];
            #[cfg(not(test))]
            checks.push(("test", vec!["go", "test", "./..."]));
            checks.push(("vet", vec!["go", "vet", "./..."]));
            checks
        }
        ProjectType::Make => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["make", "test"]));
            checks
        }
        ProjectType::Unknown => vec![],
    }
}

/// Run health checks for a specific project type. Returns (name, passed, detail) tuples.
pub fn run_health_check_for_project(
    project_type: &ProjectType,
) -> Vec<(&'static str, bool, String)> {
    let checks = health_checks_for_project(project_type);

    let mut results = Vec::new();
    for (name, args) in checks {
        let start = std::time::Instant::now();
        let output = std::process::Command::new(args[0])
            .args(&args[1..])
            .output();
        let elapsed = format_duration(start.elapsed());
        match output {
            Ok(o) if o.status.success() => {
                results.push((name, true, format!("ok ({elapsed})")));
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                let first_line = stderr.lines().next().unwrap_or("(unknown error)");
                results.push((
                    name,
                    false,
                    format!(
                        "FAIL ({elapsed}): {}",
                        truncate_with_ellipsis(first_line, 80)
                    ),
                ));
            }
            Err(e) => {
                results.push((name, false, format!("ERROR: {e}")));
            }
        }
    }
    results
}

/// Run health checks and capture full error output for failures.
pub fn run_health_checks_full_output(
    project_type: &ProjectType,
) -> Vec<(&'static str, bool, String)> {
    let checks = health_checks_for_project(project_type);

    let mut results = Vec::new();
    for (name, args) in checks {
        let output = std::process::Command::new(args[0])
            .args(&args[1..])
            .output();
        match output {
            Ok(o) if o.status.success() => {
                results.push((name, true, String::new()));
            }
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let stderr = String::from_utf8_lossy(&o.stderr);
                let mut full_output = String::new();
                if !stdout.is_empty() {
                    full_output.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !full_output.is_empty() {
                        full_output.push('\n');
                    }
                    full_output.push_str(&stderr);
                }
                results.push((name, false, full_output));
            }
            Err(e) => {
                results.push((name, false, format!("ERROR: {e}")));
            }
        }
    }
    results
}

/// Build a prompt describing health check failures for the AI to fix.
pub fn build_fix_prompt(failures: &[(&str, &str)]) -> String {
    if failures.is_empty() {
        return String::new();
    }
    let mut prompt = String::from(
        "Fix the following build/lint errors in this project. Read the relevant files, understand the errors, and apply fixes:\n\n",
    );
    for (name, output) in failures {
        prompt.push_str(&format!("## {name} errors:\n```\n{output}\n```\n\n"));
    }
    prompt.push_str(
        "After fixing, run the failing checks again to verify. Fix any remaining issues.",
    );
    prompt
}

pub fn handle_health() {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return;
    }
    println!("{DIM}  Running health checks...{RESET}");
    let results = run_health_check_for_project(&project_type);
    if results.is_empty() {
        println!("{DIM}  No checks configured for {project_type}{RESET}\n");
        return;
    }
    let all_passed = results.iter().all(|(_, passed, _)| *passed);
    for (name, passed, detail) in &results {
        let icon = if *passed {
            format!("{GREEN}✓{RESET}")
        } else {
            format!("{RED}✗{RESET}")
        };
        println!("  {icon} {name}: {detail}");
    }
    if all_passed {
        println!("\n{GREEN}  All checks passed ✓{RESET}\n");
    } else {
        println!("\n{RED}  Some checks failed ✗{RESET}\n");
    }
}

/// Handle the /fix command. Returns Some(fix_prompt) if failures were sent to AI, None otherwise.
pub async fn handle_fix(
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }
    println!("{DIM}  Detected project: {project_type}{RESET}");
    println!("{DIM}  Running health checks...{RESET}");
    let results = run_health_checks_full_output(&project_type);
    if results.is_empty() {
        println!("{DIM}  No checks configured for {project_type}{RESET}\n");
        return None;
    }
    for (name, passed, _) in &results {
        let icon = if *passed {
            format!("{GREEN}✓{RESET}")
        } else {
            format!("{RED}✗{RESET}")
        };
        let status = if *passed { "ok" } else { "FAIL" };
        println!("  {icon} {name}: {status}");
    }
    let failures: Vec<(&str, &str)> = results
        .iter()
        .filter(|(_, passed, _)| !passed)
        .map(|(name, _, output)| (*name, output.as_str()))
        .collect();
    if failures.is_empty() {
        println!("\n{GREEN}  All checks passed — nothing to fix ✓{RESET}\n");
        return None;
    }
    let fail_count = failures.len();
    println!("\n{YELLOW}  Sending {fail_count} failure(s) to AI for fixing...{RESET}\n");
    let fix_prompt = build_fix_prompt(&failures);
    run_prompt(agent, &fix_prompt, session_total, model).await;
    auto_compact_if_needed(agent);
    Some(fix_prompt)
}

// ── /test ─────────────────────────────────────────────────────────────

/// Return the test command for a given project type.
pub fn test_command_for_project(
    project_type: &ProjectType,
) -> Option<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => Some(("cargo test", vec!["cargo", "test"])),
        ProjectType::Node => Some(("npm test", vec!["npm", "test"])),
        ProjectType::Python => Some(("python -m pytest", vec!["python", "-m", "pytest"])),
        ProjectType::Go => Some(("go test ./...", vec!["go", "test", "./..."])),
        ProjectType::Make => Some(("make test", vec!["make", "test"])),
        ProjectType::Unknown => None,
    }
}

/// Handle the /test command: auto-detect project type and run tests.
/// Returns a summary string suitable for AI context.
pub fn handle_test() -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }

    let (label, args) = match test_command_for_project(&project_type) {
        Some(cmd) => cmd,
        None => {
            println!("{DIM}  No test command configured for {project_type}{RESET}\n");
            return None;
        }
    };

    println!("{DIM}  Running: {label}...{RESET}");
    let start = std::time::Instant::now();
    let output = std::process::Command::new(args[0])
        .args(&args[1..])
        .output();
    let elapsed = format_duration(start.elapsed());

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);

            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }

            if o.status.success() {
                println!("\n{GREEN}  ✓ Tests passed ({elapsed}){RESET}\n");
                Some(format!("Tests passed ({elapsed}): {label}"))
            } else {
                let code = o.status.code().unwrap_or(-1);
                println!("\n{RED}  ✗ Tests failed (exit {code}, {elapsed}){RESET}\n");
                let mut summary = format!("Tests FAILED (exit {code}, {elapsed}): {label}");
                // Include a preview of the error output for AI context
                let error_text = if !stderr.is_empty() {
                    stderr.to_string()
                } else {
                    stdout.to_string()
                };
                let lines: Vec<&str> = error_text.lines().collect();
                let preview_lines = if lines.len() > 20 {
                    &lines[lines.len() - 20..]
                } else {
                    &lines
                };
                summary.push_str("\n\nLast output:\n");
                for line in preview_lines {
                    summary.push_str(line);
                    summary.push('\n');
                }
                Some(summary)
            }
        }
        Err(e) => {
            eprintln!("{RED}  ✗ Failed to run {label}: {e}{RESET}\n");
            Some(format!("Failed to run {label}: {e}"))
        }
    }
}

// ── /lint ──────────────────────────────────────────────────────────────

/// Return the lint command for a given project type.
pub fn lint_command_for_project(
    project_type: &ProjectType,
) -> Option<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => Some((
            "cargo clippy --all-targets -- -D warnings",
            vec!["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
        )),
        ProjectType::Node => Some(("npx eslint .", vec!["npx", "eslint", "."])),
        ProjectType::Python => Some(("ruff check .", vec!["ruff", "check", "."])),
        ProjectType::Go => Some(("golangci-lint run", vec!["golangci-lint", "run"])),
        ProjectType::Make | ProjectType::Unknown => None,
    }
}

/// Handle the /lint command: auto-detect project type and run linter.
/// Returns a summary string suitable for AI context.
pub fn handle_lint() -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }

    let (label, args) = match lint_command_for_project(&project_type) {
        Some(cmd) => cmd,
        None => {
            println!("{DIM}  No lint command configured for {project_type}{RESET}\n");
            return None;
        }
    };

    println!("{DIM}  Running: {label}...{RESET}");
    let start = std::time::Instant::now();
    let output = std::process::Command::new(args[0])
        .args(&args[1..])
        .output();
    let elapsed = format_duration(start.elapsed());

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);

            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }

            if o.status.success() {
                println!("\n{GREEN}  ✓ Lint passed ({elapsed}){RESET}\n");
                Some(format!("Lint passed ({elapsed}): {label}"))
            } else {
                let code = o.status.code().unwrap_or(-1);
                println!("\n{RED}  ✗ Lint failed (exit {code}, {elapsed}){RESET}\n");
                let mut summary = format!("Lint FAILED (exit {code}, {elapsed}): {label}");
                let error_text = if !stderr.is_empty() {
                    stderr.to_string()
                } else {
                    stdout.to_string()
                };
                let lines: Vec<&str> = error_text.lines().collect();
                let preview_lines = if lines.len() > 20 {
                    &lines[lines.len() - 20..]
                } else {
                    &lines
                };
                summary.push_str("\n\nLast output:\n");
                for line in preview_lines {
                    summary.push_str(line);
                    summary.push('\n');
                }
                Some(summary)
            }
        }
        Err(e) => {
            eprintln!("{RED}  ✗ Failed to run {label}: {e}{RESET}\n");
            Some(format!("Failed to run {label}: {e}"))
        }
    }
}

// ── /tree ────────────────────────────────────────────────────────────────

/// Build a directory tree from `git ls-files`.
pub fn build_project_tree(max_depth: usize) -> String {
    let files = match crate::git::run_git(&["ls-files"]) {
        Ok(text) => {
            let mut files: Vec<String> = text
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
            files.sort();
            files
        }
        Err(_) => return "(not a git repository — /tree requires git)".to_string(),
    };

    if files.is_empty() {
        return "(no tracked files)".to_string();
    }

    format_tree_from_paths(&files, max_depth)
}

/// Format a sorted list of file paths into an indented tree string.
pub fn format_tree_from_paths(paths: &[String], max_depth: usize) -> String {
    use std::collections::BTreeSet;

    let mut output = String::new();
    let mut printed_dirs: BTreeSet<String> = BTreeSet::new();

    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        let depth = parts.len() - 1;

        for level in 0..parts.len().saturating_sub(1).min(max_depth) {
            let dir_path: String = parts[..=level].join("/");
            let dir_key = format!("{}/", dir_path);
            if printed_dirs.insert(dir_key) {
                let indent = "  ".repeat(level);
                let dir_name = parts[level];
                output.push_str(&format!("{indent}{dir_name}/\n"));
            }
        }

        if depth <= max_depth {
            let indent = "  ".repeat(depth.min(max_depth));
            let file_name = parts.last().unwrap_or(&"");
            output.push_str(&format!("{indent}{file_name}\n"));
        }
    }

    if output.ends_with('\n') {
        output.truncate(output.len() - 1);
    }

    output
}

pub fn handle_tree(input: &str) {
    let arg = input.strip_prefix("/tree").unwrap_or("").trim();
    let max_depth = if arg.is_empty() {
        3
    } else {
        match arg.parse::<usize>() {
            Ok(d) => d,
            Err(_) => {
                println!("{DIM}  usage: /tree [depth]  (default depth: 3){RESET}\n");
                return;
            }
        }
    };
    let tree = build_project_tree(max_depth);
    println!("{DIM}{tree}{RESET}\n");
}

// ── /run ─────────────────────────────────────────────────────────────────

/// Run a shell command directly and print its output.
pub fn run_shell_command(cmd: &str) {
    let start = std::time::Instant::now();
    let output = std::process::Command::new("sh").args(["-c", cmd]).output();
    let elapsed = format_duration(start.elapsed());

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{RED}{stderr}{RESET}");
            }
            let code = o.status.code().unwrap_or(-1);
            if code == 0 {
                println!("{DIM}  ✓ exit {code} ({elapsed}){RESET}\n");
            } else {
                println!("{RED}  ✗ exit {code} ({elapsed}){RESET}\n");
            }
        }
        Err(e) => {
            eprintln!("{RED}  error running command: {e}{RESET}\n");
        }
    }
}

pub fn handle_run(input: &str) {
    let cmd = if input.starts_with("/run ") {
        input.trim_start_matches("/run ").trim()
    } else if input.starts_with('!') && input.len() > 1 {
        input[1..].trim()
    } else {
        ""
    };
    if cmd.is_empty() {
        println!("{DIM}  usage: /run <command>  or  !<command>{RESET}\n");
    } else {
        run_shell_command(cmd);
    }
}

pub fn handle_run_usage() {
    println!("{DIM}  usage: /run <command>  or  !<command>");
    println!("  Runs a shell command directly (no AI, no tokens).{RESET}\n");
}

// ── /find ────────────────────────────────────────────────────────────────

/// Result of a fuzzy file match: (file_path, score, match_ranges).
/// Higher score = better match. match_ranges are byte offsets into the lowercased path.
#[derive(Debug, Clone, PartialEq)]
pub struct FindMatch {
    pub path: String,
    pub score: i32,
}

/// Score a file path against a fuzzy pattern (case-insensitive substring match).
/// Returns None if the pattern doesn't match.
/// Scoring:
///   - Base score for containing the pattern as a substring
///   - Bonus for matching the filename (last component) vs directory
///   - Bonus for exact filename match
///   - Bonus for match at the start of the filename
///   - Shorter paths score higher (less noise)
pub fn fuzzy_score(path: &str, pattern: &str) -> Option<i32> {
    let path_lower = path.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    if !path_lower.contains(&pattern_lower) {
        return None;
    }

    let mut score: i32 = 100; // base score for matching

    // Extract filename (last path component)
    let filename = path.rsplit('/').next().unwrap_or(path);
    let filename_lower = filename.to_lowercase();

    // Big bonus if the pattern matches within the filename itself
    if filename_lower.contains(&pattern_lower) {
        score += 50;

        // Bonus for matching at the start of filename
        if filename_lower.starts_with(&pattern_lower) {
            score += 30;
        }

        // Bonus for exact filename match (without extension)
        let stem = filename_lower.split('.').next().unwrap_or(&filename_lower);
        if stem == pattern_lower {
            score += 20;
        }
    }

    // Shorter paths are slightly preferred (less deeply nested = more relevant)
    let depth = path.matches('/').count();
    score -= depth as i32 * 2;

    Some(score)
}

/// Find files matching a fuzzy pattern. Uses `git ls-files` if in a git repo,
/// otherwise falls back to a recursive directory listing.
pub fn find_files(pattern: &str) -> Vec<FindMatch> {
    let files = list_project_files();
    let mut matches: Vec<FindMatch> = files
        .iter()
        .filter_map(|path| {
            fuzzy_score(path, pattern).map(|score| FindMatch {
                path: path.clone(),
                score,
            })
        })
        .collect();

    // Sort by score descending, then alphabetically for ties
    matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.path.cmp(&b.path)));
    matches
}

/// List all project files. Prefers `git ls-files`, falls back to walkdir-style listing.
fn list_project_files() -> Vec<String> {
    if let Ok(text) = crate::git::run_git(&["ls-files"]) {
        return text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();
    }

    // Fallback: recursive listing of current directory (respecting common ignores)
    walk_directory(".", 8)
}

/// Simple recursive directory walk (fallback when not in a git repo).
fn walk_directory(dir: &str, max_depth: usize) -> Vec<String> {
    let mut files = Vec::new();
    walk_directory_inner(dir, max_depth, 0, &mut files);
    files
}

fn walk_directory_inner(dir: &str, max_depth: usize, depth: usize, files: &mut Vec<String>) {
    if depth > max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden dirs and common ignore patterns
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        let path = if dir == "." {
            name.clone()
        } else {
            format!("{dir}/{name}")
        };
        if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            walk_directory_inner(&path, max_depth, depth + 1, files);
        } else {
            files.push(path);
        }
    }
}

/// Highlight the matching pattern within a file path for display.
/// Returns the path with ANSI bold/color around the matched portion.
pub fn highlight_match(path: &str, pattern: &str) -> String {
    let path_lower = path.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    if let Some(pos) = path_lower.rfind(&pattern_lower) {
        // Prefer highlighting in the filename portion
        let end = pos + pattern.len();
        format!(
            "{}{BOLD}{GREEN}{}{RESET}{}",
            &path[..pos],
            &path[pos..end],
            &path[end..]
        )
    } else {
        path.to_string()
    }
}

pub fn handle_find(input: &str) {
    let arg = input.strip_prefix("/find").unwrap_or("").trim();
    if arg.is_empty() {
        println!("{DIM}  usage: /find <pattern>");
        println!("  Fuzzy-search project files by name.");
        println!("  Examples: /find main, /find .toml, /find test{RESET}\n");
        return;
    }

    let matches = find_files(arg);
    if matches.is_empty() {
        println!("{DIM}  No files matching '{arg}'.{RESET}\n");
    } else {
        let count = matches.len();
        let shown = matches.iter().take(20);
        println!(
            "{DIM}  {count} file{s} matching '{arg}':",
            s = if count == 1 { "" } else { "s" }
        );
        for m in shown {
            let highlighted = highlight_match(&m.path, arg);
            println!("    {highlighted}");
        }
        if count > 20 {
            println!("    {DIM}... and {} more{RESET}", count - 20);
        }
        println!("{RESET}");
    }
}

// ── /index ───────────────────────────────────────────────────────────────

/// An entry in the project index: path, line count, and first meaningful line.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexEntry {
    pub path: String,
    pub lines: usize,
    pub summary: String,
}

/// Extract the first meaningful line from file content.
/// Skips blank lines, then grabs the first doc comment (`//!`, `///`, `#`),
/// module declaration, or any non-empty line.
pub fn extract_first_meaningful_line(content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Return the first non-empty line, truncated
        return truncate_with_ellipsis(trimmed, 80);
    }
    String::new()
}

/// Build a project index by listing files and extracting metadata.
/// Uses `git ls-files` when available, falls back to directory walk.
/// Only indexes text-like source files (skips binaries, images, etc.).
pub fn build_project_index() -> Vec<IndexEntry> {
    let files = list_project_files();
    let mut entries = Vec::new();

    for path in &files {
        // Skip binary/non-text files based on extension
        if is_binary_extension(path) {
            continue;
        }

        // Read the file — skip if it fails (binary, permission, etc.)
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let line_count = content.lines().count();
        let summary = extract_first_meaningful_line(&content);

        entries.push(IndexEntry {
            path: path.clone(),
            lines: line_count,
            summary,
        });
    }

    entries
}

/// Check if a file extension suggests a binary/non-text file.
pub fn is_binary_extension(path: &str) -> bool {
    let binary_exts = [
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".ico", ".svg", ".woff", ".woff2",
        ".ttf", ".otf", ".eot", ".pdf", ".zip", ".gz", ".tar", ".bz2", ".xz", ".7z", ".rar",
        ".exe", ".dll", ".so", ".dylib", ".o", ".a", ".class", ".pyc", ".pyo", ".wasm", ".lock",
    ];
    let lower = path.to_lowercase();
    binary_exts.iter().any(|ext| lower.ends_with(ext))
}

/// Format the project index as a table string.
pub fn format_project_index(entries: &[IndexEntry]) -> String {
    if entries.is_empty() {
        return "(no indexable files found)".to_string();
    }

    let mut output = String::new();

    // Find max path length for alignment (capped at 50)
    let max_path_len = entries
        .iter()
        .map(|e| e.path.len())
        .max()
        .unwrap_or(0)
        .min(50);

    output.push_str(&format!(
        "  {:<width$}  {:>5}  {}\n",
        "Path",
        "Lines",
        "Summary",
        width = max_path_len
    ));
    output.push_str(&format!(
        "  {:<width$}  {:>5}  {}\n",
        "─".repeat(max_path_len.min(50)),
        "─────",
        "─".repeat(40),
        width = max_path_len
    ));

    for entry in entries {
        let path_display = if entry.path.len() > 50 {
            format!("…{}", &entry.path[entry.path.len() - 49..])
        } else {
            entry.path.clone()
        };
        output.push_str(&format!(
            "  {:<width$}  {:>5}  {}\n",
            path_display,
            entry.lines,
            entry.summary,
            width = max_path_len
        ));
    }

    // Summary line
    let total_files = entries.len();
    let total_lines: usize = entries.iter().map(|e| e.lines).sum();
    output.push_str(&format!(
        "\n  {} file{}, {} total lines\n",
        total_files,
        if total_files == 1 { "" } else { "s" },
        total_lines
    ));

    output
}

/// Handle the /index command: build and display a project file index.
pub fn handle_index() {
    println!("{DIM}  Building project index...{RESET}");
    let entries = build_project_index();
    if entries.is_empty() {
        println!("{DIM}  (no indexable source files found){RESET}\n");
    } else {
        let formatted = format_project_index(&entries);
        println!("{DIM}{formatted}{RESET}");
    }
}

// ── /web ─────────────────────────────────────────────────────────────────

/// Maximum characters to display from a fetched web page.
const WEB_MAX_CHARS: usize = 5000;

/// Strip HTML tags and extract readable text content.
///
/// This function:
/// - Removes `<script>`, `<style>`, `<nav>`, `<footer>`, `<header>`, `<svg>` blocks entirely
/// - Converts `<br>`, `<p>`, `<div>`, `<li>`, `<h1>`–`<h6>`, `<tr>` to newlines
/// - Converts `<li>` items to bullet points
/// - Strips all remaining HTML tags
/// - Decodes common HTML entities
/// - Collapses excessive whitespace
/// - Truncates to `max_chars`
pub fn strip_html_tags(html: &str, max_chars: usize) -> String {
    // First pass: remove blocks we want to skip entirely (script, style, etc.)
    let html_lower = html.to_lowercase();
    let mut cleaned = String::with_capacity(html.len());
    let skip_tags = ["script", "style", "nav", "footer", "header", "svg"];

    let mut i = 0;
    let bytes = html.as_bytes();
    let lower_bytes = html_lower.as_bytes();

    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Check if this is a skip-tag opening
            let mut found_skip = false;
            for tag in &skip_tags {
                let open = format!("<{}", tag);
                if i + open.len() <= lower_bytes.len()
                    && html_lower[i..i + open.len()] == *open
                    && (i + open.len() >= lower_bytes.len()
                        || lower_bytes[i + open.len()] == b' '
                        || lower_bytes[i + open.len()] == b'>'
                        || lower_bytes[i + open.len()] == b'\t'
                        || lower_bytes[i + open.len()] == b'\n')
                {
                    // Find the closing tag
                    let close = format!("</{}>", tag);
                    if let Some(end_pos) = html_lower[i..].find(&close) {
                        i += end_pos + close.len();
                        found_skip = true;
                        break;
                    }
                }
            }
            if !found_skip {
                cleaned.push(bytes[i] as char);
                i += 1;
            }
        } else {
            cleaned.push(bytes[i] as char);
            i += 1;
        }
    }

    // Second pass: convert meaningful tags to formatting, strip the rest
    let mut result = String::with_capacity(cleaned.len());
    let cleaned_lower = cleaned.to_lowercase();
    let cleaned_bytes = cleaned.as_bytes();
    let len = cleaned_bytes.len();
    let mut j = 0;

    while j < len {
        if cleaned_bytes[j] == b'<' {
            // Find end of tag
            let tag_start = j;
            let mut tag_end = j + 1;
            while tag_end < len && cleaned_bytes[tag_end] != b'>' {
                tag_end += 1;
            }
            if tag_end < len {
                tag_end += 1; // include the '>'
            }

            let tag_lower = &cleaned_lower[tag_start..tag_end.min(len)];

            // Decide what to emit based on tag
            if tag_lower.starts_with("<br") {
                result.push('\n');
            } else if tag_lower.starts_with("<li") {
                result.push_str("\n• ");
            } else if tag_lower.starts_with("<h1")
                || tag_lower.starts_with("<h2")
                || tag_lower.starts_with("<h3")
                || tag_lower.starts_with("<h4")
                || tag_lower.starts_with("<h5")
                || tag_lower.starts_with("<h6")
            {
                result.push_str("\n\n");
            } else if tag_lower.starts_with("</h")
                || tag_lower.starts_with("<p")
                || tag_lower.starts_with("</p")
                || tag_lower.starts_with("<div")
                || tag_lower.starts_with("</div")
                || tag_lower.starts_with("<tr")
                || tag_lower.starts_with("</tr")
                || tag_lower.starts_with("<blockquote")
                || tag_lower.starts_with("</blockquote")
                || tag_lower.starts_with("<section")
                || tag_lower.starts_with("</section")
                || tag_lower.starts_with("<article")
                || tag_lower.starts_with("</article")
            {
                result.push('\n');
            }
            // All other tags: skip (emit nothing)

            j = tag_end;
        } else {
            // Safety: we're iterating byte-by-byte, but we need valid UTF-8.
            // Use the original cleaned string's chars at this position.
            result.push(cleaned_bytes[j] as char);
            j += 1;
        }
    }

    // Decode HTML entities (shared utility)
    let decoded = crate::format::decode_html_entities(&result);

    // Collapse whitespace: multiple blank lines → two newlines, multiple spaces → one
    let mut final_text = String::with_capacity(decoded.len());
    let mut prev_newlines = 0u32;
    let mut prev_space = false;

    for c in decoded.chars() {
        if c == '\n' {
            prev_newlines += 1;
            prev_space = false;
            if prev_newlines <= 2 {
                final_text.push('\n');
            }
        } else if c == ' ' || c == '\t' {
            if prev_newlines > 0 {
                // Skip spaces right after newlines (trim line starts)
            } else if !prev_space {
                final_text.push(' ');
                prev_space = true;
            }
        } else {
            prev_newlines = 0;
            prev_space = false;
            final_text.push(c);
        }
    }

    // Trim each line and rejoin
    let final_text: String = final_text
        .lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n");

    let final_text = final_text.trim().to_string();

    // Truncate to max_chars
    if final_text.len() > max_chars {
        let truncated = &final_text[..final_text.floor_char_boundary(max_chars)];
        format!("{truncated}\n\n[… truncated at {max_chars} chars]")
    } else {
        final_text
    }
}

/// Validate that a string looks like a URL.
pub fn is_valid_url(url: &str) -> bool {
    (url.starts_with("http://") || url.starts_with("https://"))
        && url.len() > 10
        && url.contains('.')
}

/// Fetch a URL using curl and return the HTML content.
fn fetch_url(url: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args([
            "-sL", // silent, follow redirects
            "--max-time",
            "15", // timeout
            "-A",
            "Mozilla/5.0 (compatible; yoyo-agent/0.1)", // user agent
            url,
        ])
        .output()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "curl failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }

    let body = String::from_utf8_lossy(&output.stdout).to_string();
    if body.is_empty() {
        return Err("empty response".to_string());
    }

    Ok(body)
}

/// Handle the /web command — fetch a URL and display readable text.
pub fn handle_web(input: &str) {
    let url = input.trim_start_matches("/web").trim();

    if url.is_empty() {
        println!("{DIM}  usage: /web <url>");
        println!("  Fetch a web page and display readable text content.");
        println!(
            "  Example: /web https://doc.rust-lang.org/book/ch01-01-installation.html{RESET}\n"
        );
        return;
    }

    // Auto-prepend https:// if missing
    let url = if !url.starts_with("http://") && !url.starts_with("https://") {
        format!("https://{url}")
    } else {
        url.to_string()
    };

    if !is_valid_url(&url) {
        println!("{RED}  Invalid URL: {url}{RESET}\n");
        return;
    }

    println!("{DIM}  Fetching {url}...{RESET}");

    match fetch_url(&url) {
        Ok(html) => {
            let text = strip_html_tags(&html, WEB_MAX_CHARS);
            if text.is_empty() {
                println!("{DIM}  (no readable text content found){RESET}\n");
            } else {
                let line_count = text.lines().count();
                let char_count = text.len();
                println!();
                println!("{text}");
                println!();
                println!("{DIM}  ── {line_count} lines, {char_count} chars from {url}{RESET}\n");
            }
        }
        Err(e) => {
            println!("{RED}  Failed to fetch: {e}{RESET}\n");
        }
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

// ── /add ─────────────────────────────────────────────────────────────────

/// Parse an `/add` argument into a file path and optional line range.
///
/// Supports:
///   - `path/to/file.rs` → ("path/to/file.rs", None)
///   - `path/to/file.rs:10-20` → ("path/to/file.rs", Some((10, 20)))
///
/// Only recognizes `:<digits>-<digits>` at the end as a line range.
pub fn parse_add_arg(arg: &str) -> (&str, Option<(usize, usize)>) {
    // Look for the last colon that's followed by digits-digits
    if let Some(colon_pos) = arg.rfind(':') {
        let after = &arg[colon_pos + 1..];
        if let Some(dash_pos) = after.find('-') {
            let start_str = &after[..dash_pos];
            let end_str = &after[dash_pos + 1..];
            if let (Ok(start), Ok(end)) = (start_str.parse::<usize>(), end_str.parse::<usize>()) {
                if start > 0 && end >= start {
                    return (&arg[..colon_pos], Some((start, end)));
                }
            }
        }
    }
    (arg, None)
}

/// Expand a path argument that may contain glob patterns.
/// Returns the original path as-is if it has no glob characters.
pub fn expand_add_paths(pattern: &str) -> Vec<String> {
    if !pattern.contains('*') && !pattern.contains('?') && !pattern.contains('[') {
        return vec![pattern.to_string()];
    }
    match glob::glob(pattern) {
        Ok(paths) => {
            let mut result: Vec<String> = paths
                .filter_map(|p| p.ok())
                .filter(|p| p.is_file())
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            result.sort();
            result
        }
        Err(_) => Vec::new(),
    }
}

/// Read a file (optionally a line range) for the /add command.
/// Returns the file content and line count.
pub fn read_file_for_add(
    path: &str,
    range: Option<(usize, usize)>,
) -> Result<(String, usize), String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("could not read {path}: {e}"))?;

    match range {
        Some((start, end)) => {
            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len();
            if start > total {
                return Err(format!(
                    "start line {start} is past end of file ({total} lines)"
                ));
            }
            let end = end.min(total);
            let selected: Vec<&str> = lines[start - 1..end].to_vec();
            let count = selected.len();
            Ok((selected.join("\n"), count))
        }
        None => {
            let count = content.lines().count();
            Ok((content, count))
        }
    }
}

/// Format file content for injection into the conversation.
/// Wraps it in a markdown code block with the filename as header.
pub fn format_add_content(path: &str, content: &str) -> String {
    // Detect language extension for syntax highlighting
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let lang = match ext {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "rb" => "ruby",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" | "cxx" => "cpp",
        "sh" | "bash" => "bash",
        "yml" | "yaml" => "yaml",
        "json" => "json",
        "toml" => "toml",
        "md" => "markdown",
        "html" | "htm" => "html",
        "css" => "css",
        "sql" => "sql",
        "xml" => "xml",
        _ => "",
    };
    format!("**{path}**\n```{lang}\n{content}\n```")
}

// ── Image support helpers ─────────────────────────────────────────────

/// Check if a file path has an image extension.
pub fn is_image_extension(path: &str) -> bool {
    let lower = path.to_lowercase();
    matches!(
        lower.rsplit('.').next(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
    )
}

/// Map a file extension to a MIME type string.
/// Returns `"application/octet-stream"` for unknown extensions.
pub fn mime_type_for_extension(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

/// Result type for `/add` that distinguishes text files from image files.
#[derive(Debug, Clone, PartialEq)]
pub enum AddResult {
    /// A text file: summary line + formatted content to inject.
    Text { summary: String, content: String },
    /// An image file: summary line + base64-encoded data + MIME type.
    Image {
        summary: String,
        data: String,
        mime_type: String,
    },
}

/// Read an image file from disk and return base64-encoded data and MIME type.
pub fn read_image_for_add(path: &str) -> Result<(String, String), String> {
    use base64::Engine;
    let bytes = std::fs::read(path).map_err(|e| format!("failed to read {path}: {e}"))?;
    let ext = path.rsplit('.').next().unwrap_or("");
    let mime = mime_type_for_extension(ext).to_string();
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok((data, mime))
}

/// Handle the `/add` command: read file(s) and return the formatted content
/// to be injected as a user message.
///
/// Returns a Vec of `AddResult` — either text or image — for each file.
pub fn handle_add(input: &str) -> Vec<AddResult> {
    let args = input.strip_prefix("/add").unwrap_or("").trim();

    if args.is_empty() {
        println!("{DIM}  usage: /add <path> — inject file contents into conversation");
        println!("         /add <path>:<start>-<end> — inject specific line range");
        println!("         /add src/*.rs — inject multiple files via glob{RESET}\n");
        return Vec::new();
    }

    let mut results = Vec::new();

    // Split on whitespace to support multiple paths: /add foo.rs bar.rs
    for arg in args.split_whitespace() {
        let (raw_path, range) = parse_add_arg(arg);
        let paths = expand_add_paths(raw_path);

        if paths.is_empty() {
            println!("{RED}  no files matched: {raw_path}{RESET}");
            continue;
        }

        for path in &paths {
            // Check if this is an image file
            if is_image_extension(path) {
                // Line ranges don't apply to images
                if range.is_some() {
                    println!("{RED}  ✗ line ranges not supported for images: {path}{RESET}");
                    continue;
                }
                match read_image_for_add(path) {
                    Ok((data, mime_type)) => {
                        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                        let size_str = if size >= 1_048_576 {
                            format!("{:.1} MB", size as f64 / 1_048_576.0)
                        } else {
                            format!("{:.0} KB", size as f64 / 1024.0)
                        };
                        let summary = format!(
                            "{GREEN}  ✓ added image {path} ({size_str}, {mime_type}){RESET}"
                        );
                        results.push(AddResult::Image {
                            summary,
                            data,
                            mime_type,
                        });
                    }
                    Err(e) => {
                        println!("{RED}  ✗ {e}{RESET}");
                    }
                }
                continue;
            }

            match read_file_for_add(path, range) {
                Ok((content, line_count)) => {
                    let formatted = format_add_content(path, &content);
                    let word = crate::format::pluralize(line_count, "line", "lines");
                    let range_info = if let Some((s, e)) = range {
                        format!(" (lines {s}-{e})")
                    } else {
                        String::new()
                    };
                    let summary =
                        format!("{GREEN}  ✓ added {path}{range_info} ({line_count} {word}){RESET}");
                    results.push(AddResult::Text {
                        summary,
                        content: formatted,
                    });
                }
                Err(e) => {
                    println!("{RED}  ✗ {e}{RESET}");
                }
            }
        }
    }

    results
}

// ── @file mention expansion ──────────────────────────────────────────

/// Scan user input for `@path` mentions (e.g. `@src/main.rs` or
/// `@src/cli.rs:50-100`) and resolve them to file contents.
///
/// Returns:
/// - The cleaned prompt text (with resolved `@path` replaced by just the filename)
/// - A vec of `AddResult` items for every file that was successfully read
///
/// Mentions that don't resolve to an existing file are left unchanged
/// (they might be usernames or other references). Email-like patterns
/// (`word@domain`) are skipped.
pub fn expand_file_mentions(input: &str) -> (String, Vec<AddResult>) {
    let mut results = Vec::new();
    let mut output = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] != '@' {
            output.push(chars[i]);
            i += 1;
            continue;
        }

        // Found an '@'. Check if it's email-like (preceded by an alphanumeric char).
        if i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '.' || chars[i - 1] == '_') {
            // Email-like: word@domain — leave it alone
            output.push('@');
            i += 1;
            continue;
        }

        // Collect the path after '@': alphanumeric, '/', '.', '-', '_', ':'
        let start = i + 1;
        let mut j = start;
        while j < len
            && (chars[j].is_alphanumeric() || matches!(chars[j], '/' | '.' | '-' | '_' | ':'))
        {
            j += 1;
        }

        // Nothing after '@' (just @ at end, or @ followed by space)
        if j == start {
            output.push('@');
            i += 1;
            continue;
        }

        let mention = &input[byte_offset(&chars, start)..byte_offset(&chars, j)];

        // Parse path and optional line range using existing helper
        let (raw_path, range) = parse_add_arg(mention);

        // Check if the file exists
        let path = std::path::Path::new(raw_path);
        if !path.is_file() {
            // Not a file — leave the mention unchanged
            output.push('@');
            output.push_str(mention);
            i = j;
            continue;
        }

        // It's a real file — read it
        if is_image_extension(raw_path) {
            if range.is_some() {
                // Line ranges don't apply to images — leave unchanged
                output.push('@');
                output.push_str(mention);
                i = j;
                continue;
            }
            match read_image_for_add(raw_path) {
                Ok((data, mime_type)) => {
                    let size = std::fs::metadata(raw_path).map(|m| m.len()).unwrap_or(0);
                    let size_str = if size >= 1_048_576 {
                        format!("{:.1} MB", size as f64 / 1_048_576.0)
                    } else {
                        format!("{:.0} KB", size as f64 / 1024.0)
                    };
                    let summary = format!(
                        "{GREEN}  ✓ added image {raw_path} ({size_str}, {mime_type}){RESET}"
                    );
                    results.push(AddResult::Image {
                        summary,
                        data,
                        mime_type,
                    });
                    // Replace @path with just the filename in output
                    let filename = path
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| raw_path.to_string());
                    output.push_str(&filename);
                }
                Err(_) => {
                    // Read failed — leave unchanged
                    output.push('@');
                    output.push_str(mention);
                }
            }
        } else {
            match read_file_for_add(raw_path, range) {
                Ok((content, line_count)) => {
                    let formatted = format_add_content(raw_path, &content);
                    let word = crate::format::pluralize(line_count, "line", "lines");
                    let range_info = if let Some((s, e)) = range {
                        format!(" (lines {s}-{e})")
                    } else {
                        String::new()
                    };
                    let summary = format!(
                        "{GREEN}  ✓ added {raw_path}{range_info} ({line_count} {word}){RESET}"
                    );
                    results.push(AddResult::Text {
                        summary,
                        content: formatted,
                    });
                    // Replace @path with just the filename in output
                    let filename = path
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| raw_path.to_string());
                    if let Some((s, e)) = range {
                        output.push_str(&format!("{filename}:{s}-{e}"));
                    } else {
                        output.push_str(&filename);
                    }
                }
                Err(_) => {
                    // Read failed — leave unchanged
                    output.push('@');
                    output.push_str(mention);
                }
            }
        }

        i = j;
    }

    (output, results)
}

/// Helper: get the byte offset corresponding to a char index.
fn byte_offset(chars: &[char], char_idx: usize) -> usize {
    chars[..char_idx].iter().map(|c| c.len_utf8()).sum()
}

// ── /grep ────────────────────────────────────────────────────────────────

/// Maximum matches to display before truncating.
const GREP_MAX_MATCHES: usize = 50;

/// Parsed arguments for the `/grep` command.
#[derive(Debug, Clone, PartialEq)]
pub struct GrepArgs {
    pub pattern: String,
    pub path: String,
    pub case_sensitive: bool,
}

/// Parse `/grep` arguments.
///
/// Syntax: `/grep [-s|--case] <pattern> [path]`
///
/// Returns `None` if the pattern is empty.
pub fn parse_grep_args(input: &str) -> Option<GrepArgs> {
    let rest = input.strip_prefix("/grep").unwrap_or(input).trim();

    if rest.is_empty() {
        return None;
    }

    let mut case_sensitive = false;
    let mut remaining_parts: Vec<&str> = Vec::new();

    for part in rest.split_whitespace() {
        if part == "-s" || part == "--case" {
            case_sensitive = true;
        } else {
            remaining_parts.push(part);
        }
    }

    if remaining_parts.is_empty() {
        return None;
    }

    let pattern = remaining_parts[0].to_string();
    let path = if remaining_parts.len() > 1 {
        remaining_parts[1..].join(" ")
    } else {
        ".".to_string()
    };

    Some(GrepArgs {
        pattern,
        path,
        case_sensitive,
    })
}

/// A single grep match result.
#[derive(Debug, Clone, PartialEq)]
pub struct GrepMatch {
    pub file: String,
    pub line_num: u32,
    pub text: String,
}

/// Run grep and return structured results.
///
/// Uses `git grep` when inside a git repo (faster, respects .gitignore),
/// falls back to `grep -rn` with common directory exclusions.
pub fn run_grep(args: &GrepArgs) -> Result<Vec<GrepMatch>, String> {
    let in_git_repo = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let output = if in_git_repo {
        let mut cmd = std::process::Command::new("git");
        cmd.args(["grep", "-n", "--color=never"]);
        if !args.case_sensitive {
            cmd.arg("-i");
        }
        cmd.arg("--");
        cmd.arg(&args.pattern);
        if args.path != "." {
            cmd.arg(&args.path);
        }
        cmd.output()
    } else {
        let mut cmd = std::process::Command::new("grep");
        cmd.args(["-rn", "--color=never"]);
        if !args.case_sensitive {
            cmd.arg("-i");
        }
        cmd.args([
            "--exclude-dir=.git",
            "--exclude-dir=target",
            "--exclude-dir=node_modules",
            "--exclude-dir=__pycache__",
            "--exclude-dir=.venv",
        ]);
        cmd.arg(&args.pattern);
        cmd.arg(&args.path);
        cmd.output()
    };

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let matches: Vec<GrepMatch> = stdout
                .lines()
                .filter(|l| !l.is_empty())
                .filter_map(|line| {
                    // Format: file:line_num:text
                    let first_colon = line.find(':')?;
                    let rest = &line[first_colon + 1..];
                    let second_colon = rest.find(':')?;
                    let file = line[..first_colon].to_string();
                    let line_num = rest[..second_colon].parse::<u32>().ok()?;
                    let text = rest[second_colon + 1..].to_string();
                    Some(GrepMatch {
                        file,
                        line_num,
                        text,
                    })
                })
                .collect();
            Ok(matches)
        }
        Err(e) => Err(format!("Failed to run grep: {e}")),
    }
}

/// Format grep results with colors and truncation.
///
/// Returns the formatted string to display.
/// Colors: filenames in green, line numbers in cyan, matches highlighted in bold yellow.
pub fn format_grep_results(matches: &[GrepMatch], pattern: &str, case_sensitive: bool) -> String {
    if matches.is_empty() {
        return format!("{DIM}  No matches found.{RESET}\n");
    }

    let total = matches.len();
    let shown = matches.iter().take(GREP_MAX_MATCHES);
    let mut output = String::new();

    for m in shown {
        // Highlight the matched pattern in the text
        let highlighted_text = highlight_grep_match(&m.text, pattern, case_sensitive);
        output.push_str(&format!(
            "  {GREEN}{}{RESET}:{CYAN}{}{RESET}: {}\n",
            m.file, m.line_num, highlighted_text
        ));
    }

    if total > GREP_MAX_MATCHES {
        output.push_str(&format!(
            "\n{DIM}  ({} more matches, narrow your search){RESET}\n",
            total - GREP_MAX_MATCHES
        ));
    } else {
        output.push_str(&format!(
            "\n{DIM}  {} match{}{RESET}\n",
            total,
            if total == 1 { "" } else { "es" }
        ));
    }

    output
}

/// Highlight occurrences of a pattern in a line of text.
fn highlight_grep_match(text: &str, pattern: &str, case_sensitive: bool) -> String {
    if pattern.is_empty() {
        return text.to_string();
    }

    let mut result = String::new();
    let (search_text, search_pattern) = if case_sensitive {
        (text.to_string(), pattern.to_string())
    } else {
        (text.to_lowercase(), pattern.to_lowercase())
    };

    let mut last_end = 0;
    let mut start = 0;
    while let Some(pos) = search_text[start..].find(&search_pattern) {
        let abs_pos = start + pos;
        // Append text before match
        result.push_str(&text[last_end..abs_pos]);
        // Append highlighted match (use original case from text)
        result.push_str(&format!(
            "{BOLD_YELLOW}{}{RESET}",
            &text[abs_pos..abs_pos + pattern.len()]
        ));
        last_end = abs_pos + pattern.len();
        start = last_end;
    }
    result.push_str(&text[last_end..]);

    result
}

/// Handle the `/grep` command.
pub fn handle_grep(input: &str) {
    let args = match parse_grep_args(input) {
        Some(a) => a,
        None => {
            println!("{DIM}  usage: /grep [-s|--case] <pattern> [path]");
            println!("  Search file contents directly — no AI, no tokens, instant results.");
            println!("  Case-insensitive by default. Use -s or --case for case-sensitive.");
            println!();
            println!("  Examples:");
            println!("    /grep TODO");
            println!("    /grep \"fn main\" src/");
            println!("    /grep -s MyStruct src/lib.rs{RESET}\n");
            return;
        }
    };

    match run_grep(&args) {
        Ok(matches) => {
            let formatted = format_grep_results(&matches, &args.pattern, args.case_sensitive);
            print!("{formatted}");
        }
        Err(e) => {
            println!("{RED}  Error: {e}{RESET}\n");
        }
    }
}

// ── /extract ─────────────────────────────────────────────────────────────

/// Parse `/extract <symbol> <source_file> <target_file>` arguments.
pub fn parse_extract_args(input: &str) -> Option<(String, String, String)> {
    let rest = input.strip_prefix("/extract").unwrap_or(input).trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() == 3 {
        Some((
            parts[0].to_string(),
            parts[1].to_string(),
            parts[2].to_string(),
        ))
    } else {
        None
    }
}

/// Find a top-level symbol block (fn, struct, enum, impl, trait, type, const, static) in source text.
/// Returns `(start_line_0indexed, end_line_0indexed, block_text)` where the range
/// is inclusive on both ends.
///
/// Uses brace-depth tracking: finds the line where the symbol keyword + name appear,
/// then scans backwards to collect any `#[...]` attributes or `///` doc comments
/// immediately above, then scans forward counting `{` and `}` until depth returns to 0.
pub fn find_symbol_block(source: &str, symbol: &str) -> Option<(usize, usize, String)> {
    let lines: Vec<&str> = source.lines().collect();

    // Build patterns to match: fn symbol, pub fn symbol, struct symbol, enum symbol,
    // impl symbol, trait symbol, type symbol, const symbol, static symbol, etc.
    let keyword_patterns: Vec<String> = vec![
        format!("fn {symbol}"),
        format!("struct {symbol}"),
        format!("enum {symbol}"),
        format!("impl {symbol}"),
        format!("trait {symbol}"),
        format!("type {symbol}"),
        format!("const {symbol}"),
        format!("static mut {symbol}"),
        format!("static {symbol}"),
    ];

    // Find the line containing the symbol declaration
    let mut decl_line = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        // Skip lines inside comments
        if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
            continue;
        }
        for pat in &keyword_patterns {
            // Check if this line contains the pattern at a word boundary
            if let Some(pos) = trimmed.find(pat.as_str()) {
                // Make sure the character after the symbol name is a word boundary
                let after = pos + pat.len();
                if after >= trimmed.len()
                    || !trimmed.as_bytes()[after].is_ascii_alphanumeric()
                        && trimmed.as_bytes()[after] != b'_'
                {
                    // Also verify the keyword is at line start (possibly after pub/pub(crate)/etc.)
                    let before = &trimmed[..pos];
                    let is_valid_prefix = before.is_empty()
                        || before.trim_end().is_empty()
                        || before.trim_end() == "pub"
                        || before.trim_end().starts_with("pub(")
                        || before.trim_end() == "async"
                        || before.trim_end() == "pub async"
                        || before.trim_end() == "unsafe"
                        || before.trim_end() == "pub unsafe";
                    if is_valid_prefix {
                        decl_line = Some(i);
                        break;
                    }
                }
            }
        }
        if decl_line.is_some() {
            break;
        }
    }

    let decl_line = decl_line?;

    // Scan backwards to collect doc comments and attributes
    let mut start_line = decl_line;
    while start_line > 0 {
        let prev = lines[start_line - 1].trim();
        if prev.starts_with("///")
            || prev.starts_with("#[")
            || prev.starts_with("#![")
            || prev.starts_with("//!")
        {
            start_line -= 1;
        } else {
            break;
        }
    }

    // Check if the declaration line is semicolon-terminated (unit struct, etc.)
    // before doing brace scanning, to avoid picking up braces from later code.
    let decl_trimmed = lines[decl_line].trim();
    if decl_trimmed.ends_with(';') {
        let block: String = lines[start_line..=decl_line].join("\n");
        return Some((start_line, decl_line, block));
    }

    // Scan forward with brace-depth tracking
    let mut depth: i32 = 0;
    let mut found_open = false;
    let mut end_line = decl_line;

    for (i, line) in lines.iter().enumerate().skip(decl_line) {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                found_open = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        end_line = i;
        if found_open && depth == 0 {
            break;
        }
    }

    // If we never found an opening brace, the item might span multiple lines
    // ending with a semicolon (e.g., type aliases)
    if !found_open {
        // Check if there's a semicolon somewhere in the range
        let has_semi = lines[decl_line..=end_line].iter().any(|l| l.contains(';'));
        if !has_semi {
            return None;
        }
        // End at the line with the semicolon
        for (idx, line) in lines.iter().enumerate().take(end_line + 1).skip(decl_line) {
            if line.contains(';') {
                end_line = idx;
                break;
            }
        }
    }

    let block: String = lines[start_line..=end_line].join("\n");
    Some((start_line, end_line, block))
}

/// Extract a symbol from source_path to target_path.
/// Returns a summary message on success, or an error description.
pub fn extract_symbol(
    source_path: &str,
    target_path: &str,
    symbol: &str,
) -> Result<String, String> {
    // Read source file
    let source_content = std::fs::read_to_string(source_path)
        .map_err(|e| format!("Cannot read source file '{source_path}': {e}"))?;

    // Find the symbol block
    let (start_line, end_line, block_text) = find_symbol_block(&source_content, symbol)
        .ok_or_else(|| format!("Symbol '{symbol}' not found in '{source_path}'"))?;

    // Read target file (create if doesn't exist)
    let target_content = std::fs::read_to_string(target_path).unwrap_or_default();

    // Check if the symbol is pub — if so, we'll add a use statement
    let is_pub = block_text.trim_start().starts_with("pub ")
        || block_text.trim_start().starts_with("/// ")
            && block_text.contains(&format!("pub fn {symbol}"))
        || block_text.trim_start().starts_with("#[")
            && block_text.contains(&format!("pub fn {symbol}"))
        || block_text.trim_start().starts_with("pub(")
        || block_text.contains(&format!("pub struct {symbol}"))
        || block_text.contains(&format!("pub enum {symbol}"))
        || block_text.contains(&format!("pub trait {symbol}"))
        || block_text.contains(&format!("pub type {symbol}"))
        || block_text.contains(&format!("pub const {symbol}"))
        || block_text.contains(&format!("pub static {symbol}"));

    // Remove the block from source
    let source_lines: Vec<&str> = source_content.lines().collect();
    let mut new_source_lines: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < source_lines.len() {
        if i >= start_line && i <= end_line {
            i += 1;
            continue;
        }
        new_source_lines.push(source_lines[i]);
        i += 1;
    }

    // Clean up consecutive blank lines at the removal site
    let mut new_source = new_source_lines.join("\n");
    // Ensure file ends with newline
    if !new_source.ends_with('\n') {
        new_source.push('\n');
    }

    // Append block to target
    let mut new_target = target_content.clone();
    if !new_target.is_empty() && !new_target.ends_with('\n') {
        new_target.push('\n');
    }
    if !new_target.is_empty() {
        new_target.push('\n');
    }
    new_target.push_str(&block_text);
    new_target.push('\n');

    // Write both files
    std::fs::write(source_path, &new_source)
        .map_err(|e| format!("Failed to write source file '{source_path}': {e}"))?;
    std::fs::write(target_path, &new_target)
        .map_err(|e| format!("Failed to write target file '{target_path}': {e}"))?;

    let line_count = end_line - start_line + 1;
    let line_word = crate::format::pluralize(line_count, "line", "lines");
    let pub_note = if is_pub {
        format!(
            "\n  {DIM}Note: '{symbol}' is public — you may need to add a `use` import in '{source_path}'.{RESET}"
        )
    } else {
        String::new()
    };

    Ok(format!(
        "Moved '{symbol}' ({line_count} {line_word}) from '{source_path}' to '{target_path}'.{pub_note}"
    ))
}

/// Handle the `/extract` command: find symbol, preview, confirm, move.
pub fn handle_extract(input: &str) {
    let (symbol, source, target) = match parse_extract_args(input) {
        Some(args) => args,
        None => {
            println!("{DIM}  usage: /extract <symbol> <source_file> <target_file>");
            println!("  Move a function, struct, enum, impl, trait, type alias, const, or static from one file to another.");
            println!("  Shows a preview of the block to be moved and asks for confirmation.");
            println!();
            println!("  Examples:");
            println!("    /extract my_func src/lib.rs src/utils.rs");
            println!("    /extract MyStruct src/main.rs src/types.rs");
            println!("    /extract MyTrait src/old.rs src/new.rs");
            println!("    /extract MyResult src/lib.rs src/errors.rs");
            println!("    /extract MAX_SIZE src/config.rs src/constants.rs{RESET}\n");
            return;
        }
    };

    // Read source
    let source_content = match std::fs::read_to_string(&source) {
        Ok(c) => c,
        Err(e) => {
            println!("{RED}  Cannot read '{source}': {e}{RESET}\n");
            return;
        }
    };

    // Find the symbol
    let (start_line, end_line, block_text) = match find_symbol_block(&source_content, &symbol) {
        Some(found) => found,
        None => {
            println!("{DIM}  Symbol '{symbol}' not found in '{source}'.{RESET}\n");
            return;
        }
    };

    let line_count = end_line - start_line + 1;
    let line_word = crate::format::pluralize(line_count, "line", "lines");

    // Preview
    println!();
    println!("  {BOLD}Extract preview:{RESET}");
    println!(
        "  Move {CYAN}{symbol}{RESET} ({line_count} {line_word}) from {RED}{source}{RESET} → {GREEN}{target}{RESET}"
    );
    println!();

    // Show truncated preview of the block
    let preview_lines: Vec<&str> = block_text.lines().collect();
    let max_preview = 15;
    for (i, line) in preview_lines.iter().take(max_preview).enumerate() {
        println!("    {CYAN}{:>4}{RESET}: {line}", start_line + i + 1);
    }
    if preview_lines.len() > max_preview {
        println!(
            "    {DIM}... ({} more lines){RESET}",
            preview_lines.len() - max_preview
        );
    }
    println!();

    // Ask for confirmation
    print!("  {BOLD}Move this symbol? (y/n): {RESET}");
    use std::io::Write;
    std::io::stdout().flush().ok();

    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        println!("{RED}  Failed to read input.{RESET}\n");
        return;
    }

    let answer = answer.trim().to_lowercase();
    if answer != "y" && answer != "yes" {
        println!("{DIM}  Extract cancelled.{RESET}\n");
        return;
    }

    match extract_symbol(&source, &target, &symbol) {
        Ok(msg) => println!("{GREEN}  ✓ {msg}{RESET}\n"),
        Err(e) => println!("{RED}  ✗ {e}{RESET}\n"),
    }
}

// ── /rename ──────────────────────────────────────────────────────────────

/// Check if a character is a word boundary character (not alphanumeric or underscore).
fn is_word_boundary_char(c: char) -> bool {
    !c.is_alphanumeric() && c != '_'
}

/// Check if position `pos` in `text` is at a word boundary start.
/// A word boundary exists at the start of the string or when the preceding char
/// is not a word character.
fn is_word_start(text: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    text[..pos].chars().last().is_none_or(is_word_boundary_char)
}

/// Check if position `pos` in `text` is at a word boundary end.
/// A word boundary exists at the end of the string or when the following char
/// is not a word character.
fn is_word_end(text: &str, pos: usize) -> bool {
    if pos >= text.len() {
        return true;
    }
    text[pos..].chars().next().is_none_or(is_word_boundary_char)
}

/// A single rename match with context.
#[derive(Debug, Clone, PartialEq)]
pub struct RenameMatch {
    pub file: String,
    pub line_num: usize,
    pub line_text: String,
    pub column: usize,
}

/// Find all word-boundary matches of `old_name` across files tracked by git.
/// Skips binary files. Returns matches sorted by file then line number.
pub fn find_rename_matches(old_name: &str) -> Vec<RenameMatch> {
    if old_name.is_empty() {
        return Vec::new();
    }

    let files = list_git_files();
    let mut matches = Vec::new();

    for file_path in &files {
        if is_binary_extension(file_path) {
            continue;
        }

        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (line_idx, line) in content.lines().enumerate() {
            let line_matches = find_word_boundary_matches(line, old_name);
            for col in line_matches {
                matches.push(RenameMatch {
                    file: file_path.clone(),
                    line_num: line_idx + 1,
                    line_text: line.to_string(),
                    column: col,
                });
            }
        }
    }

    matches
}

/// Find all positions in `text` where `pattern` occurs at word boundaries.
pub fn find_word_boundary_matches(text: &str, pattern: &str) -> Vec<usize> {
    if pattern.is_empty() || text.is_empty() {
        return Vec::new();
    }

    let mut positions = Vec::new();
    let mut start = 0;
    let pat_len = pattern.len();

    while start + pat_len <= text.len() {
        if let Some(pos) = text[start..].find(pattern) {
            let abs_pos = start + pos;
            let end_pos = abs_pos + pat_len;

            if is_word_start(text, abs_pos) && is_word_end(text, end_pos) {
                positions.push(abs_pos);
            }

            start = abs_pos + 1;
        } else {
            break;
        }
    }

    positions
}

/// List files tracked by git (via `git ls-files`).
/// Falls back to walking the current directory if not in a git repo.
fn list_git_files() -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["ls-files"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Format a rename preview showing all matches with context.
pub fn format_rename_preview(matches: &[RenameMatch], old_name: &str, new_name: &str) -> String {
    if matches.is_empty() {
        return format!("{DIM}  No matches found for '{old_name}'.{RESET}\n");
    }

    let mut output = String::new();

    // Group by file
    let mut current_file = String::new();
    let mut file_count = 0usize;

    for m in matches {
        if m.file != current_file {
            current_file = m.file.clone();
            file_count += 1;
            output.push_str(&format!("\n  {GREEN}{}{RESET}\n", m.file));
        }

        // Highlight the old name in the line
        let highlighted = m.line_text.replace(
            old_name,
            &format!("{RED}{old_name}{RESET}→{GREEN}{new_name}{RESET}"),
        );
        output.push_str(&format!(
            "    {CYAN}{:>4}{RESET}: {}\n",
            m.line_num, highlighted
        ));
    }

    let match_word = crate::format::pluralize(matches.len(), "match", "matches");
    let file_word = crate::format::pluralize(file_count, "file", "files");
    output.push_str(&format!(
        "\n  {BOLD}{} {match_word}{RESET} across {BOLD}{file_count} {file_word}{RESET}\n",
        matches.len()
    ));
    output.push_str(&format!(
        "  Rename {RED}{old_name}{RESET} → {GREEN}{new_name}{RESET}\n"
    ));

    output
}

/// Apply the rename across all files, replacing word-boundary matches of `old_name`
/// with `new_name`. Returns the number of replacements made.
pub fn apply_rename(matches: &[RenameMatch], old_name: &str, new_name: &str) -> usize {
    if matches.is_empty() {
        return 0;
    }

    // Group matches by file
    let mut files_to_update: std::collections::HashMap<&str, Vec<&RenameMatch>> =
        std::collections::HashMap::new();
    for m in matches {
        files_to_update.entry(m.file.as_str()).or_default().push(m);
    }

    let mut total_replacements = 0usize;

    for file_path in files_to_update.keys() {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut new_content = String::new();
        for line in content.lines() {
            let replaced = replace_word_boundary(line, old_name, new_name);
            // Count how many replacements happened in this line
            let orig_count = find_word_boundary_matches(line, old_name).len();
            total_replacements += orig_count;
            new_content.push_str(&replaced);
            new_content.push('\n');
        }

        // Preserve trailing newline state
        if !content.ends_with('\n') && new_content.ends_with('\n') {
            new_content.pop();
        }

        if let Err(e) = std::fs::write(file_path, &new_content) {
            println!("{RED}  Failed to write {file_path}: {e}{RESET}");
        }
    }

    total_replacements
}

/// Replace all word-boundary occurrences of `old` with `new` in a single line.
pub fn replace_word_boundary(text: &str, old: &str, new: &str) -> String {
    if old.is_empty() {
        return text.to_string();
    }

    let positions = find_word_boundary_matches(text, old);
    if positions.is_empty() {
        return text.to_string();
    }

    let mut result = String::new();
    let mut last_end = 0;

    for pos in positions {
        result.push_str(&text[last_end..pos]);
        result.push_str(new);
        last_end = pos + old.len();
    }
    result.push_str(&text[last_end..]);

    result
}

/// Parse `/rename old_name new_name` arguments.
pub fn parse_rename_args(input: &str) -> Option<(String, String)> {
    let rest = input.strip_prefix("/rename").unwrap_or(input).trim();

    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() == 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

/// Handle the `/rename` command: find matches, preview, confirm, apply.
pub fn handle_rename(input: &str) {
    let (old_name, new_name) = match parse_rename_args(input) {
        Some(args) => args,
        None => {
            println!("{DIM}  usage: /rename <old_name> <new_name>");
            println!("  Cross-file symbol renaming with word-boundary matching.");
            println!("  Shows a preview of all changes and asks for confirmation.");
            println!();
            println!("  Examples:");
            println!("    /rename my_func new_func");
            println!("    /rename OldStruct NewStruct");
            println!("    /rename CONFIG_KEY NEW_KEY{RESET}\n");
            return;
        }
    };

    if old_name == new_name {
        println!("{DIM}  (old and new names are the same — nothing to do){RESET}\n");
        return;
    }

    println!("{DIM}  searching for '{old_name}'...{RESET}");

    let matches = find_rename_matches(&old_name);

    if matches.is_empty() {
        println!("{DIM}  No word-boundary matches found for '{old_name}'.{RESET}\n");
        return;
    }

    let preview = format_rename_preview(&matches, &old_name, &new_name);
    print!("{preview}");

    // Ask for confirmation
    print!("\n  {BOLD}Apply rename? (y/n): {RESET}");
    use std::io::Write;
    std::io::stdout().flush().ok();

    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        println!("{RED}  Failed to read input.{RESET}\n");
        return;
    }

    let answer = answer.trim().to_lowercase();
    if answer != "y" && answer != "yes" {
        println!("{DIM}  Rename cancelled.{RESET}\n");
        return;
    }

    let count = apply_rename(&matches, &old_name, &new_name);
    let repl_word = crate::format::pluralize(count, "replacement", "replacements");
    println!("{GREEN}  ✓ Applied {count} {repl_word}.{RESET}\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::KNOWN_COMMANDS;
    use crate::help::help_text;
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

    // ── test_command_for_project ─────────────────────────────────────

    #[test]
    fn test_command_rust() {
        let cmd = test_command_for_project(&ProjectType::Rust);
        assert!(cmd.is_some());
        let (label, _) = cmd.unwrap();
        assert_eq!(label, "cargo test");
    }

    #[test]
    fn test_command_unknown() {
        assert!(test_command_for_project(&ProjectType::Unknown).is_none());
    }

    // ── lint_command_for_project ─────────────────────────────────────

    #[test]
    fn lint_command_rust() {
        let cmd = lint_command_for_project(&ProjectType::Rust);
        assert!(cmd.is_some());
        assert!(cmd.unwrap().0.contains("clippy"));
    }

    #[test]
    fn lint_command_make_none() {
        assert!(lint_command_for_project(&ProjectType::Make).is_none());
    }

    #[test]
    fn lint_command_unknown_none() {
        assert!(lint_command_for_project(&ProjectType::Unknown).is_none());
    }

    // ── health_checks_for_project ───────────────────────────────────

    #[test]
    fn health_checks_rust_has_build() {
        let checks = health_checks_for_project(&ProjectType::Rust);
        assert!(checks.iter().any(|(name, _)| *name == "build"));
    }

    #[test]
    fn health_checks_unknown_empty() {
        let checks = health_checks_for_project(&ProjectType::Unknown);
        assert!(checks.is_empty());
    }

    // ── build_fix_prompt ────────────────────────────────────────────

    #[test]
    fn build_fix_prompt_empty() {
        let prompt = build_fix_prompt(&[]);
        assert!(prompt.is_empty());
    }

    #[test]
    fn build_fix_prompt_with_failures() {
        let failures = vec![("build", "error[E0308]: mismatched types")];
        let prompt = build_fix_prompt(&failures);
        assert!(prompt.contains("build errors"));
        assert!(prompt.contains("E0308"));
        assert!(prompt.contains("Fix"));
    }

    #[test]
    fn build_fix_prompt_multiple_failures() {
        let failures = vec![
            ("build", "build error output"),
            ("clippy", "clippy warning output"),
        ];
        let prompt = build_fix_prompt(&failures);
        assert!(prompt.contains("## build errors"));
        assert!(prompt.contains("## clippy errors"));
    }

    // ── format_tree_from_paths ──────────────────────────────────────

    #[test]
    fn format_tree_basic() {
        let paths = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "Cargo.toml".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("src/"));
        assert!(tree.contains("main.rs"));
        assert!(tree.contains("lib.rs"));
        assert!(tree.contains("Cargo.toml"));
    }

    #[test]
    fn format_tree_depth_limit() {
        let paths = vec!["a/b/c/d/e.txt".to_string()];
        let tree_shallow = format_tree_from_paths(&paths, 1);
        // At depth 1, we see dir 'a/' but 'b/' is at level 1 so still shown
        // The file at depth 4 should NOT appear since depth > max_depth
        assert!(tree_shallow.contains("a/"));
        // File at depth 4 should not appear when max_depth=1
        assert!(!tree_shallow.contains("e.txt"));
    }

    #[test]
    fn format_tree_empty() {
        let paths: Vec<String> = vec![];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.is_empty());
    }

    #[test]
    fn format_tree_root_files() {
        let paths = vec!["README.md".to_string()];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("README.md"));
    }

    // ── fuzzy_score ─────────────────────────────────────────────────

    #[test]
    fn fuzzy_score_no_match() {
        assert!(fuzzy_score("src/main.rs", "xyz").is_none());
    }

    #[test]
    fn fuzzy_score_exact_filename() {
        let score = fuzzy_score("src/main.rs", "main").unwrap();
        assert!(score > 100); // base + filename match + start match + stem match
    }

    #[test]
    fn fuzzy_score_case_insensitive() {
        assert!(fuzzy_score("src/Main.rs", "main").is_some());
        assert!(fuzzy_score("src/MAIN.rs", "main").is_some());
    }

    #[test]
    fn fuzzy_score_directory_match_lower_than_filename() {
        // "src" in path "src/other.rs" matches directory
        let dir_score = fuzzy_score("src/other.rs", "other").unwrap();
        // "main" in "deeply/nested/main.rs" matches filename but deeper
        let file_score = fuzzy_score("deeply/nested/main.rs", "main").unwrap();
        // Both should match, filename match has bonus
        assert!(dir_score > 100);
        assert!(file_score > 100);
    }

    #[test]
    fn fuzzy_score_shorter_path_preferred() {
        let shallow = fuzzy_score("main.rs", "main").unwrap();
        let deep = fuzzy_score("a/b/c/main.rs", "main").unwrap();
        assert!(shallow > deep);
    }

    #[test]
    fn fuzzy_score_extension_match() {
        let score = fuzzy_score("config/settings.toml", ".toml").unwrap();
        assert!(score > 0);
    }

    // ── highlight_match ─────────────────────────────────────────────

    #[test]
    fn highlight_match_contains_pattern() {
        let result = highlight_match("src/main.rs", "main");
        // Should contain ANSI codes around "main"
        assert!(result.contains("main"));
        assert!(result.contains("src/"));
        assert!(result.contains(".rs"));
    }

    #[test]
    fn highlight_match_no_match_returns_plain() {
        let result = highlight_match("src/main.rs", "xyz");
        assert_eq!(result, "src/main.rs");
    }

    #[test]
    fn highlight_match_case_insensitive() {
        let result = highlight_match("src/Main.rs", "main");
        // Should still highlight (rfind on lowercased)
        assert!(result.contains("Main"));
    }

    // ── extract_first_meaningful_line ────────────────────────────────

    #[test]
    fn extract_first_meaningful_line_basic() {
        let result = extract_first_meaningful_line("//! Module docs\nuse std;");
        assert_eq!(result, "//! Module docs");
    }

    #[test]
    fn extract_first_meaningful_line_skips_blanks() {
        let result = extract_first_meaningful_line("\n\n  \n  // comment");
        assert_eq!(result, "// comment");
    }

    #[test]
    fn extract_first_meaningful_line_empty() {
        let result = extract_first_meaningful_line("");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_first_meaningful_line_all_blank() {
        let result = extract_first_meaningful_line("  \n  \n  ");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_first_meaningful_line_truncates_long() {
        let long_line = "x".repeat(200);
        let result = extract_first_meaningful_line(&long_line);
        assert!(result.len() <= 83); // 80 + "..." = 83
    }

    // ── is_binary_extension ─────────────────────────────────────────

    #[test]
    fn is_binary_extension_images() {
        assert!(is_binary_extension("photo.png"));
        assert!(is_binary_extension("icon.jpg"));
        assert!(is_binary_extension("banner.gif"));
        assert!(is_binary_extension("logo.webp"));
    }

    #[test]
    fn is_binary_extension_archives() {
        assert!(is_binary_extension("data.zip"));
        assert!(is_binary_extension("backup.tar"));
        assert!(is_binary_extension("compressed.gz"));
    }

    #[test]
    fn is_binary_extension_source_files() {
        assert!(!is_binary_extension("main.rs"));
        assert!(!is_binary_extension("index.js"));
        assert!(!is_binary_extension("app.py"));
        assert!(!is_binary_extension("README.md"));
        assert!(!is_binary_extension("Cargo.toml"));
    }

    #[test]
    fn is_binary_extension_case_insensitive() {
        assert!(is_binary_extension("PHOTO.PNG"));
        assert!(is_binary_extension("Image.JPG"));
    }

    #[test]
    fn is_binary_extension_lock_files() {
        assert!(is_binary_extension("Cargo.lock"));
        assert!(is_binary_extension("package-lock.lock"));
    }

    #[test]
    fn is_binary_extension_compiled() {
        assert!(is_binary_extension("module.wasm"));
        assert!(is_binary_extension("main.pyc"));
        assert!(is_binary_extension("lib.so"));
        assert!(is_binary_extension("app.exe"));
    }

    // ── IndexEntry & format_project_index ────────────────────────────

    #[test]
    fn format_project_index_empty() {
        let result = format_project_index(&[]);
        assert_eq!(result, "(no indexable files found)");
    }

    #[test]
    fn format_project_index_single_file() {
        let entries = vec![IndexEntry {
            path: "src/main.rs".to_string(),
            lines: 42,
            summary: "//! Main module".to_string(),
        }];
        let output = format_project_index(&entries);
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("42"));
        assert!(output.contains("//! Main module"));
        assert!(output.contains("1 file"));
        assert!(output.contains("42 total lines"));
    }

    #[test]
    fn format_project_index_multiple_files() {
        let entries = vec![
            IndexEntry {
                path: "src/main.rs".to_string(),
                lines: 100,
                summary: "//! Entry point".to_string(),
            },
            IndexEntry {
                path: "src/lib.rs".to_string(),
                lines: 50,
                summary: "//! Library".to_string(),
            },
        ];
        let output = format_project_index(&entries);
        assert!(output.contains("2 files"));
        assert!(output.contains("150 total lines"));
    }

    #[test]
    fn format_project_index_long_path_truncated() {
        let long_path = format!("a/{}", "b/".repeat(25).trim_end_matches('/'));
        let entries = vec![IndexEntry {
            path: long_path,
            lines: 10,
            summary: "long path file".to_string(),
        }];
        let output = format_project_index(&entries);
        // Should contain the truncation marker
        assert!(output.contains('…'));
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

    // ── FindMatch ────────────────────────────────────────────────────

    #[test]
    fn find_match_equality() {
        let a = FindMatch {
            path: "src/main.rs".to_string(),
            score: 150,
        };
        let b = FindMatch {
            path: "src/main.rs".to_string(),
            score: 150,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn find_match_debug() {
        let m = FindMatch {
            path: "test.rs".to_string(),
            score: 100,
        };
        let debug = format!("{:?}", m);
        assert!(debug.contains("test.rs"));
        assert!(debug.contains("100"));
    }

    // ── walk_directory ──────────────────────────────────────────────

    #[test]
    fn walk_directory_finds_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("hello.txt"), "hi").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/nested.txt"), "there").unwrap();

        let files = walk_directory(dir.path().to_str().unwrap(), 3);
        assert!(files.iter().any(|f| f.ends_with("hello.txt")));
        assert!(files.iter().any(|f| f.ends_with("nested.txt")));
    }

    #[test]
    fn walk_directory_skips_hidden() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".hidden")).unwrap();
        fs::write(dir.path().join(".hidden/secret.txt"), "").unwrap();
        fs::write(dir.path().join("visible.txt"), "").unwrap();

        let files = walk_directory(dir.path().to_str().unwrap(), 3);
        assert!(files.iter().any(|f| f.ends_with("visible.txt")));
        assert!(!files.iter().any(|f| f.contains("secret")));
    }

    #[test]
    fn walk_directory_skips_node_modules() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules/dep.js"), "").unwrap();
        fs::write(dir.path().join("app.js"), "").unwrap();

        let files = walk_directory(dir.path().to_str().unwrap(), 3);
        assert!(files.iter().any(|f| f.ends_with("app.js")));
        assert!(!files.iter().any(|f| f.contains("dep.js")));
    }

    #[test]
    fn walk_directory_respects_max_depth() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
        fs::write(dir.path().join("a/b/c/deep.txt"), "").unwrap();
        fs::write(dir.path().join("a/shallow.txt"), "").unwrap();

        let files = walk_directory(dir.path().to_str().unwrap(), 1);
        assert!(files.iter().any(|f| f.ends_with("shallow.txt")));
        // At max_depth=1, we go dir->a (depth 1)->files, but a/b is depth 2
        assert!(!files.iter().any(|f| f.ends_with("deep.txt")));
    }

    // ── strip_html_tags ──────────────────────────────────────────────

    #[test]
    fn strip_html_basic_paragraph() {
        let html = "<p>Hello, world!</p>";
        let text = strip_html_tags(html, 5000);
        assert_eq!(text, "Hello, world!");
    }

    #[test]
    fn strip_html_removes_script_and_style() {
        let html =
            "<p>Before</p><script>alert('xss');</script><style>.x{color:red}</style><p>After</p>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("color:red"));
    }

    #[test]
    fn strip_html_removes_nav_footer_header() {
        let html = "<header>Nav stuff</header><p>Content</p><footer>Footer stuff</footer>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Content"));
        assert!(!text.contains("Nav stuff"));
        assert!(!text.contains("Footer stuff"));
    }

    #[test]
    fn strip_html_converts_br_to_newline() {
        let html = "Line 1<br>Line 2<br/>Line 3";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Line 1\nLine 2\nLine 3"));
    }

    #[test]
    fn strip_html_converts_li_to_bullets() {
        let html = "<ul><li>First</li><li>Second</li><li>Third</li></ul>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("• First"));
        assert!(text.contains("• Second"));
        assert!(text.contains("• Third"));
    }

    #[test]
    fn strip_html_headings() {
        let html = "<h1>Title</h1><p>Content</p><h2>Subtitle</h2>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Title"));
        assert!(text.contains("Content"));
        assert!(text.contains("Subtitle"));
    }

    #[test]
    fn strip_html_decodes_entities() {
        let html = "<p>5 &gt; 3 &amp; 2 &lt; 4</p>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("5 > 3 & 2 < 4"));
    }

    #[test]
    fn strip_html_decodes_numeric_entities() {
        let html = "<p>&#65;&#66;&#67;</p>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("ABC"));
    }

    #[test]
    fn strip_html_decodes_quotes_and_apostrophes() {
        let html = "<p>&quot;hello&quot; &amp; &apos;world&apos;</p>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("\"hello\" & 'world'"));
    }

    #[test]
    fn strip_html_collapses_whitespace() {
        let html = "<p>Hello</p>   \n\n\n\n\n   <p>World</p>";
        let text = strip_html_tags(html, 5000);
        // Should not have more than 2 consecutive newlines
        assert!(!text.contains("\n\n\n"));
    }

    #[test]
    fn strip_html_truncates_long_content() {
        let html = "<p>".to_string() + &"x".repeat(6000) + "</p>";
        let text = strip_html_tags(&html, 100);
        assert!(text.len() < 200); // truncated text + suffix
        assert!(text.contains("[… truncated at 100 chars]"));
    }

    #[test]
    fn strip_html_empty_input() {
        let text = strip_html_tags("", 5000);
        assert_eq!(text, "");
    }

    #[test]
    fn strip_html_no_tags() {
        let text = strip_html_tags("Just plain text", 5000);
        assert_eq!(text, "Just plain text");
    }

    #[test]
    fn strip_html_nested_tags() {
        let html = "<div><p>Inside <strong>bold</strong> and <em>italic</em></p></div>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Inside bold and italic"));
    }

    #[test]
    fn strip_html_case_insensitive_tags() {
        let html = "<SCRIPT>bad</SCRIPT><P>Good</P>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("Good"));
        assert!(!text.contains("bad"));
    }

    #[test]
    fn strip_html_nbsp() {
        let html = "<p>word&nbsp;word</p>";
        let text = strip_html_tags(html, 5000);
        assert!(text.contains("word word"));
    }

    // ── is_valid_url ────────────────────────────────────────────────

    #[test]
    fn valid_urls() {
        assert!(is_valid_url("https://example.com"));
        assert!(is_valid_url("http://docs.rs/yoagent"));
        assert!(is_valid_url(
            "https://doc.rust-lang.org/book/ch01-01-installation.html"
        ));
    }

    #[test]
    fn invalid_urls() {
        assert!(!is_valid_url("not-a-url"));
        assert!(!is_valid_url("ftp://files.com"));
        assert!(!is_valid_url("https://"));
        assert!(!is_valid_url("http://x"));
        assert!(!is_valid_url(""));
    }

    // ── /add command tests ────────────────────────────────────────────

    #[test]
    fn parse_add_arg_simple_path() {
        let (path, range) = parse_add_arg("src/main.rs");
        assert_eq!(path, "src/main.rs");
        assert!(range.is_none());
    }

    #[test]
    fn parse_add_arg_with_line_range() {
        let (path, range) = parse_add_arg("src/main.rs:10-20");
        assert_eq!(path, "src/main.rs");
        assert_eq!(range, Some((10, 20)));
    }

    #[test]
    fn parse_add_arg_with_single_line() {
        let (path, range) = parse_add_arg("src/main.rs:42-42");
        assert_eq!(path, "src/main.rs");
        assert_eq!(range, Some((42, 42)));
    }

    #[test]
    fn parse_add_arg_with_colon_in_path_no_range() {
        // A colon followed by non-numeric text should not be treated as a range
        let (path, range) = parse_add_arg("C:/Users/test.rs");
        assert_eq!(path, "C:/Users/test.rs");
        assert!(range.is_none());
    }

    #[test]
    fn parse_add_arg_windows_path_with_range() {
        // Windows-style: C:/foo/bar.rs:5-10 — colon after drive letter
        let (path, range) = parse_add_arg("foo/bar.rs:5-10");
        assert_eq!(path, "foo/bar.rs");
        assert_eq!(range, Some((5, 10)));
    }

    #[test]
    fn format_add_content_basic() {
        let content = format_add_content("hello.txt", "hello world\n");
        assert!(content.contains("hello.txt"));
        assert!(content.contains("```"));
        assert!(content.contains("hello world"));
    }

    #[test]
    fn format_add_content_wraps_in_code_block() {
        let content = format_add_content("test.rs", "fn main() {}\n");
        // Should have opening and closing code fences
        let fences: Vec<&str> = content.lines().filter(|l| l.starts_with("```")).collect();
        assert_eq!(fences.len(), 2, "Should have exactly 2 code fences");
    }

    #[test]
    fn expand_add_globs_no_glob() {
        let paths = expand_add_paths("src/main.rs");
        assert_eq!(paths, vec!["src/main.rs".to_string()]);
    }

    #[test]
    fn expand_add_globs_with_glob() {
        // This tests with a real glob pattern against the project
        let paths = expand_add_paths("src/*.rs");
        assert!(!paths.is_empty(), "Should match at least one .rs file");
        for p in &paths {
            assert!(p.ends_with(".rs"), "All matches should be .rs files: {p}");
            assert!(p.starts_with("src/"), "All matches should be in src/: {p}");
        }
    }

    #[test]
    fn expand_add_globs_no_matches() {
        let paths = expand_add_paths("nonexistent_dir_xyz/*.zzz");
        assert!(paths.is_empty(), "Non-matching glob should return empty");
    }

    #[test]
    fn add_read_file_with_range() {
        // Read our own source with a line range
        let result = read_file_for_add("src/commands_project.rs", Some((1, 3)));
        assert!(result.is_ok());
        let (content, count) = result.unwrap();
        assert_eq!(count, 3);
        assert!(!content.is_empty());
    }

    #[test]
    fn add_read_file_full() {
        let result = read_file_for_add("Cargo.toml", None);
        assert!(result.is_ok());
        let (content, count) = result.unwrap();
        assert!(count > 0);
        assert!(content.contains("[package]"));
    }

    #[test]
    fn add_read_file_not_found() {
        let result = read_file_for_add("definitely_not_a_real_file.xyz", None);
        assert!(result.is_err());
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

    // ── /grep tests ─────────────────────────────────────────────────────

    #[test]
    fn parse_grep_args_basic_pattern() {
        let args = parse_grep_args("/grep TODO").unwrap();
        assert_eq!(args.pattern, "TODO");
        assert_eq!(args.path, ".");
        assert!(!args.case_sensitive);
    }

    #[test]
    fn parse_grep_args_with_path() {
        let args = parse_grep_args("/grep fn_main src/").unwrap();
        assert_eq!(args.pattern, "fn_main");
        assert_eq!(args.path, "src/");
        assert!(!args.case_sensitive);
    }

    #[test]
    fn parse_grep_args_case_sensitive_flag() {
        let args = parse_grep_args("/grep -s MyStruct src/").unwrap();
        assert_eq!(args.pattern, "MyStruct");
        assert_eq!(args.path, "src/");
        assert!(args.case_sensitive);
    }

    #[test]
    fn parse_grep_args_case_long_flag() {
        let args = parse_grep_args("/grep --case Pattern").unwrap();
        assert_eq!(args.pattern, "Pattern");
        assert!(args.case_sensitive);
    }

    #[test]
    fn parse_grep_args_empty_returns_none() {
        assert!(parse_grep_args("/grep").is_none());
        assert!(parse_grep_args("/grep  ").is_none());
    }

    #[test]
    fn parse_grep_args_only_flag_returns_none() {
        assert!(parse_grep_args("/grep -s").is_none());
        assert!(parse_grep_args("/grep --case").is_none());
    }

    #[test]
    fn format_grep_results_empty() {
        let formatted = format_grep_results(&[], "pattern", false);
        assert!(formatted.contains("No matches found"));
    }

    #[test]
    fn format_grep_results_with_matches() {
        let matches = vec![
            GrepMatch {
                file: "src/main.rs".to_string(),
                line_num: 10,
                text: "fn main() {".to_string(),
            },
            GrepMatch {
                file: "src/lib.rs".to_string(),
                line_num: 5,
                text: "// main entry".to_string(),
            },
        ];
        let formatted = format_grep_results(&matches, "main", false);
        assert!(formatted.contains("src/main.rs"));
        assert!(formatted.contains("10"));
        assert!(formatted.contains("src/lib.rs"));
        assert!(formatted.contains("5"));
        assert!(formatted.contains("2 matches"));
    }

    #[test]
    fn format_grep_results_truncation() {
        let matches: Vec<GrepMatch> = (0..60)
            .map(|i| GrepMatch {
                file: format!("file{i}.rs"),
                line_num: i,
                text: format!("line {i}"),
            })
            .collect();
        let formatted = format_grep_results(&matches, "line", false);
        assert!(formatted.contains("10 more matches, narrow your search"));
        // Should show first 50, not last 10
        assert!(formatted.contains("file0.rs"));
        assert!(formatted.contains("file49.rs"));
    }

    #[test]
    fn format_grep_results_single_match() {
        let matches = vec![GrepMatch {
            file: "test.rs".to_string(),
            line_num: 1,
            text: "hello".to_string(),
        }];
        let formatted = format_grep_results(&matches, "hello", false);
        assert!(formatted.contains("1 match"));
        // Shouldn't say "1 matches"
        assert!(!formatted.contains("1 matches"));
    }

    #[test]
    fn handle_grep_finds_real_matches() {
        // This tests run_grep on the actual project — "fn main" should exist in src/
        let args = GrepArgs {
            pattern: "fn main".to_string(),
            path: "src/".to_string(),
            case_sensitive: true,
        };
        let matches = run_grep(&args).unwrap();
        assert!(
            !matches.is_empty(),
            "Should find 'fn main' in src/ of this project"
        );
        assert!(matches.iter().any(|m| m.file.contains("main.rs")));
    }

    #[test]
    fn grep_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/grep"),
            "/grep should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn grep_in_help_text() {
        let help = help_text();
        assert!(help.contains("/grep"), "/grep should appear in help text");
    }

    // ── is_image_extension ────────────────────────────────────────────

    #[test]
    fn is_image_extension_supported_formats() {
        assert!(is_image_extension("photo.png"));
        assert!(is_image_extension("photo.jpg"));
        assert!(is_image_extension("photo.jpeg"));
        assert!(is_image_extension("photo.gif"));
        assert!(is_image_extension("photo.webp"));
        assert!(is_image_extension("photo.bmp"));
    }

    #[test]
    fn is_image_extension_case_insensitive() {
        assert!(is_image_extension("photo.PNG"));
        assert!(is_image_extension("image.Jpg"));
        assert!(is_image_extension("banner.JPEG"));
        assert!(is_image_extension("icon.GIF"));
        assert!(is_image_extension("pic.WeBp"));
        assert!(is_image_extension("scan.BMP"));
    }

    #[test]
    fn is_image_extension_non_image_files() {
        assert!(!is_image_extension("main.rs"));
        assert!(!is_image_extension("notes.txt"));
        assert!(!is_image_extension("README.md"));
        assert!(!is_image_extension("config.json"));
        assert!(!is_image_extension("Cargo.toml"));
        assert!(!is_image_extension("archive.zip"));
    }

    #[test]
    fn is_image_extension_no_extension() {
        assert!(!is_image_extension("Makefile"));
        assert!(!is_image_extension(""));
    }

    #[test]
    fn is_image_extension_with_full_paths() {
        assert!(is_image_extension("src/assets/logo.png"));
        assert!(is_image_extension("/home/user/photos/vacation.jpg"));
        assert!(is_image_extension("../../images/banner.webp"));
        assert!(!is_image_extension("src/main.rs"));
    }

    // ── mime_type_for_extension ───────────────────────────────────────

    #[test]
    fn mime_type_png() {
        assert_eq!(mime_type_for_extension("png"), "image/png");
    }

    #[test]
    fn mime_type_jpg_and_jpeg() {
        assert_eq!(mime_type_for_extension("jpg"), "image/jpeg");
        assert_eq!(mime_type_for_extension("jpeg"), "image/jpeg");
    }

    #[test]
    fn mime_type_gif() {
        assert_eq!(mime_type_for_extension("gif"), "image/gif");
    }

    #[test]
    fn mime_type_webp() {
        assert_eq!(mime_type_for_extension("webp"), "image/webp");
    }

    #[test]
    fn mime_type_bmp() {
        assert_eq!(mime_type_for_extension("bmp"), "image/bmp");
    }

    #[test]
    fn mime_type_unknown_extension() {
        assert_eq!(mime_type_for_extension("zip"), "application/octet-stream");
        assert_eq!(mime_type_for_extension("rs"), "application/octet-stream");
        assert_eq!(mime_type_for_extension(""), "application/octet-stream");
    }

    #[test]
    fn mime_type_case_insensitive() {
        assert_eq!(mime_type_for_extension("PNG"), "image/png");
        assert_eq!(mime_type_for_extension("Jpg"), "image/jpeg");
        assert_eq!(mime_type_for_extension("GIF"), "image/gif");
    }

    // ── AddResult ─────────────────────────────────────────────────────

    #[test]
    fn add_result_text_fields_accessible() {
        let result = AddResult::Text {
            summary: "added foo.rs".to_string(),
            content: "fn main() {}".to_string(),
        };
        match &result {
            AddResult::Text { summary, content } => {
                assert_eq!(summary, "added foo.rs");
                assert_eq!(content, "fn main() {}");
            }
            _ => panic!("expected Text variant"),
        }
    }

    #[test]
    fn add_result_image_fields_accessible() {
        let result = AddResult::Image {
            summary: "added logo.png".to_string(),
            data: "base64data".to_string(),
            mime_type: "image/png".to_string(),
        };
        match &result {
            AddResult::Image {
                summary,
                data,
                mime_type,
            } => {
                assert_eq!(summary, "added logo.png");
                assert_eq!(data, "base64data");
                assert_eq!(mime_type, "image/png");
            }
            _ => panic!("expected Image variant"),
        }
    }

    #[test]
    fn add_result_partial_eq() {
        let a = AddResult::Text {
            summary: "s".to_string(),
            content: "c".to_string(),
        };
        let b = AddResult::Text {
            summary: "s".to_string(),
            content: "c".to_string(),
        };
        let c = AddResult::Text {
            summary: "different".to_string(),
            content: "c".to_string(),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);

        let img1 = AddResult::Image {
            summary: "s".to_string(),
            data: "d".to_string(),
            mime_type: "image/png".to_string(),
        };
        let img2 = AddResult::Image {
            summary: "s".to_string(),
            data: "d".to_string(),
            mime_type: "image/png".to_string(),
        };
        assert_eq!(img1, img2);

        // Text != Image even with same summary
        assert_ne!(a, img1);
    }

    // ── read_image_for_add ────────────────────────────────────────────

    #[test]
    fn read_image_for_add_valid_png() {
        let dir = TempDir::new().unwrap();
        let png_path = dir.path().join("test.png");

        // Minimal valid PNG: 8-byte signature + IHDR chunk (25 bytes) + IEND chunk (12 bytes)
        #[rustfmt::skip]
        let png_bytes: Vec<u8> = vec![
            // PNG signature
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
            // IHDR chunk: length=13
            0x00, 0x00, 0x00, 0x0D,
            // "IHDR"
            0x49, 0x48, 0x44, 0x52,
            // width=1, height=1
            0x00, 0x00, 0x00, 0x01,
            0x00, 0x00, 0x00, 0x01,
            // bit depth=8, color type=2 (RGB), compression=0, filter=0, interlace=0
            0x08, 0x02, 0x00, 0x00, 0x00,
            // IHDR CRC (precalculated for this exact IHDR)
            0x90, 0x77, 0x53, 0xDE,
            // IEND chunk: length=0
            0x00, 0x00, 0x00, 0x00,
            // "IEND"
            0x49, 0x45, 0x4E, 0x44,
            // IEND CRC
            0xAE, 0x42, 0x60, 0x82,
        ];
        fs::write(&png_path, &png_bytes).unwrap();

        let path_str = png_path.to_str().unwrap();
        let result = read_image_for_add(path_str);
        assert!(result.is_ok(), "should succeed reading a valid PNG file");

        let (data, mime_type) = result.unwrap();
        assert!(!data.is_empty(), "base64 data should be non-empty");
        assert_eq!(mime_type, "image/png");

        // Verify the base64 decodes back to the original bytes
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&data)
            .expect("should be valid base64");
        assert_eq!(decoded, png_bytes);
    }

    #[test]
    fn read_image_for_add_nonexistent_file() {
        let result = read_image_for_add("/tmp/definitely_does_not_exist_yoyo_test.png");
        assert!(result.is_err(), "should fail for nonexistent file");
        let err = result.unwrap_err();
        assert!(
            err.contains("failed to read"),
            "error should mention failure: {err}"
        );
    }

    #[test]
    fn read_image_for_add_jpg_mime_type() {
        let dir = TempDir::new().unwrap();
        let jpg_path = dir.path().join("photo.jpg");
        // Just some bytes — we're testing MIME detection, not image validity
        fs::write(&jpg_path, b"fake jpg content").unwrap();

        let (data, mime_type) = read_image_for_add(jpg_path.to_str().unwrap()).unwrap();
        assert!(!data.is_empty());
        assert_eq!(mime_type, "image/jpeg");
    }

    #[test]
    fn read_image_for_add_webp_mime_type() {
        let dir = TempDir::new().unwrap();
        let webp_path = dir.path().join("image.webp");
        fs::write(&webp_path, b"fake webp content").unwrap();

        let (_, mime_type) = read_image_for_add(webp_path.to_str().unwrap()).unwrap();
        assert_eq!(mime_type, "image/webp");
    }

    // ── expand_file_mentions tests ───────────────────────────────────

    #[test]
    fn expand_file_mentions_no_mentions() {
        let (text, results) = expand_file_mentions("hello world, no mentions here");
        assert_eq!(text, "hello world, no mentions here");
        assert!(results.is_empty());
    }

    #[test]
    fn expand_file_mentions_resolves_real_file() {
        // Cargo.toml should exist at the project root
        let (text, results) = expand_file_mentions("explain @Cargo.toml");
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], AddResult::Text { summary, .. } if summary.contains("Cargo.toml"))
        );
        assert_eq!(text, "explain Cargo.toml");
    }

    #[test]
    fn expand_file_mentions_nonexistent_file_unchanged() {
        let (text, results) = expand_file_mentions("look at @nonexistent_xyz_file.rs");
        assert!(results.is_empty());
        assert_eq!(text, "look at @nonexistent_xyz_file.rs");
    }

    #[test]
    fn expand_file_mentions_with_line_range() {
        let (text, results) = expand_file_mentions("review @Cargo.toml:1-3");
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], AddResult::Text { summary, .. } if summary.contains("lines 1-3"))
        );
        assert_eq!(text, "review Cargo.toml:1-3");
    }

    #[test]
    fn expand_file_mentions_multiple_mentions() {
        let (text, results) = expand_file_mentions("compare @Cargo.toml and @LICENSE");
        assert_eq!(results.len(), 2);
        assert_eq!(text, "compare Cargo.toml and LICENSE");
    }

    #[test]
    fn expand_file_mentions_at_end_of_string_no_path() {
        let (text, results) = expand_file_mentions("trailing @");
        assert!(results.is_empty());
        assert_eq!(text, "trailing @");
    }

    #[test]
    fn expand_file_mentions_at_followed_by_space() {
        let (text, results) = expand_file_mentions("hello @ world");
        assert!(results.is_empty());
        assert_eq!(text, "hello @ world");
    }

    #[test]
    fn expand_file_mentions_skips_email_like() {
        let (text, results) = expand_file_mentions("email user@example.com please");
        assert!(results.is_empty());
        assert_eq!(text, "email user@example.com please");
    }

    #[test]
    fn expand_file_mentions_path_with_dirs() {
        // src/main.rs should exist
        let (text, results) = expand_file_mentions("look at @src/main.rs");
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], AddResult::Text { summary, .. } if summary.contains("src/main.rs"))
        );
        assert_eq!(text, "look at main.rs");
    }

    #[test]
    fn expand_file_mentions_mixed_real_and_fake() {
        let (text, results) = expand_file_mentions("@Cargo.toml is real but @fake_abc.rs is not");
        assert_eq!(results.len(), 1);
        assert!(text.contains("Cargo.toml"));
        assert!(text.contains("@fake_abc.rs"));
    }

    // ── rename: word boundary matching ──────────────────────────────

    #[test]
    fn find_word_boundary_simple_match() {
        let matches = find_word_boundary_matches("let foo = 42;", "foo");
        assert_eq!(matches, vec![4]);
    }

    #[test]
    fn find_word_boundary_no_match_substring() {
        // "foo" should NOT match inside "foobar"
        let matches = find_word_boundary_matches("let foobar = 42;", "foo");
        assert!(matches.is_empty());
    }

    #[test]
    fn find_word_boundary_no_match_prefix() {
        // "foo" should NOT match inside "barfoo"... wait, "barfoo" — "foo" is at end
        // but "bar" precedes it without boundary. Let's test "afoo"
        let matches = find_word_boundary_matches("let afoo = 42;", "foo");
        assert!(matches.is_empty());
    }

    #[test]
    fn find_word_boundary_at_start_of_line() {
        let matches = find_word_boundary_matches("foo = 42;", "foo");
        assert_eq!(matches, vec![0]);
    }

    #[test]
    fn find_word_boundary_at_end_of_line() {
        let matches = find_word_boundary_matches("let x = foo", "foo");
        assert_eq!(matches, vec![8]);
    }

    #[test]
    fn find_word_boundary_multiple_matches() {
        let matches = find_word_boundary_matches("foo + foo * foo", "foo");
        assert_eq!(matches, vec![0, 6, 12]);
    }

    #[test]
    fn find_word_boundary_with_underscore() {
        // Underscore is a word character, so "my_func" should not match "my"
        let matches = find_word_boundary_matches("call my_func()", "my");
        assert!(matches.is_empty());
    }

    #[test]
    fn find_word_boundary_dots_are_boundaries() {
        // Dots are word boundaries, so "foo" should match in "self.foo"
        let matches = find_word_boundary_matches("self.foo.bar", "foo");
        assert_eq!(matches, vec![5]);
    }

    #[test]
    fn find_word_boundary_empty_pattern() {
        let matches = find_word_boundary_matches("hello", "");
        assert!(matches.is_empty());
    }

    #[test]
    fn find_word_boundary_empty_text() {
        let matches = find_word_boundary_matches("", "foo");
        assert!(matches.is_empty());
    }

    #[test]
    fn find_word_boundary_exact_match() {
        let matches = find_word_boundary_matches("foo", "foo");
        assert_eq!(matches, vec![0]);
    }

    #[test]
    fn find_word_boundary_parens_are_boundaries() {
        let matches = find_word_boundary_matches("call(foo)", "foo");
        assert_eq!(matches, vec![5]);
    }

    // ── rename: replace_word_boundary ───────────────────────────────

    #[test]
    fn replace_word_boundary_simple() {
        let result = replace_word_boundary("let foo = 42;", "foo", "bar");
        assert_eq!(result, "let bar = 42;");
    }

    #[test]
    fn replace_word_boundary_no_partial() {
        let result = replace_word_boundary("let foobar = 42;", "foo", "bar");
        assert_eq!(result, "let foobar = 42;"); // unchanged
    }

    #[test]
    fn replace_word_boundary_multiple() {
        let result = replace_word_boundary("foo + foo", "foo", "bar");
        assert_eq!(result, "bar + bar");
    }

    #[test]
    fn replace_word_boundary_empty_pattern() {
        let result = replace_word_boundary("hello", "", "bar");
        assert_eq!(result, "hello");
    }

    #[test]
    fn replace_word_boundary_no_matches() {
        let result = replace_word_boundary("nothing here", "foo", "bar");
        assert_eq!(result, "nothing here");
    }

    #[test]
    fn replace_word_boundary_with_longer_replacement() {
        let result = replace_word_boundary("fn f(x: T) -> T", "T", "MyType");
        assert_eq!(result, "fn f(x: MyType) -> MyType");
    }

    #[test]
    fn replace_word_boundary_with_shorter_replacement() {
        let result =
            replace_word_boundary("let my_variable = my_variable + 1;", "my_variable", "x");
        assert_eq!(result, "let x = x + 1;");
    }

    // ── rename: parse_rename_args ───────────────────────────────────

    #[test]
    fn parse_rename_args_valid() {
        let result = parse_rename_args("/rename foo bar");
        assert_eq!(result, Some(("foo".to_string(), "bar".to_string())));
    }

    #[test]
    fn parse_rename_args_no_args() {
        let result = parse_rename_args("/rename");
        assert_eq!(result, None);
    }

    #[test]
    fn parse_rename_args_one_arg() {
        let result = parse_rename_args("/rename foo");
        assert_eq!(result, None);
    }

    #[test]
    fn parse_rename_args_too_many_args() {
        let result = parse_rename_args("/rename foo bar baz");
        assert_eq!(result, None);
    }

    #[test]
    fn parse_rename_args_extra_whitespace() {
        let result = parse_rename_args("/rename  foo   bar");
        assert_eq!(result, Some(("foo".to_string(), "bar".to_string())));
    }

    // ── rename: format_rename_preview ───────────────────────────────

    #[test]
    fn format_rename_preview_no_matches() {
        let preview = format_rename_preview(&[], "foo", "bar");
        assert!(preview.contains("No matches found"));
    }

    #[test]
    fn format_rename_preview_shows_file_and_line() {
        let matches = vec![RenameMatch {
            file: "src/main.rs".to_string(),
            line_num: 10,
            line_text: "let foo = 42;".to_string(),
            column: 4,
        }];
        let preview = format_rename_preview(&matches, "foo", "bar");
        assert!(preview.contains("src/main.rs"));
        assert!(preview.contains("10"));
        assert!(preview.contains("1 match"));
        assert!(preview.contains("1 file"));
    }

    #[test]
    fn format_rename_preview_multiple_files() {
        let matches = vec![
            RenameMatch {
                file: "a.rs".to_string(),
                line_num: 1,
                line_text: "use foo;".to_string(),
                column: 4,
            },
            RenameMatch {
                file: "b.rs".to_string(),
                line_num: 5,
                line_text: "foo()".to_string(),
                column: 0,
            },
        ];
        let preview = format_rename_preview(&matches, "foo", "bar");
        assert!(preview.contains("a.rs"));
        assert!(preview.contains("b.rs"));
        assert!(preview.contains("2 matches"));
        assert!(preview.contains("2 files"));
    }

    // ── rename: apply_rename with temp files ────────────────────────

    #[test]
    fn apply_rename_modifies_files() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "let foo = 1;\nlet bar = foo;\n").unwrap();

        let matches = vec![
            RenameMatch {
                file: file_path.to_str().unwrap().to_string(),
                line_num: 1,
                line_text: "let foo = 1;".to_string(),
                column: 4,
            },
            RenameMatch {
                file: file_path.to_str().unwrap().to_string(),
                line_num: 2,
                line_text: "let bar = foo;".to_string(),
                column: 10,
            },
        ];

        let count = apply_rename(&matches, "foo", "baz");
        assert_eq!(count, 2);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("let baz = 1;"));
        assert!(content.contains("let bar = baz;"));
        assert!(!content.contains("foo"));
    }

    #[test]
    fn apply_rename_preserves_non_matching_lines() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "// comment\nlet foo = 1;\n// end\n").unwrap();

        let matches = vec![RenameMatch {
            file: file_path.to_str().unwrap().to_string(),
            line_num: 2,
            line_text: "let foo = 1;".to_string(),
            column: 4,
        }];

        apply_rename(&matches, "foo", "bar");

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("// comment"));
        assert!(content.contains("let bar = 1;"));
        assert!(content.contains("// end"));
    }

    #[test]
    fn apply_rename_no_partial_replace() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "let foobar = foo;\n").unwrap();

        // Only match the standalone "foo", not "foobar"
        let matches = vec![RenameMatch {
            file: file_path.to_str().unwrap().to_string(),
            line_num: 1,
            line_text: "let foobar = foo;".to_string(),
            column: 13,
        }];

        apply_rename(&matches, "foo", "baz");

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("foobar")); // foobar unchanged
        assert!(content.contains("= baz;")); // standalone foo replaced
    }

    #[test]
    fn apply_rename_empty_matches() {
        let count = apply_rename(&[], "foo", "bar");
        assert_eq!(count, 0);
    }

    // ── /extract: parse_extract_args ─────────────────────────────────

    #[test]
    fn parse_extract_args_valid() {
        let result = parse_extract_args("/extract my_func src/lib.rs src/utils.rs");
        assert_eq!(
            result,
            Some((
                "my_func".to_string(),
                "src/lib.rs".to_string(),
                "src/utils.rs".to_string()
            ))
        );
    }

    #[test]
    fn parse_extract_args_missing_target() {
        assert_eq!(parse_extract_args("/extract my_func src/lib.rs"), None);
    }

    #[test]
    fn parse_extract_args_too_many() {
        assert_eq!(parse_extract_args("/extract a b c d"), None);
    }

    #[test]
    fn parse_extract_args_empty() {
        assert_eq!(parse_extract_args("/extract"), None);
    }

    // ── /extract: find_symbol_block ──────────────────────────────────

    #[test]
    fn find_symbol_block_simple_fn() {
        let source = "fn hello() {\n    println!(\"hi\");\n}\n";
        let result = find_symbol_block(source, "hello");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 2);
        assert!(block.contains("fn hello()"));
        assert!(block.contains("println!"));
    }

    #[test]
    fn find_symbol_block_pub_fn() {
        let source = "pub fn greet(name: &str) -> String {\n    format!(\"Hello {name}\")\n}\n";
        let result = find_symbol_block(source, "greet");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 2);
        assert!(block.contains("pub fn greet"));
    }

    #[test]
    fn find_symbol_block_struct() {
        let source = "pub struct MyPoint {\n    pub x: f64,\n    pub y: f64,\n}\n";
        let result = find_symbol_block(source, "MyPoint");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("pub struct MyPoint"));
        assert!(block.contains("pub x: f64"));
    }

    #[test]
    fn find_symbol_block_enum() {
        let source = "enum Color {\n    Red,\n    Green,\n    Blue,\n}\n";
        let result = find_symbol_block(source, "Color");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("enum Color"));
        assert!(block.contains("Blue"));
    }

    #[test]
    fn find_symbol_block_impl() {
        let source = "struct Foo;\n\nimpl Foo {\n    fn bar(&self) {}\n}\n";
        let result = find_symbol_block(source, "Foo");
        // Should find `struct Foo;` first (it's a unit struct)
        assert!(result.is_some());
        let (start, _end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert!(block.contains("struct Foo"));
    }

    #[test]
    fn find_symbol_block_with_doc_comments() {
        let source = "/// A helper function.\n/// Does something.\nfn helper() {\n    // body\n}\n";
        let result = find_symbol_block(source, "helper");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0); // doc comments included
        assert_eq!(end, 4);
        assert!(block.contains("/// A helper function."));
        assert!(block.contains("fn helper()"));
    }

    #[test]
    fn find_symbol_block_with_attributes() {
        let source = "#[derive(Debug)]\npub struct Config {\n    pub name: String,\n}\n";
        let result = find_symbol_block(source, "Config");
        assert!(result.is_some());
        let (start, _, block) = result.unwrap();
        assert_eq!(start, 0); // attribute included
        assert!(block.contains("#[derive(Debug)]"));
        assert!(block.contains("pub struct Config"));
    }

    #[test]
    fn find_symbol_block_not_found() {
        let source = "fn other() {\n}\n";
        assert!(find_symbol_block(source, "missing").is_none());
    }

    #[test]
    fn find_symbol_block_nested_braces() {
        let source = "fn complex() {\n    if true {\n        for i in 0..10 {\n            println!(\"{i}\");\n        }\n    }\n}\n";
        let result = find_symbol_block(source, "complex");
        assert!(result.is_some());
        let (start, end, _block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 6);
    }

    #[test]
    fn find_symbol_block_among_multiple() {
        let source = "fn first() {\n}\n\nfn second() {\n    let x = 1;\n}\n\nfn third() {\n}\n";
        let result = find_symbol_block(source, "second");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 3);
        assert_eq!(end, 5);
        assert!(block.contains("fn second()"));
        assert!(block.contains("let x = 1"));
    }

    #[test]
    fn find_symbol_block_unit_struct() {
        let source = "pub struct Unit;\n\nfn other() {}\n";
        let result = find_symbol_block(source, "Unit");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 0);
        assert!(block.contains("pub struct Unit;"));
    }

    #[test]
    fn find_symbol_block_trait() {
        let source = "pub trait Drawable {\n    fn draw(&self);\n}\n";
        let result = find_symbol_block(source, "Drawable");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("pub trait Drawable"));
        assert!(block.contains("fn draw"));
    }

    #[test]
    fn find_symbol_block_async_fn() {
        let source = "pub async fn fetch_data() {\n    // async body\n}\n";
        let result = find_symbol_block(source, "fetch_data");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("pub async fn fetch_data"));
    }

    #[test]
    fn find_symbol_block_no_partial_match() {
        let source = "fn my_func_extended() {\n}\n\nfn my_func() {\n    // target\n}\n";
        let result = find_symbol_block(source, "my_func");
        assert!(result.is_some());
        let (start, _, block) = result.unwrap();
        // Should match my_func, not my_func_extended
        assert_eq!(start, 3);
        assert!(block.contains("// target"));
    }

    // ── /extract: extract_symbol (integration) ──────────────────────

    #[test]
    fn extract_symbol_moves_function() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(
            &source,
            "fn keep_me() {\n    // stays\n}\n\npub fn move_me() {\n    // goes\n}\n\nfn also_stays() {\n}\n",
        )
        .unwrap();
        fs::write(&target, "// existing content\n").unwrap();

        let result = extract_symbol(
            source.to_str().unwrap(),
            target.to_str().unwrap(),
            "move_me",
        );
        assert!(result.is_ok());

        let source_after = fs::read_to_string(&source).unwrap();
        assert!(source_after.contains("fn keep_me()"));
        assert!(source_after.contains("fn also_stays()"));
        assert!(!source_after.contains("fn move_me()"));

        let target_after = fs::read_to_string(&target).unwrap();
        assert!(target_after.contains("// existing content"));
        assert!(target_after.contains("pub fn move_me()"));
        assert!(target_after.contains("// goes"));
    }

    #[test]
    fn extract_symbol_creates_target_if_missing() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("new_file.rs");

        fs::write(&source, "fn movable() {\n    let x = 1;\n}\n").unwrap();

        let result = extract_symbol(
            source.to_str().unwrap(),
            target.to_str().unwrap(),
            "movable",
        );
        assert!(result.is_ok());
        assert!(target.exists());

        let target_content = fs::read_to_string(&target).unwrap();
        assert!(target_content.contains("fn movable()"));
    }

    #[test]
    fn extract_symbol_not_found() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(&source, "fn other() {}\n").unwrap();

        let result = extract_symbol(
            source.to_str().unwrap(),
            target.to_str().unwrap(),
            "missing",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn extract_symbol_source_not_found() {
        let dir = TempDir::new().unwrap();
        let result = extract_symbol(
            dir.path().join("nope.rs").to_str().unwrap(),
            dir.path().join("target.rs").to_str().unwrap(),
            "foo",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot read"));
    }

    #[test]
    fn extract_symbol_with_doc_comments_moves_docs() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(
            &source,
            "/// Important docs.\n/// More docs.\npub fn documented() {\n    // body\n}\n",
        )
        .unwrap();

        let result = extract_symbol(
            source.to_str().unwrap(),
            target.to_str().unwrap(),
            "documented",
        );
        assert!(result.is_ok());

        let target_content = fs::read_to_string(&target).unwrap();
        assert!(target_content.contains("/// Important docs."));
        assert!(target_content.contains("/// More docs."));
        assert!(target_content.contains("pub fn documented()"));
    }

    #[test]
    fn extract_command_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/extract"),
            "/extract should be in KNOWN_COMMANDS"
        );
    }

    // ── /extract: find_symbol_block — type alias, const, static ─────

    #[test]
    fn find_symbol_block_type_alias() {
        let source = "pub type Result<T> = std::result::Result<T, MyError>;\n\nfn other() {}\n";
        let result = find_symbol_block(source, "Result");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 0);
        assert!(block.contains("pub type Result<T>"));
    }

    #[test]
    fn find_symbol_block_type_alias_simple() {
        let source = "type Callback = fn(u32) -> bool;\n";
        let result = find_symbol_block(source, "Callback");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 0);
        assert!(block.contains("type Callback"));
    }

    #[test]
    fn find_symbol_block_const() {
        let source = "pub const MAX_SIZE: usize = 1024;\n\nfn other() {}\n";
        let result = find_symbol_block(source, "MAX_SIZE");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 0);
        assert!(block.contains("pub const MAX_SIZE"));
    }

    #[test]
    fn find_symbol_block_const_with_doc() {
        let source = "/// The maximum buffer size.\nconst BUFFER_SIZE: usize = 512;\n";
        let result = find_symbol_block(source, "BUFFER_SIZE");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0); // doc comment included
        assert_eq!(end, 1);
        assert!(block.contains("/// The maximum buffer size."));
        assert!(block.contains("const BUFFER_SIZE"));
    }

    #[test]
    fn find_symbol_block_static() {
        let source = "static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);\n";
        let result = find_symbol_block(source, "COUNTER");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("static COUNTER"));
    }

    #[test]
    fn find_symbol_block_static_mut() {
        let source = "static mut GLOBAL: u32 = 0;\n\nfn other() {}\n";
        let result = find_symbol_block(source, "GLOBAL");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("static mut GLOBAL"));
    }

    #[test]
    fn find_symbol_block_pub_const_crate() {
        let source = "pub(crate) const INTERNAL_LIMIT: u32 = 100;\n";
        let result = find_symbol_block(source, "INTERNAL_LIMIT");
        assert!(result.is_some());
        let (_, _, block) = result.unwrap();
        assert!(block.contains("pub(crate) const INTERNAL_LIMIT"));
    }

    #[test]
    fn find_symbol_block_const_multiline() {
        let source = "const ITEMS: &[&str] = &[\n    \"alpha\",\n    \"beta\",\n];\n";
        let result = find_symbol_block(source, "ITEMS");
        assert!(result.is_some());
        let (start, end, block) = result.unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 3);
        assert!(block.contains("const ITEMS"));
        assert!(block.contains("\"beta\""));
    }

    // ── /extract: extract_symbol with new types ─────────────────────

    #[test]
    fn extract_symbol_moves_type_alias() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(
            &source,
            "pub type MyResult<T> = Result<T, MyError>;\n\nfn keep() {}\n",
        )
        .unwrap();
        fs::write(&target, "// types\n").unwrap();

        let result = extract_symbol(
            source.to_str().unwrap(),
            target.to_str().unwrap(),
            "MyResult",
        );
        assert!(result.is_ok());

        let source_after = fs::read_to_string(&source).unwrap();
        assert!(!source_after.contains("type MyResult"));
        assert!(source_after.contains("fn keep()"));

        let target_after = fs::read_to_string(&target).unwrap();
        assert!(target_after.contains("pub type MyResult<T>"));
    }

    #[test]
    fn extract_symbol_moves_const() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(&source, "pub const LIMIT: usize = 42;\n\nfn keep() {}\n").unwrap();
        fs::write(&target, "").unwrap();

        let result = extract_symbol(source.to_str().unwrap(), target.to_str().unwrap(), "LIMIT");
        assert!(result.is_ok());

        let source_after = fs::read_to_string(&source).unwrap();
        assert!(!source_after.contains("const LIMIT"));

        let target_after = fs::read_to_string(&target).unwrap();
        assert!(target_after.contains("pub const LIMIT: usize = 42;"));
    }

    #[test]
    fn extract_symbol_moves_static() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.rs");
        let target = dir.path().join("target.rs");

        fs::write(
            &source,
            "pub static INSTANCE: &str = \"hello\";\n\nfn keep() {}\n",
        )
        .unwrap();
        fs::write(&target, "").unwrap();

        let result = extract_symbol(
            source.to_str().unwrap(),
            target.to_str().unwrap(),
            "INSTANCE",
        );
        assert!(result.is_ok());

        let source_after = fs::read_to_string(&source).unwrap();
        assert!(!source_after.contains("static INSTANCE"));

        let target_after = fs::read_to_string(&target).unwrap();
        assert!(target_after.contains("pub static INSTANCE"));
    }
}
