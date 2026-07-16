#![forbid(unsafe_code)]

mod clock;
mod music_map;
mod runtime;
mod sfx_cache;
mod stream_runtime;
mod stretch;

#[cfg(windows)]
use deadlib_platform::windows_rt::{ThreadRole, boost_current_thread};
pub use deadsync_audio::{
    Cut, InitConfig, MusicStreamClockSnapshot, OutputDeviceInfo, OutputTimingSnapshot,
};
use deadsync_audio::{MusicBlockTiming, MusicBlockWriter};
use deadsync_audio_decode as decode;
use deadsync_audio_decode::resample::{
    OUT_FRAMES_PER_CALL, PLANAR_INPUT_CAP_FRAMES, PlanarAccum, apply_fade_envelope,
    drop_front_samples, resampler_params, saturating_i64_from_u64, write_channel_mapped_i16,
    write_resampler_output,
};
use log::{debug, error, warn};
use rubato::{Resampler, SincFixedOut};
use smallvec::SmallVec;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::thread;

pub use clock::{music_stream_clock_snapshot, timing_diag_enabled};
pub use music_map::{
    assist_tick_stream_frame_for_music_seconds, clear_music_pos_map, force_music_map_runtime,
    lookup_music_position,
};
#[cfg(target_os = "linux")]
pub use runtime::available_linux_backends;
pub use runtime::{
    assist_sfx_generation, collect_stutter_diag_events, get_music_stream_clock_snapshot,
    get_music_stream_position_seconds, get_output_timing_snapshot, init, is_initialized,
    play_assist_tick, play_music, play_preloaded_assist_tick, play_preloaded_sfx,
    play_scheduled_assist_tick, play_screen_sfx, play_sfx, preload_sfx, preserve_pitch_enabled,
    replaygain_enabled, set_music_rate, set_preserve_pitch_enabled, set_replaygain_enabled,
    startup_output_devices, stop_music, stop_screen_sfx, stutter_diag_trigger_seq,
    timing_diag_last_callback_gap_ns,
};
pub use sfx_cache::SfxCache;
pub use stream_runtime::{MusicStreamRuntime, StreamCommand};
use stretch::SolaStretcher;

const SILENCE_CHUNK_FRAMES: usize = 2048;
const MIN_MUSIC_RATE: f32 = 0.05;
const MAX_MUSIC_RATE: f32 = 8.0;
const MAX_PACKET_START_SNAP_SEC: f64 = 0.25;
const RESAMPLE_MAX_RELATIVE_RATIO: f64 = 64.0;
/// Threshold used everywhere to decide whether a music `rate` value is
/// "effectively 1.0". Picked to be smaller than any musically meaningful
/// rate change but well above the noise of `f32` round-tripping through
/// atomic storage. Used to gate SOLA activation, direct-audio passthrough,
/// and resampler/SOLA rebuilds, so all three stay in agreement (otherwise
/// a rate like 1.0001 could activate SOLA without triggering rebuild).
const RATE_EPS: f32 = 0.0005;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputFormat {
    pub sample_rate_hz: u32,
    pub channels: usize,
}

pub struct MusicDecodeContext {
    pub output: OutputFormat,
    pub generation: u64,
}

/// A handle to a streaming music track.
pub struct MusicStream {
    pub thread: thread::JoinHandle<MusicBlockWriter>,
    pub stop_signal: Arc<AtomicBool>,
    pub rate_bits: Arc<AtomicU32>,
    pub preserve_pitch: Arc<AtomicBool>,
    pub generation: Arc<std::sync::atomic::AtomicU64>,
}

pub fn snap_music_start_sec(path: &Path, start_sec: f64) -> f64 {
    let Ok(Some(snapped)) = decode::snap_start_forward_to_packet(path, start_sec) else {
        return start_sec;
    };
    if snapped < start_sec || snapped - start_sec > MAX_PACKET_START_SNAP_SEC {
        return start_sec;
    }
    if (snapped - start_sec).abs() > f64::EPSILON {
        debug!("Snapped music cut start from {start_sec:.6}s to packet boundary {snapped:.6}s.");
    }
    snapped
}

fn push_music_block(
    writer: &mut MusicBlockWriter,
    block: &[i16],
    out_channels: usize,
    timing: MusicBlockTiming,
    generation_source: &AtomicU64,
    stop: &AtomicBool,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    if block.is_empty() || out_channels == 0 {
        return Ok(timing.music_start_sec);
    }
    debug_assert_eq!(writer.channels(), out_channels);
    let mut sample_offset = 0usize;
    let mut next_music_sec = timing.music_start_sec;
    while sample_offset < block.len() {
        if stop.load(Ordering::Relaxed) {
            return Ok(next_music_sec);
        }
        if generation_source.load(Ordering::Acquire) != timing.generation {
            let remaining_frames = (block.len() - sample_offset) / out_channels;
            return Ok(next_music_sec + remaining_frames as f64 * timing.music_sec_per_frame);
        }
        let pushed = writer.try_push(
            &block[sample_offset..],
            MusicBlockTiming {
                generation: timing.generation,
                music_start_sec: next_music_sec,
                music_sec_per_frame: timing.music_sec_per_frame,
            },
        );
        if pushed == 0 {
            thread::sleep(std::time::Duration::from_micros(300));
            continue;
        }
        let chunk_frames = pushed / out_channels;
        sample_offset += pushed;
        next_music_sec += chunk_frames as f64 * timing.music_sec_per_frame;
    }
    Ok(next_music_sec)
}

fn push_silence(
    writer: &mut MusicBlockWriter,
    silence_frames: usize,
    out_channels: usize,
    generation_source: &AtomicU64,
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
        let generation = generation_source.load(Ordering::Acquire);
        next_music_sec = push_music_block(
            writer,
            &silence_chunk[..chunk_samples],
            out_channels,
            MusicBlockTiming {
                generation,
                music_start_sec: next_music_sec,
                music_sec_per_frame,
            },
            generation_source,
            stop,
        )?;
        frames_left -= chunk_frames;
    }
    Ok(next_music_sec)
}

pub fn spawn_music_decoder_thread(
    path: PathBuf,
    cut: Cut,
    looping: bool,
    rate_bits: Arc<AtomicU32>,
    preserve_pitch: Arc<AtomicBool>,
    writer: MusicBlockWriter,
    context: MusicDecodeContext,
) -> MusicStream {
    let stop_signal = Arc::new(AtomicBool::new(false));
    let stop_signal_clone = stop_signal.clone();
    let rate_bits_clone = rate_bits.clone();
    let preserve_pitch_clone = preserve_pitch.clone();
    let generation = Arc::new(std::sync::atomic::AtomicU64::new(context.generation));
    let generation_clone = generation.clone();

    let thread = thread::spawn(move || {
        let mut writer = writer;
        // Dev/test profiles unwind and can return the writer after a decoder
        // panic. Shipped profiles abort the process per the workspace policy.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            #[cfg(windows)]
            let _thread_policy = boost_current_thread(ThreadRole::AudioDecode);
            music_decoder_thread_loop(
                path,
                cut,
                looping,
                rate_bits_clone,
                preserve_pitch_clone,
                generation_clone,
                &mut writer,
                stop_signal_clone,
                context,
            )
        }));
        match result {
            Ok(Ok(())) => {}
            Ok(Err(err)) => error!("Music decoder thread failed: {err}"),
            Err(_) => error!("Music decoder thread panicked; transport writer recovered."),
        }
        writer
    });

    MusicStream {
        thread,
        stop_signal,
        rate_bits,
        preserve_pitch,
        generation,
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
fn seek_preroll_in_frames(seek_ok: bool, start_frame: u64, seek_start_frame: u64) -> u64 {
    if seek_ok {
        start_frame.saturating_sub(seek_start_frame)
    } else {
        0
    }
}

#[inline]
fn music_output_start_sec(
    seek_ok: bool,
    seek_start_frame: u64,
    cut_start_sec: f64,
    sample_rate_hz: u32,
) -> f64 {
    if seek_ok {
        seek_start_frame as f64 / f64::from(sample_rate_hz.max(1))
    } else {
        cut_start_sec.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{music_output_start_sec, push_music_block, seek_preroll_in_frames};
    use deadsync_audio::{MusicBlockTiming, music_transport};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    #[test]
    fn seeked_map_starts_at_decoder_frame() {
        let sec = music_output_start_sec(true, 44_092, 1.0, 44_100);

        assert!((sec - (44_092.0 / 44_100.0)).abs() <= 1e-12);
    }

    #[test]
    fn preroll_drop_uses_actual_seek_distance() {
        assert_eq!(seek_preroll_in_frames(true, 44_100, 44_092), 8);
        assert_eq!(seek_preroll_in_frames(true, 44_100, 44_120), 0);
        assert_eq!(seek_preroll_in_frames(false, 44_100, 0), 0);
    }

    #[test]
    fn generation_change_cancels_a_backpressured_block() {
        const CHANNELS: usize = 2;
        const GENERATION: u64 = 7;
        const SEC_PER_FRAME: f64 = 0.125;
        const TIMEOUT: Duration = Duration::from_secs(1);

        let (mut stream, _render) = music_transport(CHANNELS);
        let full_block = vec![1; 256 * CHANNELS];
        let timing = MusicBlockTiming {
            generation: GENERATION,
            music_start_sec: 0.0,
            music_sec_per_frame: SEC_PER_FRAME,
        };
        let mut filled = 0;
        while stream.writer.try_push(&full_block, timing) != 0 {
            filled += 1;
        }
        assert!(filled > 0);

        let generation = Arc::new(AtomicU64::new(GENERATION));
        let stop = Arc::new(AtomicBool::new(false));
        let entered = Arc::new(AtomicBool::new(false));
        let generation_thread = generation.clone();
        let stop_thread = stop.clone();
        let entered_thread = entered.clone();
        let mut writer = stream.writer;
        let thread = std::thread::spawn(move || {
            entered_thread.store(true, Ordering::Release);
            let result = push_music_block(
                &mut writer,
                &[7, 8],
                CHANNELS,
                MusicBlockTiming {
                    generation: GENERATION,
                    music_start_sec: 3.25,
                    music_sec_per_frame: SEC_PER_FRAME,
                },
                &generation_thread,
                &stop_thread,
            );
            (writer, result)
        });

        while !entered.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        std::thread::sleep(Duration::from_millis(2));
        generation.store(GENERATION + 1, Ordering::Release);
        let deadline = Instant::now() + TIMEOUT;
        while !thread.is_finished() && Instant::now() < deadline {
            std::thread::yield_now();
        }
        if !thread.is_finished() {
            stop.store(true, Ordering::Release);
        }
        let (_writer, result) = thread.join().expect("producer did not panic");
        assert!(
            Instant::now() < deadline,
            "generation reset did not unblock producer"
        );
        let next_sec = result.expect("canceling a stale block succeeds");
        assert_eq!(next_sec.to_bits(), (3.25 + SEC_PER_FRAME).to_bits());
    }
}

#[allow(clippy::too_many_arguments)]
fn music_decoder_thread_loop(
    path: PathBuf,
    cut: Cut,
    looping: bool,
    rate_bits: Arc<AtomicU32>,
    preserve_pitch: Arc<AtomicBool>,
    generation: Arc<std::sync::atomic::AtomicU64>,
    writer: &mut MusicBlockWriter,
    stop: Arc<AtomicBool>,
    context: MusicDecodeContext,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let opened = decode::open_file(&path)?;
    let mut reader = opened.reader;
    let in_ch = opened.channels;
    let in_hz = opened.sample_rate_hz;

    let out_ch = context.output.channels;
    let out_hz = context.output.sample_rate_hz;
    let is_ogg_stream = matches!(&reader, decode::Reader::Ogg(_));

    debug!(
        "Music decode start: {:?} ({} ch @ {} Hz) -> output {} ch @ {} Hz (rate x{}, preserve_pitch={}).",
        path,
        in_ch,
        in_hz,
        out_ch,
        out_hz,
        f32::from_bits(rate_bits.load(Ordering::Relaxed)),
        preserve_pitch.load(Ordering::Relaxed),
    );

    if cut.start_sec < 0.0 {
        let silence_duration_sec = -cut.start_sec;
        let silence_frames = (silence_duration_sec * f64::from(out_hz)).round() as usize;
        if silence_frames > 0 {
            let _ = push_silence(
                writer,
                silence_frames,
                out_ch,
                &generation,
                cut.start_sec,
                1.0 / f64::from(out_hz.max(1)),
                &stop,
            )?;
        }
    }

    let mut resampler: Option<SincFixedOut<f32>> = None;
    let mut resampler_rate = f32::NAN;
    let mut resampler_pp = false;
    let mut resample_out: Option<Vec<Vec<f32>>> = None;
    let mut resample_in: Option<Vec<Vec<f32>>> = None;
    let mut in_planar: Option<PlanarAccum> = None;
    let mut sola: Option<SolaStretcher> = None;
    let mut out_tmp = Vec::with_capacity(OUT_FRAMES_PER_CALL * out_ch);
    let mut pkt_buf = Vec::new();

    let mut output_generation;
    'main_loop: loop {
        // Load the generation before the settings it tags. Observing a newly
        // published generation therefore also observes the preceding setting
        // stores; new settings paired with an old generation are discarded.
        output_generation = generation.load(Ordering::Acquire);
        let mut current_rate_f32 = f32::from_bits(rate_bits.load(Ordering::Acquire));
        if !current_rate_f32.is_finite() || current_rate_f32 <= 0.0 {
            current_rate_f32 = 1.0;
        } else {
            current_rate_f32 = current_rate_f32.clamp(MIN_MUSIC_RATE, MAX_MUSIC_RATE);
        }
        let mut current_pp =
            preserve_pitch.load(Ordering::Acquire) && (current_rate_f32 - 1.0).abs() > RATE_EPS;
        let direct_audio = in_hz == out_hz && (current_rate_f32 - 1.0).abs() <= RATE_EPS;
        let mut ratio = if current_pp {
            f64::from(out_hz) / f64::from(in_hz)
        } else {
            (f64::from(out_hz) / f64::from(in_hz)) / f64::from(current_rate_f32)
        };
        if direct_audio {
            resampler = None;
            resampler_rate = f32::NAN;
            resampler_pp = false;
            resample_in = None;
            resample_out = None;
            in_planar = None;
            sola = None;
        } else if resampler.is_some()
            && resample_in.is_some()
            && resample_out.is_some()
            && in_planar.is_some()
            && resampler_rate == current_rate_f32
            && resampler_pp == current_pp
        {
            let resampler = resampler.as_mut().expect("resampler exists");
            resampler.reset();
            resampler.set_resample_ratio(ratio, false)?;
            in_planar.as_mut().expect("planar input exists").clear();
            if let Some(s) = sola.as_mut() {
                s.set_speed_ratio(current_rate_f32);
                s.reset();
            }
        } else {
            let new_resampler = SincFixedOut::<f32>::new(
                ratio,
                RESAMPLE_MAX_RELATIVE_RATIO,
                resampler_params(),
                OUT_FRAMES_PER_CALL,
                in_ch,
            )?;
            resample_in = Some(new_resampler.input_buffer_allocate(true));
            resample_out = Some(new_resampler.output_buffer_allocate(true));
            in_planar = Some(PlanarAccum::new(in_ch, PLANAR_INPUT_CAP_FRAMES));
            resampler = Some(new_resampler);
            resampler_rate = current_rate_f32;
            resampler_pp = current_pp;
            sola = if current_pp {
                let mut s = SolaStretcher::new(in_ch, in_hz);
                s.set_speed_ratio(current_rate_f32);
                Some(s)
            } else {
                None
            };
        }

        let start_frame_f = (cut.start_sec * f64::from(in_hz)).max(0.0);
        let start_floor = start_frame_f.floor() as u64;
        // Lewton can fail on seeks that land inside the first few OGG pages.
        // Decoding and dropping <1s from the start is cheap and avoids those
        // false preview failures.
        let bypass_seek = is_ogg_stream && start_floor < u64::from(in_hz);
        let mut seek_ok = false;
        let mut seek_start_frame = 0u64;
        if start_floor > 0 && !bypass_seek {
            let seek_frame = start_floor.saturating_sub(deadsync_audio::ring::PREROLL_IN_FRAMES);
            match reader.seek_frame(seek_frame) {
                Ok(()) => {
                    seek_ok = true;
                    seek_start_frame = reader.current_frame();
                }
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

        let preroll_in_frames = seek_preroll_in_frames(seek_ok, start_floor, seek_start_frame);
        let mut preroll_out_frames: u64 = if preroll_in_frames > 0 {
            (preroll_in_frames as f64 * ratio).ceil() as u64
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
            if fade_out_frames > 0 {
                let total = saturating_i64_from_u64(total);
                Some((
                    total.saturating_sub(saturating_i64_from_u64(fade_out_frames)),
                    total,
                ))
            } else if fade_in_frames > 0 {
                Some((saturating_i64_from_u64(fade_in_frames), 0))
            } else {
                None
            }
        } else if fade_in_frames > 0 {
            Some((saturating_i64_from_u64(fade_in_frames), 0))
        } else {
            None
        };

        let mut frames_emitted_total: u64 = 0;
        let mut next_music_output_sec =
            music_output_start_sec(seek_ok, seek_start_frame, cut.start_sec, in_hz);

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

        out_tmp.clear();
        pkt_buf.clear();

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
            // Load the generation before the settings it tags — see note above.
            output_generation = generation.load(Ordering::Acquire);
            let desired_rate = f32::from_bits(rate_bits.load(Ordering::Acquire));
            let mut desired_rate = if desired_rate.is_finite() && desired_rate > 0.0 {
                desired_rate
            } else {
                1.0
            };
            let desired_pp_raw = preserve_pitch.load(Ordering::Acquire);
            let desired_pp_active = desired_pp_raw && (desired_rate - 1.0).abs() > RATE_EPS;
            let rate_changed = (desired_rate - current_rate_f32).abs() > RATE_EPS;
            let pp_changed = desired_pp_active != current_pp;
            if rate_changed || pp_changed {
                desired_rate = desired_rate.clamp(MIN_MUSIC_RATE, MAX_MUSIC_RATE);
                current_rate_f32 = desired_rate;
                current_pp = desired_pp_raw && (desired_rate - 1.0).abs() > RATE_EPS;
                ratio = if current_pp {
                    f64::from(out_hz) / f64::from(in_hz)
                } else {
                    (f64::from(out_hz) / f64::from(in_hz)) / f64::from(current_rate_f32)
                };
                if in_hz == out_hz && (current_rate_f32 - 1.0).abs() <= RATE_EPS {
                    resampler = None;
                    resampler_rate = f32::NAN;
                    resampler_pp = false;
                    resample_in = None;
                    resample_out = None;
                    in_planar = None;
                    sola = None;
                } else {
                    let need_rebuild = pp_changed || resampler.is_none();
                    let mut reuse_resampler = false;
                    if !need_rebuild && let Some(existing) = &mut resampler {
                        existing.reset();
                        reuse_resampler = existing.set_resample_ratio(ratio, false).is_ok();
                    }
                    if !reuse_resampler {
                        let new_resampler = SincFixedOut::<f32>::new(
                            ratio,
                            RESAMPLE_MAX_RELATIVE_RATIO,
                            resampler_params(),
                            OUT_FRAMES_PER_CALL,
                            in_ch,
                        )?;
                        resample_in = Some(new_resampler.input_buffer_allocate(true));
                        resample_out = Some(new_resampler.output_buffer_allocate(true));
                        resampler = Some(new_resampler);
                    }
                    resampler_rate = current_rate_f32;
                    resampler_pp = current_pp;
                    match in_planar.as_mut() {
                        None => {
                            in_planar = Some(PlanarAccum::new(in_ch, PLANAR_INPUT_CAP_FRAMES));
                        }
                        Some(p) if pp_changed => p.clear(),
                        _ => {}
                    }
                    if current_pp {
                        let s = sola.get_or_insert_with(|| SolaStretcher::new(in_ch, in_hz));
                        s.set_speed_ratio(current_rate_f32);
                        if pp_changed {
                            s.reset();
                        }
                    } else {
                        sola = None;
                    }
                }
            }
            if resampler.is_none() {
                let music_sec_per_frame = 1.0 / f64::from(out_hz.max(1));
                let mut direct = slice;
                if preroll_out_frames > 0 {
                    let frames = direct.len() / in_ch;
                    let drop_frames = (preroll_out_frames as usize).min(frames);
                    let drop_samples = drop_frames * in_ch;
                    if drop_samples > 0 {
                        direct = &direct[drop_samples..];
                        preroll_out_frames = preroll_out_frames.saturating_sub(drop_frames as u64);
                        next_music_output_sec += drop_frames as f64 * music_sec_per_frame;
                    }
                }
                let mut finished = false;
                let frames = direct.len() / in_ch;
                if let Some(left) = &mut frames_left_out {
                    if *left == 0 {
                        direct = &[];
                        finished = true;
                    } else if (frames as u64) > *left {
                        direct = &direct[..(*left as usize) * in_ch];
                        *left = 0;
                        finished = true;
                    } else {
                        *left -= frames as u64;
                    }
                }
                if !direct.is_empty() {
                    let frames = (direct.len() / in_ch) as u64;
                    if in_ch == out_ch && fade_spec.is_none() {
                        frames_emitted_total = frames_emitted_total.saturating_add(frames);
                        next_music_output_sec = push_music_block(
                            writer,
                            direct,
                            out_ch,
                            MusicBlockTiming {
                                generation: output_generation,
                                music_start_sec: next_music_output_sec,
                                music_sec_per_frame,
                            },
                            &generation,
                            &stop,
                        )?;
                    } else {
                        out_tmp.clear();
                        if in_ch == out_ch {
                            out_tmp.extend_from_slice(direct);
                        } else {
                            write_channel_mapped_i16(direct, in_ch, out_ch, &mut out_tmp);
                        }
                        if let Some(fade) = fade_spec {
                            apply_fade_envelope(&mut out_tmp, out_ch, frames_emitted_total, fade);
                        }
                        frames_emitted_total = frames_emitted_total.saturating_add(frames);
                        next_music_output_sec = push_music_block(
                            writer,
                            &out_tmp,
                            out_ch,
                            MusicBlockTiming {
                                generation: output_generation,
                                music_start_sec: next_music_output_sec,
                                music_sec_per_frame,
                            },
                            &generation,
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
            let resample_out = resample_out
                .as_mut()
                .expect("resampler mode must keep output buffer");
            if let Some(sola) = sola.as_mut() {
                sola.push_interleaved_i16(slice);
                loop {
                    let pull_cap =
                        PLANAR_INPUT_CAP_FRAMES.saturating_sub(in_planar.available_frames());
                    if pull_cap == 0 {
                        break;
                    }
                    // `pull` appends planar f32 directly into in_planar's backing
                    // buffers, so the stretched audio is written once (no scratch copy).
                    if sola.pull(&mut in_planar.channels, pull_cap.min(2048)) == 0 {
                        break;
                    }
                }
            } else {
                in_planar.push_i16_interleaved(slice, in_ch);
            }
            loop {
                let need = resampler.input_frames_next();
                if in_planar.available_frames() < need {
                    break;
                }
                let produced_frames = {
                    let mut input_slices =
                        SmallVec::<[&[f32]; 2]>::with_capacity(in_planar.channels.len());
                    let start = in_planar.start_frame;
                    let end = start + need;
                    for channel in &in_planar.channels {
                        input_slices.push(&channel[start..end]);
                    }
                    resampler
                        .process_into_buffer(input_slices.as_slice(), resample_out, None)?
                        .1
                };
                in_planar.consume_frames(need);
                write_resampler_output(resample_out, produced_frames, out_ch, &mut out_tmp);
                if produced_frames == 0 {
                    break;
                }
                let music_sec_per_frame = if current_pp {
                    f64::from(current_rate_f32) / f64::from(out_hz.max(1))
                } else {
                    (need as f64 / f64::from(in_hz.max(1))) / produced_frames as f64
                };
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
                    if let Some(fade) = fade_spec {
                        apply_fade_envelope(&mut out_tmp, out_ch, frames_emitted_total, fade);
                    }
                    frames_emitted_total =
                        frames_emitted_total.saturating_add((out_tmp.len() / out_ch) as u64);
                    next_music_output_sec = push_music_block(
                        writer,
                        &out_tmp,
                        out_ch,
                        MusicBlockTiming {
                            generation: output_generation,
                            music_start_sec: next_music_output_sec,
                            music_sec_per_frame,
                        },
                        &generation,
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
            let resample_in = resample_in
                .as_mut()
                .expect("resampler mode must keep input buffer");
            let resample_out = resample_out
                .as_mut()
                .expect("resampler mode must keep output buffer");
            // Drain any residual stretched output from SOLA into in_planar.
            if let Some(sola) = sola.as_mut() {
                // On the final pass (no loop restart coming) tell SOLA to flush
                // its last partial window instead of holding it back for a
                // search that will never have enough lookahead. A looping
                // decoder keeps feeding across the seam, so it must NOT flush.
                if !looping || stop.load(Ordering::Relaxed) {
                    sola.finish();
                }
                loop {
                    let pull_cap =
                        PLANAR_INPUT_CAP_FRAMES.saturating_sub(in_planar.available_frames());
                    if pull_cap == 0 {
                        break;
                    }
                    if sola.pull(&mut in_planar.channels, pull_cap.min(2048)) == 0 {
                        break;
                    }
                }
            }
            if !in_planar.is_empty() {
                let remain = in_planar.available_frames();
                let need = resampler.input_frames_next();
                let copy_frames = remain.min(need);
                let start = in_planar.start_frame;
                let end = start + copy_frames;
                for (dst, channel) in resample_in.iter_mut().zip(&in_planar.channels) {
                    dst[..need].fill(0.0);
                    dst[..copy_frames].copy_from_slice(&channel[start..end]);
                }
                let produced_frames = resampler
                    .process_into_buffer(resample_in.as_slice(), resample_out, None)?
                    .1;
                in_planar.clear();
                if produced_frames > 0 {
                    let produced_frames =
                        write_resampler_output(resample_out, produced_frames, out_ch, &mut out_tmp);
                    let music_sec_per_frame = if produced_frames == 0 {
                        0.0
                    } else if current_pp {
                        f64::from(current_rate_f32) / f64::from(out_hz.max(1))
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
                        if let Some(fade) = fade_spec {
                            apply_fade_envelope(&mut out_tmp, out_ch, frames_emitted_total, fade);
                        }
                        frames_emitted_total =
                            frames_emitted_total.saturating_add((out_tmp.len() / out_ch) as u64);
                        next_music_output_sec = push_music_block(
                            writer,
                            &out_tmp,
                            out_ch,
                            MusicBlockTiming {
                                generation: output_generation,
                                music_start_sec: next_music_output_sec,
                                music_sec_per_frame,
                            },
                            &generation,
                            &stop,
                        )?;
                    }
                }
            }

            let need = resampler.input_frames_next();
            for dst in resample_in.iter_mut() {
                dst[..need].fill(0.0);
            }
            let produced_frames = resampler
                .process_into_buffer(resample_in.as_slice(), resample_out, None)?
                .1;
            if produced_frames > 0 {
                let _produced_frames =
                    write_resampler_output(resample_out, produced_frames, out_ch, &mut out_tmp);
                let music_sec_per_frame = f64::from(current_rate_f32) / f64::from(out_hz.max(1));
                let _ = cap_out_frames(&mut out_tmp, out_ch, &mut frames_left_out);
                if !out_tmp.is_empty() {
                    if let Some(fade) = fade_spec {
                        apply_fade_envelope(&mut out_tmp, out_ch, frames_emitted_total, fade);
                    }
                    let _ = push_music_block(
                        writer,
                        &out_tmp,
                        out_ch,
                        MusicBlockTiming {
                            generation: output_generation,
                            music_start_sec: next_music_output_sec,
                            music_sec_per_frame,
                        },
                        &generation,
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

pub fn load_and_resample_sfx(
    path: &str,
    output: OutputFormat,
) -> Result<Arc<[i16]>, Box<dyn std::error::Error + Send + Sync>> {
    let opened = decode::open_file(Path::new(path))?;
    let mut reader = opened.reader;
    let in_ch = opened.channels;
    let in_hz = opened.sample_rate_hz;
    let out_ch = output.channels;
    let out_hz = output.sample_rate_hz;

    if in_hz == out_hz {
        let mut pkt_buf = Vec::new();
        let mut decoded_data = Vec::new();
        let mut out_tmp = Vec::new();
        while reader.read_dec_packet_into(&mut pkt_buf)? {
            if !pkt_buf.is_empty() {
                if in_ch == out_ch {
                    decoded_data.extend_from_slice(&pkt_buf);
                } else {
                    write_channel_mapped_i16(&pkt_buf, in_ch, out_ch, &mut out_tmp);
                    decoded_data.extend_from_slice(&out_tmp);
                }
            }
        }
        return Ok(Arc::from(decoded_data.into_boxed_slice()));
    }

    let ratio = f64::from(out_hz) / f64::from(in_hz);
    let mut resampler =
        SincFixedOut::<f32>::new(ratio, 1.0, resampler_params(), OUT_FRAMES_PER_CALL, in_ch)?;

    let mut in_planar = PlanarAccum::new(in_ch, PLANAR_INPUT_CAP_FRAMES);
    let mut resample_in = resampler.input_buffer_allocate(true);
    let mut resample_out = resampler.output_buffer_allocate(true);
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
            let produced_frames = {
                let mut input_slices =
                    SmallVec::<[&[f32]; 2]>::with_capacity(in_planar.channels.len());
                let start = in_planar.start_frame;
                let end = start + need;
                for channel in &in_planar.channels {
                    input_slices.push(&channel[start..end]);
                }
                resampler
                    .process_into_buffer(input_slices.as_slice(), &mut resample_out, None)?
                    .1
            };
            in_planar.consume_frames(need);
            if produced_frames == 0 {
                break;
            }
            write_resampler_output(&resample_out, produced_frames, out_ch, &mut out_tmp);
            resampled_data.extend_from_slice(&out_tmp);
        }
    }

    if !in_planar.is_empty() {
        let remain = in_planar.available_frames();
        let need = resampler.input_frames_next();
        let copy_frames = remain.min(need);
        let start = in_planar.start_frame;
        let end = start + copy_frames;
        for (dst, channel) in resample_in.iter_mut().zip(&in_planar.channels) {
            dst[..need].fill(0.0);
            dst[..copy_frames].copy_from_slice(&channel[start..end]);
        }
        let produced_frames = resampler
            .process_into_buffer(resample_in.as_slice(), &mut resample_out, None)?
            .1;
        if produced_frames > 0 {
            write_resampler_output(&resample_out, produced_frames, out_ch, &mut out_tmp);
            resampled_data.extend_from_slice(&out_tmp);
        }
        in_planar.clear();
    }

    let need = resampler.input_frames_next();
    for dst in &mut resample_in {
        dst[..need].fill(0.0);
    }
    let produced_frames = resampler
        .process_into_buffer(resample_in.as_slice(), &mut resample_out, None)?
        .1;
    if produced_frames > 0 {
        write_resampler_output(&resample_out, produced_frames, out_ch, &mut out_tmp);
        resampled_data.extend_from_slice(&out_tmp);
    }
    Ok(Arc::from(resampled_data.into_boxed_slice()))
}
