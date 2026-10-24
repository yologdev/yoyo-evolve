//! `/bg kill` really stops the job (#1077).
//!
//! Before #1077, `kill` aborted the job's tokio task, which dropped a
//! `tokio::process::Child` without `kill_on_drop`, so the shell kept running
//! after a kill that returned `true`. These tests are that measurement turned
//! into assertions: real processes, pids written to a temp dir, liveness
//! checked with `kill -0` (a zombie counts as dead: it is no longer running).

use super::*;

/// Is `pid` still running? `kill(pid, 0)` succeeds for a zombie too, so on
/// Linux a `Z` state in `/proc/<pid>/stat` counts as dead.
#[cfg(unix)]
fn pid_running(pid: i32) -> bool {
    // SAFETY: signal 0 only checks existence/permission; touches no memory.
    if unsafe { libc::kill(pid, 0) } != 0 {
        return false;
    }
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        // Format: `pid (comm) S ...`; the state follows the last ')'.
        Ok(stat) => match stat.rfind(')') {
            Some(i) => !stat[i + 1..].trim_start().starts_with('Z'),
            None => true,
        },
        Err(_) => true,
    }
}

/// Wait (bounded) for a pid file the job writes, and parse it.
#[cfg(unix)]
async fn read_pid_file(path: &std::path::Path) -> i32 {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Ok(s) = std::fs::read_to_string(path) {
            if let Ok(pid) = s.trim().parse::<i32>() {
                return pid;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "job never wrote {}",
            path.display()
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Poll up to ~2s for `pid` to stop running.
#[cfg(unix)]
async fn dies_within_2s(pid: i32) -> bool {
    for _ in 0..100 {
        if !pid_running(pid) {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    !pid_running(pid)
}

/// Kill the pid if a failing assertion would otherwise leak it.
#[cfg(unix)]
fn reap_leftover(pid: i32) {
    // SAFETY: plain signal send; failure (already gone) is fine.
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn bg_kill_stops_the_jobs_shell() {
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("pid");
    let tracker = BackgroundJobTracker::new();
    let id = tracker.launch(&format!(
        "echo $$ > '{}'; exec sleep 30",
        pid_path.display()
    ));
    let pid = read_pid_file(&pid_path).await;
    assert!(
        pid_running(pid),
        "anti-vacuous: the job must be running first"
    );

    assert!(tracker.kill(id).await);
    let dead = dies_within_2s(pid).await;
    if !dead {
        reap_leftover(pid);
    }
    assert!(
        dead,
        "/bg kill returned true but pid {pid} is still running"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn bg_kill_stops_the_shells_background_children_too() {
    let dir = tempfile::tempdir().unwrap();
    let gpid_path = dir.path().join("gpid");
    let tracker = BackgroundJobTracker::new();
    let id = tracker.launch(&format!(
        "sleep 30 & echo $! > '{}'; wait",
        gpid_path.display()
    ));
    let gpid = read_pid_file(&gpid_path).await;
    assert!(
        pid_running(gpid),
        "anti-vacuous: the grandchild must be running first"
    );

    assert!(tracker.kill(id).await);
    let dead = dies_within_2s(gpid).await;
    if !dead {
        reap_leftover(gpid);
    }
    assert!(dead, "an explicit /bg kill must take down the whole group");
}

#[cfg(unix)]
#[tokio::test]
async fn bg_kill_reports_the_signal_not_success() {
    let tracker = BackgroundJobTracker::new();
    let id = tracker.launch("sleep 30");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(!tracker.is_finished(id));

    assert!(tracker.kill(id).await);
    assert!(tracker.is_finished(id));
    let job = tracker.list().into_iter().find(|j| j.id == id).unwrap();
    // What actually happened: the shell died on the group's SIGKILL, read
    // from the real wait status rather than a sentinel or a fabricated 0.
    assert_eq!(job.exit_code, Some(-libc::SIGKILL));
    assert_eq!(
        crate::tools::describe_exit_code(job.exit_code.unwrap()),
        "-9 (SIGKILL)"
    );
}

/// Near-miss: a job that ends on its own keeps its real exit code; the group
/// handle is disarmed on a normal exit and never signals.
#[tokio::test]
async fn bg_job_that_exits_on_its_own_keeps_its_code() {
    let tracker = BackgroundJobTracker::new();
    let id = tracker.launch("exit 3");
    for _ in 0..100 {
        if tracker.is_finished(id) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let job = tracker.list().into_iter().find(|j| j.id == id).unwrap();
    assert!(job.finished);
    assert_eq!(job.exit_code, Some(3));
}

/// Near-miss for #1063: once `sh` has exited normally, a backgrounded
/// grandchild is still NOT killed, by `/bg` or by the group handle.
#[cfg(unix)]
#[tokio::test]
async fn bg_normal_exit_still_leaves_the_grandchild_alone() {
    let dir = tempfile::tempdir().unwrap();
    let gpid_path = dir.path().join("gpid");
    let tracker = BackgroundJobTracker::new();
    let id = tracker.launch(&format!(
        "sleep 30 >/dev/null 2>&1 & echo $! > '{}'",
        gpid_path.display()
    ));
    let gpid = read_pid_file(&gpid_path).await;
    for _ in 0..150 {
        if tracker.is_finished(id) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(tracker.is_finished(id));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let alive = pid_running(gpid);
    reap_leftover(gpid);
    assert!(
        alive,
        "#1063: a normal shell exit must not kill its grandchild"
    );
}
