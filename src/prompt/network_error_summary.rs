//! #1115: how a network error reads once yoyo stops retrying it.
//!
//! Two defects, both on the `-p` / piped surface:
//! - the error line was reqwest's `Debug` dump (`reqwest::Error { kind: …,
//!   source: hyper_util::…(Connect, ConnectError("dns error", …)) }`), which
//!   buries the one fact a user needs (which URL, and what the OS said);
//! - the 💡 hint ended `You can retry with /retry.`, a REPL command that does
//!   not exist when yoyo was run with `-p`.
//!
//! The REPL keeps both its hint and its wording byte-identical. The full
//! `Debug` form is still printed, dimmed, under `--verbose`.
//!
//! Not this module: how long a DNS failure is retried (#1120), and the other
//! REPL-only words in non-network hints (`/provider`, `/model`).

use super::loopback_refused::dialed_url;
use crate::format::{DIM, RED, RESET};

/// The last line of `diagnose_api_error`'s network hint. One definition, so
/// the `-p` strip below cannot drift from the text it removes.
pub(crate) const REPL_RETRY_LINE: &str = "  You can retry with /retry.";

/// One line for a reqwest `Debug` dump: what failed, the URL, and the OS or
/// resolver reason. `None` for any other text, which the caller prints as is.
pub(crate) fn network_error_summary(raw: &str) -> Option<String> {
    let start = raw.find("reqwest::Error {")?;
    let (prefix, body) = raw.split_at(start); // `find` returns a char boundary
    let prefix = if prefix.trim().is_empty() {
        "Network error: "
    } else {
        prefix
    };
    let url = dialed_url(body)?;
    let reason = reason(body);
    let connect = body.contains("ConnectError(") || body.contains("Error(Connect");
    let what = if connect {
        format!("could not connect to {url}")
    } else {
        let kind = word_after(body, "kind: ").unwrap_or("request");
        format!("{} error from {url}", kind.to_lowercase())
    };
    let line = match reason {
        Some(r) => format!("{prefix}{what} ({r})"),
        None => format!("{prefix}{what}"),
    };
    Some(line.replace(['\n', '\r'], " "))
}

/// What the OS or resolver said: `message: "Connection refused"` (io::Error),
/// else `ConnectError("dns error", … error: "failed to lookup …")`.
fn reason(body: &str) -> Option<String> {
    if let Some(m) = quoted_after(body, "message: \"") {
        return Some(m.to_lowercase());
    }
    let label = quoted_after(body, "ConnectError(\"");
    let detail = quoted_after(body, "error: \"");
    match (label, detail) {
        (Some(l), Some(d)) => Some(format!("{l}: {d}")),
        (Some(l), None) => Some(l.to_string()),
        (None, Some(d)) => Some(d.to_string()),
        (None, None) => None,
    }
}

/// The text between `key` (which ends in an opening quote) and the next `"`.
fn quoted_after<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let rest = &s[s.find(key)? + key.len()..];
    rest.find('"').map(|end| &rest[..end])
}

/// The identifier after `key`, up to the next `,`, ` ` or `}`.
fn word_after<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let rest = &s[s.find(key)? + key.len()..];
    let word = rest.split([',', ' ', '}']).next()?;
    (!word.is_empty()).then_some(word)
}

/// The hint as it should read on this surface: the REPL's text unchanged; on a
/// single-shot run (`-p`, piped), without the `/retry` line.
pub(crate) fn hint_for_surface(diagnostic: String, single_shot: bool) -> String {
    if !single_shot {
        return diagnostic;
    }
    diagnostic.replace(&format!("\n{REPL_RETRY_LINE}"), "")
}

/// `diagnose_api_error`, shaped for the surface this process is running on.
pub(super) fn surface_diagnosis(error: &str, model: &str) -> Option<String> {
    super::diagnose_api_error(error, model)
        .map(|d| hint_for_surface(d, super::interrupt::is_single_shot()))
}

/// The `error:` line for a retriable error yoyo has stopped on.
pub(super) fn print_error_line(error_msg: &str) {
    match network_error_summary(error_msg) {
        Some(line) => {
            eprintln!("\n{RED}  error: {line}{RESET}");
            if crate::cli::is_verbose() {
                eprintln!("{DIM}  ({error_msg}){RESET}");
            }
        }
        None => eprintln!("\n{RED}  error: {error_msg}{RESET}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1115's raw text, verbatim (binary 38d48a9e).
    const REFUSED: &str = r#"Network error: reqwest::Error { kind: Request, url: "http://127.0.0.1:9/v1/chat/completions", source: hyper_util::client::legacy::Error(Connect, ConnectError("tcp connect error", 127.0.0.1:9, Os { code: 111, kind: ConnectionRefused, message: "Connection refused" })) }"#;
    /// Measured Day 247 on HEAD 721150bd+ with `--base-url http://nonexistent.invalid/v1`.
    const DNS: &str = r#"Network error: reqwest::Error { kind: Request, url: "http://nonexistent.invalid/v1/chat/completions", source: hyper_util::client::legacy::Error(Connect, ConnectError("dns error", Custom { kind: Uncategorized, error: "failed to lookup address information: Name or service not known" })) }"#;

    #[test]
    fn refused_becomes_one_line_with_url_and_os_reason() {
        assert_eq!(
            network_error_summary(REFUSED).as_deref(),
            Some(
                "Network error: could not connect to \
                 http://127.0.0.1:9/v1/chat/completions (connection refused)"
            )
        );
    }

    #[test]
    fn dns_failure_names_the_resolver_reason() {
        assert_eq!(
            network_error_summary(DNS).as_deref(),
            Some(
                "Network error: could not connect to \
                 http://nonexistent.invalid/v1/chat/completions \
                 (dns error: failed to lookup address information: Name or service not known)"
            )
        );
    }

    #[test]
    fn summary_holds_no_debug_dump() {
        for raw in [REFUSED, DNS] {
            assert!(
                raw.contains("reqwest::Error {"),
                "anti-vacuous: fixture is the dump"
            );
            let s = network_error_summary(raw).unwrap();
            assert!(!s.contains("reqwest::Error"), "{s}");
            assert!(!s.contains("hyper_util"), "{s}");
            assert!(!s.contains('\n'), "{s}");
        }
    }

    #[test]
    fn non_network_errors_pass_through_as_none() {
        for raw in [
            "HTTP 500: internal server error",
            "Network error: operation timed out",
            "",
            "Rate limited: 429 Too Many Requests",
        ] {
            assert_eq!(network_error_summary(raw), None, "{raw:?}");
        }
    }

    #[test]
    fn non_connect_reqwest_error_names_its_kind() {
        let raw = r#"Network error: reqwest::Error { kind: Decode, url: "https://api.example.com/v1/x", source: Eof }"#;
        assert_eq!(
            network_error_summary(raw).as_deref(),
            Some("Network error: decode error from https://api.example.com/v1/x")
        );
    }

    #[test]
    fn multibyte_url_does_not_panic_and_survives_whole() {
        let raw = r#"Network error: reqwest::Error { kind: Request, url: "http://ñandú.例え.jp/✓/v1", source: hyper_util::client::legacy::Error(Connect, ConnectError("dns error", Custom { kind: Uncategorized, error: "lookup ✓ failed" })) }"#;
        assert_eq!(
            network_error_summary(raw).as_deref(),
            Some(
                "Network error: could not connect to http://ñandú.例え.jp/✓/v1 \
                 (dns error: lookup ✓ failed)"
            )
        );
    }

    #[test]
    fn repl_hint_is_byte_identical() {
        let diag = super::super::diagnose_api_error(REFUSED, "claude-sonnet-4-20250514").unwrap();
        let expected = "Network error — could not reach the API.\n  \
             Check your internet connection and that anthropic's API is reachable.\n  \
             You can retry with /retry.";
        assert_eq!(diag, expected, "anti-vacuous: the REPL hint names /retry");
        assert_eq!(hint_for_surface(diag, false), expected);
    }

    #[test]
    fn single_shot_hint_drops_only_the_retry_line() {
        let diag = super::super::diagnose_api_error(REFUSED, "claude-sonnet-4-20250514").unwrap();
        assert_eq!(
            hint_for_surface(diag, true),
            "Network error — could not reach the API.\n  \
             Check your internet connection and that anthropic's API is reachable."
        );
    }

    #[test]
    fn single_shot_leaves_other_hints_alone() {
        let d = "Rate limited by provider 'x'.\nConsider upgrading.".to_string();
        assert_eq!(hint_for_surface(d.clone(), true), d);
    }
}
