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
    stdout: String,
    stderr: String,
}

/// Run `yoyo -p "say ok" -o out.txt` in `cwd` against `port`.
fn run_yoyo(cwd: &Path, port: u16) -> Run {
    run_yoyo_with(cwd, port, &[])
}

/// [`run_yoyo`] plus `extra` flags (#1219: `--output-format stream-json`).
fn run_yoyo_with(cwd: &Path, port: u16, extra: &[&str]) -> Run {
    run_yoyo_door(cwd, port, extra, false)
}

/// The run, through the `-p` door or (`piped`) the stdin door, which has
/// its own stream-json branch in main.rs.
fn run_yoyo_door(cwd: &Path, port: u16, extra: &[&str], piped: bool) -> Run {
    let prompt_args: &[&str] = if piped { &[] } else { &["-p", "say ok"] };
    let home = cwd.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args(["--provider", "anthropic", "--model", "claude-sonnet-4-5"])
        .args(["--base-url", &base_url, "--no-tools", "--max-turns", "1"])
        .args(prompt_args)
        .args(["--output", "out.txt"])
        .args(extra)
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
        .stdin(if piped { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(b"say ok").unwrap();
    }
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
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
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

// ---------------------------------------------------------------------------
// #1220: an interrupted run (Ctrl+C, SIGTERM from `timeout`) is not a
// complete answer either. Before the fix it replaced `out.txt` with whatever
// had streamed so far (measured on #1220: a 0-byte file, exit 130).
// ---------------------------------------------------------------------------

/// A stub that streams `message_start` plus one text delta (`NEW`) and then
/// stalls with the socket open. The flag turns true once that text has been
/// written and flushed, so the test signals a run that is really mid-turn.
#[cfg(unix)]
fn start_stall_stub() -> (u16, std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let full = sse_body("end_turn");
    // Everything up to and including the one text delta; no stop events.
    let cut = full
        .find("event: content_block_stop")
        .expect("delta prefix");
    let partial = full[..cut].to_string();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let sent = Arc::new(AtomicBool::new(false));
    let sent_thread = Arc::clone(&sent);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let partial = partial.clone();
            let sent = Arc::clone(&sent_thread);
            thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
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
                let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n";
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(partial.as_bytes());
                let _ = stream.flush();
                sent.store(true, Ordering::SeqCst);
                thread::sleep(Duration::from_secs(120));
            });
        }
    });
    (port, sent)
}

/// Run `yoyo -p "say ok" -o out.txt` in `cwd` against the stall stub, and
/// send `signal` (kill's spelling) once the partial text has been sent.
#[cfg(unix)]
fn run_yoyo_interrupted(cwd: &Path, signal: &str) -> Run {
    run_yoyo_interrupted_with(cwd, signal, &[])
}

/// [`run_yoyo_interrupted`] plus `extra` flags.
fn run_yoyo_interrupted_with(cwd: &Path, signal: &str, extra: &[&str]) -> Run {
    use std::sync::atomic::Ordering;
    let (port, sent) = start_stall_stub();
    let home = cwd.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args(["--provider", "anthropic", "--model", "claude-sonnet-4-5"])
        .args(["--base-url", &base_url, "--no-tools", "--max-turns", "1"])
        .args(["-p", "say ok", "--output", "out.txt"])
        .args(extra)
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
    let deadline = Instant::now() + Duration::from_secs(60);
    while !sent.load(Ordering::SeqCst) {
        if Instant::now() > deadline || child.try_wait().unwrap().is_some() {
            let _ = child.kill();
            let out = child.wait_with_output().unwrap();
            panic!(
                "the stub never sent the partial text; stderr={}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
    // The bytes are flushed; give yoyo a moment to parse them (localhost).
    thread::sleep(Duration::from_millis(1500));
    let status = Command::new("kill")
        .args([signal, &child.id().to_string()])
        .status()
        .expect("run kill");
    assert!(status.success(), "kill {signal} failed");
    let deadline = Instant::now() + Duration::from_secs(30);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().unwrap();
    Run {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// #1220 row (e): the file existed and a SIGTERM (what `timeout` sends) cut
/// the run mid-reply. Exit 143, the file is byte-identical, stderr says so.
#[cfg(unix)]
#[test]
fn an_interrupted_run_leaves_an_existing_output_file_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    std::fs::write(&out, OLD).unwrap();
    let run = run_yoyo_interrupted(tmp.path(), "-TERM");
    let err = &run.stderr;
    assert_eq!(run.code, Some(143), "stderr:\n{err}");
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        OLD,
        "an interrupted run replaced the existing --output file\n{err}"
    );
    assert!(
        err.contains(
            "out.txt was not written: the run was interrupted, so the existing file was left as it was"
        ),
        "stderr does not say the file was kept:\n{err}"
    );
    assert!(!err.contains("replaced out.txt"), "{err}");
}

/// #1220 row (f): no file existed and a Ctrl+C (SIGINT) cut the run. Exit
/// 130 and no file is created.
#[cfg(unix)]
#[test]
fn an_interrupted_run_creates_no_output_file() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    let run = run_yoyo_interrupted(tmp.path(), "-INT");
    let err = &run.stderr;
    assert_eq!(run.code, Some(130), "stderr:\n{err}");
    assert!(
        !out.exists(),
        "an interrupted run created the --output file\n{err}"
    );
    assert!(
        err.contains("out.txt was not written: the run was interrupted, so no file was created"),
        "stderr does not say no file was created:\n{err}"
    );
    assert!(!err.contains("wrote response to out.txt"), "{err}");
}

// ---------------------------------------------------------------------------
// #1219: `--output-format stream-json` returned before the only writer of the
// `--output` file, so `-o` wrote nothing and said nothing, exit 0. The stream
// still goes to stdout (pure NDJSON); the final answer goes to the file under
// the same rules as every other mode.
// ---------------------------------------------------------------------------

const STREAM_JSON: &[&str] = &["--output-format", "stream-json"];

/// Every stdout line parses as one JSON value: the write notes stay on stderr.
fn assert_stdout_is_ndjson(run: &Run) {
    let lines: Vec<&str> = run.stdout.lines().filter(|l| !l.is_empty()).collect();
    assert!(!lines.is_empty(), "no NDJSON on stdout\n{}", run.stderr);
    for line in lines {
        assert!(
            serde_json::from_str::<serde_json::Value>(line).is_ok(),
            "stdout line is not JSON: {line:?}\nstdout:\n{}",
            run.stdout
        );
    }
}

/// #1219 row (g): a successful stream-json run creates the file with the answer.
#[test]
fn stream_json_success_writes_the_output_file() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    let run = run_yoyo_with(tmp.path(), start_stub(sse_body("end_turn")), STREAM_JSON);
    let err = &run.stderr;
    assert_eq!(run.code, Some(0), "stderr:\n{err}");
    assert_eq!(
        std::fs::read_to_string(&out).ok().as_deref(),
        Some(NEW),
        "stream-json did not write the --output file\n{err}"
    );
    assert!(err.contains("wrote response to out.txt"), "{err}");
    assert_stdout_is_ndjson(&run);
}

/// #1219 row (h): an existing file is replaced, and stderr says "replaced".
#[test]
fn stream_json_success_replaces_the_output_file() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    std::fs::write(&out, OLD).unwrap();
    let run = run_yoyo_with(tmp.path(), start_stub(sse_body("end_turn")), STREAM_JSON);
    let err = &run.stderr;
    assert_eq!(run.code, Some(0), "stderr:\n{err}");
    assert_eq!(std::fs::read_to_string(&out).unwrap(), NEW, "{err}");
    assert!(err.contains("replaced out.txt"), "{err}");
    assert_stdout_is_ndjson(&run);
}

/// #1219 row (i): a refused stream-json run keeps the existing file, exit 1.
#[test]
fn stream_json_refusal_leaves_an_existing_output_file_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    std::fs::write(&out, OLD).unwrap();
    let run = run_yoyo_with(tmp.path(), start_stub(sse_body("refusal")), STREAM_JSON);
    let err = &run.stderr;
    assert_eq!(run.code, Some(1), "stderr:\n{err}");
    assert_eq!(std::fs::read_to_string(&out).unwrap(), OLD, "{err}");
    assert!(
        err.contains(
            "out.txt was not written: the run failed, so the existing file was left as it was"
        ),
        "{err}"
    );
    assert_stdout_is_ndjson(&run);
}

/// #1219 row (j): an HTTP 400 stream-json run creates no file, exit 1.
#[test]
fn stream_json_failed_run_creates_no_output_file() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    let body =
        r#"{"type":"error","error":{"type":"invalid_request_error","message":"stub says no"}}"#;
    let resp = format!(
        "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let run = run_yoyo_with(tmp.path(), start_stub(resp), STREAM_JSON);
    let err = &run.stderr;
    assert_ne!(run.code, Some(0), "stderr:\n{err}");
    assert!(
        !out.exists(),
        "a failed run created the --output file\n{err}"
    );
    assert!(
        err.contains("out.txt was not written: the run failed, so no file was created"),
        "{err}"
    );
}

/// #1219 row (k): Ctrl+C mid-reply on stream-json keeps the existing file
/// (the partial answer is not a final answer) and exits 130, as `-p` does.
#[cfg(unix)]
#[test]
fn stream_json_interrupted_run_leaves_an_existing_output_file_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    std::fs::write(&out, OLD).unwrap();
    let run = run_yoyo_interrupted_with(tmp.path(), "-INT", STREAM_JSON);
    let err = &run.stderr;
    assert_eq!(run.code, Some(130), "stderr:\n{err}");
    assert_eq!(std::fs::read_to_string(&out).unwrap(), OLD, "{err}");
    assert!(
        err.contains(
            "out.txt was not written: the run was interrupted, so the existing file was left as it was"
        ),
        "{err}"
    );
}

/// #1219 row (l): the piped (stdin) door's stream-json branch writes too.
#[test]
fn stream_json_piped_success_writes_the_output_file() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    let port = start_stub(sse_body("end_turn"));
    let run = run_yoyo_door(tmp.path(), port, STREAM_JSON, true);
    let err = &run.stderr;
    assert_eq!(run.code, Some(0), "stderr:\n{err}");
    assert_eq!(
        std::fs::read_to_string(&out).ok().as_deref(),
        Some(NEW),
        "piped stream-json did not write the --output file\n{err}"
    );
    assert!(err.contains("wrote response to out.txt"), "{err}");
    assert_stdout_is_ndjson(&run);
}

// ---------------------------------------------------------------------------
// #1221: `--output-format json -p` cut by a terminating signal. Before the
// fix (measured Day 267 against HEAD 54f9d2b6) a SIGTERM killed the process
// with the default disposition: exit 143 and an EMPTY stdout, so a script
// under `timeout` had nothing to parse and no note saying why.
// ---------------------------------------------------------------------------

const JSON: &[&str] = &["--output-format", "json"];

/// The one json envelope an interrupted run must still print: exit `code`,
/// stdout parses as a single object with `is_error: true` and the partial
/// reply in `response`, the `-o` file left as it was, and a stderr note.
#[cfg(unix)]
fn assert_interrupted_json_envelope(signal: &str, code: i32) {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    std::fs::write(&out, OLD).unwrap();
    let run = run_yoyo_interrupted_with(tmp.path(), signal, JSON);
    let err = &run.stderr;
    assert_eq!(
        run.code,
        Some(code),
        "stdout:\n{}\nstderr:\n{err}",
        run.stdout
    );
    let v: serde_json::Value = serde_json::from_str(run.stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "stdout is not one JSON object ({e}): {:?}\nstderr:\n{err}",
            run.stdout
        )
    });
    assert_eq!(v["is_error"], serde_json::json!(true), "{v}");
    assert_eq!(v["response"], serde_json::json!(NEW), "{v}");
    assert!(err.contains("(interrupted)"), "no interrupt note:\n{err}");
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        OLD,
        "an interrupted json run replaced the existing --output file\n{err}"
    );
}

/// #1221 row: SIGTERM (what `timeout` sends) -> 143 plus the envelope.
#[cfg(unix)]
#[test]
fn json_sigterm_prints_an_is_error_envelope_with_the_partial_reply() {
    assert_interrupted_json_envelope("-TERM", 143);
}

/// #1221 row: SIGHUP (terminal closed) -> 129 plus the envelope.
#[cfg(unix)]
#[test]
fn json_sighup_prints_an_is_error_envelope_with_the_partial_reply() {
    assert_interrupted_json_envelope("-HUP", 129);
}

// ---------------------------------------------------------------------------
// #1221 (stream-json row): before the fix (measured Day 267 12:19 against HEAD
// 5c35f81c) a SIGTERM/SIGHUP killed a stream-json run outright: the NDJSON
// stream stopped at the last delta, nothing on stderr, nothing said that the
// `-o` file was not written.
// ---------------------------------------------------------------------------

/// The end an interrupted stream-json run must still have: exit `code`, every
/// stdout line a whole JSON value, the LAST line yoagent's own `agentEnd`
/// whose final assistant message says `stopReason: "aborted"`, the `-o` file
/// left as it was, and stderr saying both that the run was interrupted and
/// that the file was not written.
#[cfg(unix)]
fn assert_interrupted_stream_json(signal: &str, code: i32) {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.txt");
    std::fs::write(&out, OLD).unwrap();
    let run = run_yoyo_interrupted_with(tmp.path(), signal, STREAM_JSON);
    let err = &run.stderr;
    assert_eq!(
        run.code,
        Some(code),
        "stdout:\n{}\nstderr:\n{err}",
        run.stdout
    );
    assert_stdout_is_ndjson(&run);
    let last = run.stdout.lines().rfind(|l| !l.is_empty()).unwrap();
    let v: serde_json::Value = serde_json::from_str(last).unwrap();
    assert_eq!(
        v["type"],
        serde_json::json!("agentEnd"),
        "last line: {last}"
    );
    let final_assistant = v["messages"]
        .as_array()
        .and_then(|m| m.iter().rev().find(|m| m["role"] == "assistant"))
        .unwrap_or_else(|| panic!("agentEnd has no assistant message: {last}"));
    assert_eq!(
        final_assistant["stopReason"],
        serde_json::json!("aborted"),
        "last line: {last}"
    );
    assert!(err.contains("(interrupted)"), "no interrupt note:\n{err}");
    assert!(
        err.contains(
            "out.txt was not written: the run was interrupted, so the existing file was left as it was"
        ),
        "stderr does not say the file was kept:\n{err}"
    );
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        OLD,
        "an interrupted stream-json run replaced the existing --output file\n{err}"
    );
}

/// #1221 row: SIGTERM (what `timeout` sends) -> 143, the stream still ends.
#[cfg(unix)]
#[test]
fn stream_json_sigterm_ends_the_stream_with_an_aborted_agent_end() {
    assert_interrupted_stream_json("-TERM", 143);
}

/// #1221 row: SIGHUP (terminal closed) -> 129, the stream still ends.
#[cfg(unix)]
#[test]
fn stream_json_sighup_ends_the_stream_with_an_aborted_agent_end() {
    assert_interrupted_stream_json("-HUP", 129);
}

/// #1221 row: Ctrl+C (SIGINT) -> 130, same ending and the same stderr note.
#[cfg(unix)]
#[test]
fn stream_json_sigint_ends_the_stream_with_an_aborted_agent_end() {
    assert_interrupted_stream_json("-INT", 130);
}
