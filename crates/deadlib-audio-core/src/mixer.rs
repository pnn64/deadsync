use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Upper bound (in device frames) on how far ahead of the audible write head a
/// scheduled SFX onset may sit before the mixer treats it as stale and drops it.
/// This is a last-resort sanity bound; seek/stop/track-change staleness is
/// handled by the caller's generation guard.
pub const MAX_SCHEDULE_AHEAD_FRAMES: u64 = 192_000;
pub const MAX_ACTIVE_SFX: usize = 32;
pub const MAX_MIX_BUSES: usize = 8;

/// Stable index into the session's fixed mixer-bus table.
///
/// Bus meaning is deliberately supplied by the owning application. The mixer
/// only applies the bus gain and generation associated with this index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MixBus(u8);

impl MixBus {
    pub const fn new(index: u8) -> Self {
        assert!(
            (index as usize) < MAX_MIX_BUSES,
            "mixer bus index out of range"
        );
        Self(index)
    }

    #[inline(always)]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Session-owned controls shared by producers and the realtime mixer.
///
/// The application thread updates atomic gains and generations. The audio
/// callback only performs bounded atomic loads; it never locks or allocates.
pub struct MixControls {
    stream_gain: AtomicU32,
    bus_gains: [AtomicU32; MAX_MIX_BUSES],
    bus_generations: [AtomicU64; MAX_MIX_BUSES],
}

impl MixControls {
    pub fn new() -> Self {
        Self {
            stream_gain: AtomicU32::new(1.0f32.to_bits()),
            bus_gains: std::array::from_fn(|_| AtomicU32::new(1.0f32.to_bits())),
            bus_generations: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    #[inline(always)]
    pub fn set_stream_gain(&self, gain: f32) {
        self.stream_gain
            .store(valid_gain(gain).to_bits(), Ordering::Release);
    }

    #[inline(always)]
    pub fn stream_gain(&self) -> f32 {
        f32::from_bits(self.stream_gain.load(Ordering::Acquire))
    }

    #[inline(always)]
    pub fn set_bus_gain(&self, bus: MixBus, gain: f32) {
        self.bus_gains[bus.index()].store(valid_gain(gain).to_bits(), Ordering::Release);
    }

    #[inline(always)]
    pub fn bus_gain(&self, bus: MixBus) -> f32 {
        f32::from_bits(self.bus_gains[bus.index()].load(Ordering::Acquire))
    }

    #[inline(always)]
    pub fn bus_generation(&self, bus: MixBus) -> u64 {
        self.bus_generations[bus.index()].load(Ordering::Acquire)
    }

    #[inline(always)]
    pub fn stop_bus(&self, bus: MixBus) -> u64 {
        self.bus_generations[bus.index()]
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
    }

    #[inline(always)]
    pub fn is_current(&self, bus: MixBus, generation: u64) -> bool {
        generation == self.bus_generation(bus)
    }
}

impl Default for MixControls {
    fn default() -> Self {
        Self::new()
    }
}

#[inline(always)]
fn valid_gain(gain: f32) -> f32 {
    if gain.is_finite() && gain >= 0.0 {
        gain
    } else {
        1.0
    }
}

#[derive(Clone)]
pub struct QueuedSfx {
    pub data: Arc<[i16]>,
    pub bus: MixBus,
    pub generation: u64,
    /// Absolute stream frame at which the first sample should become audible.
    /// `0` means "play immediately" at the start of the next buffer.
    pub target_stream_frame: u64,
}

/// Active SFX state retained across output callbacks.
pub struct ActiveSfx {
    pub data: Arc<[i16]>,
    pub cursor: usize,
    pub bus: MixBus,
    pub generation: u64,
    pub target_stream_frame: u64,
}

impl ActiveSfx {
    #[inline(always)]
    pub fn from_queued(queued: QueuedSfx) -> Self {
        Self {
            data: queued.data,
            cursor: 0,
            bus: queued.bus,
            generation: queued.generation,
            target_stream_frame: queued.target_stream_frame,
        }
    }
}

#[inline(always)]
pub fn push_queued_sfx(active: &mut Vec<ActiveSfx>, queued: QueuedSfx, controls: &MixControls) {
    if controls.is_current(queued.bus, queued.generation) && active.len() < MAX_ACTIVE_SFX {
        active.push(ActiveSfx::from_queued(queued));
    }
}

pub fn mix_active_sfx(
    active: &mut Vec<ActiveSfx>,
    mix_f32: &mut [f32],
    total_before: u64,
    device_channels: usize,
    controls: &MixControls,
) -> bool {
    let buf_len = mix_f32.len();
    let mut mixed_sfx = false;
    active.retain_mut(|sfx| {
        if !controls.is_current(sfx.bus, sfx.generation) {
            return false;
        }
        let start_sample = match scheduled_onset_decision(
            sfx.target_stream_frame,
            total_before,
            device_channels,
            buf_len,
        ) {
            ScheduledOnset::Drop => return false,
            ScheduledOnset::Pending => return true,
            ScheduledOnset::StartAt(offset) => offset,
        };
        sfx.target_stream_frame = 0;
        let n = (sfx.data.len().saturating_sub(sfx.cursor)).min(buf_len - start_sample);
        mixed_sfx |= n > 0;
        let bus_gain = controls.bus_gain(sfx.bus);
        for i in 0..n {
            let sfx_sample_f32 = i16_to_f32(sfx.data[sfx.cursor + i]) * bus_gain;
            mix_f32[start_sample + i] += sfx_sample_f32;
        }
        sfx.cursor += n;
        sfx.cursor < sfx.data.len()
    });
    mixed_sfx
}

/// Where a scheduled SFX onset lands relative to the buffer currently being
/// filled. See [`scheduled_onset_decision`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledOnset {
    /// Target frame is implausibly far ahead; drop the entry.
    Drop,
    /// Onset falls in a later buffer; keep pending without mixing.
    Pending,
    /// Mix starting at this sample offset within the current buffer.
    StartAt(usize),
}

/// Decides where a scheduled SFX onset lands within the buffer the mixer is
/// currently filling. `target_stream_frame == 0` means "play immediately".
/// `total_before` is the absolute write-head frame at the start of this buffer;
/// `buf_len` is the buffer length in interleaved samples.
#[inline(always)]
pub fn scheduled_onset_decision(
    target_stream_frame: u64,
    total_before: u64,
    device_channels: usize,
    buf_len: usize,
) -> ScheduledOnset {
    if target_stream_frame == 0 {
        return ScheduledOnset::StartAt(0);
    }
    let frames_until = target_stream_frame.saturating_sub(total_before);
    if frames_until > MAX_SCHEDULE_AHEAD_FRAMES {
        return ScheduledOnset::Drop;
    }
    let offset = (frames_until as usize) * device_channels;
    if offset >= buf_len {
        return ScheduledOnset::Pending;
    }
    ScheduledOnset::StartAt(offset)
}

#[inline(always)]
pub fn f32_to_i16(sample: f32) -> i16 {
    // Rust float-to-integer casts saturate and map NaN to zero, which exactly
    // covers the old clamp and boundary branches after scaling.
    (sample * (i16::MAX as f32 + 1.0)) as i16
}

#[inline(always)]
pub fn i16_to_f32(sample: i16) -> f32 {
    sample as f32 / (i16::MAX as f32 + 1.0)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_SCHEDULE_AHEAD_FRAMES, MixBus, MixControls, ScheduledOnset, f32_to_i16, i16_to_f32,
        scheduled_onset_decision,
    };

    #[test]
    fn scheduled_onset_immediate_when_target_zero() {
        assert_eq!(
            scheduled_onset_decision(0, 10_000, 2, 1_024),
            ScheduledOnset::StartAt(0)
        );
    }

    #[test]
    fn scheduled_onset_starts_at_offset_within_buffer() {
        assert_eq!(
            scheduled_onset_decision(10_100, 10_000, 2, 1_024),
            ScheduledOnset::StartAt(200)
        );
    }

    #[test]
    fn scheduled_onset_pending_when_beyond_buffer() {
        assert_eq!(
            scheduled_onset_decision(10_600, 10_000, 2, 1_024),
            ScheduledOnset::Pending
        );
    }

    #[test]
    fn scheduled_onset_drops_when_implausibly_far_ahead() {
        assert_eq!(
            scheduled_onset_decision(MAX_SCHEDULE_AHEAD_FRAMES + 10_001, 10_000, 2, 1_024),
            ScheduledOnset::Drop
        );
    }

    #[test]
    fn scheduled_onset_fires_when_target_already_passed() {
        assert_eq!(
            scheduled_onset_decision(9_000, 10_000, 2, 1_024),
            ScheduledOnset::StartAt(0)
        );
    }

    #[test]
    fn f32_to_i16_clamps_full_scale() {
        assert_eq!(f32_to_i16(2.0), i16::MAX);
        assert_eq!(f32_to_i16(1.0), i16::MAX);
        assert_eq!(f32_to_i16(-1.0), i16::MIN);
        assert_eq!(f32_to_i16(-2.0), i16::MIN);
    }

    #[test]
    fn f32_to_i16_maps_midpoint_samples() {
        assert_eq!(f32_to_i16(0.0), 0);
        assert_eq!(f32_to_i16(0.5), 16_384);
        assert_eq!(f32_to_i16(-0.5), -16_384);
    }

    #[test]
    fn saturating_conversion_matches_legacy_float_domain() {
        fn legacy(sample: f32) -> i16 {
            let sample = sample.clamp(-1.0, 1.0);
            if sample >= 1.0 {
                i16::MAX
            } else if sample <= -1.0 {
                i16::MIN
            } else {
                (sample * 32_768.0) as i16
            }
        }

        let edges = [
            f32::NEG_INFINITY,
            -1.000_000_1,
            -1.0,
            -0.999_999_94,
            -0.0,
            0.0,
            0.999_999_94,
            1.0,
            1.000_000_1,
            f32::INFINITY,
            f32::NAN,
        ];
        for sample in edges {
            assert_eq!(f32_to_i16(sample), legacy(sample), "sample={sample:?}");
        }
        for exponent in 0..=u8::MAX {
            for mantissa in (0..=0x7f_ffffu32).step_by(65_521) {
                for sign in [0, 1u32 << 31] {
                    let sample = f32::from_bits(sign | u32::from(exponent) << 23 | mantissa);
                    assert_eq!(
                        f32_to_i16(sample),
                        legacy(sample),
                        "bits={:08x}",
                        sample.to_bits()
                    );
                }
            }
        }
    }

    #[test]
    fn i16_to_f32_maps_full_range() {
        assert_eq!(i16_to_f32(i16::MIN), -1.0);
        assert_eq!(i16_to_f32(0), 0.0);
        assert!((i16_to_f32(i16::MAX) - 0.999_969_5).abs() <= f32::EPSILON);
    }

    #[test]
    fn mix_controls_keep_bus_policy_outside_the_mixer() {
        let controls = MixControls::new();
        let bus = MixBus::new(3);
        controls.set_stream_gain(0.75);
        controls.set_bus_gain(bus, 0.25);
        assert_eq!(controls.stream_gain(), 0.75);
        assert_eq!(controls.bus_gain(bus), 0.25);
    }

    #[test]
    fn stopping_one_bus_only_invalidates_that_bus() {
        let controls = MixControls::new();
        let first = MixBus::new(1);
        let second = MixBus::new(2);
        let first_generation = controls.bus_generation(first);
        let second_generation = controls.bus_generation(second);
        controls.stop_bus(first);
        assert!(!controls.is_current(first, first_generation));
        assert!(controls.is_current(second, second_generation));
    }
}
