use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

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
    #[must_use]
    /// # Panics
    ///
    /// Panics if the function's input or state invariants are violated.
    pub const fn new(index: u8) -> Self {
        assert!(
            (index as usize) < MAX_MIX_BUSES,
            "mixer bus index out of range"
        );
        Self(index)
    }

    #[inline(always)]
    #[must_use]
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
    #[must_use]
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

/// Stereo balance for a sound. ITGmania pans mine sounds to the owning
/// player when both sides are active; ordinary effects remain centered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SfxPan {
    #[default]
    Center,
    Left,
    Right,
}

#[inline(always)]
fn valid_gain(gain: f32) -> f32 {
    if gain.is_finite() && gain >= 0.0 {
        gain
    } else {
        1.0
    }
}

/// Optional per-voice control, independent of the sound file and mixer bus.
/// Two actors loading the same file must still be stoppable independently.
pub struct SfxVoiceControl {
    playing: AtomicBool,
    paused: AtomicBool,
}

impl SfxVoiceControl {
    pub fn new() -> Self {
        Self { playing: AtomicBool::new(true), paused: AtomicBool::new(false) }
    }
    pub fn is_playing(&self) -> bool { self.playing.load(Ordering::Acquire) }
    pub fn stop(&self) { self.playing.store(false, Ordering::Release); }
    pub fn pause(&self, paused: bool) { self.paused.store(paused, Ordering::Release); }
}

impl Default for SfxVoiceControl {
    fn default() -> Self { Self::new() }
}

#[derive(Clone)]
pub struct QueuedSfx {
    pub data: Arc<[i16]>,
    pub bus: MixBus,
    pub generation: u64,
    /// A logical sound instance to stop and restart on another trigger.
    /// Slots are scoped to a bus; `None` preserves overlapping one-shots.
    pub restart_slot: Option<u8>,
    pub pan: SfxPan,
    /// Absolute stream frame at which the first sample should become audible.
    /// `0` means "play immediately" at the start of the next buffer.
    pub target_stream_frame: u64,
    pub voice: Option<Arc<SfxVoiceControl>>,
}

/// Active SFX state retained across output callbacks.
pub struct ActiveSfx {
    pub data: Arc<[i16]>,
    pub cursor: usize,
    pub bus: MixBus,
    pub generation: u64,
    pub restart_slot: Option<u8>,
    pub pan: SfxPan,
    pub target_stream_frame: u64,
    pub voice: Option<Arc<SfxVoiceControl>>,
}

impl ActiveSfx {
    #[inline(always)]
    #[must_use]
    pub fn from_queued(queued: QueuedSfx) -> Self {
        Self {
            data: queued.data,
            cursor: 0,
            bus: queued.bus,
            generation: queued.generation,
            restart_slot: queued.restart_slot,
            pan: queued.pan,
            target_stream_frame: queued.target_stream_frame,
            voice: queued.voice,
        }
    }
}

impl Drop for ActiveSfx {
    fn drop(&mut self) {
        if let Some(voice) = &self.voice { voice.stop(); }
    }
}

#[inline(always)]
pub fn push_queued_sfx(active: &mut Vec<ActiveSfx>, queued: QueuedSfx, controls: &MixControls) {
    if !controls.is_current(queued.bus, queued.generation) {
        return;
    }
    if let Some(slot) = queued.restart_slot
        && let Some(previous) = active.iter_mut().find(|sfx| {
            sfx.bus == queued.bus
                && sfx.generation == queued.generation
                && sfx.restart_slot == Some(slot)
        })
    {
        *previous = ActiveSfx::from_queued(queued);
        return;
    }
    // A bus generation reset can leave stale voices occupying the fixed
    // capacity until the next callback. Make room for current-generation SFX.
    if active.len() == MAX_ACTIVE_SFX {
        active.retain(|sfx| controls.is_current(sfx.bus, sfx.generation));
    }
    if active.len() < MAX_ACTIVE_SFX {
        active.push(ActiveSfx::from_queued(queued));
    } else if let Some(voice) = queued.voice {
        voice.stop();
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
        if let Some(voice) = &sfx.voice {
            if !voice.is_playing() { return false; }
            if voice.paused.load(Ordering::Acquire) { return true; }
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
        let src = &sfx.data[sfx.cursor..sfx.cursor + n];
        let dst = &mut mix_f32[start_sample..start_sample + n];
        if device_channels == 2 && sfx.pan != SfxPan::Center {
            mix_sfx_panned_stereo(src, dst, bus_gain, sfx.pan);
        } else {
            mix_sfx_samples(src, dst, bus_gain);
        }
        sfx.cursor += n;
        sfx.cursor < sfx.data.len()
    });
    mixed_sfx
}

#[inline(never)]
fn mix_sfx_panned_stereo(src: &[i16], dst: &mut [f32], gain: f32, pan: SfxPan) {
    let channel = usize::from(pan == SfxPan::Right);
    // RageSoundUtil::Pan at -1/+1 routes the source's right channel entirely
    // to the owning side. The prepared mine OGG is mono expanded to stereo.
    if gain == 1.0 || gain == 0.0 {
        // Unit gain is exact; zero gain only adds the product's sign. Keep
        // that addition for signed-zero output and NaN propagation.
        let scale = gain / 32768.0;
        for (source, target) in src.as_chunks::<2>().0.iter().zip(dst.as_chunks_mut::<2>().0) {
            target[channel] += f32::from(source[1]) * scale;
        }
        return;
    }
    for (source, target) in src.chunks_exact(2).zip(dst.chunks_exact_mut(2)) {
        target[channel] = i16_to_f32(source[1]).mul_add(gain, target[channel]);
    }
}

#[inline]
fn mix_sfx_samples(src: &[i16], dst: &mut [f32], gain: f32) {
    if gain == 1.0 {
        for (dst, &src) in dst.iter_mut().zip(src) {
            *dst += i16_to_f32(src);
        }
        return;
    }
    if gain == 0.0 {
        // A zero product needs no normalization or fused multiply-add. Still
        // add its sign to preserve signed-zero output and NaN propagation.
        for (dst, &src) in dst.iter_mut().zip(src) {
            *dst += f32::from(src) * gain;
        }
        return;
    }
    for (dst, &src) in dst.iter_mut().zip(src) {
        *dst = i16_to_f32(src).mul_add(gain, *dst);
    }
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
#[must_use]
pub const fn scheduled_onset_decision(
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
#[must_use]
pub fn f32_to_i16(sample: f32) -> i16 {
    // Rust float-to-integer casts saturate and map NaN to zero, which exactly
    // covers the full-scale boundaries after scaling.
    (sample * (f32::from(i16::MAX) + 1.0)) as i16
}

#[inline(always)]
#[must_use]
pub fn i16_to_f32(sample: i16) -> f32 {
    f32::from(sample) / (f32::from(i16::MAX) + 1.0)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_ACTIVE_SFX, MAX_SCHEDULE_AHEAD_FRAMES, MixBus, MixControls, QueuedSfx, SfxPan,
        ScheduledOnset, f32_to_i16, i16_to_f32, mix_active_sfx, push_queued_sfx,
        scheduled_onset_decision,
    };
    use std::sync::Arc;

    fn sample_play(data: &Arc<[i16]>, bus: MixBus, restart_slot: Option<u8>) -> QueuedSfx {
        QueuedSfx {
            data: Arc::clone(data),
            bus,
            generation: 0,
            restart_slot,
            pan: SfxPan::Center,
            target_stream_frame: 0,
            voice: None,
        }
    }

    #[test]
    fn mine_retrigger_rewinds_one_player_without_stopping_other_players() {
        let controls = MixControls::new();
        let bus = MixBus::new(0);
        let data: Arc<[i16]> = Arc::from([1000, 2000, 3000, 4000, 5000, 6000]);
        let mut active = Vec::new();

        // Two mines on one frame are one voice per player, as in ITGmania.
        push_queued_sfx(&mut active, sample_play(&data, bus, Some(0)), &controls);
        push_queued_sfx(&mut active, sample_play(&data, bus, Some(0)), &controls);
        assert_eq!(active.len(), 1);
        push_queued_sfx(&mut active, sample_play(&data, bus, Some(1)), &controls);
        assert_eq!(active.len(), 2);

        let mut out = [0.0; 2];
        assert!(mix_active_sfx(&mut active, &mut out, 0, 1, &controls));
        assert_eq!(out, [i16_to_f32(2000), i16_to_f32(4000)]);
        assert_eq!(active[0].cursor, 2);
        assert_eq!(active[1].cursor, 2);

        push_queued_sfx(&mut active, sample_play(&data, bus, Some(0)), &controls);
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].cursor, 0);
        assert_eq!(active[1].cursor, 2);

        out.fill(0.0);
        assert!(mix_active_sfx(&mut active, &mut out, 2, 1, &controls));
        assert_eq!(out, [i16_to_f32(4000), i16_to_f32(6000)]);
    }

    #[test]
    fn menu_sounds_keep_independent_overlapping_voices() {
        let controls = MixControls::new();
        let data: Arc<[i16]> = Arc::from([1000, 1000, 1000, 1000]);
        let mut active = Vec::new();
        let bus = MixBus::new(1);
        push_queued_sfx(&mut active, sample_play(&data, bus, None), &controls);
        push_queued_sfx(&mut active, sample_play(&data, bus, None), &controls);
        assert_eq!(active.len(), 2);
        let mut out = [0.0; 4];
        assert!(mix_active_sfx(&mut active, &mut out, 0, 1, &controls));
        assert_eq!(out, [i16_to_f32(2000); 4]);
    }

    #[test]
    fn two_player_mines_pan_to_separate_sides_using_native_balance() {
        let controls = MixControls::new();
        let samples: Arc<[i16]> = Arc::from([1000, 2000, 3000, 4000]);
        let bus = MixBus::new(3);
        for (pan, expected) in [
            (SfxPan::Left, [i16_to_f32(2000), 0.0, i16_to_f32(4000), 0.0]),
            (SfxPan::Right, [0.0, i16_to_f32(2000), 0.0, i16_to_f32(4000)]),
            (SfxPan::Center, [i16_to_f32(1000), i16_to_f32(2000), i16_to_f32(3000), i16_to_f32(4000)]),
        ] {
            let mut active = Vec::new();
            let mut queued = sample_play(&samples, bus, Some(0));
            queued.pan = pan;
            push_queued_sfx(&mut active, queued, &controls);
            let mut out = [0.0; 4];
            assert!(mix_active_sfx(&mut active, &mut out, 0, 2, &controls));
            assert_eq!(out, expected);
        }
    }

    #[test]
    fn mine_retrigger_works_when_32_voice_pool_is_full() {
        let controls = MixControls::new();
        let bus = MixBus::new(0);
        let first: Arc<[i16]> = Arc::from([1000, 2000]);
        let replacement: Arc<[i16]> = Arc::from([3000, 4000]);
        let mut active = Vec::with_capacity(MAX_ACTIVE_SFX);
        push_queued_sfx(&mut active, sample_play(&first, bus, Some(0)), &controls);
        for _ in 1..MAX_ACTIVE_SFX {
            push_queued_sfx(&mut active, sample_play(&first, bus, None), &controls);
        }
        assert_eq!(active.len(), MAX_ACTIVE_SFX);
        push_queued_sfx(&mut active, sample_play(&replacement, bus, Some(0)), &controls);
        assert_eq!(active.len(), MAX_ACTIVE_SFX);
        assert!(Arc::ptr_eq(&active[0].data, &replacement));
    }

    #[test]
    fn reset_bus_reclaims_stale_voices_before_accepting_new_sfx() {
        let controls = MixControls::new();
        let bus = MixBus::new(0);
        let data: Arc<[i16]> = Arc::from([1000, 2000]);
        let mut active = Vec::with_capacity(MAX_ACTIVE_SFX);
        for _ in 0..MAX_ACTIVE_SFX {
            push_queued_sfx(&mut active, sample_play(&data, bus, None), &controls);
        }
        assert_eq!(active.len(), MAX_ACTIVE_SFX);
        controls.stop_bus(bus);
        let mut next = sample_play(&data, bus, Some(0));
        next.generation = controls.bus_generation(bus);
        push_queued_sfx(&mut active, next, &controls);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].restart_slot, Some(0));
        assert_eq!(active[0].generation, controls.bus_generation(bus));
    }

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
        assert_eq!(f32_to_i16(f32::INFINITY), i16::MAX);
        assert_eq!(f32_to_i16(f32::NEG_INFINITY), i16::MIN);
        assert_eq!(f32_to_i16(f32::NAN), 0);
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
