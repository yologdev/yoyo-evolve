//! #1114 at the real binary: a flag written in the value slot of a
//! value-taking flag is that flag's value, not a flag. Before, `yoyo -p
//! --print-system-prompt` warned that the value looked like a flag and then
//! printed the system prompt instead of sending the text.
//!
//! The spawn helper's scrubbing is copied from tests/unknown_flag_refusal.rs:
//! no inherited provider keys (the evolve job exports them and CI does not;
//! Day 233), a scratch HOME/XDG, and a fake key against a local stub.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// A stub that keeps each request body and answers with a 400.
fn start_stub() -> (u16, Arc<AtomicUsize>, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let (count, kept) = (Arc::clone(&hits), Arc::clone(&bodies));
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

struct Run {
    code: Option<i32>,
    out: String,
    err: String,
    bodies: Vec<String>,
}

/// Run `yoyo --base-url <stub> <args>` in a scratch HOME/project; a run that
/// reaches the stub is killed once it has.
fn run(args: &[&str]) -> Run {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    let (port, hits, bodies) = start_stub();
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
    let bodies = bodies.lock().unwrap().clone();
    Run {
        code: out.status.code(),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
        bodies,
    }
}

/// The opening of the default system prompt (`SYSTEM_PROMPT`); the next
/// test checks it is still in the source, so it cannot drift silently.
fn default_prompt_marker() -> &'static str {
    "You are a coding assistant working in the user's terminal."
}

#[test]
fn default_prompt_marker_is_in_the_source() {
    // Anti-vacuous: the absence rows below mean something only if the marker
    // really is what an unmodified `--print-system-prompt` prints.
    assert!(include_str!("../src/cli_config.rs").contains(default_prompt_marker()));
}

#[test]
fn print_system_prompt_as_the_prompt_value_is_sent_not_applied() {
    let r = run(&["-p", "--print-system-prompt"]);
    assert!(
        !r.out.contains(default_prompt_marker()),
        "the system prompt must not print; stdout: {}",
        r.out
    );
    assert!(
        r.err.contains("looks like another flag"),
        "the flag-like-value warning still fires; stderr: {}",
        r.err
    );
    assert!(
        r.bodies.iter().any(|b| b.contains("--print-system-prompt")),
        "the text must reach the model; bodies: {:?}; stderr: {}",
        r.bodies,
        r.err
    );
}

/// Near-miss: the flag on its own still prints and sends nothing.
#[test]
fn bare_print_system_prompt_still_prints() {
    let r = run(&["--print-system-prompt"]);
    assert_eq!(r.code, Some(0), "stderr: {}", r.err);
    assert!(r.out.contains(default_prompt_marker()), "stdout: {}", r.out);
    assert!(r.bodies.is_empty(), "nothing is sent");
}

/// The first `--print-system-prompt` is `--system`'s value; the second one is
/// a flag and prints that prompt.
#[test]
fn value_slot_taken_then_second_flag_applies() {
    let r = run(&["--system", "--print-system-prompt", "--print-system-prompt"]);
    assert_eq!(r.code, Some(0), "stderr: {}", r.err);
    assert!(
        r.out.starts_with("--print-system-prompt"),
        "the system text is the value; stdout: {}",
        r.out
    );
    assert!(
        !r.out.contains(default_prompt_marker()),
        "stdout: {}",
        r.out
    );
    assert!(r.bodies.is_empty(), "nothing is sent");
}

/// #1110 regression row: after `--` the word is prompt text.
#[test]
fn after_double_dash_it_is_prompt_text() {
    let r = run(&["--", "--print-system-prompt"]);
    assert!(
        !r.out.contains(default_prompt_marker()),
        "stdout: {}",
        r.out
    );
    assert!(
        r.bodies.iter().any(|b| b.contains("--print-system-prompt")),
        "bodies: {:?}; stderr: {}",
        r.bodies,
        r.err
    );
}

/// `dispatch_sub::flag_value` (#1114, Day 252): `--model` written as
/// `--system`'s value is the system prompt, so the run does not switch to the
/// model named after it. Before, the request went out on `claude-zzz`. Since
/// #1150 the word after it is stray beside `-p`, so the run is refused and
/// names it; before that, it vanished without a word.
#[test]
fn model_in_system_value_slot_names_the_stray_word_and_sends_nothing() {
    let r = run(&["--system", "--model", "claude-zzz", "-p", "hi"]);
    assert_eq!(r.code, Some(2), "stderr: {}", r.err);
    assert!(r.err.contains("'claude-zzz'"), "stderr: {}", r.err);
    assert!(r.err.contains("Nothing was sent"), "stderr: {}", r.err);
    assert!(
        r.bodies.is_empty(),
        "nothing is sent; bodies: {:?}",
        r.bodies
    );
    assert!(
        !r.err.contains("Unknown model 'claude-zzz'"),
        "it was never a model; stderr: {}",
        r.err
    );
}

/// The #1114 property without the stray word: `--model` in `--system`'s value
/// slot is the system prompt and does not take `-p` as the model.
#[test]
fn model_in_system_value_slot_does_not_switch_the_model() {
    let r = run(&["--system", "--model", "-p", "hi"]);
    assert!(
        !r.bodies.is_empty(),
        "anti-vacuous: a request must reach the stub; stderr: {}",
        r.err
    );
    assert!(
        r.bodies.iter().all(|b| !b.contains(r#""model":"-p""#)),
        "the model must not be -p; bodies: {:?}",
        r.bodies
    );
    assert!(
        r.bodies.iter().any(|b| b.contains("--model")),
        "the system prompt is the literal `--model`; bodies: {:?}",
        r.bodies
    );
}

/// #1150: a bare word beside `-p` is refused, named, and nothing is sent.
#[test]
fn stray_word_beside_prompt_is_refused() {
    for args in [&["-p", "hi", "extra"][..], &["extra", "-p", "hi"][..]] {
        let r = run(args);
        assert_eq!(r.code, Some(2), "{args:?} stderr: {}", r.err);
        assert!(r.err.contains("'extra'"), "{args:?} stderr: {}", r.err);
        assert!(r.err.contains("Nothing was sent"), "stderr: {}", r.err);
        assert!(r.bodies.is_empty(), "{args:?} bodies: {:?}", r.bodies);
    }
}

/// Near-misses for #1150: the two documented ways to send the extra word
/// still send all of it.
#[test]
fn quoted_or_double_dash_prompt_still_sends_every_word() {
    for args in [&["-p", "hi", "--", "extra"][..], &["-p", "hi extra"][..]] {
        let r = run(args);
        assert!(
            r.bodies.iter().any(|b| b.contains(r#""text":"hi extra""#)),
            "{args:?} bodies: {:?}; stderr: {}",
            r.bodies,
            r.err
        );
        assert!(!r.err.contains("would be dropped"), "stderr: {}", r.err);
    }
}

/// Near-miss: a real `--model` still selects the model.
#[test]
fn model_as_a_flag_still_switches_the_model() {
    let r = run(&["--model", "claude-zzz", "-p", "hi"]);
    assert!(
        r.bodies
            .iter()
            .any(|b| b.contains(r#""model":"claude-zzz""#)),
        "bodies: {:?}; stderr: {}",
        r.bodies,
        r.err
    );
    assert!(
        r.err.contains("Unknown model 'claude-zzz'"),
        "stderr: {}",
        r.err
    );
}

/// Subcommand caller (`version -v` reads `--model` through `flag_value` with
/// the verb at argv[1]): unchanged, it still reports the flag's model.
#[test]
fn subcommand_flag_value_is_unchanged() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let out = scrubbed(&home)
        .args(["version", "-v", "--model", "claude-zzz"])
        .current_dir(tmp.path())
        .output()
        .expect("run yoyo");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "stdout: {stdout}");
    assert!(stdout.contains("model: claude-zzz"), "stdout: {stdout}");
}
