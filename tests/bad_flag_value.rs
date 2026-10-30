//! Day 244 at the real binary: an explicit numeric flag whose value doesn't
//! parse (`--max-tokens abc`) is refused at startup with exit 2, before any
//! request. Before, it warned "using default" and ran a paid prompt.
//!
//! Every run points at a local stub that counts connections and answers a
//! non-retryable 400. A refused run must exit 2 with the stub at ZERO
//! connections; the near-miss (a valid value) must reach the stub, which
//! proves the refusal is the only thing that changed. All `*_API_KEY`
//! variables are scrubbed (as `tests/openai_responses_wire.rs` does), because
//! the evolve job exports them and CI does not.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// A stub that counts each request and answers it with a 400.
fn start_counting_stub() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&hits);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            count.fetch_add(1, Ordering::SeqCst);
            let Ok(clone) = stream.try_clone() else {
                continue;
            };
            let mut reader = BufReader::new(clone);
            let mut content_length = 0usize;
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
                    if k.eq_ignore_ascii_case("content-length") {
                        content_length = v.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
            let payload =
                r#"{"type":"error","error":{"type":"invalid_request_error","message":"stub"}}"#;
            let resp = format!(
                "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (port, hits)
}

/// Run `yoyo --base-url <stub> -p hi <extra>` in a scratch HOME/project.
/// Returns (exit code, stderr, requests the stub saw). A run that reaches
/// the stub is killed once it has (the near-miss only needs the request).
fn run(extra: &[&str]) -> (Option<i32>, String, usize) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    let (port, hits) = start_counting_stub();
    let base_url = format!("http://127.0.0.1:{port}");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    for (name, _) in std::env::vars() {
        if name == "API_KEY" || name.ends_with("_API_KEY") {
            cmd.env_remove(name);
        }
    }
    let mut child = cmd
        .args(["--base-url", &base_url, "-p", "hi"])
        .args(extra)
        .current_dir(&proj)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("ANTHROPIC_API_KEY", "x")
        .env_remove("MODEL")
        .env_remove("YOYO_AUDIT")
        .env_remove("YOYO_RESTRICTED")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if hits.load(Ordering::SeqCst) > 0 || start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().expect("wait");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        hits.load(Ordering::SeqCst),
    )
}

#[test]
fn non_numeric_max_tokens_refuses_with_exit_2_before_any_request() {
    let (code, err, hits) = run(&["--max-tokens", "abc"]);
    assert_eq!(code, Some(2), "stderr: {err}");
    assert!(err.contains("--max-tokens"), "{err}");
    assert!(err.contains("'abc'"), "{err}");
    assert!(err.contains("Nothing was sent"), "{err}");
    assert!(!err.contains("using default"), "{err}");
    assert_eq!(hits, 0, "a refused run must send nothing");
}

#[test]
fn every_numeric_flag_refuses_a_non_numeric_value() {
    for (flag, value) in [
        ("--temperature", "hot"),
        ("--max-turns", "x"),
        ("--context-window", "big"),
    ] {
        let (code, err, hits) = run(&[flag, value]);
        assert_eq!(code, Some(2), "{flag} {value}: {err}");
        assert_eq!(hits, 0, "{flag} {value}: a refused run must send nothing");
        assert!(err.contains(flag) && err.contains(value), "{err}");
    }
}

/// Near-miss: the same command with a valid value gets past startup and
/// reaches the stub, so the refusal is the only change.
#[test]
fn valid_max_tokens_gets_past_startup_and_sends_the_request() {
    let (_, err, hits) = run(&["--max-tokens", "64"]);
    assert!(
        hits >= 1,
        "a valid value must reach the stub; stderr: {err}"
    );
    assert!(!err.contains("is not a number"), "{err}");
}
