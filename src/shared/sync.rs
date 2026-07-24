use std::sync::{Mutex, MutexGuard};

/// Lock a `Mutex<T>` and return a guard, logging a warning if the mutex was poisoned.
/// This ensures thread-panic data corruption is never silently swallowed.
pub fn lock_or_warn<'a, T>(m: &'a Mutex<T>, name: &str) -> MutexGuard<'a, T> {
    m.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("Mutex '{}' was poisoned — recovering. Data may be stale.", name);
        poisoned.into_inner()
    })
}
