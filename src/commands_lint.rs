//! Lint, test, and security command handlers: /test, /lint, /lint fix, /lint unsafe, /security.

use crate::commands_project::{detect_project_type, ProjectType};
use crate::commands_session::auto_compact_if_needed;
use crate::format::*;
use crate::prompt::run_prompt;

use yoagent::agent::Agent;
use yoagent::*;

/// Return the test command for a given project type, resolved against `dir`.
///
/// `dir` is only consulted by the `Java` arm, which picks Maven over Gradle by
/// looking for a `pom.xml`. Before #746 that lookup was `Path::new("pom.xml")`,
/// i.e. a read of the *process* cwd from a function whose only input was a
/// `ProjectType` — so `watch.rs`'s directory-parameterised callers
/// (`detect_watch_all_command_for_dir`, `detect_watch_all_phases_for_dir`) asked
/// about `dir` and were answered about somewhere else. Callers that genuinely
/// have no directory pass `Path::new(".")`, which is byte-identical to the old
/// behaviour.
pub fn test_command_for_project(
    project_type: &ProjectType,
    dir: &std::path::Path,
) -> Option<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => Some(("cargo test", vec!["cargo", "test"])),
        ProjectType::Node => Some(("npm test", vec!["npm", "test"])),
        ProjectType::Python => Some(("python -m pytest", vec!["python", "-m", "pytest"])),
        ProjectType::Go => Some(("go test ./...", vec!["go", "test", "./..."])),
        ProjectType::Java => {
            if dir.join("pom.xml").exists() {
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

/// Combine a detected test command with caller-supplied args (#745).
///
/// Returns `(display, argv)`: the label to echo, and the argv to spawn. Caller
/// args are appended VERBATIM after the detected command's own argv — they are
/// never validated or translated per project type, so a flag the runner does
/// not understand surfaces as that runner's own error, which is what a
/// `cargo`/`pytest`/`go test` user wants. An empty `extra` returns the detected
/// argv and label unchanged, byte-identically to the no-arg path.
pub(crate) fn build_test_invocation(
    label: &str,
    base: &[&str],
    extra: &[String],
) -> (String, Vec<String>) {
    let argv: Vec<String> = base
        .iter()
        .map(|s| (*s).to_string())
        .chain(extra.iter().cloned())
        .collect();
    let display = if extra.is_empty() {
        label.to_string()
    } else {
        format!("{label} {}", extra.join(" "))
    };
    (display, argv)
}

/// Handle the /test command: auto-detect project type and run tests.
///
/// `extra` holds args the user typed after `/test` (REPL) or `yoyo test` (CLI);
/// they are forwarded verbatim to the detected runner. Empty = run the whole
/// suite, as before.
/// Returns a summary string suitable for AI context.
pub fn handle_test(extra: &[String]) -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }

    let (label, args) = match test_command_for_project(&project_type, std::path::Path::new(".")) {
        Some(cmd) => cmd,
        None => {
            println!("{DIM}  No test command configured for {project_type}{RESET}\n");
            return None;
        }
    };

    let (label, argv) = build_test_invocation(label, &args, extra);
    println!("{DIM}  Running: {label}...{RESET}");
    let start = std::time::Instant::now();
    let output = std::process::Command::new(&argv[0])
        .args(&argv[1..])
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
                append_tail_preview(&mut summary, &error_text, 20);
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

/// True when `arg` (the text after `/lint`) names the `fix` subcommand.
/// `"fixme"` is a different token and is not one.
pub(crate) fn arg_is_fix_subcommand(arg: &str) -> bool {
    arg == "fix" || arg.starts_with("fix ")
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

    // `/lint fix` is routed to handle_lint_fix (dispatch.rs), which needs a live
    // agent. Two paths miss that route and land here: the `yoyo lint fix` CLI
    // subcommand (dispatch_sub.rs has no agent) and `/lint fix --all` (only the
    // exact string "/lint fix" matches). Both used to fall through to
    // LintStrictness::Default and run a plain lint with nothing naming the token.
    if arg_is_fix_subcommand(arg) {
        println!(
            "{DIM}  `lint fix` needs an interactive session — it sends lint failures to the AI to fix.{RESET}"
        );
        println!("{DIM}  Run `yoyo`, then `/lint fix` with no extra arguments.{RESET}\n");
        return None;
    }

    let strictness = match arg {
        "pedantic" => LintStrictness::Pedantic,
        "strict" => LintStrictness::Strict,
        "" => LintStrictness::Default,
        // Day 165 (blind round 40, h5): an unrecognised token used to fall through
        // to `LintStrictness::Default`, so `/lint pedntic` ran a plain lint and
        // reported "Lint passed" — the user's request silently became a different
        // operation. Name what was not recognised instead, as /risk and /fork do.
        _ => {
            println!("{YELLOW}  Unknown /lint subcommand: {arg}{RESET}");
            println!(
                "{DIM}  Available: {}{RESET}\n",
                LINT_SUBCOMMANDS.join(" | ")
            );
            return Some(format!(
                "Unknown /lint subcommand: {arg} (available: {})",
                LINT_SUBCOMMANDS.join(" | ")
            ));
        }
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
                append_tail_preview(&mut summary, &error_text, 20);
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
/// An external tool `security_audit_command` may probe the machine for.
///
/// An enum rather than a bare program name so the probe is exhaustive (a new
/// tool is a compile error in `probe_audit_tool`) and so a test can state
/// exactly which tools it is pretending are installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuditTool {
    CargoAudit,
    PipAudit,
    Safety,
    Govulncheck,
    BundleAudit,
}

/// Ask the machine whether an audit tool is installed.
///
/// **This is the only function in this pair that spawns a subprocess**, and
/// every program name is spelled as a literal on purpose: `tests/
/// cargo_spawning_tests.rs` derives its spawner set by textual match, so
/// routing these through a variable would make the `cargo` spawn invisible to
/// that gate — laundering the defect past the check rather than fixing it.
///
/// A tool that cannot be executed at all (`Err`) and one that runs and fails
/// (non-zero exit) are both "not installed", which is what the pre-split code
/// did with `.map(|s| s.success()).unwrap_or(false)`.
fn probe_audit_tool(tool: AuditTool) -> bool {
    let status = match tool {
        AuditTool::CargoAudit => std::process::Command::new("cargo")
            .args(["audit", "--version"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status(),
        AuditTool::PipAudit => std::process::Command::new("pip-audit")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status(),
        AuditTool::Safety => std::process::Command::new("safety")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status(),
        AuditTool::Govulncheck => std::process::Command::new("govulncheck")
            .arg("-version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status(),
        AuditTool::BundleAudit => std::process::Command::new("bundle-audit")
            .arg("version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status(),
    };
    status.map(|s| s.success()).unwrap_or(false)
}

/// Return the security audit command for a given project type, or `None` if
/// the tool isn't installed (with an install hint as the error string).
///
/// Thin wrapper: it supplies the real subprocess probe and holds no decision.
/// The whole decision lives in [`security_audit_command_with`], which shells
/// nothing — so a test can pin what `/security` runs on a machine with or
/// without a given tool, instead of accommodating whichever answer the
/// machine happens to give (#834).
fn security_audit_command(
    project_type: &ProjectType,
) -> Result<(&'static str, Vec<&'static str>), Option<&'static str>> {
    security_audit_command_with(project_type, &probe_audit_tool)
}

/// The decision half: which audit command a project type gets, given an
/// answer to "is this tool installed?".
///
/// `installed` is consulted **lazily and in the original order** — a project
/// type that needs no probe asks for none, and Python asks about `safety`
/// only once `pip-audit` has said no. That ordering is behaviour, not an
/// implementation detail: it decides which tool a user's `/security` runs
/// when both are present, so it is pinned by test.
fn security_audit_command_with(
    project_type: &ProjectType,
    installed: &dyn Fn(AuditTool) -> bool,
) -> Result<(&'static str, Vec<&'static str>), Option<&'static str>> {
    match project_type {
        ProjectType::Rust => {
            if installed(AuditTool::CargoAudit) {
                Ok(("cargo audit", vec!["cargo", "audit"]))
            } else {
                Err(Some("cargo install cargo-audit"))
            }
        }
        ProjectType::Node => Ok(("npm audit", vec!["npm", "audit", "--json"])),
        ProjectType::Python => {
            // Try pip-audit first, then safety.
            if installed(AuditTool::PipAudit) {
                Ok(("pip-audit", vec!["pip-audit"]))
            } else if installed(AuditTool::Safety) {
                Ok(("safety check", vec!["safety", "check"]))
            } else {
                Err(Some("pip install pip-audit"))
            }
        }
        ProjectType::Go => {
            if installed(AuditTool::Govulncheck) {
                Ok(("govulncheck ./...", vec!["govulncheck", "./..."]))
            } else {
                Err(Some("go install golang.org/x/vuln/cmd/govulncheck@latest"))
            }
        }
        ProjectType::Java => Err(Some(
            "For Java, consider: mvn org.owasp:dependency-check-maven:check",
        )),
        ProjectType::Ruby => {
            if installed(AuditTool::BundleAudit) {
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
    fn empty_args_leave_the_detected_test_invocation_byte_identical() {
        // #745: `/test` with no args is the common case, and threading args
        // through must not disturb it. Pin the argv and the echoed label, not
        // the side effect — no test here spawns a test runner.
        for pt in [
            ProjectType::Rust,
            ProjectType::Node,
            ProjectType::Python,
            ProjectType::Go,
            ProjectType::Ruby,
            ProjectType::Cpp,
            ProjectType::Make,
        ] {
            let (label, base) = test_command_for_project(&pt, std::path::Path::new("."))
                .expect("every listed type has a cmd");
            let (display, argv) = build_test_invocation(label, &base, &[]);
            assert_eq!(display, label, "empty args must echo the bare label");
            assert_eq!(
                argv,
                base.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                "empty args must produce the detected argv unchanged"
            );
        }
    }

    #[test]
    fn caller_args_are_appended_in_order_after_the_detected_argv() {
        // #745: `yoyo test --lib` used to silently run the whole suite. Args are
        // forwarded verbatim, in order, after whatever argv was detected.
        let (label, base) =
            test_command_for_project(&ProjectType::Rust, std::path::Path::new(".")).unwrap();
        let extra = vec!["--lib".to_string(), "--".to_string(), "--nocapture".into()];
        let (display, argv) = build_test_invocation(label, &base, &extra);
        assert_eq!(argv, vec!["cargo", "test", "--lib", "--", "--nocapture"]);
        assert_eq!(display, "cargo test --lib -- --nocapture");

        // A multi-word base keeps its own argv intact ahead of the caller's.
        let (rb_label, rb_base) =
            test_command_for_project(&ProjectType::Ruby, std::path::Path::new(".")).unwrap();
        let (_, rb_argv) = build_test_invocation(rb_label, &rb_base, &["TEST=x".to_string()]);
        assert_eq!(rb_argv, vec!["bundle", "exec", "rake", "test", "TEST=x"]);
    }

    #[test]
    fn fix_subcommand_is_recognised_not_swallowed_as_default_strictness() {
        // Round 31 (#h1): `yoyo lint fix` (dispatch_sub.rs) and `/lint fix --all`
        // (route_command sends it to CommandRoute::Lint, not LintFix) both land in
        // handle_lint, where "fix" used to fall through `_ => LintStrictness::Default`
        // and run a plain lint with nothing naming the token.
        assert!(arg_is_fix_subcommand("fix"));
        assert!(arg_is_fix_subcommand("fix --all"));
        // Not the fix subcommand: a different token that merely starts with "fix".
        assert!(!arg_is_fix_subcommand("fixme"));
        assert!(!arg_is_fix_subcommand(""));
        assert!(!arg_is_fix_subcommand("pedantic"));
        // handle_lint returns early for it — no linter process is spawned.
        assert_eq!(handle_lint("/lint fix"), None);
    }

    #[test]
    fn unknown_lint_subcommand_is_named_not_silently_defaulted() {
        // Round 40 (h5): `/lint pedntic` used to fall through to
        // LintStrictness::Default and report "Lint passed", so a typo silently
        // became a different operation. This branch returns before project
        // detection, so no linter process is spawned and the assertion holds in
        // any working directory.
        let summary =
            handle_lint("/lint pedntic").expect("an unknown subcommand is reported, not ignored");
        assert!(
            summary.contains("Unknown /lint subcommand"),
            "got: {summary}"
        );
        assert!(summary.contains("pedntic"), "got: {summary}");
        // The message lists the real table, so it cannot drift from dispatch.
        for sub in LINT_SUBCOMMANDS {
            assert!(summary.contains(sub), "{sub} missing from: {summary}");
        }
        // Documented subcommands are still routed, not refused: `fix` returns
        // early with its own message (None), never the unknown-subcommand text.
        assert_eq!(handle_lint("/lint fix"), None);
    }

    #[test]
    fn test_command_rust() {
        let cmd = test_command_for_project(&ProjectType::Rust, std::path::Path::new("."));
        assert!(cmd.is_some());
        let (label, _) = cmd.unwrap();
        assert_eq!(label, "cargo test");
    }

    #[test]
    fn test_command_unknown() {
        assert!(
            test_command_for_project(&ProjectType::Unknown, std::path::Path::new(".")).is_none()
        );
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
        let cmd = test_command_for_project(&ProjectType::Rust, std::path::Path::new("."));
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
        let cmd = test_command_for_project(&ProjectType::Node, std::path::Path::new("."));
        assert!(cmd.is_some(), "Node project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(label.contains("npm"), "Node test label should mention npm");
        assert_eq!(args[0], "npm");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_python_project() {
        let cmd = test_command_for_project(&ProjectType::Python, std::path::Path::new("."));
        assert!(cmd.is_some(), "Python project should have a test command");
        let (label, _args) = cmd.unwrap();
        assert!(
            label.contains("pytest"),
            "Python test label should mention pytest"
        );
    }

    #[test]
    fn test_test_command_for_go_project() {
        let cmd = test_command_for_project(&ProjectType::Go, std::path::Path::new("."));
        assert!(cmd.is_some(), "Go project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(label.contains("go"), "Go test label should mention go");
        assert_eq!(args[0], "go");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_make_project() {
        let cmd = test_command_for_project(&ProjectType::Make, std::path::Path::new("."));
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
        let cmd = test_command_for_project(&ProjectType::Java, std::path::Path::new("."));
        assert!(cmd.is_some(), "Java project should have a test command");
        let (label, _) = cmd.unwrap();
        // Should be either mvn or gradlew depending on pom.xml presence
        assert!(
            label.contains("mvn") || label.contains("gradlew"),
            "Java test label should mention mvn or gradlew, got: {label}"
        );
    }

    /// #746: the Java arm's Maven-vs-Gradle choice must follow the `dir` argument,
    /// not the process cwd. Two tempdirs, identical but for `pom.xml`, must give
    /// different answers — which is impossible if the lookup reads the cwd.
    #[test]
    fn java_test_command_follows_the_dir_argument_not_the_cwd() {
        let maven = tempfile::TempDir::new().unwrap();
        std::fs::write(maven.path().join("pom.xml"), "<project/>").unwrap();
        let gradle = tempfile::TempDir::new().unwrap();

        let (maven_label, maven_argv) =
            test_command_for_project(&ProjectType::Java, maven.path()).unwrap();
        let (gradle_label, gradle_argv) =
            test_command_for_project(&ProjectType::Java, gradle.path()).unwrap();

        assert_eq!(maven_label, "mvn test", "a dir holding pom.xml means Maven");
        assert_eq!(maven_argv, vec!["mvn", "test"]);
        assert_eq!(
            gradle_label, "./gradlew test",
            "a dir with no pom.xml means Gradle"
        );
        assert_eq!(gradle_argv, vec!["./gradlew", "test"]);
    }

    #[test]
    fn test_test_command_for_ruby_project() {
        let cmd = test_command_for_project(&ProjectType::Ruby, std::path::Path::new("."));
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
        let cmd = test_command_for_project(&ProjectType::Cpp, std::path::Path::new("."));
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
        let cmd = test_command_for_project(&ProjectType::Unknown, std::path::Path::new("."));
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

    // ---- security_audit_command: driven through the injected probe (#834) ----
    //
    // These eight used to call `security_audit_command`, which spawns
    // `cargo audit --version` (and three sibling probes). That made every one
    // of them depend on whichever tools this machine happens to have
    // installed, and `security_audit_command_rust` accommodated BOTH the `Ok`
    // and `Err` branches — so it asserted nothing about the Rust arm at all.
    // They now drive the pure core with the answer supplied explicitly, and
    // assert the actual command produced on BOTH sides of each discriminator.
    //
    // This was never #832's defect: a `--version` subcommand probe builds
    // nothing and cannot clobber the shared `target/debug/yoyo` uplift path.
    // It is toolchain-dependence and vacuous green.

    /// Every audit tool present.
    fn all_present(_: AuditTool) -> bool {
        true
    }

    /// No audit tool present.
    fn none_present(_: AuditTool) -> bool {
        false
    }

    #[test]
    fn security_audit_command_rust() {
        assert_eq!(
            security_audit_command_with(&ProjectType::Rust, &|t| t == AuditTool::CargoAudit),
            Ok(("cargo audit", vec!["cargo", "audit"])),
            "cargo-audit installed: /security must run `cargo audit`"
        );
        assert_eq!(
            security_audit_command_with(&ProjectType::Rust, &none_present),
            Err(Some("cargo install cargo-audit")),
            "cargo-audit missing: the hint is the install command, not a run"
        );
    }

    #[test]
    fn security_audit_command_node() {
        // npm is assumed present, so the answer must not move with the probe.
        // Near-miss guard: a discriminator that ignores its input still has to
        // be shown ignoring it in both directions.
        let expected = Ok(("npm audit", vec!["npm", "audit", "--json"]));
        assert_eq!(
            security_audit_command_with(&ProjectType::Node, &all_present),
            expected
        );
        assert_eq!(
            security_audit_command_with(&ProjectType::Node, &none_present),
            expected
        );
    }

    #[test]
    fn security_audit_command_python() {
        assert_eq!(
            security_audit_command_with(&ProjectType::Python, &|t| t == AuditTool::PipAudit),
            Ok(("pip-audit", vec!["pip-audit"]))
        );
        assert_eq!(
            security_audit_command_with(&ProjectType::Python, &|t| t == AuditTool::Safety),
            Ok(("safety check", vec!["safety", "check"])),
            "pip-audit absent, safety present: the fallback arm must fire"
        );
        // Precedence, which nothing pinned before: with both installed,
        // pip-audit wins. That is a user-visible choice, not an accident.
        assert_eq!(
            security_audit_command_with(&ProjectType::Python, &all_present),
            Ok(("pip-audit", vec!["pip-audit"]))
        );
        assert_eq!(
            security_audit_command_with(&ProjectType::Python, &none_present),
            Err(Some("pip install pip-audit"))
        );
    }

    #[test]
    fn security_audit_command_go() {
        assert_eq!(
            security_audit_command_with(&ProjectType::Go, &|t| t == AuditTool::Govulncheck),
            Ok(("govulncheck ./...", vec!["govulncheck", "./..."]))
        );
        assert_eq!(
            security_audit_command_with(&ProjectType::Go, &none_present),
            Err(Some("go install golang.org/x/vuln/cmd/govulncheck@latest"))
        );
    }

    #[test]
    fn security_audit_command_ruby() {
        assert_eq!(
            security_audit_command_with(&ProjectType::Ruby, &|t| t == AuditTool::BundleAudit),
            Ok(("bundle-audit check", vec!["bundle-audit", "check"]))
        );
        assert_eq!(
            security_audit_command_with(&ProjectType::Ruby, &none_present),
            Err(Some("gem install bundler-audit"))
        );
    }

    #[test]
    fn security_audit_command_java() {
        // Hint only, and it must not move with the probe.
        let expected: Result<(&str, Vec<&str>), Option<&str>> = Err(Some(
            "For Java, consider: mvn org.owasp:dependency-check-maven:check",
        ));
        assert_eq!(
            security_audit_command_with(&ProjectType::Java, &all_present),
            expected
        );
        assert_eq!(
            security_audit_command_with(&ProjectType::Java, &none_present),
            expected
        );
    }

    #[test]
    fn security_audit_command_unknown() {
        // `Err(None)` is "no audit tool for this project type", which is a
        // different fact from `Err(Some(hint))` = "there is one, install it".
        assert_eq!(
            security_audit_command_with(&ProjectType::Unknown, &all_present),
            Err(None)
        );
        assert_eq!(
            security_audit_command_with(&ProjectType::Unknown, &none_present),
            Err(None)
        );
    }

    #[test]
    fn security_audit_command_make_returns_none() {
        assert_eq!(
            security_audit_command_with(&ProjectType::Make, &all_present),
            Err(None)
        );
        assert_eq!(
            security_audit_command_with(&ProjectType::Make, &none_present),
            Err(None)
        );
    }

    /// The seam must not have made probing eager: a project type that needs no
    /// probe must ask for none, and Python must ask about `safety` only after
    /// `pip-audit` has said no. Spawning a subprocess nobody needed is the
    /// regression an injected resolver makes easy and invisible.
    #[test]
    fn security_audit_command_probes_lazily_and_only_what_it_needs() {
        use std::cell::RefCell;
        let seen: RefCell<Vec<AuditTool>> = RefCell::new(Vec::new());
        let absent = |t: AuditTool| {
            seen.borrow_mut().push(t);
            false
        };

        let _ = security_audit_command_with(&ProjectType::Node, &absent);
        assert!(
            seen.borrow().is_empty(),
            "Node probed {:?} — it needs no tool check at all",
            seen.borrow()
        );

        seen.borrow_mut().clear();
        let _ = security_audit_command_with(&ProjectType::Java, &absent);
        assert!(
            seen.borrow().is_empty(),
            "Java is a hint, it probes nothing"
        );

        seen.borrow_mut().clear();
        let _ = security_audit_command_with(&ProjectType::Rust, &absent);
        assert_eq!(*seen.borrow(), vec![AuditTool::CargoAudit]);

        seen.borrow_mut().clear();
        let _ = security_audit_command_with(&ProjectType::Python, &absent);
        assert_eq!(
            *seen.borrow(),
            vec![AuditTool::PipAudit, AuditTool::Safety],
            "safety must be probed only after pip-audit says no"
        );

        // The other direction: pip-audit present means safety is never asked.
        let seen2: RefCell<Vec<AuditTool>> = RefCell::new(Vec::new());
        let pip_only = |t: AuditTool| {
            seen2.borrow_mut().push(t);
            t == AuditTool::PipAudit
        };
        let _ = security_audit_command_with(&ProjectType::Python, &pip_only);
        assert_eq!(*seen2.borrow(), vec![AuditTool::PipAudit]);
    }

    /// Deliberately WEAK source-level guard, and it says so: it proves the
    /// wrapper still hands the core the real subprocess probe, never that the
    /// probe works. No behavioural test may call `security_audit_command`
    /// itself — that is the spawn this task removed from the test path — so
    /// this is the only check available on the wiring.
    ///
    /// Needles are assembled at runtime so this test cannot match its own
    /// source.
    #[test]
    fn the_audit_wrapper_still_injects_the_real_probe() {
        let src = include_str!("commands_lint.rs");
        let wrapper = format!("{}{}", "fn security_audit_", "command(");
        let core = format!("{}{}", "security_audit_command", "_with(");
        let probe = format!("{}{}", "probe_audit", "_tool");

        let start = src
            .find(&wrapper)
            .unwrap_or_else(|| panic!("wrapper {wrapper} not found — was it renamed?"));
        let body = &src[start..];
        let end = body.find("\n}\n").unwrap_or(body.len());
        let body = &body[..end];

        assert!(
            body.contains(&core),
            "the wrapper no longer delegates to the pure core"
        );
        assert!(
            body.contains(&probe),
            "the wrapper no longer injects the real probe — /security would \
             stop asking the machine anything"
        );
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
