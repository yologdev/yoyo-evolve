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
        assert_eq!(
            run.requests, 1,
            "[{door}] #976: no retry after streamed text"
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
#[test]
fn yoagent_retry_after_truncated_stream_does_not_duplicate_stdout() {
    let first = sse_partial_then("PARTIAL_", None);
    assert!(first.contains("text_delta") && !first.contains("event: error"));
    let run = probe_run(vec![first, sse_text_body(RETRY_ANSWER)]);
    assert_partial_retry_reading(&run);
}

fn assert_partial_retry_reading(run: &Run) {
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(run.stub_hit, "anti-vacuous: stub never received a POST");
    // The #976 policy, applied to yoagent's retry: a pipe that already holds
    // the attempt's text gets ONE copy, no second request, and a nonzero exit.
    assert_eq!(
        stdout, "PARTIAL_\n",
        "#989: exactly one copy of the partial; requests={} stderr={}",
        run.requests, run.stderr
    );
    assert_eq!(run.requests, 1, "#989: no retry after streamed text");
    assert!(!run.success, "#989: the dead turn must exit nonzero");
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
/// the turn is still in flight when the test sends SIGINT.
#[cfg(unix)]
fn start_stall_stub(partial: &str) -> (u16, Arc<AtomicUsize>) {
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
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let body = body.clone();
            let hit = Arc::clone(&hit_thread);
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
                // Stall: hold the stream open, never finish the message.
                thread::sleep(Duration::from_secs(120));
            });
        }
    });
    (port, hit)
}

#[cfg(unix)]
struct SigintRun {
    stdout: Vec<u8>,
    stderr: String,
    code: Option<i32>,
    stub_hit: bool,
}

/// Run `yoyo <extra>` against the stall stub. Wait (no fixed sleep) until
/// `wait_for` appears on the child's stdout, then send SIGINT and collect.
#[cfg(unix)]
fn run_yoyo_sigint(extra: &[&str], partial: &str, wait_for: &str) -> SigintRun {
    use std::sync::Mutex;
    let (port, hit) = start_stall_stub(partial);
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
        if String::from_utf8_lossy(&out.lock().unwrap()).contains(wait_for) {
            break;
        }
        if Instant::now() > deadline || child.try_wait().unwrap().is_some() {
            let _ = child.kill();
            panic!(
                "streamed text never reached stdout before SIGINT; stdout={:?} stderr={}",
                String::from_utf8_lossy(&out.lock().unwrap()),
                String::from_utf8_lossy(&err.lock().unwrap())
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
    let status = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("run kill");
    assert!(status.success(), "kill -INT failed");
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("yoyo did not exit after SIGINT");
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
#[cfg(unix)]
#[test]
fn sigint_during_print_prompt_exits_130_with_note_on_stderr() {
    let run = run_yoyo_sigint(&["-p", "hi"], PARTIAL_LINE, PARTIAL_LINE);
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
