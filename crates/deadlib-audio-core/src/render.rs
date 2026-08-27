use crate::ring::{AudioRenderHandle, MusicBlock};
use crate::{
    ActiveSfx, CallbackClockSource, MAX_ACTIVE_SFX, MixControls, MusicMapSeg, QueuedSfx,
    f32_to_i16, i16_to_f32, mark_music_track_started, mix_active_sfx, music_gain_snap_generation,
    music_map_generation, music_target_gain, music_total_frames, music_track_active_relaxed,
    music_track_has_started, music_track_start_frame, note_timing_diag_callback_gap,
    publish_callback_window_end, publish_callback_window_start_nanos, push_queued_sfx,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderReport {
    pub output_underrun: bool,
    pub callback_gap_ns: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallbackInfo {
    pub anchor_nanos: u64,
    pub clock: CallbackClockSource,
}

pub enum OutputBufferMut<'a> {
    I16(&'a mut [i16]),
    F32(&'a mut [f32]),
}

pub struct RenderState {
    transport: AudioRenderHandle,
    controls: Arc<MixControls>,
    device_channels: usize,
    mix_f32: Vec<f32>,
    active_sfx: Vec<ActiveSfx>,
    active_music: Option<MusicBlock>,
    active_sample: usize,
    music_map_generation: u64,
    acknowledged_music_generation: Option<u64>,
    stale_blocks_left: usize,
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
const MUSIC_GAIN_MAX_STEP: f32 = 1.0 / MUSIC_GAIN_RAMP_FRAMES;
const MIX_CHUNK_FRAMES: usize = 2048;
#[inline(always)]
fn advance_gain(current: &mut f32, target: f32) {
    let diff = target - *current;
    if diff.abs() <= MUSIC_GAIN_MAX_STEP {
        *current = target;
    } else {
        *current += diff.signum() * MUSIC_GAIN_MAX_STEP;
    }
}

fn convert_music_samples(
    src: &[i16],
    dst: &mut [f32],
    channels: usize,
    music_vol: f32,
    target_gain: f32,
    current_gain: &mut f32,
) {
    if *current_gain == target_gain {
        let scale = music_vol * target_gain;
        for (dst, &src) in dst.iter_mut().zip(src) {
            *dst = i16_to_f32(src) * scale;
        }
        return;
    }
    for (src_frame, dst_frame) in src
        .chunks_exact(channels)
        .zip(dst.chunks_exact_mut(channels))
    {
        advance_gain(current_gain, target_gain);
        let scale = music_vol * *current_gain;
        for (dst, &src) in dst_frame.iter_mut().zip(src_frame) {
            *dst = i16_to_f32(src) * scale;
        }
    }
}

#[inline]
fn write_i16_samples(src: &[f32], dst: &mut [i16], has_audio: bool) {
    if !has_audio {
        dst.fill(0);
        return;
    }
    for (dst, &src) in dst.iter_mut().zip(src) {
        *dst = f32_to_i16(src);
    }
}

impl RenderState {
    pub fn new(
        transport: AudioRenderHandle,
        controls: Arc<MixControls>,
        device_channels: usize,
    ) -> Self {
        let capacity_blocks = transport.capacity_blocks();
        Self {
            transport,
            controls,
            device_channels,
            mix_f32: vec![0.0; MIX_CHUNK_FRAMES * device_channels.max(1)],
            active_sfx: Vec::with_capacity(MAX_ACTIVE_SFX),
            active_music: None,
            active_sample: 0,
            music_map_generation: music_map_generation(),
            acknowledged_music_generation: None,
            stale_blocks_left: capacity_blocks,
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
        self.stale_blocks_left = self.transport.capacity_blocks();
        let track_active = music_track_active_relaxed();
        self.refresh_music_generation(!track_active);
        if !track_active {
            let _ = self.recycle_active();
        }
        let total_before = music_total_frames();
        let callback_gap_ns =
            note_timing_diag_callback_gap(anchor_nanos, source).unwrap_or_default();
        publish_callback_window_start_nanos(total_before, anchor_nanos, source);
        (total_before, callback_gap_ns)
    }

    #[inline(always)]
    fn recycle_active(&mut self) -> bool {
        let Some(block) = self.active_music.take() else {
            self.active_sample = 0;
            return true;
        };
        match self.transport.recycle_block(block) {
            Ok(()) => {
                self.active_sample = 0;
                true
            }
            Err(block) => {
                // Never free a pooled allocation on the realtime thread. A
                // full recycle queue violates ownership conservation, so keep
                // the block and retry on the next callback.
                self.active_music = Some(block);
                false
            }
        }
    }

    #[inline(always)]
    fn refresh_music_generation(&mut self, allow_unpublished: bool) {
        let target_generation = music_map_generation();
        if target_generation != self.music_map_generation
            && (allow_unpublished || self.transport.published_generation() == target_generation)
        {
            self.music_map_generation = target_generation;
        }
    }

    fn load_music_block(&mut self) -> bool {
        loop {
            self.refresh_music_generation(false);
            if let Some(block) = &self.active_music {
                let exhausted = self.active_sample >= block.samples().len();
                if !exhausted && block.timing().generation == self.music_map_generation {
                    let generation = self.music_map_generation;
                    if self.acknowledged_music_generation != Some(generation) {
                        // All preceding stale blocks were recycled before this
                        // matching block was reached. Publish that fact after
                        // the recycle queue ownership transfers.
                        self.transport.acknowledge_generation_drained(generation);
                        self.acknowledged_music_generation = Some(generation);
                    }
                    return true;
                }
                let target_generation = music_map_generation();
                if !exhausted && block.timing().generation == target_generation {
                    // The producer may still be filling the transition batch.
                    // Retain its first block until readiness is published; do
                    // not recycle usable target audio as if it were stale.
                    return false;
                }
                if !exhausted {
                    if self.stale_blocks_left == 0 {
                        return false;
                    }
                    self.stale_blocks_left -= 1;
                }
                if !self.recycle_active() {
                    return false;
                }
            }
            let Some(block) = self.transport.pop_block() else {
                return false;
            };
            self.active_music = Some(block);
            self.active_sample = 0;
        }
    }

    fn mix_music(&mut self, mix_f32: &mut [f32], total_before: u64, music_vol: f32) -> usize {
        let len = mix_f32.len();
        let channels = self.device_channels.max(1);
        let frames = len / channels;
        let target_gain = music_target_gain();
        let mut track_frame_start = None;
        let mut frame = 0;
        let mut popped_samples = 0;
        while frame < frames && self.load_music_block() {
            let Some(active) = self.active_music.as_ref() else {
                break;
            };
            let timing = active.timing();
            let available_frames = (active.samples().len() - self.active_sample) / channels;
            let take = available_frames.min(frames - frame);
            let track_frame_start = *track_frame_start.get_or_insert_with(|| {
                mark_music_track_started(total_before);
                total_before.saturating_sub(music_track_start_frame()) as i64
            });
            let block_frame = self.active_sample / channels;
            let consumed = take * channels;
            let dst_start = frame * channels;
            {
                let Some(active) = self.active_music.as_ref() else {
                    break;
                };
                let src = &active.samples()[self.active_sample..self.active_sample + consumed];
                let dst = &mut mix_f32[dst_start..dst_start + consumed];
                convert_music_samples(
                    src,
                    dst,
                    channels,
                    music_vol,
                    target_gain,
                    &mut self.music_gain_current,
                );
            }
            self.transport.push_played(
                timing.generation,
                MusicMapSeg {
                    stream_frame_start: track_frame_start + frame as i64,
                    frames: take as i64,
                    music_start_sec: timing.music_start_sec
                        + block_frame as f64 * timing.music_sec_per_frame,
                    music_sec_per_frame: timing.music_sec_per_frame,
                },
            );
            self.active_sample += consumed;
            popped_samples += consumed;
            frame += take;
            let exhausted = self
                .active_music
                .as_ref()
                .is_some_and(|block| self.active_sample == block.samples().len());
            if exhausted && !self.recycle_active() {
                break;
            }
        }
        if self.music_gain_current != target_gain {
            for _ in frame..frames {
                advance_gain(&mut self.music_gain_current, target_gain);
            }
        }
        mix_f32[frame * channels..frames * channels].fill(0.0);
        mix_f32[frames * channels..len].fill(0.0);
        popped_samples
    }

    fn queue_sfx<I>(&mut self, queued_sfx: I)
    where
        I: IntoIterator<Item = QueuedSfx>,
    {
        for new_sfx in queued_sfx {
            push_queued_sfx(&mut self.active_sfx, new_sfx, &self.controls);
        }
    }

    fn mix_f32_buffer(&mut self, total_before: u64, len: usize) -> (usize, bool) {
        debug_assert!(len <= self.mix_f32.len());
        let mut mix_f32 = std::mem::take(&mut self.mix_f32);
        let result = self.mix_f32_buffer_into(total_before, &mut mix_f32[..len]);
        self.mix_f32 = mix_f32;
        result
    }

    fn mix_f32_buffer_into(&mut self, total_before: u64, mix_f32: &mut [f32]) -> (usize, bool) {
        let stream_gain = self.controls.stream_gain();
        let snap_gen = music_gain_snap_generation();
        if snap_gen != self.music_gain_snap_seen {
            self.music_gain_current = music_target_gain();
            self.music_gain_snap_seen = snap_gen;
        }
        let popped = self.mix_music(mix_f32, total_before, stream_gain);

        let mixed_sfx = mix_active_sfx(
            &mut self.active_sfx,
            mix_f32,
            total_before,
            self.device_channels.max(1),
            &self.controls,
        );

        (popped, mixed_sfx)
    }

    fn render_f32_chunks<I>(&mut self, out: &mut [f32], total_before: u64, queued_sfx: I) -> usize
    where
        I: IntoIterator<Item = QueuedSfx>,
    {
        self.queue_sfx(queued_sfx);
        let channels = self.device_channels.max(1);
        let chunk_samples = self.mix_f32.len();
        let mut emitted_samples = 0;
        let mut popped_samples = 0;
        let mut mixed_sfx = false;
        for out_chunk in out.chunks_mut(chunk_samples) {
            let chunk_total = total_before + (emitted_samples / channels) as u64;
            let (popped, chunk_mixed_sfx) = self.mix_f32_buffer_into(chunk_total, out_chunk);
            mixed_sfx |= chunk_mixed_sfx;
            emitted_samples += out_chunk.len();
            popped_samples += popped;
        }
        if mixed_sfx {
            for sample in out {
                *sample = sample.clamp(-1.0, 1.0);
            }
        }
        popped_samples
    }

    fn render_i16_chunks<I>(&mut self, out: &mut [i16], total_before: u64, queued_sfx: I) -> usize
    where
        I: IntoIterator<Item = QueuedSfx>,
    {
        self.queue_sfx(queued_sfx);
        let channels = self.device_channels.max(1);
        let chunk_samples = self.mix_f32.len();
        let mut emitted_samples = 0;
        let mut popped_samples = 0;
        for out_chunk in out.chunks_mut(chunk_samples) {
            let chunk_total = total_before + (emitted_samples / channels) as u64;
            let chunk_len = out_chunk.len();
            let (popped, mixed_sfx) = self.mix_f32_buffer(chunk_total, chunk_len);
            write_i16_samples(
                &self.mix_f32[..chunk_len],
                out_chunk,
                popped > 0 || mixed_sfx,
            );
            emitted_samples += out_chunk.len();
            popped_samples += popped;
        }
        popped_samples
    }

    #[inline(always)]
    fn finish_callback(
        &mut self,
        total_before: u64,
        emitted_samples: usize,
        popped_samples: usize,
    ) -> bool {
        let frames = emitted_samples
            .checked_div(self.device_channels)
            .unwrap_or(0);
        let popped_frames = popped_samples
            .checked_div(self.device_channels)
            .unwrap_or(0);
        let output_underrun =
            music_track_active_relaxed() && music_track_has_started() && popped_frames < frames;
        if frames > 0 {
            publish_callback_window_end(total_before, frames as u64);
        }
        output_underrun
    }

    pub fn render<I>(
        &mut self,
        output: OutputBufferMut<'_>,
        callback: CallbackInfo,
        queued_sfx: I,
    ) -> RenderReport
    where
        I: IntoIterator<Item = QueuedSfx>,
    {
        let (total_before, callback_gap_ns) =
            self.begin_callback_nanos(callback.anchor_nanos, callback.clock);
        let (emitted_samples, popped_samples) = match output {
            OutputBufferMut::I16(out) => {
                let len = out.len();
                (len, self.render_i16_chunks(out, total_before, queued_sfx))
            }
            OutputBufferMut::F32(out) => {
                let len = out.len();
                (len, self.render_f32_chunks(out, total_before, queued_sfx))
            }
        };
        RenderReport {
            output_underrun: self.finish_callback(total_before, emitted_samples, popped_samples),
            callback_gap_ns,
        }
    }
}

#[cfg(feature = "bench-support")]
pub mod bench_support {
    use super::{convert_music_samples, f32_to_i16, write_i16_samples};

    pub fn music_copy_old(src: &[i16], scratch: &mut [f32], dst: &mut [f32], gain: f32) {
        let mut current_gain = gain;
        convert_music_samples(src, scratch, 2, 1.0, gain, &mut current_gain);
        dst.copy_from_slice(scratch);
    }

    pub fn music_direct_new(src: &[i16], dst: &mut [f32], gain: f32) {
        let mut current_gain = gain;
        convert_music_samples(src, dst, 2, 1.0, gain, &mut current_gain);
    }

    pub fn silent_i16_old(src: &[f32], dst: &mut [i16]) {
        for (dst, &src) in dst.iter_mut().zip(src) {
            *dst = f32_to_i16(src);
        }
    }

    pub fn silent_i16_new(src: &[f32], dst: &mut [i16]) {
        write_i16_samples(src, dst, false);
    }
}

impl Drop for RenderState {
    fn drop(&mut self) {
        let _ = self.recycle_active();
    }
}

#[cfg(test)]
mod tests {
    use super::{MIX_CHUNK_FRAMES, MUSIC_GAIN_MAX_STEP, RenderState, write_i16_samples};
    use crate::ring::{
        AudioRenderHandle, MusicBlockTiming, MusicBlockWriter, PlayedMapReader, music_transport,
    };
    use crate::{
        CallbackClockSource, MixBus, MixControls, QueuedSfx, activate_music_track,
        bump_music_map_generation, i16_to_f32, music_map_generation, music_track_start_frame,
        reset_music_target_gain, set_music_target_gain, snap_music_gain_generation,
        stop_music_track,
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    const CHANNELS: usize = 2;
    const SAMPLE_RATE: u32 = 48_000;
    const EFFECT_BUS: MixBus = MixBus::new(0);
    const SEC_PER_FRAME: f64 = 1.0 / 48_000.0;
    static GLOBAL_AUDIO_STATE_BUSY: AtomicBool = AtomicBool::new(false);

    #[test]
    fn silent_i16_fast_path_matches_sample_conversion() {
        let src = [0.0f32; 33];
        let mut expected = [i16::MIN; 33];
        for (dst, &src) in expected.iter_mut().zip(&src) {
            *dst = crate::f32_to_i16(src);
        }
        let mut actual = [i16::MAX; 33];
        write_i16_samples(&src, &mut actual, false);
        assert_eq!(actual, expected);
    }

    #[test]
    fn silent_i16_callback_overwrites_the_whole_device_buffer() {
        let _guard = GlobalAudioGuard::acquire();
        reset_levels();
        let (_stream, render_handle) = music_transport(SAMPLE_RATE, CHANNELS);
        let mut render = test_render(render_handle);
        let mut output = vec![i16::MAX; MIX_CHUNK_FRAMES * CHANNELS + 17];

        let popped = render.render_i16_chunks(
            &mut output,
            music_track_start_frame(),
            std::iter::empty::<QueuedSfx>(),
        );

        assert_eq!(popped, 0);
        assert!(output.iter().all(|&sample| sample == 0));
    }

    struct GlobalAudioGuard;

    impl GlobalAudioGuard {
        fn acquire() -> Self {
            while GLOBAL_AUDIO_STATE_BUSY
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                std::thread::yield_now();
            }
            Self
        }
    }

    impl Drop for GlobalAudioGuard {
        fn drop(&mut self) {
            GLOBAL_AUDIO_STATE_BUSY.store(false, Ordering::Release);
        }
    }

    fn reset_levels() {
        reset_music_target_gain();
    }

    fn test_render(render_handle: AudioRenderHandle) -> RenderState {
        RenderState::new(render_handle, Arc::new(MixControls::new()), CHANNELS)
    }

    fn push_all(
        writer: &mut MusicBlockWriter,
        samples: &[i16],
        generation: u64,
        start_frame: usize,
    ) {
        let mut offset = 0;
        while offset < samples.len() {
            let frame = start_frame + offset / CHANNELS;
            let pushed = writer.try_push(
                &samples[offset..],
                MusicBlockTiming {
                    generation,
                    music_start_sec: frame as f64 * SEC_PER_FRAME,
                    music_sec_per_frame: SEC_PER_FRAME,
                },
            );
            assert!(pushed > 0, "test transport has enough recycled blocks");
            offset += pushed;
        }
    }

    fn pop_map(played: &mut PlayedMapReader) -> (u64, crate::MusicMapSeg) {
        played.pop().expect("played timing span")
    }

    #[test]
    fn mixes_across_callback_and_block_boundaries() {
        let _guard = GlobalAudioGuard::acquire();
        reset_levels();
        let generation = music_map_generation();
        let (mut stream, render_handle) = music_transport(SAMPLE_RATE, CHANNELS);
        let input: Vec<i16> = (0..300 * CHANNELS)
            .map(|sample| (sample as i32 * 97 % 60_000 - 30_000) as i16)
            .collect();
        push_all(&mut stream.writer, &input, generation, 0);
        let mut render = test_render(render_handle);
        let track_start = music_track_start_frame();

        let (popped, _) = render.mix_f32_buffer(track_start, 100 * CHANNELS);
        assert_eq!(popped, 100 * CHANNELS);
        for (&actual, &expected) in render.mix_f32.iter().zip(&input[..popped]) {
            assert_eq!(actual.to_bits(), i16_to_f32(expected).to_bits());
        }

        let (popped, _) = render.mix_f32_buffer(track_start + 100, 200 * CHANNELS);
        assert_eq!(popped, 200 * CHANNELS);
        for (&actual, &expected) in render.mix_f32.iter().zip(&input[200..600]) {
            assert_eq!(actual.to_bits(), i16_to_f32(expected).to_bits());
        }

        let expected = [(0, 100), (100, 156), (256, 44)];
        for (start, frames) in expected {
            let (tag, seg) = pop_map(&mut stream.played_map);
            assert_eq!(tag, generation);
            assert_eq!(seg.stream_frame_start, start);
            assert_eq!(seg.frames, frames);
            assert_eq!(
                seg.music_start_sec.to_bits(),
                (start as f64 * SEC_PER_FRAME).to_bits()
            );
            assert_eq!(seg.music_sec_per_frame.to_bits(), SEC_PER_FRAME.to_bits());
        }
        assert!(stream.played_map.pop().is_none());
    }

    #[test]
    fn callback_larger_than_scratch_buffer_is_chunked_without_data_loss() {
        let _guard = GlobalAudioGuard::acquire();
        reset_levels();
        let generation = music_map_generation();
        let (mut stream, render_handle) = music_transport(SAMPLE_RATE, CHANNELS);
        let input: Vec<i16> = (0..3000 * CHANNELS)
            .map(|sample| (sample as i32 * 193 % 60_000 - 30_000) as i16)
            .collect();
        push_all(&mut stream.writer, &input, generation, 0);
        let mut render = test_render(render_handle);
        let mut output = vec![f32::NAN; input.len()];

        let popped = render.render_f32_chunks(
            &mut output,
            music_track_start_frame(),
            std::iter::empty::<crate::QueuedSfx>(),
        );

        assert_eq!(popped, input.len());
        for (&actual, &expected) in output.iter().zip(&input) {
            assert_eq!(actual.to_bits(), i16_to_f32(expected).to_bits());
        }
    }

    #[test]
    fn sfx_in_later_chunk_clamps_the_whole_f32_callback() {
        let _guard = GlobalAudioGuard::acquire();
        reset_levels();
        set_music_target_gain(2.0);
        snap_music_gain_generation();
        let generation = music_map_generation();
        let (mut stream, render_handle) = music_transport(SAMPLE_RATE, CHANNELS);
        let input = vec![i16::MAX; 3000 * CHANNELS];
        push_all(&mut stream.writer, &input, generation, 0);
        let mut render = test_render(render_handle);
        let total_before = music_track_start_frame();
        let sfx = QueuedSfx {
            data: Arc::from([0, 0]),
            bus: EFFECT_BUS,
            generation: 0,
            target_stream_frame: total_before + MIX_CHUNK_FRAMES as u64 + 5,
        };
        let mut output = vec![f32::NAN; input.len()];

        let popped = render.render_f32_chunks(&mut output, total_before, [sfx]);

        assert_eq!(popped, input.len());
        assert!(output.iter().all(|&sample| sample == 1.0));
        reset_levels();
    }

    #[test]
    fn generation_reset_discards_active_and_queued_old_audio() {
        let _guard = GlobalAudioGuard::acquire();
        reset_levels();
        let old_generation = music_map_generation();
        let (mut stream, render_handle) = music_transport(SAMPLE_RATE, CHANNELS);
        let old = vec![11_000; 256 * CHANNELS];
        push_all(&mut stream.writer, &old, old_generation, 0);
        let mut render = test_render(render_handle);
        let track_start = music_track_start_frame();
        assert_eq!(
            render.mix_f32_buffer(track_start, 20 * CHANNELS).0,
            20 * CHANNELS
        );

        let new_generation = bump_music_map_generation();
        push_all(&mut stream.writer, &old, old_generation, 256);
        let new = vec![-7_000; 64 * CHANNELS];
        push_all(&mut stream.writer, &new, new_generation, 0);
        stream.writer.publish_generation_ready(new_generation);
        let (popped, _) = render.mix_f32_buffer(track_start + 20, 32 * CHANNELS);
        assert_eq!(popped, 32 * CHANNELS);
        for &actual in &render.mix_f32[..32 * CHANNELS] {
            assert_eq!(actual.to_bits(), i16_to_f32(-7_000).to_bits());
        }

        let (old_tag, old_seg) = pop_map(&mut stream.played_map);
        assert_eq!((old_tag, old_seg.frames), (old_generation, 20));
        let (new_tag, new_seg) = pop_map(&mut stream.played_map);
        assert_eq!((new_tag, new_seg.frames), (new_generation, 32));
        assert!(stream.played_map.pop().is_none());
    }

    #[test]
    fn generation_reset_keeps_old_audio_until_replacement_is_published() {
        let _guard = GlobalAudioGuard::acquire();
        reset_levels();
        let old_generation = music_map_generation();
        let (mut stream, render_handle) = music_transport(SAMPLE_RATE, CHANNELS);
        let old = vec![11_000; 128 * CHANNELS];
        push_all(&mut stream.writer, &old, old_generation, 0);
        let mut render = test_render(render_handle);
        let track_start = music_track_start_frame();

        let new_generation = bump_music_map_generation();
        let (popped, _) = render.mix_f32_buffer(track_start, 32 * CHANNELS);
        assert_eq!(popped, 32 * CHANNELS);
        assert!(
            render.mix_f32[..popped]
                .iter()
                .all(|sample| sample.to_bits() == i16_to_f32(11_000).to_bits())
        );

        let new = vec![-7_000; 64 * CHANNELS];
        push_all(&mut stream.writer, &new, new_generation, 0);
        stream.writer.publish_generation_ready(new_generation);
        let (popped, _) = render.mix_f32_buffer(track_start + 32, 32 * CHANNELS);
        assert_eq!(popped, 32 * CHANNELS);
        assert_eq!(stream.writer.render_drained_generation(), new_generation);
        assert!(
            render.mix_f32[..popped]
                .iter()
                .all(|sample| sample.to_bits() == i16_to_f32(-7_000).to_bits())
        );
    }

    #[test]
    fn target_block_is_retained_while_transition_batch_is_primed() {
        let _guard = GlobalAudioGuard::acquire();
        reset_levels();
        let (mut stream, render_handle) = music_transport(SAMPLE_RATE, CHANNELS);
        let mut render = test_render(render_handle);
        let new_generation = bump_music_map_generation();
        push_all(&mut stream.writer, &[-7_000, 7_000], new_generation, 0);

        assert_eq!(
            render.mix_f32_buffer(music_track_start_frame(), CHANNELS).0,
            0
        );
        assert!(render.active_music.is_some());

        stream.writer.publish_generation_ready(new_generation);
        let (popped, _) = render.mix_f32_buffer(music_track_start_frame(), CHANNELS);
        assert_eq!(popped, CHANNELS);
        assert_eq!(render.mix_f32[0].to_bits(), i16_to_f32(-7_000).to_bits());
        assert_eq!(render.mix_f32[1].to_bits(), i16_to_f32(7_000).to_bits());
    }

    #[test]
    fn stopped_track_does_not_render_an_unpublished_stale_generation() {
        let _guard = GlobalAudioGuard::acquire();
        reset_levels();
        activate_music_track();
        let generation = music_map_generation();
        let (mut stream, render_handle) = music_transport(SAMPLE_RATE, CHANNELS);
        push_all(&mut stream.writer, &[12_000, -12_000], generation, 0);
        let mut render = test_render(render_handle);

        bump_music_map_generation();
        stop_music_track();
        let (total_before, _) = render.begin_callback_nanos(0, CallbackClockSource::Instant);
        let (popped, _) = render.mix_f32_buffer(total_before, CHANNELS);

        assert_eq!(popped, 0);
        assert_eq!(&render.mix_f32[..CHANNELS], &[0.0, 0.0]);
    }

    #[test]
    fn underrun_zeroes_whole_frames_and_unaligned_tail() {
        let _guard = GlobalAudioGuard::acquire();
        reset_levels();
        let generation = music_map_generation();
        let (mut stream, render_handle) = music_transport(SAMPLE_RATE, CHANNELS);
        push_all(&mut stream.writer, &[1234, -1234], generation, 0);
        let mut render = test_render(render_handle);
        let (popped, _) = render.mix_f32_buffer(music_track_start_frame(), 5);

        assert_eq!(popped, 2);
        assert_eq!(render.mix_f32[0].to_bits(), i16_to_f32(1234).to_bits());
        assert_eq!(render.mix_f32[1].to_bits(), i16_to_f32(-1234).to_bits());
        assert_eq!(&render.mix_f32[2..5], &[0.0, 0.0, 0.0]);
    }

    #[test]
    fn gain_ramp_still_advances_once_per_device_frame() {
        let _guard = GlobalAudioGuard::acquire();
        reset_levels();
        let generation = music_map_generation();
        let (mut stream, render_handle) = music_transport(SAMPLE_RATE, CHANNELS);
        let input = [10_000, -10_000, 20_000, -20_000, 30_000, -30_000];
        push_all(&mut stream.writer, &input, generation, 0);
        let mut render = test_render(render_handle);
        set_music_target_gain(0.5);

        let (popped, _) = render.mix_f32_buffer(music_track_start_frame(), input.len());
        assert_eq!(popped, input.len());
        let mut gain = 1.0;
        for (frame, samples) in input.as_chunks::<CHANNELS>().0.iter().enumerate() {
            gain -= MUSIC_GAIN_MAX_STEP;
            for (channel, &sample) in samples.iter().enumerate() {
                let expected = i16_to_f32(sample) * gain;
                assert_eq!(
                    render.mix_f32[frame * CHANNELS + channel].to_bits(),
                    expected.to_bits()
                );
            }
        }
        reset_music_target_gain();
    }

    #[test]
    fn first_played_span_starts_at_zero_after_decode_delay() {
        let _guard = GlobalAudioGuard::acquire();
        reset_levels();
        let generation = music_map_generation();
        let (mut stream, render_handle) = music_transport(SAMPLE_RATE, CHANNELS);
        push_all(&mut stream.writer, &[1000, -1000], generation, 0);
        let mut render = test_render(render_handle);
        let delayed_callback = music_track_start_frame() + 123;
        activate_music_track();

        let (popped, _) = render.mix_f32_buffer(delayed_callback, CHANNELS);
        assert_eq!(popped, CHANNELS);
        let (_, seg) = pop_map(&mut stream.played_map);
        assert_eq!(seg.stream_frame_start, 0);
        stop_music_track();
    }

    #[test]
    fn reset_reaches_current_audio_behind_a_full_stale_backlog() {
        let _guard = GlobalAudioGuard::acquire();
        reset_levels();
        let old_generation = music_map_generation();
        let (mut stream, render_handle) = music_transport(SAMPLE_RATE, CHANNELS);
        let block = vec![500; crate::ring::MUSIC_BLOCK_FRAMES * CHANNELS];
        let pool_blocks = stream.writer.capacity_blocks();
        for block_index in 0..pool_blocks - 1 {
            push_all(
                &mut stream.writer,
                &block,
                old_generation,
                block_index * crate::ring::MUSIC_BLOCK_FRAMES,
            );
        }
        let new_generation = bump_music_map_generation();
        push_all(&mut stream.writer, &[700, -700], new_generation, 0);
        stream.writer.publish_generation_ready(new_generation);
        let mut render = test_render(render_handle);

        let (popped, _) = render.mix_f32_buffer(music_track_start_frame(), CHANNELS);
        assert_eq!(popped, CHANNELS);
        assert_eq!(render.mix_f32[0].to_bits(), i16_to_f32(700).to_bits());
        assert_eq!(render.mix_f32[1].to_bits(), i16_to_f32(-700).to_bits());
        assert_eq!(render.stale_blocks_left, pool_blocks - (pool_blocks - 1));
    }
}
