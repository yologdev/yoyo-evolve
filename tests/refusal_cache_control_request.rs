//! #1206 on the wire: a refused turn must not poison the next request.
//!
//! Day 261 01:38 an evolve task died: after some tool calls the model refused
//! (`stop_reason: refusal`) with only a thinking block, piped mode's
//! auto-continue (`--continue-on-silence`, which the evolve loop opts into in
//! `.yoyo.toml`) sent the next request, and Anthropic answered 400
//! `messages.19.content.0.thinking.cache_control: Extra inputs are not
//! permitted`. yoagent 0.27.9 places the conversation cache breakpoint on the
//! last block of the latest earlier message, which was that thinking block
//! (upstream ask: yologdev/yoagent#455).
//!
//! This drives the built binary through that exact path against a local
//! recording stub (never the real endpoint): reply 1 is a harmless `bash`
//! tool call (`echo hi`, so the silence branch is armed: it fires only after
//! tool use), reply 2 is a thinking block plus `stop_reason: "refusal"`,
//! reply 3 is plain text. It then parses the THIRD recorded request body and
//! asserts that no thinking block carries `cache_control` and that the refused
//! turn went back as `src/prompt/refusal_history.rs`'s text marker.
//!
//! Fixtures live under a tempdir with their own HOME/XDG; provider keys are
//! scrubbed except the dummy one (Day 233).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Must match `REFUSED_TURN_MARKER` in src/prompt/refusal_history.rs.
const REFUSED_TURN_MARKER: &str = "[the model refused this request]";
const TOOL_USE_ID: &str = "toolu_refusal_1";

fn sse(events: Vec<(&str, String)>) -> String {
    events
        .into_iter()
        .map(|(e, d)| format!("event: {e}\ndata: {d}\n\n"))
        .collect()
}

fn message_start() -> (&'static str, String) {
    (
        "message_start",
        r#"{"type":"message_start","message":{"id":"msg_s","type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-5","stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":1}}}"#.to_string(),
    )
}

fn message_end(stop: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "message_delta",
            format!(
                r#"{{"type":"message_delta","delta":{{"stop_reason":"{stop}","stop_sequence":null}},"usage":{{"output_tokens":1}}}}"#
            ),
        ),
        ("message_stop", r#"{"type":"message_stop"}"#.to_string()),
    ]
}

fn text_reply(text: &str) -> String {
    let mut ev = vec![
        message_start(),
        (
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#
                .to_string(),
        ),
        (
            "content_block_delta",
            format!(
                r#"{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{text}"}}}}"#
            ),
        ),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#.to_string(),
        ),
    ];
    ev.extend(message_end("end_turn"));
    sse(ev)
}

fn bash_call() -> String {
    let args = r#"{\"command\":\"echo hi\"}"#;
    let mut ev = vec![
        message_start(),
        (
            "content_block_start",
            format!(
                r#"{{"type":"content_block_start","index":0,"content_block":{{"type":"tool_use","id":"{TOOL_USE_ID}","name":"bash","input":{{}}}}}}"#
            ),
        ),
        (
            "content_block_delta",
            format!(
                r#"{{"type":"content_block_delta","index":0,"delta":{{"type":"input_json_delta","partial_json":"{args}"}}}}"#
            ),
        ),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#.to_string(),
        ),
    ];
    ev.extend(message_end("tool_use"));
    sse(ev)
}

/// A thinking-only turn ended by the safety system.
fn refusal_reply() -> String {
    let mut ev = vec![
        message_start(),
        (
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#.to_string(),
        ),
        (
            "content_block_delta",
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"considering"}}"#.to_string(),
        ),
        (
            "content_block_delta",
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig-refusal-1"}}"#.to_string(),
        ),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#.to_string(),
        ),
    ];
    ev.extend(message_end("refusal"));
    sse(ev)
}

/// Answers by arrival order and records every POST body.
fn start_capturing_stub() -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let bodies = Arc::new(Mutex::new(Vec::<String>::new()));
    let store = Arc::clone(&bodies);
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
            let body = String::from_utf8_lossy(&body).into_owned();
            if !request_line.starts_with("POST") {
                continue;
            }
            let n = {
                let mut s = store.lock().unwrap();
                s.push(body);
                s.len()
            };
            let payload = match n {
                1 => bash_call(),
                2 => refusal_reply(),
                _ => text_reply("done"),
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (port, bodies)
}

struct Run {
    bodies: Vec<String>,
    stderr: String,
    exit: Option<i32>,
}

impl Run {
    fn ctx(&self) -> String {
        format!(
            "posts={} exit={:?}\nstderr:\n{}\nbodies:\n{}",
            self.bodies.len(),
            self.exit,
            self.stderr,
            self.bodies.join("\n---\n")
        )
    }
}

/// `echo hi | yoyo --continue-on-silence ...` from a tempdir, the evolve
/// loop's door (piped mode).
fn run_piped() -> Run {
    let tmp = tempfile::tempdir().expect("tempdir");
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let (port, bodies) = start_capturing_stub();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args([
            "--provider",
            "anthropic",
            "--model",
            "claude-sonnet-4-5",
            "--base-url",
            &base_url,
            "--thinking",
            "high",
            "--yes",
            "--continue-on-silence",
            "--max-turns",
            "4",
        ])
        .current_dir(&proj)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("ANTHROPIC_API_KEY", "sk-ant-dummy-test-key")
        .env_remove("API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("DEEPSEEK_API_KEY")
        .env_remove("YOYO_AUDIT")
        .env_remove("YOYO_RESTRICTED")
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"hi\n").unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("yoyo did not exit within 60s");
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    let bodies = bodies.lock().unwrap().clone();
    Run {
        bodies,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        exit: out.status.code(),
    }
}

#[test]
fn request_after_a_refused_thinking_only_turn_carries_no_cache_control_on_thinking() {
    let run = run_piped();
    let ctx = run.ctx();
    assert!(
        run.stderr.contains("refused this request"),
        "the stub's refusal never reached yoyo's refusal notice\n{ctx}"
    );
    assert!(
        run.bodies.len() >= 3,
        "no request went out after the refusal (auto-continue did not fire)\n{ctx}"
    );
    let req: serde_json::Value =
        serde_json::from_str(&run.bodies[2]).expect("third request body is JSON");
    let messages = req["messages"].as_array().expect("messages array");

    let cached_thinking: Vec<&serde_json::Value> = messages
        .iter()
        .filter_map(|m| m["content"].as_array())
        .flatten()
        .filter(|b| {
            (b["type"] == "thinking" || b["type"] == "redacted_thinking")
                && b.get("cache_control").is_some()
        })
        .collect();
    assert!(
        cached_thinking.is_empty(),
        "a thinking block carries cache_control (Anthropic answers 400): {cached_thinking:?}\n{ctx}"
    );

    // The refused turn is still there, as the marker, so roles alternate.
    let refused_turn_texts: Vec<&str> = messages
        .iter()
        .filter(|m| m["role"] == "assistant")
        .filter_map(|m| m["content"].as_array())
        .flatten()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect();
    assert!(
        refused_turn_texts.contains(&REFUSED_TURN_MARKER),
        "the refused turn did not go back as {REFUSED_TURN_MARKER:?}: {refused_turn_texts:?}\n{ctx}"
    );
    // Its thinking did not go back at all.
    assert!(
        !run.bodies[2].contains("sig-refusal-1"),
        "the refused turn's thinking was resent\n{ctx}"
    );
}
