//! #1103 at the real binary: a mistyped `--provider` with no base URL is
//! refused at startup (exit 2, did-you-mean) before any network call. Before,
//! it became an OpenAI-compatible client aimed at localhost:8080 carrying the
//! fallback key, retried ~80s and blamed the network.
//!
//! Env scrub copied from `tests/guessed_limits_note_binary.rs` (d233): the
//! evolve job exports provider keys and CI does not.

use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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
        .env("ANTHROPIC_API_KEY", "sk-ant-dummy-test-key")
        .env_remove("API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("DEEPSEEK_API_KEY")
        .env_remove("MODEL")
        .env_remove("YOYO_AUDIT")
        .env_remove("YOYO_RESTRICTED")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    let start = Instant::now();
    let mut timed_out = false;
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(20) {
            let _ = child.kill();
            timed_out = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().expect("wait");
    assert!(
        !timed_out,
        "yoyo did not exit within 20s (the pre-#1103 retry loop?)"
    );
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn assert_refused(code: Option<i32>, err: &str) {
    assert_eq!(code, Some(2), "stderr: {err}");
    assert!(err.contains("did you mean 'anthropic'"), "{err}");
    assert!(err.contains("--provider custom --base-url <url>"), "{err}");
    assert!(!err.contains("Network error"), "{err}");
    assert!(!err.contains("/retry"), "{err}");
    assert!(!err.contains("localhost:8080"), "{err}");
}

#[test]
fn mistyped_provider_flag_is_refused_before_any_network_call() {
    let (code, err) = run(&["--provider", "anthropc", "-p", "hi"], None);
    assert_refused(code, &err);
}

#[test]
fn mistyped_provider_in_yoyo_toml_is_refused_the_same_way() {
    // Second door: `provider = "..."` in the project config.
    let (code, err) = run(
        &["--trust-project", "-p", "hi"],
        Some("provider = \"anthropc\"\n"),
    );
    assert_refused(code, &err);
}
