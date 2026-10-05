//! #994: a crates.io install compiles from the unpacked registry source, which
//! is not a git repo, so `git rev-parse` gives nothing and `yoyo --version`
//! used to print `dev`. `cargo publish` writes `.cargo_vcs_info.json` into the
//! package root; `build.rs` reads the commit from it. This pins the parser.
//!
//! `build.rs` is included as a module so the test exercises the real helper,
//! not a copy. Its `main` is unused here, hence the `dead_code` allow.

#[allow(dead_code)]
#[path = "../build.rs"]
mod build_script;

use build_script::short_sha_from_vcs_info;

/// The exact `.cargo_vcs_info.json` shipped in the yoyo-agent 0.2.0 crate
/// (the v0.2.0 tag commit), as quoted in #994.
const REAL_0_2_0: &str = r#"{
  "git": {
    "sha1": "365cc1214d7a3763da4aad6f10feb68750e1d1b7"
  },
  "path_in_vcs": ""
}"#;

#[test]
fn real_published_vcs_info_yields_the_short_sha() {
    assert_eq!(
        short_sha_from_vcs_info(REAL_0_2_0),
        Some("365cc12".to_string())
    );
}

#[test]
fn compact_single_line_form_also_parses() {
    let compact = r#"{"git":{"sha1":"365cc1214d7a3763da4aad6f10feb68750e1d1b7","dirty":true},"path_in_vcs":""}"#;
    assert_eq!(
        short_sha_from_vcs_info(compact),
        Some("365cc12".to_string())
    );
}

#[test]
fn unusable_vcs_info_yields_none() {
    let rows: &[(&str, &str)] = &[
        ("missing git key", r#"{ "path_in_vcs": "" }"#),
        ("empty", ""),
        ("truncated after key", r#"{ "git": { "sha1": "#),
        ("unterminated value", r#"{ "git": { "sha1": "365cc1214d7a"#),
        ("sha shorter than 7", r#"{ "git": { "sha1": "365cc1" } }"#),
        ("non-hex sha", r#"{ "git": { "sha1": "zzzzzzzzzz" } }"#),
        ("non-string sha", r#"{ "git": { "sha1": 12345678 } }"#),
        (
            "non-ASCII inside first 7",
            r#"{ "git": { "sha1": "36✓5cc1214d7a" } }"#,
        ),
    ];
    for (name, text) in rows {
        assert_eq!(short_sha_from_vcs_info(text), None, "row: {name}");
    }
}

#[test]
fn near_miss_exactly_seven_hex_chars_is_accepted() {
    assert_eq!(
        short_sha_from_vcs_info(r#"{ "git": { "sha1": "ABCdef0" } }"#),
        Some("ABCdef0".to_string())
    );
}
