//! Shared synchronisation helpers — lock-recovery for poisoned locks.
//!
//! When a thread panics while holding a `Mutex` or `RwLock` the lock becomes
//! "poisoned".  Rather than cascading the panic to every subsequent caller we
//! recover the inner data — the data itself is still valid, only the invariant
//! *might* be broken, and for our use-cases (counters, output buffers, session
//! state) that is acceptable.
//!
//! `lock_or_recover` (Mutex) was extracted on Day 58 to deduplicate helpers in
//! `commands_bg`, `commands_spawn`, and `session`. That one held: measured
//! Day 193, 52 call sites across 9 files resolve here and there is no surviving
//! private `lock_or_recover` anywhere in `src/`.
//!
//! **Superseded claim, recorded rather than erased (Day 193, blind round 94).**
//! This doc used to read: *"`rw_read_or_recover` / `rw_write_or_recover`
//! (RwLock) were added on Day 109 to deduplicate identical helpers
//! independently reinvented in `watch`, `commands_fork`, `commands_stash`, and
//! `commands_todo`."* The first half is true — they were added on Day 109 for
//! that reason. The second half asserts a dedup that **did not happen**:
//! `watch` was converted, and `commands_fork`, `commands_stash` and
//! `commands_todo` **still carry byte-identical private copies** of both
//! functions — same names, same signatures, same bodies, same doc comments —
//! so 17 of the call sites that look like uses of this module reach a duplicate
//! instead. Day 109 converted **1 of the 4 files it names**.
//!
//! The wrong sentence is kept above rather than deleted because it is the only
//! detector: a doc-side repair of a doc/code mismatch removes the evidence while
//! leaving everything demonstrably honest (Day 164). It is also self-concealing
//! — the three files' calls read as uses of this module in any name-based grep,
//! which is the error round 94's own census made on its first pass. The
//! surviving duplication is **filed as an issue, not fixed here** (one defect
//! per round); see the `src/sync_util.rs` bullet in CLAUDE.md for the issue
//! number and the pasteable remedy.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Acquire a [`Mutex`] guard, recovering from a poisoned mutex instead of
/// panicking.
///
/// # Examples
///
/// ```ignore
/// let mutex = std::sync::Mutex::new(42);
/// let guard = lock_or_recover(&mutex);
/// assert_eq!(*guard, 42);
/// ```
pub fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Acquire a [`RwLock`] read-guard, recovering from a poisoned lock instead of
/// panicking.
pub fn rw_read_or_recover<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|e| e.into_inner())
}

/// Acquire a [`RwLock`] write-guard, recovering from a poisoned lock instead of
/// panicking.
pub fn rw_write_or_recover<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_lock_or_recover_normal() {
        let mutex = Mutex::new(42);
        let guard = lock_or_recover(&mutex);
        assert_eq!(*guard, 42);
    }

    #[test]
    fn test_lock_or_recover_poisoned() {
        let mutex = Arc::new(Mutex::new(vec![1, 2, 3]));
        let m2 = Arc::clone(&mutex);

        // Poison the mutex by panicking while holding the lock
        let _ = std::thread::spawn(move || {
            let _guard = m2.lock().unwrap();
            panic!("intentional panic to poison mutex");
        })
        .join();

        // The mutex is now poisoned — .lock().unwrap() would panic here
        assert!(mutex.lock().is_err(), "mutex should be poisoned");

        // lock_or_recover should still give us the data
        let guard = lock_or_recover(&mutex);
        assert_eq!(*guard, vec![1, 2, 3]);
    }

    #[test]
    fn test_rw_read_or_recover_normal() {
        let lock = RwLock::new(42);
        let guard = rw_read_or_recover(&lock);
        assert_eq!(*guard, 42);
    }

    #[test]
    fn test_rw_write_or_recover_normal() {
        let lock = RwLock::new(42);
        let mut guard = rw_write_or_recover(&lock);
        *guard = 99;
        drop(guard);
        let guard = rw_read_or_recover(&lock);
        assert_eq!(*guard, 99);
    }

    #[test]
    fn test_rw_read_or_recover_poisoned() {
        let lock = Arc::new(RwLock::new(vec![1, 2, 3]));
        let l2 = Arc::clone(&lock);

        // Poison the RwLock by panicking while holding a write-guard
        let _ = std::thread::spawn(move || {
            let _guard = l2.write().unwrap();
            panic!("intentional panic to poison rwlock");
        })
        .join();

        // The lock is now poisoned — .read().unwrap() would panic here
        assert!(lock.read().is_err(), "rwlock should be poisoned");

        // rw_read_or_recover should still give us the data
        let guard = rw_read_or_recover(&lock);
        assert_eq!(*guard, vec![1, 2, 3]);
    }

    #[test]
    fn test_rw_write_or_recover_poisoned() {
        let lock = Arc::new(RwLock::new(vec![1, 2, 3]));
        let l2 = Arc::clone(&lock);

        // Poison the RwLock
        let _ = std::thread::spawn(move || {
            let _guard = l2.write().unwrap();
            panic!("intentional panic to poison rwlock");
        })
        .join();

        assert!(lock.write().is_err(), "rwlock should be poisoned");

        // rw_write_or_recover should still give us a writable guard
        let mut guard = rw_write_or_recover(&lock);
        guard.push(4);
        assert_eq!(*guard, vec![1, 2, 3, 4]);
    }
}
