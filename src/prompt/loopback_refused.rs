//! A refused connection to a loopback server stops retrying (Day 247).
//!
//! `yoyo --provider ollama -p hi` with nothing on :11434 used to print
//! "retrying (attempt 2/6 … 6/6)" and exit after ~70s. On loopback, "connection
//! refused" means no process is bound to that port on this machine, and a
//! backoff will not start one, so waiting is pure cost.
//!
//! **yoagent already retries underneath.** Its agent loop retries every
//! `ProviderError::Network` with its default `RetryConfig` (3 retries, 1s/2s/4s
//! with jitter, yoyo never overrides it), so the one refused error that reaches
//! yoyo has already been 4 connects over ~7s. That window is the grace for a
//! local server that is still starting, and it is why yoyo stops on the first
//! refused error it sees instead of spending its own extra attempt.
//!
//! **The URL comes from the error, not from yoyo's config.** reqwest's error
//! carries the URL it actually dialed (`url: "http://…"`), which is right for
//! ollama's default, `--base-url`, a `/provider` switch and a fallback alike,
//! where a startup copy of the config would go stale.
//!
//! **Remote refusals keep the old path.** A refused connection to a remote host
//! can be a deploy or a load balancer rotating, and there the retry works.

/// The URL reqwest dialed, read from the error text: the Debug form
/// (`url: "http://…"`) or the Display form (`for url (http://…)`).
pub(crate) fn dialed_url(err: &str) -> Option<&str> {
    if let Some(i) = err.find("url: \"") {
        let rest = &err[i + "url: \"".len()..];
        return rest.find('"').map(|end| &rest[..end]);
    }
    if let Some(i) = err.find("for url (") {
        let rest = &err[i + "for url (".len()..];
        return rest.find(')').map(|end| &rest[..end]);
    }
    None
}

/// The host part of `url` (brackets kept off an IPv6 literal), if it parses.
fn url_host(url: &str) -> Option<&str> {
    let after_scheme = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = after_scheme.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split_once(']').map(|(h, _)| h);
    }
    Some(authority.split(':').next().unwrap_or(authority))
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// The OS said "refused": no listener. Not a timeout, DNS failure or reset.
fn says_refused(err: &str) -> bool {
    let lower = err.to_lowercase();
    // "Connection refused" (Linux os error 111, macOS 61), io::ErrorKind's
    // Debug spelling, and Windows' 10061 wording.
    lower.contains("connection refused")
        || lower.contains("connectionrefused")
        || lower.contains("actively refused")
}

/// True only when `base_url`'s host is loopback AND the error says refused.
pub(crate) fn loopback_refused(base_url: Option<&str>, err: &str) -> bool {
    base_url.and_then(url_host).is_some_and(is_loopback_host) && says_refused(err)
}

/// What the user sees instead of the remaining backoff. No REPL wording.
pub(crate) fn loopback_refused_message(url: &str) -> String {
    let mut msg =
        format!("connection refused at {url}: nothing is listening there. Is the server running?");
    if url_host_port(url) == Some("11434") {
        msg.push_str(" (for Ollama: `ollama serve`)");
    }
    msg
}

fn url_host_port(url: &str) -> Option<&str> {
    let after_scheme = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = after_scheme.split(['/', '?', '#']).next()?;
    let tail = authority.rsplit_once(']').map_or(authority, |(_, t)| t);
    tail.rsplit_once(':').map(|(_, p)| p)
}

/// The message to print and stop on, or `None` for every other error.
pub(crate) fn loopback_refused_note(err: &str) -> Option<String> {
    let url = dialed_url(err)?;
    loopback_refused(Some(url), err).then(|| loopback_refused_message(url))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real error text measured on Day 247 (binary 38d48a9e).
    const REAL: &str = r#"Network error: reqwest::Error { kind: Request, url: "http://127.0.0.1:9/v1/chat/completions", source: hyper_util::client::legacy::Error(Connect, ConnectError("tcp connect error", 127.0.0.1:9, Os { code: 111, kind: ConnectionRefused, message: "Connection refused" })) }"#;

    #[test]
    fn loopback_refused_table() {
        let refused = "error sending request: Connection refused (os error 111)";
        let rows: &[(Option<&str>, &str, bool)] = &[
            (Some("http://localhost:11434/v1"), refused, true),
            (Some("http://127.0.0.1:9/v1"), refused, true),
            (Some("http://127.4.5.6:8080"), refused, true),
            (Some("http://[::1]:1234/v1"), refused, true),
            (
                Some("http://LOCALHOST:8080/v1"),
                "kind: ConnectionRefused",
                true,
            ),
            (
                Some("http://localhost:8080"),
                "os error 10061: target machine actively refused it",
                true,
            ),
            // near-miss: remote refusal, where the retry can work (d216)
            (Some("https://api.example.com/v1"), refused, false),
            (Some("http://10.0.0.5:11434/v1"), refused, false),
            // near-miss: loopback, but not refused
            (
                Some("http://localhost:11434/v1"),
                "operation timed out",
                false,
            ),
            (
                Some("http://127.0.0.1:9/v1"),
                "503 service unavailable",
                false,
            ),
            (
                Some("http://127.0.0.1:9/v1"),
                "connection reset by peer",
                false,
            ),
            // substring is not a host
            (Some("http://localhost.example.com/v1"), refused, false),
            (Some("http://user@notlocalhost:80"), refused, false),
            (None, refused, false),
        ];
        for (url, err, want) in rows {
            assert_eq!(loopback_refused(*url, err), *want, "{url:?} / {err}");
        }
    }

    #[test]
    fn dialed_url_reads_both_reqwest_forms() {
        assert_eq!(
            dialed_url(REAL),
            Some("http://127.0.0.1:9/v1/chat/completions")
        );
        assert_eq!(
            dialed_url("error sending request for url (http://localhost:11434/v1/x)"),
            Some("http://localhost:11434/v1/x")
        );
        assert_eq!(dialed_url("Connection refused"), None);
    }

    #[test]
    fn note_on_the_real_error_names_the_url() {
        assert_eq!(
            loopback_refused_note(REAL).as_deref(),
            Some(
                "connection refused at http://127.0.0.1:9/v1/chat/completions: \
                 nothing is listening there. Is the server running?"
            )
        );
        // Anti-vacuous: the remote twin of the very same error stays on the old path.
        let remote = REAL.replace("http://127.0.0.1:9", "https://api.example.com");
        assert!(remote.contains("api.example.com") && remote.contains("Connection refused"));
        assert_eq!(loopback_refused_note(&remote), None);
    }

    #[test]
    fn message_full_strings() {
        assert_eq!(
            loopback_refused_message("http://localhost:11434/v1/chat/completions"),
            "connection refused at http://localhost:11434/v1/chat/completions: nothing is \
             listening there. Is the server running? (for Ollama: `ollama serve`)"
        );
        let m = loopback_refused_message("http://[::1]:1234/v1");
        assert_eq!(
            m,
            "connection refused at http://[::1]:1234/v1: nothing is listening there. \
             Is the server running?"
        );
        assert!(!m.contains("/retry"));
    }
}
