//! #1138 (yoyo's half): an `SSL_CERT_FILE` that loads no CA certificates used
//! to make the provider client's `reqwest::Client::new()` panic mid-turn
//! ("panicked at ... No CA certificates were loaded", then "reply cut off").
//! Now yoyo refuses before building any agent and names the variable.
//!
//! Runs the built binary with a fake key and a dead loopback base URL, so no
//! network or real credential is involved. Env is scrubbed the same way
//! `tests/loopback_refused_binary.rs` does (d233): the evolve job exports
//! provider keys and CI does not.
//!
//! Only meaningful where the verifier reads these variables (Linux-like
//! targets); elsewhere the pre-flight is a no-op by design.
#![cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]

use std::path::Path;
use std::process::{Command, Output, Stdio};

fn run_with_cert_file(cert_file: &Path) -> Output {
    let tmp = tempfile::tempdir().expect("tempdir");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    let args = ["--base-url", "http://127.0.0.1:9", "--max-turns", "1"];
    run_in(&proj, &[&args[..], &["-p", "say ok"]].concat(), cert_file)
}

fn run_in(proj: &Path, args: &[&str], cert_file: &Path) -> Output {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args(args)
        .current_dir(proj)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("ANTHROPIC_API_KEY", "dummy-test-key")
        .env("SSL_CERT_FILE", cert_file)
        .env_remove("SSL_CERT_DIR")
        .env_remove("API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("DEEPSEEK_API_KEY")
        .env_remove("MODEL")
        .env_remove("YOYO_AUDIT")
        .env_remove("YOYO_RESTRICTED")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn yoyo")
}

#[test]
fn empty_ssl_cert_file_is_refused_by_name_not_by_panic() {
    let tmp = tempfile::tempdir().unwrap();
    let empty = tmp.path().join("empty-ca.pem");
    std::fs::write(&empty, b"").unwrap();

    let out = run_with_cert_file(&empty);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{err}");
    assert!(err.contains("SSL_CERT_FILE"), "stderr:\n{err}");
    assert!(err.contains("which is empty"), "stderr:\n{err}");
    assert!(!err.contains("panicked"), "stderr:\n{err}");
    assert!(!err.contains("reply cut off"), "stderr:\n{err}");
}

#[test]
fn system_bundle_in_ssl_cert_file_is_not_refused() {
    let bundle = Path::new("/etc/ssl/certs/ca-certificates.crt");
    if !bundle.is_file() {
        eprintln!("skip: no system CA bundle at {}", bundle.display());
        return;
    }
    let out = run_with_cert_file(bundle);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("no CA certificates could be loaded"),
        "a valid bundle was refused:\n{err}"
    );
    assert!(!err.contains("panicked"), "stderr:\n{err}");
    // It gets as far as the dead base URL instead.
    assert!(err.contains("127.0.0.1:9"), "stderr:\n{err}");
}

/// `yoyo review` is a shell verb that builds its own model agent (it never
/// reaches main's pre-flight), so it carries the same refusal. Probed before
/// the fix: the reqwest panic, then "review failed: agent returned empty response".
#[test]
fn review_with_empty_ssl_cert_file_is_refused_by_name_not_by_panic() {
    let tmp = tempfile::tempdir().unwrap();
    let empty = tmp.path().join("empty-ca.pem");
    std::fs::write(&empty, b"").unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |a: &[&str]| {
        let st = Command::new("git")
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(a)
            .current_dir(&repo)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git");
        assert!(st.success(), "git {a:?}");
    };
    git(&["init", "-q"]);
    std::fs::write(repo.join("f.txt"), "a\n").unwrap();
    git(&["add", "f.txt"]);
    git(&["commit", "-qm", "init"]);
    std::fs::write(repo.join("f.txt"), "a\nb\n").unwrap();

    let out = run_in(
        &repo,
        &["review", "--base-url", "http://127.0.0.1:9"],
        &empty,
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{err}");
    assert!(err.contains("SSL_CERT_FILE"), "stderr:\n{err}");
    assert!(err.contains("which is empty"), "stderr:\n{err}");
    assert!(!err.contains("panicked"), "stderr:\n{err}");
}
