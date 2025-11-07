use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, StreamConfig};
use lewton::inside_ogg::OggStreamReader;
use log::{error, info, warn};
use once_cell::sync::Lazy;
use rubato::{Resampler, SincFixedOut, SincInterpolationParameters, SincInterpolationType, WindowFunction};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

/* ============================== Public API ============================== */

#[derive(Clone, Copy, Debug)]
pub struct Cut {
    pub start_sec: f64,
    pub length_sec: f64,
    pub fade_in_sec: f64,
    pub fade_out_sec: f64,
}
impl Default for Cut {
    fn default() -> Self {
        Self {
            start_sec: 0.0,
            length_sec: f64::INFINITY,
            fade_in_sec: 0.0,
            fade_out_sec: 0.0,
        }
    }
}

// Commands to the audio engine
enum AudioCommand {
    PlaySfx(Arc<Vec<i16>>),
    PlayMusic(PathBuf, Cut, bool), // bool is for looping
    StopMusic,
}

// Global engine (initialized once)
static ENGINE: Lazy<AudioEngine> = Lazy::new(init_engine_and_thread);

struct AudioEngine {
    command_sender: Sender<AudioCommand>,
    sfx_cache: Mutex<HashMap<String, Arc<Vec<i16>>>>,
    device_sample_rate: u32,
    device_channels: usize,
}

/// A handle to a streaming music track.
struct MusicStream {
    thread: thread::JoinHandle<()>,
    stop_signal: Arc<std::sync::atomic::AtomicBool>,
}

/* ============================ Public functions ============================ */

/// Initializes the audio engine. Must be called once at startup.
pub fn init() -> Result<(), String> {
    Lazy::force(&ENGINE);
    Ok(())
}

/// Plays a sound effect from the given path (cached after first load).
pub fn play_sfx(path: &str) {
    let sound_data = {
        let mut cache = ENGINE.sfx_cache.lock().unwrap();
        if let Some(data) = cache.get(path) {
            data.clone()
        } else {
            match load_and_resample_sfx(path) {
                Ok(data) => {
                    cache.insert(path.to_string(), data.clone());
                    info!("Cached SFX: {}", path);
                    data
                }
                Err(e) => {
                    warn!("Failed to load SFX '{}': {}", path, e);
                    return;
                }
            }
        }
    };
    let _ = ENGINE.command_sender.send(AudioCommand::PlaySfx(sound_data));
}

/// Plays a music track from a file path.
pub fn play_music(path: PathBuf, cut: Cut, looping: bool) {
    let _ = ENGINE.command_sender.send(AudioCommand::PlayMusic(path, cut, looping));
}

/// Stops the currently playing music track.
pub fn stop_music() {
    let _ = ENGINE.command_sender.send(AudioCommand::StopMusic);
}

/* ============================ Engine internals ============================ */

fn init_engine_and_thread() -> AudioEngine {
    let (command_sender, command_receiver) = channel();

    let host = cpal::default_host();
    let device = host.default_output_device().expect("no audio output device");
    let config = device.default_output_config().expect("no default audio config");
    let stream_config: StreamConfig = config.clone().into();

    let device_sample_rate = stream_config.sample_rate.0;
    let device_channels = stream_config.channels as usize;

    // Spawn the audio manager thread (owns the CPAL stream and command loop)
    thread::spawn(move || {
        audio_manager_thread(command_receiver);
    });

    info!("Audio engine initialized ({} Hz, {} ch).", device_sample_rate, device_channels);
    AudioEngine {
        command_sender,
        sfx_cache: Mutex::new(HashMap::new()),
        device_sample_rate,
        device_channels,
    }
}

/// Manager thread: builds the CPAL stream, mixes SFX, and forwards music via ring.
fn audio_manager_thread(command_receiver: Receiver<AudioCommand>) {
    let mut music_stream: Option<MusicStream> = None;
    let music_ring = internal::ring_new(internal::RING_CAP_SAMPLES);
    let (sfx_sender, sfx_receiver) = channel::<Arc<Vec<i16>>>();

    let host = cpal::default_host();
    let device = host.default_output_device().expect("no audio output device");
    let config = device.default_output_config().expect("no default audio config");
    let stream_config: StreamConfig = config.clone().into();

    // State captured by the audio callback
    let music_ring_for_callback = music_ring.clone();

    // Reusable buffers captured by the callback to avoid allocations
    let mut mix_i16: Vec<i16> = Vec::new();
    let mut active_sfx_for_callback: Vec<(Arc<Vec<i16>>, usize)> = Vec::new();

    // Build the output stream matching device sample format (like v1)
    let stream = match config.sample_format() {
        SampleFormat::I16 => device.build_output_stream(
            &stream_config,
            move |out: &mut [i16], _| {
                if mix_i16.len() != out.len() { mix_i16.resize(out.len(), 0); }

                // Pull music samples
                internal::callback_fill_from_ring_i16(&music_ring_for_callback, &mut mix_i16[..]);

                // Ingest any new SFX references without allocating in RT
                for new_sfx in sfx_receiver.try_iter() {
                    active_sfx_for_callback.push((new_sfx, 0));
                }

                // Mix SFX (saturating add) into i16 domain
                active_sfx_for_callback.retain_mut(|(data, cursor)| {
                    let n = (data.len().saturating_sub(*cursor)).min(mix_i16.len());
                    for i in 0..n {
                        mix_i16[i] = mix_i16[i].saturating_add(data[*cursor + i]);
                    }
                    *cursor += n;
                    *cursor < data.len()
                });

                // Write to device
                out.copy_from_slice(&mix_i16);
            },
            |err| error!("Audio stream error: {}", err),
            None,
        ),
        SampleFormat::U16 => device.build_output_stream(
            &stream_config,
            move |out: &mut [u16], _| {
                if mix_i16.len() != out.len() { mix_i16.resize(out.len(), 0); }

                internal::callback_fill_from_ring_i16(&music_ring_for_callback, &mut mix_i16[..]);

                for new_sfx in sfx_receiver.try_iter() {
                    active_sfx_for_callback.push((new_sfx, 0));
                }

                active_sfx_for_callback.retain_mut(|(data, cursor)| {
                    let n = (data.len().saturating_sub(*cursor)).min(mix_i16.len());
                    for i in 0..n {
                        mix_i16[i] = mix_i16[i].saturating_add(data[*cursor + i]);
                    }
                    *cursor += n;
                    *cursor < data.len()
                });

                for (o, s) in out.iter_mut().zip(&mix_i16) {
                    *o = (i32::from(*s) + 32768) as u16;
                }
            },
            |err| error!("Audio stream error: {}", err),
            None,
        ),
        SampleFormat::F32 => device.build_output_stream(
            &stream_config,
            move |out: &mut [f32], _| {
                if mix_i16.len() != out.len() { mix_i16.resize(out.len(), 0); }

                internal::callback_fill_from_ring_i16(&music_ring_for_callback, &mut mix_i16[..]);

                for new_sfx in sfx_receiver.try_iter() {
                    active_sfx_for_callback.push((new_sfx, 0));
                }

                active_sfx_for_callback.retain_mut(|(data, cursor)| {
                    let n = (data.len().saturating_sub(*cursor)).min(mix_i16.len());
                    for i in 0..n {
                        mix_i16[i] = mix_i16[i].saturating_add(data[*cursor + i]);
                    }
                    *cursor += n;
                    *cursor < data.len()
                });

                for (o, s) in out.iter_mut().zip(&mix_i16) {
                    *o = (*s).to_sample::<f32>();
                }
            },
            |err| error!("Audio stream error: {}", err),
            None,
        ),
        _ => unreachable!(),
    }.expect("Failed to build audio stream");

    stream.play().expect("Failed to play audio stream");

    // Command loop: manage music decoder thread and pass SFX to the callback
    loop {
        match command_receiver.recv() {
            Ok(AudioCommand::PlaySfx(data)) => { let _ = sfx_sender.send(data); },
            Ok(AudioCommand::PlayMusic(path, cut, looping)) => {
                if let Some(old) = music_stream.take() {
                    old.stop_signal.store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = old.thread.join();
                }
                internal::ring_clear(&music_ring);
                music_stream = Some(spawn_music_decoder_thread(path, cut, looping, music_ring.clone()));
            }
            Ok(AudioCommand::StopMusic) => {
                if let Some(old) = music_stream.take() {
                    old.stop_signal.store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = old.thread.join();
                }
                internal::ring_clear(&music_ring);
            }
            Err(_) => break, // main dropped; exit thread
        }
    }
}

/* ========================= Music decode + resample ========================= */

/// Spawn a thread to decode & resample one music file into the ring buffer.
fn spawn_music_decoder_thread(path: PathBuf, cut: Cut, looping: bool, ring: Arc<internal::SpscRingI16>) -> MusicStream {
    let stop_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_signal_clone = stop_signal.clone();

    let thread = thread::spawn(move || {
        if let Err(e) = music_decoder_thread_loop(path, cut, looping, ring, stop_signal_clone) {
            error!("Music decoder thread failed: {}", e);
        }
    });

    MusicStream { thread, stop_signal }
}

#[inline]
fn secs_to_frames(seconds: f64, sample_rate: u32) -> u64 {
    if !seconds.is_finite() {
        0
    } else {
        (seconds.max(0.0) * sample_rate as f64).round() as u64
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

fn apply_fade_envelope(samples: &mut [i16], channels: usize, start_frame: u64, fade: Option<(u64, u64)>) {
    let Some((full_volume_frame, silence_frame)) = fade else { return; };
    if samples.is_empty() || channels == 0 {
        return;
    }
    if full_volume_frame == silence_frame {
        return;
    }

    let frames = samples.len() / channels;
    if frames == 0 {
        return;
    }

    let start_volume = volume_for_frame(start_frame, full_volume_frame, silence_frame);
    let end_volume = volume_for_frame(start_frame + frames as u64, full_volume_frame, silence_frame);
    if start_volume > 0.9999 && end_volume > 0.9999 {
        return;
    }

    let frames_f = frames as f32;
    for frame in 0..frames {
        let t = frame as f32 / frames_f;
        let mut volume = start_volume + (end_volume - start_volume) * t;
        volume = volume.clamp(0.0, 1.0);
        if (volume - 1.0).abs() < 0.0001 {
            continue;
        }

        for c in 0..channels {
            let idx = frame * channels + c;
            let scaled = (samples[idx] as f32) * volume;
            samples[idx] = scaled.round().clamp(-32768.0, 32767.0) as i16;
        }
    }
}

/// The decoder loop, mirrored from v1 (seek+preroll, cut capping, flush).
fn music_decoder_thread_loop(
    path: PathBuf, cut: Cut, looping: bool, ring: Arc<internal::SpscRingI16>, stop: Arc<std::sync::atomic::AtomicBool>
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(&path)?;
    let mut ogg = OggStreamReader::new(BufReader::new(file))?;
    let in_ch = ogg.ident_hdr.audio_channels as usize;
    let in_hz = ogg.ident_hdr.audio_sample_rate;

    let out_ch = ENGINE.device_channels;
    let out_hz = ENGINE.device_sample_rate;

    // --- Handle negative start time as preroll silence ---
    if cut.start_sec < 0.0 {
        let silence_duration_sec = -cut.start_sec;
        let silence_samples = (silence_duration_sec * out_hz as f64 * out_ch as f64).round() as usize;
        if silence_samples > 0 {
            let silence_buf = vec![0i16; silence_samples];
            let mut off = 0;
            while off < silence_buf.len() {
                if stop.load(std::sync::atomic::Ordering::Relaxed) { return Ok(()); }
                let pushed = internal::ring_push(&ring, &silence_buf[off..]);
                if pushed == 0 { thread::sleep(std::time::Duration::from_micros(300)); } else { off += pushed; }
            }
        }
    }

    'main_loop: loop {
        // --- rubato SincFixedOut setup ---
        const OUT_FRAMES_PER_CALL: usize = 512;
        let ratio = out_hz as f64 / in_hz as f64;
        let iparams = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };
        let mut resampler = SincFixedOut::<f32>::new(
            ratio,
            1.0,
            iparams,
            OUT_FRAMES_PER_CALL,
            in_ch,
        )?;

        // --- v1-style start & pre-roll ---
        let start_frame_f = (cut.start_sec * in_hz as f64).max(0.0);
        let start_floor   = start_frame_f.floor() as u64;

        // Try to seek a little before start to fill FIR, else fall back to decode+drop
        let mut seek_ok = true;
        if start_floor > 0 {
            let seek_frame = start_floor.saturating_sub(internal::PREROLL_IN_FRAMES);
            if ogg.seek_absgp_pg(seek_frame).is_err() {
                seek_ok = false;
            }
        }

        // How many output frames to throw away to finish pre-roll?
        let mut preroll_out_frames: u64 =
            if seek_ok && start_floor > 0 {
                (internal::PREROLL_IN_FRAMES as f64 * ratio).ceil() as u64
            } else { 0 };

        // If seek failed, decode and drop input frames until we hit start
        let mut to_drop_in: u64 = if seek_ok { 0 } else { start_floor };

        let fade_in_frames = secs_to_frames(cut.fade_in_sec, out_hz);
        let fade_out_frames = secs_to_frames(cut.fade_out_sec, out_hz);

        // Optional cut length in output frames
        let mut frames_left_out: Option<u64> = if cut.length_sec.is_finite() {
            Some((cut.length_sec * out_hz as f64).round().max(0.0) as u64)
        } else { None };
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

        #[inline(always)]
        fn cap_out_frames(out_tmp: &mut Vec<i16>, out_ch: usize, frames_left_out: &mut Option<u64>) -> bool {
            if let Some(left) = frames_left_out {
                let frames = out_tmp.len() / out_ch;
                if *left == 0 { out_tmp.clear(); return true; }
                if (frames as u64) > *left {
                    out_tmp.truncate((*left as usize) * out_ch);
                    *left = 0;
                    return true;
                } else {
                    *left -= frames as u64;
                }
            }
            false
        }

        let mut out_tmp: Vec<i16> = Vec::with_capacity(OUT_FRAMES_PER_CALL * out_ch);

        // Accumulates decoded input per channel for rubato
        let mut in_planar: Vec<Vec<f32>> = vec![Vec::with_capacity(4096); in_ch];

        // Helper: push interleaved i16 samples into planar f32
        #[inline(always)]
        fn deinterleave_accum(planar: &mut [Vec<f32>], interleaved: &[i16], channels: usize) {
            let frames = interleaved.len() / channels;
            for f in 0..frames {
                let base = f * channels;
                for c in 0..channels {
                    planar[c].push(interleaved[base + c] as f32 / 32768.0);
                }
            }
        }

        // Produce blocks as long as enough input frames are buffered for the next call.
        #[inline(always)]
        fn try_produce_blocks(
            resampler: &mut SincFixedOut<f32>,
            in_planar: &mut [Vec<f32>],
            out_ch: usize,
            out_tmp: &mut Vec<i16>,
            preroll_out_frames: &mut u64,
            frames_emitted_total: &mut u64,
            fade_spec: Option<(u64, u64)>,
            frames_left_out: &mut Option<u64>,
            push_block: &mut dyn FnMut(&[i16]) -> Result<(), Box<dyn std::error::Error + Send + Sync>>,
        ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
            let mut produced_any = false;
            loop {
                let need = resampler.input_frames_next();
                if in_planar.iter().any(|ch| ch.len() < need) {
                    break;
                }
                // Build slice-of-slices without copying
                let mut input_slices: Vec<&[f32]> = Vec::with_capacity(in_planar.len());
                for ch in in_planar.iter() {
                    input_slices.push(&ch[..need]);
                }
                let out = resampler.process(&input_slices, None)?;
                // Drain consumed input
                for ch in in_planar.iter_mut() {
                    ch.drain(0..need);
                }
                if out.is_empty() {
                    break;
                }
                let produced_frames = out[0].len();
                out_tmp.clear();
                out_tmp.reserve(produced_frames * out_ch);
                for f in 0..produced_frames {
                    for c in 0..out_ch {
                        let src = out[c % out.len()][f];
                        let s = (src * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                        out_tmp.push(s);
                    }
                }
                // Preroll discard
                if *preroll_out_frames > 0 {
                    let frames = out_tmp.len() / out_ch;
                    let drop_frames = (*preroll_out_frames as usize).min(frames);
                    let drop_samples = drop_frames * out_ch;
                    if drop_samples > 0 {
                        out_tmp.drain(0..drop_samples);
                        *preroll_out_frames = (*preroll_out_frames).saturating_sub(drop_frames as u64);
                    }
                }
                // Cut length cap
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
                    *frames_emitted_total = (*frames_emitted_total).saturating_add((out_tmp.len() / out_ch) as u64);
                    push_block(out_tmp)?;
                    produced_any = true;
                }
                if finished { break; }
            }
            Ok(produced_any)
        }

        // --- Main decode loop ---
        while let Ok(pkt_opt) = ogg.read_dec_packet_itl() {
            if stop.load(std::sync::atomic::Ordering::Relaxed) { break 'main_loop; }

            let p = match pkt_opt { Some(p) if !p.is_empty() => p, Some(_) => continue, None => break };

            // If seek failed, drop whole input frames until we reach start
            let mut slice = &p[..];
            if to_drop_in > 0 {
                let pkt_frames = (p.len() / in_ch) as u64;
                if to_drop_in >= pkt_frames {
                    to_drop_in -= pkt_frames;
                    continue;
                } else {
                    let drop_samples = (to_drop_in as usize) * in_ch;
                    slice = &p[drop_samples..];
                    to_drop_in = 0;
                }
            }

            // Accumulate input, then try to resample one or more chunks
            deinterleave_accum(&mut in_planar, slice, in_ch);
            let mut finished = false;
            let _ = try_produce_blocks(
                &mut resampler,
                &mut in_planar,
                out_ch,
                &mut out_tmp,
                &mut preroll_out_frames,
                &mut frames_emitted_total,
                fade_spec,
                &mut frames_left_out,
                &mut |block: &[i16]| {
                    let mut off = 0;
                    while off < block.len() {
                        if stop.load(std::sync::atomic::Ordering::Relaxed) { return Ok(()); }
                        let pushed = internal::ring_push(&ring, &block[off..]);
                        if pushed == 0 { thread::sleep(std::time::Duration::from_micros(300)); } else { off += pushed; }
                    }
                    Ok(())
                },
            )?;
            finished = matches!(frames_left_out, Some(0));

            // Stop if cut length has been reached
            if finished { break; }
        }

        // --- Flush remainder ---
        if !in_planar.iter().all(|v| v.is_empty()) {
            // Process the final short chunk using process_partial
            let mut input_slices: Vec<&[f32]> = Vec::with_capacity(in_planar.len());
            let remain = in_planar.iter().map(|v| v.len()).min().unwrap_or(0);
            for ch in in_planar.iter() {
                input_slices.push(&ch[..remain]);
            }
            let out = resampler.process_partial(Some(&input_slices), None)?;
            for ch in in_planar.iter_mut() { ch.clear(); }
            if !out.is_empty() {
                let produced_frames = out[0].len();
                out_tmp.clear();
                out_tmp.reserve(produced_frames * out_ch);
                for f in 0..produced_frames {
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
                        preroll_out_frames = (preroll_out_frames).saturating_sub(drop_frames as u64);
                    }
                }
                if !out_tmp.is_empty() {
                    apply_fade_envelope(&mut out_tmp, out_ch, frames_emitted_total, fade_spec);
                    frames_emitted_total = (frames_emitted_total).saturating_add((out_tmp.len() / out_ch) as u64);
                    let mut off = 0;
                    while off < out_tmp.len() {
                        if stop.load(std::sync::atomic::Ordering::Relaxed) { return Ok(()); }
                        let pushed = internal::ring_push(&ring, &out_tmp[off..]);
                        if pushed == 0 { thread::sleep(std::time::Duration::from_micros(300)); } else { off += pushed; }
                    }
                }
            }
        }
        // Final tail flush from the resampler
        // Tail flush of delayed frames
        let out_tail = resampler.process_partial::<&[f32]>(None, None)?;
        if !out_tail.is_empty() {
            let produced_frames = out_tail[0].len();
            out_tmp.clear();
            out_tmp.reserve(produced_frames * out_ch);
            for f in 0..produced_frames {
                for c in 0..out_ch {
                    let v = out_tail[c % out_tail.len()][f];
                    let s = (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                    out_tmp.push(s);
                }
            }
            let _ = cap_out_frames(&mut out_tmp, out_ch, &mut frames_left_out);
            if !out_tmp.is_empty() {
                let mut off = 0;
                while off < out_tmp.len() {
                    if stop.load(std::sync::atomic::Ordering::Relaxed) { return Ok(()); }
                    let pushed = internal::ring_push(&ring, &out_tmp[off..]);
                    if pushed == 0 { thread::sleep(std::time::Duration::from_micros(300)); } else { off += pushed; }
                }
            }
        }

        // --- Looping logic ---
        if !looping {
            break 'main_loop;
        }
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            break 'main_loop;
        }

        // Re-open the file for the next loop iteration (gapless enough; continue with fresh resampler)
        match File::open(&path).ok().and_then(|f| OggStreamReader::new(BufReader::new(f)).ok()) {
            Some(new_reader) => {
                info!("Looping music: restarted {:?}", path);
                ogg = new_reader;
            }
            None => {
                warn!("Could not reopen OGG stream for looping: {:?}", path);
                break 'main_loop;
            }
        }
    }

    Ok(())
}

/// Loads an Ogg file fully and resamples it to the device rate for SFX (cached).
fn load_and_resample_sfx(path: &str) -> Result<Arc<Vec<i16>>, Box<dyn std::error::Error>> {
    let file = File::open(Path::new(path))?;
    let mut ogg = OggStreamReader::new(BufReader::new(file))?;
    let in_ch = ogg.ident_hdr.audio_channels as usize;
    let in_hz = ogg.ident_hdr.audio_sample_rate;

    let out_ch = ENGINE.device_channels;
    let out_hz = ENGINE.device_sample_rate;

    const OUT_FRAMES_PER_CALL: usize = 512;
    let ratio = out_hz as f64 / in_hz as f64;
    let iparams = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler = SincFixedOut::<f32>::new(
        ratio,
        1.0,
        iparams,
        OUT_FRAMES_PER_CALL,
        in_ch,
    )?;

    let mut resampled_data: Vec<i16> = Vec::new();
    let mut in_planar: Vec<Vec<f32>> = vec![Vec::with_capacity(4096); in_ch];

    while let Some(pkt) = ogg.read_dec_packet_itl()? {
        let frames = pkt.len() / in_ch;
        if frames == 0 { continue; }
        // Push into planar
        for f in 0..frames {
            let base = f * in_ch;
            for c in 0..in_ch {
                in_planar[c].push(pkt[base + c] as f32 / 32768.0);
            }
        }
        // Produce as many blocks as possible based on required input
        loop {
            let need = resampler.input_frames_next();
            if in_planar.iter().any(|ch| ch.len() < need) { break; }
            let mut input_slices: Vec<&[f32]> = Vec::with_capacity(in_planar.len());
            for ch in in_planar.iter() { input_slices.push(&ch[..need]); }
            let out = resampler.process(&input_slices, None)?;
            for ch in in_planar.iter_mut() { ch.drain(0..need); }
            if out.is_empty() { break; }
            let produced_frames = out[0].len();
            resampled_data.reserve(produced_frames * out_ch);
            for f in 0..produced_frames {
                for c in 0..out_ch {
                    let v = out[c % out.len()][f];
                    let s = (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                    resampled_data.push(s);
                }
            }
        }
    }

    // Flush any remaining samples
    if !in_planar.iter().all(|v| v.is_empty()) {
        let remain = in_planar.iter().map(|v| v.len()).min().unwrap_or(0);
        let mut input_slices: Vec<&[f32]> = Vec::with_capacity(in_planar.len());
        for ch in in_planar.iter() { input_slices.push(&ch[..remain]); }
        let out = resampler.process_partial(Some(&input_slices), None)?;
        if !out.is_empty() {
            let produced_frames = out[0].len();
            resampled_data.reserve(produced_frames * out_ch);
            for f in 0..produced_frames {
                for c in 0..out_ch {
                    let v = out[c % out.len()][f];
                    let s = (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                    resampled_data.push(s);
                }
            }
        }
        for ch in in_planar.iter_mut() { ch.clear(); }
    }
    // Tail flush
    let out_tail = resampler.process_partial::<&[f32]>(None, None)?;
    if !out_tail.is_empty() {
        let produced_frames = out_tail[0].len();
        resampled_data.reserve(produced_frames * out_ch);
        for f in 0..produced_frames {
            for c in 0..out_ch {
                let v = out_tail[c % out_tail.len()][f];
                let s = (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                resampled_data.push(s);
            }
        }
    }

    Ok(Arc::new(resampled_data))
}

/* =========================== Internal primitives =========================== */

mod internal {
    use super::*;
    use std::cell::UnsafeCell;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Match v1 defaults (safe to reduce later once everything’s stable)
    pub const BASE_TAPS: usize = 8;
    pub const BETA: f64 = 8.0;
    pub const PREROLL_IN_FRAMES: u64 = 8;
    pub const RING_CAP_SAMPLES: usize = 1 << 18; // interleaved i16 samples

    /* ----------------------------- SPSC ring ----------------------------- */

    pub struct SpscRingI16 {
        buf: UnsafeCell<Box<[i16]>>,
        mask: usize,
        head: AtomicUsize,
        tail: AtomicUsize,
    }
    unsafe impl Send for SpscRingI16 {}
    unsafe impl Sync for SpscRingI16 {}

    pub fn ring_new(cap_pow2: usize) -> Arc<SpscRingI16> {
        assert!(cap_pow2.is_power_of_two());
        Arc::new(SpscRingI16 {
            buf: UnsafeCell::new(vec![0i16; cap_pow2].into_boxed_slice()),
            mask: cap_pow2 - 1,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        })
    }

    #[inline(always)]
    fn ring_cap(r: &SpscRingI16) -> usize { unsafe { (&*r.buf.get()).len() } }

    pub fn ring_push(r: &SpscRingI16, data: &[i16]) -> usize {
        let cap = ring_cap(r); let mask = r.mask;
        let h = r.head.load(Ordering::Relaxed); let t = r.tail.load(Ordering::Acquire);
        let free = cap - h.wrapping_sub(t);
        let n = data.len().min(free); if n == 0 { return 0; }
        let idx = h & mask;
        unsafe {
            let buf = &mut *r.buf.get();
            let first = (cap - idx).min(n);
            buf[idx..idx + first].copy_from_slice(&data[..first]);
            if n > first { buf[0..(n - first)].copy_from_slice(&data[first..n]); }
        }
        r.head.store(h.wrapping_add(n), Ordering::Release); n
    }

    pub fn ring_pop(r: &SpscRingI16, out: &mut [i16]) -> usize {
        let cap = ring_cap(r); let mask = r.mask;
        let h = r.head.load(Ordering::Acquire); let t = r.tail.load(Ordering::Relaxed);
        let avail = h.wrapping_sub(t);
        let n = out.len().min(avail); if n == 0 { return 0; }
        let idx = t & mask;
        unsafe {
            let buf = &*r.buf.get();
            let first = (cap - idx).min(n);
            out[..first].copy_from_slice(&buf[idx..idx + first]);
            if n > first { out[first..n].copy_from_slice(&buf[0..(n - first)]); }
        }
        r.tail.store(t.wrapping_add(n), Ordering::Release); n
    }

    pub fn ring_clear(r: &SpscRingI16) {
        // This is called from the manager thread when the producer (decoder) is stopped.
        // It makes the buffer appear empty to the consumer (audio callback).
        let tail_pos = r.tail.load(Ordering::Relaxed);
        r.head.store(tail_pos, Ordering::Release);
    }

    pub fn callback_fill_from_ring_i16(ring: &SpscRingI16, dst: &mut [i16]) {
        let mut filled = 0;
        while filled < dst.len() {
            let got = ring_pop(ring, &mut dst[filled..]);
            if got == 0 {
                // underrun: zero the rest
                for d in &mut dst[filled..] { *d = 0; }
                break;
            }
            filled += got;
        }
    }

    /* ----------------------------- Math utils ----------------------------- */

    #[inline(always)] fn gcd(mut a: u32, mut b: u32) -> u32 { while b != 0 { let r = a % b; a = b; b = r; } a }
    #[inline(always)] fn reduce_ratio(out_hz: u32, in_hz: u32) -> (u32, u32) { let g = gcd(out_hz, in_hz); (out_hz / g, in_hz / g) }

    #[inline(always)]
    fn i0(mut x: f64) -> f64 { x*=0.5; let (mut t, mut s)=(1.0,1.0); for k in 1..=10 { t *= (x*x)/((k as f64)*(k as f64)); s+=t; } s }

    /* -------------------------- Polyphase FIR -------------------------- */

    fn design_kaiser_sinc(n: usize, fc: f64, beta: f64) -> Vec<f64> {
        let m = (n - 1) as f64 / 2.0; let denom = i0(beta); let two_fc = 2.0 * fc;
        (0..n).map(|u| {
            let x = u as f64 - m;
            let sinc = if x == 0.0 { 1.0 } else { (std::f64::consts::PI * two_fc * x).sin() / (std::f64::consts::PI * x) };
            let w = i0(beta * (1.0 - (x / m).powi(2)).max(0.0).sqrt()) / denom;
            two_fc * sinc * w
        }).collect()
    }

    fn build_polyphase(l: usize, m: usize, base_taps: usize, beta: f64) -> (Vec<f32>, usize) {
        let n = base_taps * l;
        let mut h = design_kaiser_sinc(n, 0.5f64 / (l.max(m) as f64), beta);

        // Normalize DC gain
        let scale = (l as f64) / h.iter().sum::<f64>();
        h.iter_mut().for_each(|v| *v *= scale);

        let tpp = n / l;
        let mut phases = vec![0.0f32; n];
        for p in 0..l {
            // phase taps are h[p + k*l], most-recent-first
            let mut t: Vec<f32> = (0..tpp).map(|k| h[p + k * l] as f32).collect();
            t.reverse();
            phases[p * tpp .. (p + 1) * tpp].copy_from_slice(&t);
        }
        (phases, tpp)
    }

    #[inline(always)]
    fn dot8(a: &[f32;8], b: &[f32;8]) -> f32 {
        a[0]*b[0] + a[1]*b[1] + a[2]*b[2] + a[3]*b[3] + a[4]*b[4] + a[5]*b[5] + a[6]*b[6] + a[7]*b[7]
    }

    pub struct PolyState {
        pub l: usize, pub m: usize,
        pub in_ch: usize, pub out_ch: usize,
        pub tpp: usize, pub phase: usize,
        phases: Vec<f32>,
        delay8: Option<Vec<[f32;8]>>,
        delay: Vec<f32>,
        inbuf: VecDeque<f32>,
        mapped: Vec<i16>,
        acc_frame: Vec<f32>,
    }

    pub fn poly_init(in_hz: u32, out_hz: u32, in_ch: usize, out_ch: usize, base_taps: usize, beta: f64) -> PolyState {
        let (l_u, m_u) = reduce_ratio(out_hz, in_hz);
        let (phases, tpp) = build_polyphase(l_u as usize, m_u as usize, base_taps, beta);
        PolyState {
            l: l_u as usize, m: m_u as usize, in_ch, out_ch, tpp, phase: 0,
            phases,
            delay8: if tpp == 8 { Some(vec![[0.0;8]; in_ch]) } else { None },
            delay:  if tpp != 8 { vec![0.0; in_ch * tpp] } else { Vec::new() },
            inbuf: VecDeque::with_capacity(1<<15),
            mapped: vec![0; out_ch], acc_frame: vec![0.0; in_ch],
        }
    }

    #[inline(always)]
    pub fn poly_set_fractional_phase(st: &mut PolyState, frac: f64) {
        // frac in [0,1); nearest phase like v1
        let p = ((frac * st.l as f64).round() as usize) % st.l;
        st.phase = p;
    }

    #[inline(always)] fn poly_need_input(st: &PolyState) -> bool { st.phase >= st.l }

    fn poly_shift_in(st: &mut PolyState) -> bool {
        if st.inbuf.len() < st.in_ch { return false; }
        if let Some(d8) = &mut st.delay8 {
            for c in 0..st.in_ch {
                d8[c].rotate_right(1);
                d8[c][0] = st.inbuf.pop_front().unwrap();
            }
        } else {
            let tpp = st.tpp;
            for c in 0..st.in_ch {
                let base = c * tpp;
                // FIX: rotate only this channel’s slice
                st.delay[base .. base + tpp].rotate_right(1);
                st.delay[base] = st.inbuf.pop_front().unwrap();
            }
        }
        true
    }

    /// Push decoded input (i16) and produce as many output frames as possible (i16, interleaved).
    pub fn poly_push_produce(st: &mut PolyState, input: &[i16], out_tmp: &mut Vec<i16>) {
        // Extend input buffer
        st.inbuf.extend(input.iter().map(|&s| s as f32 / 32768.0));

        loop {
            while poly_need_input(st) {
                st.phase -= st.l;
                if !poly_shift_in(st) { return; }
            }
            let p = st.phase;

            // Convolution
            if st.tpp == 8 {
                let coeffs: &[f32;8] = st.phases[p*8 .. (p+1)*8].try_into().unwrap();
                let d8 = st.delay8.as_ref().unwrap();
                for c in 0..st.in_ch { st.acc_frame[c] = dot8(coeffs, &d8[c]); }
            } else {
                let tpp = st.tpp;
                let coeffs = &st.phases[p * tpp .. (p+1) * tpp];
                for c in 0..st.in_ch {
                    let base = c * tpp;
                    let mut acc = 0.0f32;
                    for k in 0..tpp { acc += coeffs[k] * st.delay[base + k]; }
                    st.acc_frame[c] = acc;
                }
            }

            // Channel map -> i16 (match v1’s behavior)
            for c in 0..st.out_ch {
                let s = (st.acc_frame[c % st.in_ch] * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                st.mapped[c] = s;
            }
            out_tmp.extend_from_slice(&st.mapped);

            st.phase += st.m;
            if poly_need_input(st) && st.inbuf.len() < st.in_ch { return; }
        }
    }
}
