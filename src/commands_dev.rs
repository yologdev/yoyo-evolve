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
    // Check if running from cargo (development mode)
    if is_cargo_dev_build() {
        println!(
            "{}You're running a development build. Use `cargo install yoyo-agent` to update, \
             or build from source with `cargo build --release`.{}",
            YELLOW, RESET
        );
        return Ok(());
    }

    // Step 1: Check for latest version
    let latest_release = match fetch_latest_release() {
        Ok(release) => release,
        Err(e) => {
            let install_cmd = if std::env::consts::OS == "windows" {
                "irm https://raw.githubusercontent.com/yologdev/yoyo-evolve/main/install.ps1 | iex"
            } else {
                "curl -fsSL https://raw.githubusercontent.com/yologdev/yoyo-evolve/main/install.sh | bash"
            };
            return Err(format!(
                "Failed to check for updates: {}. Try manual install:\n  {}",
                e, install_cmd
            ));
        }
    };

    let current_version = cli::VERSION;
    let tag_name = latest_release
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // version_is_newer(current, latest) — current is our version, latest is the tag
    let tag_version = tag_name.strip_prefix('v').unwrap_or(tag_name);
    if !crate::update::version_is_newer(current_version, tag_version) {
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
    let asset_name = match platform_asset_name(os, arch) {
        Some(name) => name,
        None => {
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
    let _ = std::io::Write::flush(&mut std::io::stdout());

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
            #[cfg(unix)]
            {
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

/// Map OS/ARCH to the expected GitHub release asset name.
/// Returns None for unsupported platforms.
fn platform_asset_name(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("yoyo-x86_64-unknown-linux-gnu.tar.gz"),
        ("macos", "x86_64") => Some("yoyo-x86_64-apple-darwin.tar.gz"),
        ("macos", "aarch64") => Some("yoyo-aarch64-apple-darwin.tar.gz"),
        ("windows", "x86_64") => Some("yoyo-x86_64-pc-windows-msvc.zip"),
        _ => None,
    }
}

/// Check if we're running from a cargo build directory (development mode).
fn is_cargo_dev_build() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .map(|p| {
            p.contains("/target/debug/")
                || p.contains("/target/release/")
                || p.contains("\\target\\debug\\")
                || p.contains("\\target\\release\\")
        })
        .unwrap_or(false)
}

/// Fetch the latest release from GitHub API
fn fetch_latest_release() -> Result<serde_json::Value, String> {
    let output = std::process::Command::new("curl")
        .args([
            "-sf",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
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

    // 11. RTK (Rust Token Killer) — optional tool output compression
    {
        let rtk_available = crate::tools::detect_rtk();
        let rtk_disabled = crate::tools::is_rtk_disabled();
        if rtk_available && !rtk_disabled {
            checks.push(DoctorCheck {
                name: "RTK".to_string(),
                status: DoctorStatus::Pass,
                detail: "installed (auto-compressing tool output)".to_string(),
            });
        } else if rtk_available && rtk_disabled {
            checks.push(DoctorCheck {
                name: "RTK".to_string(),
                status: DoctorStatus::Warn,
                detail: "installed but disabled (--no-rtk flag)".to_string(),
            });
        } else {
            checks.push(DoctorCheck {
                name: "RTK".to_string(),
                status: DoctorStatus::Pass,
                detail: "not installed (optional — compresses build output)".to_string(),
            });
        }
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
            // In test builds the push is cfg-gated out, leaving `checks`
            // effectively immutable — but mut is required for production.
            #[cfg(not(test))]
            {
                vec![("test", vec!["make", "test"])]
            }
            #[cfg(test)]
            {
                vec![]
            }
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
        ProjectType::Make | ProjectType::Unknown => None,
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

// ── /watch ──────────────────────────────────────────────────────────────

/// Auto-detect the test command for the current project.
/// Returns the command string (e.g. "cargo test") if a project type is detected.
#[allow(dead_code)]
fn detect_test_command() -> Option<String> {
    let dir = std::env::current_dir().unwrap_or_default();
    let project_type = detect_project_type(&dir);
    test_command_for_project(&project_type).map(|(label, _args)| label.to_string())
}

/// Auto-detect the appropriate watch command for the current project.
/// Returns a lint+test combo command (e.g. `cargo clippy … && cargo test`) when
/// both are available, falls back to test-only, or `None` for unknown project types.
pub fn auto_detect_watch_command() -> Option<String> {
    detect_watch_all_command()
}

/// Auto-detect a combined lint + test command for the current project.
/// Returns both commands chained with `&&` so the first failure stops execution.
/// Falls back to just the test command if no lint command is available,
/// or `None` if neither can be detected.
pub fn detect_watch_all_command() -> Option<String> {
    let dir = std::env::current_dir().unwrap_or_default();
    let project_type = detect_project_type(&dir);
    let lint = lint_command_for_project(&project_type, LintStrictness::Default);
    let test = test_command_for_project(&project_type);
    match (lint, test) {
        (Some((lint_label, _)), Some((test_label, _))) => {
            Some(format!("{lint_label} && {test_label}"))
        }
        (None, Some((test_label, _))) => Some(test_label.to_string()),
        (Some((lint_label, _)), None) => Some(lint_label),
        (None, None) => None,
    }
}

/// Watch subcommand names for tab completion.
pub const WATCH_SUBCOMMANDS: &[&str] = &["off", "status", "all", "lint"];

/// Handle the /watch command: toggle auto-test-on-edit mode.
pub fn handle_watch(input: &str) {
    let arg = input.strip_prefix("/watch").unwrap_or("").trim();

    match arg {
        "" => {
            // Auto-detect lint+test combo and toggle on
            match detect_watch_all_command() {
                Some(cmd) => {
                    crate::prompt::set_watch_command(&cmd);
                    println!(
                        "{GREEN}  👀 Watch mode ON — will run `{cmd}` after agent edits{RESET}\n"
                    );
                }
                None => {
                    println!("{DIM}  No lint or test command detected. Specify one:{RESET}");
                    println!("{DIM}    /watch cargo clippy && cargo test{RESET}");
                    println!("{DIM}    /watch npm run lint && npm test{RESET}\n");
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
        "all" => {
            // Auto-detect lint + test and chain them
            match detect_watch_all_command() {
                Some(cmd) => {
                    crate::prompt::set_watch_command(&cmd);
                    println!(
                        "{GREEN}  👀 Watch mode ON — will run `{cmd}` after agent edits{RESET}\n"
                    );
                }
                None => {
                    println!("{DIM}  No lint or test command detected. Specify one:{RESET}");
                    println!("{DIM}    /watch cargo clippy && cargo test{RESET}");
                    println!("{DIM}    /watch npm run lint && npm test{RESET}\n");
                }
            }
        }
        "lint" => {
            // Auto-detect lint-only command
            let dir = std::env::current_dir().unwrap_or_default();
            let project_type = detect_project_type(&dir);
            match lint_command_for_project(&project_type, LintStrictness::Default) {
                Some((lint_label, _)) => {
                    crate::prompt::set_watch_command(&lint_label);
                    println!("{GREEN}  👀 Watch set to: {lint_label}{RESET}\n");
                }
                None => {
                    println!("{DIM}  No lint command detected for this project type.{RESET}");
                    println!("{DIM}    /watch cargo clippy{RESET}");
                    println!("{DIM}    /watch npx eslint .{RESET}\n");
                }
            }
        }
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
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let start = std::time::Instant::now();
    let child = Command::new("sh")
        .args(["-c", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{RED}  error running command: {e}{RESET}\n");
            return;
        }
    };

    // Read stderr in a background thread so we don't block on either pipe
    let stderr_pipe = child.stderr.take().expect("stderr was piped");
    let stderr_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stderr_pipe);
        for line in reader.lines() {
            match line {
                Ok(l) => eprintln!("{RED}{l}{RESET}"),
                Err(_) => break,
            }
        }
    });

    // Stream stdout line-by-line on the main thread
    if let Some(stdout_pipe) = child.stdout.take() {
        let reader = BufReader::new(stdout_pipe);
        for line in reader.lines() {
            match line {
                Ok(l) => println!("{l}"),
                Err(_) => break,
            }
        }
    }

    // Wait for stderr thread to finish
    let _ = stderr_handle.join();

    // Collect exit status
    let elapsed = format_duration(start.elapsed());
    match child.wait() {
        Ok(status) => {
            let code = status.code().unwrap_or(-1);
            if code == 0 {
                println!("{DIM}  ✓ exit {code} ({elapsed}){RESET}\n");
            } else {
                println!("{RED}  ✗ exit {code} ({elapsed}){RESET}\n");
            }
        }
        Err(e) => {
            eprintln!("{RED}  error waiting for command: {e}{RESET}\n");
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
    use crate::commands::{is_unknown_command, KNOWN_COMMANDS};

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

    #[test]
    fn auto_detect_watch_command_returns_lint_and_test_in_rust_project() {
        // We're running from a directory with Cargo.toml, so this should detect Rust
        // After the Day 58 change, auto-detect defaults to lint+test (not test-only)
        let cmd = auto_detect_watch_command();
        assert!(
            cmd.is_some(),
            "should detect a watch command in a Rust project"
        );
        let cmd = cmd.unwrap();
        assert!(
            cmd.contains("clippy"),
            "auto-detect should include lint (clippy): {cmd}"
        );
        assert!(
            cmd.contains("cargo test"),
            "auto-detect should include test: {cmd}"
        );
        assert!(
            cmd.contains("&&"),
            "auto-detect should chain lint && test: {cmd}"
        );
    }

    #[test]
    fn detect_watch_all_command_returns_lint_and_test_for_rust() {
        // We're running from a directory with Cargo.toml, so this should detect Rust
        let cmd = detect_watch_all_command();
        assert!(
            cmd.is_some(),
            "should detect a combined command in a Rust project"
        );
        let cmd = cmd.unwrap();
        assert!(
            cmd.contains("clippy"),
            "combined command should include lint (clippy): {cmd}"
        );
        assert!(
            cmd.contains("cargo test"),
            "combined command should include test: {cmd}"
        );
        assert!(
            cmd.contains("&&"),
            "combined command should chain with &&: {cmd}"
        );
    }

    #[test]
    fn watch_subcommands_includes_all() {
        assert!(
            WATCH_SUBCOMMANDS.contains(&"all"),
            "WATCH_SUBCOMMANDS should include 'all'"
        );
    }

    #[test]
    fn handle_watch_all_sets_combined_command() {
        // Clear any previous watch command
        crate::prompt::clear_watch_command();
        // Run /watch all — since we're in a Rust project, it should set a combined command
        handle_watch("/watch all");
        let cmd = crate::prompt::get_watch_command();
        assert!(
            cmd.is_some(),
            "watch command should be set after /watch all"
        );
        let cmd = cmd.unwrap();
        assert!(
            cmd.contains("clippy") && cmd.contains("cargo test"),
            "watch all should set lint && test: {cmd}"
        );
        // Cleanup
        crate::prompt::clear_watch_command();
    }

    #[test]
    fn watch_subcommands_includes_lint() {
        assert!(
            WATCH_SUBCOMMANDS.contains(&"lint"),
            "WATCH_SUBCOMMANDS should include 'lint'"
        );
    }

    #[test]
    fn handle_watch_lint_sets_lint_only_command() {
        // Clear any previous watch command
        crate::prompt::clear_watch_command();
        // Run /watch lint — since we're in a Rust project, it should set clippy only
        handle_watch("/watch lint");
        let cmd = crate::prompt::get_watch_command();
        assert!(
            cmd.is_some(),
            "watch command should be set after /watch lint"
        );
        let cmd = cmd.unwrap();
        assert!(
            cmd.contains("clippy"),
            "watch lint should set lint command: {cmd}"
        );
        assert!(
            !cmd.contains("cargo test"),
            "watch lint should NOT include test: {cmd}"
        );
        // Cleanup
        crate::prompt::clear_watch_command();
    }

    #[test]
    fn handle_watch_bare_sets_lint_and_test() {
        // Clear any previous watch command
        crate::prompt::clear_watch_command();
        // Run bare /watch — should now set lint+test, not just test
        handle_watch("/watch");
        let cmd = crate::prompt::get_watch_command();
        assert!(
            cmd.is_some(),
            "watch command should be set after bare /watch"
        );
        let cmd = cmd.unwrap();
        assert!(
            cmd.contains("clippy") && cmd.contains("cargo test"),
            "bare /watch should set lint && test: {cmd}"
        );
        // Cleanup
        crate::prompt::clear_watch_command();
    }

    // ── lint_command_for_project ─────────────────────────────────────

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

    #[test]
    fn doctor_checks_include_rtk() {
        let checks = run_doctor_checks("anthropic", "test-model");
        assert!(
            checks.iter().any(|c| c.name == "RTK"),
            "doctor checks should include an RTK entry"
        );
        // RTK check should always be Pass (never Fail), since it's optional
        let rtk_check = checks.iter().find(|c| c.name == "RTK").unwrap();
        assert_ne!(
            rtk_check.status,
            DoctorStatus::Fail,
            "RTK should never be Fail — it's optional"
        );
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

    // ── build_lint_fix_prompt ──────────────────────────────────────────

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

    // ── update helpers ────────────────────────────────────────────────

    #[test]
    fn update_platform_linux_x86_64() {
        let name = platform_asset_name("linux", "x86_64");
        assert_eq!(name, Some("yoyo-x86_64-unknown-linux-gnu.tar.gz"));
    }

    #[test]
    fn update_platform_macos_intel() {
        let name = platform_asset_name("macos", "x86_64");
        assert_eq!(name, Some("yoyo-x86_64-apple-darwin.tar.gz"));
    }

    #[test]
    fn update_platform_macos_arm() {
        let name = platform_asset_name("macos", "aarch64");
        assert_eq!(name, Some("yoyo-aarch64-apple-darwin.tar.gz"));
    }

    #[test]
    fn update_platform_windows() {
        let name = platform_asset_name("windows", "x86_64");
        assert_eq!(name, Some("yoyo-x86_64-pc-windows-msvc.zip"));
    }

    #[test]
    fn update_platform_unsupported() {
        assert!(platform_asset_name("freebsd", "x86_64").is_none());
        assert!(platform_asset_name("linux", "arm").is_none());
        assert!(platform_asset_name("windows", "aarch64").is_none());
    }

    #[test]
    fn update_find_asset_url_found() {
        let assets = vec![
            serde_json::json!({
                "name": "yoyo-x86_64-unknown-linux-gnu.tar.gz",
                "browser_download_url": "https://example.com/download/linux.tar.gz"
            }),
            serde_json::json!({
                "name": "yoyo-aarch64-apple-darwin.tar.gz",
                "browser_download_url": "https://example.com/download/macos-arm.tar.gz"
            }),
        ];
        let url = find_asset_url(&assets, "yoyo-x86_64-unknown-linux-gnu.tar.gz");
        assert_eq!(
            url,
            Some("https://example.com/download/linux.tar.gz".to_string())
        );
    }

    #[test]
    fn update_find_asset_url_not_found() {
        let assets = vec![serde_json::json!({
            "name": "yoyo-x86_64-unknown-linux-gnu.tar.gz",
            "browser_download_url": "https://example.com/download/linux.tar.gz"
        })];
        let url = find_asset_url(&assets, "yoyo-x86_64-pc-windows-msvc.zip");
        assert!(url.is_none());
    }

    #[test]
    fn update_find_asset_url_empty() {
        let assets: Vec<serde_json::Value> = vec![];
        let url = find_asset_url(&assets, "yoyo-x86_64-unknown-linux-gnu.tar.gz");
        assert!(url.is_none());
    }

    #[test]
    fn update_version_comparison() {
        // Sanity check version_is_newer works as expected for our use case
        assert!(crate::update::version_is_newer("0.1.5", "0.2.0"));
        assert!(!crate::update::version_is_newer("0.2.0", "0.2.0"));
        assert!(!crate::update::version_is_newer("0.3.0", "0.2.0"));
    }

    #[test]
    fn update_is_cargo_dev_build_runs() {
        // Just ensure the function runs without panicking
        // In test context, we're running from target/debug so should return true
        let result = is_cargo_dev_build();
        assert!(
            result,
            "tests run from target/debug, should detect as dev build"
        );
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

    // ── moved from commands.rs (issue #260) ────────────────────────

    #[test]
    fn test_health_check_function() {
        // run_health_check_for_project skips "cargo test" under #[cfg(test)] to avoid recursion
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);
        let results = run_health_check_for_project(&project_type);
        assert!(
            !results.is_empty(),
            "Health check should return at least one result"
        );
        for (name, passed, _) in &results {
            assert!(!name.is_empty(), "Check name should not be empty");
            if *name == "build" {
                assert!(passed, "cargo build should pass in test environment");
            }
        }
        // "test" check should be excluded under cfg(test)
        assert!(
            !results.iter().any(|(name, _, _)| *name == "test"),
            "cargo test check should be skipped to avoid recursion"
        );
    }

    #[test]
    fn test_health_checks_for_rust_project() {
        let checks = health_checks_for_project(&ProjectType::Rust);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Rust should have build check");
        assert!(names.contains(&"clippy"), "Rust should have clippy check");
        assert!(names.contains(&"fmt"), "Rust should have fmt check");
        // test is excluded under cfg(test)
        assert!(
            !names.contains(&"test"),
            "test should be excluded in cfg(test)"
        );
    }

    #[test]
    fn test_health_checks_for_node_project() {
        let checks = health_checks_for_project(&ProjectType::Node);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"lint"), "Node should have lint check");
    }

    #[test]
    fn test_health_checks_for_go_project() {
        let checks = health_checks_for_project(&ProjectType::Go);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Go should have build check");
        assert!(names.contains(&"vet"), "Go should have vet check");
    }

    #[test]
    fn test_health_checks_for_python_project() {
        let checks = health_checks_for_project(&ProjectType::Python);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"lint"), "Python should have lint check");
        assert!(names.contains(&"typecheck"), "Python should have typecheck");
    }

    #[test]
    fn test_health_checks_for_unknown_returns_empty() {
        let checks = health_checks_for_project(&ProjectType::Unknown);
        assert!(checks.is_empty(), "Unknown project should return no checks");
    }

    #[test]
    fn test_run_command_recognized() {
        assert!(!is_unknown_command("/run"));
        assert!(!is_unknown_command("/run echo hello"));
        assert!(!is_unknown_command("/run ls -la"));
    }

    #[test]
    fn test_run_shell_command_basic() {
        // Verify run_shell_command doesn't panic on basic commands
        // (output streams to stdout/stderr line-by-line)
        run_shell_command("echo hello");
    }

    #[test]
    fn test_run_shell_command_failing() {
        // Non-zero exit should not panic
        run_shell_command("false");
    }

    #[test]
    fn test_run_shell_command_streams_multiline() {
        // Multi-line output should stream without panic
        run_shell_command("echo line1; echo line2; echo line3");
    }

    #[test]
    fn test_run_shell_command_mixed_stdout_stderr() {
        // Both stdout and stderr should be handled without deadlock or panic
        run_shell_command("echo out; echo err >&2; echo out2");
    }

    #[test]
    fn test_run_shell_command_large_output() {
        // Ensure streaming handles larger output without buffering issues
        run_shell_command("seq 1 100");
    }

    #[test]
    fn test_bang_shortcut_matching() {
        // ! prefix should match for /run shortcut
        let bang_matches = |s: &str| s.starts_with('!') && s.len() > 1;
        assert!(bang_matches("!ls"));
        assert!(bang_matches("!echo hello"));
        assert!(bang_matches("! ls")); // space after bang is fine
        assert!(!bang_matches("!")); // bare bang alone should not match
    }

    #[test]
    fn test_run_command_matching() {
        // /run should only match /run or /run <cmd>, not /running
        let run_matches = |s: &str| s == "/run" || s.starts_with("/run ");
        assert!(run_matches("/run"));
        assert!(run_matches("/run echo hello"));
        assert!(!run_matches("/running"));
        assert!(!run_matches("/runaway"));
    }

    #[test]
    fn test_format_tree_from_paths_basic() {
        let paths = vec![
            "Cargo.toml".to_string(),
            "README.md".to_string(),
            "src/cli.rs".to_string(),
            "src/format.rs".to_string(),
            "src/main.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("Cargo.toml"));
        assert!(tree.contains("README.md"));
        assert!(tree.contains("src/"));
        assert!(tree.contains("  main.rs"));
        assert!(tree.contains("  cli.rs"));
    }

    #[test]
    fn test_format_tree_from_paths_nested() {
        let paths = vec![
            "src/main.rs".to_string(),
            "src/utils/helpers.rs".to_string(),
            "src/utils/format.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("src/"));
        assert!(tree.contains("  utils/"));
        assert!(tree.contains("    helpers.rs"));
        assert!(tree.contains("    format.rs"));
    }

    #[test]
    fn test_format_tree_from_paths_depth_limit() {
        let paths = vec![
            "a/b/c/d/deep.txt".to_string(),
            "a/shallow.txt".to_string(),
            "top.txt".to_string(),
        ];
        // depth 1: show dirs at level 0 ('a/'), files at depth ≤ 1
        let tree = format_tree_from_paths(&paths, 1);
        assert!(tree.contains("top.txt"));
        assert!(tree.contains("a/"));
        assert!(tree.contains("  shallow.txt"));
        // Files deeper than max_depth should not appear
        assert!(!tree.contains("deep.txt"));
        // Directory 'b/' is at level 1, beyond max_depth=1 for dirs
        assert!(!tree.contains("b/"));
    }

    #[test]
    fn test_format_tree_from_paths_empty() {
        let paths: Vec<String> = vec![];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_format_tree_from_paths_root_files_only() {
        let paths = vec![
            "Cargo.lock".to_string(),
            "Cargo.toml".to_string(),
            "README.md".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        // No directories, just root files
        assert!(!tree.contains('/'));
        assert!(tree.contains("Cargo.lock"));
        assert!(tree.contains("Cargo.toml"));
        assert!(tree.contains("README.md"));
    }

    #[test]
    fn test_format_tree_from_paths_depth_zero() {
        let paths = vec!["README.md".to_string(), "src/main.rs".to_string()];
        let tree = format_tree_from_paths(&paths, 0);
        // Depth 0: only root-level files shown
        assert!(tree.contains("README.md"));
        // main.rs is at depth 1, should not show at depth 0
        assert!(!tree.contains("main.rs"));
    }

    #[test]
    fn test_format_tree_dir_printed_once() {
        let paths = vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        // "src/" should appear exactly once
        assert_eq!(tree.matches("src/").count(), 1);
    }

    #[test]
    fn test_build_project_tree_runs() {
        // build_project_tree should return something non-empty
        let tree = build_project_tree(3);
        assert!(!tree.is_empty());
        // In a git repo, should contain Cargo.toml; outside one (e.g. cargo-mutants
        // temp dir) the tree still works but uses filesystem walk instead of git ls-files
    }

    #[test]
    fn test_tree_command_recognized() {
        assert!(!is_unknown_command("/tree"));
        assert!(!is_unknown_command("/tree 2"));
        assert!(!is_unknown_command("/tree 5"));
    }

    #[test]
    fn test_fix_command_recognized() {
        assert!(!is_unknown_command("/fix"));
        assert!(
            KNOWN_COMMANDS.contains(&"/fix"),
            "/fix should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_run_health_checks_full_output_returns_results() {
        // In a Rust project, should return results with full error output
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);
        let results = run_health_checks_full_output(&project_type);
        assert!(
            !results.is_empty(),
            "Should return at least one check result"
        );
        for (name, passed, _output) in &results {
            assert!(!name.is_empty(), "Check name should not be empty");
            if *name == "build" {
                assert!(passed, "cargo build should pass in test environment");
            }
        }
    }

    #[test]
    fn test_build_fix_prompt_with_failures() {
        let failures = vec![
            (
                "build",
                "error[E0308]: mismatched types\n  --> src/main.rs:42",
            ),
            (
                "clippy",
                "warning: unused variable `x`\n  --> src/lib.rs:10",
            ),
        ];
        let prompt = build_fix_prompt(&failures);
        assert!(prompt.contains("build"), "Prompt should mention build");
        assert!(prompt.contains("clippy"), "Prompt should mention clippy");
        assert!(
            prompt.contains("error[E0308]"),
            "Prompt should include build error"
        );
        assert!(
            prompt.contains("unused variable"),
            "Prompt should include clippy warning"
        );
    }

    #[test]
    fn test_build_fix_prompt_empty_failures() {
        let failures: Vec<(&str, &str)> = vec![];
        let prompt = build_fix_prompt(&failures);
        assert!(
            prompt.is_empty() || prompt.contains("Fix"),
            "Empty failures should produce empty or minimal prompt"
        );
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

    // ── lint strictness levels ──────────────────────────────────────────

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

    // ── scan_for_unsafe ────────────────────────────────────────────────

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

    // ── has_unsafe_code_attribute ──────────────────────────────────────

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
}
