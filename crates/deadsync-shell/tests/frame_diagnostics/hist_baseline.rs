// Frozen from 09ab0b740 (0.5.1213).
use super::current::DHIST_BINS;
#[derive(Clone, Copy)]
pub struct DecayingHist {
    bins: [f32; DHIST_BINS],
    total: f32,
}

impl DecayingHist {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            bins: [0.0; DHIST_BINS],
            total: 0.0,
        }
    }

    #[inline(always)]
    pub const fn reset(&mut self) {
        self.bins = [0.0; DHIST_BINS];
        self.total = 0.0;
    }

    /// Decay all bins and add the new `value_us` to its bucket.
    #[inline]
    pub fn update(&mut self, value_us: u32, gamma: f32, bucket_us: u32) {
        let bucket_us = bucket_us.max(1);
        let idx = (value_us / bucket_us).min(DHIST_BINS as u32 - 1) as usize;
        for bin in &mut self.bins {
            *bin *= gamma;
        }
        self.bins[idx] += 1.0;
        self.total = self.total.mul_add(gamma, 1.0);
    }

    /// Weighted percentile in microseconds (bucket-quantized). Returns 0 until warmed up.
    #[inline]
    pub fn percentile_us(&self, pct: f32, bucket_us: u32) -> u32 {
        if self.total <= 0.0 {
            return 0;
        }
        let bucket_us = bucket_us.max(1);
        let target = (self.total * pct.clamp(0.0, 1.0)).max(f32::MIN_POSITIVE);
        let mut cumulative = 0.0;
        for (idx, &bin) in self.bins.iter().enumerate() {
            cumulative += bin;
            if cumulative >= target {
                return (idx as u32 + 1) * bucket_us;
            }
        }
        DHIST_BINS as u32 * bucket_us
    }

    /// Effective sample count represented by the decaying histogram.
    #[inline(always)]
    #[cfg(test)]
    pub const fn effective_n(&self) -> u32 {
        self.total.round().max(0.0) as u32
    }
}

impl Default for DecayingHist {
    fn default() -> Self {
        Self::new()
    }
}

// Test-only inspection of the frozen state.
pub(super) fn state(hist: &DecayingHist) -> ([f32; DHIST_BINS], f32) {
    (hist.bins, hist.total)
}
