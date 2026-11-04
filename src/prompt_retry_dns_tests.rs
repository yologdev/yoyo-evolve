//! #1126: `diagnose_api_error`'s network branch must recognise a DNS failure
//! in the text reqwest really produces. Before this, it matched only
//! "dns resolution failed" / "name resolution failed", which the real error
//! never contains, so an ordinary DNS failure ended with no 💡 hint at all.
//! The real text is reused from `reserved_dns::REAL` (captured on Day 248
//! with --verbose), never retyped, so the fixture cannot agree with itself.

use super::diagnose_api_error;
use crate::prompt::reserved_dns::REAL;

const MODEL: &str = "claude-sonnet-4-20250514";
/// What the network branch says today, written out from the intent.
const NETWORK_HINT: &str = "Network error — could not reach the API.\n  \
     Check your internet connection and that anthropic's API is reachable.\n  \
     You can retry with /retry.";

#[test]
fn real_reqwest_dns_error_gets_the_network_hint() {
    // Anti-vacuous: the fixture really is the shape the old matcher missed.
    assert!(REAL.contains("dns error") && !REAL.contains("dns resolution failed"));
    assert_eq!(
        diagnose_api_error(REAL, MODEL).as_deref(),
        Some(NETWORK_HINT)
    );
}

#[test]
fn other_resolver_wordings_get_the_network_hint() {
    for err in [
        // macOS getaddrinfo wording
        "failed to lookup address information: nodename nor servname provided, or not known",
        // glibc EAI_AGAIN
        "Temporary failure in name resolution",
        // the pre-existing spellings still match
        "dns resolution failed",
        "name resolution failed",
    ] {
        assert_eq!(
            diagnose_api_error(err, MODEL).as_deref(),
            Some(NETWORK_HINT),
            "{err}"
        );
    }
}

#[test]
fn near_misses_keep_what_they_got_before() {
    // A non-DNS connect error: the same network hint as before, byte for byte.
    let refused = diagnose_api_error("tcp connect error: Connection refused", MODEL);
    assert_eq!(refused.as_deref(), Some(NETWORK_HINT));
    // "resolution" on its own is not a resolver failure.
    let image = diagnose_api_error("image resolution too large", MODEL);
    assert!(
        !image.as_deref().unwrap_or("").contains("Network error"),
        "{image:?}"
    );
}
