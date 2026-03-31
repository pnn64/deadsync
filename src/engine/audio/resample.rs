use super::{Cut, ENGINE, MusicMapSeg, MusicStream, QUEUED_MUSIC_MAP_SEGS, internal};
use crate::engine::audio::decode;
#[cfg(windows)]
use crate::engine::windows_rt::{ThreadRole, boost_current_thread};
use log::{debug, error, warn};
use rubato::{
    Resampler, SincFixedOut, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;

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
        let silence_samples =
            (silence_duration_sec * f64::from(out_hz) * out_ch as f64).round() as usize;
        if silence_samples > 0 {
            let silence_buf = vec![0i16; silence_samples];
            let _ = push_music_block_with_map(
                &ring,
                &queued_music_map,
                &silence_buf,
                out_ch,
                cut.start_sec,
                1.0 / f64::from(out_hz.max(1)),
                &stop,
            )?;
        }
    }

    'main_loop: loop {
        const OUT_FRAMES_PER_CALL: usize = 256;
        let mut current_rate_f32 = f32::from_bits(rate_bits.load(Ordering::Relaxed));
        if !current_rate_f32.is_finite() || current_rate_f32 <= 0.0 {
            current_rate_f32 = 1.0;
        }
        let mut ratio = (f64::from(out_hz) / f64::from(in_hz)) / f64::from(current_rate_f32);
        let mut resampler = SincFixedOut::<f32>::new(
            ratio,
            1.0,
            SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 128,
                window: WindowFunction::BlackmanHarris2,
            },
            OUT_FRAMES_PER_CALL,
            in_ch,
        )?;

        let start_frame_f = (cut.start_sec * f64::from(in_hz)).max(0.0);
        let start_floor = start_frame_f.floor() as u64;
        let mut seek_ok = true;
        if start_floor > 0 {
            let seek_frame = start_floor.saturating_sub(internal::PREROLL_IN_FRAMES);
            if reader.seek_frame(seek_frame).is_err() {
                seek_ok = false;
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

        let mut out_tmp: Vec<i16> = Vec::with_capacity(OUT_FRAMES_PER_CALL * out_ch);
        let mut in_planar: Vec<Vec<f32>> = vec![Vec::with_capacity(4096); in_ch];

        #[inline(always)]
        fn deinterleave_accum(planar: &mut [Vec<f32>], interleaved: &[i16], channels: usize) {
            let frames = interleaved.len() / channels;
            for f in 0..frames {
                let base = f * channels;
                for c in 0..channels {
                    planar[c].push(f32::from(interleaved[base + c]) / 32768.0);
                }
            }
        }

        #[inline(always)]
        fn try_produce_blocks(
            resampler: &mut SincFixedOut<f32>,
            in_planar: &mut [Vec<f32>],
            in_hz: u32,
            out_ch: usize,
            out_tmp: &mut Vec<i16>,
            preroll_out_frames: &mut u64,
            frames_emitted_total: &mut u64,
            fade_spec: Option<(u64, u64)>,
            frames_left_out: &mut Option<u64>,
            next_music_output_sec: &mut f64,
            ring: &internal::SpscRingI16,
            seg_ring: &internal::SpscRingMusicSeg,
            stop: &AtomicBool,
        ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
            let mut produced_any = false;
            loop {
                let need = resampler.input_frames_next();
                if in_planar.iter().any(|ch| ch.len() < need) {
                    break;
                }
                let mut input_slices: Vec<&[f32]> = Vec::with_capacity(in_planar.len());
                for ch in in_planar.iter() {
                    input_slices.push(&ch[..need]);
                }
                let out = resampler.process(&input_slices, None)?;
                for ch in in_planar.iter_mut() {
                    ch.drain(0..need);
                }
                if out.is_empty() {
                    break;
                }
                let produced_frames = out[0].len();
                if produced_frames == 0 {
                    break;
                }
                let music_sec_per_frame =
                    (need as f64 / f64::from(in_hz.max(1))) / produced_frames as f64;
                out_tmp.clear();
                out_tmp.reserve(produced_frames * out_ch);
                for (f, _) in out[0].iter().enumerate() {
                    for c in 0..out_ch {
                        let src = out[c % out.len()][f];
                        let s = (src * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                        out_tmp.push(s);
                    }
                }
                if *preroll_out_frames > 0 {
                    let frames = out_tmp.len() / out_ch;
                    let drop_frames = (*preroll_out_frames as usize).min(frames);
                    let drop_samples = drop_frames * out_ch;
                    if drop_samples > 0 {
                        out_tmp.drain(0..drop_samples);
                        *preroll_out_frames =
                            (*preroll_out_frames).saturating_sub(drop_frames as u64);
                        *next_music_output_sec += drop_frames as f64 * music_sec_per_frame;
                    }
                }
                let mut finished = false;
                if let Some(left) = frames_left_out {
                    let frames = out_tmp.len() / out_ch;
                    if *left == 0 {
                        out_tmp.clear();
                        finished = true;
                    } else if (frames as u64) > *left {
                        out_tmp.truncate((*left as usize) * out_ch);
                        *left = 0;
                        finished = true;
                    } else {
                        *left -= frames as u64;
                    }
                }
                if !out_tmp.is_empty() {
                    apply_fade_envelope(out_tmp, out_ch, *frames_emitted_total, fade_spec);
                    *frames_emitted_total =
                        (*frames_emitted_total).saturating_add((out_tmp.len() / out_ch) as u64);
                    *next_music_output_sec = push_music_block_with_map(
                        ring,
                        seg_ring,
                        out_tmp,
                        out_ch,
                        *next_music_output_sec,
                        music_sec_per_frame,
                        stop,
                    )?;
                    produced_any = true;
                }
                if finished {
                    break;
                }
            }
            Ok(produced_any)
        }

        while let Ok(pkt_opt) = reader.read_dec_packet_itl() {
            if stop.load(Ordering::Relaxed) {
                break 'main_loop;
            }
            let p = match pkt_opt {
                Some(p) if !p.is_empty() => p,
                Some(_) => continue,
                None => break,
            };
            let mut slice = &p[..];
            if to_drop_in > 0 {
                let pkt_frames = (p.len() / in_ch) as u64;
                if to_drop_in >= pkt_frames {
                    to_drop_in -= pkt_frames;
                    continue;
                }
                let drop_samples = (to_drop_in as usize) * in_ch;
                slice = &p[drop_samples..];
                to_drop_in = 0;
            }
            deinterleave_accum(&mut in_planar, slice, in_ch);
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
                resampler = SincFixedOut::<f32>::new(
                    ratio,
                    1.0,
                    SincInterpolationParameters {
                        sinc_len: 256,
                        f_cutoff: 0.95,
                        interpolation: SincInterpolationType::Linear,
                        oversampling_factor: 128,
                        window: WindowFunction::BlackmanHarris2,
                    },
                    OUT_FRAMES_PER_CALL,
                    in_ch,
                )?;
            }
            let _ = try_produce_blocks(
                &mut resampler,
                &mut in_planar,
                in_hz,
                out_ch,
                &mut out_tmp,
                &mut preroll_out_frames,
                &mut frames_emitted_total,
                fade_spec,
                &mut frames_left_out,
                &mut next_music_output_sec,
                &ring,
                &queued_music_map,
                &stop,
            )?;
            if matches!(frames_left_out, Some(0)) {
                break;
            }
        }

        if !in_planar.iter().all(Vec::is_empty) {
            let mut input_slices: Vec<&[f32]> = Vec::with_capacity(in_planar.len());
            let remain = in_planar.iter().map(Vec::len).min().unwrap_or(0);
            for ch in &in_planar {
                input_slices.push(&ch[..remain]);
            }
            let out = resampler.process_partial(Some(&input_slices), None)?;
            for ch in &mut in_planar {
                ch.clear();
            }
            if !out.is_empty() {
                let produced_frames = out[0].len();
                let music_sec_per_frame = if produced_frames == 0 {
                    0.0
                } else {
                    (remain as f64 / f64::from(in_hz.max(1))) / produced_frames as f64
                };
                out_tmp.clear();
                out_tmp.reserve(produced_frames * out_ch);
                for (f, _) in out[0].iter().enumerate() {
                    for c in 0..out_ch {
                        let v = out[c % out.len()][f];
                        let s = (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                        out_tmp.push(s);
                    }
                }
                if preroll_out_frames > 0 {
                    let frames = out_tmp.len() / out_ch;
                    let drop_frames = (preroll_out_frames as usize).min(frames);
                    let drop_samples = drop_frames * out_ch;
                    if drop_samples > 0 {
                        out_tmp.drain(0..drop_samples);
                        next_music_output_sec += drop_frames as f64 * music_sec_per_frame;
                    }
                }
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
            let produced_frames = out_tail[0].len();
            let music_sec_per_frame = f64::from(current_rate_f32) / f64::from(out_hz.max(1));
            out_tmp.clear();
            out_tmp.reserve(produced_frames * out_ch);
            for (f, _) in out_tail[0].iter().enumerate() {
                for c in 0..out_ch {
                    let v = out_tail[c % out_tail.len()][f];
                    let s = (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                    out_tmp.push(s);
                }
            }
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

    const OUT_FRAMES_PER_CALL: usize = 256;
    let ratio = f64::from(out_hz) / f64::from(in_hz);
    let iparams = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler = SincFixedOut::<f32>::new(ratio, 1.0, iparams, OUT_FRAMES_PER_CALL, in_ch)?;

    let mut in_planar: Vec<Vec<f32>> = vec![Vec::with_capacity(4096); in_ch];
    let mut resampled_data: Vec<i16> = Vec::new();

    while let Some(pck_samples) = reader.read_dec_packet_itl()? {
        if pck_samples.is_empty() {
            continue;
        }
        let frames = pck_samples.len() / in_ch;
        for f in 0..frames {
            let base = f * in_ch;
            for c in 0..in_ch {
                in_planar[c].push(f32::from(pck_samples[base + c]) / 32768.0);
            }
        }
        loop {
            let need = resampler.input_frames_next();
            if in_planar.iter().any(|ch| ch.len() < need) {
                break;
            }
            let mut input_slices: Vec<&[f32]> = Vec::with_capacity(in_planar.len());
            for ch in &in_planar {
                input_slices.push(&ch[..need]);
            }
            let out = resampler.process(&input_slices, None)?;
            for ch in &mut in_planar {
                ch.drain(0..need);
            }
            if out.is_empty() {
                break;
            }
            let produced_frames = out[0].len();
            resampled_data.reserve(produced_frames * out_ch);
            for (f, _) in out[0].iter().enumerate() {
                for c in 0..out_ch {
                    let v = out[c % out.len()][f];
                    let s = (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                    resampled_data.push(s);
                }
            }
        }
    }

    if !in_planar.iter().all(Vec::is_empty) {
        let remain = in_planar.iter().map(Vec::len).min().unwrap_or(0);
        let mut input_slices: Vec<&[f32]> = Vec::with_capacity(in_planar.len());
        for ch in &in_planar {
            input_slices.push(&ch[..remain]);
        }
        let out = resampler.process_partial(Some(&input_slices), None)?;
        if !out.is_empty() {
            let produced_frames = out[0].len();
            resampled_data.reserve(produced_frames * out_ch);
            for (f, _) in out[0].iter().enumerate() {
                for c in 0..out_ch {
                    let v = out[c % out.len()][f];
                    let s = (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                    resampled_data.push(s);
                }
            }
        }
        for ch in &mut in_planar {
            ch.clear();
        }
    }

    let out_tail = resampler.process_partial::<&[f32]>(None, None)?;
    if !out_tail.is_empty() {
        let produced_frames = out_tail[0].len();
        resampled_data.reserve(produced_frames * out_ch);
        for (f, _) in out_tail[0].iter().enumerate() {
            for c in 0..out_ch {
                let v = out_tail[c % out_tail.len()][f];
                let s = (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                resampled_data.push(s);
            }
        }
    }
    Ok(Arc::new(resampled_data))
}
