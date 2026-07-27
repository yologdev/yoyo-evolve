/// Split a version string into its numeric core and optional pre-release tag,
/// following semver: an optional leading `v`, `+build` metadata is discarded,
/// and everything after the first `-` is the pre-release identifier.
fn split_version(s: &str) -> (Vec<u64>, Option<&str>) {
    let s = s.trim();
    let s = s.strip_prefix('v').unwrap_or(s);
    // Build metadata (`+...`) is ignored for precedence.
    let s = s.split('+').next().unwrap_or(s);
    let (core, pre) = match s.split_once('-') {
        Some((core, pre)) if !pre.is_empty() => (core, Some(pre)),
        _ => (s, None),
    };
    let nums = core
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect();
    (nums, pre)
}

/// Compare two dot-separated pre-release tags by semver precedence rules:
/// identifiers are compared field by field, numeric identifiers compare
/// numerically and rank *lower* than alphanumeric ones, and a longer tag
/// wins when all preceding fields are equal.
fn compare_prerelease(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut ai = a.split('.');
    let mut bi = b.split('.');
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(nx), Ok(ny)) => nx.cmp(&ny),
                    // Numeric identifiers have lower precedence than alphanumeric.
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
        }
    }
}

/// Compare two version strings (e.g. "0.1.5" vs "0.2.0").
/// Returns true if `latest` is strictly newer than `current`.
///
/// Pre-release suffixes are honored: `0.2.0-rc1` is older than `0.2.0`, so
/// someone running a release candidate is still offered the final release.
pub fn version_is_newer(current: &str, latest: &str) -> bool {
    use std::cmp::Ordering;
    let (cur, cur_pre) = split_version(current);
    let (lat, lat_pre) = split_version(latest);
    let len = cur.len().max(lat.len());
    for i in 0..len {
        let c = cur.get(i).copied().unwrap_or(0);
        let l = lat.get(i).copied().unwrap_or(0);
        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }
    // Numeric cores are equal — the pre-release tag decides.
    match (cur_pre, lat_pre) {
        (None, None) => false,
        // A pre-release is older than its own stable release.
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (Some(c), Some(l)) => compare_prerelease(l, c) == Ordering::Greater,
    }
}

/// Check GitHub for a newer release. Returns `Some("x.y.z")` if a newer version
/// exists, `None` if current or on any error. Uses a 3-second timeout to avoid
/// blocking startup.
///
/// `current_version` is the running binary's version (e.g. `cli::VERSION`).
pub fn check_for_update(current_version: &str) -> Option<String> {
    let output = std::process::Command::new("curl")
        .args([
            "-sf",
            "--max-time",
            "3",
            "https://api.github.com/repos/yologdev/yoyo-evolve/releases/latest",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let body = String::from_utf8(output.stdout).ok()?;

    // Simple JSON extraction: find "tag_name": "v0.1.5"
    let tag = body
        .split("\"tag_name\"")
        .nth(1)?
        .split('"')
        .find(|s| !s.is_empty() && *s != ":" && *s != ": ")?;

    let latest = tag.strip_prefix('v').unwrap_or(tag);

    if version_is_newer(current_version, latest) {
        Some(latest.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_is_newer_basic() {
        assert!(version_is_newer("0.1.5", "0.2.0"));
    }

    #[test]
    fn test_version_is_newer_same() {
        assert!(!version_is_newer("0.1.5", "0.1.5"));
    }

    #[test]
    fn test_version_is_newer_older() {
        assert!(!version_is_newer("0.2.0", "0.1.5"));
    }

    #[test]
    fn test_version_is_newer_numeric_comparison() {
        // Must compare numerically, not lexicographically
        assert!(version_is_newer("0.1.5", "0.1.10"));
    }

    #[test]
    fn test_version_is_newer_major_dominates() {
        assert!(!version_is_newer("1.0.0", "0.99.99"));
    }

    #[test]
    fn test_version_is_newer_different_lengths() {
        assert!(version_is_newer("0.1", "0.1.1"));
        assert!(!version_is_newer("0.1.1", "0.1"));
    }

    #[test]
    fn test_version_is_newer_0_1_8_to_0_1_11() {
        // The actual upgrade path for this release
        assert!(version_is_newer("0.1.8", "0.1.11"));
        assert!(!version_is_newer("0.1.11", "0.1.8"));
    }

    #[test]
    fn test_prerelease_is_older_than_its_stable() {
        // Semver: 0.2.0-rc1 < 0.2.0. Someone running the rc must be offered the
        // final release — this was silently broken (the suffix parsed to 0).
        assert!(version_is_newer("0.2.0-rc1", "0.2.0"));
        assert!(!version_is_newer("0.2.0", "0.2.0-rc1"));
    }

    #[test]
    fn test_prerelease_ordering() {
        assert!(version_is_newer("1.0.0-alpha", "1.0.0-beta"));
        assert!(version_is_newer("1.0.0-rc1", "1.0.0-rc2"));
        assert!(!version_is_newer("1.0.0-rc2", "1.0.0-rc1"));
        assert!(!version_is_newer("1.0.0-rc1", "1.0.0-rc1"));
        // Numeric identifiers compare numerically, not lexicographically.
        assert!(version_is_newer("1.0.0-alpha.9", "1.0.0-alpha.10"));
        assert!(!version_is_newer("1.0.0-alpha.10", "1.0.0-alpha.9"));
        // Numeric identifiers have lower precedence than alphanumeric ones.
        assert!(version_is_newer("1.0.0-1", "1.0.0-alpha"));
    }

    #[test]
    fn test_prerelease_core_still_dominates() {
        assert!(version_is_newer("0.9.0", "1.0.0-beta"));
        assert!(!version_is_newer("1.0.0-beta", "0.9.0"));
    }

    #[test]
    fn test_build_metadata_is_ignored() {
        assert!(!version_is_newer("1.0.0", "1.0.0+build.7"));
        assert!(!version_is_newer("1.0.0+build.7", "1.0.0"));
        assert!(version_is_newer("1.0.0+build.7", "1.0.1"));
    }

    #[test]
    fn test_leading_v_and_whitespace_tolerated() {
        assert!(version_is_newer("v0.1.5", " 0.2.0 "));
        assert!(!version_is_newer("0.2.0", "v0.1.5"));
    }

    #[test]
    fn test_check_for_update_graceful_failure() {
        // When curl isn't available or network fails, should return None
        // We can't control the network in tests, but we can verify it doesn't panic
        let _result = check_for_update("0.1.0");
        // Just assert it doesn't panic — the result depends on network state
    }
}
