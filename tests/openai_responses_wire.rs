//! Which OpenAI wire does a tool-using run go out on? (Day 234.)
//!
//! OpenAI documents that Chat Completions does not support function calling
//! with GPT-6 Astra or GPT-6.1 Sol, and allows it for GPT-6 Sol/Luna only at
//! `reasoning_effort: none` (an omitted effort means `medium`). yoagent's
//! GPT-6 presets therefore use the Responses API. These tests drive the real
//! binary against a request-capturing stub on 127.0.0.1 and assert on the
//! FIRST request it sends: its path and its body. No real OpenAI call is made
//! (and none could be: there is no key here), so this pins the wire shape, not
//! OpenAI's acceptance of it.
//!
//! Hermetic: every inherited `*_API_KEY` / `API_KEY` is stripped and a dummy
//! `--api-key` is passed, so the result is the same on the evolve runner
//! (which exports provider keys) and in ci.yml (which exports none). HOME/XDG
//! point into a tempdir, never the repo.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// One captured POST: the request path and the body.
#[derive(Clone, Debug)]
struct Captured {
    path: String,
    body: String,
}

/// A stub answering every request with a non-retryable 400 after recording
/// it. The test asserts the captured request, never yoyo's exit.
fn start_capturing_stub() -> (u16, Arc<Mutex<Vec<Captured>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<Captured>>> = Arc::new(Mutex::new(Vec::new()));
    let store = Arc::clone(&seen);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let Ok(clone) = stream.try_clone() else {
                continue;
            };
            let mut reader = BufReader::new(clone);
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
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
            let path = request_line
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string();
            store.lock().unwrap().push(Captured {
                path,
                body: String::from_utf8_lossy(&body).into_owned(),
            });
            let payload = r#"{"error":{"message":"stub: captured","type":"invalid_request_error","code":"stub"}}"#;
            let resp = format!(
                "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (port, seen)
}

/// Run `yoyo --provider openai --model <model> -p hi` (tools ON) against a
/// fresh stub and return the first captured request.
fn first_request(model: &str, extra: &[&str]) -> Captured {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    let (port, seen) = start_capturing_stub();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    for (name, _) in std::env::vars() {
        if name == "API_KEY" || name.ends_with("_API_KEY") {
            cmd.env_remove(name);
        }
    }
    let mut child = cmd
        .args([
            "--provider",
            "openai",
            "--model",
            model,
            "--base-url",
            &base_url,
            "--api-key",
            "sk-dummy-test-key",
            "--max-turns",
            "1",
        ])
        .args(extra)
        .args(["-p", "hi"])
        .current_dir(&proj)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env_remove("MODEL")
        .env_remove("YOYO_MODEL")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    let deadline = Instant::now() + Duration::from_secs(60);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("yoyo did not exit within 60s");
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    let all = seen.lock().unwrap();
    all.first().cloned().unwrap_or_else(|| {
        panic!(
            "stub saw no request for {model}; stderr:\n{}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn json(body: &str) -> serde_json::Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("body is not JSON ({e}): {body}"))
}

#[test]
fn gpt_6_sol_with_tools_goes_to_the_responses_endpoint() {
    let req = first_request("gpt-6-sol", &[]);
    assert!(
        req.path.ends_with("/v1/responses"),
        "gpt-6-sol must use the Responses API (OpenAI: Chat Completions allows \
         function calling for GPT-6 Sol only at reasoning_effort none); got {}",
        req.path
    );
    let body = json(&req.body);
    let tools = body["tools"].as_array().expect("tools array in body");
    assert!(
        !tools.is_empty(),
        "a tool-using run must send tools: {body}"
    );
    assert_eq!(body["model"], "gpt-6-sol");
}

/// Near-miss: an id with no GPT-6 preset keeps today's route byte-for-byte.
#[test]
fn non_preset_openai_id_still_goes_to_chat_completions() {
    let req = first_request("gpt-4o", &[]);
    assert_eq!(
        Path::new(&req.path),
        Path::new("/v1/chat/completions"),
        "gpt-4o has no preset and must stay on Chat Completions"
    );
    let body = json(&req.body);
    assert!(
        !body["tools"].as_array().expect("tools array").is_empty(),
        "anti-vacuous: the near-miss run is tool-using too"
    );
}

/// Record what the Responses body carries with `--thinking high`: OpenAI says
/// to drop temperature/top_p when a reasoning effort is set, and yoyo sends no
/// temperature unless `--temperature` is given, so none must appear here.
#[test]
fn gpt_6_sol_thinking_high_sends_effort_and_no_temperature() {
    let req = first_request("gpt-6-sol", &["--thinking", "high"]);
    assert!(req.path.ends_with("/v1/responses"), "{}", req.path);
    let body = json(&req.body);
    assert_eq!(body["reasoning"]["effort"], "high", "{body}");
    assert!(
        body.get("temperature").is_none(),
        "no --temperature given, so no temperature may be sent: {body}"
    );
}
