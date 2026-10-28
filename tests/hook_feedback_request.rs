//! #1054 at the real emission point: a post-tool shell hook's stderr is
//! appended by `HookedTool` (src/hooks.rs) as an EXTRA `Content::Text` block
//! after the tool's own output. Before yoagent 0.25.9 the provider layer sent
//! only the first text block of a tool result, so that block was built,
//! returned and unit-tested — and never reached the model. Every older test
//! checked the `ToolResult` the wrapper returns, one layer above the drop, so
//! they stayed green while the feature was dead.
//!
//! This test reads the layer that matters: the SECOND request body the
//! provider receives. It drives the built binary against a local stub that
//! answers turn 1 with a `bash` tool call (`echo hi`) and every later turn
//! with text, captures every POST body, and asserts on the parsed JSON:
//!
//! - the `tool_result` for that `tool_use_id` carries the tool's own output
//!   (anti-vacuous: `hi`) AND, after yoagent's block separator, the hook's
//!   `[Hook feedback]` block, exactly (see `Run::tool_result` for why the
//!   Anthropic wire carries the blocks joined into one string);
//! - near-miss: the same run with no hook configured carries the output and no
//!   `[Hook feedback]` text at all.
//!
//! Every fixture lives under a tempdir with its own HOME/XDG, never the repo,
//! and provider keys are scrubbed except the dummy one (Day 233: CI exports no
//! keys, the evolve job does).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Printed by the hook on stderr; unique enough that nothing else in a
/// request body can contain it.
const HOOK_MARKER: &str = "HOOKMARK-7731";
const TOOL_USE_ID: &str = "toolu_hook_1";

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

fn text_reply(text: &str) -> String {
    sse(vec![
        message_start(),
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

fn bash_call() -> String {
    let args = r#"{\"command\":\"echo hi\"}"#;
    sse(vec![
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
        (
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":1}}"#.to_string(),
        ),
        ("message_stop", r#"{"type":"message_stop"}"#.to_string()),
    ])
}

/// Turn 1 (no tool result yet) gets the tool call; anything later gets text.
fn reply_for(body: &str) -> String {
    if body.contains("tool_result") {
        text_reply("done")
    } else {
        bash_call()
    }
}

/// A stub on 127.0.0.1:0 answering by request shape and recording each POST
/// body in arrival order.
fn start_capturing_stub() -> (u16, Arc<Mutex<Vec<String>>>) {
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

struct Run {
    bodies: Vec<String>,
    stderr: String,
    /// Kept so a red run can say what yoyo printed before it died; capped by
    /// `cap_on_char_boundary` because a runaway reply would bury the rest.
    stdout: String,
    exit: Option<i32>,
    /// The signal that terminated yoyo, if any. `exit=None` alone cannot tell
    /// a SIGKILL from a SIGTERM from a SIGSEGV, and that is the one datum that
    /// locates the cause of a flake (the 2026-10-28 01:54 CI red was
    /// `posts=1 exit=None`, stderr empty, and nothing else).
    signal: Option<i32>,
}

/// Bytes of yoyo's stdout kept in a failure message.
const STDOUT_KEEP_BYTES: usize = 4096;

/// Truncate to at most `max` bytes without splitting a UTF-8 character
/// (never byte-index a string, #250), marking the cut in band.
fn cap_on_char_boundary(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut b = max;
    while b > 0 && !s.is_char_boundary(b) {
        b -= 1;
    }
    format!("{}… [{} more bytes]", &s[..b], s.len() - b)
}

#[cfg(unix)]
fn termination_signal(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
fn termination_signal(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}

impl Run {
    fn ctx(&self) -> String {
        format!(
            "posts={} exit={:?} signal={:?}\nstderr:\n{}\nstdout:\n{}\nbodies:\n{}",
            self.bodies.len(),
            self.exit,
            self.signal,
            self.stderr,
            self.stdout,
            self.bodies.join("\n---\n")
        )
    }

    /// The `tool_result` for `TOOL_USE_ID` in the second request body, parsed
    /// from JSON (never a raw-body `contains`), as the text the model receives.
    ///
    /// On the Anthropic wire a text-only tool result is sent as ONE string:
    /// yoagent's `provider::tool_result_text` joins every non-empty text block
    /// with `"\n"` (the array form is used only when an image is present), so
    /// the block boundary is observable as that separator, which is what the
    /// presence test pins. The array shape is joined the same way.
    fn tool_result(&self) -> String {
        let ctx = self.ctx();
        assert!(
            self.bodies.len() >= 2,
            "the tool result never went back to the provider\n{ctx}"
        );
        let req: serde_json::Value =
            serde_json::from_str(&self.bodies[1]).expect("second request body is JSON");
        let messages = req["messages"].as_array().expect("messages array");
        let result = messages
            .iter()
            .filter_map(|m| m["content"].as_array())
            .flatten()
            .find(|b| b["type"] == "tool_result" && b["tool_use_id"] == TOOL_USE_ID)
            .unwrap_or_else(|| panic!("no tool_result for {TOOL_USE_ID}\n{ctx}"));
        match &result["content"] {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(blocks) => {
                let texts: Vec<&str> = blocks
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .filter(|t| !t.is_empty())
                    .collect();
                texts.join("\n")
            }
            other => panic!("unexpected tool_result content shape {other}\n{ctx}"),
        }
    }
}

/// Run `yoyo -p` from `proj` (which may hold a `.yoyo.toml`) against a fresh
/// capturing stub. `--trust-project` is what loads a project-local shell hook
/// non-interactively; `--yes` lets the bash call run without a prompt.
fn run_in(proj: &Path, home: &Path) -> Run {
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
            "--trust-project",
            "--yes",
            "--max-turns",
            "4",
            "-p",
            "run echo hi",
        ])
        .current_dir(proj)
        .env("HOME", home)
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
    let bodies = bodies.lock().unwrap().clone();
    Run {
        bodies,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        stdout: cap_on_char_boundary(&String::from_utf8_lossy(&out.stdout), STDOUT_KEEP_BYTES),
        exit: out.status.code(),
        signal: termination_signal(&out.status),
    }
}

fn fixture(with_hook: bool) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    if with_hook {
        std::fs::write(
            proj.join(".yoyo.toml"),
            format!("hooks.post.bash = \"echo {HOOK_MARKER} >&2\"\n"),
        )
        .unwrap();
    }
    (tmp, proj, home)
}

/// The hooked run's tool result must be EXACTLY the un-hooked run's result,
/// then yoagent's block separator, then `HookedTool`'s feedback text
/// (`"\n[Hook feedback]\n{stderr}"`). Deriving the prefix from a second, hook-
/// free run keeps this independent of the bash tool's output format while
/// still proving three things at once: the tool's own output went back
/// unchanged, the feedback went back too, and it was appended after the
/// output rather than spliced into it. On the Anthropic wire a text-only
/// tool result is ONE string (yoagent joins its blocks with `"\n"`), so the
/// model does not see a separate block — measured Day 239 (be3869a4); this
/// test's old name claimed it did.
#[test]
fn post_hook_feedback_is_appended_after_the_tool_output_in_the_same_tool_result() {
    let (_tmp, proj, home) = fixture(true);
    let run = run_in(&proj, &home);
    let ctx = run.ctx();
    let wire = run.tool_result();

    let (_tmp0, proj0, home0) = fixture(false);
    let base = run_in(&proj0, &home0).tool_result();
    // Anti-vacuous: the derived prefix really is the tool's output.
    assert!(
        base.contains("hi") && !base.contains(HOOK_MARKER),
        "baseline tool output is not `echo hi`'s: {base:?}"
    );

    let feedback_block = format!("\n[Hook feedback]\n{HOOK_MARKER}");
    assert_eq!(
        wire,
        format!("{base}\n{feedback_block}"),
        "hook feedback was not appended after the tool output in the same tool_result\n{ctx}"
    );
}

#[test]
fn no_hook_means_tool_output_and_no_hook_feedback_text() {
    let (_tmp, proj, home) = fixture(false);
    let run = run_in(&proj, &home);
    let ctx = run.ctx();
    let wire = run.tool_result();
    assert!(
        wire.contains("hi"),
        "tool output `hi` missing from tool_result {wire:?}\n{ctx}"
    );
    assert!(
        !wire.contains("[Hook feedback]"),
        "hook feedback appeared with no hook configured: {wire:?}\n{ctx}"
    );
    assert!(
        !run.bodies.iter().any(|b| b.contains(HOOK_MARKER)),
        "marker leaked with no hook configured\n{ctx}"
    );
}

/// The failure context's two new fields, pinned without spawning anything:
/// a char straddling the cut is dropped whole, and an at-budget string is
/// untouched (the near-miss every ordinary run takes).
#[test]
fn stdout_cap_never_splits_a_char_and_leaves_short_output_alone() {
    assert_eq!(cap_on_char_boundary("hi\n", 4096), "hi\n");
    assert_eq!(cap_on_char_boundary("abcd", 4), "abcd");
    // '🐙' is 4 bytes; a 6-byte budget lands inside the second one.
    let out = cap_on_char_boundary("a🐙🐙", 6);
    assert_eq!(out, "a🐙… [4 more bytes]");
}

/// A process killed by a signal reports it, so `ctx` can name it.
#[cfg(unix)]
#[test]
fn termination_signal_names_a_killed_child() {
    let mut child = Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");
    child.kill().unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.code(), None);
    assert_eq!(termination_signal(&status), Some(9));
}
