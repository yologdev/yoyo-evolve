//! `/doctor`'s API row (#1242): one minimal request, sent through yoagent's
//! own provider for the session's provider, model and base URL, so a rejected
//! key, a model or endpoint the vendor does not serve, or a local server that
//! is not running shows up in the report instead of on the first real prompt.
//!
//! The request goes where a real run would send it: the `ModelConfig` comes
//! from `agent_builder::create_model_config` (the run's own builder, headers
//! included) and the provider from `ProviderRegistry::default()`, which picks
//! it by the config's protocol, the same rule the run's builder follows. No
//! retries: yoagent retries in the agent loop, and this calls the provider
//! directly, so one check is one request.

use crate::commands_dev::{DoctorCheck, DoctorStatus};
use std::time::Duration;
use yoagent::provider::{ModelConfig, ProviderError, ProviderRegistry, StreamConfig};
use yoagent::types::{Message, StopReason};

/// How long doctor waits for the provider's answer before calling it silent.
pub const API_CHECK_TIMEOUT: Duration = Duration::from_secs(20);

/// The reply budget of the check request. 16, not 1: OpenAI's Responses API
/// refuses `max_output_tokens` below 16, and a refusal of the probe's own
/// shape would read as a broken setup.
const PROBE_MAX_TOKENS: u32 = 16;

/// Bytes of a provider's error text kept in the row.
const DETAIL_MAX_BYTES: usize = 200;

/// What the API row needs from the session: the same four values a run uses.
pub struct ApiTarget {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    /// Empty when no key was found.
    pub api_key: String,
}

/// The host part of a base URL: no scheme, no `user:pass@`, no path or query.
pub fn api_host(base_url: &str) -> String {
    let rest = base_url
        .split_once("://")
        .map(|(_, r)| r)
        .unwrap_or(base_url);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    host.to_string()
}

/// Run the API row for `/doctor` and `yoyo doctor`: prints one line saying a
/// request is about to go out, then blocks on it. A provider that needs a key
/// and has none is not contacted.
pub fn run_api_check(target: &ApiTarget) -> DoctorCheck {
    let config = crate::agent_builder::create_model_config(
        &target.provider,
        &target.model,
        target.base_url.as_deref(),
    );
    let host = host_or_provider(&config);
    let key_optional =
        crate::model_limits_note::USER_RUN_SERVER_PROVIDERS.contains(&target.provider.as_str());
    if target.api_key.is_empty() && !key_optional {
        return DoctorCheck {
            name: "API".into(),
            status: DoctorStatus::Warn,
            detail: "not tested: no API key".into(),
        };
    }
    println!("  Checking the API: sending one minimal request to {host}...");
    let key = target.api_key.clone();
    // Both doors (REPL and `yoyo doctor`) run inside the multi-thread runtime
    // `main` starts; the review subcommand uses the same bridge.
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(check_api(config, &key, API_CHECK_TIMEOUT))
    })
}

fn host_or_provider(config: &ModelConfig) -> String {
    let host = api_host(&config.base_url);
    if host.is_empty() {
        format!("the {} endpoint", config.provider)
    } else {
        host
    }
}

/// Send one minimal request (no tools, no system prompt, a one-word prompt)
/// through yoagent's provider for `config`, bounded by `timeout`.
pub async fn check_api(config: ModelConfig, api_key: &str, timeout: Duration) -> DoctorCheck {
    let host = host_or_provider(&config);
    let model = config.id.clone();
    let Some(provider) = ProviderRegistry::default().resolve(&config.api) else {
        return api_row(
            DoctorStatus::Fail,
            format!("no provider for protocol {}", config.api),
        );
    };
    let mut request = StreamConfig::new(model.clone(), api_key);
    request.messages = vec![Message::user("hi")];
    request.max_tokens = Some(PROBE_MAX_TOKENS);
    request.model_config = Some(config);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let cancel = tokio_util::sync::CancellationToken::new();
    let outcome = tokio::time::timeout(timeout, provider.stream(request, tx, cancel.clone())).await;
    cancel.cancel();
    let outcome = match outcome {
        Ok(result) => Some(result),
        Err(_) => None,
    };
    classify(&host, &model, outcome, timeout)
}

fn api_row(status: DoctorStatus, detail: String) -> DoctorCheck {
    DoctorCheck {
        name: "API".into(),
        status,
        detail,
    }
}

/// The HTTP status yoagent wrote at the head of an error's text: "HTTP 404
/// Not Found: ...", "Google API error 404 Not Found: ...", "Bedrock error 404
/// ...". yoagent's `ProviderError` keeps the status only in that text.
fn status_in(text: &str) -> Option<u16> {
    let head = text.split(':').next().unwrap_or("");
    head.split_whitespace()
        .filter(|w| w.len() == 3)
        .find_map(|w| w.parse::<u16>().ok().filter(|s| (100..=599).contains(s)))
}

/// A provider's error text on one line (Google pretty-prints its JSON, so a
/// first line alone can be just `{`), secrets masked, capped.
fn one_line(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut line = crate::safety::redact_secrets(&joined);
    if line.len() > DETAIL_MAX_BYTES {
        let mut b = DETAIL_MAX_BYTES;
        while b > 0 && !line.is_char_boundary(b) {
            b -= 1;
        }
        line.truncate(b);
        line.push('…');
    }
    line
}

fn with_status(status: Option<u16>) -> String {
    status.map(|s| format!(" (HTTP {s})")).unwrap_or_default()
}

/// Turn the request's outcome into the row. `None` = no answer in time.
fn classify(
    host: &str,
    model: &str,
    outcome: Option<Result<Message, ProviderError>>,
    timeout: Duration,
) -> DoctorCheck {
    use DoctorStatus::{Fail, Pass, Warn};
    let err = match outcome {
        None => {
            return api_row(
                Fail,
                format!("no answer from {host} within {} s", timeout.as_secs()),
            )
        }
        Some(Ok(Message::Assistant {
            stop_reason: StopReason::Error,
            error_message,
            ..
        })) => ProviderError::Other(error_message.unwrap_or_default()),
        Some(Ok(_)) => return api_row(Pass, format!("reached {host} as {model}")),
        Some(Err(e)) => e,
    };
    match err {
        ProviderError::Auth(text) => api_row(
            Fail,
            format!(
                "{host} rejected the API key{}",
                with_status(status_in(&text))
            ),
        ),
        ProviderError::Network(text) => {
            let lower = text.to_lowercase();
            let why = if lower.contains("refused") {
                "connection refused"
            } else if lower.contains("dns") || lower.contains("resolve") {
                "host name did not resolve"
            } else if lower.contains("certificate") || lower.contains("tls") {
                "TLS handshake failed"
            } else {
                "network error"
            };
            api_row(
                Fail,
                format!("cannot reach {host} ({why}); is the server running and the base URL right?"),
            )
        }
        ProviderError::RateLimited { .. } => api_row(
            Warn,
            format!("reached {host}, but it answered rate-limited or overloaded; key and model not confirmed"),
        ),
        ProviderError::Api(text) if status_in(&text) == Some(404) => api_row(
            Fail,
            format!("{host} answered 404: model {model} not found, or the base URL is not the API's"),
        ),
        other => {
            let text = match &other {
                ProviderError::Api(t) | ProviderError::Other(t) => t.clone(),
                ProviderError::ContextOverflow { message } => message.clone(),
                e => e.to_string(),
            };
            let status = status_in(&text);
            api_row(
                Fail,
                format!(
                    "{host} answered with an error{}: {}",
                    with_status(status),
                    one_line(&text)
                ),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const KEY: &str = "doctor-canary-1242";
    const MODEL: &str = "claude-opus-5-5";

    fn sse_ok() -> String {
        [
            ("message_start", r#"{"type":"message_start","message":{"id":"msg_d","type":"message","role":"assistant","content":[],"model":"claude-opus-5-5","stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1}}}"#),
            ("content_block_start", r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#),
            ("content_block_delta", r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#),
            ("content_block_stop", r#"{"type":"content_block_stop","index":0}"#),
            ("message_delta", r#"{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":1}}"#),
            ("message_stop", r#"{"type":"message_stop"}"#),
        ]
        .iter()
        .map(|(e, d)| format!("event: {e}\ndata: {d}\n\n"))
        .collect()
    }

    /// A stub on 127.0.0.1 that answers every request with `status` (a 200
    /// gets a valid Anthropic stream) and records each request's head.
    /// `silent` accepts and never answers.
    async fn stub(status: u16, silent: bool) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let store = Arc::clone(&seen);
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = vec![0u8; 65536];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                store
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf[..n]).into_owned());
                if silent {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    continue;
                }
                let (ctype, body) = if status == 200 {
                    ("text/event-stream", sse_ok())
                } else {
                    (
                        "application/json",
                        format!(
                            r#"{{"type":"error","error":{{"type":"stub","message":"stub says {status}"}}}}"#
                        ),
                    )
                };
                let resp = format!(
                    "HTTP/1.1 {status} STUB\r\ncontent-type: {ctype}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.shutdown().await;
            }
        });
        (format!("http://127.0.0.1:{port}"), seen)
    }

    async fn check_against(url: &str, timeout: Duration) -> DoctorCheck {
        let config = crate::agent_builder::create_model_config("anthropic", MODEL, Some(url));
        check_api(config, KEY, timeout).await
    }

    async fn row_for_status(status: u16) -> (DoctorStatus, String, String) {
        let (url, _) = stub(status, false).await;
        let row = check_against(&url, Duration::from_secs(10)).await;
        (row.status, row.detail, api_host(&url))
    }

    #[tokio::test]
    async fn a_200_passes_and_names_host_and_model() {
        let (status, detail, host) = row_for_status(200).await;
        assert_eq!(detail, format!("reached {host} as {MODEL}"));
        assert_eq!(status, DoctorStatus::Pass);
    }

    #[tokio::test]
    async fn a_401_says_the_key_was_rejected() {
        let (status, detail, host) = row_for_status(401).await;
        assert_eq!(detail, format!("{host} rejected the API key (HTTP 401)"));
        assert_eq!(status, DoctorStatus::Fail);
    }

    #[tokio::test]
    async fn a_403_says_the_key_was_rejected() {
        let (status, detail, host) = row_for_status(403).await;
        assert_eq!(detail, format!("{host} rejected the API key (HTTP 403)"));
        assert_eq!(status, DoctorStatus::Fail);
    }

    #[tokio::test]
    async fn a_404_names_both_the_model_and_the_endpoint() {
        let (status, detail, host) = row_for_status(404).await;
        assert_eq!(
            detail,
            format!(
                "{host} answered 404: model {MODEL} not found, or the base URL is not the API's"
            )
        );
        assert_eq!(status, DoctorStatus::Fail);
    }

    #[tokio::test]
    async fn a_500_reports_its_status_and_first_line() {
        let (status, detail, host) = row_for_status(500).await;
        assert_eq!(
            detail,
            format!(
                "{host} answered with an error (HTTP 500): HTTP 500 Internal Server Error: \
                 {{\"type\":\"error\",\"error\":{{\"type\":\"stub\",\"message\":\"stub says 500\"}}}}"
            )
        );
        assert_eq!(status, DoctorStatus::Fail);
    }

    #[tokio::test]
    async fn a_closed_port_says_it_cannot_reach_the_host() {
        // Bind, read the port, drop: nothing listens there any more.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
            l.local_addr().unwrap().port()
        };
        let url = format!("http://127.0.0.1:{port}");
        let row = check_against(&url, Duration::from_secs(10)).await;
        assert_eq!(
            row.detail,
            format!(
                "cannot reach 127.0.0.1:{port} (connection refused); \
                 is the server running and the base URL right?"
            )
        );
        assert_eq!(row.status, DoctorStatus::Fail);
    }

    #[tokio::test]
    async fn a_server_that_never_answers_times_out() {
        let (url, _) = stub(200, true).await;
        let row = check_against(&url, Duration::from_secs(1)).await;
        assert_eq!(
            row.detail,
            format!("no answer from {} within 1 s", api_host(&url))
        );
        assert_eq!(row.status, DoctorStatus::Fail);
    }

    #[tokio::test]
    async fn the_key_goes_only_to_the_configured_host_in_the_runs_header() {
        let (url, seen) = stub(200, false).await;
        let row = check_against(&url, Duration::from_secs(10)).await;
        assert_eq!(row.status, DoctorStatus::Pass, "{}", row.detail);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "exactly one request: {seen:?}");
        let head = seen[0].to_lowercase();
        assert!(head.starts_with("post /v1/messages "), "{head}");
        assert!(
            head.contains(&format!("\r\nhost: {}\r\n", api_host(&url))),
            "{head}"
        );
        assert!(
            head.contains(&format!("\r\nx-api-key: {KEY}\r\n")),
            "{head}"
        );
        // The key appears once, in its header, and nowhere else in the request.
        assert_eq!(seen[0].matches(KEY).count(), 1, "{}", seen[0]);
        // The detail never carries the key.
        assert!(!row.detail.contains(KEY));
    }

    #[test]
    fn a_missing_key_is_not_tested_and_sends_nothing() {
        // No stub: had a request gone out, it would have gone to a closed
        // port and come back Fail, not this Warn.
        let target = ApiTarget {
            provider: "anthropic".into(),
            model: MODEL.into(),
            base_url: Some("http://127.0.0.1:9".into()),
            api_key: String::new(),
        };
        let row = run_api_check(&target);
        assert_eq!(row.detail, "not tested: no API key");
        assert_eq!(row.status, DoctorStatus::Warn);
    }

    #[test]
    fn error_text_is_one_line_masked_and_capped() {
        let pretty = "Google API error 400 Bad Request: {\n  \"error\": {\n    \"message\": \"API key not valid\"\n  }\n}";
        assert_eq!(
            one_line(pretty),
            "Google API error 400 Bad Request: { \"error\": { \"message\": \"API key not valid\" } }"
        );
        let long = "é".repeat(300);
        let capped = one_line(&long);
        assert_eq!(capped, format!("{}…", "é".repeat(100)));
    }

    #[test]
    fn api_host_drops_scheme_credentials_path_and_query() {
        assert_eq!(
            api_host("https://api.anthropic.com/v1"),
            "api.anthropic.com"
        );
        assert_eq!(
            api_host("http://u:pw@127.0.0.1:8080/v1?x=1"),
            "127.0.0.1:8080"
        );
        assert_eq!(api_host("localhost:11434"), "localhost:11434");
        assert_eq!(api_host(""), "");
    }

    #[test]
    fn status_in_reads_each_providers_error_head() {
        assert_eq!(status_in("HTTP 404 Not Found: {}"), Some(404));
        assert_eq!(status_in("Google API error 404 Not Found: x"), Some(404));
        assert_eq!(status_in("Bedrock error 404 Not Found (X): y"), Some(404));
        assert_eq!(status_in("model 404 is gone"), Some(404));
        assert_eq!(status_in("no status: HTTP 404"), None);
    }
}
