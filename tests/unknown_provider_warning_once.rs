//! #1176 at the real binary: "treating unknown provider X as OpenAI-compatible"
//! prints exactly once per run, whichever door named the provider (the
//! `--provider` flag or `.yoyo.toml`). Before the fix `create_model_config`
//! printed it on every model-config build, five times for one `-p hi`.
//!
//! The base URL is a dead loopback port, so no real host is contacted. The
//! env scrubbing is copied from tests/unknown_provider_refusal.rs.

use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const NEEDLE: &str = "treating unknown provider";
const DEAD_URL: &str = "http://127.0.0.1:9";

/// stderr of yoyo with `args` in a scratch HOME/project.
fn run(args: &[&str], project_toml: Option<&str>) -> String {
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
        .env_remove("DEEPSEEK_API_KEY")
        .env_remove("YOYO_RESTRICTED")
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
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            timed_out = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().expect("wait");
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(!timed_out, "yoyo did not exit within 60s; stderr: {err}");
    err
}

fn count(err: &str) -> usize {
    err.lines().filter(|l| l.contains(NEEDLE)).count()
}

#[test]
fn unknown_provider_flag_warns_exactly_once() {
    let err = run(
        &["--provider", "nosuch", "--base-url", DEAD_URL, "-p", "hi"],
        None,
    );
    assert_eq!(count(&err), 1, "stderr: {err}");
    // The one line still names the user's provider and URL.
    assert!(
        err.contains(&format!("'nosuch' as OpenAI-compatible ({DEAD_URL})")),
        "{err}"
    );
}

#[test]
fn unknown_provider_from_config_file_warns_exactly_once() {
    let toml = format!("provider = \"nosuch\"\nbase_url = \"{DEAD_URL}\"\n");
    let err = run(&["-p", "hi"], Some(&toml));
    assert_eq!(count(&err), 1, "stderr: {err}");
    assert!(err.contains("'nosuch'"), "{err}");
}

#[test]
fn known_provider_never_prints_the_unknown_provider_warning() {
    let err = run(
        &["--provider", "openai", "--base-url", DEAD_URL, "-p", "hi"],
        None,
    );
    assert_eq!(count(&err), 0, "stderr: {err}");
}
