use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Upper bound (in device frames) on how far ahead of the audible write head a
/// scheduled SFX onset may sit before the mixer treats it as stale and drops it.
/// This is a last-resort sanity bound; seek/stop/track-change staleness is
/// handled by the caller's generation guard.
pub const MAX_SCHEDULE_AHEAD_FRAMES: u64 = 192_000;
pub const MAX_ACTIVE_SFX: usize = 32;

#[derive(Clone, Copy, Debug)]
pub enum SfxLane {
    Effect,
    Screen,
    AssistTick,
}

static SCREEN_SFX_STOP_GEN: AtomicU64 = AtomicU64::new(0);
static ASSIST_SFX_GEN: AtomicU64 = AtomicU64::new(0);

#[inline(always)]
pub fn bump_screen_sfx_generation() {
    SCREEN_SFX_STOP_GEN.fetch_add(1, Ordering::AcqRel);
}

#[inline(always)]
pub fn bump_assist_sfx_generation() {
    ASSIST_SFX_GEN.fetch_add(1, Ordering::AcqRel);
}

#[inline(always)]
pub fn assist_sfx_generation() -> u64 {
    ASSIST_SFX_GEN.load(Ordering::Acquire)
}

#[inline(always)]
pub fn sfx_stop_generation(lane: SfxLane) -> u64 {
    match lane {
        SfxLane::Screen => SCREEN_SFX_STOP_GEN.load(Ordering::Acquire),
        SfxLane::AssistTick => ASSIST_SFX_GEN.load(Ordering::Acquire),
        SfxLane::Effect => 0,
    }
}

#[inline(always)]
pub fn sfx_is_stale(lane: SfxLane, stop_generation: u64) -> bool {
    match lane {
        SfxLane::Screen => stop_generation != SCREEN_SFX_STOP_GEN.load(Ordering::Acquire),
        SfxLane::AssistTick => stop_generation != ASSIST_SFX_GEN.load(Ordering::Acquire),
        SfxLane::Effect => false,
    }
}

#[derive(Clone)]
pub struct QueuedSfx {
    pub data: Arc<[i16]>,
    pub lane: SfxLane,
    pub stop_generation: u64,
    /// Absolute stream frame at which the first sample should become audible.
    /// `0` means "play immediately" at the start of the next buffer.
    pub target_stream_frame: u64,
}

/// Active SFX state retained across output callbacks.
pub struct ActiveSfx {
    pub data: Arc<[i16]>,
    pub cursor: usize,
    pub lane: SfxLane,
    pub stop_generation: u64,
    pub target_stream_frame: u64,
}

impl ActiveSfx {
    #[inline(always)]
    pub fn from_queued(queued: QueuedSfx) -> Self {
        Self {
            data: queued.data,
            cursor: 0,
            lane: queued.lane,
            stop_generation: queued.stop_generation,
            target_stream_frame: queued.target_stream_frame,
        }
    }
}

#[inline(always)]
pub fn push_queued_sfx(
    active: &mut Vec<ActiveSfx>,
    queued: QueuedSfx,
    is_stale: impl Fn(SfxLane, u64) -> bool,
) {
    if !is_stale(queued.lane, queued.stop_generation) && active.len() < MAX_ACTIVE_SFX {
        active.push(ActiveSfx::from_queued(queued));
    }
}

pub fn mix_active_sfx(
    active: &mut Vec<ActiveSfx>,
    mix_f32: &mut [f32],
    total_before: u64,
    device_channels: usize,
    sfx_vol: f32,
    assist_tick_vol: f32,
    is_stale: impl Fn(SfxLane, u64) -> bool,
) -> bool {
    let buf_len = mix_f32.len();
    let mut mixed_sfx = false;
    active.retain_mut(|sfx| {
        if is_stale(sfx.lane, sfx.stop_generation) {
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
        let lane_vol = match sfx.lane {
            SfxLane::Effect | SfxLane::Screen => sfx_vol,
            SfxLane::AssistTick => assist_tick_vol,
        };
        for i in 0..n {
            let sfx_sample_f32 = i16_to_f32(sfx.data[sfx.cursor + i]) * lane_vol;
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
    let sample = sample.clamp(-1.0, 1.0);
    if sample >= 1.0 {
        i16::MAX
    } else if sample <= -1.0 {
        i16::MIN
    } else {
        (sample * (i16::MAX as f32 + 1.0)) as i16
    }
}

#[inline(always)]
pub fn i16_to_f32(sample: i16) -> f32 {
    sample as f32 / (i16::MAX as f32 + 1.0)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_SCHEDULE_AHEAD_FRAMES, ScheduledOnset, f32_to_i16, i16_to_f32, scheduled_onset_decision,
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
    fn i16_to_f32_maps_full_range() {
        assert_eq!(i16_to_f32(i16::MIN), -1.0);
        assert_eq!(i16_to_f32(0), 0.0);
        assert!((i16_to_f32(i16::MAX) - 0.999_969_5).abs() <= f32::EPSILON);
    }
}
