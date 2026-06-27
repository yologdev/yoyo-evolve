//! Self-update command handler: /update.

use crate::cli;
use crate::format::*;

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
                if let Err(restore_err) = std::fs::copy(&backup_path, &current_exe) {
                    eprintln!(
                        "  ⚠ CRITICAL: failed to restore backup after update failure: {}",
                        restore_err
                    );
                    eprintln!(
                        "    Backup is at: {} — manually copy it to restore",
                        backup_path
                    );
                } else {
                    // Backup restored successfully, clean it up
                    let _ = std::fs::remove_file(&backup_path);
                }
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

#[cfg(test)]
mod tests {
    use super::*;

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

    // --- Additional tests for broader coverage ---

    #[test]
    fn update_platform_empty_strings() {
        assert!(platform_asset_name("", "").is_none());
        assert!(platform_asset_name("linux", "").is_none());
        assert!(platform_asset_name("", "x86_64").is_none());
    }

    #[test]
    fn update_platform_case_sensitivity() {
        // platform_asset_name should be case-sensitive (OS constants are lowercase)
        assert!(platform_asset_name("Linux", "x86_64").is_none());
        assert!(platform_asset_name("MACOS", "aarch64").is_none());
        assert!(platform_asset_name("Windows", "x86_64").is_none());
    }

    #[test]
    fn update_platform_all_supported_return_some() {
        // Exhaustive: every supported combo returns Some
        let supported = [
            ("linux", "x86_64"),
            ("macos", "x86_64"),
            ("macos", "aarch64"),
            ("windows", "x86_64"),
        ];
        for (os, arch) in &supported {
            assert!(
                platform_asset_name(os, arch).is_some(),
                "Expected Some for ({}, {})",
                os,
                arch
            );
        }
    }

    #[test]
    fn update_platform_tar_gz_vs_zip() {
        // Linux and macOS should produce .tar.gz, Windows should produce .zip
        for os in &["linux", "macos"] {
            for arch in &["x86_64", "aarch64"] {
                if let Some(name) = platform_asset_name(os, arch) {
                    assert!(
                        name.ends_with(".tar.gz"),
                        "Expected .tar.gz for {} {}, got {}",
                        os,
                        arch,
                        name
                    );
                }
            }
        }
        if let Some(name) = platform_asset_name("windows", "x86_64") {
            assert!(
                name.ends_with(".zip"),
                "Expected .zip for windows, got {}",
                name
            );
        }
    }

    #[test]
    fn update_find_asset_url_missing_name_field() {
        // Asset without a "name" field should not match
        let assets = vec![serde_json::json!({
            "browser_download_url": "https://example.com/download/linux.tar.gz"
        })];
        let url = find_asset_url(&assets, "yoyo-x86_64-unknown-linux-gnu.tar.gz");
        assert!(url.is_none());
    }

    #[test]
    fn update_find_asset_url_missing_download_url() {
        // Asset matches name but has no browser_download_url → None
        let assets = vec![serde_json::json!({
            "name": "yoyo-x86_64-unknown-linux-gnu.tar.gz"
        })];
        let url = find_asset_url(&assets, "yoyo-x86_64-unknown-linux-gnu.tar.gz");
        assert!(url.is_none());
    }

    #[test]
    fn update_find_asset_url_picks_correct_among_many() {
        // With all 4 platform assets, each one should resolve correctly
        let assets = vec![
            serde_json::json!({
                "name": "yoyo-x86_64-unknown-linux-gnu.tar.gz",
                "browser_download_url": "https://example.com/linux-x86.tar.gz"
            }),
            serde_json::json!({
                "name": "yoyo-x86_64-apple-darwin.tar.gz",
                "browser_download_url": "https://example.com/macos-x86.tar.gz"
            }),
            serde_json::json!({
                "name": "yoyo-aarch64-apple-darwin.tar.gz",
                "browser_download_url": "https://example.com/macos-arm.tar.gz"
            }),
            serde_json::json!({
                "name": "yoyo-x86_64-pc-windows-msvc.zip",
                "browser_download_url": "https://example.com/windows.zip"
            }),
        ];

        assert_eq!(
            find_asset_url(&assets, "yoyo-x86_64-apple-darwin.tar.gz"),
            Some("https://example.com/macos-x86.tar.gz".to_string())
        );
        assert_eq!(
            find_asset_url(&assets, "yoyo-aarch64-apple-darwin.tar.gz"),
            Some("https://example.com/macos-arm.tar.gz".to_string())
        );
        assert_eq!(
            find_asset_url(&assets, "yoyo-x86_64-pc-windows-msvc.zip"),
            Some("https://example.com/windows.zip".to_string())
        );
    }

    #[test]
    fn update_extract_archive_nonexistent_file() {
        let tmp = std::env::temp_dir().join("yoyo-test-extract-nofile");
        let result = extract_archive(
            "/tmp/nonexistent-archive-12345.tar.gz",
            tmp.to_str().unwrap(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn update_extract_archive_unsupported_format() {
        // Create a temp file with unsupported extension
        let tmp_file = std::env::temp_dir().join("yoyo-test-archive.rar");
        std::fs::write(&tmp_file, b"fake data").unwrap();
        let extract_dir = std::env::temp_dir().join("yoyo-test-extract-rar");

        let result = extract_archive(tmp_file.to_str().unwrap(), extract_dir.to_str().unwrap());
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("Unsupported archive format"),
            "Expected 'Unsupported archive format' error"
        );

        let _ = std::fs::remove_file(&tmp_file);
        let _ = std::fs::remove_dir_all(&extract_dir);
    }

    #[test]
    fn update_extract_archive_empty_tar_no_binary() {
        // Create a valid but empty tar.gz and verify it fails with "Could not find"
        let extract_dir = std::env::temp_dir().join("yoyo-test-extract-empty");
        let tar_path = std::env::temp_dir().join("yoyo-test-empty.tar.gz");

        // Create an empty tar.gz using the tar command
        let _ = std::fs::create_dir_all(&extract_dir);
        let empty_src = std::env::temp_dir().join("yoyo-test-empty-src");
        let _ = std::fs::create_dir_all(&empty_src);

        let status = std::process::Command::new("tar")
            .args([
                "czf",
                tar_path.to_str().unwrap(),
                "-C",
                empty_src.to_str().unwrap(),
                ".",
            ])
            .status();

        if let Ok(s) = status {
            if s.success() {
                let result =
                    extract_archive(tar_path.to_str().unwrap(), extract_dir.to_str().unwrap());
                assert!(result.is_err());
                let err = result.unwrap_err();
                assert!(
                    err.contains("Could not find yoyo binary"),
                    "Expected 'Could not find yoyo binary', got: {}",
                    err
                );
            }
        }

        let _ = std::fs::remove_file(&tar_path);
        let _ = std::fs::remove_dir_all(&extract_dir);
        let _ = std::fs::remove_dir_all(&empty_src);
    }

    #[test]
    fn update_extract_archive_finds_binary_at_root() {
        // Create a tar.gz containing a file named "yoyo" — extract_archive should find it
        let test_id = "yoyo-test-root-binary";
        let src_dir = std::env::temp_dir().join(format!("{}-src", test_id));
        let tar_path = std::env::temp_dir().join(format!("{}.tar.gz", test_id));
        let extract_dir = std::env::temp_dir().join(format!("{}-out", test_id));

        let _ = std::fs::create_dir_all(&src_dir);
        std::fs::write(src_dir.join("yoyo"), b"#!/bin/sh\necho hello").unwrap();

        let status = std::process::Command::new("tar")
            .args([
                "czf",
                tar_path.to_str().unwrap(),
                "-C",
                src_dir.to_str().unwrap(),
                "yoyo",
            ])
            .status();

        if let Ok(s) = status {
            if s.success() {
                let result =
                    extract_archive(tar_path.to_str().unwrap(), extract_dir.to_str().unwrap());
                assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
                let binary_path = result.unwrap();
                assert!(
                    binary_path.contains("yoyo"),
                    "Binary path should contain 'yoyo': {}",
                    binary_path
                );
            }
        }

        let _ = std::fs::remove_file(&tar_path);
        let _ = std::fs::remove_dir_all(&src_dir);
        let _ = std::fs::remove_dir_all(&extract_dir);
    }

    #[test]
    fn update_extract_archive_finds_binary_in_subdir() {
        // Create tar.gz where "yoyo" is inside a subdirectory
        let test_id = "yoyo-test-subdir-binary";
        let src_dir = std::env::temp_dir().join(format!("{}-src", test_id));
        let sub_dir = src_dir.join("yoyo-v1.0.0");
        let tar_path = std::env::temp_dir().join(format!("{}.tar.gz", test_id));
        let extract_dir = std::env::temp_dir().join(format!("{}-out", test_id));

        let _ = std::fs::create_dir_all(&sub_dir);
        std::fs::write(sub_dir.join("yoyo"), b"#!/bin/sh\necho hello").unwrap();

        let status = std::process::Command::new("tar")
            .args([
                "czf",
                tar_path.to_str().unwrap(),
                "-C",
                src_dir.to_str().unwrap(),
                "yoyo-v1.0.0",
            ])
            .status();

        if let Ok(s) = status {
            if s.success() {
                let result =
                    extract_archive(tar_path.to_str().unwrap(), extract_dir.to_str().unwrap());
                assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
                let binary_path = result.unwrap();
                assert!(
                    binary_path.contains("yoyo"),
                    "Binary path should contain 'yoyo': {}",
                    binary_path
                );
            }
        }

        let _ = std::fs::remove_file(&tar_path);
        let _ = std::fs::remove_dir_all(&src_dir);
        let _ = std::fs::remove_dir_all(&extract_dir);
    }

    #[test]
    fn update_version_comparison_extended() {
        // Edge cases for version comparison
        assert!(crate::update::version_is_newer("0.1.0", "0.1.1"));
        assert!(crate::update::version_is_newer("0.9.9", "1.0.0"));
        assert!(!crate::update::version_is_newer("1.0.0", "0.9.9"));
        assert!(!crate::update::version_is_newer("1.0.0", "1.0.0"));
        // Major version jump
        assert!(crate::update::version_is_newer("1.9.9", "2.0.0"));
    }

    #[test]
    fn update_current_exe_exists() {
        // current_exe() should succeed and point to an existing file in test context
        let exe = std::env::current_exe();
        assert!(exe.is_ok(), "current_exe() should succeed");
        let path = exe.unwrap();
        assert!(path.exists(), "current exe path should exist: {:?}", path);
    }

    #[test]
    fn update_download_file_bad_url() {
        // download_file with a non-routable URL should fail
        let tmp_path = std::env::temp_dir().join("yoyo-test-download-bad");
        let result = download_file("https://0.0.0.0:1/nonexistent", tmp_path.to_str().unwrap());
        assert!(result.is_err());
        let _ = std::fs::remove_file(&tmp_path);
    }
}
