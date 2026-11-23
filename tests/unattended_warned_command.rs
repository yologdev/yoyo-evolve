//! #1231 at the built binary: what a command the bash safety check warns
//! about does in each way a run nobody can answer may be started.
//!
//! A stub on 127.0.0.1:0 answers turn 1 with one `bash` tool call and every
//! later turn with text, and records each POST body. The command is warned
//! (`chmod -R 777`) and harmless: it creates and chmods `RAN` inside the
//! run's own tempdir, so the file's existence says whether it ran, and a
//! regressed build touches nothing outside the tempdir. The environment is
//! cleared (`env_clear`) so no ambient key or YOYO_* setting reaches the run.
//!
//! Rows: `-p` without `--yes` is refused (the refusal text goes back to the
//! model as the tool result); `--yes -p` runs; piped stdin with no prompt
//! flag runs, unchanged by #1231.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const WARNED: &str = "touch RAN && chmod -R 777 RAN";
const REFUSAL_STEM: &str = "Command needs user confirmation that this run cannot obtain: \
     Recursive permission change";

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

fn block(index_start: String, delta: String, stop_reason: &str) -> String {
    sse(vec![
        message_start(),
        ("content_block_start", index_start),
        ("content_block_delta", delta),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#.to_string(),
        ),
        (
            "message_delta",
            format!(
                r#"{{"type":"message_delta","delta":{{"stop_reason":"{stop_reason}","stop_sequence":null}},"usage":{{"output_tokens":1}}}}"#
            ),
        ),
        ("message_stop", r#"{"type":"message_stop"}"#.to_string()),
    ])
}

fn reply_for(body: &str) -> String {
    if body.contains("tool_result") {
        block(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#.to_string(),
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"done"}}"#.to_string(),
            "end_turn",
        )
    } else {
        let args = format!(r#"{{\"command\":\"{WARNED}\"}}"#);
        block(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1231","name":"bash","input":{}}}"#.to_string(),
            format!(
                r#"{{"type":"content_block_delta","index":0,"delta":{{"type":"input_json_delta","partial_json":"{args}"}}}}"#
            ),
            "tool_use",
        )
    }
}

fn start_stub() -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let bodies = Arc::new(Mutex::new(Vec::new()));
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
            let payload = reply_for(&body);
            if request_line.starts_with("POST") {
                store.lock().unwrap().push(body);
            }
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

/// The `messages` array of a request body, without the system prompt and
/// tool schemas that would bury it in a failure message.
fn messages_only(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .map(|v| v["messages"].to_string())
        .unwrap_or_else(|_| body.to_string())
}

struct Run {
    ran: bool,
    bodies: Vec<String>,
    stderr: String,
}

impl Run {
    fn ctx(&self) -> String {
        format!(
            "ran={} posts={}\nstderr:\n{}\nmessages:\n{}",
            self.ran,
            self.bodies.len(),
            self.stderr,
            self.bodies
                .iter()
                .map(|b| messages_only(b))
                .collect::<Vec<_>>()
                .join("\n---\n")
        )
    }
}

/// `flags` go before the prompt; `piped_prompt` sends the prompt on stdin
/// instead of `-p`.
fn run(flags: &[&str], piped_prompt: bool) -> Run {
    let tmp = tempfile::tempdir().expect("tempdir");
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let (port, bodies) = start_stub();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    cmd.args([
        "--provider",
        "anthropic",
        "--model",
        "claude-sonnet-4-5",
        "--base-url",
        &base_url,
        "--max-turns",
        "4",
        "--no-update-check",
    ])
    .args(flags);
    if !piped_prompt {
        cmd.args(["-p", "clean up"]);
    }
    let mut child = cmd
        .current_dir(&proj)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("ANTHROPIC_API_KEY", "dummy-test-key-1231")
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    {
        let mut stdin = child.stdin.take().unwrap();
        if piped_prompt {
            let _ = stdin.write_all(b"clean up\n");
        }
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
    let ran = proj.join("RAN").exists();
    let bodies = bodies.lock().unwrap().clone();
    Run {
        ran,
        bodies,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

#[test]
fn a_prompt_flag_without_yes_refuses_the_warned_command() {
    let r = run(&[], false);
    let ctx = r.ctx();
    assert!(
        !r.ran,
        "the warned command ran under -p without --yes\n{ctx}"
    );
    assert!(
        r.bodies.len() >= 2 && r.bodies[1].contains(REFUSAL_STEM),
        "the refusal never reached the model as the tool result\n{ctx}"
    );
    assert!(
        r.bodies[1].contains("rerun with --yes."),
        "remedy missing\n{ctx}"
    );
}

#[test]
fn explicit_yes_with_a_prompt_flag_runs_the_warned_command() {
    let r = run(&["--yes"], false);
    let ctx = r.ctx();
    assert!(r.ran, "--yes -p did not run the warned command\n{ctx}");
    assert!(
        !r.bodies.iter().any(|b| b.contains(REFUSAL_STEM)),
        "--yes -p was refused\n{ctx}"
    );
}

/// Unchanged by #1231: every loop script runs yoyo this way (census on #1231).
#[test]
fn piped_stdin_without_a_prompt_flag_still_runs_the_warned_command() {
    let r = run(&[], true);
    let ctx = r.ctx();
    assert!(
        r.bodies.len() >= 2,
        "the piped run never sent the tool result back\n{ctx}"
    );
    assert!(
        r.ran,
        "piped stdin stopped running the warned command\n{ctx}"
    );
}
