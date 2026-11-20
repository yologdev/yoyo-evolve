//! #1218: a failed `-p` run never replaces or creates the `--output` file.
//!
//! #1214 decided that `-o` replaces an existing file because the file is the
//! run's own product. A failed run (exit 1: API error, a turn that died, a
//! refusal, a cut at an output limit nobody set) has no product, and before
//! this fix it still wrote its (usually empty) text over the user's last good
//! answer. The shape follows `curl -o`: a refused connection leaves an
//! existing file untouched and creates none. Real binary, scrubbed env, temp
//! dirs; failures come from a closed loopback port or a local stub, never a
//! real vendor.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const OLD: &str = "keep me";
const NEW: &str = "NEW ANSWER";

/// One Anthropic Messages SSE stream whose only text is `NEW`, ending with `stop`.
fn sse_body(stop: &str) -> String {
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
                r#"{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{NEW}"}}}}"#
            ),
        ),
        (
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#.to_string(),
        ),
        (
            "message_delta",
            format!(
                r#"{{"type":"message_delta","delta":{{"stop_reason":"{stop}","stop_sequence":null}},"usage":{{"output_tokens":7}}}}"#
            ),
        ),
        ("message_stop", r#"{"type":"message_stop"}"#.to_string()),
    ];
    let mut out = String::new();
    for (name, data) in events {
        out.push_str(&format!("event: {name}\ndata: {data}\n\n"));
    }
    out
}

/// A stub on 127.0.0.1:0 answering every POST with `body` (an SSE stream, or
/// a full HTTP response when it starts with a status line). Returns its port.
fn start_stub(body: String) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
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
            let mut req_body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut req_body);
            let resp = if body.starts_with("HTTP/1.1 ") {
                body.clone()
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
            };
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    port
}

/// A loopback port with nothing listening: connects are refused.
fn closed_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    l.local_addr().unwrap().port()
}

struct Run {
    code: Option<i32>,
    stderr: String,
}

/// Run `yoyo -p "say ok" -o out.txt` in `cwd` against `port`.
fn run_yoyo(cwd: &Path, port: u16) -> Run {
    let home = cwd.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args(["--provider", "anthropic", "--model", "claude-sonnet-4-5"])
        .args(["--base-url", &base_url, "--no-tools", "--max-turns", "1"])
        .args(["-p", "say ok", "--output", "out.txt"])
        .current_dir(cwd)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("ANTHROPIC_API_KEY", "dummy-test-key")
        .env_remove("API_KEY")
        .env_remove("MODEL")
        .env_remove("YOYO_MODEL")
        .env_remove("YOYO_PROVIDER")
        .env_remove("YOYO_RESTRICTED")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    let deadline = Instant::now() + Duration::from_secs(120);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    Run {
        code: out.status.code(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Row (a): the file existed and the run failed (connection refused).
#[test]
fn a_failed_run_leaves_an_existing_output_file_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    std::fs::write(&out, OLD).unwrap();
    let run = run_yoyo(tmp.path(), closed_port());
    let err = &run.stderr;
    assert_eq!(run.code, Some(1), "stderr:\n{err}");
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        OLD,
        "a failed run replaced the existing --output file\n{err}"
    );
    assert!(
        err.contains(
            "out.txt was not written: the run failed, so the existing file was left as it was"
        ),
        "stderr does not say the file was kept:\n{err}"
    );
    assert!(!err.contains("replaced out.txt"), "{err}");
}

/// Row (b): no file existed and the run failed (an HTTP 400 from the stub).
#[test]
fn a_failed_run_creates_no_output_file() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    let body =
        r#"{"type":"error","error":{"type":"invalid_request_error","message":"stub says no"}}"#;
    let resp = format!(
        "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let run = run_yoyo(tmp.path(), start_stub(resp));
    let err = &run.stderr;
    assert_eq!(run.code, Some(1), "stderr:\n{err}");
    assert!(
        !out.exists(),
        "a failed run created the --output file\n{err}"
    );
    assert!(
        err.contains("out.txt was not written: the run failed, so no file was created"),
        "stderr does not say no file was created:\n{err}"
    );
    assert!(!err.contains("wrote response to out.txt"), "{err}");
}

/// Row (d): a refusal is exit 1 (`incomplete_forces_failure`), so even with
/// text in hand the run is a failure and the file is kept.
#[test]
fn a_refused_run_leaves_an_existing_output_file_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    std::fs::write(&out, OLD).unwrap();
    let run = run_yoyo(tmp.path(), start_stub(sse_body("refusal")));
    let err = &run.stderr;
    assert_eq!(run.code, Some(1), "stderr:\n{err}");
    assert_eq!(std::fs::read_to_string(&out).unwrap(), OLD, "{err}");
    assert!(
        err.contains(
            "out.txt was not written: the run failed, so the existing file was left as it was"
        ),
        "{err}"
    );
}

/// Row (c), the positive companion: a successful run still replaces (#1214).
#[test]
fn a_successful_run_still_replaces_the_output_file() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    std::fs::write(&out, OLD).unwrap();
    let run = run_yoyo(tmp.path(), start_stub(sse_body("end_turn")));
    let err = &run.stderr;
    assert_eq!(run.code, Some(0), "stderr:\n{err}");
    assert_eq!(std::fs::read_to_string(&out).unwrap(), NEW, "{err}");
    assert!(err.contains("replaced out.txt"), "{err}");
    assert!(!err.contains("was not written"), "{err}");
}
