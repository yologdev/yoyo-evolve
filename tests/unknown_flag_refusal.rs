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
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Request bodies the stub has read, in arrival order (#1110: the prompt).
type Bodies = Arc<Mutex<Vec<String>>>;

/// A stub that counts each request, keeps its body, and answers with a 400.
/// The count goes up only once the body is read, so a caller that stops at
/// the first hit has the body in hand.
fn start_counting_stub() -> (u16, Arc<AtomicUsize>, Bodies) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&hits);
    let bodies: Bodies = Arc::default();
    let kept = Arc::clone(&bodies);
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
            kept.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&body).into_owned());
            count.fetch_add(1, Ordering::SeqCst);
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
    (port, hits, bodies)
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
    let (code, out, err, hits, _) = run_with_bodies(args);
    (code, out, err, hits)
}

/// [`run`], plus the request bodies the stub read.
fn run_with_bodies(args: &[&str]) -> (Option<i32>, String, String, usize, Vec<String>) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    let (port, hits, bodies) = start_counting_stub();
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
    let seen = bodies.lock().unwrap().clone();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        hits.load(Ordering::SeqCst),
        seen,
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

/// #1110 (this test pinned the old warning until Day 246): after `--` a
/// flag-shaped word is an operand, so neither `--` nor `--modle` is reported.
#[test]
fn flag_after_double_dash_is_an_operand_not_a_warning() {
    let (code, _, err, _) = run(&["--print-system-prompt", "--", "--modle"]);
    assert_eq!(code, Some(0), "stderr: {err}");
    assert!(!err.contains("Unknown flag"), "{err}");
    assert!(!err.contains("unknown flag"), "{err}");
}

/// #1110 at the request: `yoyo -- hi` warns about nothing and sends "hi";
/// `yoyo -p -- --explain this` sends the dash-led words verbatim. Anti-vacuous:
/// each run must reach the stub, so the body checks are about a real request.
#[test]
fn words_after_double_dash_reach_the_prompt_verbatim() {
    let (_, _, err, hits, bodies) = run_with_bodies(&["--", "hi"]);
    assert!(hits >= 1, "must reach the stub; stderr: {err}");
    assert!(!err.contains("Unknown flag"), "{err}");
    assert!(bodies[0].contains(r#""text":"hi""#), "{}", bodies[0]);

    let (_, _, err, hits, bodies) = run_with_bodies(&["-p", "--", "--explain", "this"]);
    assert!(hits >= 1, "must reach the stub; stderr: {err}");
    assert!(!err.contains("unknown flag"), "{err}");
    assert!(
        bodies[0].contains(r#""text":"--explain this""#),
        "{}",
        bodies[0]
    );
}

/// #1112 at the request: `yoyo -- --no-color --quiet hi` sends all three
/// words; the early scans no longer also apply them as flags (that half is
/// pinned in cli_config's has_option table, since a piped stdout already
/// turns colour off and quiet on, so a pipe cannot observe it).
#[test]
fn early_flags_after_double_dash_reach_the_prompt_verbatim() {
    let (_, _, err, hits, bodies) = run_with_bodies(&["--", "--no-color", "--quiet", "hi"]);
    assert!(hits >= 1, "must reach the stub; stderr: {err}");
    assert!(
        bodies[0].contains(r#""text":"--no-color --quiet hi""#),
        "{}",
        bodies[0]
    );
}

/// #1110: `--help` after `--` is a prompt, not the help page; before `--` it
/// is still the help page (near-miss: exit 0, nothing sent).
#[test]
fn help_after_double_dash_is_sent_as_the_prompt() {
    let (_, out, err, hits, bodies) = run_with_bodies(&["--", "--help"]);
    assert!(hits >= 1, "must reach the stub; stderr: {err}");
    assert!(!out.contains("Usage:"), "{out}");
    assert!(bodies[0].contains(r#""text":"--help""#), "{}", bodies[0]);

    let (code, out, _, hits) = run(&["--help", "--", "x"]);
    assert_eq!(code, Some(0));
    assert!(out.contains("Usage:"), "{out}");
    assert_eq!(hits, 0);
}

/// Near-miss: an unknown flag BEFORE `--` is still #1109's refusal.
#[test]
fn unknown_flag_before_double_dash_still_refuses() {
    let (code, _, err, hits) = run(&["--bogus", "--", "hi"]);
    assert_eq!(code, Some(2), "stderr: {err}");
    assert!(err.contains("'--bogus'"), "{err}");
    assert_eq!(hits, 0);
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

/// #1111: `yoyo commit --ammend` with a staged change used to make a NEW commit
/// titled `--ammend`. It must now exit 2 and leave HEAD where it was; the
/// near-miss `-m "fix x"` (git's habit) still commits with message `fix x`.
#[test]
fn commit_with_a_misspelled_flag_refuses_and_creates_no_commit() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(["-c", "user.email=a@b", "-c", "user.name=a"])
            .args(args)
            .current_dir(&repo)
            .env("HOME", &home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q"]);
    std::fs::write(repo.join("f"), "a\n").unwrap();
    git(&["add", "f"]);
    git(&["commit", "-qm", "i"]);
    std::fs::write(repo.join("f"), "a\nb\n").unwrap();
    git(&["add", "f"]);
    let head = git(&["rev-parse", "HEAD"]);
    let yoyo = |args: &[&str]| {
        scrubbed(&home)
            .args(args)
            .current_dir(&repo)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "a")
            .env("GIT_AUTHOR_EMAIL", "a@b")
            .env("GIT_COMMITTER_NAME", "a")
            .env("GIT_COMMITTER_EMAIL", "a@b")
            .output()
            .expect("spawn yoyo")
    };

    let out = yoyo(&["commit", "--ammend"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr: {err}");
    assert!(err.contains("did you mean --amend?"), "{err}");
    assert_eq!(git(&["rev-parse", "HEAD"]), head, "a commit was made");

    let out = yoyo(&["commit", "-m", "fix x"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr: {err}");
    assert_ne!(git(&["rev-parse", "HEAD"]), head);
    assert_eq!(git(&["log", "-1", "--format=%s"]), "fix x");
}
