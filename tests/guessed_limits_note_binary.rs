//! #1021 (Day 241) at the real emission point: the anthropic arm of
//! `build_agent` prints the guessed-limits note for an id with no preset, and
//! stays silent for a preset id (the no-config default, `claude-opus-5-5`).
//!
//! `--print-system-prompt` never builds the agent, so it cannot tell the two
//! apart (measured: silent for both). This runs a real `-p` turn against a
//! local stub that answers one text reply, so the process ends on its own.
//! The environment is scrubbed of ambient keys the way the sibling stub tests
//! do it (d233): the evolve job exports provider keys and CI does not.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn text_reply() -> String {
    [
        ("message_start", r#"{"type":"message_start","message":{"id":"msg_s","type":"message","role":"assistant","content":[],"model":"m","stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1}}}"#),
        ("content_block_start", r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#),
        ("content_block_delta", r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ok"}}"#),
        ("content_block_stop", r#"{"type":"content_block_stop","index":0}"#),
        ("message_delta", r#"{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":1}}"#),
        ("message_stop", r#"{"type":"message_stop"}"#),
    ]
    .iter()
    .map(|(e, d)| format!("event: {e}\ndata: {d}\n\n"))
    .collect()
}

/// A stub on 127.0.0.1:0 that answers every request with one text reply.
fn start_stub() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let Ok(clone) = stream.try_clone() else {
                continue;
            };
            let mut reader = BufReader::new(clone);
            let mut content_length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let t = line.trim_end();
                if t.is_empty() {
                    break;
                }
                if let Some((k, v)) = t.split_once(':') {
                    if k.eq_ignore_ascii_case("content-length") {
                        content_length = v.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
            let payload = text_reply();
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    port
}

/// (stdout, stderr) of `yoyo --provider anthropic --model <model> -p hi` against the stub.
fn run_for(model: &str) -> (String, String) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    let base_url = format!("http://127.0.0.1:{}/v1", start_stub());
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args(["--provider", "anthropic", "--model", model])
        .args(["--base-url", &base_url, "--max-turns", "1", "-p", "hi"])
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
        .env_remove("MODEL")
        .env_remove("YOYO_AUDIT")
        .env_remove("YOYO_RESTRICTED")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const NOTE_STEM: &str = "so yoyo is using guessed limits";

#[test]
fn preset_anthropic_id_gets_no_guessed_limits_note() {
    let (out, err) = run_for("claude-opus-5-5");
    // Anti-vacuous: the agent was built and ran (the stub's reply came back).
    assert!(
        out.contains("ok"),
        "run never reached the stub\n{out}\n{err}"
    );
    assert!(!err.contains(NOTE_STEM), "preset id was warned\n{err}");
}

#[test]
fn anthropic_id_without_preset_gets_the_note_once() {
    let (out, err) = run_for("claude-made-up-9");
    assert!(
        out.contains("ok"),
        "run never reached the stub\n{out}\n{err}"
    );
    // Not quiet- or NO_COLOR-gated: the marker follows is_plain_output().
    let line = "⚠ no preset is known for anthropic model 'claude-made-up-9', so yoyo is \
using guessed limits: a 200000-token context window and max_tokens = 16000. Every reply \
(reasoning included) is capped at 16000 output tokens. If the model allows more, set max_tokens \
and context_window in .yoyo.toml (or pass --max-tokens / --context-window).";
    assert_eq!(
        err.lines().filter(|l| l.contains(NOTE_STEM)).count(),
        1,
        "{err}"
    );
    assert!(err.lines().any(|l| l == line), "{err}");
}
