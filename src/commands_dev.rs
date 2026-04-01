//! Dev workflow command handlers: /doctor, /health, /fix, /test, /lint, /watch, /tree, /run.

use crate::cli;
use crate::commands::auto_compact_if_needed;
use crate::commands_project::{detect_project_type, ProjectType};
use crate::format::*;
use crate::prompt::*;

use yoagent::agent::Agent;
use yoagent::*;

// ── /update ───────────────────────────────────────────────────────────────

/// Handle the /update command - download and replace the binary with latest release
pub fn handle_update() -> Result<(), String> {
    // Step 1: Check for latest version
    let latest_release = match fetch_latest_release() {
        Ok(release) => release,
        Err(e) => return Err(format!("Failed to fetch latest release: {}", e)),
    };

    let current_version = cli::VERSION;
    let tag_name = latest_release
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    if !cli::version_is_newer(tag_name, current_version) {
        println!(
            "Already on the latest version (v{}). No update needed.",
            current_version
        );
        return Ok(());
    }

    let latest_version = tag_name;
    println!(
        "Update available: v{} → {}",
        current_version, latest_version
    );

    // Step 2: Detect platform and find the right asset
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    let asset_name = match (os, arch) {
        ("linux", "x86_64") => "yoyo-x86_64-unknown-linux-gnu.tar.gz",
        ("macos", "x86_64") => "yoyo-x86_64-apple-darwin.tar.gz",
        ("macos", "aarch64") => "yoyo-aarch64-apple-darwin.tar.gz",
        ("windows", "x86_64") => "yoyo-x86_64-pc-windows-msvc.zip",
        _ => {
            return Err(format!("Unsupported platform: {} {}", os, arch));
        }
    };

    let empty_assets = Vec::new();
    let assets = latest_release
        .get("assets")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_assets);

    let download_url = match find_asset_url(assets, asset_name) {
        Some(url) => url,
        None => {
            let install_cmd = if os == "windows" {
                "irm https://raw.githubusercontent.com/yologdev/yoyo-evolve/main/install.ps1 | iex"
            } else {
                "curl -fsSL https://raw.githubusercontent.com/yologdev/yoyo-evolve/main/install.sh | bash"
            };
            return Err(format!(
                "No pre-built binary available for your platform ({} {}). Please install manually:\n  {}",
                os, arch, install_cmd
            ));
        }
    };

    // Step 3: Confirm with user
    print!("This will download and replace the current binary.\nContinue? [y/N] ");
    std::io::Write::flush(&mut std::io::stdout()).unwrap();

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("Failed to read input: {}", e))?;

    let input = input.trim().to_lowercase();
    if !matches!(input.as_str(), "y" | "yes") {
        println!("Update cancelled.");
        return Ok(());
    }

    // Step 4: Download
    let temp_path = format!(
        "/tmp/yoyo-update-{}.{}",
        latest_version,
        if asset_name.ends_with(".zip") {
            "zip"
        } else {
            "tar.gz"
        }
    );

    println!("Downloading {}...", asset_name);
    match download_file(&download_url, &temp_path) {
        Ok(_) => (),
        Err(e) => {
            let install_cmd = if os == "windows" {
                "irm https://raw.githubusercontent.com/yologdev/yoyo-evolve/main/install.ps1 | iex"
            } else {
                "curl -fsSL https://raw.githubusercontent.com/yologdev/yoyo-evolve/main/install.sh | bash"
            };
            return Err(format!(
                "Download failed: {}. Please try manual install:\n  {}",
                e, install_cmd
            ));
        }
    }

    // Step 5: Extract and replace
    let extract_dir = "/tmp/yoyo-update-dir";
    match extract_archive(&temp_path, extract_dir) {
        Ok(binary_path) => {
            // Get current executable path
            let current_exe = std::env::current_exe()
                .map_err(|e| format!("Failed to get current executable path: {}", e))?;

            // Create backup
            let backup_path = format!("{}.bak", current_exe.display());
            std::fs::copy(&current_exe, &backup_path)
                .map_err(|e| format!("Failed to create backup: {}", e))?;

            // Replace binary
            std::fs::copy(&binary_path, &current_exe)
                .map_err(|e| format!("Failed to replace binary: {}", e))?;

            // Set executable permission (Unix only)
            if os != "windows" {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&current_exe)
                    .map_err(|e| format!("Failed to get file metadata: {}", e))?
                    .permissions();
                perms.set_mode(0o755); // rwxr-xr-x
                std::fs::set_permissions(&current_exe, perms)
                    .map_err(|e| format!("Failed to set permissions: {}", e))?;
            }

            // Clean up temp files
            let _ = std::fs::remove_file(&temp_path);
            let _ = std::fs::remove_dir_all(extract_dir);

            println!(
                "✓ Updated to v{}! Please restart yoyo to use the new version.",
                latest_version
            );
            Ok(())
        }
        Err(e) => {
            // Try to restore from backup if it exists
            let current_exe = match std::env::current_exe() {
                Ok(exe) => exe,
                Err(_) => {
                    return Err(format!(
                        "Failed to extract and failed to get current executable: {}",
                        e
                    ))
                }
            };
            let backup_path = format!("{}.bak", current_exe.display());
            if std::path::Path::new(&backup_path).exists() {
                let _ = std::fs::copy(&backup_path, &current_exe);
                let _ = std::fs::remove_file(&backup_path);
            }
            Err(format!("Failed to extract archive: {}", e))
        }
    }
}

/// Fetch the latest release from GitHub API
fn fetch_latest_release() -> Result<serde_json::Value, String> {
    let output = std::process::Command::new("curl")
        .args([
            "-sf",
            "https://api.github.com/repos/yologdev/yoyo-evolve/releases/latest",
        ])
        .output()
        .map_err(|e| format!("Failed to run curl: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "GitHub API request failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let response = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&response).map_err(|e| format!("Failed to parse JSON response: {}", e))
}

/// Find the download URL for a specific asset
fn find_asset_url(assets: &[serde_json::Value], asset_name: &str) -> Option<String> {
    assets
        .iter()
        .find(|asset| {
            asset
                .get("name")
                .and_then(|name| name.as_str())
                .map(|name| name == asset_name)
                .unwrap_or(false)
        })
        .and_then(|asset| asset.get("browser_download_url"))
        .and_then(|url| url.as_str())
        .map(|url| url.to_string())
}

/// Download a file from URL to a path
fn download_file(url: &str, path: &str) -> Result<(), String> {
    std::process::Command::new("curl")
        .args(["-fSL", "-o", path, url])
        .output()
        .map_err(|e| format!("Failed to run curl: {}", e))?
        .status
        .success()
        .then_some(())
        .ok_or_else(|| "Download failed".to_string())
}

/// Extract an archive and return the path to the extracted binary
fn extract_archive(archive_path: &str, extract_dir: &str) -> Result<String, String> {
    // Create extract directory
    std::fs::create_dir_all(extract_dir)
        .map_err(|e| format!("Failed to create extract directory: {}", e))?;

    if archive_path.ends_with(".tar.gz") {
        // Extract tar.gz
        std::process::Command::new("tar")
            .args(["xzf", archive_path, "-C", extract_dir])
            .output()
            .map_err(|e| format!("Failed to extract tar.gz: {}", e))?
            .status
            .success()
            .then_some(())
            .ok_or_else(|| "Failed to extract tar.gz".to_string())?;
    } else if archive_path.ends_with(".zip") {
        // Extract zip
        std::process::Command::new("unzip")
            .args([archive_path, "-d", extract_dir])
            .output()
            .map_err(|e| format!("Failed to extract zip: {}", e))?
            .status
            .success()
            .then_some(())
            .ok_or_else(|| "Failed to extract zip".to_string())?;
    } else {
        return Err("Unsupported archive format".to_string());
    }

    // Find the yoyo binary in the extracted directory
    let entries = std::fs::read_dir(extract_dir)
        .map_err(|e| format!("Failed to read extract directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|name| name.to_str()) {
                if filename == "yoyo" {
                    return Ok(path.to_string_lossy().to_string());
                }
            }
        }
    }

    // If not found at root, check subdirectories (common for tar.gz structure)
    let entries = std::fs::read_dir(extract_dir)
        .map_err(|e| format!("Failed to read extract directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.is_dir() {
            let binary_path = path.join("yoyo");
            if binary_path.exists() {
                return Ok(binary_path.to_string_lossy().to_string());
            }
        }
    }

    Err("Could not find yoyo binary in extracted archive".to_string())
}

// ── /doctor ──────────────────────────────────────────────────────────────

/// Status of a single doctor check.
#[derive(Debug, Clone, PartialEq)]
pub enum DoctorStatus {
    Pass,
    Fail,
    Warn,
}

/// A single diagnostic check result from `/doctor`.
#[derive(Debug, Clone)]
pub struct DoctorCheck {
    pub name: String,
    pub status: DoctorStatus,
    pub detail: String,
}

/// Run all environment diagnostic checks and return structured results.
///
/// This is separated from the display logic so it can be tested.
pub fn run_doctor_checks(provider: &str, model: &str) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();

    // 1. Version
    checks.push(DoctorCheck {
        name: "Version".to_string(),
        status: DoctorStatus::Pass,
        detail: cli::VERSION.to_string(),
    });

    // 2. Git installed
    match std::process::Command::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let ver = String::from_utf8_lossy(&output.stdout)
                .trim()
                .replace("git version ", "")
                .to_string();
            checks.push(DoctorCheck {
                name: "Git".to_string(),
                status: DoctorStatus::Pass,
                detail: format!("installed ({ver})"),
            });
        }
        _ => {
            checks.push(DoctorCheck {
                name: "Git".to_string(),
                status: DoctorStatus::Fail,
                detail: "not found".to_string(),
            });
        }
    }

    // 3. Git repo
    match std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let branch = std::process::Command::new("git")
                .args(["branch", "--show-current"])
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        let b = String::from_utf8_lossy(&o.stdout).trim().to_string();
                        if b.is_empty() {
                            None
                        } else {
                            Some(b)
                        }
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "detached".to_string());
            checks.push(DoctorCheck {
                name: "Git repo".to_string(),
                status: DoctorStatus::Pass,
                detail: format!("yes (branch: {branch})"),
            });
        }
        _ => {
            checks.push(DoctorCheck {
                name: "Git repo".to_string(),
                status: DoctorStatus::Warn,
                detail: "not inside a git repository".to_string(),
            });
        }
    }

    // 4. Provider
    checks.push(DoctorCheck {
        name: "Provider".to_string(),
        status: DoctorStatus::Pass,
        detail: provider.to_string(),
    });

    // 5. API key
    let env_var = cli::provider_api_key_env(provider);
    match env_var {
        Some(var_name) => {
            if std::env::var(var_name).is_ok() {
                checks.push(DoctorCheck {
                    name: "API key".to_string(),
                    status: DoctorStatus::Pass,
                    detail: format!("set ({var_name})"),
                });
            } else {
                checks.push(DoctorCheck {
                    name: "API key".to_string(),
                    status: DoctorStatus::Fail,
                    detail: format!("{var_name} not set"),
                });
            }
        }
        None => {
            // Unknown provider — can't check env var
            if provider == "ollama" {
                checks.push(DoctorCheck {
                    name: "API key".to_string(),
                    status: DoctorStatus::Pass,
                    detail: "not required (ollama)".to_string(),
                });
            } else {
                checks.push(DoctorCheck {
                    name: "API key".to_string(),
                    status: DoctorStatus::Warn,
                    detail: format!("unknown env var for provider '{provider}'"),
                });
            }
        }
    }

    // 6. Model
    checks.push(DoctorCheck {
        name: "Model".to_string(),
        status: DoctorStatus::Pass,
        detail: model.to_string(),
    });

    // 7. Config file
    let mut config_found = Vec::new();
    if std::path::Path::new(".yoyo.toml").exists() {
        config_found.push(".yoyo.toml");
    }
    if let Some(user_path) = cli::user_config_path() {
        if user_path.exists() {
            config_found.push("~/.config/yoyo/config.toml");
        }
    }
    if config_found.is_empty() {
        checks.push(DoctorCheck {
            name: "Config file".to_string(),
            status: DoctorStatus::Warn,
            detail: "none found (.yoyo.toml or ~/.config/yoyo/config.toml)".to_string(),
        });
    } else {
        checks.push(DoctorCheck {
            name: "Config file".to_string(),
            status: DoctorStatus::Pass,
            detail: format!("found: {}", config_found.join(", ")),
        });
    }

    // 8. Project context
    let context_files = cli::list_project_context_files();
    if context_files.is_empty() {
        checks.push(DoctorCheck {
            name: "Project context".to_string(),
            status: DoctorStatus::Warn,
            detail: "no context file (create YOYO.md or run /init)".to_string(),
        });
    } else {
        let descriptions: Vec<String> = context_files
            .iter()
            .map(|(name, lines)| format!("{name} ({lines} lines)"))
            .collect();
        checks.push(DoctorCheck {
            name: "Project context".to_string(),
            status: DoctorStatus::Pass,
            detail: descriptions.join(", "),
        });
    }

    // 9. Curl
    match std::process::Command::new("curl").arg("--version").output() {
        Ok(output) if output.status.success() => {
            checks.push(DoctorCheck {
                name: "Curl".to_string(),
                status: DoctorStatus::Pass,
                detail: "installed (for /docs and /web)".to_string(),
            });
        }
        _ => {
            checks.push(DoctorCheck {
                name: "Curl".to_string(),
                status: DoctorStatus::Warn,
                detail: "not found (/docs and /web won't work)".to_string(),
            });
        }
    }

    // 10. Memory dir (.yoyo/)
    if std::path::Path::new(".yoyo").is_dir() {
        checks.push(DoctorCheck {
            name: "Memory dir".to_string(),
            status: DoctorStatus::Pass,
            detail: ".yoyo/ found".to_string(),
        });
    } else {
        checks.push(DoctorCheck {
            name: "Memory dir".to_string(),
            status: DoctorStatus::Warn,
            detail: ".yoyo/ not found (run /remember to create)".to_string(),
        });
    }

    checks
}

/// Display the doctor report from a list of checks.
pub fn print_doctor_report(checks: &[DoctorCheck]) {
    println!("\n  {BOLD}🩺 yoyo doctor{RESET}");
    println!("  {DIM}─────────────────────────────{RESET}");

    for check in checks {
        let (icon, color) = match check.status {
            DoctorStatus::Pass => ("✓", &GREEN),
            DoctorStatus::Fail => ("✗", &RED),
            DoctorStatus::Warn => ("⚠", &YELLOW),
        };
        println!(
            "  {color}{icon}{RESET} {BOLD}{}{RESET}: {}",
            check.name, check.detail
        );
    }

    let passed = checks
        .iter()
        .filter(|c| c.status == DoctorStatus::Pass)
        .count();
    let total = checks.len();
    let summary_color = if passed == total { &GREEN } else { &YELLOW };
    println!("\n  {summary_color}{passed}/{total} checks passed{RESET}\n");
}

/// Handle the `/doctor` command.
pub fn handle_doctor(provider: &str, model: &str) {
    let checks = run_doctor_checks(provider, model);
    print_doctor_report(&checks);
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
            #[allow(unused_mut)]
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

// ── /watch ──────────────────────────────────────────────────────────────

/// Auto-detect the test command for the current project.
/// Returns the command string (e.g. "cargo test") if a project type is detected.
pub fn detect_test_command() -> Option<String> {
    let dir = std::env::current_dir().unwrap_or_default();
    let project_type = detect_project_type(&dir);
    test_command_for_project(&project_type).map(|(label, _args)| label.to_string())
}

/// Watch subcommand names for tab completion.
pub const WATCH_SUBCOMMANDS: &[&str] = &["off", "status"];

/// Handle the /watch command: toggle auto-test-on-edit mode.
pub fn handle_watch(input: &str) {
    let arg = input.strip_prefix("/watch").unwrap_or("").trim();

    match arg {
        "" => {
            // Auto-detect and toggle on
            match detect_test_command() {
                Some(cmd) => {
                    crate::prompt::set_watch_command(&cmd);
                    println!(
                        "{GREEN}  👀 Watch mode ON — will run `{cmd}` after agent edits{RESET}\n"
                    );
                }
                None => {
                    println!("{DIM}  No test command detected. Specify one:{RESET}");
                    println!("{DIM}    /watch cargo test{RESET}");
                    println!("{DIM}    /watch npm test{RESET}\n");
                }
            }
        }
        "off" => {
            crate::prompt::clear_watch_command();
            println!("{DIM}  👀 Watch mode OFF{RESET}\n");
        }
        "status" => match crate::prompt::get_watch_command() {
            Some(cmd) => {
                println!("{DIM}  👀 Watch mode: ON{RESET}");
                println!("{DIM}  Command: `{cmd}`{RESET}\n");
            }
            None => {
                println!("{DIM}  👀 Watch mode: OFF{RESET}\n");
            }
        },
        custom_cmd => {
            crate::prompt::set_watch_command(custom_cmd);
            println!(
                "{GREEN}  👀 Watch mode ON — will run `{custom_cmd}` after agent edits{RESET}\n"
            );
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
