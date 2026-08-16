//! Self-update command handler: /update.

use crate::cli;
use crate::format::*;
use std::path::{Path, PathBuf};

/// Where one `/update` run stages its download and its extracted tree.
///
/// Both paths are built with [`Path::join`], never by string concatenation with
/// `/`, so they are correct on Windows as well as Unix. See [`update_paths`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpdatePaths {
    /// The downloaded archive.
    pub archive: PathBuf,
    /// The directory the archive is extracted into — unique per run.
    pub extract_dir: PathBuf,
}

/// Build the staging paths for one update run (pure).
///
/// `temp_root` is the caller's temp directory (`std::env::temp_dir()` in
/// production — `%TEMP%` on Windows, `/tmp` or `$TMPDIR` elsewhere); `run_id`
/// makes the extract directory unique per run, so two concurrent updates and a
/// stale tree from a previously failed run cannot collide in one shared
/// directory and leave a half-extracted tree that the next run reads as
/// complete.
///
/// Every path segment is added with `join`, so no `/` is ever hardcoded into a
/// path and the separator is whatever the host uses.
fn update_paths(temp_root: &Path, run_id: &str, version: &str, ext: &str) -> UpdatePaths {
    UpdatePaths {
        archive: temp_root.join(format!("yoyo-update-{}-{}.{}", run_id, version, ext)),
        extract_dir: temp_root.join(format!("yoyo-update-{}", run_id)),
    }
}

/// An id unique to this process (and re-run-safe enough in practice): the pid
/// plus the current wall-clock seconds, so a recycled pid does not reuse a
/// stale directory.
fn update_run_id() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}-{}", std::process::id(), secs)
}

/// Choose the extractor argv for an archive (pure).
///
/// Returns the full argv — program first — or `None` for an archive kind we do
/// not know how to unpack.
///
/// `tar` is used for **both** kinds on purpose. Windows has shipped bsdtar
/// since Windows 10 1803 and does **not** ship `unzip`, while the release
/// pipeline publishes the `.zip` only for the Windows target — so the zip
/// branch is the Windows branch, and shelling out to `unzip` there named the
/// one extractor that host does not have. bsdtar reads zip archives, and GNU
/// tar is not asked to (no Unix target ships a zip).
fn extractor_argv(archive_path: &str, extract_dir: &str) -> Option<Vec<String>> {
    let args: &[&str] = if archive_path.ends_with(".tar.gz") {
        &["tar", "xzf"]
    } else if archive_path.ends_with(".zip") {
        &["tar", "-xf"]
    } else {
        return None;
    };
    let mut argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    argv.push(archive_path.to_string());
    argv.push("-C".to_string());
    argv.push(extract_dir.to_string());
    Some(argv)
}

/// The error shown when the extractor could not be run at all (pure).
///
/// It names the command that was not found **and** the archive that was left
/// behind, so the user can finish the update by hand instead of being told
/// only that something failed.
fn extractor_missing_message(program: &str, archive_path: &str, err: &str) -> String {
    format!(
        "could not run `{}` to extract the update ({}). \
         The downloaded archive was left at {} — extract it yourself and replace the yoyo binary manually.",
        program, err, archive_path
    )
}

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
    let (triple, ext) = match platform_target(os, arch) {
        Some(t) => t,
        None => {
            return Err(format!("Unsupported platform: {} {}", os, arch));
        }
    };

    let empty_assets = Vec::new();
    let assets = latest_release
        .get("assets")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_assets);

    let (asset_name, download_url) = match find_asset(assets, triple, ext) {
        Some(found) => found,
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
    let run_id = update_run_id();
    let paths = update_paths(&std::env::temp_dir(), &run_id, latest_version, ext);
    let temp_path = paths.archive.to_string_lossy().to_string();
    let extract_dir = paths.extract_dir.to_string_lossy().to_string();

    println!("Downloading {}...", asset_name);
    match download_file(&download_url, &temp_path) {
        Ok(_) => (),
        Err(e) => {
            let _ = std::fs::remove_file(&paths.archive);
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

    // Step 5: Extract and replace. The extract dir is per-run, and created
    // fresh — a leftover tree from a crashed earlier run must never be read as
    // a complete extraction.
    let _ = std::fs::remove_dir_all(&paths.extract_dir);
    match extract_archive(&temp_path, &extract_dir) {
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

            println!("{}", update_success_message(latest_version));
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
            // Remove the half-extracted tree so a failed update leaves nothing
            // a later run could mistake for a complete extraction. The archive
            // itself is deliberately kept — the extractor errors above name it
            // so the user can finish by hand.
            let _ = std::fs::remove_dir_all(&paths.extract_dir);
            Err(format!("Failed to extract archive: {}", e))
        }
    }
}

/// Map OS/ARCH to the release asset's target triple and archive extension.
/// Returns None for unsupported platforms.
///
/// The full asset name is NOT constructed here: `release.yml` embeds the tag
/// (`yoyo-v0.1.16-x86_64-unknown-linux-gnu.tar.gz`), so any constructed name
/// would need to know the version. Selection happens by suffix instead —
/// see `asset_matches`.
fn platform_target(os: &str, arch: &str) -> Option<(&'static str, &'static str)> {
    match (os, arch) {
        ("linux", "x86_64") => Some(("x86_64-unknown-linux-gnu", "tar.gz")),
        ("macos", "x86_64") => Some(("x86_64-apple-darwin", "tar.gz")),
        ("macos", "aarch64") => Some(("aarch64-apple-darwin", "tar.gz")),
        ("windows", "x86_64") => Some(("x86_64-pc-windows-msvc", "zip")),
        _ => None,
    }
}

/// Compose the success line printed after a successful update.
///
/// `tag` is the raw `tag_name` from the release JSON, which already carries a
/// leading `v` (`v0.1.16`) — strip it so the message doesn't read `vv0.1.16`.
fn update_success_message(tag: &str) -> String {
    let version = tag.strip_prefix('v').unwrap_or(tag);
    format!(
        "✓ Updated to v{}! Please restart yoyo to use the new version.",
        version
    )
}

/// Does this release asset name belong to `triple` + `ext`?
///
/// Version-agnostic by construction: matches on the `-<triple>.<ext>` suffix so
/// both `yoyo-x86_64-unknown-linux-gnu.tar.gz` and the tagged form
/// `yoyo-v0.1.16-x86_64-unknown-linux-gnu.tar.gz` are accepted.
/// `.sha256` sidecars share the prefix, so they are excluded explicitly.
fn asset_matches(name: &str, triple: &str, ext: &str) -> bool {
    if name.ends_with(".sha256") {
        return false;
    }
    if !name.starts_with("yoyo-") {
        return false;
    }
    name.ends_with(&format!("-{}.{}", triple, ext))
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
/// Find the release asset for `triple` + `ext`, returning `(name, download_url)`.
///
/// Selection is by suffix rather than exact-string equality, so it survives the
/// tag being embedded in the asset name. If several assets match, the first wins
/// — the release workflow publishes exactly one archive per target, so there is
/// no meaningful tie-break to invent.
fn find_asset(assets: &[serde_json::Value], triple: &str, ext: &str) -> Option<(String, String)> {
    assets.iter().find_map(|asset| {
        let name = asset.get("name").and_then(|name| name.as_str())?;
        if !asset_matches(name, triple, ext) {
            return None;
        }
        let url = asset
            .get("browser_download_url")
            .and_then(|url| url.as_str())?;
        Some((name.to_string(), url.to_string()))
    })
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

    if let Some(argv) = extractor_argv(archive_path, extract_dir) {
        let program = &argv[0];
        let output = std::process::Command::new(program)
            .args(&argv[1..])
            .output()
            .map_err(|e| extractor_missing_message(program, archive_path, &e.to_string()))?;
        if !output.status.success() {
            return Err(format!(
                "`{}` failed to extract {} (exit {}). {}",
                program,
                archive_path,
                output
                    .status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".to_string()),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    } else {
        return Err("Unsupported archive format".to_string());
    }

    // Find the yoyo binary in the extracted directory.
    // The Windows zip packs `yoyo.exe` (see release.yml's Package (Windows) step),
    // Unix tarballs pack `yoyo` — accept either.
    let binary_names = ["yoyo", "yoyo.exe"];
    let entries = std::fs::read_dir(extract_dir)
        .map_err(|e| format!("Failed to read extract directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|name| name.to_str()) {
                if binary_names.contains(&filename) {
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
            for name in binary_names {
                let binary_path = path.join(name);
                if binary_path.exists() {
                    return Ok(binary_path.to_string_lossy().to_string());
                }
            }
        }
    }

    Err("Could not find yoyo binary in extracted archive".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Windows-safety of the staging paths and the extractor choice (#756) ----
    //
    // These pin what a caller actually receives: the `PathBuf`s handed to the
    // download/extract steps, and the argv that reaches the OS. They never
    // shell out to a real download or a real extractor.

    #[test]
    fn update_paths_are_under_the_given_temp_root() {
        for root in [
            Path::new("/tmp"),
            Path::new("/var/folders/x9/tmpdir"),
            Path::new(r"C:\Users\me\AppData\Local\Temp"),
        ] {
            let p = update_paths(root, "4242-1700000000", "v0.1.16", "tar.gz");
            assert!(
                p.archive.starts_with(root),
                "archive {:?} not under {:?}",
                p.archive,
                root
            );
            assert!(
                p.extract_dir.starts_with(root),
                "extract dir {:?} not under {:?}",
                p.extract_dir,
                root
            );
        }
    }

    #[test]
    fn update_paths_never_hardcode_a_separator_in_a_segment() {
        let root = Path::new("ROOT");
        let p = update_paths(root, "77-1700000000", "v0.1.16", "tar.gz");
        // Exactly one component is appended to the root for each path, so the
        // separator between them is the platform's own — no `/` was baked in.
        let archive_tail: Vec<_> = p.archive.strip_prefix(root).unwrap().components().collect();
        let dir_tail: Vec<_> = p
            .extract_dir
            .strip_prefix(root)
            .unwrap()
            .components()
            .collect();
        assert_eq!(archive_tail.len(), 1, "archive tail: {:?}", archive_tail);
        assert_eq!(dir_tail.len(), 1, "extract dir tail: {:?}", dir_tail);

        let file = p.archive.file_name().unwrap().to_str().unwrap();
        let dir = p.extract_dir.file_name().unwrap().to_str().unwrap();
        assert!(
            !file.contains('/'),
            "archive segment holds a slash: {}",
            file
        );
        assert!(
            !file.contains('\\'),
            "archive segment holds a backslash: {}",
            file
        );
        assert!(!dir.contains('/'), "dir segment holds a slash: {}", dir);
        assert!(
            !dir.contains('\\'),
            "dir segment holds a backslash: {}",
            dir
        );
        assert_eq!(file, "yoyo-update-77-1700000000-v0.1.16.tar.gz");
        assert_eq!(dir, "yoyo-update-77-1700000000");
    }

    #[test]
    fn update_paths_extract_dir_is_unique_per_run() {
        let root = Path::new("/tmp");
        let a = update_paths(root, "100-1700000000", "v0.1.16", "tar.gz");
        let b = update_paths(root, "101-1700000000", "v0.1.16", "tar.gz");
        assert_ne!(
            a.extract_dir, b.extract_dir,
            "two runs shared one extract dir"
        );
        assert_ne!(a.archive, b.archive, "two runs shared one archive path");
        // Same run id → same paths (the helper is pure).
        assert_eq!(a, update_paths(root, "100-1700000000", "v0.1.16", "tar.gz"));
    }

    #[test]
    fn update_run_id_is_not_a_fixed_shared_name() {
        let id = update_run_id();
        assert!(!id.is_empty());
        assert!(
            id.starts_with(&format!("{}-", std::process::id())),
            "run id {} does not carry this process's pid",
            id
        );
    }

    #[test]
    fn extractor_argv_zip_branch_never_uses_unzip() {
        let argv = extractor_argv(r"C:\Temp\yoyo-update-1-2.zip", r"C:\Temp\yoyo-update-1")
            .expect("zip is a supported archive kind");
        assert_ne!(
            argv[0], "unzip",
            "the zip branch is the Windows branch and Windows ships no unzip"
        );
        assert_eq!(
            argv,
            vec![
                "tar".to_string(),
                "-xf".to_string(),
                r"C:\Temp\yoyo-update-1-2.zip".to_string(),
                "-C".to_string(),
                r"C:\Temp\yoyo-update-1".to_string(),
            ]
        );
    }

    #[test]
    fn extractor_argv_tar_gz_branch_is_the_tar_invocation() {
        let argv = extractor_argv("/tmp/yoyo-update-1-2.tar.gz", "/tmp/yoyo-update-1")
            .expect("tar.gz is a supported archive kind");
        assert_eq!(
            argv,
            vec![
                "tar".to_string(),
                "xzf".to_string(),
                "/tmp/yoyo-update-1-2.tar.gz".to_string(),
                "-C".to_string(),
                "/tmp/yoyo-update-1".to_string(),
            ]
        );
    }

    #[test]
    fn extractor_argv_rejects_unknown_archive_kinds() {
        assert!(extractor_argv("/tmp/yoyo.rar", "/tmp/out").is_none());
        assert!(extractor_argv("/tmp/yoyo", "/tmp/out").is_none());
        assert!(extractor_argv("/tmp/yoyo.tar", "/tmp/out").is_none());
    }

    #[test]
    fn extractor_missing_message_names_the_command_and_the_kept_archive() {
        let msg = extractor_missing_message("tar", "/tmp/yoyo-update-9-1.zip", "No such file");
        assert!(
            msg.contains("tar"),
            "message must name the command: {}",
            msg
        );
        assert!(
            msg.contains("/tmp/yoyo-update-9-1.zip"),
            "message must name the archive left behind: {}",
            msg
        );
        assert!(
            msg.contains("No such file"),
            "message must carry the OS error: {}",
            msg
        );
    }

    /// The zip branch exists for exactly one target, and that is why it may not
    /// shell out to `unzip`.
    #[test]
    fn update_zip_extension_belongs_to_the_windows_target_only() {
        assert_eq!(
            platform_target("windows", "x86_64").map(|(_, e)| e),
            Some("zip")
        );
        for (os, arch) in [
            ("linux", "x86_64"),
            ("macos", "x86_64"),
            ("macos", "aarch64"),
        ] {
            assert_eq!(platform_target(os, arch).map(|(_, e)| e), Some("tar.gz"));
        }
    }

    /// A realistic published asset list: names carry the tag (as `release.yml`
    /// writes them), every platform is present, and the `.sha256` sidecars —
    /// which share the `yoyo-` prefix — are included.
    fn realistic_assets(tag: &str) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        for (triple, ext) in [
            ("x86_64-unknown-linux-gnu", "tar.gz"),
            ("x86_64-apple-darwin", "tar.gz"),
            ("aarch64-apple-darwin", "tar.gz"),
            ("x86_64-pc-windows-msvc", "zip"),
        ] {
            let name = format!("yoyo-{}-{}.{}", tag, triple, ext);
            out.push(serde_json::json!({
                "name": name,
                "browser_download_url": format!("https://example.com/{}", name),
            }));
            out.push(serde_json::json!({
                "name": format!("{}.sha256", name),
                "browser_download_url": format!("https://example.com/{}.sha256", name),
            }));
        }
        out
    }

    fn select_for(os: &str, arch: &str, assets: &[serde_json::Value]) -> Option<(String, String)> {
        let (triple, ext) = platform_target(os, arch)?;
        find_asset(assets, triple, ext)
    }

    #[test]
    fn update_selects_linux_x86_64_from_published_assets() {
        let (name, url) = select_for("linux", "x86_64", &realistic_assets("v0.1.16")).unwrap();
        assert_eq!(name, "yoyo-v0.1.16-x86_64-unknown-linux-gnu.tar.gz");
        assert_eq!(
            url,
            "https://example.com/yoyo-v0.1.16-x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn update_selects_macos_intel_from_published_assets() {
        let (name, _) = select_for("macos", "x86_64", &realistic_assets("v0.1.16")).unwrap();
        assert_eq!(name, "yoyo-v0.1.16-x86_64-apple-darwin.tar.gz");
    }

    #[test]
    fn update_selects_macos_arm_from_published_assets() {
        let (name, _) = select_for("macos", "aarch64", &realistic_assets("v0.1.16")).unwrap();
        assert_eq!(name, "yoyo-v0.1.16-aarch64-apple-darwin.tar.gz");
    }

    #[test]
    fn update_selects_windows_zip_from_published_assets() {
        let (name, _) = select_for("windows", "x86_64", &realistic_assets("v0.1.16")).unwrap();
        assert_eq!(name, "yoyo-v0.1.16-x86_64-pc-windows-msvc.zip");
    }

    #[test]
    fn update_selection_is_version_agnostic() {
        // The tag is not part of the match, so a future release and the old
        // untagged naming both resolve.
        for tag in ["v0.1.16", "v9.9.9", "v10.0.0-rc.1"] {
            let (name, _) = select_for("linux", "x86_64", &realistic_assets(tag)).unwrap();
            assert_eq!(
                name,
                format!("yoyo-{}-x86_64-unknown-linux-gnu.tar.gz", tag)
            );
        }
        let untagged = vec![serde_json::json!({
            "name": "yoyo-x86_64-unknown-linux-gnu.tar.gz",
            "browser_download_url": "https://example.com/legacy.tar.gz",
        })];
        assert_eq!(
            select_for("linux", "x86_64", &untagged).map(|(n, _)| n),
            Some("yoyo-x86_64-unknown-linux-gnu.tar.gz".to_string())
        );
    }

    #[test]
    fn update_never_selects_a_sha256_sidecar() {
        // The sidecar shares the prefix and the triple; only the suffix differs.
        let sidecars_only = vec![
            serde_json::json!({
                "name": "yoyo-v0.1.16-x86_64-unknown-linux-gnu.tar.gz.sha256",
                "browser_download_url": "https://example.com/sum",
            }),
            serde_json::json!({
                "name": "yoyo-v0.1.16-x86_64-pc-windows-msvc.zip.sha256",
                "browser_download_url": "https://example.com/sum2",
            }),
        ];
        assert!(select_for("linux", "x86_64", &sidecars_only).is_none());
        assert!(select_for("windows", "x86_64", &sidecars_only).is_none());
        for (name, _) in [
            select_for("linux", "x86_64", &realistic_assets("v0.1.16")).unwrap(),
            select_for("windows", "x86_64", &realistic_assets("v0.1.16")).unwrap(),
        ] {
            assert!(
                !name.ends_with(".sha256"),
                "picked a checksum file: {}",
                name
            );
        }
    }

    #[test]
    fn update_platform_target_unsupported() {
        assert!(platform_target("freebsd", "x86_64").is_none());
        assert!(platform_target("linux", "arm").is_none());
        assert!(platform_target("windows", "aarch64").is_none());
    }

    /// The one test in this repo that compares the updater against the *authority*
    /// (`release.yml`) instead of against another copy of the same belief.
    ///
    /// What it pins from the workflow: the asset-name shape (prefix, tag position,
    /// suffix), the `.sha256` sidecars, and the list of built targets. The
    /// triple→extension pairing still comes from `platform_target` — but the
    /// workflow's own archive suffixes are asserted to contain it.
    #[test]
    fn update_asset_selection_is_pinned_against_release_workflow() {
        const MARKER: &str = "yoyo-${{ github.ref_name }}-${{ matrix.target }}";
        const FAKE_TAG: &str = "v9.9.9";

        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
        let workflow = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e));

        // Every suffix the workflow appends to the archive-name marker.
        let mut suffixes: Vec<String> = Vec::new();
        let mut rest = workflow.as_str();
        while let Some(idx) = rest.find(MARKER) {
            let after = &rest[idx + MARKER.len()..];
            let end = after
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .unwrap_or(after.len());
            let suffix = after[..end].to_string();
            if !suffix.is_empty() && !suffixes.contains(&suffix) {
                suffixes.push(suffix);
            }
            rest = after;
        }
        if suffixes.is_empty() {
            panic!(
                "no occurrence of `{}` found in {} — the release asset naming changed, \
                 so the updater's selector must be re-derived rather than silently passing",
                MARKER,
                path.display()
            );
        }
        let archive_suffixes: Vec<&String> = suffixes
            .iter()
            .filter(|s| !s.ends_with(".sha256"))
            .collect();
        assert!(
            !archive_suffixes.is_empty(),
            "{} names only .sha256 files after `{}` — no archive to download",
            path.display(),
            MARKER
        );

        // Every target the workflow builds.
        let mut targets: Vec<String> = Vec::new();
        for line in workflow.lines() {
            if let Some(value) = line.trim().strip_prefix("- target:") {
                let target = value.trim().to_string();
                if !target.is_empty() && !targets.contains(&target) {
                    targets.push(target);
                }
            }
        }
        if targets.is_empty() {
            panic!(
                "no `- target: <triple>` entries found in {} — the release matrix changed",
                path.display()
            );
        }

        let supported = [
            ("linux", "x86_64"),
            ("macos", "x86_64"),
            ("macos", "aarch64"),
            ("windows", "x86_64"),
        ];
        let ext_for = |triple: &str| -> Option<&'static str> {
            supported
                .iter()
                .find_map(|(os, arch)| match platform_target(os, arch) {
                    Some((t, e)) if t == triple => Some(e),
                    _ => None,
                })
        };

        // Reconstruct what a real release publishes, from the workflow's own data.
        let mut assets = Vec::new();
        for target in &targets {
            let ext = ext_for(target).unwrap_or("tar.gz");
            let suffix = format!(".{}", ext);
            assert!(
                archive_suffixes.iter().any(|s| **s == suffix),
                "updater expects a `{}` archive for {}, but {} publishes {:?}",
                suffix,
                target,
                path.display(),
                archive_suffixes
            );
            let name = format!("yoyo-{}-{}{}", FAKE_TAG, target, suffix);
            assets.push(serde_json::json!({
                "name": name,
                "browser_download_url": format!("https://example.com/{}", name),
            }));
            assets.push(serde_json::json!({
                "name": format!("{}.sha256", name),
                "browser_download_url": format!("https://example.com/{}.sha256", name),
            }));
        }

        for (os, arch) in supported {
            let (triple, ext) = platform_target(os, arch)
                .unwrap_or_else(|| panic!("platform_target lost ({}, {})", os, arch));
            assert!(
                targets.contains(&triple.to_string()),
                "{} no longer builds {} — {} {} users have no asset",
                path.display(),
                triple,
                os,
                arch
            );
            let expected = format!("yoyo-{}-{}.{}", FAKE_TAG, triple, ext);
            assert_eq!(
                find_asset(&assets, triple, ext),
                Some((
                    expected.clone(),
                    format!("https://example.com/{}", expected)
                )),
                "updater failed to select {} for {} {}",
                expected,
                os,
                arch
            );
        }
    }

    #[test]
    fn update_success_message_does_not_double_the_v() {
        assert_eq!(
            update_success_message("v0.1.16"),
            "✓ Updated to v0.1.16! Please restart yoyo to use the new version."
        );
        // A tag without the prefix still renders exactly one `v`.
        assert_eq!(
            update_success_message("0.1.16"),
            "✓ Updated to v0.1.16! Please restart yoyo to use the new version."
        );
        assert!(!update_success_message("v0.1.16").contains("vv"));
    }

    #[test]
    fn update_find_asset_no_match_and_empty() {
        let assets = realistic_assets("v0.1.16");
        // A triple nobody published.
        assert!(find_asset(&assets, "riscv64gc-unknown-linux-gnu", "tar.gz").is_none());
        // Right triple, wrong archive kind.
        assert!(find_asset(&assets, "x86_64-unknown-linux-gnu", "zip").is_none());
        let empty: Vec<serde_json::Value> = vec![];
        assert!(find_asset(&empty, "x86_64-unknown-linux-gnu", "tar.gz").is_none());
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
    fn update_platform_target_empty_strings() {
        assert!(platform_target("", "").is_none());
        assert!(platform_target("linux", "").is_none());
        assert!(platform_target("", "x86_64").is_none());
    }

    #[test]
    fn update_platform_target_case_sensitivity() {
        // platform_target is case-sensitive (std::env::consts are lowercase)
        assert!(platform_target("Linux", "x86_64").is_none());
        assert!(platform_target("MACOS", "aarch64").is_none());
        assert!(platform_target("Windows", "x86_64").is_none());
    }

    #[test]
    fn update_platform_target_all_supported_resolve() {
        // Exhaustive: every supported combo yields a triple that selects an asset.
        for (os, arch) in [
            ("linux", "x86_64"),
            ("macos", "x86_64"),
            ("macos", "aarch64"),
            ("windows", "x86_64"),
        ] {
            let assets = realistic_assets("v0.1.16");
            assert!(
                select_for(os, arch, &assets).is_some(),
                "no asset selected for ({}, {})",
                os,
                arch
            );
        }
    }

    #[test]
    fn update_platform_target_archive_kind() {
        // Unix targets ship tarballs, Windows ships a zip.
        for os in ["linux", "macos"] {
            for arch in ["x86_64", "aarch64"] {
                if let Some((_, ext)) = platform_target(os, arch) {
                    assert_eq!(ext, "tar.gz", "expected tar.gz for {} {}", os, arch);
                }
            }
        }
        assert_eq!(
            platform_target("windows", "x86_64").map(|(_, e)| e),
            Some("zip")
        );
    }

    #[test]
    fn update_find_asset_ignores_malformed_entries() {
        // Asset without a "name" field, and one matching by name but with no URL:
        // neither may be selected, and neither may hide a later valid asset.
        let assets = vec![
            serde_json::json!({
                "browser_download_url": "https://example.com/nameless.tar.gz"
            }),
            serde_json::json!({
                "name": "yoyo-v0.1.16-x86_64-unknown-linux-gnu.tar.gz"
            }),
            serde_json::json!({
                "name": "yoyo-v0.1.16-x86_64-unknown-linux-gnu.tar.gz",
                "browser_download_url": "https://example.com/real.tar.gz"
            }),
        ];
        assert_eq!(
            find_asset(&assets, "x86_64-unknown-linux-gnu", "tar.gz"),
            Some((
                "yoyo-v0.1.16-x86_64-unknown-linux-gnu.tar.gz".to_string(),
                "https://example.com/real.tar.gz".to_string()
            ))
        );
    }

    #[test]
    fn update_find_asset_picks_correct_among_many() {
        // Every platform resolves to its own archive out of the full published set.
        let assets = realistic_assets("v0.1.16");
        for (os, arch, expected) in [
            (
                "linux",
                "x86_64",
                "yoyo-v0.1.16-x86_64-unknown-linux-gnu.tar.gz",
            ),
            ("macos", "x86_64", "yoyo-v0.1.16-x86_64-apple-darwin.tar.gz"),
            (
                "macos",
                "aarch64",
                "yoyo-v0.1.16-aarch64-apple-darwin.tar.gz",
            ),
            (
                "windows",
                "x86_64",
                "yoyo-v0.1.16-x86_64-pc-windows-msvc.zip",
            ),
        ] {
            assert_eq!(
                select_for(os, arch, &assets).map(|(n, _)| n),
                Some(expected.to_string()),
                "wrong asset for {} {}",
                os,
                arch
            );
        }
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
