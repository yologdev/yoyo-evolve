//! A DNS failure on an RFC 6761 reserved name stops retrying (Day 248, #1120).
//!
//! **Why.** `.invalid`, `.test`, `.example` and `.localhost` are reserved by
//! RFC 6761: no public resolver will ever answer for them, so a DNS failure on
//! one is permanent and every backoff retry is wasted time (measured Day 248:
//! `--base-url http://nonexistent.invalid/v1` spent 1m04s on six attempts
//! before exiting 1). Such a name is almost always a placeholder that was
//! never replaced, so the useful answer is "check --base-url", at once.
//!
//! **Every other DNS failure keeps retrying.** NXDOMAIN or SERVFAIL on an
//! ordinary name can be a transient resolver outage (a VPN reconnecting, a
//! flaky home router), and there the retry works. That split is @yuanhao's
//! decision on #1120; do not widen this to all NXDOMAIN.
//!
//! A bare `localhost` counts too: it normally resolves, so a DNS *error* on it
//! means a broken hosts setup, which retrying will not repair either.

use super::loopback_refused::{dialed_url, loopback_refused_note, url_host};

/// The RFC 6761 special-use names that never resolve on the public DNS.
const RESERVED_TLDS: [&str; 4] = ["invalid", "test", "example", "localhost"];

/// True when `host` is a reserved TLD or a name under one, matched on a label
/// boundary, case-insensitively, with a trailing root dot tolerated.
fn is_reserved_host(host: &str) -> bool {
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    RESERVED_TLDS.iter().any(|tld| {
        host == *tld
            || host
                .strip_suffix(tld)
                .is_some_and(|head| head.ends_with('.'))
    })
}

/// The error says the name could not be resolved (not refused, not a timeout).
fn says_dns_failure(err: &str) -> bool {
    let lower = err.to_lowercase();
    // hyper's `ConnectError("dns error", …)`, glibc's getaddrinfo text, macOS'
    // and Windows' wordings.
    lower.contains("dns error")
        || lower.contains("failed to lookup address")
        || lower.contains("name or service not known")
        || lower.contains("nodename nor servname")
        || lower.contains("no such host is known")
}

/// The message to print and stop on, or `None` for every other error.
pub(crate) fn reserved_dns_note(err: &str) -> Option<String> {
    let url = dialed_url(err)?;
    let host = url_host(url)?;
    (says_dns_failure(err) && is_reserved_host(host)).then(|| {
        format!(
            "could not resolve {host}: it is a reserved name (RFC 6761) that can never \
             resolve, so retrying cannot help. Check --base-url (now {url})."
        )
    })
}

/// The one fast-fail check both retry loops in `prompt.rs` consult: a refused
/// loopback connection first, then a DNS failure on a reserved name.
pub(crate) fn no_retry_note(err: &str) -> Option<String> {
    loopback_refused_note(err).or_else(|| reserved_dns_note(err))
}

/// The real error text measured on Day 248 (`--verbose`, built binary).
/// Shared with `prompt_retry_dns_tests` (#1126) so neither retypes it.
#[cfg(test)]
pub(crate) const REAL: &str = r#"Network error: reqwest::Error { kind: Request, url: "http://nonexistent.invalid/v1/chat/completions", source: hyper_util::client::legacy::Error(Connect, ConnectError("dns error", Custom { kind: Uncategorized, error: "failed to lookup address information: Name or service not known" })) }"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn with_host(host: &str) -> String {
        let out = REAL.replace("nonexistent.invalid", host);
        assert!(out.contains(host), "fixture must carry {host}"); // anti-vacuous
        out
    }

    #[test]
    fn reserved_dns_table() {
        let refused = r#"Network error: reqwest::Error { kind: Request, url: "http://foo.test/v1", source: ConnectError("tcp connect error", Os { code: 111, kind: ConnectionRefused, message: "Connection refused" }) }"#;
        let timeout = r#"Network error: reqwest::Error { kind: Request, url: "http://api.example/v1", source: TimedOut, "operation timed out" }"#;
        let rows: &[(String, bool)] = &[
            (REAL.to_string(), true),
            (with_host("api.example"), true),
            (with_host("foo.test"), true),
            (with_host("x.localhost"), true),
            (with_host("localhost"), true),
            (with_host("Api.EXAMPLE."), true),
            (with_host("invalid"), true),
            // the owner's second required row: ordinary NXDOMAIN keeps retrying
            (with_host("yoyo-no-such-host-1120.com"), false),
            // a real domain, not the `.example` TLD
            (with_host("example.com"), false),
            // suffix must sit on a label boundary
            (with_host("invalidator.dev"), false),
            (with_host("notinvalid"), false),
            (with_host("mytest"), false),
            // reserved host, but not a DNS failure
            (refused.to_string(), false),
            (timeout.to_string(), false),
            // no URL in the error
            (
                "dns error: failed to lookup address information".to_string(),
                false,
            ),
        ];
        for (err, want) in rows {
            assert_eq!(reserved_dns_note(err).is_some(), *want, "{err}");
        }
    }

    #[test]
    fn note_on_the_real_error_full_string() {
        assert_eq!(
            reserved_dns_note(REAL).as_deref(),
            Some(
                "could not resolve nonexistent.invalid: it is a reserved name (RFC 6761) \
                 that can never resolve, so retrying cannot help. Check --base-url \
                 (now http://nonexistent.invalid/v1/chat/completions)."
            )
        );
    }

    #[test]
    fn no_retry_note_chains_loopback_then_reserved() {
        assert_eq!(no_retry_note(REAL), reserved_dns_note(REAL));
        assert!(no_retry_note(REAL).is_some());
        let refused = r#"Network error: reqwest::Error { kind: Request, url: "http://127.0.0.1:9/v1", source: ConnectError("tcp connect error", Os { code: 111, kind: ConnectionRefused, message: "Connection refused" }) }"#;
        assert_eq!(no_retry_note(refused), loopback_refused_note(refused));
        assert!(no_retry_note(refused).is_some());
        // near-miss: ordinary NXDOMAIN reaches neither check and keeps retrying
        assert_eq!(
            no_retry_note(&with_host("yoyo-no-such-host-1120.com")),
            None
        );
    }

    #[test]
    fn other_dns_wordings_count_as_dns_failures() {
        for w in [
            "dns error",
            "nodename nor servname provided, or not known",
            "No such host is known. (os error 11001)",
        ] {
            let err = format!(r#"error {{ url: "https://foo.test/v1", source: "{w}" }}"#);
            assert!(reserved_dns_note(&err).is_some(), "{w}");
        }
    }
}
