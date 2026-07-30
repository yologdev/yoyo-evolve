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

/// Pull the `tag_name` string value out of a GitHub "latest release" JSON body.
///
/// Deliberately structural rather than an enumeration of separator spellings:
/// the previous version skipped the piece between the key and the value by
/// listing the shapes it expected to see (`":"`, `": "`), so any other spacing
/// (`":  "`, a newline before the value) made the *separator itself* the
/// "tag" — a confidently wrong string handed straight to the version
/// comparison. Here we require the JSON grammar instead: key, then `:`, then a
/// quoted string. Anything else — `null` (a repo with no releases), a number,
/// a missing key — is an explicit `None`, not a neighbouring token.
///
/// Not a JSON parser: a tag containing an escaped quote would be truncated.
/// Git refnames can't contain `"`, so that case doesn't exist in practice.
fn extract_tag_name(body: &str) -> Option<&str> {
    let after_key = body.split("\"tag_name\"").nth(1)?;
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let value_start = after_colon.trim_start().strip_prefix('"')?;
    let (value, _) = value_start.split_once('"')?;
    if value.is_empty() {
        None
    } else {
        Some(value)
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
    let tag = extract_tag_name(&body)?;

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

    /// Verbatim excerpt of `curl -sf
    /// https://api.github.com/repos/yologdev/yoyo-evolve/releases/latest`
    /// captured 2026-07-30. Captured, not authored: a fixture I write myself
    /// only proves the parser agrees with my belief about the API's shape.
    const GITHUB_LATEST_RELEASE_VERBATIM: &str = r#"{
  "url": "https://api.github.com/repos/yologdev/yoyo-evolve/releases/351953743",
  "html_url": "https://github.com/yologdev/yoyo-evolve/releases/tag/v0.1.15",
  "id": 351953743,
  "author": {
    "login": "github-actions[bot]",
    "id": 41898282,
    "site_admin": false
  },
  "node_id": "RE_kwDORbb9zc4U-mNP",
  "tag_name": "v0.1.15",
  "target_commitish": "main",
  "name": "v0.1.15",
  "draft": false,
  "prerelease": false
}"#;

    #[test]
    fn test_extract_tag_name_verbatim_github_response() {
        assert_eq!(
            extract_tag_name(GITHUB_LATEST_RELEASE_VERBATIM),
            Some("v0.1.15")
        );
    }

    #[test]
    fn test_extract_tag_name_shape_table() {
        // The enumeration of shapes this parser must forgive or refuse, written
        // down where it fails loudly instead of living in my confidence.
        let cases: &[(&str, Option<&str>, &str)] = &[
            (
                r#"{"tag_name": "v1.2.3"}"#,
                Some("v1.2.3"),
                "pretty-printed (one space) — the shape GitHub actually returns",
            ),
            (
                r#"{"tag_name":"v1.2.3"}"#,
                Some("v1.2.3"),
                "compact, no space after the colon",
            ),
            (
                r#"{"tag_name":  "v1.2.3"}"#,
                Some("v1.2.3"),
                "two spaces — the old separator enumeration returned \":  \" here",
            ),
            (
                "{\"tag_name\":\n    \"v1.2.3\"}",
                Some("v1.2.3"),
                "newline between colon and value",
            ),
            (
                "{\"tag_name\"  :  \"v1.2.3\"}",
                Some("v1.2.3"),
                "whitespace on both sides of the colon",
            ),
            (
                r#"{"tag_name": "0.1.15"}"#,
                Some("0.1.15"),
                "tag without a leading v",
            ),
            (
                r#"{"tag_name": null, "name": "nothing"}"#,
                None,
                "no releases yet — null is an absent tag, not the next token",
            ),
            (
                r#"{"tag_name": "", "name": "empty"}"#,
                None,
                "empty string is not a version",
            ),
            (
                r#"{"message": "API rate limit exceeded", "status": "403"}"#,
                None,
                "GitHub error body — key absent entirely",
            ),
            (
                r#"{"tag_name"}"#,
                None,
                "key with no colon (truncated body)",
            ),
            (
                r#"{"tag_name": "#,
                None,
                "body cut off right after the colon",
            ),
            (
                r#"{"tag_name": "v1.2.3"#,
                None,
                "unterminated value — no closing quote",
            ),
            ("", None, "empty body"),
        ];
        for (body, want, why) in cases {
            assert_eq!(extract_tag_name(body), *want, "{why} (body: {body:?})");
        }
    }

    #[test]
    fn test_extract_tag_name_never_yields_a_separator() {
        // Regression guard for the actual defect: the old parser handed the
        // punctuation between key and value to version_is_newer as if it were
        // a version. Whatever comes back must never start with a colon.
        for body in [
            r#"{"tag_name":  "v9.9.9"}"#,
            "{\"tag_name\":\n\t\"v9.9.9\"}",
            r#"{"tag_name":   "v9.9.9"}"#,
        ] {
            let got = extract_tag_name(body);
            assert_eq!(got, Some("v9.9.9"), "body: {body:?}");
            assert!(
                !got.unwrap().starts_with(':'),
                "parser returned a separator as a tag: {got:?}"
            );
        }
    }
}
