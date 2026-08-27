#[cfg(any(target_os = "freebsd", test))]
use super::BackendHost;
#[cfg(any(target_os = "freebsd", test))]
use std::time::Instant;

#[cfg(any(target_os = "freebsd", test))]
const MIN_REPORT_BUFFER_LEN: usize = 64;
const NO_REPORT: u16 = u16::MAX;

#[cfg(any(target_os = "freebsd", test))]
pub(super) fn required_report_buffer_len(report_lens: impl IntoIterator<Item = usize>) -> usize {
    report_lens
        .into_iter()
        .max()
        .unwrap_or(0)
        .max(MIN_REPORT_BUFFER_LEN)
}

pub struct HidReportRoute {
    by_id: [u16; 256],
    unnumbered: u16,
    direct: bool,
}

impl HidReportRoute {
    pub fn new(report_ids: impl IntoIterator<Item = Option<u8>>) -> Self {
        let mut route = Self {
            by_id: [NO_REPORT; 256],
            unnumbered: NO_REPORT,
            direct: true,
        };
        let mut numbered = false;
        let mut report_count = 0usize;
        for (index, report_id) in report_ids.into_iter().enumerate() {
            report_count += 1;
            if index >= NO_REPORT as usize {
                route.direct = false;
                continue;
            }
            let index = index as u16;
            match report_id {
                Some(report_id) => {
                    numbered = true;
                    let slot = &mut route.by_id[report_id as usize];
                    if *slot == NO_REPORT {
                        *slot = index;
                    } else {
                        route.direct = false;
                    }
                }
                None if route.unnumbered == NO_REPORT => route.unnumbered = index,
                None => route.direct = false,
            }
        }
        if numbered && route.unnumbered != NO_REPORT {
            route.direct = false;
        }
        // The ordered scan is cheaper for one to three numbered reports; the
        // fixed lookup starts winning once failed ID checks become meaningful.
        if numbered && report_count < 4 {
            route.direct = false;
        }
        route
    }

    #[inline(always)]
    #[must_use]
    pub const fn needs_fallback(&self) -> bool {
        !self.direct
    }

    #[inline(always)]
    #[must_use]
    pub fn direct_index(&self, first_byte: Option<u8>) -> Option<usize> {
        debug_assert!(self.direct);
        if self.unnumbered != NO_REPORT {
            return Some(self.unnumbered as usize);
        }
        let report_id = first_byte?;
        let index = self.by_id[report_id as usize];
        (index != NO_REPORT).then_some(index as usize)
    }
}

#[cfg(any(target_os = "freebsd", test))]
pub(super) struct HidReportCache {
    bytes: Box<[u8]>,
    valid: bool,
}

#[cfg(any(target_os = "freebsd", test))]
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

#[cfg(any(target_os = "freebsd", test))]
pub(super) struct HidReportTime {
    sampled: Option<(Instant, u64)>,
}

#[cfg(any(target_os = "freebsd", test))]
impl HidReportTime {
    #[inline(always)]
    pub(super) const fn new() -> Self {
        Self { sampled: None }
    }

    #[inline(always)]
    pub(super) fn sample(&mut self, host: BackendHost) -> (Instant, u64) {
        *self.sampled.get_or_insert_with(|| host.sample_time())
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
    fn report_buffer_covers_the_largest_validated_report() {
        assert_eq!(required_report_buffer_len(std::iter::empty()), 64);
        assert_eq!(required_report_buffer_len([8, 32, 16]), 64);
        assert_eq!(required_report_buffer_len([32, 256, 128]), 256);
    }

    #[test]
    fn report_routes_numbered_and_single_unnumbered_reports_directly() {
        let numbered = HidReportRoute::new([Some(7), Some(2), Some(255), Some(1)]);
        assert!(!numbered.needs_fallback());
        assert_eq!(numbered.direct_index(Some(7)), Some(0));
        assert_eq!(numbered.direct_index(Some(2)), Some(1));
        assert_eq!(numbered.direct_index(Some(255)), Some(2));
        assert_eq!(numbered.direct_index(Some(1)), Some(3));
        assert_eq!(numbered.direct_index(Some(3)), None);
        assert_eq!(numbered.direct_index(None), None);

        let unnumbered = HidReportRoute::new([None]);
        assert!(!unnumbered.needs_fallback());
        assert_eq!(unnumbered.direct_index(Some(99)), Some(0));
        assert_eq!(unnumbered.direct_index(None), Some(0));
    }

    #[test]
    fn ambiguous_report_routes_request_the_ordered_fallback() {
        assert!(HidReportRoute::new([Some(1), Some(2), Some(3)]).needs_fallback());
        assert!(HidReportRoute::new([Some(1), Some(1)]).needs_fallback());
        assert!(HidReportRoute::new([Some(1), None]).needs_fallback());
        assert!(HidReportRoute::new([None, None]).needs_fallback());
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
            7
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
