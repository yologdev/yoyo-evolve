/// Extract a 7-char short commit hash from the text of `.cargo_vcs_info.json`,
/// which `cargo publish` writes into the package root (#994). A crates.io
/// install builds from unpacked registry source with no `.git`, so this file
/// is the only record of the published commit.
///
/// Deliberately a substring scan, not a JSON parse: build.rs takes no build
/// dependencies. Returns `None` unless the `"sha1"` value is a string whose
/// first 7 chars are ASCII hex — so the slice below is always on an ASCII
/// boundary (never byte-index non-ASCII text, #250).
pub fn short_sha_from_vcs_info(text: &str) -> Option<String> {
    let after_key = &text[text.find("\"sha1\"")? + "\"sha1\"".len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let value_start = after_colon.trim_start().strip_prefix('"')?;
    let value = &value_start[..value_start.find('"')?];
    let short = value.get(..7)?;
    if short.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(short.to_string())
    } else {
        None
    }
}

/// The git files whose change means HEAD moved, as `rerun-if-changed` paths (#995).
///
/// Any `rerun-if-changed` line (this script emits one for `DAY_COUNT`) turns off
/// cargo's default "rerun on any package file change", so without these the
/// script never reruns after a commit and `GIT_HASH` freezes. Watched: HEAD
/// itself (checkout, detach), the branch ref HEAD points at (a commit on that
/// branch), and `packed-refs` (where the ref lives after `git gc`/`pack-refs`).
///
/// `git_path` answers `git rev-parse --git-path <name>` (so worktrees and
/// `.git`-as-file checkouts resolve correctly); `None` means git is absent or
/// failed, which yields no paths. `head_ref` is `git symbolic-ref -q HEAD`, `None`
/// when detached (then HEAD itself changes on a new commit). Only paths that
/// `exists` are returned: cargo treats a missing path as always stale, which
/// would rerun the script on every build.
pub fn git_rerun_paths(
    git_path: impl Fn(&str) -> Option<String>,
    head_ref: Option<&str>,
    exists: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut names = vec!["HEAD"];
    if let Some(r) = head_ref.map(str::trim).filter(|r| !r.is_empty()) {
        names.push(r);
    }
    names.push("packed-refs");
    let mut out: Vec<String> = Vec::new();
    for name in names {
        if let Some(p) = git_path(name) {
            if !p.is_empty() && exists(&p) && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// Run `git <args>` and return trimmed stdout, or `None` if git is missing or fails.
fn git_stdout(args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Some(text)
}

fn main() {
    // Expose git short hash at compile time. A packaged (crates.io) build
    // has no .git, so prefer the commit cargo publish recorded (#994).
    let vcs_info = std::path::Path::new(
        &std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()),
    )
    .join(".cargo_vcs_info.json");
    if vcs_info.exists() {
        // Only when it exists: rerun-if-changed on a missing path makes cargo
        // rerun the build script on every build.
        println!("cargo:rerun-if-changed=.cargo_vcs_info.json");
    }
    let packaged_hash = std::fs::read_to_string(&vcs_info)
        .ok()
        .and_then(|text| short_sha_from_vcs_info(&text));
    if std::env::var("GIT_HASH").is_err() {
        if let Some(hash) = packaged_hash {
            println!("cargo:rustc-env=GIT_HASH={hash}");
        } else if let Ok(output) = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
        {
            if output.status.success() {
                let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
                println!("cargo:rustc-env=GIT_HASH={hash}");
                // The hash came from git, so rerun when HEAD moves (#995).
                // Not reached for a packaged build or a GIT_HASH override.
                let head_ref = git_stdout(&["symbolic-ref", "-q", "HEAD"]);
                let paths = git_rerun_paths(
                    |name| git_stdout(&["rev-parse", "--git-path", name]),
                    head_ref.as_deref(),
                    |p| std::path::Path::new(p).exists(),
                );
                for p in paths {
                    println!("cargo:rerun-if-changed={p}");
                }
            }
        }
    }

    // Expose build date at compile time if not already set
    if std::env::var("BUILD_DATE").is_err() {
        // Use a simple date from the build environment
        if let Ok(output) = std::process::Command::new("date")
            .args(["+%Y-%m-%d"])
            .output()
        {
            if output.status.success() {
                let date = String::from_utf8_lossy(&output.stdout).trim().to_string();
                println!("cargo:rustc-env=BUILD_DATE={date}");
            }
        }
    }

    // Expose evolution day count at compile time (only present in yoyo's own repo)
    if std::env::var("DAY_COUNT").is_err() {
        if let Ok(content) = std::fs::read_to_string("DAY_COUNT") {
            if let Ok(day) = content.trim().parse::<u32>() {
                println!("cargo:rustc-env=DAY_COUNT={day}");
            }
        }
    }
    println!("cargo:rerun-if-changed=DAY_COUNT");

    // Read yoagent version from Cargo.lock (more reliable than parsing Cargo.toml)
    if let Ok(lock_content) = std::fs::read_to_string("Cargo.lock") {
        for chunk in lock_content.split("\n[[package]]") {
            let mut name = None;
            let mut version = None;
            for line in chunk.lines() {
                let line = line.trim();
                if let Some(n) = line.strip_prefix("name = \"") {
                    name = n.strip_suffix('"');
                }
                if let Some(v) = line.strip_prefix("version = \"") {
                    version = v.strip_suffix('"');
                }
            }
            if name == Some("yoagent") {
                if let Some(v) = version {
                    println!("cargo:rustc-env=YOAGENT_VERSION={v}");
                }
                break;
            }
        }
    }
}
