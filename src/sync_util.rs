//! Shared synchronisation helpers — lock-recovery for poisoned locks.
//!
//! When a thread panics while holding a `Mutex` or `RwLock` the lock becomes
//! "poisoned".  Rather than cascading the panic to every subsequent caller we
//! recover the inner data — the data itself is still valid, only the invariant
//! *might* be broken, and for our use-cases (counters, output buffers, session
//! state) that is acceptable.
//!
//! `lock_or_recover` (Mutex) was extracted on Day 58 to deduplicate helpers in
//! `commands_bg`, `commands_spawn`, and `session`.
//!
//! `rw_read_or_recover` / `rw_write_or_recover` (RwLock) were added on Day 109
//! to deduplicate identical helpers independently reinvented in `watch`,
//! `commands_fork`, `commands_stash`, and `commands_todo`.

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
