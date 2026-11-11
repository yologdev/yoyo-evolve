//! #1175 at the real binary: `custom` (and an unknown OpenAI-compatible
//! provider) with no model named refuses with exit 2 and sends nothing,
//! instead of sending yoyo's Anthropic default to the user's own server.
//! The printed remedy (`--model <id>`), added to the same argv, proceeds.
//! Every URL is loopback; env scrubbing as in tests/unknown_provider_warning_once.rs.

use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DEAD_URL: &str = "http://127.0.0.1:9";

/// (exit code, stderr) of yoyo with `args` in a scratch HOME/project.
fn run(args: &[&str], project_toml: Option<&str>) -> (Option<i32>, String) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    if let Some(toml) = project_toml {
        std::fs::write(proj.join(".yoyo.toml"), toml).unwrap();
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args(args)
        .current_dir(&proj)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env_remove("ANTHROPIC_API_KEY")
        .env("API_KEY", "x")
        .env_remove("OPENAI_API_KEY")
        .env_remove("YOYO_PROVIDER")
        .env_remove("YOYO_MODEL")
        .env_remove("MODEL")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    let start = Instant::now();
    let mut timed_out = false;
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            timed_out = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().expect("wait");
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(!timed_out, "yoyo did not exit within 30s; stderr: {err}");
    (out.status.code(), err)
}

/// A loopback listener nobody serves, and a check that nothing connected.
fn quiet_server() -> (TcpListener, String) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.set_nonblocking(true).unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    (l, url)
}

fn assert_refused(code: Option<i32>, err: &str, provider: &str, url: &str, l: &TcpListener) {
    assert_eq!(code, Some(2), "stderr: {err}");
    assert!(
        err.contains(&format!("provider '{provider}' needs a model name")),
        "{err}"
    );
    assert!(err.contains("--model <id>"), "{err}");
    assert!(err.contains(&format!("curl {url}/models")), "{err}");
    assert!(!err.contains("claude-opus-5-5"), "{err}");
    assert!(l.accept().is_err(), "a request reached the server: {err}");
}

#[test]
fn custom_with_no_model_refuses_and_sends_nothing() {
    let (l, url) = quiet_server();
    let (code, err) = run(
        &["--provider", "custom", "--base-url", &url, "-p", "hi"],
        None,
    );
    assert_refused(code, &err, "custom", &url, &l);
}

#[test]
fn unknown_provider_with_no_model_refuses_and_sends_nothing() {
    let (l, url) = quiet_server();
    let (code, err) = run(
        &["--provider", "nosuch", "--base-url", &url, "-p", "hi"],
        None,
    );
    assert_refused(code, &err, "nosuch", &url, &l);
}

#[test]
fn custom_from_config_file_with_no_model_refuses() {
    let (l, url) = quiet_server();
    let toml = format!("provider = \"custom\"\nbase_url = \"{url}\"\n");
    let (code, err) = run(&["-p", "hi"], Some(&toml));
    assert_refused(code, &err, "custom", &url, &l);
}

/// The remedy the refusal prints, appended to the motivating argv, runs on
/// the named model (the dead port then fails the request, which is fine).
#[test]
fn printed_remedy_appended_to_the_same_argv_proceeds_on_that_model() {
    let (code, err) = run(
        &[
            "--provider",
            "custom",
            "--base-url",
            DEAD_URL,
            "-p",
            "hi",
            "--model",
            "my-model",
        ],
        None,
    );
    assert!(err.contains("model: my-model"), "{err}");
    assert!(!err.contains("needs a model name"), "{err}");
    assert_ne!(code, Some(2), "{err}");
}

/// Config `model` is the other remedy the message names.
#[test]
fn config_file_model_satisfies_the_requirement() {
    let toml = format!("provider = \"custom\"\nbase_url = \"{DEAD_URL}\"\nmodel = \"my-model\"\n");
    let (_, err) = run(&["-p", "hi"], Some(&toml));
    assert!(err.contains("model: my-model"), "{err}");
    assert!(!err.contains("needs a model name"), "{err}");
}
