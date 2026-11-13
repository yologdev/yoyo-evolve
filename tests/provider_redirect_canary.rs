//! #1181 at the wire: a provider base URL that redirects to ANOTHER origin
//! must send that origin nothing: no key, no prompt, no retry.
//!
//! Before yoagent 0.27.8 the provider HTTP client used reqwest's default
//! redirect policy, which follows any redirect and strips only
//! `Authorization`/`Cookie` on a cross-origin hop. Measured Day 257 at a local
//! stub: `--provider anthropic` re-sent `x-api-key` and the whole ~10 KB
//! prompt to the other host on each of 6 retry attempts. 0.27.8 follows
//! redirects only within the request's own origin; a cross-origin one fails
//! with `ProviderError::Other` ("the server redirected to <B>, outside <A>;
//! refused, ...") and is not retried. These rows pin that from yoyo's side:
//! the built binary, two stubs on 127.0.0.1, nothing else contacted (d255:
//! a leak is never reproduced against a real third party).
//!
//! Stub A answers every request with a 307 or a 302 to the same path on stub
//! B; stub B records every request it gets. Each row asserts B recorded zero
//! requests, A recorded exactly one (no retry), the run fails, and stderr
//! names B's origin, i.e. the user sees yoagent's refusal and not a generic
//! error.
//!
//! The owner's list on #1181 also named azure and "an openai-compatible
//! provider with a custom credential header (`headers` in config)". yoyo has
//! neither: no `azure` arm in `create_model_config` (an unknown provider name
//! becomes `ModelConfig::local`, a bearer, which the `openai` row covers) and
//! no config key that fills `ModelConfig::headers` (`TOP_LEVEL_KEYS` in
//! `src/config_unknown_keys.rs`). The only thing yoyo puts in that map is its
//! own client headers; the `openrouter` row is the provider where that map
//! carries the most (User-Agent, HTTP-Referer, X-Title), so it exercises the
//! same `headers` path a credential would ride.
//!
//! The environment is cleared per run (`env_clear`, temp HOME and XDG dirs),
//! the same scrub `tests/provider_key_canary.rs` uses (Day 233: hermeticity is
//! per file, not inherited).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// A fake key: nothing vendor-shaped, and only ever sent to 127.0.0.1.
const KEY: &str = "canary-not-a-key";

/// One recorded request.
#[derive(Clone)]
struct Seen {
    line: String,
    /// Every header as `name: value`, names lowercased.
    headers: Vec<String>,
    body_bytes: usize,
}

impl Seen {
    fn carries_key(&self) -> bool {
        self.headers.iter().any(|h| h.contains(KEY))
    }
}

fn render(seen: &[Seen]) -> String {
    seen.iter()
        .map(|s| {
            format!(
                "  {} body_bytes={} headers={:?}",
                s.line, s.body_bytes, s.headers
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// How a stub answers: given the request path, the raw HTTP response.
type Responder = Arc<dyn Fn(&str) -> String + Send + Sync>;

fn start_stub(respond: Responder) -> (u16, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::<Seen>::new()));
    let store = Arc::clone(&seen);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                continue;
            }
            let mut content_length = 0usize;
            let mut headers = Vec::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    break;
                }
                if let Some((k, v)) = trimmed.split_once(':') {
                    let k = k.trim().to_ascii_lowercase();
                    let v = v.trim();
                    if k == "content-length" {
                        content_length = v.parse().unwrap_or(0);
                    }
                    headers.push(format!("{k}: {v}"));
                }
            }
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
            let line = request_line.trim_end().to_string();
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            store.lock().unwrap().push(Seen {
                line,
                headers,
                body_bytes: content_length,
            });
            let _ = stream.write_all(respond(&path).as_bytes());
            let _ = stream.flush();
        }
    });
    (port, seen)
}

/// Stub B's answer: a 401, so a request that does arrive ends the run fast.
fn unauthorized(_path: &str) -> String {
    let reply =
        r#"{"type":"error","error":{"type":"authentication_error","message":"invalid key"}}"#;
    format!(
        "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        reply.len(),
        reply
    )
}

/// Stub A's answer: `code` to the same path on `target_port`.
fn redirect_to(code: u16, target_port: u16) -> Responder {
    let reason = match code {
        307 => "Temporary Redirect",
        302 => "Found",
        _ => "Redirect",
    };
    Arc::new(move |path: &str| {
        format!(
            "HTTP/1.1 {code} {reason}\r\nlocation: http://127.0.0.1:{target_port}{path}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        )
    })
}

struct Run {
    a: Vec<Seen>,
    b: Vec<Seen>,
    stderr: String,
    exit: Option<i32>,
}

impl Run {
    fn ctx(&self) -> String {
        format!(
            "exit={:?}\nA saw {} request(s):\n{}\nB saw {} request(s):\n{}\nstderr:\n{}",
            self.exit,
            self.a.len(),
            render(&self.a),
            self.b.len(),
            render(&self.b),
            self.stderr
        )
    }
}

/// Run `yoyo -p hi` for `provider`, its base URL pointing at a stub A that
/// answers with `a_responder` (built from B's port), and a recording stub B.
fn run(provider: &str, base_suffix: &str, a_responder: impl Fn(u16) -> Responder) -> (Run, u16) {
    let (b_port, b_seen) = start_stub(Arc::new(unauthorized));
    let (a_port, a_seen) = start_stub(a_responder(b_port));
    let tmp = tempfile::tempdir().expect("tempdir");
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let base_url = format!("http://127.0.0.1:{a_port}{base_suffix}");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    cmd.args([
        "--provider",
        provider,
        "--base-url",
        &base_url,
        "--model",
        "x",
        "--api-key",
        KEY,
        "-p",
        "hi",
    ])
    .current_dir(&proj)
    .env_clear()
    .env("PATH", std::env::var_os("PATH").unwrap_or_default())
    .env("HOME", &home)
    .env("XDG_CONFIG_HOME", home.join(".config"))
    .env("XDG_DATA_HOME", home.join(".local/share"))
    .env("XDG_STATE_HOME", home.join(".local/state"))
    .env("XDG_CACHE_HOME", home.join(".cache"))
    .env("NO_COLOR", "1")
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn yoyo");
    let deadline = Instant::now() + Duration::from_secs(90);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    let a = a_seen.lock().unwrap().clone();
    let b = b_seen.lock().unwrap().clone();
    (
        Run {
            a,
            b,
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            exit: out.status.code(),
        },
        b_port,
    )
}

/// The providers whose credential rides in a header reqwest does NOT strip
/// cross-origin (anthropic `x-api-key`, google `x-goog-api-key`), one bearer
/// (openai, which reqwest strips; the prompt body still went), and openrouter
/// (the most-populated `ModelConfig::headers`). The base suffix is what each
/// arm expects a user to give.
const PROVIDERS: &[(&str, &str)] = &[
    ("anthropic", ""),
    ("google", ""),
    ("openai", "/v1"),
    ("openrouter", "/api/v1"),
];

/// 307 keeps method and body (the prompt); 302 turns POST into a bodyless
/// GET but the old policy still re-sent every header.
const CODES: &[u16] = &[307, 302];

#[test]
fn cross_origin_redirect_sends_nothing_to_the_other_origin() {
    let mut failures = Vec::new();
    for &(provider, suffix) in PROVIDERS {
        for &code in CODES {
            let row = format!("{provider} x {code}");
            let (r, b_port) = run(provider, suffix, |b| redirect_to(code, b));
            let ctx = r.ctx();
            let b_origin = format!("127.0.0.1:{b_port}");
            let mut problems = Vec::new();
            if !r.b.is_empty() {
                problems.push(format!(
                    "B (the other origin) received {} request(s)",
                    r.b.len()
                ));
            }
            // Exactly one: the refused redirect is not retried. A retry would
            // show here as more requests to A, each refused the same way.
            if r.a.len() != 1 {
                problems.push(format!(
                    "A received {} request(s), expected exactly 1",
                    r.a.len()
                ));
            }
            // A's one request did carry the key: the row exercised a keyed
            // request, so B's zero is about the redirect, not a missing key.
            if !r.a.iter().any(Seen::carries_key) {
                problems
                    .push("A's request carried no key (row did not exercise a credential)".into());
            }
            if r.exit == Some(0) || r.exit.is_none() {
                problems.push(format!("exit {:?}, expected a non-zero exit", r.exit));
            }
            // The readable refusal: yoagent's text names B's origin and says
            // it is outside A's. A generic "API error" alone would lose both.
            if !r.stderr.contains(&b_origin) || !r.stderr.contains("outside") {
                problems.push(format!(
                    "stderr does not carry the refusal naming {b_origin}"
                ));
            }
            // One line per row, shown with --nocapture: the measured table.
            let refusal = r.stderr.lines().find(|l| l.contains("outside"));
            eprintln!(
                "[{row}] A={} B={} exit={:?} refusal={:?}",
                r.a.len(),
                r.b.len(),
                r.exit,
                refusal.map(str::trim)
            );
            if !problems.is_empty() {
                failures.push(format!("[{row}] {}\n{ctx}", problems.join("; ")));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Recall control for B's recorder: point yoyo straight at B. If the matrix's
/// zeros were a recorder that cannot see requests (or a key it cannot read),
/// this row would read zero too. It must see the request, the key in a
/// header, and a non-empty body.
#[test]
fn recorder_sees_a_keyed_request_sent_to_it_directly() {
    for &(provider, suffix) in PROVIDERS {
        // A "redirect" responder that is never consulted: yoyo talks to B.
        let (b_port, b_seen) = start_stub(Arc::new(unauthorized));
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let base_url = format!("http://127.0.0.1:{b_port}{suffix}");
        let out = Command::new(env!("CARGO_BIN_EXE_yoyo"))
            .args([
                "--provider",
                provider,
                "--base-url",
                &base_url,
                "--model",
                "x",
                "--api-key",
                KEY,
                "-p",
                "hi",
            ])
            .current_dir(tmp.path())
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("XDG_STATE_HOME", home.join(".local/state"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .output()
            .expect("run yoyo");
        let seen = b_seen.lock().unwrap().clone();
        let ctx = format!(
            "exit={:?}\nB saw:\n{}\nstderr:\n{}",
            out.status.code(),
            render(&seen),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!seen.is_empty(), "{provider}: recorder saw nothing\n{ctx}");
        assert!(
            seen[0].carries_key(),
            "{provider}: recorder did not see the key in any header\n{ctx}"
        );
        assert!(
            seen[0].body_bytes > 0,
            "{provider}: recorder saw no body\n{ctx}"
        );
    }
}
