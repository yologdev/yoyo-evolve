//! Project-related command handlers: /context, /init, /health, /fix, /test, /lint,
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
    let files = match std::process::Command::new("git")
        .args(["ls-files"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut files: Vec<String> = text
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
            files.sort();
            files
        }
        _ => return "(not a git repository — /tree requires git)".to_string(),
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
    if let Ok(output) = std::process::Command::new("git")
        .args(["ls-files"])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            return text
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
        }
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

    // Decode common HTML entities
    result = result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&#x27;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&hellip;", "…")
        .replace("&copy;", "©")
        .replace("&reg;", "®");

    // Decode numeric HTML entities (&#NNN;)
    let mut decoded = String::with_capacity(result.len());
    let mut chars = result.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' && chars.peek() == Some(&'#') {
            let mut entity = String::from("&#");
            chars.next(); // consume '#'
            while let Some(&nc) = chars.peek() {
                if nc == ';' {
                    chars.next();
                    break;
                }
                entity.push(nc);
                chars.next();
            }
            // Try to parse as number
            let num_str = &entity[2..];
            if let Ok(num) = num_str.parse::<u32>() {
                if let Some(ch) = char::from_u32(num) {
                    decoded.push(ch);
                    continue;
                }
            }
            // Failed to decode — emit original
            decoded.push_str(&entity);
            decoded.push(';');
        } else {
            decoded.push(c);
        }
    }

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
}
