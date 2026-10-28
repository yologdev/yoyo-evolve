//! Tests for `sigpipe_safe_write`. The cargo test harness runs with SIGPIPE
//! ignored (Rust's default), where a dead-reader write already returns EPIPE,
//! so an in-process test cannot tell the fix from its absence. The
//! discriminating tests therefore re-exec this test binary, filtered to the
//! probe below, with SIGPIPE set to `SIG_DFL` as `main.rs` sets it for yoyo.
//! Mutating the disposition only in that child keeps the parallel harness safe.

use super::*;
use std::os::unix::io::FromRawFd;
use std::os::unix::process::ExitStatusExt;
use std::process::Command;

const PROBE_ENV: &str = "YOYO_SIGPIPE_PROBE";
const SURVIVED: &str = "SIGPIPE-PROBE-SURVIVED";

/// A pipe whose read end is already closed: any write raises SIGPIPE/EPIPE.
fn dead_reader_pipe() -> std::fs::File {
    let mut fds = [0 as libc::c_int; 2];
    // SAFETY: `pipe` fills two fds we then own; the read end is closed at once.
    unsafe {
        assert_eq!(libc::pipe(fds.as_mut_ptr()), 0, "pipe() failed");
        libc::close(fds[0]);
        std::fs::File::from_raw_fd(fds[1])
    }
}

/// Inert in a normal run. With `PROBE_ENV` set (only by the re-exec below) it
/// restores `SIG_DFL` for SIGPIPE, writes into a dead pipe, and prints
/// `SURVIVED` if the process is still alive afterwards.
#[test]
fn sigpipe_probe_child() {
    let Ok(mode) = std::env::var(PROBE_ENV) else {
        return;
    };
    // SAFETY: process-wide, but this process is the dedicated probe child.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let mut w = dead_reader_pipe();
    let err = if mode == "raw" {
        w.write_all(b"payload").unwrap_err()
    } else {
        write_all_no_sigpipe(&mut w, b"payload").unwrap_err()
    };
    assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    // A second write must also survive: the consumed signal is not left
    // pending, and the mask was restored rather than leaked.
    if mode != "raw" {
        let again = write_all_no_sigpipe(&mut w, b"payload").unwrap_err();
        assert_eq!(again.kind(), io::ErrorKind::BrokenPipe);
    }
    println!("{SURVIVED}");
}

fn run_probe(mode: &str) -> (std::process::ExitStatus, String) {
    let exe = std::env::current_exe().expect("current_exe");
    let out = Command::new(exe)
        .args([
            "--exact",
            "sigpipe_safe_write::sigpipe_safe_write_tests::sigpipe_probe_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(PROBE_ENV, mode)
        .output()
        .expect("re-exec test binary");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    (out.status, stdout)
}

/// The fix: under `SIG_DFL`, a write to a reader that has exited returns
/// `BrokenPipe` and the process lives.
#[test]
fn dead_reader_returns_broken_pipe_instead_of_killing_the_process() {
    let (status, stdout) = run_probe("safe");
    assert_eq!(
        status.signal(),
        None,
        "the probe died by signal (13 = SIGPIPE): {stdout}"
    );
    assert!(status.success(), "probe failed: {status:?}\n{stdout}");
    // Anti-vacuous: the filter really selected and ran the probe.
    assert!(stdout.contains(SURVIVED), "probe never ran: {stdout}");
}

/// The control that makes the test above mean something: the same fixture
/// with a plain `write_all` dies by SIGPIPE, as yoyo did.
#[test]
fn control_a_plain_write_under_sig_dfl_dies_by_sigpipe() {
    let (status, stdout) = run_probe("raw");
    assert_eq!(
        status.signal(),
        Some(libc::SIGPIPE),
        "fixture did not reproduce the SIGPIPE death: {status:?}\n{stdout}"
    );
    assert!(!stdout.contains(SURVIVED));
}

/// Near-miss: a live reader gets the bytes unchanged, and the calling
/// thread's signal mask is left exactly as it was.
#[test]
fn live_reader_gets_the_bytes_and_the_mask_is_restored() {
    let mut fds = [0 as libc::c_int; 2];
    // SAFETY: `pipe` fills two fds that the two `File`s then own.
    let (mut r, mut w) = unsafe {
        assert_eq!(libc::pipe(fds.as_mut_ptr()), 0);
        (
            std::fs::File::from_raw_fd(fds[0]),
            std::fs::File::from_raw_fd(fds[1]),
        )
    };
    let blocked = || unsafe {
        let mut cur: libc::sigset_t = std::mem::zeroed();
        libc::pthread_sigmask(libc::SIG_BLOCK, std::ptr::null(), &mut cur);
        libc::sigismember(&cur, libc::SIGPIPE) == 1
    };
    let before = blocked();
    write_all_no_sigpipe(&mut w, "héllo 🐙\n".as_bytes()).unwrap();
    drop(w);
    let mut got = String::new();
    io::Read::read_to_string(&mut r, &mut got).unwrap();
    assert_eq!(got, "héllo 🐙\n");
    assert_eq!(blocked(), before, "SIGPIPE mask leaked out of the helper");
    // And on the failure path too.
    let _ = write_all_no_sigpipe(&mut dead_reader_pipe(), b"x");
    assert_eq!(blocked(), before, "SIGPIPE mask leaked on the EPIPE path");
}
