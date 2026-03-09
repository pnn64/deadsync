use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, StreamConfig};
use lewton::inside_ogg::OggStreamReader;
use log::{debug, error, info, warn};
use rubato::{
    Resampler, SincFixedOut, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

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

#[derive(Clone, Debug)]
pub struct OutputDeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub sample_rates_hz: Vec<u32>,
}

struct OutputDeviceProbe {
    device: cpal::Device,
    info: OutputDeviceInfo,
}

#[derive(Clone, Copy, Debug)]
enum SfxLane {
    Effect,
    AssistTick,
}

#[derive(Clone)]
struct QueuedSfx {
    data: Arc<Vec<i16>>,
    lane: SfxLane,
}

// Commands to the audio engine
enum AudioCommand {
    PlaySfx(QueuedSfx),
    // Path, cut, looping, rate (1.0 = normal)
    PlayMusic(PathBuf, Cut, bool, f32),
    StopMusic,
    // Change rate of currently playing music without restarting
    SetMusicRate(f32),
}

// Global engine (initialized once)
static ENGINE: std::sync::LazyLock<AudioEngine> = std::sync::LazyLock::new(init_engine_and_thread);

struct AudioEngine {
    command_sender: Sender<AudioCommand>,
    sfx_cache: Mutex<HashMap<String, Arc<Vec<i16>>>>,
    device_sample_rate: u32,
    device_channels: usize,
    startup_output_devices: Vec<OutputDeviceInfo>,
}

/// A handle to a streaming music track.
struct MusicStream {
    thread: thread::JoinHandle<()>,
    stop_signal: Arc<AtomicBool>,
    rate_bits: Arc<AtomicU32>,
}

// Global playback position tracking for the current music stream.
// All counters are in *frames* at the device sample rate (not interleaved samples).
static MUSIC_TOTAL_FRAMES: AtomicU64 = AtomicU64::new(0);
static MUSIC_TRACK_START_FRAME: AtomicU64 = AtomicU64::new(0);
static MUSIC_TRACK_HAS_STARTED: AtomicBool = AtomicBool::new(false);
static MUSIC_TRACK_ACTIVE: AtomicBool = AtomicBool::new(false);
static MUSIC_MAP_GEN: AtomicU64 = AtomicU64::new(1);

// Last audio callback timing, used to interpolate the playback position
// between callback invocations so that the reported stream time is
// continuous instead of jumping in whole buffer increments.
static CALLBACK_EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
static CALLBACK_CLOCK_SEQ: AtomicU64 = AtomicU64::new(0);
// Stored as elapsed nanos + 1 from CALLBACK_EPOCH; 0 means "no callback yet".
static LAST_CALLBACK_ELAPSED_NANOS: AtomicU64 = AtomicU64::new(0);
static LAST_CALLBACK_BASE_FRAMES: AtomicU64 = AtomicU64::new(0);
static LAST_CALLBACK_FRAMES: AtomicU64 = AtomicU64::new(0);
static PREV_CALLBACK_ELAPSED_NANOS: AtomicU64 = AtomicU64::new(0);
static PREV_CALLBACK_BASE_FRAMES: AtomicU64 = AtomicU64::new(0);
static PREV_CALLBACK_FRAMES: AtomicU64 = AtomicU64::new(0);

const MUSIC_POS_MAP_BACKLOG_FRAMES: i64 = 80_000;

#[derive(Clone, Copy, Debug)]
pub struct MusicStreamClockSnapshot {
    pub stream_seconds: f32,
    pub music_seconds: f32,
    pub music_seconds_per_second: f32,
    pub has_music_mapping: bool,
    pub valid_at: Instant,
}

#[derive(Clone, Copy, Debug, Default)]
struct CallbackClockWindow {
    total_frames: u64,
    last_nanos: u64,
    last_base_frames: u64,
    last_callback_frames: u64,
    prev_nanos: u64,
    prev_base_frames: u64,
    prev_callback_frames: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct MusicMapSeg {
    stream_frame_start: i64,
    frames: i64,
    music_start_sec: f64,
    music_sec_per_frame: f64,
}

#[derive(Default)]
struct PlaybackPosMap {
    queue: VecDeque<MusicMapSeg>,
    backlog_frames: i64,
}

impl PlaybackPosMap {
    fn clear(&mut self) {
        self.queue.clear();
        self.backlog_frames = 0;
    }

    fn insert(&mut self, seg: MusicMapSeg) {
        if seg.frames <= 0
            || !seg.music_start_sec.is_finite()
            || !seg.music_sec_per_frame.is_finite()
        {
            return;
        }
        if let Some(last) = self.queue.back_mut() {
            let contiguous_stream = last.stream_frame_start + last.frames == seg.stream_frame_start;
            let ratio_match = (last.music_sec_per_frame - seg.music_sec_per_frame).abs() <= 1e-9;
            let expected_music_start =
                last.music_start_sec + last.music_sec_per_frame * last.frames as f64;
            let music_contiguous = (expected_music_start - seg.music_start_sec).abs()
                <= seg.music_sec_per_frame.abs().max(1e-9);
            if contiguous_stream && ratio_match && music_contiguous {
                last.frames += seg.frames;
                self.backlog_frames = self.backlog_frames.saturating_add(seg.frames);
                self.cleanup();
                return;
            }
        }
        self.backlog_frames = self.backlog_frames.saturating_add(seg.frames);
        self.queue.push_back(seg);
        self.cleanup();
    }

    fn cleanup(&mut self) {
        while self.backlog_frames > MUSIC_POS_MAP_BACKLOG_FRAMES {
            if let Some(front) = self.queue.pop_front() {
                self.backlog_frames = self.backlog_frames.saturating_sub(front.frames);
            } else {
                self.backlog_frames = 0;
                break;
            }
        }
    }

    fn search(&self, stream_frame: f64) -> Option<(f64, f64)> {
        if self.queue.is_empty() || !stream_frame.is_finite() {
            return None;
        }
        let mut closest = None;
        let mut closest_dist = f64::INFINITY;
        for seg in &self.queue {
            let start = seg.stream_frame_start as f64;
            let end = start + seg.frames as f64;
            if stream_frame >= start && stream_frame < end {
                let diff = stream_frame - start;
                return Some((
                    seg.music_start_sec + diff * seg.music_sec_per_frame,
                    seg.music_sec_per_frame,
                ));
            }
            let start_dist = (stream_frame - start).abs();
            if start_dist < closest_dist {
                closest_dist = start_dist;
                closest = Some((seg.music_start_sec, seg.music_sec_per_frame));
            }
            let end_music = seg.music_start_sec + seg.music_sec_per_frame * seg.frames as f64;
            let end_dist = (stream_frame - end).abs();
            if end_dist < closest_dist {
                closest_dist = end_dist;
                closest = Some((end_music, seg.music_sec_per_frame));
            }
        }
        closest
    }
}

static QUEUED_MUSIC_MAP_SEGS: std::sync::LazyLock<Arc<internal::SpscRingMusicSeg>> =
    std::sync::LazyLock::new(|| internal::music_seg_ring_new(internal::MUSIC_SEG_RING_CAP));
static PLAYED_MUSIC_MAP_SEGS: std::sync::LazyLock<Arc<internal::SpscRingMusicSeg>> =
    std::sync::LazyLock::new(|| internal::music_seg_ring_new(internal::MUSIC_SEG_RING_CAP));
static PLAYBACK_POS_MAP: std::sync::LazyLock<Mutex<PlaybackPosMap>> =
    std::sync::LazyLock::new(|| Mutex::new(PlaybackPosMap::default()));

/* ============================ Public functions ============================ */

/// Initializes the audio engine. Must be called once at startup.
pub fn init() -> Result<(), String> {
    std::sync::LazyLock::force(&ENGINE);
    std::sync::LazyLock::force(&QUEUED_MUSIC_MAP_SEGS);
    std::sync::LazyLock::force(&PLAYED_MUSIC_MAP_SEGS);
    std::sync::LazyLock::force(&PLAYBACK_POS_MAP);
    Ok(())
}

pub fn startup_output_devices() -> Vec<OutputDeviceInfo> {
    ENGINE.startup_output_devices.clone()
}

/// Plays a sound effect from the given path (cached after first load).
pub fn play_sfx(path: &str) {
    play_sfx_on_lane(path, SfxLane::Effect);
}

/// Plays a gameplay assist tick that uses its own volume lane.
pub fn play_assist_tick(path: &str) {
    play_sfx_on_lane(path, SfxLane::AssistTick);
}

fn play_sfx_on_lane(path: &str, lane: SfxLane) {
    let sound_data = {
        let mut cache = ENGINE.sfx_cache.lock().unwrap();
        if let Some(data) = cache.get(path) {
            data.clone()
        } else {
            match load_and_resample_sfx(path) {
                Ok(data) => {
                    cache.insert(path.to_string(), data.clone());
                    debug!("Cached SFX: {path}");
                    data
                }
                Err(e) => {
                    warn!("Failed to load SFX '{path}': {e}");
                    return;
                }
            }
        }
    };
    let queued = QueuedSfx {
        data: sound_data,
        lane,
    };
    let _ = ENGINE.command_sender.send(AudioCommand::PlaySfx(queued));
}

/// Preloads a sound effect into cache without playing it.
pub fn preload_sfx(path: &str) {
    let mut cache = ENGINE.sfx_cache.lock().unwrap();
    if cache.contains_key(path) {
        return;
    }
    match load_and_resample_sfx(path) {
        Ok(data) => {
            cache.insert(path.to_string(), data);
            debug!("Cached SFX: {path}");
        }
        Err(e) => {
            warn!("Failed to preload SFX '{path}': {e}");
        }
    }
}

#[inline(always)]
fn clear_music_pos_map() {
    internal::music_seg_ring_clear(&QUEUED_MUSIC_MAP_SEGS);
    internal::music_seg_ring_clear(&PLAYED_MUSIC_MAP_SEGS);
    PLAYBACK_POS_MAP.lock().unwrap().clear();
    MUSIC_MAP_GEN.fetch_add(1, Ordering::Release);
}

#[inline(always)]
fn reset_music_stream_clock() {
    // Reset immediately on the caller thread so async command handoff can't
    // leak the previous track's stream position into gameplay timing.
    let total = MUSIC_TOTAL_FRAMES.load(Ordering::Acquire);
    MUSIC_TRACK_START_FRAME.store(total, Ordering::Release);
    MUSIC_TRACK_HAS_STARTED.store(false, Ordering::Release);
    MUSIC_TRACK_ACTIVE.store(false, Ordering::Release);
    clear_music_pos_map();
}

#[inline(always)]
fn callback_nanos_at(at: Instant) -> u64 {
    at.checked_duration_since(*CALLBACK_EPOCH)
        .map(|delta| delta.as_nanos().min((u64::MAX - 1) as u128) as u64)
        .unwrap_or(0)
}

#[inline(always)]
fn output_playback_anchor(now: Instant, info: &cpal::OutputCallbackInfo) -> Instant {
    let timestamp = info.timestamp();
    if let Some(delay) = timestamp.playback.duration_since(&timestamp.callback) {
        now.checked_add(delay).unwrap_or(now)
    } else if let Some(lead) = timestamp.callback.duration_since(&timestamp.playback) {
        now.checked_sub(lead).unwrap_or(now)
    } else {
        now
    }
}

#[inline(always)]
fn stream_position_frames_from_callback(
    sample_rate: u32,
    start_frame: u64,
    at_nanos: u64,
    cb_nanos_plus_one: u64,
    base_frames: u64,
    buf_frames: u64,
) -> Option<f64> {
    if cb_nanos_plus_one == 0 {
        return None;
    }
    let cb_nanos = cb_nanos_plus_one.saturating_sub(1);
    if at_nanos < cb_nanos {
        return None;
    }
    let dt = (at_nanos.saturating_sub(cb_nanos) as f64) * 1e-9;
    let frames_since_cb = (dt * sample_rate as f64).clamp(0.0, buf_frames as f64);
    let frames_now = base_frames as f64 + frames_since_cb;
    Some((frames_now.max(start_frame as f64) - start_frame as f64).max(0.0))
}

#[inline(always)]
fn begin_callback_clock_write() {
    CALLBACK_CLOCK_SEQ.fetch_add(1, Ordering::AcqRel);
}

#[inline(always)]
fn end_callback_clock_write() {
    CALLBACK_CLOCK_SEQ.fetch_add(1, Ordering::Release);
}

#[inline(always)]
fn publish_callback_window_start(total_before: u64, anchor_at: Instant) {
    begin_callback_clock_write();
    PREV_CALLBACK_BASE_FRAMES.store(
        LAST_CALLBACK_BASE_FRAMES.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    PREV_CALLBACK_FRAMES.store(
        LAST_CALLBACK_FRAMES.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    PREV_CALLBACK_ELAPSED_NANOS.store(
        LAST_CALLBACK_ELAPSED_NANOS.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    LAST_CALLBACK_BASE_FRAMES.store(total_before, Ordering::Relaxed);
    LAST_CALLBACK_FRAMES.store(0, Ordering::Relaxed);
    LAST_CALLBACK_ELAPSED_NANOS.store(
        callback_nanos_at(anchor_at).saturating_add(1),
        Ordering::Relaxed,
    );
    end_callback_clock_write();
}

#[inline(always)]
fn publish_callback_window_end(total_before: u64, frames: u64) {
    begin_callback_clock_write();
    LAST_CALLBACK_FRAMES.store(frames, Ordering::Relaxed);
    MUSIC_TOTAL_FRAMES.store(total_before.saturating_add(frames), Ordering::Relaxed);
    end_callback_clock_write();
}

fn load_callback_clock_snapshot_now() -> (Instant, u64, CallbackClockWindow) {
    loop {
        let seq_start = CALLBACK_CLOCK_SEQ.load(Ordering::Acquire);
        if seq_start & 1 != 0 {
            std::hint::spin_loop();
            continue;
        }
        let valid_at = Instant::now();
        let at_nanos = callback_nanos_at(valid_at);
        let window = CallbackClockWindow {
            total_frames: MUSIC_TOTAL_FRAMES.load(Ordering::Relaxed),
            last_nanos: LAST_CALLBACK_ELAPSED_NANOS.load(Ordering::Relaxed),
            last_base_frames: LAST_CALLBACK_BASE_FRAMES.load(Ordering::Relaxed),
            last_callback_frames: LAST_CALLBACK_FRAMES.load(Ordering::Relaxed),
            prev_nanos: PREV_CALLBACK_ELAPSED_NANOS.load(Ordering::Relaxed),
            prev_base_frames: PREV_CALLBACK_BASE_FRAMES.load(Ordering::Relaxed),
            prev_callback_frames: PREV_CALLBACK_FRAMES.load(Ordering::Relaxed),
        };
        let seq_end = CALLBACK_CLOCK_SEQ.load(Ordering::Acquire);
        if seq_start == seq_end {
            return (valid_at, at_nanos, window);
        }
    }
}

#[inline(always)]
fn stream_position_frames_from_window(
    sample_rate: u32,
    start_frame: u64,
    at_nanos: u64,
    window: CallbackClockWindow,
) -> f64 {
    if let Some(frames) = stream_position_frames_from_callback(
        sample_rate,
        start_frame,
        at_nanos,
        window.last_nanos,
        window.last_base_frames,
        window.last_callback_frames,
    ) {
        return frames;
    }
    if let Some(frames) = stream_position_frames_from_callback(
        sample_rate,
        start_frame,
        at_nanos,
        window.prev_nanos,
        window.prev_base_frames,
        window.prev_callback_frames,
    ) {
        return frames;
    }
    window.total_frames.saturating_sub(start_frame) as f64
}

fn drain_played_music_map_segments() {
    let mut map = PLAYBACK_POS_MAP.lock().unwrap();
    while let Some(seg) = internal::music_seg_ring_pop(&PLAYED_MUSIC_MAP_SEGS) {
        map.insert(seg);
    }
}

fn lookup_music_position(stream_frames: f64, sample_rate: u32) -> Option<(f32, f32)> {
    drain_played_music_map_segments();
    let map = PLAYBACK_POS_MAP.lock().unwrap();
    map.search(stream_frames).map(|(music_sec, sec_per_frame)| {
        (
            music_sec as f32,
            (sec_per_frame * sample_rate as f64) as f32,
        )
    })
}

/// Plays a music track from a file path.
pub fn play_music(path: PathBuf, cut: Cut, looping: bool, rate: f32) {
    let rate = if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    };
    reset_music_stream_clock();
    let _ = ENGINE
        .command_sender
        .send(AudioCommand::PlayMusic(path, cut, looping, rate));
}

/// Stops the currently playing music track.
pub fn stop_music() {
    reset_music_stream_clock();
    let _ = ENGINE.command_sender.send(AudioCommand::StopMusic);
}

/// Adjusts the playback rate for the current music stream, if any.
pub fn set_music_rate(rate: f32) {
    let rate = if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    };
    let _ = ENGINE.command_sender.send(AudioCommand::SetMusicRate(rate));
}

/// Returns the elapsed real time (in seconds) of the currently playing
/// music stream, measured from the moment the first sample of that stream
/// reached the output callback. This is derived from the device's sample
/// clock and is independent of wall-clock time. The value is smoothed
/// between callbacks using the callback timestamp so it advances
/// continuously instead of in buffer-sized jumps.
pub fn get_music_stream_position_seconds() -> f32 {
    get_music_stream_clock_snapshot().stream_seconds
}

/// Returns the current stream position and the `Instant` it is valid for.
pub fn get_music_stream_clock_snapshot() -> MusicStreamClockSnapshot {
    let sample_rate = ENGINE.device_sample_rate.max(1);
    let has_started = MUSIC_TRACK_HAS_STARTED.load(Ordering::Acquire);
    if !has_started {
        return MusicStreamClockSnapshot {
            stream_seconds: 0.0,
            music_seconds: 0.0,
            music_seconds_per_second: 1.0,
            has_music_mapping: false,
            valid_at: Instant::now(),
        };
    }
    let start = MUSIC_TRACK_START_FRAME.load(Ordering::Acquire);
    let (valid_at, at_nanos, window) = load_callback_clock_snapshot_now();
    let stream_frames = stream_position_frames_from_window(sample_rate, start, at_nanos, window);
    let stream_seconds = (stream_frames / sample_rate as f64) as f32;
    let (music_seconds, music_seconds_per_second, has_music_mapping) =
        match lookup_music_position(stream_frames, sample_rate) {
            Some((music_seconds, slope)) => (music_seconds, slope, true),
            None => (stream_seconds, 1.0, false),
        };
    MusicStreamClockSnapshot {
        stream_seconds,
        music_seconds,
        music_seconds_per_second,
        has_music_mapping,
        valid_at,
    }
}

/* ============================ Engine internals ============================ */

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

fn commit_played_music_map(
    track_frame_start: i64,
    frames_popped: i64,
    queued_seg_ring: &internal::SpscRingMusicSeg,
    played_seg_ring: &internal::SpscRingMusicSeg,
    current_seg: &mut Option<MusicMapSeg>,
) {
    let mut stream_frame = track_frame_start;
    let mut remaining = frames_popped.max(0);
    while remaining > 0 {
        let mut seg = match current_seg.take() {
            Some(seg) => seg,
            None => match internal::music_seg_ring_pop(queued_seg_ring) {
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
        let _ = internal::music_seg_ring_push(played_seg_ring, played);
        seg.frames -= take;
        seg.music_start_sec += seg.music_sec_per_frame * take as f64;
        stream_frame += take;
        remaining -= take;
        if seg.frames > 0 {
            *current_seg = Some(seg);
        }
    }
}

fn init_engine_and_thread() -> AudioEngine {
    let (command_sender, command_receiver) = channel();

    let host = cpal::default_host();
    let default_device = host
        .default_output_device()
        .expect("no audio output device");
    let default_device_name = cpal_device_name(&default_device);
    let mut device_probes = enumerate_output_device_probes(&host, default_device_name.as_str());
    if device_probes.is_empty() {
        let fallback_rates = collect_supported_sample_rates(&default_device);
        device_probes.push(OutputDeviceProbe {
            device: default_device.clone(),
            info: OutputDeviceInfo {
                name: default_device_name.clone(),
                is_default: true,
                sample_rates_hz: fallback_rates,
            },
        });
    }

    let cfg = crate::config::get();
    let mut device = default_device;
    let mut device_name = default_device_name;
    if let Some(requested_idx) = cfg.audio_output_device_index {
        if let Some(probe) = device_probes.get(requested_idx as usize) {
            device = probe.device.clone();
            device_name = probe.info.name.clone();
            info!(
                "Audio output device override selected: index {} '{}'.",
                requested_idx, device_name
            );
        } else {
            warn!(
                "Audio output device override index {} not found; using default device.",
                requested_idx
            );
        }
    }

    let default_config = device
        .default_output_config()
        .expect("no default audio config");
    let mut stream_config: StreamConfig = default_config.clone().into();
    let requested_rate = cfg.audio_sample_rate_hz;
    if let Some(target_hz) = requested_rate {
        debug!(
            "Audio sample rate override requested: {} Hz (device default {} Hz).",
            target_hz, stream_config.sample_rate
        );
        stream_config.sample_rate = target_hz;
    } else {
        debug!(
            "Audio sample rate override: auto (using device default {} Hz).",
            stream_config.sample_rate
        );
    }

    debug!(
        "Audio device: '{}' (sample_format={:?}, default={} Hz, channels={}).",
        device_name,
        default_config.sample_format(),
        default_config.sample_rate(),
        default_config.channels()
    );
    debug!(
        "Audio output stream config: {} Hz, {} ch (may be resampled by OS/driver).",
        stream_config.sample_rate, stream_config.channels
    );

    let device_sample_rate = stream_config.sample_rate;
    let device_channels = stream_config.channels as usize;

    // Spawn the audio manager thread (owns the CPAL stream and command loop)
    thread::spawn(move || {
        audio_manager_thread(
            command_receiver,
            device,
            default_config.sample_format(),
            stream_config,
        );
    });

    info!("Audio engine initialized ({device_sample_rate} Hz, {device_channels} ch).");
    AudioEngine {
        command_sender,
        sfx_cache: Mutex::new(HashMap::new()),
        device_sample_rate,
        device_channels,
        startup_output_devices: device_probes.into_iter().map(|probe| probe.info).collect(),
    }
}

#[inline(always)]
fn cpal_device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|desc| desc.name().to_string())
        .unwrap_or_else(|_| "<unknown>".to_string())
}

fn sample_rates_from_ranges(ranges: &[(u32, u32)], default_rate_hz: u32) -> Vec<u32> {
    const COMMON_SAMPLE_RATES: [u32; 11] = [
        11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000, 384000,
    ];

    let mut rates = Vec::with_capacity(COMMON_SAMPLE_RATES.len() + 4);
    if default_rate_hz > 0 {
        rates.push(default_rate_hz);
    }
    for &hz in &COMMON_SAMPLE_RATES {
        if ranges.iter().any(|&(min, max)| hz >= min && hz <= max) {
            rates.push(hz);
        }
    }
    for &(min, max) in ranges {
        rates.push(min);
        rates.push(max);
    }
    rates.sort_unstable();
    rates.dedup();
    rates
}

fn collect_supported_sample_rates(device: &cpal::Device) -> Vec<u32> {
    let default_rate_hz = device
        .default_output_config()
        .map(|cfg| cfg.sample_rate())
        .unwrap_or(0);
    let mut ranges = Vec::new();
    match device.supported_output_configs() {
        Ok(configs) => {
            for cfg_range in configs {
                let min = cfg_range.min_sample_rate();
                let max = cfg_range.max_sample_rate();
                ranges.push((min.min(max), max.max(min)));
            }
        }
        Err(_) => {
            if default_rate_hz > 0 {
                return vec![default_rate_hz];
            }
            return Vec::new();
        }
    }
    let mut rates = sample_rates_from_ranges(&ranges, default_rate_hz);
    if rates.is_empty() && default_rate_hz > 0 {
        rates.push(default_rate_hz);
    }
    rates
}

fn enumerate_output_device_probes(
    host: &cpal::Host,
    default_device_name: &str,
) -> Vec<OutputDeviceProbe> {
    let mut probes = Vec::new();
    match host.output_devices() {
        Ok(devices) => {
            debug!("Enumerating audio output devices for host {:?}:", host.id());
            for (idx, dev) in devices.enumerate() {
                let name = cpal_device_name(&dev);
                let is_default = name == default_device_name;
                let tag = if is_default { " (default)" } else { "" };
                debug!("  Device {idx}: '{name}'{tag}");
                let sample_rates_hz = match dev.supported_output_configs() {
                    Ok(configs) => {
                        let mut ranges = Vec::new();
                        for cfg_range in configs {
                            let min = cfg_range.min_sample_rate();
                            let max = cfg_range.max_sample_rate();
                            let channels = cfg_range.channels();
                            let fmt = cfg_range.sample_format();
                            debug!("    - {fmt:?}, {channels} ch, {min}..{max} Hz");
                            ranges.push((min.min(max), max.max(min)));
                        }
                        let default_rate_hz = dev
                            .default_output_config()
                            .map(|cfg| cfg.sample_rate())
                            .unwrap_or(0);
                        sample_rates_from_ranges(&ranges, default_rate_hz)
                    }
                    Err(e) => {
                        warn!("    ! Failed to query supported output configs: {e}");
                        collect_supported_sample_rates(&dev)
                    }
                };
                probes.push(OutputDeviceProbe {
                    device: dev,
                    info: OutputDeviceInfo {
                        name,
                        is_default,
                        sample_rates_hz,
                    },
                });
            }
        }
        Err(e) => {
            warn!("Failed to enumerate audio output devices: {e}");
        }
    }
    probes
}

/// Manager thread: builds the CPAL stream, mixes SFX, and forwards music via ring.
fn audio_manager_thread(
    command_receiver: Receiver<AudioCommand>,
    device: cpal::Device,
    sample_format: SampleFormat,
    stream_config: StreamConfig,
) {
    let mut music_stream: Option<MusicStream> = None;
    let music_ring = internal::ring_new(internal::RING_CAP_SAMPLES);
    let (sfx_sender, sfx_receiver) = channel::<QueuedSfx>();

    // State captured by the audio callback
    let music_ring_for_callback = music_ring.clone();

    let device_channels = stream_config.channels as usize;

    // Reusable buffers captured by the callback to avoid allocations
    let mut mix_i16: Vec<i16> = Vec::new();
    let mut mix_f32: Vec<f32> = Vec::new();
    let mut active_sfx_for_callback: Vec<(Arc<Vec<i16>>, usize, SfxLane)> = Vec::new();
    let queued_music_map_for_callback = QUEUED_MUSIC_MAP_SEGS.clone();
    let played_music_map_for_callback = PLAYED_MUSIC_MAP_SEGS.clone();
    let mut active_music_map_for_callback: Option<MusicMapSeg> = None;
    let mut music_map_generation_for_callback = MUSIC_MAP_GEN.load(Ordering::Acquire);

    // Build the output stream matching chosen sample format (like v1)
    let stream = match sample_format {
        SampleFormat::I16 => device.build_output_stream(
            &stream_config,
            move |out: &mut [i16], info| {
                let anchor_at = output_playback_anchor(Instant::now(), info);
                let map_generation = MUSIC_MAP_GEN.load(Ordering::Acquire);
                if map_generation != music_map_generation_for_callback {
                    active_music_map_for_callback = None;
                    music_map_generation_for_callback = map_generation;
                }
                if !MUSIC_TRACK_ACTIVE.load(Ordering::Relaxed) {
                    active_music_map_for_callback = None;
                }
                let total_before = MUSIC_TOTAL_FRAMES.load(Ordering::Relaxed);
                publish_callback_window_start(total_before, anchor_at);
                let config = crate::config::audio_mix_levels();
                let master_vol = f32::from(config.master_volume) * 0.01;
                let music_vol = f32::from(config.music_volume) * 0.01;
                let sfx_vol = f32::from(config.sfx_volume) * 0.01;
                let assist_tick_vol = f32::from(config.assist_tick_volume) * 0.01;
                let final_music_vol = master_vol * music_vol;
                let final_sfx_vol = master_vol * sfx_vol;
                let final_assist_tick_vol = master_vol * assist_tick_vol;

                if mix_i16.len() != out.len() {
                    mix_i16.resize(out.len(), 0);
                }
                if mix_f32.len() != out.len() {
                    mix_f32.resize(out.len(), 0.0);
                }

                // Pull music samples
                let popped = internal::callback_fill_from_ring_i16(
                    &music_ring_for_callback,
                    &mut mix_i16[..],
                );

                // Detect the first callback that actually consumed music data
                // for the currently active track and record its starting frame.
                if MUSIC_TRACK_ACTIVE.load(Ordering::Relaxed)
                    && !MUSIC_TRACK_HAS_STARTED.load(Ordering::Acquire)
                    && popped > 0
                {
                    MUSIC_TRACK_START_FRAME.store(total_before, Ordering::Release);
                    MUSIC_TRACK_HAS_STARTED.store(true, Ordering::Release);
                }

                // Convert music to f32 with volume
                for (f, s) in mix_f32.iter_mut().zip(&mix_i16) {
                    *f = s.to_sample::<f32>() * final_music_vol;
                }

                for new_sfx in sfx_receiver.try_iter() {
                    active_sfx_for_callback.push((new_sfx.data, 0, new_sfx.lane));
                }

                active_sfx_for_callback.retain_mut(|(data, cursor, lane)| {
                    let n = (data.len().saturating_sub(*cursor)).min(mix_f32.len());
                    let lane_vol = match *lane {
                        SfxLane::Effect => final_sfx_vol,
                        SfxLane::AssistTick => final_assist_tick_vol,
                    };
                    for i in 0..n {
                        let sfx_sample_f32 = data[*cursor + i].to_sample::<f32>() * lane_vol;
                        mix_f32[i] = (mix_f32[i] + sfx_sample_f32).clamp(-1.0, 1.0);
                    }
                    *cursor += n;
                    *cursor < data.len()
                });

                for (o, f) in out.iter_mut().zip(&mix_f32) {
                    *o = i16::from_sample(*f);
                }

                // Advance the global frame counter after emitting this buffer.
                let frames = if device_channels == 0 {
                    0
                } else {
                    out.len() / device_channels
                };
                let popped_frames = if device_channels == 0 {
                    0
                } else {
                    popped / device_channels
                };
                let track_frames_before =
                    total_before.saturating_sub(MUSIC_TRACK_START_FRAME.load(Ordering::Acquire));
                if popped_frames > 0 {
                    commit_played_music_map(
                        track_frames_before as i64,
                        popped_frames as i64,
                        &queued_music_map_for_callback,
                        &played_music_map_for_callback,
                        &mut active_music_map_for_callback,
                    );
                }
                if frames > 0 {
                    publish_callback_window_end(total_before, frames as u64);
                }
            },
            |err| error!("Audio stream error: {err}"),
            None,
        ),
        SampleFormat::U16 => device.build_output_stream(
            &stream_config,
            move |out: &mut [u16], info| {
                let anchor_at = output_playback_anchor(Instant::now(), info);
                let map_generation = MUSIC_MAP_GEN.load(Ordering::Acquire);
                if map_generation != music_map_generation_for_callback {
                    active_music_map_for_callback = None;
                    music_map_generation_for_callback = map_generation;
                }
                if !MUSIC_TRACK_ACTIVE.load(Ordering::Relaxed) {
                    active_music_map_for_callback = None;
                }
                let total_before = MUSIC_TOTAL_FRAMES.load(Ordering::Relaxed);
                publish_callback_window_start(total_before, anchor_at);
                let config = crate::config::audio_mix_levels();
                let master_vol = f32::from(config.master_volume) * 0.01;
                let music_vol = f32::from(config.music_volume) * 0.01;
                let sfx_vol = f32::from(config.sfx_volume) * 0.01;
                let assist_tick_vol = f32::from(config.assist_tick_volume) * 0.01;
                let final_music_vol = master_vol * music_vol;
                let final_sfx_vol = master_vol * sfx_vol;
                let final_assist_tick_vol = master_vol * assist_tick_vol;

                if mix_i16.len() != out.len() {
                    mix_i16.resize(out.len(), 0);
                }
                if mix_f32.len() != out.len() {
                    mix_f32.resize(out.len(), 0.0);
                }

                let popped = internal::callback_fill_from_ring_i16(
                    &music_ring_for_callback,
                    &mut mix_i16[..],
                );

                if MUSIC_TRACK_ACTIVE.load(Ordering::Relaxed)
                    && !MUSIC_TRACK_HAS_STARTED.load(Ordering::Acquire)
                    && popped > 0
                {
                    MUSIC_TRACK_START_FRAME.store(total_before, Ordering::Release);
                    MUSIC_TRACK_HAS_STARTED.store(true, Ordering::Release);
                }

                for (f, s) in mix_f32.iter_mut().zip(&mix_i16) {
                    *f = s.to_sample::<f32>() * final_music_vol;
                }

                for new_sfx in sfx_receiver.try_iter() {
                    active_sfx_for_callback.push((new_sfx.data, 0, new_sfx.lane));
                }

                active_sfx_for_callback.retain_mut(|(data, cursor, lane)| {
                    let n = (data.len().saturating_sub(*cursor)).min(mix_f32.len());
                    let lane_vol = match *lane {
                        SfxLane::Effect => final_sfx_vol,
                        SfxLane::AssistTick => final_assist_tick_vol,
                    };
                    for i in 0..n {
                        let sfx_sample_f32 = data[*cursor + i].to_sample::<f32>() * lane_vol;
                        mix_f32[i] = (mix_f32[i] + sfx_sample_f32).clamp(-1.0, 1.0);
                    }
                    *cursor += n;
                    *cursor < data.len()
                });

                for (o, f) in out.iter_mut().zip(&mix_f32) {
                    *o = u16::from_sample(*f);
                }

                let frames = if device_channels == 0 {
                    0
                } else {
                    out.len() / device_channels
                };
                let popped_frames = if device_channels == 0 {
                    0
                } else {
                    popped / device_channels
                };
                let track_frames_before =
                    total_before.saturating_sub(MUSIC_TRACK_START_FRAME.load(Ordering::Acquire));
                if popped_frames > 0 {
                    commit_played_music_map(
                        track_frames_before as i64,
                        popped_frames as i64,
                        &queued_music_map_for_callback,
                        &played_music_map_for_callback,
                        &mut active_music_map_for_callback,
                    );
                }
                if frames > 0 {
                    publish_callback_window_end(total_before, frames as u64);
                }
            },
            |err| error!("Audio stream error: {err}"),
            None,
        ),
        SampleFormat::F32 => device.build_output_stream(
            &stream_config,
            move |out: &mut [f32], info| {
                let anchor_at = output_playback_anchor(Instant::now(), info);
                let map_generation = MUSIC_MAP_GEN.load(Ordering::Acquire);
                if map_generation != music_map_generation_for_callback {
                    active_music_map_for_callback = None;
                    music_map_generation_for_callback = map_generation;
                }
                if !MUSIC_TRACK_ACTIVE.load(Ordering::Relaxed) {
                    active_music_map_for_callback = None;
                }
                let total_before = MUSIC_TOTAL_FRAMES.load(Ordering::Relaxed);
                publish_callback_window_start(total_before, anchor_at);
                let config = crate::config::audio_mix_levels();
                let master_vol = f32::from(config.master_volume) * 0.01;
                let music_vol = f32::from(config.music_volume) * 0.01;
                let sfx_vol = f32::from(config.sfx_volume) * 0.01;
                let assist_tick_vol = f32::from(config.assist_tick_volume) * 0.01;
                let final_music_vol = master_vol * music_vol;
                let final_sfx_vol = master_vol * sfx_vol;
                let final_assist_tick_vol = master_vol * assist_tick_vol;

                if mix_i16.len() != out.len() {
                    mix_i16.resize(out.len(), 0);
                }

                let popped = internal::callback_fill_from_ring_i16(
                    &music_ring_for_callback,
                    &mut mix_i16[..],
                );

                if MUSIC_TRACK_ACTIVE.load(Ordering::Relaxed)
                    && !MUSIC_TRACK_HAS_STARTED.load(Ordering::Acquire)
                    && popped > 0
                {
                    MUSIC_TRACK_START_FRAME.store(total_before, Ordering::Release);
                    MUSIC_TRACK_HAS_STARTED.store(true, Ordering::Release);
                }

                for (o, s) in out.iter_mut().zip(&mix_i16) {
                    *o = s.to_sample::<f32>() * final_music_vol;
                }

                for new_sfx in sfx_receiver.try_iter() {
                    active_sfx_for_callback.push((new_sfx.data, 0, new_sfx.lane));
                }

                active_sfx_for_callback.retain_mut(|(data, cursor, lane)| {
                    let n = (data.len().saturating_sub(*cursor)).min(out.len());
                    let lane_vol = match *lane {
                        SfxLane::Effect => final_sfx_vol,
                        SfxLane::AssistTick => final_assist_tick_vol,
                    };
                    for i in 0..n {
                        let sfx_sample_f32 = data[*cursor + i].to_sample::<f32>() * lane_vol;
                        out[i] = (out[i] + sfx_sample_f32).clamp(-1.0, 1.0);
                    }
                    *cursor += n;
                    *cursor < data.len()
                });

                let frames = if device_channels == 0 {
                    0
                } else {
                    out.len() / device_channels
                };
                let popped_frames = if device_channels == 0 {
                    0
                } else {
                    popped / device_channels
                };
                let track_frames_before =
                    total_before.saturating_sub(MUSIC_TRACK_START_FRAME.load(Ordering::Acquire));
                if popped_frames > 0 {
                    commit_played_music_map(
                        track_frames_before as i64,
                        popped_frames as i64,
                        &queued_music_map_for_callback,
                        &played_music_map_for_callback,
                        &mut active_music_map_for_callback,
                    );
                }
                if frames > 0 {
                    publish_callback_window_end(total_before, frames as u64);
                }
            },
            |err| error!("Audio stream error: {err}"),
            None,
        ),
        _ => unreachable!(),
    }
    .expect("Failed to build audio stream");

    stream.play().expect("Failed to play audio stream");

    // Command loop: manage music decoder thread and pass SFX to the callback
    loop {
        match command_receiver.recv() {
            Ok(AudioCommand::PlaySfx(queued)) => {
                let _ = sfx_sender.send(queued);
            }
            Ok(AudioCommand::PlayMusic(path, cut, looping, rate)) => {
                if let Some(old) = music_stream.take() {
                    old.stop_signal
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = old.thread.join();
                }
                internal::ring_clear(&music_ring);
                MUSIC_TRACK_ACTIVE.store(true, Ordering::Relaxed);
                MUSIC_TRACK_HAS_STARTED.store(false, Ordering::Relaxed);
                let rate_bits = Arc::new(AtomicU32::new(rate.to_bits()));
                music_stream = Some(spawn_music_decoder_thread(
                    path,
                    cut,
                    looping,
                    rate_bits,
                    music_ring.clone(),
                ));
            }
            Ok(AudioCommand::StopMusic) => {
                if let Some(old) = music_stream.take() {
                    old.stop_signal
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = old.thread.join();
                }
                internal::ring_clear(&music_ring);
                MUSIC_TRACK_ACTIVE.store(false, Ordering::Relaxed);
                MUSIC_TRACK_HAS_STARTED.store(false, Ordering::Relaxed);
            }
            Ok(AudioCommand::SetMusicRate(new_rate)) => {
                if let Some(ms) = &music_stream {
                    ms.rate_bits.store(new_rate.to_bits(), Ordering::Relaxed);
                }
                // Drop buffered old-rate samples so the change is heard immediately.
                internal::ring_clear(&music_ring);
                clear_music_pos_map();
            }
            Err(_) => break, // main dropped; exit thread
        }
    }
}

/* ========================= Music decode + resample ========================= */

/// Spawn a thread to decode & resample one music file into the ring buffer.
fn spawn_music_decoder_thread(
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
    if !seconds.is_finite() {
        0
    } else {
        (seconds.max(0.0) * f64::from(sample_rate)).round() as u64
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

/// The decoder loop, mirrored from v1 (seek+preroll, cut capping, flush).
fn music_decoder_thread_loop(
    path: PathBuf,
    cut: Cut,
    looping: bool,
    rate_bits: Arc<AtomicU32>,
    ring: Arc<internal::SpscRingI16>,
    stop: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(&path)?;
    let mut ogg = OggStreamReader::new(BufReader::new(file))?;
    let in_ch = ogg.ident_hdr.audio_channels as usize;
    let in_hz = ogg.ident_hdr.audio_sample_rate;

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

    // --- Handle negative start time as preroll silence ---
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
        // --- rubato SincFixedOut setup ---
        const OUT_FRAMES_PER_CALL: usize = 256;
        // Adjust ratio by 1/rate to speed up (rate>1) or slow down (rate<1)
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

        // --- v1-style start & pre-roll ---
        let start_frame_f = (cut.start_sec * f64::from(in_hz)).max(0.0);
        let start_floor = start_frame_f.floor() as u64;

        // Try to seek a little before start to fill FIR, else fall back to decode+drop
        let mut seek_ok = true;
        if start_floor > 0 {
            let seek_frame = start_floor.saturating_sub(internal::PREROLL_IN_FRAMES);
            if ogg.seek_absgp_pg(seek_frame).is_err() {
                seek_ok = false;
            }
        }

        // How many output frames to throw away to finish pre-roll?
        let mut preroll_out_frames: u64 = if seek_ok && start_floor > 0 {
            (internal::PREROLL_IN_FRAMES as f64 * ratio).ceil() as u64
        } else {
            0
        };

        // If seek failed, decode and drop input frames until we hit start
        let mut to_drop_in: u64 = if seek_ok { 0 } else { start_floor };

        let fade_in_frames = secs_to_frames(cut.fade_in_sec, out_hz);
        let fade_out_frames = secs_to_frames(cut.fade_out_sec, out_hz);

        // Optional cut length in output frames
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

        // Accumulates decoded input per channel for rubato
        let mut in_planar: Vec<Vec<f32>> = vec![Vec::with_capacity(4096); in_ch];

        // Helper: push interleaved i16 samples into planar f32
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

        // Produce blocks as long as enough input frames are buffered for the next call.
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
                if produced_frames == 0 {
                    break;
                }
                let music_sec_per_frame =
                    (need as f64 / f64::from(in_hz.max(1))) / produced_frames as f64;
                out_tmp.clear();
                out_tmp.reserve(produced_frames * out_ch);

                // Interleave without a needless range loop.
                for (f, _) in out[0].iter().enumerate() {
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
                        *preroll_out_frames =
                            (*preroll_out_frames).saturating_sub(drop_frames as u64);
                        *next_music_output_sec += drop_frames as f64 * music_sec_per_frame;
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

        // --- Main decode loop ---
        while let Ok(pkt_opt) = ogg.read_dec_packet_itl() {
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                break 'main_loop;
            }

            let p = match pkt_opt {
                Some(p) if !p.is_empty() => p,
                Some(_) => continue,
                None => break,
            };

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

            // Check for dynamic rate changes and rebuild resampler if needed
            let desired_rate = f32::from_bits(rate_bits.load(Ordering::Relaxed));
            let mut desired_rate = if desired_rate.is_finite() && desired_rate > 0.0 {
                desired_rate
            } else {
                1.0
            };
            // Avoid very tiny thrash around last digit
            if (desired_rate - current_rate_f32).abs() > 0.0005 {
                // Clamp to sane bounds
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
            // Stop if cut length has been reached
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
            let finished = matches!(frames_left_out, Some(0));
            if finished {
                break;
            }
        }

        // --- Flush remainder ---
        if !in_planar.iter().all(std::vec::Vec::is_empty) {
            // Process the final short chunk using process_partial
            let mut input_slices: Vec<&[f32]> = Vec::with_capacity(in_planar.len());
            let remain = in_planar.iter().map(std::vec::Vec::len).min().unwrap_or(0);
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

                // Interleave without a needless range loop.
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
        // Final tail flush from the resampler
        // Tail flush of delayed frames
        let out_tail = resampler.process_partial::<&[f32]>(None, None)?;
        if !out_tail.is_empty() {
            let produced_frames = out_tail[0].len();
            let music_sec_per_frame = f64::from(current_rate_f32) / f64::from(out_hz.max(1));
            out_tmp.clear();
            out_tmp.reserve(produced_frames * out_ch);

            // Interleave without a needless range loop.
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

        // --- Looping logic ---
        if !looping {
            break 'main_loop;
        }
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            break 'main_loop;
        }

        // Re-open the file for the next loop iteration (gapless enough; continue with fresh resampler)
        match File::open(&path)
            .ok()
            .and_then(|f| OggStreamReader::new(BufReader::new(f)).ok())
        {
            Some(new_reader) => {
                debug!("Looping music: restarted {path:?}");
                ogg = new_reader;
            }
            None => {
                warn!("Could not reopen OGG stream for looping: {path:?}");
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

    debug!(
        "SFX decode: '{path}' ({in_ch} ch @ {in_hz} Hz) -> output {out_ch} ch @ {out_hz} Hz (ratio {ratio:.6})."
    );

    let mut resampled_data: Vec<i16> = Vec::new();
    let mut in_planar: Vec<Vec<f32>> = vec![Vec::with_capacity(4096); in_ch];

    while let Some(pkt) = ogg.read_dec_packet_itl()? {
        let frames = pkt.len() / in_ch;
        if frames == 0 {
            continue;
        }

        // Push into planar
        for f in 0..frames {
            let base = f * in_ch;
            for c in 0..in_ch {
                in_planar[c].push(f32::from(pkt[base + c]) / 32768.0);
            }
        }

        // Produce as many blocks as possible based on required input
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

            // Interleave without a needless range loop.
            for (f, _) in out[0].iter().enumerate() {
                for c in 0..out_ch {
                    let v = out[c % out.len()][f];
                    let s = (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                    resampled_data.push(s);
                }
            }
        }
    }

    // Flush any remaining samples
    if !in_planar.iter().all(std::vec::Vec::is_empty) {
        let remain = in_planar.iter().map(std::vec::Vec::len).min().unwrap_or(0);
        let mut input_slices: Vec<&[f32]> = Vec::with_capacity(in_planar.len());
        for ch in &in_planar {
            input_slices.push(&ch[..remain]);
        }

        let out = resampler.process_partial(Some(&input_slices), None)?;
        if !out.is_empty() {
            let produced_frames = out[0].len();
            resampled_data.reserve(produced_frames * out_ch);

            // Interleave without a needless range loop.
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

    // Tail flush
    let out_tail = resampler.process_partial::<&[f32]>(None, None)?;
    if !out_tail.is_empty() {
        let produced_frames = out_tail[0].len();
        resampled_data.reserve(produced_frames * out_ch);

        // Interleave without a needless range loop.
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

/* =========================== Internal primitives =========================== */

mod internal {
    use super::{Arc, MusicMapSeg};
    use std::cell::UnsafeCell;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Pre-roll input frames and ring capacity
    pub const PREROLL_IN_FRAMES: u64 = 8;
    pub const RING_CAP_SAMPLES: usize = 1 << 16; // interleaved i16 samples (smaller = snappier)
    pub const MUSIC_SEG_RING_CAP: usize = 1 << 11;

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
    fn ring_cap(r: &SpscRingI16) -> usize {
        unsafe { (&*r.buf.get()).len() }
    }

    #[inline(always)]
    pub fn ring_free_samples(r: &SpscRingI16) -> usize {
        let cap = ring_cap(r);
        let h = r.head.load(Ordering::Relaxed);
        let t = r.tail.load(Ordering::Acquire);
        cap.saturating_sub(h.wrapping_sub(t))
    }

    pub fn ring_push(r: &SpscRingI16, data: &[i16]) -> usize {
        let cap = ring_cap(r);
        let mask = r.mask;
        let h = r.head.load(Ordering::Relaxed);
        let t = r.tail.load(Ordering::Acquire);
        let free = cap - h.wrapping_sub(t);
        let n = data.len().min(free);
        if n == 0 {
            return 0;
        }
        let idx = h & mask;
        unsafe {
            let buf = &mut *r.buf.get();
            let first = (cap - idx).min(n);
            buf[idx..idx + first].copy_from_slice(&data[..first]);
            if n > first {
                buf[0..(n - first)].copy_from_slice(&data[first..n]);
            }
        }
        r.head.store(h.wrapping_add(n), Ordering::Release);
        n
    }

    pub fn ring_pop(r: &SpscRingI16, out: &mut [i16]) -> usize {
        let cap = ring_cap(r);
        let mask = r.mask;
        let h = r.head.load(Ordering::Acquire);
        let t = r.tail.load(Ordering::Relaxed);
        let avail = h.wrapping_sub(t);
        let n = out.len().min(avail);
        if n == 0 {
            return 0;
        }
        let idx = t & mask;
        unsafe {
            let buf = &*r.buf.get();
            let first = (cap - idx).min(n);
            out[..first].copy_from_slice(&buf[idx..idx + first]);
            if n > first {
                out[first..n].copy_from_slice(&buf[0..(n - first)]);
            }
        }
        r.tail.store(t.wrapping_add(n), Ordering::Release);
        n
    }

    pub fn ring_clear(r: &SpscRingI16) {
        // This is called from the manager thread when the producer (decoder) is stopped.
        // It makes the buffer appear empty to the consumer (audio callback).
        let tail_pos = r.tail.load(Ordering::Relaxed);
        r.head.store(tail_pos, Ordering::Release);
    }

    /// Fill `dst` from the ring buffer, returning the number of interleaved
    /// samples actually popped from the ring. Any remaining slots are zeroed.
    pub fn callback_fill_from_ring_i16(ring: &SpscRingI16, dst: &mut [i16]) -> usize {
        let mut filled = 0;
        while filled < dst.len() {
            let got = ring_pop(ring, &mut dst[filled..]);
            if got == 0 {
                // underrun: zero the rest
                for d in &mut dst[filled..] {
                    *d = 0;
                }
                break;
            }
            filled += got;
        }
        filled
    }

    pub struct SpscRingMusicSeg {
        buf: UnsafeCell<Box<[MusicMapSeg]>>,
        mask: usize,
        head: AtomicUsize,
        tail: AtomicUsize,
    }
    unsafe impl Send for SpscRingMusicSeg {}
    unsafe impl Sync for SpscRingMusicSeg {}

    pub fn music_seg_ring_new(cap_pow2: usize) -> Arc<SpscRingMusicSeg> {
        assert!(cap_pow2.is_power_of_two());
        Arc::new(SpscRingMusicSeg {
            buf: UnsafeCell::new(vec![MusicMapSeg::default(); cap_pow2].into_boxed_slice()),
            mask: cap_pow2 - 1,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        })
    }

    #[inline(always)]
    fn music_seg_ring_cap(r: &SpscRingMusicSeg) -> usize {
        unsafe { (&*r.buf.get()).len() }
    }

    #[inline(always)]
    pub fn music_seg_ring_has_space(r: &SpscRingMusicSeg) -> bool {
        let cap = music_seg_ring_cap(r);
        let h = r.head.load(Ordering::Relaxed);
        let t = r.tail.load(Ordering::Acquire);
        h.wrapping_sub(t) < cap
    }

    pub fn music_seg_ring_push(r: &SpscRingMusicSeg, seg: MusicMapSeg) -> bool {
        let cap = music_seg_ring_cap(r);
        let h = r.head.load(Ordering::Relaxed);
        let t = r.tail.load(Ordering::Acquire);
        if h.wrapping_sub(t) >= cap {
            return false;
        }
        let idx = h & r.mask;
        unsafe {
            (&mut *r.buf.get())[idx] = seg;
        }
        r.head.store(h.wrapping_add(1), Ordering::Release);
        true
    }

    pub fn music_seg_ring_pop(r: &SpscRingMusicSeg) -> Option<MusicMapSeg> {
        let h = r.head.load(Ordering::Acquire);
        let t = r.tail.load(Ordering::Relaxed);
        if h == t {
            return None;
        }
        let idx = t & r.mask;
        let seg = unsafe { (&*r.buf.get())[idx] };
        r.tail.store(t.wrapping_add(1), Ordering::Release);
        Some(seg)
    }

    pub fn music_seg_ring_clear(r: &SpscRingMusicSeg) {
        let tail_pos = r.tail.load(Ordering::Relaxed);
        r.head.store(tail_pos, Ordering::Release);
    }
}
