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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// The only text the stub's model ever says.
const ANSWER: &str = r#"{"answer": 42}"#;

/// One Anthropic Messages SSE stream whose only text is `ANSWER`.
fn sse_body() -> String {
    sse_text_body(ANSWER)
}

/// One Anthropic Messages SSE stream whose single text delta is `answer`.
fn sse_text_body(answer: &str) -> String {
    let text = serde_json::to_string(answer).unwrap();
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
fn start_stub_seq(bodies: Vec<String>) -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let hit = Arc::new(AtomicUsize::new(0));
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
                hit_thread.fetch_add(1, Ordering::SeqCst);
            }
            let payload = bodies[served.min(bodies.len() - 1)].clone();
            if request_line.starts_with("POST") {
                served += 1;
            }
            // A body that is already a full HTTP response (status line first)
            // is sent verbatim, so a test can serve a non-200 status (#987).
            let resp = if payload.starts_with("HTTP/1.1 ") {
                payload
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                )
            };
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
    /// The exit code (`None` for a signal death).
    code: Option<i32>,
    stub_hit: bool,
    /// POSTs the stub received (retries included).
    requests: usize,
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
    run_yoyo_within(base, extra, stdin, bodies, Duration::from_secs(20))
}

/// `run_yoyo_with` with an explicit wall-clock budget (retry backoff needs more).
fn run_yoyo_within(
    base: &[&str],
    extra: &[&str],
    stdin: Option<&str>,
    bodies: Vec<String>,
    budget: Duration,
) -> Run {
    run_yoyo_seeded(&[], base, extra, stdin, bodies, budget)
}

/// `run_yoyo_within` with extra `(relative path, contents)` files seeded in the cwd.
fn run_yoyo_seeded(
    files: &[(&str, &str)],
    base: &[&str],
    extra: &[&str],
    stdin: Option<&str>,
    bodies: Vec<String>,
    budget: Duration,
) -> Run {
    let (port, hit) = start_stub_seq(bodies);
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    for (rel, contents) in files {
        let path = cwd.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
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
    let deadline = Instant::now() + budget;
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let out = child.wait_with_output().unwrap();
            panic!(
                "yoyo did not exit within {budget:?}; stdout={:?} stderr={}",
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
        code: out.status.code(),
        stub_hit: hit.load(Ordering::SeqCst) > 0,
        requests: hit.load(Ordering::SeqCst),
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
        body.contains("\"thinking_delta\"")
            && body.find("thinking_delta") < body.find("text_delta"),
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

/// Day 212 (evaluator re-run): with the default model `claude-opus-4-6` the
/// provider's FIRST text delta itself is `"\n\nPONG"` (seen verbatim in
/// `--output-format stream-json`), so `--print` stdout began `\n\n` even with
/// every chrome emitter already on stderr. The stub replays that exact shape.
const LEADING_BLANK_LINES: &str = "\n\n";

#[test]
fn print_mode_strips_model_emitted_leading_blank_lines() {
    let body = sse_text_body(&format!("{LEADING_BLANK_LINES}{ANSWER}"));
    // Anti-vacuous: the stub really sends the answer prefixed by blank lines.
    assert!(
        body.contains(r#""text":"\n\n{"#),
        "fixture must stream a text delta that opens with two newlines"
    );
    let run = run_yoyo_with(
        &["--no-tools", "--max-turns", "1"],
        &["--print", "-p", "hi"],
        None,
        vec![body.clone()],
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

    // Piped door into the same reserve.
    let piped = run_yoyo_with(
        &["--no-tools", "--max-turns", "1"],
        &["--print"],
        Some("hi\n"),
        vec![body.clone()],
    );
    assert_reached_stub(&piped);
    assert_eq!(String::from_utf8_lossy(&piped.stdout), ANSWER);

    // JSON envelope: same payload in `response`.
    let json = run_yoyo_with(
        &["--no-tools", "--max-turns", "1"],
        &["--output-format", "json", "-p", "hi"],
        None,
        vec![body],
    );
    assert_reached_stub(&json);
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap_or_else(|e| {
        panic!(
            "json stdout must parse: {e}; stdout={:?}",
            String::from_utf8_lossy(&json.stdout)
        )
    });
    assert_eq!(v["response"], ANSWER);
}

/// Day 215: plain `-p` (no reserve) streams too, and its stdout no longer
/// leads with blank lines — the model's `\n\n` and yoyo's own first-text
/// framing newline are both dropped, so the FIRST stdout byte is the answer's.
/// (Superseded pin, recorded rather than erased: until Day 215 this test was
/// `plain_prompt_mode_still_streams_model_leading_blank_lines` and asserted the
/// leading blank lines reached stdout verbatim.)
#[test]
fn plain_prompt_mode_still_streams_model_leading_blank_lines() {
    let run = run_yoyo_with(
        &["--no-tools", "--max-turns", "1"],
        &["-p", "hi"],
        None,
        vec![sse_text_body(&format!("{LEADING_BLANK_LINES}{ANSWER}"))],
    );
    assert_reached_stub(&run);
    let out = String::from_utf8_lossy(&run.stdout);
    assert!(
        LEADING_BLANK_LINES.starts_with('\n'),
        "anti-vacuous: the fixture must open with a blank line"
    );
    assert!(
        out.starts_with(ANSWER),
        "plain -p stdout must start with the answer; stdout={out:?}"
    );
    // Day 215 (trailing mirror): the stdout ends with exactly ONE newline.
    // Superseded pin, recorded rather than erased: this was
    // `format!("{ANSWER}\n\n")` — yoyo's two end-of-turn framing writes —
    // a scope decision of the leading-blank task, not a want.
    assert_eq!(out, format!("{ANSWER}\n"), "stdout={out:?}");
}

/// Distinctive partial answer the broken stream delivers before it dies.
const PARTIAL: &str = "PARTIAL_ANSWER_7Q";

/// A stream that delivers `PARTIAL` as a text delta and then dies on an SSE
/// `error` event (`overloaded_error`) — no `content_block_stop`, no
/// `message_stop`. Served to EVERY request, so every retry dies the same way.
fn sse_dies_mid_stream_body() -> String {
    sse_dies_mid_stream_body_with(PARTIAL)
}

/// [`sse_dies_mid_stream_body`] with an arbitrary partial text delta.
fn sse_dies_mid_stream_body_with(partial: &str) -> String {
    let text = serde_json::to_string(partial).unwrap();
    sse_events(vec![
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_die","type":"message","role":"assistant","model":"claude-stub","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":5,"output_tokens":1}}}"#.to_string(),
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
            "error",
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#.to_string(),
        ),
    ])
}

/// Transferred class (Claude Code: "`-p` text output dropping the answer
/// already produced when a turn dies on a mid-stream API error"). A turn whose
/// stream dies after text arrived must put that text on stdout under
/// `--print`, while the NONZERO exit says it failed.
#[test]
fn print_mode_keeps_partial_answer_when_turn_dies_mid_stream() {
    let run = run_yoyo_within(
        &["--no-tools", "--max-turns", "1"],
        &["--print", "-p", "hi"],
        None,
        vec![sse_dies_mid_stream_body()],
        Duration::from_secs(120),
    );
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    eprintln!(
        "PROBE readings: stdout={stdout:?} success={} requests={} stderr_tail={:?}",
        run.success,
        run.requests,
        run.stderr.lines().rev().take(4).collect::<Vec<_>>()
    );
    assert!(run.stub_hit, "anti-vacuous: stub never received a POST");
    assert!(
        !run.success,
        "a dead turn must exit nonzero; stderr={}",
        run.stderr
    );
    assert!(
        run.stderr.to_lowercase().contains("error") || run.stderr.contains("failed"),
        "stderr must carry the failure; stderr={}",
        run.stderr
    );
    assert_eq!(
        stdout, PARTIAL,
        "--print stdout must carry the partial answer already produced; stderr={}",
        run.stderr
    );
}

/// Partial answer ending in a line terminator, so the armed stdout filter's
/// trailing-whitespace HOLD (`src/stream_leading_blank.rs`, Day 215) is in
/// play when the stream dies. `PARTIAL` alone has no trailing whitespace and
/// only reaches the reserved `--print` door, which bypasses that filter.
const PARTIAL_LINE: &str = "PARTIAL_ANSWER_7Q\n";

/// The unreserved, filter-ARMED doors (plain `-p`, piped stdin without
/// `--print`) when the stream dies mid-turn after `PARTIAL_LINE`. Day 212's
/// test above covers only `--print`, whose payload never passes through the
/// streaming filter, so the hold-back added on Day 215 had never met an error
/// path. Asserts: the streamed text survives on stdout (the filter may drop
/// only trailing whitespace, never text), the dead turn exits nonzero, and the
/// error is on stderr, not stdout.
///
/// Measured Day 215 (20:47): this already holds (probed, no code change).
/// The same reading shows two OTHER defects this test deliberately does not
/// pin: each retry re-streams the partial (one copy per attempt on stdout),
/// and two BEL bytes reach stdout. Day 216 (#976 half 2): now one copy, pinned by `assert_eq!`.
#[test]
fn armed_doors_keep_streamed_partial_answer_when_turn_dies_mid_stream() {
    let body = sse_dies_mid_stream_body_with(PARTIAL_LINE);
    // Anti-vacuous: the fixture really streams text ending in a newline,
    // then dies with no message_stop.
    assert!(body.contains(r#""text":"PARTIAL_ANSWER_7Q\n""#));
    assert!(body.contains("event: error") && !body.contains("message_stop"));

    let doors: [(&str, &[&str], Option<&str>); 2] = [
        ("plain -p", &["-p", "hi"], None),
        ("piped, no --print", &[], Some("hi\n")),
    ];
    for (door, extra, stdin) in doors {
        let run = run_yoyo_within(
            &["--no-tools", "--max-turns", "1"],
            extra,
            stdin,
            vec![body.clone()],
            Duration::from_secs(120),
        );
        let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
        eprintln!(
            "PROBE [{door}]: stdout={stdout:?} success={} requests={} stderr_tail={:?}",
            run.success,
            run.requests,
            run.stderr.lines().rev().take(4).collect::<Vec<_>>()
        );
        assert!(
            run.stub_hit,
            "[{door}] anti-vacuous: stub never received a POST"
        );
        assert!(
            !run.success,
            "[{door}] a dead turn must exit nonzero; stderr={}",
            run.stderr
        );
        assert!(
            stdout.contains(PARTIAL_LINE.trim_end()),
            "[{door}] stdout must keep the text already streamed; stdout={stdout:?} stderr={}",
            run.stderr
        );
        assert!(
            run.stderr.contains("Overloaded") || run.stderr.to_lowercase().contains("error"),
            "[{door}] stderr must carry the failure; stderr={}",
            run.stderr
        );
        assert!(
            !stdout.contains("Overloaded") && !stdout.to_lowercase().contains("error"),
            "[{door}] the error must not land on stdout; stdout={stdout:?}"
        );
        // #976 half 1 (BEL): the terminal bell is chrome for a human at a
        // tty, never payload. A retried turn is slow enough to cross the
        // bell threshold, and it used to write two 0x07 bytes here.
        // Anti-vacuous: stdout really carries the streamed answer, so an
        // empty buffer cannot pass the byte check by having nothing in it.
        assert!(
            !run.stdout.is_empty() && stdout.contains("PARTIAL_ANSWER_7Q"),
            "[{door}] anti-vacuous: stdout must carry the partial answer; stdout={stdout:?}"
        );
        assert!(
            !run.stdout.contains(&0x07),
            "[{door}] #976: no BEL byte may land on stdout (a pipe); stdout={stdout:?}"
        );
        // #976 half 2: the partial answer used to be re-streamed once per
        // retry (six copies, Day 216). A non-terminal stdout that already
        // holds text is not retried, so a pipe gets exactly ONE copy.
        assert_eq!(
            stdout, PARTIAL_LINE,
            "[{door}] #976: exactly one copy of the partial; stderr={}",
            run.stderr
        );
        // #991 (Day 226): the in-stream `overloaded_error` is a
        // yoagent-retried `RateLimited` (0.24.3+), and behind
        // `retry_safe_events` those 3 retries print nothing, so they are now
        // let through: 1 + 3 requests. Only the FINAL failure's partial
        // reaches the pipe, and yoyo's own loop still refuses to retry it.
        // Superseded expectation, recorded rather than erased: 1 request
        // (Day 216-226), when yoyo aborted yoagent's first retry.
        assert_eq!(
            run.requests, 4,
            "[{door}] #991: yoagent's silent retries, then no yoyo retry after streamed text"
        );
        assert!(
            run.stderr
                .contains("not retrying: part of the answer was already written"),
            "[{door}] #976: stderr must say why the retry was skipped; stderr={}",
            run.stderr
        );
    }
}

/// #976 near-miss: an attempt that dies BEFORE writing any text (overloaded at
/// request start) must still retry on a pipe, and the successful answer lands
/// exactly once with exit 0.
#[test]
fn armed_door_retries_when_turn_dies_before_any_text() {
    let early_death = sse_events(vec![
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_die0","type":"message","role":"assistant","model":"claude-stub","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":5,"output_tokens":1}}}"#.to_string(),
        ),
        (
            "error",
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#.to_string(),
        ),
    ]);
    assert!(!early_death.contains("text_delta"), "anti-vacuous: no text");
    let run = run_yoyo_within(
        &["--no-tools", "--max-turns", "1"],
        &["-p", "hi"],
        None,
        vec![early_death, sse_text_body(PARTIAL_LINE)],
        Duration::from_secs(120),
    );
    assert_eq!(
        run.requests, 2,
        "the early death must be retried; stderr={}",
        run.stderr
    );
    assert!(
        run.success,
        "retry succeeded, exit 0; stderr={}",
        run.stderr
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), PARTIAL_LINE);
}

/// Near-miss for the test above: the same partial text on a stream that
/// finishes normally stays byte-identical on the armed door.
#[test]
fn armed_door_completed_stream_of_partial_line_is_byte_identical() {
    let run = run_yoyo_with(
        &["--no-tools", "--max-turns", "1"],
        &["-p", "hi"],
        None,
        vec![sse_text_body(PARTIAL_LINE)],
    );
    assert_reached_stub(&run);
    assert_eq!(String::from_utf8_lossy(&run.stdout), PARTIAL_LINE);
}

// ---------------------------------------------------------------------------
// #997: the opt-in `retry_after_partial = true` (project `.yoyo.toml`).
// The default rows are the #976 tests above, left unedited; these pin what the
// opt-in changes on the armed `-p` door with stdout a pipe.
// ---------------------------------------------------------------------------

/// Answer of the attempt that succeeds after the opt-in retry.
const OPT_IN_ANSWER: &str = "PONG\n";

/// The opt-in seeded as a project config in the run's cwd.
const OPT_IN_CONFIG: (&str, &str) = (".yoyo.toml", "retry_after_partial = true\n");

fn run_opt_in(bodies: Vec<String>) -> Run {
    let run = run_yoyo_seeded(
        &[OPT_IN_CONFIG],
        &["--no-tools", "--max-turns", "1"],
        &["-p", "hi"],
        None,
        bodies,
        Duration::from_secs(150),
    );
    eprintln!(
        "PROBE #997: stdout={:?} success={} requests={} stderr_tail={:?}",
        String::from_utf8_lossy(&run.stdout),
        run.success,
        run.requests,
        run.stderr.lines().rev().take(6).collect::<Vec<_>>()
    );
    run
}

/// The row #997 exists for: text streamed, then a transient Overloaded, then
/// a clean answer. Default: exit 1 after 1 request (the #976 test above).
/// Opt-in: the turn is retried and succeeds. The cost is named, not hidden:
/// the dying attempt's partial already sits in the pipe, so stdout carries it
/// once, followed by the retried answer.
#[test]
fn opt_in_retries_after_streamed_partial_and_succeeds() {
    let run = run_opt_in(vec![
        sse_dies_mid_stream_body_with(PARTIAL_LINE),
        sse_text_body(OPT_IN_ANSWER),
    ]);
    assert_eq!(
        run.requests, 2,
        "#997: opted in, the dying attempt is retried; stderr={}",
        run.stderr
    );
    assert!(
        run.success,
        "#997: the retry succeeded, so exit 0; stderr={}",
        run.stderr
    );
    // #991 (Day 226): the in-stream `overloaded_error` is a yoagent-retried
    // `RateLimited` since 0.24.3, so this death is now retried INSIDE yoagent,
    // behind `retry_safe_events`: the partial never reaches the pipe.
    // Superseded expectation, recorded rather than erased: Day 225 pinned
    // "PARTIAL_ANSWER_7Q\n\n\nPONG\n" (partial once, separator, answer).
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        OPT_IN_ANSWER,
        "#991: only the successful attempt reaches the pipe; stderr={}",
        run.stderr
    );
    assert!(
        !run.stderr
            .contains("not retrying: part of the answer was already written"),
        "#997: the #976 skip note must not print when the retry happens; stderr={}",
        run.stderr
    );
}

/// Sibling coupling (#991's motivating input): a death BEFORE any text must
/// still give exactly the clean answer when the opt-in is on.
#[test]
fn opt_in_death_before_text_still_yields_exactly_the_answer() {
    let early_death = sse_events(vec![
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_die0","type":"message","role":"assistant","model":"claude-stub","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":5,"output_tokens":1}}}"#.to_string(),
        ),
        (
            "error",
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#.to_string(),
        ),
    ]);
    assert!(!early_death.contains("text_delta"), "anti-vacuous: no text");
    let run = run_opt_in(vec![early_death, sse_text_body(OPT_IN_ANSWER)]);
    assert_eq!(run.requests, 2, "stderr={}", run.stderr);
    assert!(run.success, "stderr={}", run.stderr);
    assert_eq!(String::from_utf8_lossy(&run.stdout), OPT_IN_ANSWER);
}

/// Day-216 row: the case the widened retry does NOT save. Every attempt dies
/// after streaming text; the opt-in must still terminate, nonzero, bounded,
/// never loop.
///
/// Superseded reading (yoagent 0.25.2 upgrade, Day 225, #1007), recorded rather
/// than erased: this row pinned 6 requests (MAX_RETRIES + 1). The fixture dies
/// with an in-stream `overloaded_error`, which yoagent >= 0.24.3 classifies as
/// a retried `RateLimited` (`provider/traits.rs:435`). So the death now goes
/// through yoagent's own retry first (1 + `RetryConfig::default`'s 3) and only
/// then reaches yoyo's 6 attempts: 24 requests, measured. This is the same
/// worst case `opt_in_provider_retry_every_attempt_dying_still_exits_nonzero`
/// already pins for `rate_limit_error`, as `retry_after_partial.rs` predicted.
/// Opt-in only: with the default (off) the #989 rows above stay at 1 request.
/// (Superseded Day 226, #991: the default rows now take 4 requests — yoagent's
/// retries are held by `retry_safe_events` and let through — and this row
/// carries 6 partials, one per yoyo attempt, not 24.)
#[test]
fn opt_in_every_attempt_dying_after_text_terminates_nonzero_within_the_cap() {
    let run = run_opt_in(vec![sse_dies_mid_stream_body_with(PARTIAL_LINE)]);
    assert!(
        !run.success,
        "every attempt failed, so exit nonzero; stderr={}",
        run.stderr
    );
    assert_eq!(
        run.requests, 24,
        "bounded: 4 yoagent attempts x 6 yoyo attempts; stderr={}",
        run.stderr
    );
    // #991 (Day 226): one partial per YOYO attempt (6), not per request (24,
    // Day 225's reading, superseded rather than erased): yoagent's own retries
    // are held and dropped by `retry_safe_events`; each final failure keeps
    // its partial once.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        [PARTIAL_LINE; 6].join("\n\n"),
        "the cost of the opt-in at its worst: one partial per yoyo attempt; stderr={}",
        run.stderr
    );
    assert!(
        run.stderr.contains("failed after 6 attempts"),
        "yoyo's own cap still reported; stderr={}",
        run.stderr
    );
    // Superseded claim (0.25.2 upgrade, Day 225, #1007), recorded rather than
    // erased: this asserted stderr contains "Overloaded" — the provider's own
    // message. yoagent 0.25.2 reclassifies the in-stream `overloaded_error`
    // as `RateLimited`, which carries no message (yoagent CHANGELOG: "its
    // text now reads 'Rate limited'"), so the server's text no longer reaches
    // yoyo. The failure is still on stderr, under its new classification.
    assert!(
        run.stderr.contains("Rate limited"),
        "stderr carries the failure; stderr={}",
        run.stderr
    );
}

// ── The whole contract in one scenario ───────────────────────────────────
//
// Five point fixes in two days (#966 duplication, the tool-progress leak,
// model-emitted leading blank lines, the dropped partial answer, help text)
// each added a test for ITS OWN emitter. The tests above therefore each
// exercise one writer at a time, and a sixth writer that only fires when two
// features meet (say, a separator emitted at the tool-result -> thinking
// transition) would pass every one of them. This scenario puts every stdout
// writer the stub can provoke into ONE run and pins the whole buffer:
//
//   turn 1: thinking block -> `read_file` tool call (a.txt in the tempdir cwd)
//   (tool result: the real read_file runs, its progress line is chrome)
//   turn 2: thinking block -> text delta "\n\n" -> text delta = answer,
//           whose body carries an internal blank line
//
// Writers covered: the streamed text renderer, turn-boundary / tool progress
// chrome, the thinking renderer, the leading-blank-line strip, and
// `emit_output` itself (the single sanctioned stdout writer). Not covered:
// the `--help` text (no model turn at all) and the mid-stream-death path
// (a nonzero-exit run) — each keeps its own test above.

/// Final answer for the whole-contract run. The internal blank line is the
/// near-miss: the leading-blank-line strip must NOT touch blank lines that
/// sit inside the answer.
const CONTRACT_ANSWER: &str = "first line of the answer\n\nthird line after a blank";

/// Build one assistant turn from content blocks. Each block is
/// (`content_block_start` JSON for `content_block`, list of delta JSONs).
fn sse_turn(id: &str, blocks: Vec<(String, Vec<String>)>, stop_reason: &str) -> String {
    let mut events: Vec<(&str, String)> = vec![(
        "message_start",
        format!(
            r#"{{"type":"message_start","message":{{"id":"{id}","type":"message","role":"assistant","model":"claude-stub","content":[],"stop_reason":null,"stop_sequence":null,"usage":{{"input_tokens":5,"output_tokens":1}}}}}}"#
        ),
    )];
    for (index, (block, deltas)) in blocks.into_iter().enumerate() {
        events.push((
            "content_block_start",
            format!(r#"{{"type":"content_block_start","index":{index},"content_block":{block}}}"#),
        ));
        for delta in deltas {
            events.push((
                "content_block_delta",
                format!(r#"{{"type":"content_block_delta","index":{index},"delta":{delta}}}"#),
            ));
        }
        events.push((
            "content_block_stop",
            format!(r#"{{"type":"content_block_stop","index":{index}}}"#),
        ));
    }
    events.push((
        "message_delta",
        format!(
            r#"{{"type":"message_delta","delta":{{"stop_reason":"{stop_reason}","stop_sequence":null}},"usage":{{"output_tokens":9}}}}"#
        ),
    ));
    events.push(("message_stop", r#"{"type":"message_stop"}"#.to_string()));
    sse_events(events)
}

fn thinking_block(text: &str) -> (String, Vec<String>) {
    let t = serde_json::to_string(text).unwrap();
    (
        r#"{"type":"thinking","thinking":""}"#.to_string(),
        vec![
            format!(r#"{{"type":"thinking_delta","thinking":{t}}}"#),
            r#"{"type":"signature_delta","signature":"stub-signature"}"#.to_string(),
        ],
    )
}

fn text_block(deltas: &[&str]) -> (String, Vec<String>) {
    (
        r#"{"type":"text","text":""}"#.to_string(),
        deltas
            .iter()
            .map(|d| {
                let t = serde_json::to_string(d).unwrap();
                format!(r#"{{"type":"text_delta","text":{t}}}"#)
            })
            .collect(),
    )
}

/// The two stub turns of the whole-contract scenario.
fn contract_bodies() -> Vec<String> {
    let partial = serde_json::to_string(r#"{"path": "a.txt"}"#).unwrap();
    let turn1 = sse_turn(
        "msg_contract_1",
        vec![
            thinking_block(THINKING),
            (
                r#"{"type":"tool_use","id":"toolu_contract_1","name":"read_file","input":{}}"#
                    .to_string(),
                vec![format!(
                    r#"{{"type":"input_json_delta","partial_json":{partial}}}"#
                )],
            ),
        ],
        "tool_use",
    );
    let turn2 = sse_turn(
        "msg_contract_2",
        vec![
            thinking_block(THINKING),
            text_block(&[LEADING_BLANK_LINES, CONTRACT_ANSWER]),
        ],
        "end_turn",
    );
    vec![turn1, turn2]
}

fn run_contract(extra: &[&str]) -> Run {
    let bodies = contract_bodies();
    // Anti-vacuous: the fixture really carries every emitter's trigger, so a
    // green result cannot come from a stub that skipped one.
    assert!(bodies[0].contains("thinking_delta") && bodies[0].contains("\"tool_use\""));
    assert!(bodies[1].contains("thinking_delta"));
    assert!(
        bodies[1].contains(r#""text":"\n\n"}"#),
        "turn 2 must stream a text delta that is exactly two newlines"
    );
    assert!(
        CONTRACT_ANSWER.contains("\n\n"),
        "near-miss needs an internal blank line"
    );
    run_yoyo_with(&["--max-turns", "4"], extra, None, bodies)
}

/// One run, every stdout writer the stub can provoke: `--print` stdout must be
/// EXACTLY the final answer bytes — no leading blank lines, no tool progress,
/// no thinking text, no turn separators, internal blank line intact.
#[test]
fn print_mode_whole_contract_stdout_is_exactly_the_answer() {
    let run = run_contract(&["--print", "-p", "read a.txt"]);
    assert_reached_stub(&run);
    assert!(
        run.requests >= 2,
        "anti-vacuous: the tool turn must be followed by a second request; requests={}",
        run.requests
    );
    assert_tool_progress_on_stderr(&run);
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        CONTRACT_ANSWER,
        "--print stdout must be exactly the final answer bytes, untrimmed; stderr={}",
        run.stderr
    );
}

/// Same scenario under `--output-format json`: stdout is exactly ONE JSON
/// value with nothing before or after it (only the envelope's own trailing
/// newline), and its `response` is the answer with the internal blank line.
#[test]
fn json_mode_whole_contract_stdout_is_exactly_one_value() {
    let run = run_contract(&["--output-format", "json", "-p", "read a.txt"]);
    assert_reached_stub(&run);
    assert!(run.requests >= 2, "requests={}", run.requests);
    assert_tool_progress_on_stderr(&run);
    let stdout = String::from_utf8(run.stdout.clone()).expect("utf-8 stdout");
    // No stray byte before the document: the very first byte opens it.
    assert_eq!(
        stdout.as_bytes().first().copied(),
        Some(b'{'),
        "json stdout must start with the document; stdout={stdout:?} stderr={}",
        run.stderr
    );
    let values: Vec<serde_json::Value> = serde_json::Deserializer::from_str(&stdout)
        .into_iter::<serde_json::Value>()
        .collect::<Result<_, _>>()
        .unwrap_or_else(|e| panic!("json stdout must parse cleanly: {e}; stdout={stdout:?}"));
    assert_eq!(values.len(), 1, "exactly one JSON value; stdout={stdout:?}");
    // Nothing after the value but the single newline `println!` adds.
    assert_eq!(
        stdout,
        format!("{}\n", stdout.trim_end_matches('\n')),
        "only one trailing newline may follow the document; stdout={stdout:?}"
    );
    assert_eq!(
        values[0]["response"], CONTRACT_ANSWER,
        "envelope response must be the answer, internal blank line intact"
    );
}

/// #979: `--continue-strict` on a corrupt session refuses BEFORE any model call.
/// This is the file's one deliberate exception to "every test hits the stub":
/// zero POSTs is the claim. Its anti-vacuous half is the paired lenient run
/// on the same corrupt file, which must reach the stub and exit 0.
#[test]
fn continue_strict_refuses_corrupt_session_before_any_model_call() {
    let files = [(".yoyo/last-session.json", "{ not json")];
    let base = ["--no-tools", "--max-turns", "1"];
    let budget = Duration::from_secs(20);
    let strict = run_yoyo_seeded(
        &files,
        &base,
        &["-p", "x", "--continue-strict"],
        None,
        vec![sse_body()],
        budget,
    );
    assert!(
        !strict.success,
        "#979: strict must exit non-zero; stderr={}",
        strict.stderr
    );
    assert!(
        strict.stderr.contains("--continue-strict"),
        "stderr={}",
        strict.stderr
    );
    assert_eq!(
        strict.requests, 0,
        "#979: no model call may be made; stderr={}",
        strict.stderr
    );

    let lenient = run_yoyo_seeded(
        &files,
        &base,
        &["-p", "x", "--continue"],
        None,
        vec![sse_body()],
        budget,
    );
    assert_reached_stub(&lenient);
    assert!(
        lenient.stderr.contains("Failed to restore session"),
        "stderr={}",
        lenient.stderr
    );
}

/// #978: assert `path` holds a session that parses as a non-empty message list.
fn assert_saved_session(path: &std::path::Path, run: &Run) {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("#978: no session at {path:?} ({e}); stderr={}", run.stderr));
    let v: serde_json::Value = serde_json::from_str(&raw).expect("session must be JSON");
    let n = v.as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n >= 1, "#978: saved session must carry the turn; got {raw}");
}

/// #978 door 1: `-p` with `--save-session` writes a resumable session.
#[test]
fn save_session_prompt_door_writes_the_session() {
    let out = tempfile::tempdir().unwrap();
    let path = out.path().join("s.json");
    let p = path.to_str().unwrap();
    let run = run_yoyo(&["-p", "x", "--save-session", p], None);
    assert_reached_stub(&run);
    assert_saved_session(&path, &run);
}

/// #978 door 2: the piped-stdin door is wired too (two doors, one policy).
#[test]
fn save_session_piped_door_writes_the_session() {
    let out = tempfile::tempdir().unwrap();
    let path = out.path().join("s.json");
    let p = path.to_str().unwrap();
    let run = run_yoyo(&["--save-session", p], Some("x"));
    assert_reached_stub(&run);
    assert_saved_session(&path, &run);
}

/// #978: a missing parent dir is never created; the run fails and says why.
#[test]
fn save_session_missing_parent_dir_fails_loudly() {
    let out = tempfile::tempdir().unwrap();
    let path = out.path().join("no/such/dir/s.json");
    let p = path.to_str().unwrap();
    let run = run_yoyo(&["-p", "x", "--save-session", p], None);
    assert!(run.stub_hit, "anti-vacuous: the turn must have run");
    assert!(!run.success, "#978: a failed save must exit non-zero");
    assert!(
        run.stderr
            .contains("error: --save-session: could not write"),
        "stderr={}",
        run.stderr
    );
    assert!(
        !out.path().join("no").exists(),
        "parent dir must not be created"
    );
}

/// #978 near-miss: without the flag, a `-p` run exits 0 and says nothing about
/// saving. (Weak by design: the run's cwd is a dropped tempdir, so this pins the
/// exit and the absence of the message, not the absence of every file.)
#[test]
fn save_session_absent_flag_is_default_behaviour() {
    let run = run_yoyo(&["-p", "x"], None);
    assert_reached_stub(&run);
    assert!(
        !run.stderr.contains("--save-session"),
        "stderr={}",
        run.stderr
    );
}

/// #978: a turn that dies mid-stream still writes the session, and the run
/// keeps its own failure exit (the save never turns a failure into 0).
#[test]
fn save_session_written_after_a_failed_turn() {
    let out = tempfile::tempdir().unwrap();
    let path = out.path().join("s.json");
    let p = path.to_str().unwrap();
    let run = run_yoyo_within(
        &["--no-tools", "--max-turns", "1"],
        &["--print", "-p", "hi", "--save-session", p],
        None,
        vec![sse_dies_mid_stream_body()],
        Duration::from_secs(120),
    );
    assert!(run.stub_hit, "anti-vacuous: stub never received a POST");
    assert!(
        !run.success,
        "a dead turn must exit nonzero; stderr={}",
        run.stderr
    );
    assert_saved_session(&path, &run);
}

/// stdout split into NDJSON lines, each parsed (panics on a non-JSON line).
fn ndjson_lines(run: &Run) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&run.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("non-JSON line {l:?}: {e}")))
        .collect()
}

/// #979 half 2: stream-json acknowledges a restored session with
/// `{"type":"sessionRestored","messages":N}` directly after `agentStart`.
/// The seeded session is produced by the REAL writer (`--save-session` in a
/// first run), never a typed JSON literal, and N is read back from it.
#[test]
fn stream_json_continue_emits_session_restored_after_agent_start() {
    let out = tempfile::tempdir().unwrap();
    let saved = out.path().join("s.json");
    let p = saved.to_str().unwrap();
    let first = run_yoyo(&["-p", "x", "--save-session", p], None);
    assert_reached_stub(&first);
    let session = std::fs::read_to_string(&saved).expect("real writer saved a session");
    let n = serde_json::from_str::<serde_json::Value>(&session)
        .unwrap()
        .as_array()
        .map(|a| a.len())
        .unwrap();
    assert!(n >= 1, "anti-vacuous: the seeded session must be non-empty");

    let files = [(".yoyo/last-session.json", session.as_str())];
    let base = ["--no-tools", "--max-turns", "1"];
    let budget = Duration::from_secs(20);
    for flag in ["--continue", "--continue-strict"] {
        let run = run_yoyo_seeded(
            &files,
            &base,
            &["--output-format", "stream-json", "-p", "x", flag],
            None,
            vec![sse_body()],
            budget,
        );
        assert_reached_stub(&run);
        let raw = String::from_utf8_lossy(&run.stdout).into_owned();
        let lines: Vec<&str> = raw.lines().collect();
        assert_eq!(lines[0], r#"{"type":"agentStart"}"#, "{flag}: stdout={raw}");
        assert_eq!(
            lines[1],
            format!(r#"{{"type":"sessionRestored","messages":{n}}}"#),
            "{flag}: stdout={raw}"
        );
        assert_eq!(
            occurrences(&raw, "sessionRestored"),
            1,
            "{flag}: exactly once; stdout={raw}"
        );
    }

    // Near-miss: the same stream-json run without --continue emits no such line.
    let fresh = run_yoyo_seeded(
        &files,
        &base,
        &["--output-format", "stream-json", "-p", "x"],
        None,
        vec![sse_body()],
        budget,
    );
    assert_reached_stub(&fresh);
    let lines = ndjson_lines(&fresh);
    assert_eq!(lines[0]["type"], "agentStart");
    assert!(
        lines.iter().all(|v| v["type"] != "sessionRestored"),
        "no restore -> no sessionRestored line"
    );
}

/// #979 half 2 near-miss: a lenient `--continue` whose restore FAILED emits no
/// `sessionRestored` (never `messages: 0`, which would read as an empty session).
#[test]
fn stream_json_failed_lenient_continue_emits_no_session_restored() {
    let files = [(".yoyo/last-session.json", "{ not json")];
    let run = run_yoyo_seeded(
        &files,
        &["--no-tools", "--max-turns", "1"],
        &["--output-format", "stream-json", "-p", "x", "--continue"],
        None,
        vec![sse_body()],
        Duration::from_secs(20),
    );
    assert_reached_stub(&run);
    let lines = ndjson_lines(&run);
    assert_eq!(lines[0]["type"], "agentStart");
    assert!(lines.iter().all(|v| v["type"] != "sessionRestored"));
}

// ---------------------------------------------------------------------------
// #987: yoagent 0.24's OWN provider retry, one layer below yoyo's.
//
// yoagent retries inside the agent loop when `ProviderError::is_retryable()`
// holds, i.e. `RateLimited` (HTTP 429, or an SSE `error` event whose type is
// `rate_limit_error`) or `Network` (including a stream that ends with no
// `message_stop` and no stop_reason). A failed attempt's partial text has
// already streamed by then. The Day-216 guard (`should_retry_after_partial`)
// lives in yoyo's own retry loop and cannot see these retries, so each case
// below drives a retry that ONLY yoagent performs (the `overloaded_error`
// fixtures above classify as `Api`, which yoagent does not retry).
// ---------------------------------------------------------------------------

/// Answer of the successful second attempt in the #987 probes.
const RETRY_ANSWER: &str = "PONG\n";

/// HTTP 429 with `Retry-After: 0`, served before any text. yoagent classifies
/// it as `RateLimited` and retries; `Retry-After: 0` keeps the backoff at 0s.
fn http_429_response() -> String {
    let body = r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#;
    format!(
        "HTTP/1.1 429 Too Many Requests\r\ncontent-type: application/json\r\nretry-after: 0\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

/// Streams `partial` as a text delta, then ends the stream with `tail`
/// (`None` = connection closes with no terminator at all).
fn sse_partial_then(partial: &str, tail: Option<(&str, String)>) -> String {
    let text = serde_json::to_string(partial).unwrap();
    let mut events = vec![
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_retry","type":"message","role":"assistant","model":"claude-stub","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":5,"output_tokens":1}}}"#.to_string(),
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
    ];
    if let Some(t) = tail {
        events.push(t);
    }
    sse_events(events)
}

fn probe_run(bodies: Vec<String>) -> Run {
    let run = run_yoyo_within(
        &["--no-tools", "--max-turns", "1"],
        &["-p", "hi"],
        None,
        bodies,
        Duration::from_secs(120),
    );
    eprintln!(
        "PROBE #987: stdout={:?} success={} requests={} stderr_tail={:?}",
        String::from_utf8_lossy(&run.stdout),
        run.success,
        run.requests,
        run.stderr.lines().rev().take(6).collect::<Vec<_>>()
    );
    run
}

/// The case where retry WORKS (Day-216 lesson: measure the capability a fix
/// might narrow). A 429 before any text is retried by yoagent and the answer
/// lands exactly once with exit 0, and the retried attempt's error does not
/// make yoyo classify the recovered turn as failed (risk A).
#[test]
fn yoagent_retry_after_429_before_text_yields_one_clean_answer() {
    let run = probe_run(vec![http_429_response(), sse_text_body(RETRY_ANSWER)]);
    assert_eq!(
        run.requests, 2,
        "anti-vacuous: yoagent must have retried the 429; stderr={}",
        run.stderr
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        RETRY_ANSWER,
        "stderr={}",
        run.stderr
    );
    assert!(
        run.success,
        "#987 (A): a recovered turn must exit 0; stderr={}",
        run.stderr
    );
}

/// Partial text, then a mid-stream `rate_limit_error` SSE event: yoagent
/// retries (RateLimited) after `PARTIAL_` already streamed (risk B).
/// Measured Day 218: stdout="PARTIAL_\n\nPONG\n", exit 0, requests=2.
/// #991 (Day 226): this IS the case #991 changes; see `assert_partial_retry_reading`.
#[test]
fn yoagent_retry_after_partial_rate_limit_does_not_duplicate_stdout() {
    let first = sse_partial_then(
        "PARTIAL_",
        Some((
            "error",
            r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#
                .to_string(),
        )),
    );
    assert!(first.contains("text_delta") && !first.contains("message_stop"));
    let run = probe_run(vec![first, sse_text_body(RETRY_ANSWER)]);
    assert_partial_retry_reading(&run);
}

/// Partial text, then the connection closes with no terminator: yoagent reads
/// `StreamEnded` as `Network` and retries (risk B, second spelling).
/// Measured Day 218: stdout="PARTIAL_\n\nPONG\n", exit 0, requests=2.
/// (`Network` is in yoagent 0.25.2's `is_retryable`, retry.rs:70, like
/// `RateLimited`; that is why both spellings reach yoagent's retry.)
#[test]
fn yoagent_retry_after_truncated_stream_does_not_duplicate_stdout() {
    let first = sse_partial_then("PARTIAL_", None);
    assert!(first.contains("text_delta") && !first.contains("event: error"));
    let run = probe_run(vec![first, sse_text_body(RETRY_ANSWER)]);
    assert_partial_retry_reading(&run);
}

/// #991, the third row of the task: EVERY attempt dies after text, default
/// (no opt-in). yoagent retries 3 times behind `retry_safe_events` (nothing
/// printed), the final failure's partial reaches the pipe ONCE, and yoyo's own
/// loop refuses to retry text already written (#976). Before #991 the reading
/// was the same stdout after 1 request (yoyo aborted yoagent's first retry).
#[test]
fn yoagent_retry_every_attempt_dying_after_text_prints_the_partial_once() {
    let run = probe_run(vec![rate_limited_after("PARTIAL_")]);
    assert!(run.stub_hit, "anti-vacuous: stub never received a POST");
    assert!(!run.success, "every attempt failed; stderr={}", run.stderr);
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "PARTIAL_\n",
        "#991: the final partial once, the retried copies never; stderr={}",
        run.stderr
    );
    assert_eq!(
        run.requests, 4,
        "1 + RetryConfig::default's 3, then no yoyo retry; stderr={}",
        run.stderr
    );
    assert!(
        run.stderr
            .contains("not retrying: part of the answer was already written"),
        "#976: stderr says why yoyo did not retry; stderr={}",
        run.stderr
    );
}

fn assert_partial_retry_reading(run: &Run) {
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(run.stub_hit, "anti-vacuous: stub never received a POST");
    // #991 (Day 226): `-p` to a pipe reads through yoagent's
    // `retry_safe_events`, so the retried attempt's `PARTIAL_` never reaches
    // the pipe and the retry is let through: one answer, exit 0.
    // Superseded expectation, recorded rather than erased: from Day 219 (#989)
    // to Day 226 this pinned stdout == "PARTIAL_\n", requests == 1 and a
    // nonzero exit, because yoyo aborted yoagent's retry once text was on the
    // pipe. That abort was the right policy while the bytes were already
    // written; behind the filter they never are, so the retry is safe.
    assert_eq!(
        stdout, RETRY_ANSWER,
        "#991: only the successful attempt reaches the pipe; requests={} stderr={}",
        run.requests, run.stderr
    );
    assert_eq!(
        run.requests, 2,
        "#991: yoagent's retry is let through; stderr={}",
        run.stderr
    );
    assert!(
        run.success,
        "#991: the recovered turn exits 0; stderr={}",
        run.stderr
    );
    assert!(
        !run.stderr
            .contains("not retrying: part of the answer was already written"),
        "#991: no skip note, nothing was written; stderr={}",
        run.stderr
    );
}

// #997 on the ProviderRetry door (Day 225, unblocks the yoagent upgrade #1007).
// The #989 tests above are the opt-in-OFF rows (their expectations changed
// Day 226 for #991; the superseded readings are recorded in place). These run the
// SAME fixtures with `retry_after_partial = true` seeded in the cwd. Why these
// statuses: yoagent 0.24.2 `ProviderError::is_retryable` (retry.rs:69) is
// `RateLimited | Network`, and an SSE `error` event of type `rate_limit_error`
// classifies as `RateLimited`, so yoagent ITSELF retries; yoyo's own loop
// never sees the dying attempt. yoagent >= 0.24.3 moves Anthropic 529 /
// `overloaded_error` onto this same classification, which is why this door
// must honour the opt-in before that upgrade.
// That upgrade landed Day 225 (#1007, yoagent 0.25.2); `is_retryable` is
// unchanged there (retry.rs:70) and the 529 reclassification is now live.

fn rate_limited_after(partial: &str) -> String {
    sse_partial_then(
        partial,
        Some((
            "error",
            r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#
                .to_string(),
        )),
    )
}

fn provider_retry_opt_in_run(bodies: Vec<String>) -> Run {
    let run = run_yoyo_seeded(
        &[OPT_IN_CONFIG],
        &["--no-tools", "--max-turns", "1"],
        &["-p", "hi"],
        None,
        bodies,
        Duration::from_secs(150),
    );
    eprintln!(
        "PROBE #997/ProviderRetry: stdout={:?} success={} requests={} stderr_tail={:?}",
        String::from_utf8_lossy(&run.stdout),
        run.success,
        run.requests,
        run.stderr.lines().rev().take(6).collect::<Vec<_>>()
    );
    run
}

/// Opt-in ON, partial then a yoagent-retried `rate_limit_error`, then a clean
/// answer: the retry is let through, exit 0, and the two attempts are kept
/// apart on the pipe by a blank line (never glued into `PARTIAL_PONG`).
#[test]
fn opt_in_provider_retry_after_partial_succeeds_with_attempts_separated() {
    let run = provider_retry_opt_in_run(vec![
        rate_limited_after("PARTIAL_"),
        sse_text_body(RETRY_ANSWER),
    ]);
    assert_eq!(
        run.requests, 2,
        "#997: opted in, yoagent's retry is let through; stderr={}",
        run.stderr
    );
    assert!(
        run.success,
        "#997: retry succeeded, exit 0; stderr={}",
        run.stderr
    );
    // #991 (Day 226): behind `retry_safe_events` the retried attempt never
    // reaches the pipe, so opted in or not, stdout is the answer alone.
    // Superseded expectation, recorded rather than erased: Day 225 pinned
    // "PARTIAL_\n\nPONG\n" here (the dying partial, a blank line, the answer).
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        RETRY_ANSWER,
        "#991: only the successful attempt reaches the pipe; stderr={}",
        run.stderr
    );
    assert!(
        !run.stderr
            .contains("not retrying: part of the answer was already written"),
        "#997: no skip note when the retry happens; stderr={}",
        run.stderr
    );
}

/// Second spelling: partial, then the stream simply ends (`Network`).
#[test]
fn opt_in_provider_retry_after_truncated_stream_succeeds_with_attempts_separated() {
    let run = provider_retry_opt_in_run(vec![
        sse_partial_then("PARTIAL_", None),
        sse_text_body(RETRY_ANSWER),
    ]);
    assert_eq!(run.requests, 2, "stderr={}", run.stderr);
    assert!(run.success, "stderr={}", run.stderr);
    // #991 (Day 226): behind `retry_safe_events` the retried attempt never
    // reaches the pipe, so opted in or not, stdout is the answer alone.
    // Superseded expectation, recorded rather than erased: Day 225 pinned
    // "PARTIAL_\n\nPONG\n" here (the dying partial, a blank line, the answer).
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        RETRY_ANSWER,
        "stderr={}",
        run.stderr
    );
}

/// Opt-in ON, death BEFORE any text: stdout is exactly the answer, with no
/// stray separator for an attempt that never wrote anything.
#[test]
fn opt_in_provider_retry_before_text_yields_exactly_the_answer() {
    let run = provider_retry_opt_in_run(vec![http_429_response(), sse_text_body(RETRY_ANSWER)]);
    assert_eq!(run.requests, 2, "stderr={}", run.stderr);
    assert!(run.success, "stderr={}", run.stderr);
    assert_eq!(String::from_utf8_lossy(&run.stdout), RETRY_ANSWER);
}

/// Opt-in ON, every attempt dies after text: the run must still end nonzero,
/// bounded (yoagent's own cap, then yoyo's). Readings pinned as observed
/// Day 225: 24 requests = 4 per yoagent call (1 + RetryConfig::default's 3)
/// times 6 yoyo attempts (MAX_RETRIES + 1), ~70s of backoff, exit nonzero.
/// The worst-case cost of the opt-in on this door: one partial per request,
/// each kept apart by a blank line. Without the opt-in this is 1 request.
#[test]
fn opt_in_provider_retry_every_attempt_dying_still_exits_nonzero() {
    let run = provider_retry_opt_in_run(vec![rate_limited_after("PARTIAL_")]);
    assert!(!run.success, "every attempt failed; stderr={}", run.stderr);
    assert_eq!(
        run.requests, 24,
        "4 yoagent x 6 yoyo; stderr={}",
        run.stderr
    );
    // #991 (Day 226): yoagent's three retries per yoyo attempt are held and
    // dropped; only each yoyo attempt's FINAL failure keeps its partial, so
    // one copy per yoyo attempt (6), not per request (24, Day 225's reading,
    // superseded rather than erased).
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        ["PARTIAL_\n"; 6].join("\n"),
        "stderr={}",
        run.stderr
    );
    assert!(
        run.stderr.contains("failed after 6 attempts"),
        "yoyo's own cap reported; stderr={}",
        run.stderr
    );
}

// #989 stream-json door, probed Day 219 (Task 2). Hypothesis tested: "the
// final record concatenates PARTIAL_ with PONG, exit 0". FALSE on the record:
// messageEnd/turnEnd/agentEnd carry exactly "PONG\n". The dead attempt is
// marked IN BAND: its messageEnd has stopReason "error" + "will be retried",
// then a providerRetry event. Unlike plain text, an NDJSON consumer can tell
// the attempts apart, so the abort policy is deliberately NOT applied here
// (it would turn every recoverable stream-json turn into a failure).

fn stream_json_probe_run(bodies: Vec<String>) -> Run {
    run_yoyo_within(
        &["--no-tools", "--max-turns", "1"],
        &["--output-format", "stream-json", "-p", "hi"],
        None,
        bodies,
        Duration::from_secs(120),
    )
}

/// Text of the last assistant message in the final `agentEnd` record.
fn final_assistant_text(lines: &[serde_json::Value]) -> String {
    let end = lines
        .iter()
        .rfind(|v| v["type"] == "agentEnd")
        .expect("agentEnd");
    let msg = end["messages"]
        .as_array()
        .unwrap()
        .iter()
        .rfind(|m| m["role"] == "assistant")
        .expect("assistant message");
    msg["content"][0]["text"].as_str().unwrap().to_string()
}

#[test]
fn stream_json_retry_after_partial_marks_dead_attempt_and_final_record_is_clean() {
    let first = sse_partial_then(
        "PARTIAL_",
        Some((
            "error",
            r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#
                .to_string(),
        )),
    );
    let run = stream_json_probe_run(vec![first, sse_text_body(RETRY_ANSWER)]);
    assert_eq!(
        run.requests, 2,
        "anti-vacuous: yoagent must retry; stderr={}",
        run.stderr
    );
    assert!(
        run.success,
        "recovered stream-json turn exits 0; stderr={}",
        run.stderr
    );
    let lines = ndjson_lines(&run);
    assert_eq!(final_assistant_text(&lines), RETRY_ANSWER);
    // The dead attempt's text did reach the wire...
    let partial = lines
        .iter()
        .position(|v| v["type"] == "messageUpdate" && v["delta"]["delta"] == "PARTIAL_")
        .expect("anti-vacuous: PARTIAL_ streamed");
    // ...and is closed in band before the retry: error messageEnd, then providerRetry.
    assert_eq!(lines[partial + 1]["type"], "messageEnd");
    assert_eq!(lines[partial + 1]["message"]["stopReason"], "error");
    assert_eq!(lines[partial + 2]["type"], "providerRetry");
}

#[test]
fn stream_json_retry_after_429_before_text_yields_clean_answer() {
    let run = stream_json_probe_run(vec![http_429_response(), sse_text_body(RETRY_ANSWER)]);
    assert_eq!(
        run.requests, 2,
        "anti-vacuous: yoagent must retry; stderr={}",
        run.stderr
    );
    assert!(run.success, "stderr={}", run.stderr);
    let lines = ndjson_lines(&run);
    assert_eq!(final_assistant_text(&lines), RETRY_ANSWER);
    assert!(lines.iter().any(|v| v["type"] == "providerRetry"));
}

// ---------------------------------------------------------------------------
// #993: SIGINT during a `-p` stream.
// ---------------------------------------------------------------------------

/// A stub that streams `partial` as one text delta and then STALLS: it never
/// sends `content_block_stop`/`message_stop` and holds the connection open, so
/// the turn is still in flight when the test sends SIGINT. The third value
/// counts responses whose partial text has been written and flushed.
#[cfg(unix)]
fn start_stall_stub(partial: &str) -> (u16, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let text = serde_json::to_string(partial).unwrap();
    let body = sse_events(vec![
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_stall","type":"message","role":"assistant","model":"claude-stub","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":5,"output_tokens":1}}}"#.to_string(),
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
    ]);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let hit = Arc::new(AtomicUsize::new(0));
    let hit_thread = Arc::clone(&hit);
    let sent = Arc::new(AtomicUsize::new(0));
    let sent_thread = Arc::clone(&sent);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let body = body.clone();
            let hit = Arc::clone(&hit_thread);
            let sent = Arc::clone(&sent_thread);
            thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() {
                    return;
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
                let mut req_body = vec![0u8; content_length];
                let _ = reader.read_exact(&mut req_body);
                if request_line.starts_with("POST") {
                    hit.fetch_add(1, Ordering::SeqCst);
                }
                let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n";
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body.as_bytes());
                let _ = stream.flush();
                sent.fetch_add(1, Ordering::SeqCst);
                // Stall: hold the stream open, never finish the message.
                thread::sleep(Duration::from_secs(120));
            });
        }
    });
    (port, hit, sent)
}

#[cfg(unix)]
struct SigintRun {
    stdout: Vec<u8>,
    stderr: String,
    code: Option<i32>,
    stub_hit: bool,
}

/// Run `yoyo <extra>` against the stall stub. Wait until the stub has flushed
/// the partial text, then a grace period for yoyo to read it, then send SIGINT
/// and collect.
///
/// Superseded harness, recorded rather than erased (Day 226, #991): this used
/// to wait (no fixed sleep) until `wait_for` appeared on the child's STDOUT.
/// A `-p` run whose stdout is a pipe now reads through yoagent's
/// `retry_safe_events`, which holds an attempt's text until the attempt ends,
/// so a stalled attempt puts nothing on the pipe before the signal: the text
/// reaches stdout only because Ctrl+C flushes what the gate holds. Waiting on
/// stdout would wait forever. The stub-side flag plus `SIGINT_GRACE` is the
/// readiness signal left (localhost bytes, parsed in microseconds).
#[cfg(unix)]
fn run_yoyo_sigint(extra: &[&str], partial: &str) -> SigintRun {
    run_yoyo_signal(extra, partial, "-INT")
}

/// `run_yoyo_sigint` with the signal named (`kill`'s spelling: `-INT`, `-TERM`).
#[cfg(unix)]
fn run_yoyo_signal(extra: &[&str], partial: &str, signal: &str) -> SigintRun {
    use std::sync::Mutex;
    const SIGINT_GRACE: Duration = Duration::from_millis(1500);
    let (port, hit, sent) = start_stall_stub(partial);
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args([
            "--provider",
            "anthropic",
            "--model",
            "claude-sonnet-4-5",
            "--base-url",
            &base_url,
            "--no-tools",
            "--max-turns",
            "1",
        ])
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
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    let out = Arc::new(Mutex::new(Vec::<u8>::new()));
    let err = Arc::new(Mutex::new(Vec::<u8>::new()));
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let (o2, e2) = (Arc::clone(&out), Arc::clone(&err));
    let t_out = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = so.read(&mut buf) {
            if n == 0 {
                break;
            }
            o2.lock().unwrap().extend_from_slice(&buf[..n]);
        }
    });
    let t_err = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = se.read(&mut buf) {
            if n == 0 {
                break;
            }
            e2.lock().unwrap().extend_from_slice(&buf[..n]);
        }
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if sent.load(Ordering::SeqCst) > 0 {
            thread::sleep(SIGINT_GRACE);
            break;
        }
        if Instant::now() > deadline || child.try_wait().unwrap().is_some() {
            let _ = child.kill();
            panic!(
                "the stub never sent the partial text before SIGINT; stdout={:?} stderr={}",
                String::from_utf8_lossy(&out.lock().unwrap()),
                String::from_utf8_lossy(&err.lock().unwrap())
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
    let status = Command::new("kill")
        .args([signal, &child.id().to_string()])
        .status()
        .expect("run kill");
    assert!(status.success(), "kill {signal} failed");
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("yoyo did not exit after kill {signal}");
        }
        thread::sleep(Duration::from_millis(20));
    };
    t_out.join().unwrap();
    t_err.join().unwrap();
    let stdout = out.lock().unwrap().clone();
    let stderr = String::from_utf8_lossy(&err.lock().unwrap()).into_owned();
    SigintRun {
        stdout,
        stderr,
        code: status.code(),
        stub_hit: hit.load(Ordering::SeqCst) > 0,
    }
}

/// #993: `-p` cut short by SIGINT keeps the streamed text on stdout and
/// nothing else, writes the interrupt note to stderr, and exits 130.
/// Since #991 (Day 226) the text is held by the retry-safe gate until the
/// interrupt, so this also pins that Ctrl+C flushes the held text exactly once.
#[cfg(unix)]
#[test]
fn sigint_during_print_prompt_exits_130_with_note_on_stderr() {
    let run = run_yoyo_sigint(&["-p", "hi"], PARTIAL_LINE);
    assert!(run.stub_hit, "anti-vacuous: stub never received a POST");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    eprintln!(
        "PROBE #993: stdout={stdout:?} code={:?} stderr_tail={:?}",
        run.code,
        run.stderr.lines().rev().take(3).collect::<Vec<_>>()
    );
    assert_eq!(
        stdout, PARTIAL_LINE,
        "#993: stdout must be exactly the streamed text, no REPL hint; stderr={}",
        run.stderr
    );
    assert!(
        run.stderr.contains("(interrupted)"),
        "#993: the interrupt note belongs on stderr; stderr={}",
        run.stderr
    );
    assert!(
        !run.stderr.contains("again"),
        "#993: a single-shot run has exited, so no 'press Ctrl+C again'; stderr={}",
        run.stderr
    );
    assert_eq!(
        run.code,
        Some(130),
        "#993: SIGINT exits 130; stderr={}",
        run.stderr
    );
}

/// #993 near-miss: the same partial text, streamed to completion without a
/// signal, is byte-identical on stdout and exits 0.
#[test]
fn uninterrupted_print_prompt_is_unchanged_and_exits_zero() {
    let run = run_yoyo_with(
        &["--no-tools", "--max-turns", "1"],
        &["-p", "hi"],
        None,
        vec![sse_text_body(PARTIAL_LINE)],
    );
    assert_reached_stub(&run);
    assert_eq!(String::from_utf8_lossy(&run.stdout), PARTIAL_LINE);
    assert!(
        run.success,
        "uninterrupted -p must exit 0; stderr={}",
        run.stderr
    );
    assert!(
        !run.stderr.contains("(interrupted"),
        "stderr={}",
        run.stderr
    );
}

// ---------------------------------------------------------------------------
// #1015: SIGTERM during a piped `-p` stream (what `timeout` sends).
// ---------------------------------------------------------------------------

/// #1015: on a pipe the retry-safe gate (#991) holds the open attempt's text
/// until its `MessageEnd`. Before the fix a SIGTERM killed the process with
/// the default disposition and that held text never reached stdout (0 bytes);
/// SIGINT already flushed it. Now SIGTERM flushes exactly what SIGINT does and
/// exits 143 (128 + 15), so a `timeout` wrapper can still tell it apart.
#[cfg(unix)]
#[test]
fn sigterm_during_piped_print_prompt_flushes_held_text_and_exits_143() {
    let run = run_yoyo_signal(&["-p", "hi"], PARTIAL_LINE, "-TERM");
    assert!(run.stub_hit, "anti-vacuous: stub never received a POST");
    eprintln!(
        "PROBE #1015: stdout={:?} code={:?} stderr_tail={:?}",
        String::from_utf8_lossy(&run.stdout),
        run.code,
        run.stderr.lines().rev().take(3).collect::<Vec<_>>()
    );
    assert_eq!(
        run.stdout,
        PARTIAL_LINE.as_bytes(),
        "#1015: the text streamed before SIGTERM must be on stdout, exactly once; stderr={}",
        run.stderr
    );
    assert!(
        run.stderr.contains("(interrupted)"),
        "#1015: the interrupt note belongs on stderr; stderr={}",
        run.stderr
    );
    assert_eq!(
        run.code,
        Some(143),
        "#1015: SIGTERM exits 143 (SIGINT stays 130); stderr={}",
        run.stderr
    );
}

/// #1016: SIGHUP (a dropped ssh session, a closed terminal under
/// `yoyo -p ... | tee log`) hits the same held text SIGTERM did. Before the
/// fix SIGHUP kept its default disposition (die), so the partial was lost.
/// Now it flushes exactly what SIGTERM does and exits 129 (128 + 1).
#[cfg(unix)]
#[test]
fn sighup_during_piped_print_prompt_flushes_held_text_and_exits_129() {
    let run = run_yoyo_signal(&["-p", "hi"], PARTIAL_LINE, "-HUP");
    assert!(run.stub_hit, "anti-vacuous: stub never received a POST");
    eprintln!(
        "PROBE #1016: stdout={:?} code={:?} stderr_tail={:?}",
        String::from_utf8_lossy(&run.stdout),
        run.code,
        run.stderr.lines().rev().take(3).collect::<Vec<_>>()
    );
    assert_eq!(
        run.stdout,
        PARTIAL_LINE.as_bytes(),
        "#1016: the text streamed before SIGHUP must be on stdout, exactly once; stderr={}",
        run.stderr
    );
    assert!(
        run.stderr.contains("(interrupted)"),
        "#1016: the interrupt note belongs on stderr; stderr={}",
        run.stderr
    );
    assert_eq!(
        run.code,
        Some(129),
        "#1016: SIGHUP exits 129 (SIGTERM stays 143, SIGINT 130); stderr={}",
        run.stderr
    );
}

/// #1016: SIGQUIT (Ctrl+\ from a terminal while stdout is piped, as in
/// `yoyo -p ... | tee log`). Measured before the fix: 0 bytes on stdout,
/// death by signal. Now it rides the SIGHUP/SIGTERM watcher and exits 131
/// (128 + 3). Trade: no core dump from SIGQUIT while the handler is armed.
#[cfg(unix)]
#[test]
fn sigquit_during_piped_print_prompt_flushes_held_text_and_exits_131() {
    let run = run_yoyo_signal(&["-p", "hi"], PARTIAL_LINE, "-QUIT");
    assert!(run.stub_hit, "anti-vacuous: stub never received a POST");
    eprintln!(
        "PROBE #1016 SIGQUIT: stdout={:?} code={:?} stderr_tail={:?}",
        String::from_utf8_lossy(&run.stdout),
        run.code,
        run.stderr.lines().rev().take(3).collect::<Vec<_>>()
    );
    assert_eq!(
        run.stdout,
        PARTIAL_LINE.as_bytes(),
        "#1016: the text streamed before SIGQUIT must be on stdout, exactly once; stderr={}",
        run.stderr
    );
    assert!(
        run.stderr.contains("(interrupted)"),
        "#1016: the interrupt note belongs on stderr; stderr={}",
        run.stderr
    );
    assert_eq!(
        run.code,
        Some(131),
        "#1016: SIGQUIT exits 131 (SIGHUP 129, SIGTERM 143, SIGINT 130); stderr={}",
        run.stderr
    );
}

/// A tool turn whose only call is `bash` running `sleep 30`.
#[cfg(unix)]
fn sse_bash_sleep_body() -> String {
    let partial = serde_json::to_string(r#"{"command": "sleep 30"}"#).unwrap();
    sse_events(vec![
        (
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_sleep","type":"message","role":"assistant","model":"claude-stub","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":5,"output_tokens":1}}}"#.to_string(),
        ),
        (
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_sleep_1","name":"bash","input":{}}}"#.to_string(),
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

/// #1015 hazard: once a SIGTERM handler is registered the default "die"
/// disposition is gone for the rest of the process, so a SIGTERM that lands
/// while a long tool call runs must still END the process (not set a flag
/// nothing reads, which would leave `timeout` to escalate to SIGKILL).
#[cfg(unix)]
#[test]
fn sigterm_during_a_long_tool_call_on_a_pipe_still_terminates() {
    use std::sync::Mutex;
    let (port, hit) = start_stub_seq(vec![sse_bash_sleep_body(), sse_body()]);
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args([
            "--provider",
            "anthropic",
            "--model",
            "claude-sonnet-4-5",
            "--base-url",
            &base_url,
            "--max-turns",
            "4",
            "--yes",
            "-p",
            "sleep",
        ])
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
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    let err = Arc::new(Mutex::new(Vec::<u8>::new()));
    let e2 = Arc::clone(&err);
    let mut se = child.stderr.take().unwrap();
    let mut so = child.stdout.take().unwrap();
    // Readers are detached: an orphaned `sleep` may hold the pipes open.
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = so.read(&mut buf) {
            if n == 0 {
                break;
            }
        }
    });
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = se.read(&mut buf) {
            if n == 0 {
                break;
            }
            e2.lock().unwrap().extend_from_slice(&buf[..n]);
        }
    });
    // The bash progress line is printed only when the tool COMPLETES, so the
    // readiness signal is the stub having served the tool turn, plus a grace
    // for yoyo to parse it and spawn `sleep 30`.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if hit.load(Ordering::SeqCst) > 0 {
            thread::sleep(Duration::from_millis(2000));
            break;
        }
        if Instant::now() > deadline || child.try_wait().unwrap().is_some() {
            let _ = child.kill();
            panic!(
                "the bash tool never started; stderr={}",
                String::from_utf8_lossy(&err.lock().unwrap())
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        hit.load(Ordering::SeqCst),
        1,
        "anti-vacuous: the tool turn was served and `sleep 30` has not finished"
    );
    let ok = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("run kill")
        .success();
    assert!(ok, "kill -TERM failed");
    // Well inside `sleep 30`: the process must not wait out the tool.
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!(
                "#1015: SIGTERM during a tool call was swallowed; stderr={}",
                String::from_utf8_lossy(&err.lock().unwrap())
            );
        }
        thread::sleep(Duration::from_millis(20));
    };
    thread::sleep(Duration::from_millis(200));
    let stderr = String::from_utf8_lossy(&err.lock().unwrap()).into_owned();
    assert_eq!(
        status.code(),
        Some(143),
        "#1015: SIGTERM mid-tool exits 143; stderr={stderr}"
    );
}

/// A reply cut off at the output-token limit (Anthropic `stop_reason:
/// "max_tokens"`, yoagent `StopReason::Length`) used to end silently: exit 0,
/// nothing on stderr. Now stderr carries one warning naming the cause and the
/// fix; stdout is unchanged and, under the `--max-tokens 30` the user set
/// here, so is the exit code (0, #1026: the exit code follows who set the
/// limit; the default-limit and refusal rows are further down). The twin
/// below is the same stream with `end_turn`, and the two stdouts must be
/// byte-identical.
fn run_with_stop_reason(print: bool, stop_reason: &str) -> Run {
    run_with_stop_reason_and(print, stop_reason, &["--max-tokens", "30"])
}

fn run_with_stop_reason_and(print: bool, stop_reason: &str, limit: &[&str]) -> Run {
    let body = sse_turn("msg_len", vec![text_block(&["1 2 3 ", "4 5"])], stop_reason);
    let mut extra = limit.to_vec();
    extra.extend(["-p", "numbers 1 to 200"]);
    if print {
        extra.push("--print");
    }
    run_yoyo_with(
        &["--no-tools", "--max-turns", "1"],
        &extra,
        None,
        vec![body],
    )
}

const OUTPUT_LIMIT_NOTE: &str = "reply stopped at the output limit (max_tokens = 30)";

#[test]
fn reply_cut_off_at_output_limit_warns_on_stderr_and_keeps_stdout() {
    for print in [false, true] {
        assert_cut_off_reply_warns(print);
    }
}

fn assert_cut_off_reply_warns(print: bool) {
    let cut = run_with_stop_reason(print, "max_tokens");
    assert_reached_stub(&cut);
    let normal = run_with_stop_reason(print, "end_turn");
    assert_reached_stub(&normal);
    let stdout = String::from_utf8(cut.stdout.clone()).expect("utf-8 stdout");
    assert!(
        stdout.contains("1 2 3 4 5"),
        "anti-vacuous: the streamed text must reach stdout; stdout={stdout:?}"
    );
    assert_eq!(
        cut.stdout, normal.stdout,
        "stdout must be exactly the streamed bytes, cut or not; stderr={}",
        cut.stderr
    );
    assert!(
        cut.stderr.contains(OUTPUT_LIMIT_NOTE),
        "a cut-off reply must say so on stderr; stderr={}",
        cut.stderr
    );
    assert!(
        cut.stderr
            .contains("raise --max-tokens or max_tokens in .yoyo.toml"),
        "the note must name the fix; stderr={}",
        cut.stderr
    );
    assert_eq!(occurrences(&cut.stderr, OUTPUT_LIMIT_NOTE), 1);
    // Near-miss: a normal ending prints no such line.
    assert!(
        !normal.stderr.contains("output limit"),
        "a normal end_turn must not warn; stderr={}",
        normal.stderr
    );
}

/// #1026, @yuanhao's decision (2026-11-03): "the exit code follows who set the
/// limit". This drives the real binary, so it checks the hop the compiler
/// can't: that the FINAL stop reason reaches `-p`'s exit code. Each row keeps
/// the partial text on stdout, whatever the code.
#[test]
fn exit_code_follows_who_set_the_output_limit_and_a_refusal_fails() {
    // (stop_reason, limit flags, expected exit code, what the row means)
    let rows: [(&str, &[&str], i32, &str); 4] = [
        (
            "max_tokens",
            &[],
            1,
            "cut by a default limit the user never set",
        ),
        (
            "max_tokens",
            &["--max-tokens", "30"],
            0,
            "cut by the user's own max_tokens",
        ),
        ("refusal", &[], 1, "a refusal produced no answer"),
        ("end_turn", &[], 0, "near-miss: a normal ending"),
    ];
    for print in [false, true] {
        for (stop_reason, limit, want, why) in rows {
            let run = run_with_stop_reason_and(print, stop_reason, limit);
            // Not `assert_reached_stub`: it also demands exit 0.
            assert!(run.stub_hit, "anti-vacuous: no POST; stderr={}", run.stderr);
            let stdout = String::from_utf8(run.stdout.clone()).expect("utf-8 stdout");
            assert!(
                stdout.contains("1 2 3 4 5"),
                "{why}: the partial text must still reach stdout (print={print}); \
                 stdout={stdout:?} stderr={}",
                run.stderr
            );
            assert_eq!(
                run.code,
                Some(want),
                "{why}: stop_reason={stop_reason} limit={limit:?} print={print}; stderr={}",
                run.stderr
            );
        }
    }
    // The user-set row still carries the existing note, naming their limit.
    let user_set = run_with_stop_reason_and(true, "max_tokens", &["--max-tokens", "30"]);
    assert!(
        user_set.stderr.contains(OUTPUT_LIMIT_NOTE),
        "stderr={}",
        user_set.stderr
    );
    // The default row's note names the default, not a number the user chose.
    let default = run_with_stop_reason_and(true, "max_tokens", &[]);
    assert!(
        default
            .stderr
            .contains("reply stopped at the output limit (the model's default max_tokens)"),
        "stderr={}",
        default.stderr
    );
}

/// The same decision on the other two exit doors: piped stdin (the door every
/// `scripts/evolve.sh` agent uses) and `--output-format stream-json`, which has
/// its own event loop and its own exit line in main.rs.
#[test]
fn exit_code_follows_who_set_the_limit_on_the_piped_and_stream_json_doors() {
    let rows: [(&str, &[&str], i32); 4] = [
        ("max_tokens", &[], 1),
        ("max_tokens", &["--max-tokens", "30"], 0),
        ("refusal", &[], 1),
        ("end_turn", &[], 0),
    ];
    for (stop_reason, limit, want) in rows {
        let body = || sse_turn("msg_len", vec![text_block(&["1 2 3 ", "4 5"])], stop_reason);
        let piped = run_yoyo_with(
            &["--no-tools", "--max-turns", "1"],
            limit,
            Some("numbers 1 to 200"),
            vec![body()],
        );
        let mut sj_extra = limit.to_vec();
        sj_extra.extend(["--output-format", "stream-json", "-p", "numbers 1 to 200"]);
        let stream_json = run_yoyo_with(
            &["--no-tools", "--max-turns", "1"],
            &sj_extra,
            None,
            vec![body()],
        );
        for (door, run) in [("piped", piped), ("stream-json", stream_json)] {
            assert!(
                run.stub_hit,
                "{door}: anti-vacuous: no POST; stderr={}",
                run.stderr
            );
            let stdout = String::from_utf8_lossy(&run.stdout);
            assert!(
                stdout.contains("1 2 3") && stdout.contains("4 5"),
                "{door}: the partial text must still reach stdout; stdout={stdout:?}"
            );
            assert_eq!(
                run.code,
                Some(want),
                "{door}: stop_reason={stop_reason} limit={limit:?}; stderr={}",
                run.stderr
            );
        }
    }
}

/// #1026 item 1, @yuanhao's decision: "Add the note to json and stream-json
/// output too". The json result object carries `incomplete` (null, or
/// `{"reason": "output_limit", "limit_set_by": ...}` / `{"reason": "refusal"}`),
/// and `is_error` agrees with the exit code for the same outcome. Both the
/// prompt door and the piped door build the object, so both are driven.
#[test]
fn json_result_object_says_when_the_reply_was_cut_or_refused() {
    // (stop_reason, limit flags, want incomplete, want is_error, want exit)
    let rows: [(&str, &[&str], serde_json::Value, bool, i32); 4] = [
        (
            "max_tokens",
            &[],
            serde_json::json!({"reason": "output_limit", "limit_set_by": "default"}),
            true,
            1,
        ),
        (
            "max_tokens",
            &["--max-tokens", "30"],
            serde_json::json!({"reason": "output_limit", "limit_set_by": "user"}),
            false,
            0,
        ),
        (
            "refusal",
            &[],
            serde_json::json!({"reason": "refusal"}),
            true,
            1,
        ),
        // Near-miss: a normal ending must not claim a cut.
        ("end_turn", &[], serde_json::Value::Null, false, 0),
    ];
    for (stop_reason, limit, want_incomplete, want_is_error, want_code) in rows {
        let body = || sse_turn("msg_len", vec![text_block(&["1 2 3 ", "4 5"])], stop_reason);
        let mut prompt_extra = limit.to_vec();
        prompt_extra.extend(["--output-format", "json", "-p", "numbers 1 to 200"]);
        let prompt = run_yoyo_with(
            &["--no-tools", "--max-turns", "1"],
            &prompt_extra,
            None,
            vec![body()],
        );
        let mut piped_extra = limit.to_vec();
        piped_extra.extend(["--output-format", "json"]);
        let piped = run_yoyo_with(
            &["--no-tools", "--max-turns", "1"],
            &piped_extra,
            Some("numbers 1 to 200"),
            vec![body()],
        );
        for (door, run) in [("prompt", prompt), ("piped", piped)] {
            assert!(
                run.stub_hit,
                "{door}: anti-vacuous: no POST; stderr={}",
                run.stderr
            );
            let v: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
                panic!(
                    "{door}: json stdout must be one value: {e}; stdout={:?}",
                    String::from_utf8_lossy(&run.stdout)
                )
            });
            assert_eq!(
                v["incomplete"], want_incomplete,
                "{door}: stop_reason={stop_reason} limit={limit:?}; object={v}"
            );
            assert_eq!(
                v["is_error"],
                serde_json::Value::Bool(want_is_error),
                "{door}: is_error must agree with the exit code; stop_reason={stop_reason} \
                 limit={limit:?}; object={v}"
            );
            assert_eq!(
                run.code,
                Some(want_code),
                "{door}: stop_reason={stop_reason} limit={limit:?}; stderr={}",
                run.stderr
            );
            assert!(
                v["response"].as_str().unwrap_or("").contains("1 2 3 4 5"),
                "{door}: the partial answer must still be in the object; object={v}"
            );
        }
    }
}

/// #1026 item 1, stream-json half: the stderr note the text path prints for a
/// cut or refused reply now prints here too, through the same wording, and
/// stdout stays pure JSON lines.
#[test]
fn stream_json_prints_the_cut_and_refusal_notes_on_stderr_only() {
    // (stop_reason, limit flags, stderr must contain, stderr must not contain)
    let rows: [(&str, &[&str], Option<&str>); 4] = [
        (
            "max_tokens",
            &[],
            Some("reply stopped at the output limit (the model's default max_tokens)"),
        ),
        (
            "max_tokens",
            &["--max-tokens", "30"],
            Some(OUTPUT_LIMIT_NOTE),
        ),
        ("refusal", &[], Some("refused this request")),
        ("end_turn", &[], None),
    ];
    for (stop_reason, limit, want_note) in rows {
        let body = sse_turn("msg_len", vec![text_block(&["1 2 3 ", "4 5"])], stop_reason);
        let mut extra = limit.to_vec();
        extra.extend(["--output-format", "stream-json", "-p", "numbers 1 to 200"]);
        let run = run_yoyo_with(
            &["--no-tools", "--max-turns", "1"],
            &extra,
            None,
            vec![body],
        );
        assert!(run.stub_hit, "anti-vacuous: no POST; stderr={}", run.stderr);
        let stdout = String::from_utf8(run.stdout.clone()).expect("utf-8 stdout");
        let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(
            !lines.is_empty(),
            "anti-vacuous: stream-json printed nothing"
        );
        for line in &lines {
            assert!(
                serde_json::from_str::<serde_json::Value>(line).is_ok(),
                "stop_reason={stop_reason}: every stdout line must be JSON; line={line:?}"
            );
            assert!(
                !line.contains("reply stopped at the output limit"),
                "the note goes to stderr only; line={line:?}"
            );
        }
        match want_note {
            Some(note) => assert_eq!(
                occurrences(&run.stderr, note),
                1,
                "stop_reason={stop_reason} limit={limit:?}: stderr must carry the note once; \
                 stderr={}",
                run.stderr
            ),
            None => assert!(
                !run.stderr.contains("output limit")
                    && !run.stderr.contains("refused this request"),
                "near-miss: a normal end must not warn; stderr={}",
                run.stderr
            ),
        }
    }
}
