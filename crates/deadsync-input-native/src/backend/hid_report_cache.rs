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

#[cfg(test)]
mod tests {
    use super::*;

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
}
