//! Frozen production reference from bab7ef065.
pub const STUTTER_SAMPLE_COUNT: usize = 5;
pub const STUTTER_SAMPLE_LIFETIME: f32 = 3.4;

#[derive(Clone, Copy, Debug)]
pub struct StutterSample {
    pub at_seconds: f32,
    pub frame_seconds: f32,
    pub expected_seconds: f32,
    pub severity: u8,
}

impl StutterSample {
    #[inline(always)]
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            at_seconds: -1.0,
            frame_seconds: 0.0,
            expected_seconds: 0.0,
            severity: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisibleStutterSample {
    pub timestamp_seconds: f32,
    pub frame_ms: f32,
    pub frame_multiple: f32,
    pub severity: u8,
    pub age_seconds: f32,
}

const EMPTY_VISIBLE_STUTTER: VisibleStutterSample = VisibleStutterSample {
    timestamp_seconds: 0.0,
    frame_ms: 0.0,
    frame_multiple: 0.0,
    severity: 0,
    age_seconds: 0.0,
};

/// Fixed-capacity visible view of the five-entry stutter ring.
#[derive(Clone, Copy, Debug)]
pub struct VisibleStutterSamples {
    samples: [VisibleStutterSample; STUTTER_SAMPLE_COUNT],
    len: u8,
}

impl VisibleStutterSamples {
    #[inline(always)]
    const fn new() -> Self {
        Self {
            samples: [EMPTY_VISIBLE_STUTTER; STUTTER_SAMPLE_COUNT],
            len: 0,
        }
    }

    #[inline(always)]
    fn push(&mut self, sample: VisibleStutterSample) {
        debug_assert!((self.len as usize) < STUTTER_SAMPLE_COUNT);
        self.samples[self.len as usize] = sample;
        self.len += 1;
    }
}

impl std::ops::Deref for VisibleStutterSamples {
    type Target = [VisibleStutterSample];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.samples[..self.len as usize]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StutterSampleRing {
    samples: [StutterSample; STUTTER_SAMPLE_COUNT],
    cursor: usize,
}

impl Default for StutterSampleRing {
    fn default() -> Self {
        Self::new()
    }
}

impl StutterSampleRing {
    #[inline(always)]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            samples: [StutterSample::empty(); STUTTER_SAMPLE_COUNT],
            cursor: 0,
        }
    }

    #[inline(always)]
    pub const fn clear(&mut self) {
        *self = Self::new();
    }

    #[inline(always)]
    pub const fn push(
        &mut self,
        at_seconds: f32,
        frame_seconds: f32,
        expected_seconds: f32,
        severity: u8,
    ) {
        self.samples[self.cursor] = StutterSample {
            at_seconds,
            frame_seconds,
            expected_seconds,
            severity,
        };
        self.cursor = (self.cursor + 1) % STUTTER_SAMPLE_COUNT;
    }

    #[must_use]
    pub fn visible(self, now_seconds: f32) -> VisibleStutterSamples {
        let mut out = VisibleStutterSamples::new();
        for i in 0..STUTTER_SAMPLE_COUNT {
            let sample = self.samples[(self.cursor + i) % STUTTER_SAMPLE_COUNT];
            if sample.severity == 0 {
                continue;
            }
            let age_seconds = now_seconds - sample.at_seconds;
            if !(0.0..=STUTTER_SAMPLE_LIFETIME).contains(&age_seconds) {
                continue;
            }
            let frame_multiple = if sample.expected_seconds > 0.0 {
                sample.frame_seconds / sample.expected_seconds
            } else {
                0.0
            };
            out.push(VisibleStutterSample {
                timestamp_seconds: sample.at_seconds,
                frame_ms: sample.frame_seconds * 1000.0,
                frame_multiple,
                severity: sample.severity,
                age_seconds,
            });
        }
        out
    }
}

/// Fixed-size copy ring for frame diagnostic samples.
///
/// The app owns when samples are produced; this type owns the storage policy:
/// no heap allocation, overwrite oldest on overflow, snapshot oldest-first.
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
        if N == 0 || self.len == 0 {
            return;
        }
        let start = self.cursor.saturating_add(N).saturating_sub(self.len) % N;
        let first_len = self.len.min(N - start);
        out.reserve(self.len);
        out.extend_from_slice(&self.samples[start..start + first_len]);
        out.extend_from_slice(&self.samples[..self.len - first_len]);
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
