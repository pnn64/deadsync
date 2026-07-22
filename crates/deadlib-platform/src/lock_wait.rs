use log::debug;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

static LOCK_WAIT_EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);

const LOCK_WAIT_REPORT_INTERVAL_NS: u64 = 5_000_000_000;
const LOCK_WAIT_SLOW_NS: u64 = 50_000;
const LOCK_WAIT_SPIKE_NS: u64 = 2_000_000;

pub struct LockWaitStats {
    lock_count: AtomicU64,
    wait_ns_total: AtomicU64,
    wait_ns_max: AtomicU64,
    slow_wait_count: AtomicU64,
    last_report_ns: AtomicU64,
}

impl Default for LockWaitStats {
    fn default() -> Self {
        Self::new()
    }
}

impl LockWaitStats {
    pub const fn new() -> Self {
        Self {
            lock_count: AtomicU64::new(0),
            wait_ns_total: AtomicU64::new(0),
            wait_ns_max: AtomicU64::new(0),
            slow_wait_count: AtomicU64::new(0),
            last_report_ns: AtomicU64::new(0),
        }
    }

    fn record(&self, lock_name: &str, waited_ns: u64) {
        self.lock_count.fetch_add(1, Ordering::Relaxed);
        self.wait_ns_total.fetch_add(waited_ns, Ordering::Relaxed);
        self.wait_ns_max.fetch_max(waited_ns, Ordering::Relaxed);
        if waited_ns >= LOCK_WAIT_SLOW_NS {
            self.slow_wait_count.fetch_add(1, Ordering::Relaxed);
        }
        if waited_ns >= LOCK_WAIT_SPIKE_NS {
            debug!(
                "lock-wait[{lock_name}] spike={:.3}ms",
                waited_ns as f64 / 1_000_000.0
            );
        }

        let now_ns = lock_wait_now_ns();
        let last_ns = self.last_report_ns.load(Ordering::Relaxed);
        if now_ns.saturating_sub(last_ns) < LOCK_WAIT_REPORT_INTERVAL_NS {
            return;
        }
        if self
            .last_report_ns
            .compare_exchange(last_ns, now_ns, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        let lock_count = self.lock_count.swap(0, Ordering::Relaxed);
        if lock_count == 0 {
            return;
        }
        let total_ns = self.wait_ns_total.swap(0, Ordering::Relaxed);
        let max_ns = self.wait_ns_max.swap(0, Ordering::Relaxed);
        let slow_count = self.slow_wait_count.swap(0, Ordering::Relaxed);
        let avg_us = (total_ns as f64 / lock_count as f64) / 1_000.0;
        debug!(
            "lock-wait[{lock_name}] n={} avg={avg_us:.3}us max={:.3}us slow(>50us)={}",
            lock_count,
            max_ns as f64 / 1_000.0,
            slow_count
        );
    }
}

#[inline(always)]
pub fn lock_mutex<'a, T>(
    lock_name: &str,
    mutex: &'a Mutex<T>,
    stats: &LockWaitStats,
) -> MutexGuard<'a, T> {
    if log::max_level() < log::LevelFilter::Debug {
        return mutex.lock().unwrap();
    }
    let start = Instant::now();
    let guard = mutex.lock().unwrap();
    let waited_ns = start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    stats.record(lock_name, waited_ns);
    guard
}

#[inline(always)]
fn lock_wait_now_ns() -> u64 {
    LOCK_WAIT_EPOCH.elapsed().as_nanos().min(u64::MAX as u128) as u64
}
