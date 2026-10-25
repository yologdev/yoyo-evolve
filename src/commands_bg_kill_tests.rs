//! `/bg kill` really stops the job (#1077).
//!
//! Before #1077, `kill` aborted the job's tokio task, which dropped a
//! `tokio::process::Child` without `kill_on_drop`, so the shell kept running
//! after a kill that returned `true`. These tests are that measurement turned
//! into assertions: real processes, pids written to a temp dir, liveness
//! checked with `kill -0` (a zombie counts as dead: it is no longer running).

use super::*;

/// Grace the TERM-ignoring tests pass through the `kill_with` /
/// `stop_running_with` seam, so each waits ~1s instead of `BG_STOP_GRACE`.
/// The shared-grace test needs it well above its jobs' start-up noise: two
/// per-job windows (2s) must be distinguishable from one shared one (1s).
const TEST_GRACE: std::time::Duration = std::time::Duration::from_secs(1);

/// #1081: the value is decided, with a reason in its doc comment. Changing it
/// means re-arguing it there, not just editing the number.
#[test]
fn bg_stop_grace_is_docker_stops_ten_seconds() {
    assert_eq!(
        BG_STOP_GRACE,
        std::time::Duration::from_secs(10),
        "BG_STOP_GRACE was chosen as docker stop's 10s default (#1081); re-argue it in its doc comment before changing it"
    );
}

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
    // Ignores TERM since #1080 (kill is TERM, grace, then KILL), so this
    // still pins the SIGKILL end; the TERM end has its own test below.
    let id = tracker.launch("trap '' TERM; sleep 30");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(!tracker.is_finished(id));

    assert!(tracker.kill_with(id, TEST_GRACE).await);
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

// ---- #1080: TERM first, a short grace, then KILL ----

/// Wait (bounded) for the job to finish.
async fn wait_finished(tracker: &BackgroundJobTracker, id: u32) {
    for _ in 0..150 {
        if tracker.is_finished(id) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

fn code_of(tracker: &BackgroundJobTracker, id: u32) -> Option<i32> {
    tracker
        .list()
        .into_iter()
        .find(|j| j.id == id)
        .unwrap()
        .exit_code
}

/// A job that does not catch TERM dies of it, and `/bg list` says so.
#[cfg(unix)]
#[tokio::test]
async fn bg_kill_ends_an_ordinary_job_with_sigterm() {
    let tracker = BackgroundJobTracker::new();
    let id = tracker.launch("sleep 30");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(!tracker.is_finished(id), "anti-vacuous: must be running");

    let start = std::time::Instant::now();
    assert!(tracker.kill(id).await);
    assert!(
        start.elapsed() < std::time::Duration::from_millis(1500),
        "a job that dies on TERM must not wait out the grace: {:?}",
        start.elapsed()
    );
    assert_eq!(code_of(&tracker, id), Some(-libc::SIGTERM));
    assert_eq!(
        crate::tools::describe_exit_code(-libc::SIGTERM),
        "-15 (SIGTERM)"
    );
}

/// The grace is real: a job that traps TERM gets to run its cleanup (write a
/// marker) and exit with its own code, which is never replaced by `-9`.
#[cfg(unix)]
#[tokio::test]
async fn bg_kill_lets_a_term_trap_clean_up() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("cleaned");
    let ready = dir.path().join("ready");
    let tracker = BackgroundJobTracker::new();
    let id = tracker.launch(&format!(
        "trap 'echo done > \"{}\"; exit 7' TERM; echo $$ > '{}'; while :; do sleep 0.05; done",
        marker.display(),
        ready.display()
    ));
    let pid = read_pid_file(&ready).await;
    assert!(pid_running(pid), "anti-vacuous: the job must be running");
    assert!(!marker.exists(), "anti-vacuous: no cleanup before the kill");

    assert!(tracker.kill(id).await);
    let dead = dies_within_2s(pid).await;
    if !dead {
        reap_leftover(pid);
    }
    assert!(dead);
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap_or_default().trim(),
        "done",
        "the TERM trap never ran: no grace was given"
    );
    assert_eq!(code_of(&tracker, id), Some(7));
}

/// A job that ignores TERM is still stopped (by KILL, after the grace), and
/// so is its grandchild: the #1077 group property survives the grace.
#[cfg(unix)]
#[tokio::test]
async fn bg_kill_falls_back_to_sigkill_for_the_whole_group() {
    let dir = tempfile::tempdir().unwrap();
    let gpid_path = dir.path().join("gpid");
    let tracker = BackgroundJobTracker::new();
    let id = tracker.launch(&format!(
        "trap '' TERM; sleep 60 & echo $! > '{}'; wait",
        gpid_path.display()
    ));
    let gpid = read_pid_file(&gpid_path).await;
    assert!(
        pid_running(gpid),
        "anti-vacuous: grandchild must be running"
    );

    let start = std::time::Instant::now();
    assert!(tracker.kill_with(id, TEST_GRACE).await);
    let took = start.elapsed();
    let dead = dies_within_2s(gpid).await;
    if !dead {
        reap_leftover(gpid);
    }
    assert!(dead, "the TERM-ignoring grandchild survived the kill");
    assert!(took >= TEST_GRACE, "KILL came before the grace: {took:?}");
    assert!(
        took < TEST_GRACE + std::time::Duration::from_secs(2),
        "kill took {took:?}"
    );
    assert_eq!(code_of(&tracker, id), Some(-libc::SIGKILL));
}

/// Near-miss: a job that already finished gets no signal; its code stays.
#[tokio::test]
async fn bg_kill_on_a_finished_job_keeps_its_code() {
    let tracker = BackgroundJobTracker::new();
    let id = tracker.launch("exit 3");
    wait_finished(&tracker, id).await;
    assert_eq!(code_of(&tracker, id), Some(3), "anti-vacuous");
    let start = std::time::Instant::now();
    tracker.kill(id).await;
    assert!(start.elapsed() < std::time::Duration::from_millis(500));
    assert_eq!(code_of(&tracker, id), Some(3));
}

/// `/quit`: two TERM-ignoring jobs share ONE grace, so leaving takes about
/// one grace, not one per job; both end on KILL; a finished job is untouched.
#[cfg(unix)]
#[tokio::test]
async fn bg_stop_running_shares_one_grace_across_jobs() {
    let tracker = BackgroundJobTracker::new();
    let done = tracker.launch("exit 4");
    wait_finished(&tracker, done).await;
    let a = tracker.launch("trap '' TERM; sleep 60");
    let b = tracker.launch("trap '' TERM; sleep 60");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(!tracker.is_finished(a) && !tracker.is_finished(b));

    let start = std::time::Instant::now();
    assert_eq!(tracker.stop_running_with(TEST_GRACE).await, 2);
    let took = start.elapsed();
    assert!(took >= TEST_GRACE, "KILL came before the grace: {took:?}");
    assert!(
        took < TEST_GRACE * 2,
        "two jobs took {took:?}: the grace is per job, not shared"
    );
    assert_eq!(code_of(&tracker, a), Some(-libc::SIGKILL));
    assert_eq!(code_of(&tracker, b), Some(-libc::SIGKILL));
    assert_eq!(code_of(&tracker, done), Some(4));
}
