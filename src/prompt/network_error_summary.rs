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
//! #1128 widened it to the other REPL-only words: the billing hint's
//! `/provider` and `/model`, and model-not-found's `Switch with: /model`.
//!
//! Not this module: how long a DNS failure is retried (#1120).

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

/// The billing hint's options (#1128). The REPL keeps its slash commands; a
/// single-shot run (`-p`, piped) cannot type them, so it is told the flags.
/// The REPL's local option is `/provider ollama`, not `/model ollama/...`
/// (#1139): `/model` keeps the exhausted provider and only swaps the id,
/// while `handle_provider_switch` sets the provider AND resets the model to
/// `default_model_for_provider("ollama")`, the same llama3.2 the flags name.
pub(crate) fn billing_options(provider: &str, single_shot: bool) -> String {
    let (switch, local) = if single_shot {
        ("--provider <name>", "--provider ollama --model llama3.2")
    } else {
        ("/provider <name>", "/provider ollama")
    };
    format!(
        "\n  Options:\n    \
         • Add credits or raise your spending limit in the {provider} dashboard\n    \
         • Switch to a different provider: {switch}\n    \
         • Use a local model: {local}"
    )
}

/// The model-not-found hint's last line (#1128): `/model` exists only in the REPL.
pub(crate) fn switch_model_line(id: &str, single_shot: bool) -> String {
    if single_shot {
        format!("\nSwitch with: --model {id}")
    } else {
        format!("\nSwitch with: /model {id} or --model {id}")
    }
}

/// `diagnose_api_error`, shaped for the surface this process is running on.
/// Every 💡 door goes through here, so no door shows REPL words on `-p`.
pub(super) fn surface_diagnosis(error: &str, model: &str) -> Option<String> {
    let single_shot = super::interrupt::is_single_shot();
    crate::prompt_retry::diagnose_api_error_for(error, model, single_shot)
        .map(|d| hint_for_surface(d, single_shot))
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
        let diag =
            crate::prompt_retry::diagnose_api_error(REFUSED, "claude-sonnet-4-20250514").unwrap();
        let expected = "Network error — could not reach the API.\n  \
             Check your internet connection and that anthropic's API is reachable.\n  \
             You can retry with /retry.";
        assert_eq!(diag, expected, "anti-vacuous: the REPL hint names /retry");
        assert_eq!(hint_for_surface(diag, false), expected);
    }

    #[test]
    fn single_shot_hint_drops_only_the_retry_line() {
        let diag =
            crate::prompt_retry::diagnose_api_error(REFUSED, "claude-sonnet-4-20250514").unwrap();
        assert_eq!(
            hint_for_surface(diag, true),
            "Network error — could not reach the API.\n  \
             Check your internet connection and that anthropic's API is reachable."
        );
    }

    // #1128: every hint `diagnose_api_error` can give, on both surfaces.
    const MODEL: &str = "claude-sonnet-4-20250514";
    const BILLING: &str = "402 Payment Required";
    const MISSING: &str = "model not found";
    const REPL_BILLING_OPTIONS: &str = "\n  Options:\n    \
        • Add credits or raise your spending limit in the anthropic dashboard\n    \
        • Switch to a different provider: /provider <name>\n    \
        • Use a local model: /provider ollama";

    fn for_surface(raw: &str, single_shot: bool) -> String {
        let d = crate::prompt_retry::diagnose_api_error_for(raw, MODEL, single_shot).unwrap();
        hint_for_surface(d, single_shot)
    }

    /// A slash command at a word start: ` /model`, `\n/x`, or the very start.
    fn has_slash_command(s: &str) -> bool {
        let b = s.as_bytes();
        (0..b.len().saturating_sub(1)).any(|i| {
            b[i] == b'/'
                && b[i + 1].is_ascii_lowercase()
                && (i == 0 || b[i - 1].is_ascii_whitespace())
        })
    }

    #[test]
    fn repl_billing_hint_names_a_command_that_switches_provider() {
        let d = for_surface(BILLING, false);
        assert!(d.ends_with(REPL_BILLING_OPTIONS), "{d}");
        assert_eq!(billing_options("anthropic", false), REPL_BILLING_OPTIONS);
        assert_eq!(
            crate::prompt_retry::diagnose_api_error(BILLING, MODEL).unwrap(),
            d
        );
        // #1139: `/model ollama/llama3` stayed on the exhausted provider.
        assert!(
            !d.contains("ollama/llama3") && !d.contains("/model ollama"),
            "{d}"
        );
    }

    /// #1139: the REPL hint's command really lands on a local model. The
    /// switch handler refuses providers outside KNOWN_PROVIDERS and otherwise
    /// resets the model to the provider default, which must be the model the
    /// single-shot hint names, so both surfaces send the user to one place.
    #[test]
    fn repl_local_option_resolves_to_the_single_shot_model() {
        assert!(crate::providers::KNOWN_PROVIDERS.contains(&"ollama"));
        let model = crate::providers::default_model_for_provider("ollama");
        assert_eq!(model, "llama3.2");
        assert!(billing_options("x", true).ends_with(&format!("--provider ollama --model {model}")));
    }

    #[test]
    fn single_shot_billing_hint_names_flags_not_slash_commands() {
        let d = for_surface(BILLING, true);
        assert!(
            d.ends_with(
                "\n  Options:\n    \
             • Add credits or raise your spending limit in the anthropic dashboard\n    \
             • Switch to a different provider: --provider <name>\n    \
             • Use a local model: --provider ollama --model llama3.2"
            ),
            "{d}"
        );
        assert!(!d.contains(" /provider") && !d.contains(" /model"), "{d}");
    }

    #[test]
    fn repl_model_switch_line_is_byte_identical() {
        let d = for_surface(MISSING, false);
        let known = crate::cli::known_models_for_provider("anthropic");
        assert!(
            !known.is_empty(),
            "anti-vacuous: the line is only built with a known list"
        );
        let line = format!("\nSwitch with: /model {0} or --model {0}", known[0]);
        assert!(d.ends_with(&line), "{d}");
    }

    #[test]
    fn single_shot_model_switch_line_names_only_the_flag() {
        let d = for_surface(MISSING, true);
        let known = crate::cli::known_models_for_provider("anthropic");
        assert!(
            d.ends_with(&format!("\nSwitch with: --model {}", known[0])),
            "{d}"
        );
        assert!(!d.contains(" /model"), "{d}");
    }

    #[test]
    fn single_shot_hints_carry_no_slash_command_and_repl_ones_still_do() {
        let fixtures = [
            ("401 Unauthorized", false),
            (BILLING, true),
            ("429 Too Many Requests", false),
            (MISSING, true),
            (REFUSED, true),
            ("403 Forbidden", false),
            ("stream closed unexpectedly", false),
        ];
        for (raw, repl_has_one) in fixtures {
            let shot = for_surface(raw, true);
            assert!(!has_slash_command(&shot), "{raw:?} -> {shot}");
            assert_eq!(
                has_slash_command(&for_surface(raw, false)),
                repl_has_one,
                "{raw:?}"
            );
        }
    }

    #[test]
    fn single_shot_leaves_other_hints_alone() {
        let d = "Rate limited by provider 'x'.\nConsider upgrading.".to_string();
        assert_eq!(hint_for_surface(d.clone(), true), d);
    }
}
