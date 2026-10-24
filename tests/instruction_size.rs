//! An oversized project instruction file gets a loud stderr warning, and is
//! still loaded in full (Part of #1070). Emission-point test on the real binary.
//!
//! The spawn helper's env handling is copied from `tests/config_fifo.rs`
//! (`run_yoyo_env`) for hygiene: inherited provider keys are stripped, so the result does
//! not depend on whether the runner exports keys (the evolve gate does, CI
//! does not). Never invokes cargo.
#![cfg(unix)]

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

const DEADLINE: Duration = Duration::from_secs(20);

fn run_yoyo_env(args: &[&str], cwd: &Path, home: &Path, env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    for (name, _) in std::env::vars() {
        if name == "API_KEY" || name.ends_with("_API_KEY") {
            cmd.env_remove(name);
        }
    }
    let child = cmd
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env_remove("MODEL")
        .env_remove("YOYO_MODEL")
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    // Drain on a thread: `--print-system-prompt` writes ~200 KB, more than a
    // pipe buffer, so the `try_wait` poll in config_fifo.rs would deadlock here.
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output().expect("collect output"));
    });
    match rx.recv_timeout(DEADLINE) {
        Ok(out) => out,
        Err(_) => {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            panic!("yoyo {args:?} still running after {DEADLINE:?}");
        }
    }
}

/// A CLAUDE.md of exactly `bytes` bytes whose last line is a unique marker,
/// so stdout containing the marker proves the file was not truncated.
fn write_claude_md(dir: &Path, bytes: usize, marker: &str) {
    // No trailing newline: the loader trims, and the warning counts the
    // bytes that reach the prompt, so file size == loaded size here.
    let tail = format!("\n{marker}");
    assert!(bytes > tail.len());
    let mut body = "x".repeat(bytes - tail.len());
    body.push_str(&tail);
    assert_eq!(body.len(), bytes);
    std::fs::write(dir.join("CLAUDE.md"), body).unwrap();
}

fn print_system_prompt(bytes: usize, marker: &str) -> Output {
    let proj = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(proj.path())
        .status();
    write_claude_md(proj.path(), bytes, marker);
    let out = run_yoyo_env(
        &["--print-system-prompt"],
        proj.path(),
        home.path(),
        &[("ANTHROPIC_API_KEY", "sk-dummy-not-a-real-key")],
    );
    // Keep the tempdirs alive until the process has finished.
    drop(proj);
    drop(home);
    out
}

#[test]
fn oversized_claude_md_warns_on_stderr_and_still_loads_in_full() {
    let marker = "END-OF-BIG-FILE-MARKER-1070";
    let out = print_system_prompt(200_000, marker);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "exit {:?}\nstderr:\n{stderr}",
        out.status
    );
    assert!(
        stderr.contains("project instruction file CLAUDE.md is 200000 bytes"),
        "no oversized-file warning on stderr:\n{stderr}"
    );
    assert!(stderr.contains("~50000 tokens, estimated"), "{stderr}");
    assert!(
        stdout.contains(marker),
        "the file must still load in full (warning, not a cap)"
    );
}

#[test]
fn ordinary_claude_md_gets_no_size_warning() {
    let marker = "END-OF-SMALL-FILE-MARKER-1070";
    let out = print_system_prompt(41_713, marker);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "exit {:?}\nstderr:\n{stderr}",
        out.status
    );
    assert!(
        stdout.contains(marker),
        "control: CLAUDE.md really reached the prompt"
    );
    assert!(
        !stderr.contains("project instruction file"),
        "41,713 bytes must not warn:\n{stderr}"
    );
}
