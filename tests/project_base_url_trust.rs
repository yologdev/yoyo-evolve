//! #1179 at the wire: a project's `.yoyo.toml` `base_url` is a trust grant.
//!
//! A repository's `.yoyo.toml` is written by whoever wrote the repository. Before
//! #1179 its `base_url` was used with no trust check, so cloning a repo and
//! running `yoyo -p hi` in it sent the user's API key to the repo author's host.
//! Now an untrusted project's `base_url` is withheld: for a provider with an
//! endpoint of its own the run goes there instead (the key reaches its owner),
//! and for `custom` / an unknown provider (no endpoint of its own) yoyo refuses
//! before any request.
//!
//! Every row runs the built binary against a recording stub on 127.0.0.1:0,
//! with the environment cleared (`env_clear`) and a temp `HOME`, so no ambient
//! key or user config reaches the run. The key is `canary-not-a-key`: it names
//! nothing of the user's, and no row contacts a real vendor host.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const CANARY: &str = "canary-not-a-key";

/// A stub that records each request's line and credential headers, answering 401.
fn start_stub() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let store = Arc::clone(&seen);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                continue;
            }
            let mut content_length = 0usize;
            let mut creds = Vec::new();
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
                    } else if k.eq_ignore_ascii_case("authorization")
                        || k.eq_ignore_ascii_case("x-api-key")
                    {
                        creds.push(format!("{k}={}", v.trim()));
                    }
                }
            }
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
            store
                .lock()
                .unwrap()
                .push(format!("{} {}", request_line.trim_end(), creds.join(" ")));
            let reply = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid key"}}"#;
            let resp = format!(
                "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                reply.len(),
                reply
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("http://127.0.0.1:{port}/v1"), seen)
}

struct Run {
    seen: Vec<String>,
    stderr: String,
    exit: Option<i32>,
}

impl Run {
    fn ctx(&self) -> String {
        format!(
            "exit={:?}\nrequests:\n{}\nstderr:\n{}",
            self.exit,
            self.seen.join("\n"),
            self.stderr
        )
    }
}

struct Dirs {
    _tmp: tempfile::TempDir,
    proj: std::path::PathBuf,
    home: std::path::PathBuf,
}

/// A temp project (a git repo with one staged file, so `yoyo review` has a
/// diff) and a temp `HOME`. `project_toml`, when given, becomes `.yoyo.toml`.
fn dirs(project_toml: Option<&str>) -> Dirs {
    let tmp = tempfile::tempdir().expect("tempdir");
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(home.join(".config/yoyo")).unwrap();
    std::fs::write(proj.join("f.txt"), "a\n").unwrap();
    for g in [&["init", "-q", "."][..], &["add", "f.txt"][..]] {
        let ok = Command::new("git")
            .args(g)
            .current_dir(&proj)
            .env("HOME", &home)
            .status()
            .expect("git")
            .success();
        assert!(ok, "git {g:?} failed");
    }
    if let Some(t) = project_toml {
        std::fs::write(proj.join(".yoyo.toml"), t).unwrap();
    }
    Dirs {
        _tmp: tmp,
        proj,
        home,
    }
}

/// Run the built binary in `d.proj` with ONLY `PATH`, the temp `HOME`/XDG dirs,
/// `NO_COLOR` and `API_KEY=canary` (the generic key every provider falls back to).
fn run(d: &Dirs, args: &[&str], seen: &Arc<Mutex<Vec<String>>>) -> Run {
    let home = &d.home;
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    cmd.args(args)
        .current_dir(&d.proj)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("NO_COLOR", "1")
        .env("API_KEY", CANARY)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn yoyo");
    let deadline = Instant::now() + Duration::from_secs(60);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    Run {
        seen: seen.lock().unwrap().clone(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        exit: out.status.code(),
    }
}

fn custom_toml(url: &str) -> String {
    format!("provider = \"custom\"\nmodel = \"x\"\nbase_url = \"{url}\"\n")
}

fn assert_names_the_remedies(r: &Run) {
    let ctx = r.ctx();
    assert!(r.stderr.contains("base_url"), "names the key\n{ctx}");
    assert!(r.stderr.contains(".yoyo.toml"), "names the file\n{ctx}");
    assert!(r.stderr.contains("--trust-project"), "remedy 1\n{ctx}");
    assert!(r.stderr.contains("--base-url"), "remedy 2\n{ctx}");
}

/// The issue's repro, refuse branch: `custom` has no endpoint of its own, so
/// withholding the project's URL leaves nothing honest to talk to. Before the
/// fix the stub received the canary on every attempt.
#[test]
fn untrusted_project_custom_base_url_is_refused_and_nothing_is_sent() {
    let (url, seen) = start_stub();
    let d = dirs(Some(&custom_toml(&url)));
    let r = run(&d, &["-p", "hi"], &seen);
    let ctx = r.ctx();
    assert!(r.seen.is_empty(), "the project's host got requests\n{ctx}");
    assert_eq!(r.exit, Some(2), "{ctx}");
    assert!(r.stderr.contains("Nothing was sent"), "{ctx}");
    assert_names_the_remedies(&r);
}

/// Ignore branch: `ollama` has an endpoint of its own (localhost:11434), so the
/// run goes there, not to the project's URL, and says so.
#[test]
fn untrusted_project_base_url_is_withheld_for_a_provider_with_its_own_endpoint() {
    let (url, seen) = start_stub();
    let toml = format!("provider = \"ollama\"\nmodel = \"x\"\nbase_url = \"{url}\"\n");
    let d = dirs(Some(&toml));
    let r = run(&d, &["-p", "hi", "--max-turns", "1"], &seen);
    let ctx = r.ctx();
    assert!(r.seen.is_empty(), "the project's host got requests\n{ctx}");
    assert!(r.stderr.contains("not used"), "{ctx}");
    assert_names_the_remedies(&r);
}

/// The opt-in still works: `--trust-project` hands the project its URL.
#[test]
fn trust_project_flag_uses_the_project_base_url() {
    let (url, seen) = start_stub();
    let d = dirs(Some(&custom_toml(&url)));
    let r = run(&d, &["--trust-project", "-p", "hi"], &seen);
    let ctx = r.ctx();
    assert!(
        !r.seen.is_empty(),
        "trusted run never reached the stub\n{ctx}"
    );
    assert!(r.seen[0].contains(CANARY), "{ctx}");
    assert!(!r.stderr.contains("Nothing was sent"), "{ctx}");
}

/// The remembered trust store (`--trust-project-always`) counts as trust too,
/// on the main path and on `yoyo review`, which resolves its endpoint itself.
#[test]
fn trust_store_uses_the_project_base_url_on_both_doors() {
    for verb in [&["-p", "hi"][..], &["review"][..]] {
        let (url, seen) = start_stub();
        let d = dirs(Some(&custom_toml(&url)));
        let canon = std::fs::canonicalize(&d.proj).unwrap();
        std::fs::write(
            d.home.join(".config/yoyo/trusted_dirs"),
            format!("{}\n", canon.display()),
        )
        .unwrap();
        let r = run(&d, verb, &seen);
        let ctx = r.ctx();
        assert!(
            !r.seen.is_empty(),
            "{verb:?}: store-trusted run never reached the stub\n{ctx}"
        );
    }
}

/// `base_url` in the user's own config (no project file) is the user's choice.
#[test]
fn user_config_base_url_is_used() {
    let (url, seen) = start_stub();
    let d = dirs(None);
    std::fs::write(d.home.join(".config/yoyo/config.toml"), custom_toml(&url)).unwrap();
    let r = run(&d, &["-p", "hi"], &seen);
    let ctx = r.ctx();
    assert!(!r.seen.is_empty(), "user config URL not used\n{ctx}");
    assert!(!r.stderr.contains("Nothing was sent"), "{ctx}");
}

/// `--base-url` on the command line is the user's choice and beats the
/// project's URL without any notice about it.
#[test]
fn base_url_flag_is_used_in_an_untrusted_project() {
    let (url, seen) = start_stub();
    let (other, other_seen) = start_stub();
    let d = dirs(Some(&custom_toml(&other)));
    let r = run(&d, &["--base-url", &url, "-p", "hi"], &seen);
    let ctx = r.ctx();
    assert!(!r.seen.is_empty(), "--base-url not used\n{ctx}");
    assert!(other_seen.lock().unwrap().is_empty(), "{ctx}");
    assert!(!r.stderr.contains("Nothing was sent"), "{ctx}");
}

/// The second door: `yoyo review` reads `base_url` on its own path
/// (dispatch_sub.rs), before the main path's trust decision runs.
#[test]
fn review_refuses_an_untrusted_project_base_url() {
    let (url, seen) = start_stub();
    let d = dirs(Some(&custom_toml(&url)));
    let r = run(&d, &["review"], &seen);
    let ctx = r.ctx();
    assert!(
        r.seen.is_empty(),
        "review sent to the project's host\n{ctx}"
    );
    assert_eq!(r.exit, Some(2), "{ctx}");
    assert_names_the_remedies(&r);
}
