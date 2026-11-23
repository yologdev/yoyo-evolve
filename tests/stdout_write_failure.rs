//! #1234: when stdout cannot be written (full disk, I/O error), `yoyo -p`
//! must say so on stderr and exit 1, in every `--output-format`, instead of
//! exiting 0 with the answer lost (text, stream-json) or panicking with 101
//! (json). `/dev/full` makes every write fail with ENOSPC, which is the
//! full-disk case a script running `yoyo -p ... > out.txt` meets.
//!
//! Near-miss rows: a normal run to a pipe prints no such error, and a pipe
//! whose reader has gone away keeps today's quiet SIGPIPE death (main.rs
//! restores default SIGPIPE on purpose, so `yoyo -p ... | head -1` behaves
//! like `cat`). Only non-EPIPE failures are loud.
//!
//! Hermetic: a local 127.0.0.1 stub plays the Anthropic API, HOME and XDG
//! dirs are temp dirs, ambient keys and model overrides are removed.
#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const ANSWER: &str = "the answer is ok";
const ERROR_NEEDLE: &str = "error writing to stdout";

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

/// A stub that answers every POST with the same one-text-block stream.
fn start_stub() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_thread = Arc::clone(&hits);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
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
                hits_thread.fetch_add(1, Ordering::SeqCst);
            }
            let payload = sse_body();
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (port, hits)
}

/// Where the child's stdout goes.
enum Out {
    /// `/dev/full`: every write fails with ENOSPC.
    DevFull,
    /// A pipe this test reads to the end.
    Pipe,
    /// A pipe whose read end this test closes right after spawn, before
    /// yoyo can have written anything (it first has to reach the stub).
    ClosedPipe,
}

struct Run {
    stdout: String,
    stderr: String,
    code: Option<i32>,
    signal: Option<i32>,
    hits: usize,
}

fn run_yoyo(extra: &[&str], out: Out) -> Run {
    let (port, hits) = start_stub();
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let stdout = match out {
        Out::DevFull => Stdio::from(
            std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/full")
                .expect("open /dev/full"),
        ),
        Out::Pipe | Out::ClosedPipe => Stdio::piped(),
    };
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    cmd.args([
        "--provider",
        "anthropic",
        "--model",
        "claude-sonnet-5-5",
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
    .env("ANTHROPIC_API_KEY", "dummy-test-key-1234")
    .env_remove("API_KEY")
    .env_remove("MODEL")
    .env_remove("YOYO_MODEL")
    .env_remove("YOYO_PROVIDER")
    .env("NO_COLOR", "1")
    .stdout(stdout)
    .stderr(Stdio::piped())
    .stdin(Stdio::null());
    let mut child: Child = cmd.spawn().expect("spawn yoyo");
    if matches!(out, Out::ClosedPipe) {
        drop(child.stdout.take());
    }
    // Drain stdout/stderr on threads so a full pipe can't wedge the child.
    let stdout_reader = child.stdout.take().map(|mut s| {
        thread::spawn(move || {
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            buf
        })
    });
    let mut stderr_pipe = child.stderr.take().unwrap();
    let stderr_reader = thread::spawn(move || {
        let mut buf = String::new();
        let _ = stderr_pipe.read_to_string(&mut buf);
        buf
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("yoyo did not exit within 30s (args {extra:?})");
        }
        thread::sleep(Duration::from_millis(50));
    };
    Run {
        stdout: stdout_reader.map(|h| h.join().unwrap()).unwrap_or_default(),
        stderr: stderr_reader.join().unwrap(),
        code: status.code(),
        signal: std::os::unix::process::ExitStatusExt::signal(&status),
        hits: hits.load(Ordering::SeqCst),
    }
}

fn assert_full_disk_is_reported(extra: &[&str]) {
    let run = run_yoyo(extra, Out::DevFull);
    assert!(
        run.hits > 0,
        "anti-vacuous: the stub never got a POST, so the run never had an answer to write; \
         args={extra:?} stderr={}",
        run.stderr
    );
    assert_eq!(
        run.code,
        Some(1),
        "#1234: a stdout write that fails with ENOSPC must exit 1 (args {extra:?}); stderr={}",
        run.stderr
    );
    assert!(
        run.stderr.contains(ERROR_NEEDLE) && run.stderr.contains("No space left on device"),
        "#1234: stderr must name the failed stdout write and the OS error (args {extra:?}); \
         stderr={}",
        run.stderr
    );
    assert!(
        !run.stderr.contains("panicked"),
        "#1234: a failed stdout write must not panic (args {extra:?}); stderr={}",
        run.stderr
    );
    assert_eq!(
        run.stderr.matches(ERROR_NEEDLE).count(),
        1,
        "one error line, not one per failed write (args {extra:?}); stderr={}",
        run.stderr
    );
}

#[test]
fn text_mode_on_a_full_disk_reports_the_write_error_and_exits_1() {
    assert_full_disk_is_reported(&["-p", "hi"]);
}

#[test]
fn print_mode_on_a_full_disk_reports_the_write_error_and_exits_1() {
    assert_full_disk_is_reported(&["--print", "-p", "hi"]);
}

#[test]
fn json_mode_on_a_full_disk_reports_the_write_error_and_exits_1() {
    assert_full_disk_is_reported(&["--output-format", "json", "-p", "hi"]);
}

#[test]
fn stream_json_mode_on_a_full_disk_reports_the_write_error_and_exits_1() {
    assert_full_disk_is_reported(&["--output-format", "stream-json", "-p", "hi"]);
}

/// Near-miss: a healthy pipe gets the answer, exit 0, and no write error.
#[test]
fn a_healthy_pipe_gets_the_answer_and_no_write_error() {
    for extra in [
        &["-p", "hi"][..],
        &["--output-format", "json", "-p", "hi"][..],
        &["--output-format", "stream-json", "-p", "hi"][..],
    ] {
        let run = run_yoyo(extra, Out::Pipe);
        assert!(run.hits > 0, "anti-vacuous: stub never hit; args={extra:?}");
        assert_eq!(run.code, Some(0), "args={extra:?} stderr={}", run.stderr);
        assert!(
            run.stdout.contains("the answer is ok"),
            "args={extra:?} stdout={:?}",
            run.stdout
        );
        assert!(
            !run.stderr.contains(ERROR_NEEDLE),
            "args={extra:?} stderr={}",
            run.stderr
        );
    }
}

/// Near-miss: a closed pipe (`| head -c1` whose reader has left) keeps the
/// quiet SIGPIPE death; EPIPE is never reported as a write error.
#[test]
fn a_closed_pipe_stays_quiet() {
    for extra in [
        &["-p", "hi"][..],
        &["--output-format", "json", "-p", "hi"][..],
        &["--output-format", "stream-json", "-p", "hi"][..],
    ] {
        let run = run_yoyo(extra, Out::ClosedPipe);
        // Anti-vacuous: the run must have tried to write. stream-json writes
        // its first event before the request, so it may die before the stub.
        assert_eq!(
            run.signal,
            Some(13),
            "a write to a closed pipe dies by SIGPIPE, quietly (args {extra:?}); stderr={}",
            run.stderr
        );
        assert!(
            !run.stderr.contains(ERROR_NEEDLE) && !run.stderr.contains("panicked"),
            "#1234: EPIPE must stay quiet (args {extra:?}); stderr={}",
            run.stderr
        );
    }
}
