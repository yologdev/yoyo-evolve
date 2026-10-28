//! Write to a child's stdin without letting SIGPIPE kill yoyo.
//!
//! `main.rs` restores SIGPIPE to `SIG_DFL` so `yoyo ... | head` dies quietly
//! like any Unix CLI. The cost of that process-wide choice: a `write` into a
//! pipe whose reader has already exited no longer returns `EPIPE`. It kills
//! yoyo outright, with no stderr and no exit code. Every "a failed write is
//! fine, ignore it" (`let _ = stdin.write_all(..)`) and every
//! `map_err("Failed to write ...")` on a child's stdin was therefore
//! unreachable: the branch it guards is a death by signal 13.
//!
//! Measured, not inferred (Day 242): `tests/hook_feedback_request.rs` ran a
//! post-hook `echo X >&2`, which never reads stdin. Under CPU load the hook
//! exited before yoyo wrote the payload in 48 of 60 runs, and yoyo died
//! `exit=None signal=Some(13)` after one POST. That is the CI red of
//! 2026-10-28 01:54 (`posts=1 exit=None`, stderr empty).
//!
//! The fix is the standard per-thread one, not a change to the process-wide
//! disposition (other threads may be writing stdout, and toggling a global
//! from one thread races them). Block SIGPIPE in this thread, write, consume
//! the SIGPIPE our own write raised (if one was not already pending), then
//! restore the mask. SIGPIPE from `write` is directed at the writing thread,
//! so blocking it here is enough. `sigpending` + `sigwait` rather than
//! `sigtimedwait`, because macOS has no `sigtimedwait`; `sigwait` returns at
//! once when the signal is already pending, which is the only time we call it.

use std::io::{self, Write};

/// `w.write_all(buf)`, except that a reader that has gone away yields
/// `Err(BrokenPipe)` instead of SIGPIPE killing the process. Use it for
/// every write into a child process's piped stdin.
pub(crate) fn write_all_no_sigpipe<W: Write>(w: &mut W, buf: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        // SAFETY: the sigset calls only touch the local `sigset_t`s we pass,
        // and `pthread_sigmask` changes only the calling thread's mask, which
        // is restored before returning.
        unsafe {
            let mut pipe_set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut pipe_set);
            libc::sigaddset(&mut pipe_set, libc::SIGPIPE);
            let mut old: libc::sigset_t = std::mem::zeroed();
            if libc::pthread_sigmask(libc::SIG_BLOCK, &pipe_set, &mut old) != 0 {
                return w.write_all(buf);
            }
            let already_pending = sigpipe_pending();
            let result = w.write_all(buf);
            if !already_pending && sigpipe_pending() {
                let mut sig: libc::c_int = 0;
                libc::sigwait(&pipe_set, &mut sig);
            }
            libc::pthread_sigmask(libc::SIG_SETMASK, &old, std::ptr::null_mut());
            result
        }
    }
    #[cfg(not(unix))]
    {
        w.write_all(buf)
    }
}

#[cfg(unix)]
fn sigpipe_pending() -> bool {
    // SAFETY: as above, only a local `sigset_t` is touched.
    unsafe {
        let mut pending: libc::sigset_t = std::mem::zeroed();
        libc::sigpending(&mut pending) == 0 && libc::sigismember(&pending, libc::SIGPIPE) == 1
    }
}

#[cfg(all(test, unix))]
#[path = "sigpipe_safe_write_tests.rs"]
mod sigpipe_safe_write_tests;
