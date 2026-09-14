// Frozen from 09ab0b740 (0.5.1213).
#[derive(Clone, Copy)]
pub struct FixedFrameStatsRing<T: Copy, const N: usize> {
    samples: [T; N],
    empty: T,
    cursor: usize,
    len: usize,
}

impl<T: Copy, const N: usize> FixedFrameStatsRing<T, N> {
    #[inline(always)]
    pub const fn new(empty: T) -> Self {
        Self {
            samples: [empty; N],
            empty,
            cursor: 0,
            len: 0,
        }
    }

    #[inline(always)]
    pub const fn clear(&mut self) {
        self.samples = [self.empty; N];
        self.cursor = 0;
        self.len = 0;
    }

    #[inline(always)]
    pub fn push(&mut self, sample: T) {
        if N == 0 {
            return;
        }
        self.samples[self.cursor] = sample;
        self.cursor = (self.cursor + 1) % N;
        self.len = self.len.saturating_add(1).min(N);
    }

    /// Copy the ring into `out` in chronological order.
    pub fn snapshot(&self, out: &mut Vec<T>) {
        out.clear();
        if N == 0 {
            return;
        }
        let start = self.cursor.saturating_add(N).saturating_sub(self.len) % N;
        for i in 0..self.len {
            out.push(self.samples[(start + i) % N]);
        }
    }

    /// Copy samples whose timestamp is inside `window_ns`, in chronological order.
    pub fn collect_recent_by(
        &self,
        now_host_nanos: u64,
        window_ns: u64,
        out: &mut Vec<T>,
        timestamp: impl Fn(T) -> u64,
    ) {
        out.clear();
        if N == 0 {
            return;
        }
        let start = self.cursor.saturating_add(N).saturating_sub(self.len) % N;
        for i in 0..self.len {
            let sample = self.samples[(start + i) % N];
            let sample_time = timestamp(sample);
            if sample_time != 0 && now_host_nanos.saturating_sub(sample_time) <= window_ns {
                out.push(sample);
            }
        }
    }
}
