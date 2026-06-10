use crate::ring::{self, SpscRingI16, SpscRingMusicSeg};
use crate::{
    ActiveSfx, CallbackClockSource, MAX_ACTIVE_SFX, MusicMapSeg, QueuedSfx, audio_mix_level_gains,
    f32_to_i16, i16_to_f32, mark_music_track_started, mix_active_sfx, music_gain_snap_generation,
    music_map_generation, music_target_gain, music_total_frames, music_track_active_relaxed,
    music_track_has_started, music_track_start_frame, note_timing_diag_callback_gap,
    publish_callback_window_end, publish_callback_window_start_nanos, push_queued_sfx,
    sfx_is_stale,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct AudioRenderMaps {
    pub queued_music_map: Arc<SpscRingMusicSeg>,
    pub played_music_map: Arc<SpscRingMusicSeg>,
}

impl AudioRenderMaps {
    #[inline(always)]
    pub fn new(
        queued_music_map: Arc<SpscRingMusicSeg>,
        played_music_map: Arc<SpscRingMusicSeg>,
    ) -> Self {
        Self {
            queued_music_map,
            played_music_map,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioRenderCallbackResult {
    pub output_underrun: bool,
    pub callback_gap_ns: u64,
}

pub struct RenderState {
    music_ring: Arc<SpscRingI16>,
    device_channels: usize,
    mix_i16: Vec<i16>,
    mix_f32: Vec<f32>,
    active_sfx: Vec<ActiveSfx>,
    queued_music_map: Arc<SpscRingMusicSeg>,
    played_music_map: Arc<SpscRingMusicSeg>,
    active_music_map: Option<MusicMapSeg>,
    music_map_generation: u64,
    /// Current music gain as seen by the mixer. Ramps toward the shared music
    /// gain target over [`MUSIC_GAIN_RAMP_FRAMES`] frames so asynchronous
    /// ReplayGain results do not produce an audible step.
    music_gain_current: f32,
    /// Gain snap generation last observed; when it changes, the mixer snaps
    /// `music_gain_current` straight to the target instead of ramping.
    music_gain_snap_seen: u64,
}

/// Number of device frames over which the music gain ramps when the target
/// changes. 4000 frames is about 83 ms at 48 kHz and about 91 ms at 44.1 kHz:
/// fast enough to feel instantaneous, slow enough to eliminate the click/step
/// that an atomic gain swap would produce.
const MUSIC_GAIN_RAMP_FRAMES: f32 = 4000.0;

impl RenderState {
    pub fn new(
        music_ring: Arc<SpscRingI16>,
        device_channels: usize,
        render_maps: AudioRenderMaps,
    ) -> Self {
        Self {
            music_ring,
            device_channels,
            mix_i16: Vec::new(),
            mix_f32: Vec::new(),
            active_sfx: Vec::with_capacity(MAX_ACTIVE_SFX),
            queued_music_map: render_maps.queued_music_map,
            played_music_map: render_maps.played_music_map,
            active_music_map: None,
            music_map_generation: music_map_generation(),
            music_gain_current: music_target_gain(),
            music_gain_snap_seen: music_gain_snap_generation(),
        }
    }

    #[inline(always)]
    fn begin_callback_nanos(
        &mut self,
        anchor_nanos: u64,
        source: CallbackClockSource,
    ) -> (u64, u64) {
        let map_generation = music_map_generation();
        if map_generation != self.music_map_generation {
            self.active_music_map = None;
            self.music_map_generation = map_generation;
        }
        if !music_track_active_relaxed() {
            self.active_music_map = None;
        }
        let total_before = music_total_frames();
        let callback_gap_ns =
            note_timing_diag_callback_gap(anchor_nanos, source).unwrap_or_default();
        publish_callback_window_start_nanos(total_before, anchor_nanos, source);
        (total_before, callback_gap_ns)
    }

    #[cfg(windows)]
    #[inline(always)]
    fn begin_callback_qpc(&mut self, anchor_nanos: u64) -> (u64, u64) {
        self.begin_callback_nanos(anchor_nanos, CallbackClockSource::Qpc)
    }

    #[inline(always)]
    fn ensure_mix_buffers(&mut self, len: usize) {
        if self.mix_i16.len() != len {
            self.mix_i16.resize(len, 0);
        }
        if self.mix_f32.len() != len {
            self.mix_f32.resize(len, 0.0);
        }
    }

    fn mix_f32_buffer<I>(&mut self, total_before: u64, len: usize, queued_sfx: I) -> (usize, bool)
    where
        I: IntoIterator<Item = QueuedSfx>,
    {
        self.ensure_mix_buffers(len);
        let popped = ring::callback_fill_from_ring_i16(&self.music_ring, &mut self.mix_i16);
        if popped > 0 {
            mark_music_track_started(total_before);
        }

        let (music_vol, sfx_vol, assist_tick_vol) = audio_mix_level_gains();
        let target_gain = music_target_gain();
        let snap_gen = music_gain_snap_generation();
        if snap_gen != self.music_gain_snap_seen {
            self.music_gain_current = target_gain;
            self.music_gain_snap_seen = snap_gen;
        }
        let device_channels = self.device_channels.max(1);
        let frames_in_buf = len / device_channels;
        let max_step = 1.0 / MUSIC_GAIN_RAMP_FRAMES;
        for f in 0..frames_in_buf {
            let diff = target_gain - self.music_gain_current;
            if diff.abs() <= max_step {
                self.music_gain_current = target_gain;
            } else {
                self.music_gain_current += diff.signum() * max_step;
            }
            let scale = music_vol * self.music_gain_current;
            let base = f * device_channels;
            for ch in 0..device_channels {
                let idx = base + ch;
                self.mix_f32[idx] = i16_to_f32(self.mix_i16[idx]) * scale;
            }
        }
        // Zero any tail that does not divide evenly into whole frames; the
        // mixer downstream expects exactly `len` valid f32 samples.
        for idx in frames_in_buf * device_channels..len {
            self.mix_f32[idx] = 0.0;
        }

        for new_sfx in queued_sfx {
            push_queued_sfx(&mut self.active_sfx, new_sfx, sfx_is_stale);
        }

        let mixed_sfx = mix_active_sfx(
            &mut self.active_sfx,
            &mut self.mix_f32,
            total_before,
            device_channels,
            sfx_vol,
            assist_tick_vol,
            sfx_is_stale,
        );

        (popped, mixed_sfx)
    }

    #[inline(always)]
    fn finish_callback(
        &mut self,
        total_before: u64,
        emitted_samples: usize,
        popped_samples: usize,
    ) -> bool {
        let frames = if self.device_channels == 0 {
            0
        } else {
            emitted_samples / self.device_channels
        };
        let popped_frames = if self.device_channels == 0 {
            0
        } else {
            popped_samples / self.device_channels
        };
        let output_underrun =
            music_track_active_relaxed() && music_track_has_started() && popped_frames < frames;
        let track_frames_before = total_before.saturating_sub(music_track_start_frame());
        if popped_frames > 0 {
            commit_played_music_map(
                track_frames_before as i64,
                popped_frames as i64,
                &self.queued_music_map,
                &self.played_music_map,
                &mut self.active_music_map,
            );
        }
        if frames > 0 {
            publish_callback_window_end(total_before, frames as u64);
        }
        output_underrun
    }

    #[cfg(windows)]
    pub fn render_i16_qpc<I>(
        &mut self,
        out: &mut [i16],
        anchor_nanos: u64,
        queued_sfx: I,
    ) -> AudioRenderCallbackResult
    where
        I: IntoIterator<Item = QueuedSfx>,
    {
        let (total_before, callback_gap_ns) = self.begin_callback_qpc(anchor_nanos);
        let (popped, _) = self.mix_f32_buffer(total_before, out.len(), queued_sfx);
        for (dst, src) in out.iter_mut().zip(&self.mix_f32) {
            *dst = f32_to_i16(*src);
        }
        AudioRenderCallbackResult {
            output_underrun: self.finish_callback(total_before, out.len(), popped),
            callback_gap_ns,
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub fn render_i16_host_nanos<I>(
        &mut self,
        out: &mut [i16],
        anchor_nanos: u64,
        queued_sfx: I,
    ) -> AudioRenderCallbackResult
    where
        I: IntoIterator<Item = QueuedSfx>,
    {
        let (total_before, callback_gap_ns) =
            self.begin_callback_nanos(anchor_nanos, CallbackClockSource::Instant);
        let (popped, _) = self.mix_f32_buffer(total_before, out.len(), queued_sfx);
        for (dst, src) in out.iter_mut().zip(&self.mix_f32) {
            *dst = f32_to_i16(*src);
        }
        AudioRenderCallbackResult {
            output_underrun: self.finish_callback(total_before, out.len(), popped),
            callback_gap_ns,
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub fn render_f32_host_nanos<I>(
        &mut self,
        out: &mut [f32],
        anchor_nanos: u64,
        queued_sfx: I,
    ) -> AudioRenderCallbackResult
    where
        I: IntoIterator<Item = QueuedSfx>,
    {
        let (total_before, callback_gap_ns) =
            self.begin_callback_nanos(anchor_nanos, CallbackClockSource::Instant);
        let (popped, mixed_sfx) = self.mix_f32_buffer(total_before, out.len(), queued_sfx);
        if mixed_sfx {
            for (dst, src) in out.iter_mut().zip(&self.mix_f32) {
                *dst = src.clamp(-1.0, 1.0);
            }
        } else {
            out.copy_from_slice(&self.mix_f32[..out.len()]);
        }
        AudioRenderCallbackResult {
            output_underrun: self.finish_callback(total_before, out.len(), popped),
            callback_gap_ns,
        }
    }

    #[cfg(windows)]
    pub fn render_f32_qpc<I>(
        &mut self,
        out: &mut [f32],
        anchor_nanos: u64,
        queued_sfx: I,
    ) -> AudioRenderCallbackResult
    where
        I: IntoIterator<Item = QueuedSfx>,
    {
        let (total_before, callback_gap_ns) = self.begin_callback_qpc(anchor_nanos);
        let (popped, mixed_sfx) = self.mix_f32_buffer(total_before, out.len(), queued_sfx);
        if mixed_sfx {
            for (dst, src) in out.iter_mut().zip(&self.mix_f32) {
                *dst = src.clamp(-1.0, 1.0);
            }
        } else {
            out.copy_from_slice(&self.mix_f32[..out.len()]);
        }
        AudioRenderCallbackResult {
            output_underrun: self.finish_callback(total_before, out.len(), popped),
            callback_gap_ns,
        }
    }
}

fn commit_played_music_map(
    track_frame_start: i64,
    frames_popped: i64,
    queued_seg_ring: &SpscRingMusicSeg,
    played_seg_ring: &SpscRingMusicSeg,
    current_seg: &mut Option<MusicMapSeg>,
) {
    let mut stream_frame = track_frame_start;
    let mut remaining = frames_popped.max(0);
    while remaining > 0 {
        let mut seg = match current_seg.take() {
            Some(seg) => seg,
            None => match ring::music_seg_ring_pop(queued_seg_ring) {
                Some(seg) => seg,
                None => break,
            },
        };
        let take = remaining.min(seg.frames);
        let played = MusicMapSeg {
            stream_frame_start: stream_frame,
            frames: take,
            music_start_sec: seg.music_start_sec,
            music_sec_per_frame: seg.music_sec_per_frame,
        };
        let _ = ring::music_seg_ring_push(played_seg_ring, played);
        seg.frames -= take;
        seg.music_start_sec += seg.music_sec_per_frame * take as f64;
        stream_frame += take;
        remaining -= take;
        if seg.frames > 0 {
            *current_seg = Some(seg);
        }
    }
}
