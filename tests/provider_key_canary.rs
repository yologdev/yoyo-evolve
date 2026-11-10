//! #1170 at the wire: which credential yoyo sends to which provider.
//!
//! Before #1170, `ANTHROPIC_API_KEY` was the fallback key for EVERY provider
//! whose own variable was unset, so a user with an Anthropic key in their
//! shell who pointed yoyo at ollama, a `--provider custom` proxy, OpenAI,
//! xAI... sent their Anthropic secret to that host as a bearer token.
//! `ANTHROPIC_API_KEY` is Anthropic's credential by name; it now reaches only
//! the `anthropic` arm. The generic `API_KEY` stays a fallback for every
//! provider (the docs call it the "generic fallback").
//!
//! Each row runs the built binary against a stub on 127.0.0.1:0 that records
//! every request's credential headers and answers 401, with the environment
//! cleared (`env_clear`) so no ambient key from the developer's or CI's shell
//! can reach the run (Day 233: hermeticity is per file, not inherited).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const CANARY: &str = "sk-ant-CANARY1170";
const GENERIC: &str = "generic-API_KEY-1170";

/// One recorded request: its request line plus the `authorization` and
/// `x-api-key` header values (empty string when the header was absent).
#[derive(Clone)]
struct Seen {
    line: String,
    authorization: Option<String>,
    x_api_key: Option<String>,
}

fn start_stub() -> (u16, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<Seen>>> = Arc::new(Mutex::new(Vec::new()));
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
            let mut authorization = None;
            let mut x_api_key = None;
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
                    let v = v.trim().to_string();
                    if k.eq_ignore_ascii_case("content-length") {
                        content_length = v.parse().unwrap_or(0);
                    } else if k.eq_ignore_ascii_case("authorization") {
                        authorization = Some(v);
                    } else if k.eq_ignore_ascii_case("x-api-key") {
                        x_api_key = Some(v);
                    }
                }
            }
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
            store.lock().unwrap().push(Seen {
                line: request_line.trim_end().to_string(),
                authorization,
                x_api_key,
            });
            let reply = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
            let resp = format!(
                "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                reply.len(),
                reply
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (port, seen)
}

struct Run {
    seen: Vec<Seen>,
    stderr: String,
    exit: Option<i32>,
}

impl Run {
    fn ctx(&self) -> String {
        let reqs: Vec<String> = self
            .seen
            .iter()
            .map(|s| {
                format!(
                    "{} authorization={:?} x-api-key={:?}",
                    s.line, s.authorization, s.x_api_key
                )
            })
            .collect();
        format!(
            "exit={:?}\nrequests:\n{}\nstderr:\n{}",
            self.exit,
            reqs.join("\n"),
            self.stderr
        )
    }

    /// Every credential value any request carried, joined, for "does the
    /// canary appear anywhere on the wire" checks.
    fn all_credentials(&self) -> String {
        self.seen
            .iter()
            .flat_map(|s| [s.authorization.clone(), s.x_api_key.clone()])
            .flatten()
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

/// Run the built binary with ONLY `PATH`, a temp `HOME`, and `env` set.
fn run(provider: &str, base_suffix: &str, env: &[(&str, &str)]) -> Run {
    run_verb(&[], provider, base_suffix, env)
}

/// `run`, with `verb` (e.g. `["review"]`) before the flags. The project dir is
/// a git repo with one staged file, so `yoyo review` has something to review.
fn run_verb(verb: &[&str], provider: &str, base_suffix: &str, env: &[(&str, &str)]) -> Run {
    let tmp = tempfile::tempdir().expect("tempdir");
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    if !verb.is_empty() {
        std::fs::write(proj.join("f.txt"), "a\n").unwrap();
        for g in [&["init", "-q", "."][..], &["add", "f.txt"][..]] {
            let ok = Command::new("git")
                .args(g)
                .current_dir(&proj)
                .status()
                .expect("git")
                .success();
            assert!(ok, "git {g:?} failed");
        }
    }
    run_in(verb, &proj, &home, provider, base_suffix, env)
}

fn run_in(
    verb: &[&str],
    proj: &Path,
    home: &Path,
    provider: &str,
    base_suffix: &str,
    env: &[(&str, &str)],
) -> Run {
    let (port, seen) = start_stub();
    let base_url = format!("http://127.0.0.1:{port}{base_suffix}");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    cmd.args(verb);
    cmd.args([
        "--provider",
        provider,
        "--base-url",
        &base_url,
        "--model",
        "x",
    ]);
    if verb.is_empty() {
        cmd.args(["-p", "hi"]);
    }
    cmd.current_dir(proj)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("spawn yoyo");
    let deadline = Instant::now() + Duration::from_secs(60);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    let seen = seen.lock().unwrap().clone();
    Run {
        seen,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        exit: out.status.code(),
    }
}

/// Rows (a) and (b): the user-run-server providers. With only
/// `ANTHROPIC_API_KEY` set, the request still goes out (they need no key)
/// and carries no trace of the Anthropic secret.
#[test]
fn user_run_servers_never_receive_the_anthropic_key() {
    for provider in ["custom", "ollama"] {
        let r = run(provider, "/v1", &[("ANTHROPIC_API_KEY", CANARY)]);
        let ctx = r.ctx();
        assert!(
            !r.seen.is_empty(),
            "{provider}: yoyo never reached the stub (a no-key provider must still run)\n{ctx}"
        );
        let creds = r.all_credentials();
        assert!(
            !creds.contains(CANARY),
            "{provider}: ANTHROPIC_API_KEY went over the wire to a non-Anthropic host: {creds}\n{ctx}"
        );
        // The 401 hint's remedy must name a variable this provider reads. It
        // used to say `export ANTHROPIC_API_KEY`, which no longer reaches it.
        assert!(
            r.stderr.contains("export API_KEY="),
            "{provider}: auth hint does not point at API_KEY\n{ctx}"
        );
        assert!(
            !r.stderr.contains("export ANTHROPIC_API_KEY"),
            "{provider}: auth hint advises a variable this provider ignores\n{ctx}"
        );
        // Where it went instead: no x-api-key at all, and the bearer (if any)
        // is the no-key placeholder, never a real credential.
        for s in &r.seen {
            assert_eq!(s.x_api_key, None, "{provider}: unexpected x-api-key\n{ctx}");
            if let Some(auth) = &s.authorization {
                assert_eq!(
                    auth, "Bearer not-needed",
                    "{provider}: bearer is not the no-key placeholder\n{ctx}"
                );
            }
        }
    }
}

/// Row (c), the positive control: the canary DOES reach an anthropic
/// endpoint, in the header the anthropic provider uses (`x-api-key`). Without
/// this row, rows (a)/(b) would also pass if the canary never reached
/// anything at all.
#[test]
fn anthropic_still_receives_its_own_key() {
    let r = run("anthropic", "", &[("ANTHROPIC_API_KEY", CANARY)]);
    let ctx = r.ctx();
    assert!(!r.seen.is_empty(), "yoyo never reached the stub\n{ctx}");
    assert_eq!(
        r.seen[0].x_api_key.as_deref(),
        Some(CANARY),
        "anthropic did not send ANTHROPIC_API_KEY as x-api-key\n{ctx}"
    );
}

/// Row (d): the generic `API_KEY` stays a fallback for every provider; it
/// names no vendor, so the user set it to mean "my key for whatever yoyo
/// talks to". Pinned so narrowing the Anthropic fallback did not also drop it.
#[test]
fn generic_api_key_is_still_the_fallback_for_custom() {
    let r = run(
        "custom",
        "/v1",
        &[("ANTHROPIC_API_KEY", CANARY), ("API_KEY", GENERIC)],
    );
    let ctx = r.ctx();
    assert!(!r.seen.is_empty(), "yoyo never reached the stub\n{ctx}");
    let expected = format!("Bearer {GENERIC}");
    assert_eq!(
        r.seen[0].authorization.as_deref(),
        Some(expected.as_str()),
        "custom did not fall back to API_KEY\n{ctx}"
    );
    assert!(!r.all_credentials().contains(CANARY), "{ctx}");
}

/// Row (e): a provider that needs a key, with only `ANTHROPIC_API_KEY` set,
/// refuses before any request and names its OWN variable, instead of sending
/// the Anthropic key and coming back with a 401.
#[test]
fn keyed_provider_without_its_own_key_refuses_and_names_its_variable() {
    let r = run("openai", "/v1", &[("ANTHROPIC_API_KEY", CANARY)]);
    let ctx = r.ctx();
    assert!(
        r.seen.is_empty(),
        "openai sent a request with no key of its own\n{ctx}"
    );
    assert_eq!(r.exit, Some(1), "{ctx}");
    assert!(r.stderr.contains("No API key found"), "{ctx}");
    assert!(r.stderr.contains("OPENAI_API_KEY"), "{ctx}");
}

/// Row (f): the second door. `yoyo review` resolves its key separately
/// (dispatch_sub.rs `resolve_api_key`) and had the same ANTHROPIC_API_KEY
/// fallback. It must not send the canary, and, like the main path, must still
/// run against a user-run server with no key at all.
#[test]
fn review_never_sends_the_anthropic_key_to_a_user_run_server() {
    for provider in ["ollama", "custom"] {
        let r = run_verb(
            &["review"],
            provider,
            "/v1",
            &[("ANTHROPIC_API_KEY", CANARY)],
        );
        let ctx = r.ctx();
        assert!(
            !r.seen.is_empty(),
            "review on {provider} never reached the stub (a no-key provider must still run)\n{ctx}"
        );
        assert!(
            !r.all_credentials().contains(CANARY),
            "review on {provider} sent ANTHROPIC_API_KEY\n{ctx}"
        );
    }
}
