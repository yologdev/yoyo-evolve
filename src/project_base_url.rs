//! #1179: a project-local `.yoyo.toml`'s `base_url` is a trust grant.
//!
//! `base_url` decides which host receives the user's API key. A repository's
//! `.yoyo.toml` is written by whoever wrote the repository, so before #1179
//! cloning a repo and running `yoyo -p hi` in it sent the user's key to the repo
//! author's server, with no trust check and no message. Every other project-local
//! grant (MCP, `permissions.allow`, hooks, `notify_command`, skills) was already
//! gated by trust in `cli.rs`; this is the same gate for the one config key that
//! names an endpoint a credential is sent to.
//!
//! Census of the readers (Day 256): `base_url` is read from the loaded config in
//! exactly two places, `cli::parse_model_config` (the main path: `-p`, REPL,
//! piped, and every agent built from its `Config`: side, sub, fallback, `/provider`)
//! and `dispatch_sub::run_review_subcommand` (`yoyo review`, which runs before
//! the main path's trust decision). Both now resolve through [`resolve_base_url`].
//!
//! The rule, by who reaches it:
//! - `--base-url` on the command line, or `base_url` in a user-level config: the
//!   user's own choice, used as before.
//! - A project-local `base_url` in a trusted run (`--trust-project`,
//!   `--trust-project-always`, the remembered trust store, or a yes at the trust
//!   prompt): used, and named on stderr every run (#1188): trust is per
//!   directory and never re-checked, so a pulled change to `base_url` would
//!   otherwise move the key to a new host in silence. Detection, not prevention.
//! - A project-local `base_url` in an untrusted run, provider with an endpoint
//!   of its own (anthropic, openai, ollama, ...): **withheld** with a warning; the
//!   request goes to that provider's own endpoint, which is where the user's key
//!   for that provider belongs.
//! - The same, provider `custom` or a name yoyo does not know: **refused**, exit 2,
//!   nothing sent. Such a provider has no endpoint of its own; withholding the URL
//!   would send the key to `localhost:8080`, a host nobody chose.

use std::collections::HashMap;

/// What the gate decided for one run.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BaseUrlGateOutcome {
    /// The URL to use, or `None` for the provider's own endpoint.
    pub base_url: Option<String>,
    /// The project file's `base_url` that was NOT used. `None` when nothing was
    /// withheld, which is every run without a project-local `base_url`.
    pub withheld: Option<String>,
    /// #1188: `base_url` is the project file's, used because the run is
    /// trusted. Trust is per directory and never re-checked, so a `git pull`
    /// can change this host silently; the caller names it on every such run.
    pub from_trusted_project: bool,
}

/// Pure gate. `flag_url` is `--base-url`; `file_url` is the loaded config's
/// `base_url`, whose provenance is `project_local`.
pub(crate) fn gate_project_base_url(
    flag_url: Option<String>,
    file_url: Option<String>,
    project_local: bool,
    trusted: bool,
) -> BaseUrlGateOutcome {
    if flag_url.is_some() {
        // The user's own flag wins over any file, so the file's URL is not used
        // for reasons that have nothing to do with trust: nothing to announce.
        return BaseUrlGateOutcome {
            base_url: flag_url,
            withheld: None,
            from_trusted_project: false,
        };
    }
    // A blank value names no host, so there is nothing to withhold.
    let names_a_host = file_url.as_deref().is_some_and(|u| !u.trim().is_empty());
    if !project_local || trusted || !names_a_host {
        return BaseUrlGateOutcome {
            base_url: file_url,
            withheld: None,
            from_trusted_project: project_local && trusted && names_a_host,
        };
    }
    BaseUrlGateOutcome {
        base_url: None,
        withheld: file_url,
        from_trusted_project: false,
    }
}

/// Does `provider` have an endpoint of its own to fall back to when the
/// project's URL is withheld? `custom` and names yoyo does not know do not:
/// their only default is `localhost:8080`, a host nobody chose.
pub(crate) fn provider_has_own_endpoint(provider: &str) -> bool {
    provider != "custom" && crate::providers::KNOWN_PROVIDERS.contains(&provider)
}

/// The stderr block for a withheld project `base_url`. Pure and ANSI-free (the
/// caller colours it); `refused` picks the exit-2 wording.
pub(crate) fn project_base_url_message(
    url: &str,
    config_file: &str,
    provider: &str,
    refused: bool,
) -> String {
    let url = crate::cli::sanitize_for_display(url);
    let config_file = crate::cli::sanitize_for_display(config_file);
    let provider = crate::cli::sanitize_for_display(provider);
    let outcome = if refused {
        format!(
            "Provider '{provider}' has no endpoint of its own to use instead, so yoyo stopped.\n  \
Nothing was sent."
        )
    } else {
        format!("It was not used: provider '{provider}' uses its own endpoint instead.")
    };
    format!(
        "A project-local config ({config_file}) sets the host your API key is sent to:\n    \
base_url = {url}\n  This config came with the project, not from you. {outcome}\n  \
Re-run with --trust-project to use it this session, or pass --base-url <url> yourself."
    )
}

/// #1188: the note for a trusted project's `base_url`, naming where the key
/// goes. Only scheme, host and port are shown: userinfo, path and query can
/// carry a token. A URL with no readable host is reported without its value.
pub(crate) fn trusted_project_base_url_note(
    provider: &str,
    url: &str,
    config_file: &str,
) -> String {
    let provider = crate::cli::sanitize_for_display(provider);
    let config_file = crate::cli::sanitize_for_display(config_file);
    let parsed = url::Url::parse(url.trim()).ok();
    let origin = parsed.as_ref().and_then(|u| {
        let host = u.host_str()?;
        Some(match u.port_or_known_default() {
            Some(port) => format!("{}://{host}:{port}", u.scheme()),
            None => format!("{}://{host}", u.scheme()),
        })
    });
    match origin {
        Some(origin) => format!(
            "requests for {provider} (with your API key) go to {origin}, set by base_url in {config_file}"
        ),
        None => format!(
            "requests for {provider} (with your API key) go to the base_url set in {config_file}, \
which yoyo could not read a host from (value not shown)"
        ),
    }
}

/// Resolve the run's base URL through the gate, printing the notice and
/// exiting 2 on a refusal. `trusted` is the caller's trust reading.
pub(crate) fn resolve_base_url(
    args: &[String],
    file_config: &HashMap<String, String>,
    provider: &str,
    trusted: bool,
) -> Option<String> {
    use crate::format::{RED, RESET, YELLOW};
    let gated = gate_project_base_url(
        crate::dispatch_sub::flag_value(args, &["--base-url"]),
        file_config.get("base_url").cloned(),
        crate::config::loaded_config_is_project_local(),
        trusted,
    );
    let config_file = || {
        crate::config::loaded_config_resolved_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| ".yoyo.toml".to_string())
    };
    let Some(url) = gated.withheld else {
        if gated.from_trusted_project {
            if let Some(used) = gated.base_url.as_deref() {
                // Same policy as the withheld notice: not silenced by --quiet,
                // because it says where the user's key goes.
                let note = trusted_project_base_url_note(provider, used, &config_file());
                eprintln!("{YELLOW}note:{RESET} {note}");
            }
        }
        return gated.base_url;
    };
    let config_file = config_file();
    let refused = !provider_has_own_endpoint(provider);
    let msg = project_base_url_message(&url, &config_file, provider, refused);
    // Not silenced by --quiet: it says where the user's key goes (or that the
    // run stopped), which is not informational chatter.
    if refused {
        eprintln!("{RED}error:{RESET} {msg}");
        crate::dispatch_sub::exit_process(2);
    }
    eprintln!("{YELLOW}warning:{RESET} {msg}");
    gated.base_url
}

/// Trust for doors that run before `parse_args_with_config`'s trust decision
/// (`yoyo review`): the flags, or the remembered store for the cwd. The
/// interactive prompt is not asked here; a non-trusted run withholds.
pub(crate) fn early_door_trusted(args: &[String]) -> bool {
    crate::cli::is_trust_project()
        || crate::cli_config::has_option(args, &["--trust-project", "--trust-project-always"])
        || std::env::current_dir()
            .ok()
            .is_some_and(|d| crate::config_paths::dir_is_trusted(&d))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    #[test]
    fn gate_table() {
        let u = "http://evil.example/v1";
        // (flag, file, project_local, trusted, expected base_url, expected withheld)
        type Row = (
            Option<String>,
            Option<String>,
            bool,
            bool,
            Option<String>,
            Option<String>,
            bool,
        );
        let rows: Vec<Row> = vec![
            // The defect: untrusted project file URL is withheld.
            (None, s(u), true, false, None, s(u), false),
            // Trusted project: used, and named (#1188).
            (None, s(u), true, true, s(u), None, true),
            // User-level file: used, trusted or not.
            (None, s(u), false, false, s(u), None, false),
            // User-level file in a trusted dir: the user's own, not named.
            (None, s(u), false, true, s(u), None, false),
            // The flag always wins, and nothing is announced about the file.
            (
                s("http://mine/v1"),
                s(u),
                true,
                false,
                s("http://mine/v1"),
                None,
                false,
            ),
            (
                s("http://mine/v1"),
                None,
                true,
                false,
                s("http://mine/v1"),
                None,
                false,
            ),
            // The flag in a trusted project: the user typed it, not named.
            (
                s("http://mine/v1"),
                s(u),
                true,
                true,
                s("http://mine/v1"),
                None,
                false,
            ),
            // Nothing set: nothing to do.
            (None, None, true, false, None, None, false),
            (None, None, true, true, None, None, false),
            // A blank value names no host: passed through, not announced.
            (None, s("  "), true, false, s("  "), None, false),
            (None, s("  "), true, true, s("  "), None, false),
        ];
        for (flag, file, local, trusted, want_url, want_withheld, want_named) in rows {
            let got = gate_project_base_url(flag.clone(), file.clone(), local, trusted);
            assert_eq!(
                got,
                BaseUrlGateOutcome {
                    base_url: want_url,
                    withheld: want_withheld,
                    from_trusted_project: want_named,
                },
                "flag={flag:?} file={file:?} local={local} trusted={trusted}"
            );
        }
    }

    #[test]
    fn own_endpoint_split() {
        for p in ["anthropic", "openai", "ollama", "bedrock", "openrouter"] {
            assert!(provider_has_own_endpoint(p), "{p}");
        }
        for p in ["custom", "zzzz", "my-proxy"] {
            assert!(!provider_has_own_endpoint(p), "{p}");
        }
    }

    #[test]
    fn message_names_key_file_and_both_remedies_and_escapes_control_bytes() {
        for refused in [true, false] {
            let m = project_base_url_message(
                "http://x\u{1b}[31m/v1",
                "/repo/.yoyo.toml",
                "custom",
                refused,
            );
            assert!(m.contains("base_url = http://x"), "{m}");
            assert!(m.contains("/repo/.yoyo.toml"), "{m}");
            assert!(m.contains("--trust-project"), "{m}");
            assert!(m.contains("--base-url <url>"), "{m}");
            assert!(!m.as_bytes().contains(&0x1b), "{m:?}");
            assert_eq!(m.contains("Nothing was sent."), refused, "{m}");
            assert_eq!(m.contains("It was not used"), !refused, "{m}");
        }
    }

    #[test]
    fn trusted_note_names_scheme_host_and_port() {
        assert_eq!(
            trusted_project_base_url_note(
                "custom",
                "https://proxy.example:8443/v1",
                "/p/.yoyo.toml"
            ),
            "requests for custom (with your API key) go to https://proxy.example:8443, \
set by base_url in /p/.yoyo.toml"
        );
        // The default port is spelled out, so the host line always has one shape.
        assert_eq!(
            trusted_project_base_url_note("openai", "https://api.example/v1", "./.yoyo.toml"),
            "requests for openai (with your API key) go to https://api.example:443, \
set by base_url in ./.yoyo.toml"
        );
    }

    #[test]
    fn trusted_note_never_shows_userinfo_path_or_query() {
        let url = "http://user:s3cret@evil.example:8080/v1/sk-path?key=sk-query#frag";
        let n = trusted_project_base_url_note("custom", url, "/p/.yoyo.toml");
        assert_eq!(
            n,
            "requests for custom (with your API key) go to http://evil.example:8080, \
set by base_url in /p/.yoyo.toml"
        );
        for secret in ["user", "s3cret", "sk-path", "sk-query", "key=", "frag"] {
            assert!(!n.contains(secret), "{secret} leaked: {n}");
        }
    }

    #[test]
    fn trusted_note_without_a_readable_host_withholds_the_value() {
        for bad in ["not a url sk-raw", "mailto:sk-raw@x", "http://[sk-raw"] {
            let n = trusted_project_base_url_note("custom", bad, "/p/.yoyo.toml");
            assert_eq!(
                n,
                "requests for custom (with your API key) go to the base_url set in /p/.yoyo.toml, \
which yoyo could not read a host from (value not shown)",
                "{bad}"
            );
        }
    }

    #[test]
    fn trusted_note_escapes_control_bytes_in_provider_and_path() {
        let n = trusted_project_base_url_note("cu\u{1b}[2Jstom", "http://h/v1", "/p\n/.yoyo.toml");
        assert!(!n.as_bytes().contains(&0x1b), "{n:?}");
        assert!(!n.contains('\n'), "{n:?}");
    }
}
