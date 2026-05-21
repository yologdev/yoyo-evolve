//! Lint, test, and security command handlers: /test, /lint, /lint fix, /lint unsafe, /security.

use crate::commands_project::{detect_project_type, ProjectType};
use crate::commands_session::auto_compact_if_needed;
use crate::format::*;
use crate::prompt::run_prompt;

use yoagent::agent::Agent;
use yoagent::*;

/// Return the test command for a given project type.
pub fn test_command_for_project(
    project_type: &ProjectType,
) -> Option<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => Some(("cargo test", vec!["cargo", "test"])),
        ProjectType::Node => Some(("npm test", vec!["npm", "test"])),
        ProjectType::Python => Some(("python -m pytest", vec!["python", "-m", "pytest"])),
        ProjectType::Go => Some(("go test ./...", vec!["go", "test", "./..."])),
        ProjectType::Java => {
            if std::path::Path::new("pom.xml").exists() {
                Some(("mvn test", vec!["mvn", "test"]))
            } else {
                Some(("./gradlew test", vec!["./gradlew", "test"]))
            }
        }
        ProjectType::Ruby => Some((
            "bundle exec rake test",
            vec!["bundle", "exec", "rake", "test"],
        )),
        ProjectType::Cpp => Some((
            "ctest --test-dir build",
            vec!["ctest", "--test-dir", "build"],
        )),
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

/// Lint strictness level for clippy (Rust only; other languages ignore this).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintStrictness {
    /// Default: `-D warnings`
    Default,
    /// Pedantic: `-D warnings -W clippy::pedantic`
    Pedantic,
    /// Strict: `-D warnings -W clippy::pedantic -W clippy::nursery`
    Strict,
}

/// Lint subcommand names for tab completion.
pub const LINT_SUBCOMMANDS: &[&str] = &["fix", "pedantic", "strict", "unsafe"];

/// Return the lint command for a given project type and strictness level.
pub fn lint_command_for_project(
    project_type: &ProjectType,
    strictness: LintStrictness,
) -> Option<(String, Vec<String>)> {
    match project_type {
        ProjectType::Rust => {
            let mut label = String::from("cargo clippy --all-targets -- -D warnings");
            let mut args: Vec<String> =
                vec!["cargo", "clippy", "--all-targets", "--", "-D", "warnings"]
                    .into_iter()
                    .map(String::from)
                    .collect();
            match strictness {
                LintStrictness::Default => {}
                LintStrictness::Pedantic => {
                    label.push_str(" -W clippy::pedantic");
                    args.push("-W".into());
                    args.push("clippy::pedantic".into());
                }
                LintStrictness::Strict => {
                    label.push_str(" -W clippy::pedantic -W clippy::nursery");
                    args.push("-W".into());
                    args.push("clippy::pedantic".into());
                    args.push("-W".into());
                    args.push("clippy::nursery".into());
                }
            }
            Some((label, args))
        }
        ProjectType::Node => Some((
            "npx eslint .".into(),
            vec!["npx".into(), "eslint".into(), ".".into()],
        )),
        ProjectType::Python => Some((
            "ruff check .".into(),
            vec!["ruff".into(), "check".into(), ".".into()],
        )),
        ProjectType::Go => Some((
            "golangci-lint run".into(),
            vec!["golangci-lint".into(), "run".into()],
        )),
        ProjectType::Ruby => Some((
            "bundle exec rubocop".into(),
            vec!["bundle".into(), "exec".into(), "rubocop".into()],
        )),
        ProjectType::Java | ProjectType::Cpp | ProjectType::Make | ProjectType::Unknown => None,
    }
}

/// Handle the /lint command: auto-detect project type and run linter.
/// Returns a summary string suitable for AI context.
/// Accepts the full input string (e.g. "/lint", "/lint pedantic", "/lint strict").
pub fn handle_lint(input: &str) -> Option<String> {
    // Parse strictness from subcommand
    let arg = input.strip_prefix("/lint").unwrap_or("").trim();

    // Dispatch to specialized subcommand handlers
    if arg == "unsafe" {
        return handle_lint_unsafe();
    }

    let strictness = match arg {
        "pedantic" => LintStrictness::Pedantic,
        "strict" => LintStrictness::Strict,
        _ => LintStrictness::Default,
    };

    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }

    let (label, args) = match lint_command_for_project(&project_type, strictness) {
        Some(cmd) => cmd,
        None => {
            println!("{DIM}  No lint command configured for {project_type}{RESET}\n");
            return None;
        }
    };

    println!("{DIM}  Running: {label}...{RESET}");
    let start = std::time::Instant::now();
    let output = std::process::Command::new(&args[0])
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

/// Build a prompt asking the AI to fix lint errors.
/// Takes the lint command label and the raw lint output.
pub fn build_lint_fix_prompt(lint_command: &str, lint_output: &str) -> String {
    let mut prompt = String::from(
        "Fix the following lint errors in this project. Read the relevant files, \
         understand the warnings/errors, and apply fixes:\n\n",
    );
    prompt.push_str(&format!(
        "## Lint errors (`{lint_command}`):\n```\n{lint_output}\n```\n\n"
    ));
    prompt
        .push_str("After fixing, run the lint command again to verify. Fix any remaining issues.");
    prompt
}

/// Handle the `/lint fix` command: run lint and send failures to AI for auto-fixing.
/// Returns Some(fix_prompt) if failures were sent to AI, None otherwise.
pub async fn handle_lint_fix(
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let lint_result = handle_lint("/lint");
    match lint_result {
        Some(ref summary)
            if summary.starts_with("Lint FAILED") || summary.starts_with("Failed to run") =>
        {
            println!("{YELLOW}  Sending lint failures to AI for fixing...{RESET}\n");
            // Extract the lint command label for the prompt
            let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
            let lint_label = lint_command_for_project(&project_type, LintStrictness::Default)
                .map(|(label, _)| label)
                .unwrap_or_else(|| "lint".into());
            let fix_prompt = build_lint_fix_prompt(&lint_label, summary);
            run_prompt(agent, &fix_prompt, session_total, model).await;
            auto_compact_if_needed(agent);
            Some(fix_prompt)
        }
        Some(_) => {
            // Lint passed — nothing to fix
            println!("{GREEN}  No lint errors to fix ✓{RESET}\n");
            None
        }
        None => None,
    }
}

// ── /lint unsafe ────────────────────────────────────────────────────────

/// A single occurrence of `unsafe` found in a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsafeOccurrence {
    pub file: String,
    pub line_number: usize,
    pub line_text: String,
    pub kind: UnsafeKind,
}

/// What kind of `unsafe` usage was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsafeKind {
    Block,
    Function,
    Impl,
    Trait,
}

impl std::fmt::Display for UnsafeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Block => write!(f, "unsafe block"),
            Self::Function => write!(f, "unsafe fn"),
            Self::Impl => write!(f, "unsafe impl"),
            Self::Trait => write!(f, "unsafe trait"),
        }
    }
}

/// Scan file content for `unsafe` usage. Returns occurrences with line numbers.
/// This is the pure, testable core — no filesystem access.
pub fn scan_for_unsafe(file_path: &str, content: &str) -> Vec<UnsafeOccurrence> {
    let mut results = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
            continue;
        }
        // Skip string literals containing "unsafe" — simple heuristic:
        // if the line has a quote before `unsafe`, it's likely in a string
        if let Some(unsafe_pos) = trimmed.find("unsafe") {
            let before = &trimmed[..unsafe_pos];
            // Count unescaped quotes — odd count means we're inside a string
            let quote_count = before.chars().filter(|&c| c == '"').count();
            if quote_count % 2 == 1 {
                continue;
            }
            // Determine kind
            let after_unsafe = &trimmed[unsafe_pos + 6..]; // len("unsafe") == 6
            let kind = if after_unsafe.trim_start().starts_with("fn ") {
                UnsafeKind::Function
            } else if after_unsafe.trim_start().starts_with("impl") {
                UnsafeKind::Impl
            } else if after_unsafe.trim_start().starts_with("trait") {
                UnsafeKind::Trait
            } else if after_unsafe.trim_start().starts_with('{')
                || after_unsafe.trim_start().is_empty()
                || before.is_empty()
                || before.ends_with(' ')
                || before.ends_with('{')
            {
                UnsafeKind::Block
            } else {
                continue; // Not a real unsafe keyword usage
            };
            results.push(UnsafeOccurrence {
                file: file_path.to_string(),
                line_number: idx + 1,
                line_text: line.to_string(),
                kind,
            });
        }
    }
    results
}

/// Check whether file content contains `#![deny(unsafe_code)]` or `#![forbid(unsafe_code)]`.
pub fn has_unsafe_code_attribute(content: &str) -> Option<&'static str> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        if trimmed.contains("#![forbid(unsafe_code)]") {
            return Some("forbid");
        }
        if trimmed.contains("#![deny(unsafe_code)]") {
            return Some("deny");
        }
    }
    None
}

/// Collect all `.rs` files under a directory (non-recursive into target/).
fn collect_rs_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    collect_rs_files_recursive(dir, &mut files);
    files.sort();
    files
}

fn collect_rs_files_recursive(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            // Skip target/, .git/, and hidden directories
            if name == "target" || name == ".git" || name.starts_with('.') {
                continue;
            }
            collect_rs_files_recursive(&path, files);
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
}

/// Handle the `/lint unsafe` command: scan for unsafe code and report findings.
pub fn handle_lint_unsafe() -> Option<String> {
    let cwd = std::env::current_dir().unwrap_or_default();

    // Check for Cargo.toml — this is Rust-specific
    if !cwd.join("Cargo.toml").exists() {
        println!("{DIM}  /lint unsafe is only available for Rust projects (no Cargo.toml found){RESET}\n");
        return None;
    }

    println!("{DIM}  Scanning for unsafe code...{RESET}");

    // Find the crate root file to check for deny/forbid attribute
    let mut crate_root_attr: Option<&str> = None;
    for root_file in &["src/main.rs", "src/lib.rs"] {
        let root_path = cwd.join(root_file);
        if root_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&root_path) {
                if let Some(attr) = has_unsafe_code_attribute(&content) {
                    crate_root_attr = Some(attr);
                    break;
                }
            }
        }
    }

    // Collect and scan all .rs files
    let src_dir = cwd.join("src");
    let scan_dir = if src_dir.exists() { &src_dir } else { &cwd };
    let rs_files = collect_rs_files(scan_dir);

    let mut all_occurrences: Vec<UnsafeOccurrence> = Vec::new();
    for file_path in &rs_files {
        if let Ok(content) = std::fs::read_to_string(file_path) {
            let relative = file_path
                .strip_prefix(&cwd)
                .unwrap_or(file_path)
                .to_string_lossy()
                .to_string();
            let occurrences = scan_for_unsafe(&relative, &content);
            all_occurrences.extend(occurrences);
        }
    }

    // Build report
    let mut summary = String::new();

    if all_occurrences.is_empty() {
        if let Some(attr) = crate_root_attr {
            let msg = format!("✓ No unsafe code found — #![{attr}(unsafe_code)] is active");
            println!("\n{GREEN}  {msg}{RESET}\n");
            summary.push_str(&msg);
        } else {
            println!("\n{GREEN}  ✓ No unsafe code found{RESET}");
            println!(
                "{YELLOW}  💡 Consider adding #![forbid(unsafe_code)] to your crate root for compile-time enforcement{RESET}\n"
            );
            summary.push_str(
                "No unsafe code found. Suggest adding #![forbid(unsafe_code)] to crate root.",
            );
        }
    } else {
        println!(
            "\n{YELLOW}  ⚠ Found {} unsafe occurrence(s):{RESET}\n",
            all_occurrences.len()
        );
        for occ in &all_occurrences {
            println!(
                "  {RED}{}:{}{RESET} — {} — {}",
                occ.file,
                occ.line_number,
                occ.kind,
                occ.line_text.trim()
            );
        }
        summary.push_str(&format!(
            "Found {} unsafe occurrence(s):\n",
            all_occurrences.len()
        ));
        for occ in &all_occurrences {
            summary.push_str(&format!(
                "  {}:{} — {} — {}\n",
                occ.file,
                occ.line_number,
                occ.kind,
                occ.line_text.trim()
            ));
        }

        match crate_root_attr {
            Some(attr) => {
                println!(
                    "\n{DIM}  #![{attr}(unsafe_code)] is set — these unsafe usages require #[allow(unsafe_code)] or will fail to compile{RESET}\n"
                );
                summary.push_str(&format!("\n#![{attr}(unsafe_code)] is set in crate root."));
            }
            None => {
                println!(
                    "\n{YELLOW}  💡 No #![deny(unsafe_code)] or #![forbid(unsafe_code)] found in crate root{RESET}"
                );
                println!(
                    "{YELLOW}  💡 Consider adding #![forbid(unsafe_code)] to prevent future unsafe additions{RESET}\n"
                );
                summary.push_str(
                    "\nNo unsafe_code attribute found. Suggest adding #![forbid(unsafe_code)] to crate root."
                );
            }
        }
    }

    Some(summary)
}
/// Return the security audit command for a given project type, or `None` if
/// the tool isn't installed (with an install hint as the error string).
fn security_audit_command(
    project_type: &ProjectType,
) -> Result<(&'static str, Vec<&'static str>), Option<&'static str>> {
    match project_type {
        ProjectType::Rust => {
            // Check if cargo-audit is installed
            let check = std::process::Command::new("cargo")
                .args(["audit", "--version"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            if check.map(|s| s.success()).unwrap_or(false) {
                Ok(("cargo audit", vec!["cargo", "audit"]))
            } else {
                Err(Some("cargo install cargo-audit"))
            }
        }
        ProjectType::Node => Ok(("npm audit", vec!["npm", "audit", "--json"])),
        ProjectType::Python => {
            // Try pip-audit first, then safety
            let pip_check = std::process::Command::new("pip-audit")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            if pip_check.map(|s| s.success()).unwrap_or(false) {
                Ok(("pip-audit", vec!["pip-audit"]))
            } else {
                let safety_check = std::process::Command::new("safety")
                    .arg("--version")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                if safety_check.map(|s| s.success()).unwrap_or(false) {
                    Ok(("safety check", vec!["safety", "check"]))
                } else {
                    Err(Some("pip install pip-audit"))
                }
            }
        }
        ProjectType::Go => {
            let check = std::process::Command::new("govulncheck")
                .arg("-version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            if check.map(|s| s.success()).unwrap_or(false) {
                Ok(("govulncheck ./...", vec!["govulncheck", "./..."]))
            } else {
                Err(Some("go install golang.org/x/vuln/cmd/govulncheck@latest"))
            }
        }
        ProjectType::Java => Err(Some(
            "For Java, consider: mvn org.owasp:dependency-check-maven:check",
        )),
        ProjectType::Ruby => {
            let check = std::process::Command::new("bundle-audit")
                .arg("version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            if check.map(|s| s.success()).unwrap_or(false) {
                Ok(("bundle-audit check", vec!["bundle-audit", "check"]))
            } else {
                Err(Some("gem install bundler-audit"))
            }
        }
        _ => Err(None),
    }
}

/// Severity level for vulnerability findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Critical => write!(f, "critical"),
            Severity::High => write!(f, "high"),
            Severity::Medium => write!(f, "medium"),
            Severity::Low => write!(f, "low"),
            Severity::Info => write!(f, "info"),
        }
    }
}

fn severity_color(severity: &Severity) -> &'static Color {
    match severity {
        Severity::Critical | Severity::High => &RED,
        Severity::Medium => &YELLOW,
        Severity::Low | Severity::Info => &DIM,
    }
}

/// Parse npm audit --json output into a severity summary.
fn parse_npm_audit_json(json_str: &str) -> (Vec<(Severity, u32)>, Vec<String>) {
    let mut counts = Vec::new();
    let mut findings = Vec::new();

    // npm audit JSON has: { "metadata": { "vulnerabilities": { "critical": N, ... } } }
    // and: { "vulnerabilities": { "<pkg>": { "severity": "...", ... } } }
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
        // Try metadata.vulnerabilities first (npm 7+)
        if let Some(meta_vulns) = val.get("metadata").and_then(|m| m.get("vulnerabilities")) {
            for (sev_name, sev_enum) in [
                ("critical", Severity::Critical),
                ("high", Severity::High),
                ("moderate", Severity::Medium),
                ("low", Severity::Low),
                ("info", Severity::Info),
            ] {
                if let Some(n) = meta_vulns.get(sev_name).and_then(|v| v.as_u64()) {
                    if n > 0 {
                        counts.push((sev_enum, n as u32));
                    }
                }
            }
        }

        // Extract top findings from vulnerabilities object
        if let Some(vulns) = val.get("vulnerabilities").and_then(|v| v.as_object()) {
            for (pkg, info) in vulns.iter().take(10) {
                let sev = info
                    .get("severity")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                findings.push(format!("{pkg} ({sev})"));
            }
        }
    }

    (counts, findings)
}

/// Run a dependency vulnerability scan appropriate for the detected project type.
pub fn handle_security() -> Option<String> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let project_type = detect_project_type(&cwd);

    println!("\n{BOLD}  \u{1f512} Security Audit{RESET}");
    println!("{DIM}  Detected project: {project_type}{RESET}");

    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  Could not detect project type. Looked for: Cargo.toml, package.json, go.mod, pyproject.toml, etc.{RESET}"
        );
        println!("{DIM}  Try running your language's audit tool manually:{RESET}");
        println!("{DIM}    Rust:   cargo audit{RESET}");
        println!("{DIM}    Node:   npm audit{RESET}");
        println!("{DIM}    Python: pip-audit{RESET}");
        println!("{DIM}    Go:     govulncheck ./...{RESET}\n");
        return Some("Security scan: could not detect project type".to_string());
    }

    let (label, args) = match security_audit_command(&project_type) {
        Ok(cmd) => cmd,
        Err(Some(install_hint)) => {
            println!("\n{YELLOW}  \u{26a0} No audit tool found for {project_type}{RESET}");
            println!("{DIM}  Install with: {install_hint}{RESET}\n");
            return Some(format!(
                "Security scan: audit tool not installed for {project_type}. Install with: {install_hint}"
            ));
        }
        Err(None) => {
            println!("\n{DIM}  No audit tool configured for {project_type}{RESET}\n");
            return Some(format!(
                "Security scan: no audit tool configured for {project_type}"
            ));
        }
    };

    println!("{DIM}  Running: {label}...{RESET}");
    let start = std::time::Instant::now();

    let output = std::process::Command::new(args[0])
        .args(&args[1..])
        .output();

    let elapsed = format_duration(start.elapsed());

    match output {
        Err(e) => {
            println!("\n{RED}  \u{2717} Failed to run {label}: {e}{RESET}\n");
            Some(format!("Security scan failed to run {label}: {e}"))
        }
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            let combined = if stderr.is_empty() {
                stdout.to_string()
            } else {
                format!("{stdout}\n{stderr}")
            };

            // For npm audit --json, parse the JSON for a clean summary
            let is_npm_json = project_type == ProjectType::Node && args.contains(&"--json");

            if o.status.success() {
                println!("\n{GREEN}  \u{2713} No vulnerabilities found ({elapsed}){RESET}\n");
                if !combined.trim().is_empty() {
                    let lines: Vec<&str> = combined.lines().collect();
                    let tail = if lines.len() > 5 {
                        &lines[lines.len() - 5..]
                    } else {
                        &lines
                    };
                    for line in tail {
                        println!("{DIM}  {line}{RESET}");
                    }
                    println!();
                }
                Some(format!("Security scan passed ({elapsed}): {label}"))
            } else {
                let code = o.status.code().unwrap_or(-1);

                if is_npm_json {
                    let (counts, findings) = parse_npm_audit_json(&stdout);
                    if !counts.is_empty() || !findings.is_empty() {
                        let total: u32 = counts.iter().map(|(_, n)| n).sum();
                        println!(
                            "\n{RED}  \u{26a0} Found {total} vulnerability/ies ({elapsed}){RESET}\n"
                        );

                        for (sev, count) in &counts {
                            let color = severity_color(sev);
                            println!("  {color}  {sev}: {count}{RESET}");
                        }
                        if !findings.is_empty() {
                            println!();
                            println!("{DIM}  Top findings:{RESET}");
                            for f in &findings {
                                println!("  {YELLOW}  \u{2022} {f}{RESET}");
                            }
                        }
                        println!();

                        let mut summary = format!(
                            "Security scan FOUND {total} vulnerabilities ({elapsed}): {label}\n"
                        );
                        for (sev, count) in &counts {
                            summary.push_str(&format!("  {sev}: {count}\n"));
                        }
                        return Some(summary);
                    }
                }

                // Generic output for non-npm or fallback
                println!("\n{RED}  \u{26a0} Audit found issues (exit {code}, {elapsed}){RESET}\n");

                let lines: Vec<&str> = combined.lines().collect();
                let tail = if lines.len() > 20 {
                    &lines[lines.len() - 20..]
                } else {
                    &lines
                };
                for line in tail {
                    println!("  {line}");
                }
                println!();

                let mut summary =
                    format!("Security scan found issues (exit {code}, {elapsed}): {label}\n");
                let preview: String = tail.iter().take(20).fold(String::new(), |mut acc, l| {
                    acc.push_str(l);
                    acc.push('\n');
                    acc
                });
                summary.push_str(&preview);
                Some(summary)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{is_unknown_command, KNOWN_COMMANDS};

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

    #[test]
    fn lint_command_rust() {
        let cmd = lint_command_for_project(&ProjectType::Rust, LintStrictness::Default);
        assert!(cmd.is_some());
        assert!(cmd.unwrap().0.contains("clippy"));
    }

    #[test]
    fn lint_command_make_none() {
        assert!(lint_command_for_project(&ProjectType::Make, LintStrictness::Default).is_none());
    }

    #[test]
    fn lint_command_unknown_none() {
        assert!(lint_command_for_project(&ProjectType::Unknown, LintStrictness::Default).is_none());
    }

    #[test]
    fn lint_fix_prompt_contains_command_and_output() {
        let prompt = build_lint_fix_prompt(
            "cargo clippy --all-targets -- -D warnings",
            "warning: unused variable `x`\n  --> src/main.rs:5:9",
        );
        assert!(prompt.contains("cargo clippy"));
        assert!(prompt.contains("unused variable"));
        assert!(prompt.contains("src/main.rs:5:9"));
    }

    #[test]
    fn lint_fix_prompt_asks_to_fix() {
        let prompt = build_lint_fix_prompt("ruff check .", "E501 line too long");
        assert!(prompt.contains("Fix the following lint errors"));
        assert!(prompt.contains("ruff check ."));
        assert!(prompt.contains("E501 line too long"));
        assert!(prompt.contains("run the lint command again to verify"));
    }

    #[test]
    fn lint_fix_prompt_includes_structured_output() {
        let lint_output = "Lint FAILED (exit 1, 2.3s): cargo clippy\n\nLast output:\nwarning: field `foo` is never read";
        let prompt =
            build_lint_fix_prompt("cargo clippy --all-targets -- -D warnings", lint_output);
        assert!(prompt.contains("## Lint errors"));
        assert!(prompt.contains("field `foo` is never read"));
    }

    #[test]
    fn test_test_command_recognized() {
        assert!(!is_unknown_command("/test"));
        assert!(
            KNOWN_COMMANDS.contains(&"/test"),
            "/test should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_test_command_for_rust_project() {
        let cmd = test_command_for_project(&ProjectType::Rust);
        assert!(cmd.is_some(), "Rust project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("cargo"),
            "Rust test label should mention cargo"
        );
        assert_eq!(args[0], "cargo");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_node_project() {
        let cmd = test_command_for_project(&ProjectType::Node);
        assert!(cmd.is_some(), "Node project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(label.contains("npm"), "Node test label should mention npm");
        assert_eq!(args[0], "npm");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_python_project() {
        let cmd = test_command_for_project(&ProjectType::Python);
        assert!(cmd.is_some(), "Python project should have a test command");
        let (label, _args) = cmd.unwrap();
        assert!(
            label.contains("pytest"),
            "Python test label should mention pytest"
        );
    }

    #[test]
    fn test_test_command_for_go_project() {
        let cmd = test_command_for_project(&ProjectType::Go);
        assert!(cmd.is_some(), "Go project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(label.contains("go"), "Go test label should mention go");
        assert_eq!(args[0], "go");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_make_project() {
        let cmd = test_command_for_project(&ProjectType::Make);
        assert!(cmd.is_some(), "Make project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("make"),
            "Make test label should mention make"
        );
        assert_eq!(args[0], "make");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_java_project() {
        let cmd = test_command_for_project(&ProjectType::Java);
        assert!(cmd.is_some(), "Java project should have a test command");
        let (label, _) = cmd.unwrap();
        // Should be either mvn or gradlew depending on pom.xml presence
        assert!(
            label.contains("mvn") || label.contains("gradlew"),
            "Java test label should mention mvn or gradlew, got: {label}"
        );
    }

    #[test]
    fn test_test_command_for_ruby_project() {
        let cmd = test_command_for_project(&ProjectType::Ruby);
        assert!(cmd.is_some(), "Ruby project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("rake"),
            "Ruby test label should mention rake"
        );
        assert_eq!(args[0], "bundle");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_cpp_project() {
        let cmd = test_command_for_project(&ProjectType::Cpp);
        assert!(cmd.is_some(), "Cpp project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("ctest"),
            "Cpp test label should mention ctest"
        );
        assert_eq!(args[0], "ctest");
    }

    #[test]
    fn test_test_command_for_unknown_project() {
        let cmd = test_command_for_project(&ProjectType::Unknown);
        assert!(
            cmd.is_none(),
            "Unknown project should not have a test command"
        );
    }

    #[test]
    fn test_lint_command_recognized() {
        assert!(!is_unknown_command("/lint"));
        assert!(
            KNOWN_COMMANDS.contains(&"/lint"),
            "/lint should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_lint_command_for_rust_project() {
        let cmd = lint_command_for_project(&ProjectType::Rust, LintStrictness::Default);
        assert!(cmd.is_some(), "Rust project should have a lint command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("clippy"),
            "Rust lint label should mention clippy"
        );
        assert_eq!(args[0], "cargo");
        assert!(args.iter().any(|a| a == "clippy"));
    }

    #[test]
    fn test_lint_command_for_node_project() {
        let cmd = lint_command_for_project(&ProjectType::Node, LintStrictness::Default);
        assert!(cmd.is_some(), "Node project should have a lint command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("eslint"),
            "Node lint label should mention eslint"
        );
        assert_eq!(args[0], "npx");
        assert!(args.iter().any(|a| a == "eslint"));
    }

    #[test]
    fn test_lint_command_for_python_project() {
        let cmd = lint_command_for_project(&ProjectType::Python, LintStrictness::Default);
        assert!(cmd.is_some(), "Python project should have a lint command");
        let (label, _args) = cmd.unwrap();
        assert!(
            label.contains("ruff"),
            "Python lint label should mention ruff"
        );
    }

    #[test]
    fn test_lint_command_for_go_project() {
        let cmd = lint_command_for_project(&ProjectType::Go, LintStrictness::Default);
        assert!(cmd.is_some(), "Go project should have a lint command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("golangci-lint"),
            "Go lint label should mention golangci-lint"
        );
        assert_eq!(args[0], "golangci-lint");
    }

    #[test]
    fn test_lint_command_for_make_project() {
        let cmd = lint_command_for_project(&ProjectType::Make, LintStrictness::Default);
        assert!(cmd.is_none(), "Make project should not have a lint command");
    }

    #[test]
    fn test_lint_command_for_unknown_project() {
        let cmd = lint_command_for_project(&ProjectType::Unknown, LintStrictness::Default);
        assert!(
            cmd.is_none(),
            "Unknown project should not have a lint command"
        );
    }

    #[test]
    fn test_lint_command_for_ruby_project() {
        let cmd = lint_command_for_project(&ProjectType::Ruby, LintStrictness::Default);
        assert!(cmd.is_some(), "Ruby project should have a lint command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("rubocop"),
            "Ruby lint label should mention rubocop"
        );
        assert_eq!(args[0], "bundle");
        assert!(args.iter().any(|a| a == "rubocop"));
    }

    #[test]
    fn test_lint_command_for_java_project() {
        let cmd = lint_command_for_project(&ProjectType::Java, LintStrictness::Default);
        assert!(
            cmd.is_none(),
            "Java project should not have a lint command (too varied)"
        );
    }

    #[test]
    fn test_lint_command_for_cpp_project() {
        let cmd = lint_command_for_project(&ProjectType::Cpp, LintStrictness::Default);
        assert!(
            cmd.is_none(),
            "Cpp project should not have a lint command (too varied)"
        );
    }

    #[test]
    fn test_lint_pedantic_adds_flag() {
        let cmd = lint_command_for_project(&ProjectType::Rust, LintStrictness::Pedantic);
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("-W clippy::pedantic"),
            "Pedantic label should contain -W clippy::pedantic, got: {label}"
        );
        assert!(
            args.iter().any(|a| a == "clippy::pedantic"),
            "Pedantic args should contain clippy::pedantic"
        );
    }

    #[test]
    fn test_lint_strict_adds_both_flags() {
        let cmd = lint_command_for_project(&ProjectType::Rust, LintStrictness::Strict);
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("-W clippy::pedantic"),
            "Strict label should contain -W clippy::pedantic, got: {label}"
        );
        assert!(
            label.contains("-W clippy::nursery"),
            "Strict label should contain -W clippy::nursery, got: {label}"
        );
        assert!(
            args.iter().any(|a| a == "clippy::pedantic"),
            "Strict args should contain clippy::pedantic"
        );
        assert!(
            args.iter().any(|a| a == "clippy::nursery"),
            "Strict args should contain clippy::nursery"
        );
    }

    #[test]
    fn test_lint_default_no_extra_flags() {
        let cmd = lint_command_for_project(&ProjectType::Rust, LintStrictness::Default);
        let (label, args) = cmd.unwrap();
        assert!(
            !label.contains("clippy::pedantic"),
            "Default should not contain clippy::pedantic"
        );
        assert!(
            !label.contains("clippy::nursery"),
            "Default should not contain clippy::nursery"
        );
        assert!(
            !args.iter().any(|a| a == "clippy::pedantic"),
            "Default args should not contain clippy::pedantic"
        );
    }

    #[test]
    fn test_lint_strictness_ignored_for_non_rust() {
        // Non-Rust projects should return the same command regardless of strictness
        let default = lint_command_for_project(&ProjectType::Node, LintStrictness::Default);
        let pedantic = lint_command_for_project(&ProjectType::Node, LintStrictness::Pedantic);
        let strict = lint_command_for_project(&ProjectType::Node, LintStrictness::Strict);
        assert_eq!(default, pedantic);
        assert_eq!(default, strict);
    }

    #[test]
    fn scan_for_unsafe_finds_blocks() {
        let content = r#"
fn main() {
    unsafe {
        std::ptr::null::<u8>();
    }
}
"#;
        let results = scan_for_unsafe("test.rs", content);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, UnsafeKind::Block);
        assert_eq!(results[0].line_number, 3);
        assert_eq!(results[0].file, "test.rs");
    }

    #[test]
    fn scan_for_unsafe_finds_functions() {
        let content = r#"
unsafe fn dangerous() {
    // do something dangerous
}
"#;
        let results = scan_for_unsafe("test.rs", content);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, UnsafeKind::Function);
        assert_eq!(results[0].line_number, 2);
    }

    #[test]
    fn scan_for_unsafe_finds_impl() {
        let content = r#"
unsafe impl Send for MyType {}
"#;
        let results = scan_for_unsafe("test.rs", content);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, UnsafeKind::Impl);
    }

    #[test]
    fn scan_for_unsafe_finds_trait() {
        let content = r#"
unsafe trait MyTrait {}
"#;
        let results = scan_for_unsafe("test.rs", content);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, UnsafeKind::Trait);
    }

    #[test]
    fn scan_for_unsafe_ignores_comments() {
        let content = r#"
// unsafe { this is a comment }
fn safe() {}
"#;
        let results = scan_for_unsafe("test.rs", content);
        assert!(results.is_empty());
    }

    #[test]
    fn scan_for_unsafe_ignores_strings() {
        let content = r#"
let s = "unsafe { not real code }";
"#;
        let results = scan_for_unsafe("test.rs", content);
        assert!(results.is_empty());
    }

    #[test]
    fn scan_for_unsafe_no_occurrences() {
        let content = r#"
fn main() {
    println!("hello world");
}
"#;
        let results = scan_for_unsafe("test.rs", content);
        assert!(results.is_empty());
    }

    #[test]
    fn scan_for_unsafe_multiple_occurrences() {
        let content = r#"
unsafe fn one() {}
fn two() {
    unsafe {
        // block
    }
}
unsafe impl Send for Foo {}
"#;
        let results = scan_for_unsafe("test.rs", content);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].kind, UnsafeKind::Function);
        assert_eq!(results[1].kind, UnsafeKind::Block);
        assert_eq!(results[2].kind, UnsafeKind::Impl);
    }

    #[test]
    fn detects_forbid_attribute() {
        let content = "#![forbid(unsafe_code)]\nfn main() {}";
        assert_eq!(has_unsafe_code_attribute(content), Some("forbid"));
    }

    #[test]
    fn detects_deny_attribute() {
        let content = "#![deny(unsafe_code)]\nfn main() {}";
        assert_eq!(has_unsafe_code_attribute(content), Some("deny"));
    }

    #[test]
    fn no_attribute_returns_none() {
        let content = "fn main() {}";
        assert_eq!(has_unsafe_code_attribute(content), None);
    }

    #[test]
    fn ignores_commented_attribute() {
        let content = "// #![forbid(unsafe_code)]\nfn main() {}";
        assert_eq!(has_unsafe_code_attribute(content), None);
    }

    #[test]
    fn lint_unsafe_in_subcommands() {
        assert!(
            LINT_SUBCOMMANDS.contains(&"unsafe"),
            "LINT_SUBCOMMANDS should contain 'unsafe'"
        );
    }

    // ── /security tests ──

    #[test]
    fn security_command_recognized() {
        assert!(!is_unknown_command("/security"));
        assert!(
            KNOWN_COMMANDS.contains(&"/security"),
            "/security should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn security_audit_command_rust() {
        // Result depends on whether cargo-audit is installed, but type must be correct
        let result = security_audit_command(&ProjectType::Rust);
        match result {
            Ok((label, args)) => {
                assert_eq!(label, "cargo audit");
                assert_eq!(args[0], "cargo");
                assert!(args.contains(&"audit"));
            }
            Err(hint) => {
                assert_eq!(hint, Some("cargo install cargo-audit"));
            }
        }
    }

    #[test]
    fn security_audit_command_node() {
        // Node always returns Ok (npm is assumed present)
        let result = security_audit_command(&ProjectType::Node);
        assert!(result.is_ok(), "Node should always return a command");
        let (label, args) = result.unwrap();
        assert_eq!(label, "npm audit");
        assert_eq!(args[0], "npm");
        assert!(args.contains(&"audit"));
        assert!(args.contains(&"--json"));
    }

    #[test]
    fn security_audit_command_python() {
        // Returns Ok with pip-audit or safety, or Err with install hint
        let result = security_audit_command(&ProjectType::Python);
        match result {
            Ok((label, _args)) => {
                assert!(
                    label == "pip-audit" || label == "safety check",
                    "Python audit label should be pip-audit or safety check, got: {label}"
                );
            }
            Err(hint) => {
                assert_eq!(hint, Some("pip install pip-audit"));
            }
        }
    }

    #[test]
    fn security_audit_command_go() {
        let result = security_audit_command(&ProjectType::Go);
        match result {
            Ok((label, args)) => {
                assert_eq!(label, "govulncheck ./...");
                assert_eq!(args[0], "govulncheck");
            }
            Err(hint) => {
                assert_eq!(
                    hint,
                    Some("go install golang.org/x/vuln/cmd/govulncheck@latest")
                );
            }
        }
    }

    #[test]
    fn security_audit_command_ruby() {
        let result = security_audit_command(&ProjectType::Ruby);
        match result {
            Ok((label, args)) => {
                assert_eq!(label, "bundle-audit check");
                assert_eq!(args[0], "bundle-audit");
            }
            Err(hint) => {
                assert_eq!(hint, Some("gem install bundler-audit"));
            }
        }
    }

    #[test]
    fn security_audit_command_java() {
        // Java always returns Err with a Maven hint
        let result = security_audit_command(&ProjectType::Java);
        assert!(result.is_err(), "Java should return Err (hint only)");
        let hint = result.unwrap_err();
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("mvn"));
    }

    #[test]
    fn security_audit_command_unknown() {
        let result = security_audit_command(&ProjectType::Unknown);
        assert!(result.is_err(), "Unknown project should return Err");
        assert_eq!(result.unwrap_err(), None);
    }

    #[test]
    fn security_audit_command_make_returns_none() {
        // Make has no security audit tool
        let result = security_audit_command(&ProjectType::Make);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), None);
    }

    #[test]
    fn parse_npm_audit_json_full() {
        let json = r#"{
            "metadata": {
                "vulnerabilities": {
                    "critical": 1,
                    "high": 2,
                    "moderate": 3,
                    "low": 4,
                    "info": 0
                }
            },
            "vulnerabilities": {
                "lodash": {"severity": "critical", "via": []},
                "express": {"severity": "high", "via": []}
            }
        }"#;

        let (counts, findings) = parse_npm_audit_json(json);
        assert_eq!(counts.len(), 4, "Should have 4 non-zero severity counts");
        assert_eq!(counts[0], (Severity::Critical, 1));
        assert_eq!(counts[1], (Severity::High, 2));
        assert_eq!(counts[2], (Severity::Medium, 3));
        assert_eq!(counts[3], (Severity::Low, 4));

        assert_eq!(findings.len(), 2);
        assert!(findings.iter().any(|f| f.contains("lodash")));
        assert!(findings.iter().any(|f| f.contains("express")));
    }

    #[test]
    fn parse_npm_audit_json_empty() {
        let json = r#"{
            "metadata": {
                "vulnerabilities": {
                    "critical": 0,
                    "high": 0,
                    "moderate": 0,
                    "low": 0,
                    "info": 0
                }
            },
            "vulnerabilities": {}
        }"#;

        let (counts, findings) = parse_npm_audit_json(json);
        assert!(counts.is_empty(), "All-zero counts should be empty");
        assert!(findings.is_empty());
    }

    #[test]
    fn parse_npm_audit_json_invalid() {
        let (counts, findings) = parse_npm_audit_json("not valid json");
        assert!(counts.is_empty());
        assert!(findings.is_empty());
    }

    #[test]
    fn parse_npm_audit_json_missing_metadata() {
        // Only vulnerabilities object, no metadata
        let json = r#"{
            "vulnerabilities": {
                "shelljs": {"severity": "high", "via": []}
            }
        }"#;
        let (counts, findings) = parse_npm_audit_json(json);
        assert!(counts.is_empty(), "No metadata means no severity counts");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("shelljs"));
        assert!(findings[0].contains("high"));
    }

    #[test]
    fn parse_npm_audit_json_truncates_findings() {
        // More than 10 vulnerabilities — should only take first 10
        let mut vulns = String::from("{\"vulnerabilities\":{");
        for i in 0..15 {
            if i > 0 {
                vulns.push(',');
            }
            vulns.push_str(&format!("\"pkg-{i}\":{{\"severity\":\"low\",\"via\":[]}}"));
        }
        vulns.push_str("},\"metadata\":{\"vulnerabilities\":{\"low\":15}}}");

        let (counts, findings) = parse_npm_audit_json(&vulns);
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0], (Severity::Low, 15));
        assert_eq!(
            findings.len(),
            10,
            "Should truncate findings to 10, got {}",
            findings.len()
        );
    }

    #[test]
    fn severity_display() {
        assert_eq!(format!("{}", Severity::Critical), "critical");
        assert_eq!(format!("{}", Severity::High), "high");
        assert_eq!(format!("{}", Severity::Medium), "medium");
        assert_eq!(format!("{}", Severity::Low), "low");
        assert_eq!(format!("{}", Severity::Info), "info");
    }

    #[test]
    fn severity_colors_correct() {
        // Critical and High should be red
        assert!(std::ptr::eq(severity_color(&Severity::Critical), &RED));
        assert!(std::ptr::eq(severity_color(&Severity::High), &RED));
        // Medium should be yellow
        assert!(std::ptr::eq(severity_color(&Severity::Medium), &YELLOW));
        // Low and Info should be dim
        assert!(std::ptr::eq(severity_color(&Severity::Low), &DIM));
        assert!(std::ptr::eq(severity_color(&Severity::Info), &DIM));
    }
}
