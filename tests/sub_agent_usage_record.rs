//! #1073 at the real emission point: a `yoyo -p --audit` run whose model calls
//! `sub_agent` must write a `"type":"usage"` record whose `sub_agents` object
//! carries the CHILD's tokens, while the top-level token fields stay the
//! parent's own. Drives the built binary against a local stub provider that
//! answers by request shape:
//!
//! - parent turn 1 (no task marker, no tool result) → a `sub_agent` tool call;
//! - the sub-agent's own request (carries the task marker) → text, with a
//!   distinctive input-token count;
//! - parent turn 2 (carries the tool result) → final text.
//!
//! Every fixture lives under a tempdir with its own HOME/XDG, never the repo,
//! and provider keys are scrubbed except the dummy one (Day 233: CI exports no
//! keys, the evolve job does).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const TASK_MARKER: &str = "CHILD_TASK_ZZ1073";
const CHILD_INPUT_TOKENS: u64 = 7_771;
const PARENT_INPUT_TOKENS: u64 = 11;

fn sse(events: Vec<(&str, String)>) -> String {
    events
        .into_iter()
        .map(|(e, d)| format!("event: {e}\ndata: {d}\n\n"))
        .collect()
}

fn message_start(input: u64) -> (&'static str, String) {
    (
        "message_start",
        format!(
            r#"{{"type":"message_start","message":{{"id":"msg_s","type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-5","stop_reason":null,"stop_sequence":null,"usage":{{"input_tokens":{input},"output_tokens":1}}}}}}"#
        ),
    )
}

fn text_reply(input: u64, text: &str) -> String {
    sse(vec![
        message_start(input),
        (
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#.to_string(),
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
        (
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":1}}"#.to_string(),
        ),
        ("message_stop", r#"{"type":"message_stop"}"#.to_string()),
    ])
}

fn sub_agent_call(input: u64) -> String {
    let args = format!(r#"{{\"task\":\"{TASK_MARKER}: say ok\"}}"#);
    sse(vec![
        message_start(input),
        (
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"sub_agent","input":{}}}"#.to_string(),
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
        (
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":1}}"#.to_string(),
        ),
        ("message_stop", r#"{"type":"message_stop"}"#.to_string()),
    ])
}

/// Pick the reply from the request body's shape (see module doc).
fn reply_for(body: &str) -> String {
    if body.contains("tool_result") {
        text_reply(PARENT_INPUT_TOKENS, "done")
    } else if body.contains(TASK_MARKER) {
        text_reply(CHILD_INPUT_TOKENS, "child ok")
    } else {
        sub_agent_call(PARENT_INPUT_TOKENS)
    }
}

fn start_stub() -> (u16, Arc<Mutex<usize>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let posts = Arc::new(Mutex::new(0usize));
    let count = Arc::clone(&posts);
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
            if request_line.starts_with("POST") {
                *count.lock().unwrap() += 1;
            }
            let payload = reply_for(&body);
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (port, posts)
}

/// Run yoyo against the stub with `extra` args before `-p`; returns the
/// output, the audit text and the stub's POST count.
fn run_delegating(extra: &[&str]) -> (std::process::Output, String, usize) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let (port, posts) = start_stub();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args([
            "--provider",
            "anthropic",
            "--model",
            "claude-sonnet-4-5",
            "--base-url",
            &base_url,
            "--audit",
        ])
        .args(extra)
        .args(["-p", "delegate please"])
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
    let audit = std::fs::read_to_string(proj.join(".yoyo/audit.jsonl")).unwrap_or_default();
    let n = *posts.lock().unwrap();
    (out, audit, n)
}

#[test]
fn a_run_that_delegates_records_the_childs_spend_in_sub_agents() {
    let (out, audit, posts) = run_delegating(&[]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let usage: Vec<serde_json::Value> = audit
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["type"] == "usage")
        .collect();
    let ctx = format!(
        "posts={} exit={:?}\naudit:\n{audit}\nstderr:\n{stderr}",
        posts,
        out.status.code()
    );
    // Anti-vacuous: the sub-agent really ran (parent, child, parent).
    assert!(
        posts >= 3,
        "the sub_agent call never reached the stub\n{ctx}"
    );
    assert_eq!(usage.len(), 1, "one usage record per run\n{ctx}");
    let rec = &usage[0];
    let sub = &rec["sub_agents"];
    assert_eq!(sub["runs"], 1, "{ctx}");
    assert_eq!(sub["input_tokens"], CHILD_INPUT_TOKENS, "{ctx}");
    // Own spend stays the parent's: two parent turns, never the child's.
    assert_eq!(rec["input_tokens"], 2 * PARENT_INPUT_TOKENS, "{ctx}");
}

/// The `--output-format json` envelope at the real emission point: the same
/// run's delegated spend is in a `sub_agents` object, the top-level usage stays
/// the parent's own, and the audit record (drained from the same single
/// per-run take) still carries it too — proof the json read added no second
/// drain that would have emptied one of them.
#[test]
fn output_format_json_carries_the_childs_spend_in_sub_agents() {
    let (out, audit, posts) = run_delegating(&["--output-format", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let ctx = format!(
        "posts={posts} exit={:?}\nstdout:\n{stdout}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        posts >= 3,
        "the sub_agent call never reached the stub\n{ctx}"
    );
    let env: serde_json::Value = stdout
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .unwrap_or_else(|| panic!("no json envelope on stdout\n{ctx}"));
    assert_eq!(env["sub_agents"]["runs"], 1, "{ctx}");
    assert_eq!(
        env["sub_agents"]["input_tokens"], CHILD_INPUT_TOKENS,
        "{ctx}"
    );
    assert_eq!(
        env["usage"]["input_tokens"],
        2 * PARENT_INPUT_TOKENS,
        "{ctx}"
    );
    let rec = audit
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v["type"] == "usage")
        .unwrap_or_else(|| panic!("no usage record\naudit:\n{audit}\n{ctx}"));
    assert_eq!(rec["sub_agents"], env["sub_agents"], "{ctx}");
}
