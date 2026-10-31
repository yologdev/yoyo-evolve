//! #1109 at the real binary: an unknown top-level flag refuses with exit 2
//! before anything is sent. `yoyo --modle claude-haiku-4-5 -p hi` used to warn,
//! ignore the flag, and send a paid request on the default model.
//!
//! Each run points `--base-url` at a local stub that counts requests, so
//! "nothing was sent" is measured, not inferred from wording. The spawn
//! scrubs every `*_API_KEY`/`API_KEY` from the inherited environment (the
//! evolve job exports them and CI does not; Day 233) and sets a fake key.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// A stub that counts each request and answers it with a 400.
fn start_counting_stub() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&hits);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            count.fetch_add(1, Ordering::SeqCst);
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
            let payload =
                r#"{"type":"error","error":{"type":"invalid_request_error","message":"stub"}}"#;
            let resp = format!(
                "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (port, hits)
}

fn scrubbed(home: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    for (name, _) in std::env::vars() {
        if name == "API_KEY" || name.ends_with("_API_KEY") {
            cmd.env_remove(name);
        }
    }
    cmd.env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("ANTHROPIC_API_KEY", "x")
        .env_remove("MODEL")
        .env_remove("YOYO_AUDIT")
        .env_remove("YOYO_RESTRICTED")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null());
    cmd
}

/// Run `yoyo --base-url <stub> <args>` in a scratch HOME/project. Returns
/// (exit code, stdout, stderr, requests the stub saw). A run that reaches the
/// stub is killed once it has.
fn run(args: &[&str]) -> (Option<i32>, String, String, usize) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    let (port, hits) = start_counting_stub();
    let base_url = format!("http://127.0.0.1:{port}");
    let mut child = scrubbed(&home)
        .args(["--base-url", &base_url])
        .args(args)
        .current_dir(&proj)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if hits.load(Ordering::SeqCst) > 0 || start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().expect("wait");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        hits.load(Ordering::SeqCst),
    )
}

#[test]
fn misspelled_model_flag_refuses_with_exit_2_and_sends_nothing() {
    let (code, _, err, hits) = run(&["--modle", "claude-haiku-4-5", "-p", "hi"]);
    assert_eq!(code, Some(2), "stderr: {err}");
    assert!(err.contains("'--modle'"), "{err}");
    assert!(err.contains("did you mean --model?"), "{err}");
    assert!(err.contains("Nothing was sent"), "{err}");
    assert_eq!(hits, 0, "a refused run must send nothing");
}

/// `yoyo log --bogus`: "log" is not a subcommand, so this used to send "log"
/// as a prompt and exit 0.
#[test]
fn unknown_flag_after_a_non_subcommand_word_refuses() {
    let (code, _, err, hits) = run(&["log", "--bogus"]);
    assert_eq!(code, Some(2), "stderr: {err}");
    assert!(err.contains("'--bogus'"), "{err}");
    assert_eq!(hits, 0, "a refused run must send nothing");
}

/// Near-miss: known flags still get past startup. `--print-system-prompt`
/// exits 0 without a request.
#[test]
fn known_flags_still_run_as_before() {
    let (code, out, err, hits) = run(&["--model", "x", "--yes", "--print-system-prompt"]);
    assert_eq!(code, Some(0), "stderr: {err}");
    assert!(!out.is_empty(), "the system prompt should print");
    assert!(!err.contains("unknown flag"), "{err}");
    assert_eq!(hits, 0);
}

/// Near-miss: the same prompt run with the flag spelled right reaches the stub,
/// so the refusal is the only change.
#[test]
fn correctly_spelled_model_flag_sends_the_request() {
    let (_, _, err, hits) = run(&["--model", "claude-haiku-4-5", "-p", "hi"]);
    assert!(hits >= 1, "a known flag must reach the stub; stderr: {err}");
    assert!(!err.contains("unknown flag"), "{err}");
}

/// Near-miss: after `--` the old warn-and-continue path is unchanged.
#[test]
fn flag_after_double_dash_keeps_the_old_warning() {
    let (code, _, err, _) = run(&["--print-system-prompt", "--", "--modle"]);
    assert_eq!(code, Some(0), "stderr: {err}");
    assert!(
        err.contains("warning: Unknown flag '--modle'"),
        "the old warning path should still run: {err}"
    );
}

/// Near-miss: a subcommand parses its own flags and never reaches the refusal.
#[test]
fn subcommand_flags_are_untouched() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .env("HOME", &home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .expect("git")
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    std::fs::write(repo.join("f"), "a\n").unwrap();
    git(&["add", "f"]);
    git(&[
        "-c",
        "user.email=a@b",
        "-c",
        "user.name=a",
        "commit",
        "-qm",
        "i",
    ]);
    std::fs::write(repo.join("f"), "a\nb\n").unwrap();
    let out = scrubbed(&home)
        .args(["diff", "--stat"])
        .current_dir(&repo)
        .output()
        .expect("spawn yoyo");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr: {err}");
    assert!(!err.contains("unknown flag"), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 file changed"));
}
