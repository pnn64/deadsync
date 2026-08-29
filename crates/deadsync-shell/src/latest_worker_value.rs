use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, TryLockError};

/// Single-value worker-to-frame handoff for replaceable progress state.
///
/// Background workers publish under a mutex; the frame thread uses `try_lock`
/// and skips a busy sample instead of blocking. The shell owns the slot for one
/// service session. `start` clears stale state, storage is capped at one value,
/// and `finish` destroys any unconsumed value on the caller's thread. Replaced
/// values are destroyed by the publishing worker. There is no eviction scan or
/// allocation inside the handoff itself; payload allocation belongs to `T`.
/// Callers expose behavior tests and handoff benchmarks instead of counters.
/// Worst-case frame work is one atomic load, one uncontended try-lock, and one
/// moved value.
pub(crate) struct LatestWorkerValue<T> {
    active_id: AtomicU64,
    ready: AtomicBool,
    value: Mutex<Option<T>>,
}

impl<T> Default for LatestWorkerValue<T> {
    fn default() -> Self {
        Self {
            active_id: AtomicU64::new(0),
            ready: AtomicBool::new(false),
            value: Mutex::new(None),
        }
    }
}

impl<T> LatestWorkerValue<T> {
    pub(crate) fn start(&self, id: u64) {
        let mut value = self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.active_id.store(id, Ordering::Release);
        *value = None;
        self.ready.store(false, Ordering::Release);
    }

    pub(crate) fn publish(&self, id: u64, next: T) {
        if self.active_id.load(Ordering::Acquire) != id {
            return;
        }
        let mut value = self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.active_id.load(Ordering::Relaxed) == id {
            *value = Some(next);
            self.ready.store(true, Ordering::Release);
        }
    }

    pub(crate) fn take(&self, id: u64) -> Option<T> {
        if self.active_id.load(Ordering::Acquire) != id {
            return None;
        }
        if !self.ready.load(Ordering::Acquire) {
            return None;
        }
        let mut value = match self.value.try_lock() {
            Ok(value) => value,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
            Err(TryLockError::WouldBlock) => return None,
        };
        if self.active_id.load(Ordering::Relaxed) != id {
            return None;
        }
        let value = value.take();
        self.ready.store(false, Ordering::Release);
        value
    }

    pub(crate) fn finish(&self, id: u64) {
        if self
            .active_id
            .compare_exchange(id, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        self.value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        self.ready.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_generation_replaces_and_moves_latest_value() {
        let latest = LatestWorkerValue::default();
        latest.start(7);
        latest.publish(7, "first".to_owned());
        latest.publish(7, "latest".to_owned());

        assert_eq!(latest.take(7).as_deref(), Some("latest"));
        assert!(latest.take(7).is_none());
    }

    #[test]
    fn stale_generation_cannot_publish_or_finish_current_value() {
        let latest = LatestWorkerValue::default();
        latest.start(1);
        latest.publish(1, "old".to_owned());
        latest.start(2);
        latest.publish(1, "stale".to_owned());
        latest.publish(2, "current".to_owned());
        latest.finish(1);

        assert_eq!(latest.take(2).as_deref(), Some("current"));
    }
}
