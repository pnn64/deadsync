use super::BackendHost;
use std::time::Instant;

pub(super) struct HidReportCache {
    bytes: Box<[u8]>,
    valid: bool,
}

impl HidReportCache {
    #[inline(always)]
    pub(super) fn new(report_len: usize) -> Self {
        Self {
            bytes: vec![0; report_len].into_boxed_slice(),
            valid: false,
        }
    }

    #[inline(always)]
    pub(super) fn is_duplicate(&self, report: &[u8]) -> bool {
        self.valid && self.bytes.as_ref() == report
    }

    #[inline(always)]
    pub(super) fn remember(&mut self, report: &[u8]) {
        if self.bytes.len() != report.len() {
            self.valid = false;
            return;
        }
        self.bytes.copy_from_slice(report);
        self.valid = true;
    }
}

pub(super) struct HidReportTime {
    sampled: Option<(Instant, u64)>,
}

impl HidReportTime {
    #[inline(always)]
    pub(super) const fn new() -> Self {
        Self { sampled: None }
    }

    #[inline(always)]
    pub(super) fn sample(&mut self, host: BackendHost) -> (Instant, u64) {
        *self
            .sampled
            .get_or_insert_with(|| (Instant::now(), host.now_nanos()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn cache_skips_only_a_remembered_exact_report() {
        let mut cache = HidReportCache::new(3);

        assert!(!cache.is_duplicate(&[1, 2, 3]));
        cache.remember(&[1, 2, 3]);
        assert!(cache.is_duplicate(&[1, 2, 3]));
        assert!(!cache.is_duplicate(&[1, 2, 4]));

        cache.remember(&[1, 2, 4]);
        assert!(cache.is_duplicate(&[1, 2, 4]));
        assert!(!cache.is_duplicate(&[1, 2, 3]));
    }

    #[test]
    fn cache_reuses_fixed_storage_and_invalidates_wrong_lengths() {
        let mut cache = HidReportCache::new(3);
        let storage = cache.bytes.as_ptr();

        cache.remember(&[1, 2, 3]);
        cache.remember(&[4, 5, 6]);
        assert_eq!(cache.bytes.as_ptr(), storage);
        assert!(cache.is_duplicate(&[4, 5, 6]));

        cache.remember(&[4, 5]);
        assert!(!cache.is_duplicate(&[4, 5, 6]));
        cache.remember(&[7, 8, 9]);
        assert_eq!(cache.bytes.as_ptr(), storage);
        assert!(cache.is_duplicate(&[7, 8, 9]));
    }

    #[test]
    fn report_time_samples_lazily_and_only_once() {
        static CLOCK_CALLS: AtomicUsize = AtomicUsize::new(0);

        fn pad_idx(_: super::super::PadOrderBackend, _: [u8; 16]) -> u32 {
            0
        }
        fn smx_owns(_: Option<u16>, _: Option<u16>) -> bool {
            false
        }
        fn now() -> u64 {
            CLOCK_CALLS.fetch_add(1, Ordering::Relaxed);
            42
        }
        fn instant_nanos(_: Instant) -> u64 {
            0
        }
        fn qpc(_: u64) -> Option<u64> {
            None
        }
        fn boost() -> super::super::InputThreadPolicy {
            super::super::InputThreadPolicy::none()
        }

        CLOCK_CALLS.store(0, Ordering::Relaxed);
        let host = BackendHost::new(pad_idx, smx_owns, now, instant_nanos, qpc, boost);
        let mut time = HidReportTime::new();
        assert_eq!(CLOCK_CALLS.load(Ordering::Relaxed), 0);

        let first = time.sample(host);
        let second = time.sample(host);
        assert_eq!(first, second);
        assert_eq!(first.1, 42);
        assert_eq!(CLOCK_CALLS.load(Ordering::Relaxed), 1);
    }
}
