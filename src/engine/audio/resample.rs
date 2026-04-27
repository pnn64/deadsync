use super::{Cut, ENGINE, MusicMapSeg, MusicStream, QUEUED_MUSIC_MAP_SEGS, internal};
use crate::engine::audio::decode;
#[cfg(windows)]
use crate::engine::windows_rt::{ThreadRole, boost_current_thread};
use log::{debug, error, warn};
use rubato::{
    Resampler, SincFixedOut, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use smallvec::SmallVec;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;

const OUT_FRAMES_PER_CALL: usize = 256;
const PLANAR_INPUT_CAP_FRAMES: usize = 4096;
const PLANAR_COMPACT_THRESHOLD_FRAMES: usize = 2048;
const SILENCE_CHUNK_FRAMES: usize = 2048;

fn push_music_block_with_map(
    sample_ring: &internal::SpscRingI16,
    seg_ring: &internal::SpscRingMusicSeg,
    block: &[i16],
    out_channels: usize,
    music_start_sec: f64,
    music_sec_per_frame: f64,
    stop: &AtomicBool,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    if block.is_empty() || out_channels == 0 {
        return Ok(music_start_sec);
    }
    let mut sample_offset = 0usize;
    let mut next_music_sec = music_start_sec;
    while sample_offset < block.len() {
        if stop.load(Ordering::Relaxed) {
            return Ok(next_music_sec);
        }
        let free_samples = internal::ring_free_samples(sample_ring);
        if free_samples < out_channels || !internal::music_seg_ring_has_space(seg_ring) {
            thread::sleep(std::time::Duration::from_micros(300));
            continue;
        }
        let chunk_samples =
            (block.len() - sample_offset).min(free_samples - (free_samples % out_channels));
        if chunk_samples == 0 {
            thread::sleep(std::time::Duration::from_micros(300));
            continue;
        }
        let chunk_frames = chunk_samples / out_channels;
        let pushed = internal::ring_push(
            sample_ring,
            &block[sample_offset..sample_offset + chunk_samples],
        );
        debug_assert_eq!(pushed, chunk_samples);
        let seg = MusicMapSeg {
            stream_frame_start: 0,
            frames: chunk_frames as i64,
            music_start_sec: next_music_sec,
            music_sec_per_frame,
        };
        let pushed_seg = internal::music_seg_ring_push(seg_ring, seg);
        debug_assert!(pushed_seg);
        sample_offset += chunk_samples;
        next_music_sec += chunk_frames as f64 * music_sec_per_frame;
    }
    Ok(next_music_sec)
}

struct PlanarAccum {
    channels: Vec<Vec<f32>>,
    start_frame: usize,
}

impl PlanarAccum {
    fn new(channels: usize, capacity_frames: usize) -> Self {
        let mut planar = Vec::with_capacity(channels);
        for _ in 0..channels {
            planar.push(Vec::with_capacity(capacity_frames));
        }
        Self {
            channels: planar,
            start_frame: 0,
        }
    }

    #[inline(always)]
    fn available_frames(&self) -> usize {
        self.channels
            .first()
            .map_or(0, |channel| channel.len().saturating_sub(self.start_frame))
    }

    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.available_frames() == 0
    }

    fn push_i16_interleaved(&mut self, interleaved: &[i16], channels: usize) {
        if interleaved.is_empty() || channels == 0 {
            return;
        }
        debug_assert_eq!(channels, self.channels.len());
        let frames = interleaved.len() / channels;
        if frames == 0 {
            return;
        }
        for channel in &mut self.channels {
            channel.reserve(frames);
        }
        for frame in interleaved.chunks_exact(channels) {
            for (channel, sample) in self.channels.iter_mut().zip(frame.iter()) {
                channel.push(f32::from(*sample) / 32768.0);
            }
        }
    }

    fn consume_frames(&mut self, frames: usize) {
        let total_frames = self.channels.first().map_or(0, Vec::len);
        self.start_frame = (self.start_frame + frames).min(total_frames);
        self.compact_if_needed();
    }

    fn clear(&mut self) {
        self.start_frame = 0;
        for channel in &mut self.channels {
            channel.clear();
        }
    }

    fn compact_if_needed(&mut self) {
        if self.start_frame == 0 {
            return;
        }
        let total_frames = self.channels.first().map_or(0, Vec::len);
        let remaining_frames = total_frames.saturating_sub(self.start_frame);
        if remaining_frames == 0 {
            self.clear();
            return;
        }
        if self.start_frame < PLANAR_COMPACT_THRESHOLD_FRAMES && self.start_frame * 2 < total_frames
        {
            return;
        }
        for channel in &mut self.channels {
            channel.copy_within(self.start_frame.., 0);
            channel.truncate(remaining_frames);
        }
        self.start_frame = 0;
    }
}

#[inline(always)]
fn resampler_params() -> SincInterpolationParameters {
    SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    }
}

#[inline(always)]
fn write_resampler_output(out: &[Vec<f32>], out_ch: usize, out_tmp: &mut Vec<i16>) -> usize {
    let produced_frames = out.first().map_or(0, Vec::len);
    let produced_samples = produced_frames.saturating_mul(out_ch);
    if out_tmp.len() < produced_samples {
        out_tmp.resize(produced_samples, 0);
    } else {
        out_tmp.truncate(produced_samples);
    }
    for frame in 0..produced_frames {
        let base = frame * out_ch;
        for channel in 0..out_ch {
            let sample = out[channel % out.len()][frame];
            out_tmp[base + channel] = (sample * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
        }
    }
    produced_frames
}

#[inline(always)]
fn drop_front_samples(samples: &mut Vec<i16>, drop_samples: usize) {
    if drop_samples == 0 {
        return;
    }
    if drop_samples >= samples.len() {
        samples.clear();
        return;
    }
    let remaining = samples.len() - drop_samples;
    samples.copy_within(drop_samples.., 0);
    samples.truncate(remaining);
}

fn push_silence_with_map(
    sample_ring: &internal::SpscRingI16,
    seg_ring: &internal::SpscRingMusicSeg,
    silence_frames: usize,
    out_channels: usize,
    music_start_sec: f64,
    music_sec_per_frame: f64,
    stop: &AtomicBool,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    if silence_frames == 0 || out_channels == 0 {
        return Ok(music_start_sec);
    }
    let mut next_music_sec = music_start_sec;
    let silence_chunk = vec![0i16; SILENCE_CHUNK_FRAMES * out_channels];
    let mut frames_left = silence_frames;
    while frames_left > 0 {
        let chunk_frames = frames_left.min(SILENCE_CHUNK_FRAMES);
        let chunk_samples = chunk_frames * out_channels;
        next_music_sec = push_music_block_with_map(
            sample_ring,
            seg_ring,
            &silence_chunk[..chunk_samples],
            out_channels,
            next_music_sec,
            music_sec_per_frame,
            stop,
        )?;
        frames_left -= chunk_frames;
    }
    Ok(next_music_sec)
}

fn process_resampler_block<'a>(
    resampler: &mut SincFixedOut<f32>,
    in_planar: &'a PlanarAccum,
    frames: usize,
) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error + Send + Sync>> {
    debug_assert!(frames <= in_planar.available_frames());
    let mut input_slices = SmallVec::<[&[f32]; 2]>::with_capacity(in_planar.channels.len());
    let start = in_planar.start_frame;
    let end = start + frames;
    for channel in &in_planar.channels {
        input_slices.push(&channel[start..end]);
    }
    Ok(resampler.process(input_slices.as_slice(), None)?)
}

fn process_resampler_partial<'a>(
    resampler: &mut SincFixedOut<f32>,
    in_planar: &'a PlanarAccum,
    frames: usize,
) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error + Send + Sync>> {
    debug_assert!(frames <= in_planar.available_frames());
    let mut input_slices = SmallVec::<[&[f32]; 2]>::with_capacity(in_planar.channels.len());
    let start = in_planar.start_frame;
    let end = start + frames;
    for channel in &in_planar.channels {
        input_slices.push(&channel[start..end]);
    }
    Ok(resampler.process_partial(Some(input_slices.as_slice()), None)?)
}

pub(super) fn spawn_music_decoder_thread(
    path: PathBuf,
    cut: Cut,
    looping: bool,
    rate_bits: Arc<AtomicU32>,
    ring: Arc<internal::SpscRingI16>,
) -> MusicStream {
    let stop_signal = Arc::new(AtomicBool::new(false));
    let stop_signal_clone = stop_signal.clone();
    let rate_bits_clone = rate_bits.clone();

    let thread = thread::spawn(move || {
        #[cfg(windows)]
        let _thread_policy = boost_current_thread(ThreadRole::AudioDecode);
        if let Err(e) =
            music_decoder_thread_loop(path, cut, looping, rate_bits_clone, ring, stop_signal_clone)
        {
            error!("Music decoder thread failed: {e}");
        }
    });

    MusicStream {
        thread,
        stop_signal,
        rate_bits,
    }
}

#[inline]
fn secs_to_frames(seconds: f64, sample_rate: u32) -> u64 {
    if seconds.is_finite() {
        (seconds.max(0.0) * f64::from(sample_rate)).round() as u64
    } else {
        0
    }
}

#[inline]
fn volume_for_frame(position: u64, full_volume_frame: u64, silence_frame: u64) -> f32 {
    if full_volume_frame == silence_frame {
        return 1.0;
    }
    let full = full_volume_frame as f64;
    let silence = silence_frame as f64;
    let pos = position as f64;
    let denom = silence - full;
    if denom.abs() < f64::EPSILON {
        return if silence > full { 0.0 } else { 1.0 };
    }
    let volume = ((pos - full) * (0.0 - 1.0) / denom) + 1.0;
    volume.clamp(0.0, 1.0) as f32
}

fn apply_fade_envelope(
    samples: &mut [i16],
    channels: usize,
    start_frame: u64,
    fade: Option<(u64, u64)>,
) {
    let Some((full_volume_frame, silence_frame)) = fade else {
        return;
    };
    if samples.is_empty() || channels == 0 || full_volume_frame == silence_frame {
        return;
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return;
    }
    let start_volume = volume_for_frame(start_frame, full_volume_frame, silence_frame);
    let end_volume = volume_for_frame(
        start_frame + frames as u64,
        full_volume_frame,
        silence_frame,
    );
    if start_volume > 0.9999 && end_volume > 0.9999 {
        return;
    }
    let frames_f = frames as f32;
    for frame in 0..frames {
        let t = frame as f32 / frames_f;
        let mut volume = (end_volume - start_volume).mul_add(t, start_volume);
        volume = volume.clamp(0.0, 1.0);
        if (volume - 1.0).abs() < 0.0001 {
            continue;
        }
        for c in 0..channels {
            let idx = frame * channels + c;
            let scaled = f32::from(samples[idx]) * volume;
            samples[idx] = scaled.round().clamp(-32768.0, 32767.0) as i16;
        }
    }
}

fn music_decoder_thread_loop(
    path: PathBuf,
    cut: Cut,
    looping: bool,
    rate_bits: Arc<AtomicU32>,
    ring: Arc<internal::SpscRingI16>,
    stop: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let opened = decode::open_file(&path)?;
    let mut reader = opened.reader;
    let in_ch = opened.channels;
    let in_hz = opened.sample_rate_hz;

    let out_ch = ENGINE.device_channels;
    let out_hz = ENGINE.device_sample_rate;
    let queued_music_map = QUEUED_MUSIC_MAP_SEGS.clone();
    let is_ogg_stream = matches!(&reader, decode::Reader::Ogg(_));

    debug!(
        "Music decode start: {:?} ({} ch @ {} Hz) -> output {} ch @ {} Hz (rate x{}).",
        path,
        in_ch,
        in_hz,
        out_ch,
        out_hz,
        f32::from_bits(rate_bits.load(Ordering::Relaxed))
    );

    if cut.start_sec < 0.0 {
        let silence_duration_sec = -cut.start_sec;
        let silence_frames = (silence_duration_sec * f64::from(out_hz)).round() as usize;
        if silence_frames > 0 {
            let _ = push_silence_with_map(
                &ring,
                &queued_music_map,
                silence_frames,
                out_ch,
                cut.start_sec,
                1.0 / f64::from(out_hz.max(1)),
                &stop,
            )?;
        }
    }

    'main_loop: loop {
        let mut current_rate_f32 = f32::from_bits(rate_bits.load(Ordering::Relaxed));
        if !current_rate_f32.is_finite() || current_rate_f32 <= 0.0 {
            current_rate_f32 = 1.0;
        }
        let passthrough_audio = in_ch == out_ch && in_hz == out_hz;
        let mut ratio = (f64::from(out_hz) / f64::from(in_hz)) / f64::from(current_rate_f32);
        let mut resampler =
            if passthrough_audio && (current_rate_f32 - 1.0).abs() <= f32::EPSILON {
                None
            } else {
                Some(SincFixedOut::<f32>::new(
                    ratio,
                    1.0,
                    resampler_params(),
                    OUT_FRAMES_PER_CALL,
                    in_ch,
                )?)
            };

        let start_frame_f = (cut.start_sec * f64::from(in_hz)).max(0.0);
        let start_floor = start_frame_f.floor() as u64;
        // Lewton can fail on seeks that land inside the first few OGG pages.
        // Decoding and dropping <1s from the start is cheap and avoids those
        // false preview failures.
        let bypass_seek = is_ogg_stream && start_floor < u64::from(in_hz);
        let mut seek_ok = false;
        if start_floor > 0 && !bypass_seek {
            let seek_frame = start_floor.saturating_sub(internal::PREROLL_IN_FRAMES);
            match reader.seek_frame(seek_frame) {
                Ok(()) => seek_ok = true,
                Err(e) => {
                    warn!(
                        "Music seek failed for {path:?} at frame {seek_frame}; restarting from start: {e}"
                    );
                    let reopened = decode::open_file(&path)?;
                    debug_assert_eq!(reopened.channels, in_ch);
                    debug_assert_eq!(reopened.sample_rate_hz, in_hz);
                    reader = reopened.reader;
                }
            }
        }

        let mut preroll_out_frames: u64 = if seek_ok && start_floor > 0 {
            (internal::PREROLL_IN_FRAMES as f64 * ratio).ceil() as u64
        } else {
            0
        };
        let mut to_drop_in: u64 = if seek_ok { 0 } else { start_floor };
        let fade_in_frames = secs_to_frames(cut.fade_in_sec, out_hz);
        let fade_out_frames = secs_to_frames(cut.fade_out_sec, out_hz);
        let mut frames_left_out: Option<u64> = if cut.length_sec.is_finite() {
            Some((cut.length_sec * f64::from(out_hz)).round().max(0.0) as u64)
        } else {
            None
        };
        let total_frames_target = frames_left_out;
        let fade_spec = if let Some(total) = total_frames_target {
            let fade_out_frames = fade_out_frames.min(total);
            if fade_out_frames > 0 {
                Some((total.saturating_sub(fade_out_frames), total))
            } else if fade_in_frames > 0 {
                Some((fade_in_frames, 0))
            } else {
                None
            }
        } else if fade_in_frames > 0 {
            Some((fade_in_frames, 0))
        } else {
            None
        };

        let mut frames_emitted_total: u64 = 0;
        let mut next_music_output_sec = cut.start_sec.max(0.0);

        #[inline(always)]
        fn cap_out_frames(
            out_tmp: &mut Vec<i16>,
            out_ch: usize,
            frames_left_out: &mut Option<u64>,
        ) -> bool {
            if let Some(left) = frames_left_out {
                let frames = out_tmp.len() / out_ch;
                if *left == 0 {
                    out_tmp.clear();
                    return true;
                }
                if (frames as u64) > *left {
                    out_tmp.truncate((*left as usize) * out_ch);
                    *left = 0;
                    return true;
                }
                *left -= frames as u64;
            }
            false
        }

        let mut out_tmp = Vec::with_capacity(OUT_FRAMES_PER_CALL * out_ch);
        let mut in_planar = resampler
            .as_ref()
            .map(|_| PlanarAccum::new(in_ch, PLANAR_INPUT_CAP_FRAMES));
        let mut pkt_buf = Vec::new();

        loop {
            if stop.load(Ordering::Relaxed) {
                break 'main_loop;
            }
            if !reader.read_dec_packet_into(&mut pkt_buf)? {
                break;
            }
            if pkt_buf.is_empty() {
                continue;
            }
            let mut slice = &pkt_buf[..];
            if to_drop_in > 0 {
                let pkt_frames = (pkt_buf.len() / in_ch) as u64;
                if to_drop_in >= pkt_frames {
                    to_drop_in -= pkt_frames;
                    continue;
                }
                let drop_samples = (to_drop_in as usize) * in_ch;
                slice = &pkt_buf[drop_samples..];
                to_drop_in = 0;
            }
            let desired_rate = f32::from_bits(rate_bits.load(Ordering::Relaxed));
            let mut desired_rate = if desired_rate.is_finite() && desired_rate > 0.0 {
                desired_rate
            } else {
                1.0
            };
            if (desired_rate - current_rate_f32).abs() > 0.0005 {
                desired_rate = desired_rate.clamp(0.05, 8.0);
                current_rate_f32 = desired_rate;
                ratio = (f64::from(out_hz) / f64::from(in_hz)) / f64::from(current_rate_f32);
                resampler = Some(SincFixedOut::<f32>::new(
                    ratio,
                    1.0,
                    resampler_params(),
                    OUT_FRAMES_PER_CALL,
                    in_ch,
                )?);
                if in_planar.is_none() {
                    in_planar = Some(PlanarAccum::new(in_ch, PLANAR_INPUT_CAP_FRAMES));
                }
            }
            if resampler.is_none() {
                let music_sec_per_frame = 1.0 / f64::from(out_hz.max(1));
                let mut direct = slice;
                if preroll_out_frames > 0 {
                    let frames = direct.len() / out_ch;
                    let drop_frames = (preroll_out_frames as usize).min(frames);
                    let drop_samples = drop_frames * out_ch;
                    if drop_samples > 0 {
                        direct = &direct[drop_samples..];
                        preroll_out_frames = preroll_out_frames.saturating_sub(drop_frames as u64);
                        next_music_output_sec += drop_frames as f64 * music_sec_per_frame;
                    }
                }
                let mut finished = false;
                let frames = direct.len() / out_ch;
                if let Some(left) = &mut frames_left_out {
                    if *left == 0 {
                        direct = &[];
                        finished = true;
                    } else if (frames as u64) > *left {
                        direct = &direct[..(*left as usize) * out_ch];
                        *left = 0;
                        finished = true;
                    } else {
                        *left -= frames as u64;
                    }
                }
                if !direct.is_empty() {
                    let frames = (direct.len() / out_ch) as u64;
                    if fade_spec.is_some() {
                        out_tmp.clear();
                        out_tmp.extend_from_slice(direct);
                        apply_fade_envelope(&mut out_tmp, out_ch, frames_emitted_total, fade_spec);
                        frames_emitted_total = frames_emitted_total.saturating_add(frames);
                        next_music_output_sec = push_music_block_with_map(
                            &ring,
                            &queued_music_map,
                            &out_tmp,
                            out_ch,
                            next_music_output_sec,
                            music_sec_per_frame,
                            &stop,
                        )?;
                    } else {
                        frames_emitted_total = frames_emitted_total.saturating_add(frames);
                        next_music_output_sec = push_music_block_with_map(
                            &ring,
                            &queued_music_map,
                            direct,
                            out_ch,
                            next_music_output_sec,
                            music_sec_per_frame,
                            &stop,
                        )?;
                    }
                }
                if finished {
                    break;
                }
                continue;
            }
            let in_planar = in_planar
                .as_mut()
                .expect("resampler mode must keep planar input");
            let resampler = resampler
                .as_mut()
                .expect("resampler mode must keep a resampler");
            in_planar.push_i16_interleaved(slice, in_ch);
            loop {
                let need = resampler.input_frames_next();
                if in_planar.available_frames() < need {
                    break;
                }
                let out = process_resampler_block(resampler, in_planar, need)?;
                in_planar.consume_frames(need);
                if out.is_empty() {
                    break;
                }
                let produced_frames = write_resampler_output(&out, out_ch, &mut out_tmp);
                if produced_frames == 0 {
                    break;
                }
                let music_sec_per_frame =
                    (need as f64 / f64::from(in_hz.max(1))) / produced_frames as f64;
                if preroll_out_frames > 0 {
                    let drop_frames = (preroll_out_frames as usize).min(produced_frames);
                    let drop_samples = drop_frames * out_ch;
                    if drop_samples > 0 {
                        drop_front_samples(&mut out_tmp, drop_samples);
                        preroll_out_frames = preroll_out_frames.saturating_sub(drop_frames as u64);
                        next_music_output_sec += drop_frames as f64 * music_sec_per_frame;
                    }
                }
                let finished = cap_out_frames(&mut out_tmp, out_ch, &mut frames_left_out);
                if !out_tmp.is_empty() {
                    apply_fade_envelope(&mut out_tmp, out_ch, frames_emitted_total, fade_spec);
                    frames_emitted_total =
                        frames_emitted_total.saturating_add((out_tmp.len() / out_ch) as u64);
                    next_music_output_sec = push_music_block_with_map(
                        &ring,
                        &queued_music_map,
                        &out_tmp,
                        out_ch,
                        next_music_output_sec,
                        music_sec_per_frame,
                        &stop,
                    )?;
                }
                if finished {
                    break;
                }
            }
            if matches!(frames_left_out, Some(0)) {
                break;
            }
        }

        if let Some(resampler) = &mut resampler {
            let in_planar = in_planar
                .as_mut()
                .expect("resampler mode must keep planar input");
            if !in_planar.is_empty() {
                let remain = in_planar.available_frames();
                let out = process_resampler_partial(resampler, in_planar, remain)?;
                in_planar.clear();
                if !out.is_empty() {
                    let produced_frames = write_resampler_output(&out, out_ch, &mut out_tmp);
                    let music_sec_per_frame = if produced_frames == 0 {
                        0.0
                    } else {
                        (remain as f64 / f64::from(in_hz.max(1))) / produced_frames as f64
                    };
                    if preroll_out_frames > 0 {
                        let drop_frames = (preroll_out_frames as usize).min(produced_frames);
                        let drop_samples = drop_frames * out_ch;
                        if drop_samples > 0 {
                            drop_front_samples(&mut out_tmp, drop_samples);
                            next_music_output_sec += drop_frames as f64 * music_sec_per_frame;
                        }
                    }
                    let _ = cap_out_frames(&mut out_tmp, out_ch, &mut frames_left_out);
                    if !out_tmp.is_empty() {
                        apply_fade_envelope(&mut out_tmp, out_ch, frames_emitted_total, fade_spec);
                        frames_emitted_total =
                            frames_emitted_total.saturating_add((out_tmp.len() / out_ch) as u64);
                        next_music_output_sec = push_music_block_with_map(
                            &ring,
                            &queued_music_map,
                            &out_tmp,
                            out_ch,
                            next_music_output_sec,
                            music_sec_per_frame,
                            &stop,
                        )?;
                    }
                }
            }

            let out_tail = resampler.process_partial::<&[f32]>(None, None)?;
            if !out_tail.is_empty() {
                let _produced_frames = write_resampler_output(&out_tail, out_ch, &mut out_tmp);
                let music_sec_per_frame = f64::from(current_rate_f32) / f64::from(out_hz.max(1));
                let _ = cap_out_frames(&mut out_tmp, out_ch, &mut frames_left_out);
                if !out_tmp.is_empty() {
                    apply_fade_envelope(&mut out_tmp, out_ch, frames_emitted_total, fade_spec);
                    next_music_output_sec = push_music_block_with_map(
                        &ring,
                        &queued_music_map,
                        &out_tmp,
                        out_ch,
                        next_music_output_sec,
                        music_sec_per_frame,
                        &stop,
                    )?;
                }
            }
        }

        if !looping || stop.load(Ordering::Relaxed) {
            break 'main_loop;
        }
        match decode::open_file(&path) {
            Ok(reopened) => {
                debug!("Looping music: restarted {path:?}");
                reader = reopened.reader;
            }
            Err(_) => {
                warn!("Could not reopen audio stream for looping: {path:?}");
                break 'main_loop;
            }
        }
    }
    Ok(())
}

pub(super) fn load_and_resample_sfx(
    path: &str,
) -> Result<Arc<Vec<i16>>, Box<dyn std::error::Error + Send + Sync>> {
    let opened = decode::open_file(Path::new(path))?;
    let mut reader = opened.reader;
    let in_ch = opened.channels;
    let in_hz = opened.sample_rate_hz;
    let out_ch = ENGINE.device_channels;
    let out_hz = ENGINE.device_sample_rate;

    if in_ch == out_ch && in_hz == out_hz {
        let mut pkt_buf = Vec::new();
        let mut decoded_data = Vec::new();
        while reader.read_dec_packet_into(&mut pkt_buf)? {
            if !pkt_buf.is_empty() {
                decoded_data.extend_from_slice(&pkt_buf);
            }
        }
        return Ok(Arc::new(decoded_data));
    }

    let ratio = f64::from(out_hz) / f64::from(in_hz);
    let mut resampler =
        SincFixedOut::<f32>::new(ratio, 1.0, resampler_params(), OUT_FRAMES_PER_CALL, in_ch)?;

    let mut in_planar = PlanarAccum::new(in_ch, PLANAR_INPUT_CAP_FRAMES);
    let mut out_tmp = Vec::with_capacity(OUT_FRAMES_PER_CALL * out_ch);
    let mut pkt_buf = Vec::new();
    let mut resampled_data = Vec::new();

    while reader.read_dec_packet_into(&mut pkt_buf)? {
        if pkt_buf.is_empty() {
            continue;
        }
        in_planar.push_i16_interleaved(&pkt_buf, in_ch);
        loop {
            let need = resampler.input_frames_next();
            if in_planar.available_frames() < need {
                break;
            }
            let out = process_resampler_block(&mut resampler, &in_planar, need)?;
            in_planar.consume_frames(need);
            if out.is_empty() {
                break;
            }
            write_resampler_output(&out, out_ch, &mut out_tmp);
            resampled_data.extend_from_slice(&out_tmp);
        }
    }

    if !in_planar.is_empty() {
        let remain = in_planar.available_frames();
        let out = process_resampler_partial(&mut resampler, &in_planar, remain)?;
        if !out.is_empty() {
            write_resampler_output(&out, out_ch, &mut out_tmp);
            resampled_data.extend_from_slice(&out_tmp);
        }
        in_planar.clear();
    }

    let out_tail = resampler.process_partial::<&[f32]>(None, None)?;
    if !out_tail.is_empty() {
        write_resampler_output(&out_tail, out_ch, &mut out_tmp);
        resampled_data.extend_from_slice(&out_tmp);
    }
    Ok(Arc::new(resampled_data))
}
