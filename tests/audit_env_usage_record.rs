//! #1071 near-miss: the fix that keeps test binaries out of the cwd's real
//! `.yoyo/audit.jsonl` lives under `#[cfg(test)]` in `prompt_budget::audit_base`,
//! so it must not change what a real `yoyo` run does. This spawns the real
//! binary with `YOYO_AUDIT=1` (the env door, not `--audit`) in a temp project
//! and asserts its usage record lands in that project's `.yoyo/audit.jsonl`.
//!
//! The provider is a local stub that answers every POST with a 400, which is
//! not retried, so the run ends in about a second with `is_error: true` and
//! still records usage (`emit_output` records before branching on anything).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Answer every request with a non-retryable Anthropic 400.
fn start_400_stub() -> u16 {
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
            let payload = r#"{"type":"error","error":{"type":"invalid_request_error","message":"stub says no"}}"#;
            let resp = format!(
                "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    port
}

#[test]
fn yoyo_audit_env_still_records_a_real_runs_usage_in_its_project() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let port = start_400_stub();
    let base_url = format!("http://127.0.0.1:{port}");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args([
            "--provider",
            "anthropic",
            "--model",
            "claude-sonnet-4-5",
            "--base-url",
            &base_url,
            "-p",
            "hi",
        ])
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
        .env_remove("YOYO_RESTRICTED")
        .env("YOYO_AUDIT", "1")
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
    let stderr = String::from_utf8_lossy(&out.stderr);
    let audit = std::fs::read_to_string(proj.join(".yoyo/audit.jsonl")).unwrap_or_default();
    let ctx = format!(
        "exit={:?}\naudit:\n{audit}\nstderr:\n{stderr}",
        out.status.code()
    );
    // Anti-vacuous: the run really reached the stub and failed there.
    assert!(
        stderr.contains("stub says no"),
        "the stub was never reached\n{ctx}"
    );
    let usage: Vec<serde_json::Value> = audit
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["type"] == "usage")
        .collect();
    assert_eq!(usage.len(), 1, "one usage record per run\n{ctx}");
    assert_eq!(usage[0]["model"], "claude-sonnet-4-5", "{ctx}");
    assert_eq!(usage[0]["is_error"], true, "{ctx}");
}
