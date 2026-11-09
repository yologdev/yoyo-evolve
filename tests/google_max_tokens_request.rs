//! #1158 at the wire: what `generationConfig.maxOutputTokens` yoyo sends to
//! Google for a model id with no preset, with and without a user cap.
//!
//! The guessed-limits note used to tell a google user "Every reply (reasoning
//! included) is capped at 8192 output tokens", interpolating
//! `ModelConfig::google`'s `max_tokens`. Reading yoagent 0.27.4 said that
//! value never goes out (`provider/google.rs` sets `maxOutputTokens` only from
//! `StreamConfig.max_tokens`, which yoyo fills only from a user
//! `--max-tokens` / `max_tokens`). Reading is evidence for my half of the hop
//! only, so this test runs the built binary against a capturing stub and
//! looks at the request body itself.
//!
//! Hermetic the same way `tests/hook_feedback_request.rs` is: HOME and every
//! XDG dir point into a tempdir, and cwd is a tempdir, so neither this repo's
//! `.yoyo.toml` (`max_tokens = 128000`, which would silence the note and put
//! a cap on the wire) nor the runner's own config applies. Ambient provider
//! keys are removed so the only key is the dummy one set here.
//!
//! The two rows are each other's controls: row (b) proves the stub reads the
//! field at all, so row (a)'s absence is not a parse miss.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// A minimal successful Gemini `streamGenerateContent?alt=sse` reply.
const SSE_REPLY: &str = "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"ok\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1,\"totalTokenCount\":2}}\n\n";

/// A stub on 127.0.0.1:0 that records every POST as `(request line, body)`
/// and answers each with [`SSE_REPLY`], whatever the path.
#[allow(clippy::type_complexity)]
fn start_capturing_stub() -> (u16, Arc<Mutex<Vec<(String, String)>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let posts = Arc::new(Mutex::new(Vec::new()));
    let store = Arc::clone(&posts);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let Ok(clone) = stream.try_clone() else {
                continue;
            };
            let mut reader = BufReader::new(clone);
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
            let body = String::from_utf8_lossy(&body).into_owned();
            if request_line.starts_with("POST") {
                store
                    .lock()
                    .unwrap()
                    .push((request_line.trim_end().to_string(), body));
            }
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                SSE_REPLY.len(),
                SSE_REPLY
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (port, posts)
}

struct Run {
    posts: Vec<(String, String)>,
    stderr: String,
    stdout: String,
    exit: Option<i32>,
    /// `exit=None` alone cannot say why yoyo died (Day 242: a SIGPIPE death
    /// read as `exit=None`, stderr empty), so the signal is kept too.
    signal: Option<i32>,
}

#[cfg(unix)]
fn termination_signal(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
fn termination_signal(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}

impl Run {
    fn ctx(&self) -> String {
        let posts: Vec<String> = self
            .posts
            .iter()
            .map(|(line, body)| format!("{line}\n{body}"))
            .collect();
        format!(
            "posts={} exit={:?} signal={:?}\nstderr:\n{}\nstdout:\n{}\nrequests:\n{}",
            self.posts.len(),
            self.exit,
            self.signal,
            self.stderr,
            self.stdout,
            posts.join("\n---\n")
        )
    }

    /// The first request's `generationConfig.maxOutputTokens`, as JSON
    /// (`Null` when the key, or the whole `generationConfig`, is absent).
    fn max_output_tokens(&self) -> serde_json::Value {
        let ctx = self.ctx();
        let (line, body) = self
            .posts
            .first()
            .unwrap_or_else(|| panic!("yoyo never reached the stub\n{ctx}"));
        assert!(
            line.contains(":streamGenerateContent"),
            "first POST is not a Gemini generate call\n{ctx}"
        );
        let req: serde_json::Value = serde_json::from_str(body)
            .unwrap_or_else(|e| panic!("request body is not JSON ({e})\n{ctx}"));
        assert!(
            req["contents"].is_array(),
            "request body has no `contents`; not a Gemini body\n{ctx}"
        );
        req["generationConfig"]["maxOutputTokens"].clone()
    }
}

fn run_google(extra: &[&str]) -> Run {
    let tmp = tempfile::tempdir().expect("tempdir");
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    run_in(&proj, &home, extra)
}

fn run_in(proj: &Path, home: &Path, extra: &[&str]) -> Run {
    let (port, posts) = start_capturing_stub();
    let base_url = format!("http://127.0.0.1:{port}");
    let mut args: Vec<&str> = vec![
        "--provider",
        "google",
        "--model",
        "gemini-zzz",
        "--base-url",
        &base_url,
    ];
    args.extend_from_slice(extra);
    args.extend_from_slice(&["-p", "hi"]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args(&args)
        .current_dir(proj)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("GEMINI_API_KEY", "x")
        .env_remove("GOOGLE_API_KEY")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("API_KEY")
        .env_remove("MODEL")
        .env_remove("YOYO_RESTRICTED")
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
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    let posts = posts.lock().unwrap().clone();
    Run {
        posts,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        exit: out.status.code(),
        signal: termination_signal(&out.status),
    }
}

/// Row (a): no `--max-tokens`. ModelConfig's guessed 8192 is NOT sent, so
/// Google applies the model's own default output limit, and the note says
/// that instead of claiming a cap.
#[test]
fn no_user_max_tokens_sends_no_output_cap_and_the_note_says_so() {
    let run = run_google(&[]);
    let ctx = run.ctx();
    assert_eq!(
        run.max_output_tokens(),
        serde_json::Value::Null,
        "maxOutputTokens went out with no user cap\n{ctx}"
    );
    // The note still fires (the window guess is real) and names the model...
    assert!(
        run.stderr
            .contains("no preset is known for google model 'gemini-zzz'"),
        "guessed-limits note did not fire\n{ctx}"
    );
    // ...but no longer claims a cap the wire does not carry.
    assert!(
        !run.stderr.contains("capped at"),
        "note still claims an output cap that is never sent\n{ctx}"
    );
    assert!(
        run.stderr
            .contains("yoyo sends no output cap, so Google applies the model's own default"),
        "note does not say what goes on the wire\n{ctx}"
    );
}

/// Row (b), the positive control: a user cap reaches the wire, so the stub
/// does read the field and row (a)'s `Null` is a real absence.
#[test]
fn user_max_tokens_reaches_the_wire_as_max_output_tokens() {
    let run = run_google(&["--max-tokens", "1000"]);
    let ctx = run.ctx();
    assert_eq!(
        run.max_output_tokens(),
        serde_json::json!(1000),
        "--max-tokens 1000 did not reach generationConfig.maxOutputTokens\n{ctx}"
    );
    // A user cap silences the note entirely (it only speaks for guesses).
    assert!(
        !run.stderr.contains("no preset is known"),
        "note fired despite a user max_tokens\n{ctx}"
    );
}
