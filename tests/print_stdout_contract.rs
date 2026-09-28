//! #966: a process-level witness for the `-p` stdout contract.
//!
//! The defect lived only in the real binary's stdout: in `--print` and
//! `--output-format json` modes the streamed renderer wrote the answer AND
//! `emit_output` wrote it again, so a caller piping yoyo into `jq` (or reading
//! a single value) got the answer twice. The unit tests in `src/` pin the gate
//! decision; nothing else in the suite ran the binary against a model
//! endpoint. This file does, against a local SSE stub that speaks just enough
//! of the Anthropic Messages streaming dialect to produce one text block.
//!
//! Isolation: HOME / XDG_* point at a tempdir and the cwd is a second tempdir,
//! so no user config, trust store or project `.yoyo.toml` is read or written.
//! The API key is a dummy — the stub never checks it.
//!
//! Anti-vacuous: every test asserts the stub actually received a request, so
//! a run that never reached the network cannot pass on an empty stdout.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// The only text the stub's model ever says.
const ANSWER: &str = r#"{"answer": 42}"#;

/// One Anthropic Messages SSE stream whose only text is `ANSWER`.
fn sse_body() -> String {
    let text = serde_json::to_string(ANSWER).unwrap();
    let events: Vec<(&str, String)> = vec![
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_stub","type":"message","role":"assistant","model":"claude-stub","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":5,"output_tokens":1}}}"#.to_string(),
        ),
        (
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#.to_string(),
        ),
        (
            "content_block_delta",
            format!(
                r#"{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":{text}}}}}"#
            ),
        ),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#.to_string(),
        ),
        (
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":7}}"#.to_string(),
        ),
        ("message_stop", r#"{"type":"message_stop"}"#.to_string()),
    ];
    let mut out = String::new();
    for (name, data) in events {
        out.push_str(&format!("event: {name}\ndata: {data}\n\n"));
    }
    out
}

/// Contents of the file the tool turn reads.
const TOOL_FILE_CONTENTS: &str = "hello world\n";

fn sse_events(events: Vec<(&str, String)>) -> String {
    let mut out = String::new();
    for (name, data) in events {
        out.push_str(&format!("event: {name}\ndata: {data}\n\n"));
    }
    out
}

/// An assistant turn that asks for `read_file` on `a.txt` and stops for the tool.
fn sse_tool_use_body() -> String {
    let partial = serde_json::to_string(r#"{"path": "a.txt"}"#).unwrap();
    sse_events(vec![
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_tool","type":"message","role":"assistant","model":"claude-stub","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":5,"output_tokens":1}}}"#.to_string(),
        ),
        (
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_stub_1","name":"read_file","input":{}}}"#.to_string(),
        ),
        (
            "content_block_delta",
            format!(
                r#"{{"type":"content_block_delta","index":0,"delta":{{"type":"input_json_delta","partial_json":{partial}}}}}"#
            ),
        ),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#.to_string(),
        ),
        (
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":7}}"#.to_string(),
        ),
        ("message_stop", r#"{"type":"message_stop"}"#.to_string()),
    ])
}

/// Start a stub on 127.0.0.1:0 that answers POSTs with SSE bodies: the Nth POST gets `bodies[N]` (the last body
/// repeats), so a tool turn can be followed by the final answer. Returns the
/// port and a flag set once a POST has been received.
fn start_stub_seq(bodies: Vec<String>) -> (u16, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let hit = Arc::new(AtomicBool::new(false));
    let hit_thread = Arc::clone(&hit);
    thread::spawn(move || {
        // Serve connections until the test process exits; the binary should
        // make exactly one request, but a retry must not hang it.
        let mut served = 0usize;
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            let mut reader = BufReader::new(stream.try_clone().unwrap());
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
            if request_line.starts_with("POST") {
                hit_thread.store(true, Ordering::SeqCst);
            }
            let payload = bodies[served.min(bodies.len() - 1)].clone();
            if request_line.starts_with("POST") {
                served += 1;
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
    (port, hit)
}

struct Run {
    stdout: Vec<u8>,
    stderr: String,
    success: bool,
    stub_hit: bool,
}

/// Run the real binary against a fresh stub with `extra` args. When `stdin`
/// is `Some`, it is piped in (piped mode); otherwise stdin is null.
fn run_yoyo(extra: &[&str], stdin: Option<&str>) -> Run {
    run_yoyo_with(
        &["--no-tools", "--max-turns", "1"],
        extra,
        stdin,
        vec![sse_body()],
    )
}

/// General form: `base` replaces the tool/turn flags and the stub serves
/// `bodies` in order. The cwd holds `a.txt` (`TOOL_FILE_CONTENTS`).
fn run_yoyo_with(base: &[&str], extra: &[&str], stdin: Option<&str>, bodies: Vec<String>) -> Run {
    let (port, hit) = start_stub_seq(bodies);
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    std::fs::write(cwd.path().join("a.txt"), TOOL_FILE_CONTENTS).unwrap();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    cmd.args([
        "--provider",
        "anthropic",
        "--model",
        "claude-sonnet-4-5",
        "--base-url",
        &base_url,
    ])
    .args(base)
    .args(extra)
    .current_dir(cwd.path())
    .env("HOME", home.path())
    .env("XDG_CONFIG_HOME", home.path().join(".config"))
    .env("XDG_DATA_HOME", home.path().join(".local/share"))
    .env("XDG_STATE_HOME", home.path().join(".local/state"))
    .env("XDG_CACHE_HOME", home.path().join(".cache"))
    .env("ANTHROPIC_API_KEY", "sk-ant-dummy-test-key")
    .env_remove("API_KEY")
    .env_remove("MODEL")
    .env_remove("YOYO_MODEL")
    .env_remove("YOYO_PROVIDER")
    .env("NO_COLOR", "1")
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = cmd.spawn().expect("spawn yoyo");
    if let Some(input) = stdin {
        let mut pipe = child.stdin.take().unwrap();
        pipe.write_all(input.as_bytes()).unwrap();
        drop(pipe);
    }
    // 20s budget: poll, then kill so a hang fails loudly instead of wedging CI.
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let out = child.wait_with_output().unwrap();
            panic!(
                "yoyo did not exit within 20s; stdout={:?} stderr={}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    Run {
        stdout: out.stdout,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        success: out.status.success(),
        stub_hit: hit.load(Ordering::SeqCst),
    }
}

fn occurrences(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

fn assert_reached_stub(run: &Run) {
    assert!(
        run.stub_hit,
        "anti-vacuous: the stub never received a POST, so stdout proves nothing; stderr={}",
        run.stderr
    );
    assert!(run.success, "yoyo exited non-zero; stderr={}", run.stderr);
}

#[test]
fn print_mode_emits_the_answer_exactly_once() {
    let run = run_yoyo(&["--print", "-p", "hi"], None);
    assert_reached_stub(&run);
    let stdout = String::from_utf8(run.stdout.clone()).expect("utf-8 stdout");
    assert_eq!(
        stdout.trim_end(),
        ANSWER,
        "#966: --print stdout must be the answer and nothing else; stderr={}",
        run.stderr
    );
    assert_eq!(occurrences(&stdout, ANSWER), 1, "stdout={stdout:?}");
}

#[test]
fn json_mode_emits_exactly_one_value() {
    let run = run_yoyo(&["--output-format", "json", "-p", "hi"], None);
    assert_reached_stub(&run);
    let values: Vec<serde_json::Value> = serde_json::Deserializer::from_slice(&run.stdout)
        .into_iter::<serde_json::Value>()
        .collect::<Result<_, _>>()
        .unwrap_or_else(|e| {
            panic!(
                "#966: json stdout must parse cleanly with no stray bytes: {e}; stdout={:?}",
                String::from_utf8_lossy(&run.stdout)
            )
        });
    assert_eq!(
        values.len(),
        1,
        "#966: json stdout must be exactly one value; stdout={:?}",
        String::from_utf8_lossy(&run.stdout)
    );
    let rendered = values[0].to_string();
    // The answer is a JSON string inside the envelope, so it appears escaped.
    let escaped = serde_json::to_string(ANSWER).unwrap();
    let escaped_inner = &escaped[1..escaped.len() - 1];
    assert!(
        rendered.contains(escaped_inner),
        "envelope must carry the answer text; envelope={rendered}"
    );
}

#[test]
fn piped_print_mode_emits_the_answer_exactly_once() {
    let run = run_yoyo(&["--print"], Some("hi\n"));
    assert_reached_stub(&run);
    let stdout = String::from_utf8(run.stdout.clone()).expect("utf-8 stdout");
    assert_eq!(
        stdout.trim_end(),
        ANSWER,
        "#966: piped --print stdout must be the answer and nothing else; stderr={}",
        run.stderr
    );
    assert_eq!(occurrences(&stdout, ANSWER), 1, "stdout={stdout:?}");
}

/// Near-miss: plain `-p` without `--print` still streams the answer to stdout.
/// The #966 gate must silence the streamer ONLY when stdout is reserved.
#[test]
fn plain_prompt_mode_still_streams_the_answer() {
    let run = run_yoyo(&["-p", "hi"], None);
    assert_reached_stub(&run);
    let stdout = String::from_utf8(run.stdout.clone()).expect("utf-8 stdout");
    assert!(
        stdout.contains(ANSWER),
        "plain -p must still stream the answer; stdout={stdout:?} stderr={}",
        run.stderr
    );
}

/// Run with tools on: turn 1 calls `read_file`, turn 2 answers `ANSWER`.
fn run_tool_turn(extra: &[&str]) -> Run {
    run_yoyo_with(
        &["--max-turns", "4"],
        extra,
        None,
        vec![sse_tool_use_body(), sse_body()],
    )
}

/// Anti-vacuous half of the tool-turn tests: the tool really ran and its
/// progress line was MOVED to stderr, not deleted.
fn assert_tool_progress_on_stderr(run: &Run) {
    assert!(
        run.stderr.contains("\u{25b6} read a.txt"),
        "anti-vacuous: the read_file progress line must appear on stderr; stderr={}",
        run.stderr
    );
}

/// The Day 212 bug: `--print` with a tool turn wrote `  ▶ read a.txt ✓` to
/// stdout ahead of the answer. Stdout must be exactly the answer bytes.
#[test]
fn print_mode_with_tool_turn_keeps_stdout_to_the_answer() {
    let run = run_tool_turn(&["--print", "-p", "read a.txt"]);
    assert_reached_stub(&run);
    let stdout = String::from_utf8(run.stdout.clone()).expect("utf-8 stdout");
    assert!(
        !stdout.contains('\u{25b6}'),
        "tool progress leaked to stdout: {stdout:?}"
    );
    // Whole-stdout equality; only a trailing newline is tolerated, so any
    // chrome before or after the answer fails.
    assert_eq!(
        stdout.trim_end_matches('\n'),
        ANSWER,
        "--print stdout must be exactly the answer; stderr={}",
        run.stderr
    );
    assert_tool_progress_on_stderr(&run);
}

/// Same leak under `--output-format json` would put non-JSON bytes before the
/// document, so `jq` fails. Stdout must parse as exactly one JSON value.
#[test]
fn json_mode_with_tool_turn_emits_exactly_one_value() {
    let run = run_tool_turn(&["--output-format", "json", "-p", "read a.txt"]);
    assert_reached_stub(&run);
    let stdout = String::from_utf8(run.stdout.clone()).expect("utf-8 stdout");
    let value: Result<serde_json::Value, _> = serde_json::from_str(stdout.trim());
    assert!(
        value.is_ok(),
        "json stdout must be exactly one JSON value: {:?}; stdout={stdout:?} stderr={}",
        value.err(),
        run.stderr
    );
    assert_tool_progress_on_stderr(&run);
}

/// Near-miss: without `--print`, the reserve is off and tool progress stays
/// on stdout exactly as before — the REPL / plain `-p` surface is unchanged.
#[test]
fn plain_prompt_mode_with_tool_turn_keeps_progress_on_stdout() {
    let run = run_tool_turn(&["-p", "read a.txt"]);
    assert_reached_stub(&run);
    let stdout = String::from_utf8(run.stdout.clone()).expect("utf-8 stdout");
    assert!(
        stdout.contains("\u{25b6} read a.txt"),
        "plain -p must keep tool progress on stdout; stdout={stdout:?} stderr={}",
        run.stderr
    );
    assert!(stdout.contains(ANSWER), "stdout={stdout:?}");
}

/// Thinking text the stub streams before the answer. Distinct from `ANSWER`
/// so a leak of the thinking block itself onto stdout is also caught.
const THINKING: &str = "stub is thinking about the answer";

/// A turn that streams a THINKING block (index 0) and then the text answer
/// (index 1) — the shape a real provider sends with thinking on. The
/// thinking→text transition is where a turn-start or "first delta"
/// separator newline would sit, and the plain `sse_body()` never crosses it.
fn sse_thinking_then_text_body() -> String {
    let thinking = serde_json::to_string(THINKING).unwrap();
    let text = serde_json::to_string(ANSWER).unwrap();
    sse_events(vec![
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_think","type":"message","role":"assistant","model":"claude-stub","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":5,"output_tokens":1}}}"#.to_string(),
        ),
        (
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#.to_string(),
        ),
        (
            "content_block_delta",
            format!(
                r#"{{"type":"content_block_delta","index":0,"delta":{{"type":"thinking_delta","thinking":{thinking}}}}}"#
            ),
        ),
        (
            "content_block_delta",
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"stub-signature"}}"#.to_string(),
        ),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#.to_string(),
        ),
        (
            "content_block_start",
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#.to_string(),
        ),
        (
            "content_block_delta",
            format!(
                r#"{{"type":"content_block_delta","index":1,"delta":{{"type":"text_delta","text":{text}}}}}"#
            ),
        ),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":1}"#.to_string(),
        ),
        (
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":9}}"#.to_string(),
        ),
        ("message_stop", r#"{"type":"message_stop"}"#.to_string()),
    ])
}

/// Day 212 (01:05): the assessment measured `--print` stdout as `\n\nPONG`.
/// Pin the FIRST byte and the whole buffer, UNTRIMMED, on the stream shape a
/// real thinking provider sends. `--print` writes the model's text verbatim
/// (`print!`, no added trailing newline), so stdout must equal `ANSWER`
/// byte-for-byte — any separator newline before or after it fails here, where
/// the older tests' `trim_end` would have forgiven the trailing half.
#[test]
fn print_mode_stdout_is_exactly_the_answer_bytes_after_a_thinking_block() {
    let body = sse_thinking_then_text_body();
    // Anti-vacuous: the stub really streams a thinking block before the text,
    // so this test crosses the thinking→text transition the plain body skips.
    assert!(
        body.contains("\"thinking_delta\"") && body.find("thinking_delta") < body.find("text_delta"),
        "fixture must stream a thinking delta before the text delta"
    );
    let run = run_yoyo_with(
        &["--no-tools", "--max-turns", "1"],
        &["--print", "-p", "hi"],
        None,
        vec![body],
    );
    assert_reached_stub(&run);
    assert_eq!(
        run.stdout.first().copied(),
        ANSWER.as_bytes().first().copied(),
        "--print stdout must START with the answer's first byte; stdout={:?} stderr={}",
        String::from_utf8_lossy(&run.stdout),
        run.stderr
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        ANSWER,
        "--print stdout must be exactly the answer bytes, untrimmed; stderr={}",
        run.stderr
    );
}

/// Same pin for piped stdin + `--print`: the second door into the reserve.
#[test]
fn piped_print_mode_stdout_is_exactly_the_answer_bytes_after_a_thinking_block() {
    let run = run_yoyo_with(
        &["--no-tools", "--max-turns", "1"],
        &["--print"],
        Some("hi\n"),
        vec![sse_thinking_then_text_body()],
    );
    assert_reached_stub(&run);
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        ANSWER,
        "piped --print stdout must be exactly the answer bytes, untrimmed; stderr={}",
        run.stderr
    );
}
