//! Day 247: a refused connection to a loopback server stops in seconds and
//! names the URL, instead of ~70s of "retrying (attempt N/6)". Real binary,
//! real closed port. The env is scrubbed the way the sibling spawning tests do
//! it (d233): the evolve job exports provider keys and CI does not.

use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn refused_loopback_fails_fast_and_names_the_url() {
    // Bind then drop: the port is now closed, so connects are refused.
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").expect("bind");
        l.local_addr().unwrap().port()
    };
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args(["--provider", "custom", "--base-url", &base_url])
        .args(["--model", "x", "--max-turns", "1", "-p", "hi"])
        .current_dir(&proj)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("OPENAI_API_KEY", "dummy-test-key")
        .env_remove("API_KEY")
        .env_remove("ANTHROPIC_API_KEY")
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
    // The old path took ~70s; cap the wait well past the new bound so a
    // regression reports as a slow run, not a hung test.
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    let elapsed = start.elapsed();
    let err = String::from_utf8_lossy(&out.stderr);

    assert_eq!(out.status.code(), Some(1), "stderr:\n{err}");
    assert!(
        err.contains(&format!("connection refused at http://127.0.0.1:{port}/")),
        "URL not named:\n{err}"
    );
    assert!(!err.contains("/retry"), "REPL wording in -p mode:\n{err}");
    assert!(
        !err.contains("retrying (attempt"),
        "yoyo's own backoff still ran:\n{err}"
    );
    // yoagent's own 3 retries (1s/2s/4s with jitter) are the ~7s floor.
    assert!(elapsed < Duration::from_secs(20), "took {elapsed:?}\n{err}");
}
