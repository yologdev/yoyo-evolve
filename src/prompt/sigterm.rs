//! #1015: SIGTERM on a single-shot run whose stdout is a pipe.
//!
//! On that population the retry-safe gate (#991) HOLDS each attempt's text
//! until its `MessageEnd`, so the default SIGTERM disposition (die at once)
//! lost it: `timeout -s TERM 12 yoyo -p ... | wc -c` read 0 bytes while the
//! same run with `-s INT` read 4920. Callers: any CI / `timeout` /
//! supervisor wrapper around `yoyo -p | ...`, and evolve.sh's own
//! `timeout $t yoyo ... | tee log` phases.
//!
//! **The hazard this module is shaped around:** once tokio registers a
//! SIGTERM handler, the default disposition is gone for the rest of the
//! process. A SIGTERM arriving while no `select!` polls it would be
//! swallowed and `timeout` would have to escalate to SIGKILL. So:
//! - the handler is registered only when the gate is filtering (TTY, REPL,
//!   `--print`, json and stream-json never register it and still die by
//!   default), and
//! - the handler is a process-lifetime watcher task that ALWAYS ends the
//!   process with 143 (129 for SIGHUP, 131 for SIGQUIT): when a text loop is listening it hands the loop the
//!   signal (the loop flushes and returns) and exits after a grace if `main`
//!   has not already; when no loop is listening it flushes stdout and exits.
//!
//! **#1016: SIGHUP rides the same watcher** (a dropped ssh session or a
//! closed terminal under `yoyo -p ... | tee log`). Measured before the fix:
//! `kill -HUP` mid-stream left 0 of the 18 streamed bytes on stdout and the
//! process died by signal. Same registration condition, same flush, exit 129
//! (128 + 1). The module keeps its name; it now owns every *terminating*
//! signal yoyo handles on the held-text population.
//!
//! **SIGQUIT rides it too (Day 228, #1016), exit 131 (128 + 3).** Ctrl+\
//! from a terminal while stdout is piped (`yoyo -p ... | tee log`) measured
//! 0 bytes on stdout and death by signal. Superseded claim, recorded rather
//! than erased: this doc said SIGQUIT was "not handled, by decision ... a
//! 'dump core' request, not a polite stop". The decision is reversed because
//! the user reaching for "harder Ctrl+C" lost everything already produced.
//! The trade, stated: while the handler is armed, SIGQUIT no longer dumps
//! core; it is armed in exactly the window SIGHUP/SIGTERM are (the filtering
//! population only), so every other run keeps the core-dump default.
//! Still not handled: SIGKILL (unflushable), SIGPIPE (the reader is gone;
//! by reasoning, not probed); a panic while text is held is
//! flushed by `RetrySafeGate`'s `Drop` (Day 228, probed: before it, `""`).

use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::sync::OnceLock;
use tokio::sync::watch;

/// 128 + SIGTERM(15), the shell convention (SIGINT is 130).
pub(super) const SIGTERM_EXIT: i32 = 143;
/// #1016: 128 + SIGHUP(1).
pub(super) const SIGHUP_EXIT: i32 = 129;
/// #1016: 128 + SIGQUIT(3).
pub(super) const SIGQUIT_EXIT: i32 = 131;

/// The exit code of the first terminating signal received (0 = none).
static TERMINATED: AtomicI32 = AtomicI32::new(0);
/// Text loops currently polling a [`SigtermListener`].
static LISTENING: AtomicUsize = AtomicUsize::new(0);
static CHANNEL: OnceLock<watch::Sender<bool>> = OnceLock::new();

/// The exit code owed to a terminating signal this process received:
/// 143 for SIGTERM (#1015), 129 for SIGHUP, 131 for SIGQUIT (#1016),
/// `None` if none.
pub(super) fn terminated_exit_code() -> Option<i32> {
    match TERMINATED.load(Ordering::SeqCst) {
        0 => None,
        code => Some(code),
    }
}

/// A text loop's handle on the SIGTERM watcher; dropping it tells the
/// watcher no loop will flush, so it must end the process itself.
pub(super) struct SigtermListener {
    rx: watch::Receiver<bool>,
}

impl Drop for SigtermListener {
    fn drop(&mut self) {
        LISTENING.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Arm the watcher (once per process) and return a listener — only when
/// `filtering` (the population whose text is held). `None` everywhere else,
/// and on non-unix targets, so those runs never replace the default.
pub(super) fn listen(filtering: bool) -> Option<SigtermListener> {
    if !filtering || !cfg!(unix) {
        return None;
    }
    let tx = CHANNEL.get_or_init(install);
    LISTENING.fetch_add(1, Ordering::SeqCst);
    Some(SigtermListener { rx: tx.subscribe() })
}

/// Resolve when SIGTERM arrives; never resolves for `None` (unarmed).
pub(super) async fn recv(listener: &mut Option<SigtermListener>) {
    match listener.as_mut() {
        Some(l) => {
            let _ = l.rx.wait_for(|terminated| *terminated).await;
        }
        None => std::future::pending::<()>().await,
    }
}

#[cfg(unix)]
fn install() -> watch::Sender<bool> {
    use tokio::signal::unix::SignalKind;
    let (tx, _) = watch::channel(false);
    arm(SignalKind::terminate(), SIGTERM_EXIT);
    // #1016: same watcher, same flush, its own exit code.
    arm(SignalKind::hangup(), SIGHUP_EXIT);
    // #1016: Ctrl+\ — same watcher; trades SIGQUIT's core dump for the flush.
    arm(SignalKind::quit(), SIGQUIT_EXIT);
    tx
}

/// Register one terminating signal. Registration failure leaves that
/// signal's default disposition intact.
#[cfg(unix)]
fn arm(kind: tokio::signal::unix::SignalKind, exit_code: i32) {
    use std::io::Write;
    use std::time::Duration;
    use tokio::signal::unix::signal;
    /// How long the listening loop gets to flush and `main` to exit.
    const LOOP_GRACE: Duration = Duration::from_secs(5);
    let Ok(mut sig) = signal(kind) else {
        return;
    };
    tokio::spawn(async move {
        if sig.recv().await.is_none() {
            return;
        }
        // First signal wins the exit code; a later one cannot relabel it.
        let _ = TERMINATED.compare_exchange(0, exit_code, Ordering::SeqCst, Ordering::SeqCst);
        if LISTENING.load(Ordering::SeqCst) > 0 {
            if let Some(tx) = CHANNEL.get() {
                tx.send_replace(true);
            }
            // A second instance of the same signal skips the grace.
            tokio::select! {
                _ = tokio::time::sleep(LOOP_GRACE) => {}
                _ = sig.recv() => {}
            }
        } else {
            let note = super::interrupt::interrupt_note(true);
            let _ = std::io::stderr().write_all(note.as_bytes());
        }
        let _ = std::io::stdout().flush();
        // #1099: MCP servers sit in their own process group, so this
        // signal never reached them; close them (bounded) on the way out.
        crate::mcp_prefix::close_live_mcp_servers(crate::mcp_prefix::MCP_CLOSE_BOUND).await;
        std::process::exit(terminated_exit_code().unwrap_or(exit_code));
    });
}

#[cfg(not(unix))]
fn install() -> watch::Sender<bool> {
    watch::channel(false).0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The population guard: a run whose gate does not hold text (TTY, REPL,
    /// --print, json, stream-json) never registers a handler, so SIGTERM
    /// keeps its default disposition there.
    #[test]
    fn unfiltered_runs_never_register_a_sigterm_handler() {
        assert!(listen(false).is_none());
    }
}
