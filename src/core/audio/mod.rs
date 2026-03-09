mod backends;
mod resample;

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Sample, StreamConfig};
use log::{debug, info, warn};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
#[cfg(windows)]
use std::time::Duration;
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
    #[cfg(windows)]
    wasapi_id: Option<String>,
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

#[cfg(windows)]
#[derive(Clone)]
struct WasapiBackendHint {
    device_id: Option<String>,
    device_name: String,
    requested_rate_hz: Option<u32>,
}

#[derive(Clone)]
struct AudioThreadLaunch {
    cpal: backends::cpal::CpalBackendLaunch,
    #[cfg(windows)]
    wasapi: Option<WasapiBackendHint>,
}

#[derive(Clone, Debug)]
struct OutputBackendReady {
    device_sample_rate: u32,
    device_channels: usize,
    device_name: String,
    backend_name: &'static str,
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
            match resample::load_and_resample_sfx(path) {
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
    match resample::load_and_resample_sfx(path) {
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

struct RenderState {
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
    device_channels: usize,
    mix_i16: Vec<i16>,
    mix_f32: Vec<f32>,
    active_sfx: Vec<(Arc<Vec<i16>>, usize, SfxLane)>,
    queued_music_map: Arc<internal::SpscRingMusicSeg>,
    played_music_map: Arc<internal::SpscRingMusicSeg>,
    active_music_map: Option<MusicMapSeg>,
    music_map_generation: u64,
}

impl RenderState {
    fn new(
        music_ring: Arc<internal::SpscRingI16>,
        sfx_receiver: Receiver<QueuedSfx>,
        device_channels: usize,
    ) -> Self {
        Self {
            music_ring,
            sfx_receiver,
            device_channels,
            mix_i16: Vec::new(),
            mix_f32: Vec::new(),
            active_sfx: Vec::new(),
            queued_music_map: QUEUED_MUSIC_MAP_SEGS.clone(),
            played_music_map: PLAYED_MUSIC_MAP_SEGS.clone(),
            active_music_map: None,
            music_map_generation: MUSIC_MAP_GEN.load(Ordering::Acquire),
        }
    }

    #[inline(always)]
    fn begin_callback(&mut self, anchor_at: Instant) -> u64 {
        let map_generation = MUSIC_MAP_GEN.load(Ordering::Acquire);
        if map_generation != self.music_map_generation {
            self.active_music_map = None;
            self.music_map_generation = map_generation;
        }
        if !MUSIC_TRACK_ACTIVE.load(Ordering::Relaxed) {
            self.active_music_map = None;
        }
        let total_before = MUSIC_TOTAL_FRAMES.load(Ordering::Relaxed);
        publish_callback_window_start(total_before, anchor_at);
        total_before
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

    #[inline(always)]
    fn mix_levels() -> (f32, f32, f32) {
        let config = crate::config::audio_mix_levels();
        let master_vol = f32::from(config.master_volume) * 0.01;
        let music_vol = f32::from(config.music_volume) * 0.01;
        let sfx_vol = f32::from(config.sfx_volume) * 0.01;
        let assist_tick_vol = f32::from(config.assist_tick_volume) * 0.01;
        (
            master_vol * music_vol,
            master_vol * sfx_vol,
            master_vol * assist_tick_vol,
        )
    }

    fn mix_f32_buffer(&mut self, total_before: u64, len: usize) -> usize {
        self.ensure_mix_buffers(len);
        let popped = internal::callback_fill_from_ring_i16(&self.music_ring, &mut self.mix_i16);
        if MUSIC_TRACK_ACTIVE.load(Ordering::Relaxed)
            && !MUSIC_TRACK_HAS_STARTED.load(Ordering::Acquire)
            && popped > 0
        {
            MUSIC_TRACK_START_FRAME.store(total_before, Ordering::Release);
            MUSIC_TRACK_HAS_STARTED.store(true, Ordering::Release);
        }

        let (music_vol, sfx_vol, assist_tick_vol) = Self::mix_levels();
        for (dst, src) in self.mix_f32.iter_mut().zip(&self.mix_i16) {
            *dst = src.to_sample::<f32>() * music_vol;
        }

        for new_sfx in self.sfx_receiver.try_iter() {
            self.active_sfx.push((new_sfx.data, 0, new_sfx.lane));
        }

        self.active_sfx.retain_mut(|(data, cursor, lane)| {
            let n = (data.len().saturating_sub(*cursor)).min(self.mix_f32.len());
            let lane_vol = match *lane {
                SfxLane::Effect => sfx_vol,
                SfxLane::AssistTick => assist_tick_vol,
            };
            for i in 0..n {
                let sfx_sample_f32 = data[*cursor + i].to_sample::<f32>() * lane_vol;
                self.mix_f32[i] = (self.mix_f32[i] + sfx_sample_f32).clamp(-1.0, 1.0);
            }
            *cursor += n;
            *cursor < data.len()
        });

        popped
    }

    #[inline(always)]
    fn finish_callback(
        &mut self,
        total_before: u64,
        emitted_samples: usize,
        popped_samples: usize,
    ) {
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
        let track_frames_before =
            total_before.saturating_sub(MUSIC_TRACK_START_FRAME.load(Ordering::Acquire));
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
    }

    fn render_i16(&mut self, out: &mut [i16], anchor_at: Instant) {
        let total_before = self.begin_callback(anchor_at);
        let popped = self.mix_f32_buffer(total_before, out.len());
        for (dst, src) in out.iter_mut().zip(&self.mix_f32) {
            *dst = i16::from_sample(*src);
        }
        self.finish_callback(total_before, out.len(), popped);
    }

    fn render_u16(&mut self, out: &mut [u16], anchor_at: Instant) {
        let total_before = self.begin_callback(anchor_at);
        let popped = self.mix_f32_buffer(total_before, out.len());
        for (dst, src) in out.iter_mut().zip(&self.mix_f32) {
            *dst = u16::from_sample(*src);
        }
        self.finish_callback(total_before, out.len(), popped);
    }

    fn render_f32(&mut self, out: &mut [f32], anchor_at: Instant) {
        let total_before = self.begin_callback(anchor_at);
        let popped = self.mix_f32_buffer(total_before, out.len());
        out.copy_from_slice(&self.mix_f32[..out.len()]);
        self.finish_callback(total_before, out.len(), popped);
    }
}

fn init_engine_and_thread() -> AudioEngine {
    let (command_sender, command_receiver) = channel();
    let (ready_sender, ready_receiver) = channel();

    let host = cpal::default_host();
    let default_device = host
        .default_output_device()
        .expect("no audio output device");
    let default_device_name = backends::cpal::device_name(&default_device);
    let mut device_probes =
        backends::cpal::enumerate_output_device_probes(&host, default_device_name.as_str());
    if device_probes.is_empty() {
        let fallback_rates = backends::cpal::collect_supported_sample_rates(&default_device);
        device_probes.push(OutputDeviceProbe {
            device: default_device.clone(),
            info: OutputDeviceInfo {
                name: default_device_name.clone(),
                is_default: true,
                sample_rates_hz: fallback_rates,
            },
            #[cfg(windows)]
            wasapi_id: backends::cpal::device_id_string(&default_device),
        });
    }

    let cfg = crate::config::get();
    let mut device = default_device;
    let mut device_name = default_device_name;
    #[cfg(windows)]
    let mut wasapi_device_id = backends::cpal::device_id_string(&device);
    if let Some(requested_idx) = cfg.audio_output_device_index {
        if let Some(probe) = device_probes.get(requested_idx as usize) {
            device = probe.device.clone();
            device_name = probe.info.name.clone();
            #[cfg(windows)]
            {
                wasapi_device_id = probe.wasapi_id.clone();
            }
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

    let launch = AudioThreadLaunch {
        cpal: backends::cpal::CpalBackendLaunch {
            device,
            device_name: device_name.clone(),
            sample_format: default_config.sample_format(),
            stream_config,
        },
        #[cfg(windows)]
        wasapi: Some(WasapiBackendHint {
            device_id: wasapi_device_id,
            device_name: device_name.clone(),
            requested_rate_hz: requested_rate,
        }),
    };

    thread::spawn(move || {
        audio_manager_thread(command_receiver, ready_sender, launch);
    });

    let ready = match ready_receiver.recv() {
        Ok(Ok(ready)) => ready,
        Ok(Err(err)) => panic!("failed to initialize audio engine: {err}"),
        Err(_) => panic!("audio manager thread exited before reporting ready"),
    };

    info!(
        "Audio engine initialized ({} Hz, {} ch, backend={} device='{}').",
        ready.device_sample_rate, ready.device_channels, ready.backend_name, ready.device_name
    );
    AudioEngine {
        command_sender,
        sfx_cache: Mutex::new(HashMap::new()),
        device_sample_rate: ready.device_sample_rate,
        device_channels: ready.device_channels,
        startup_output_devices: device_probes.into_iter().map(|probe| probe.info).collect(),
    }
}

#[cfg(windows)]
#[inline(always)]
fn playback_anchor_after_frames(now: Instant, sample_rate: u32, frames: u32) -> Instant {
    if sample_rate == 0 || frames == 0 {
        return now;
    }
    now.checked_add(Duration::from_secs_f64(frames as f64 / sample_rate as f64))
        .unwrap_or(now)
}

#[allow(dead_code)]
enum OutputBackend {
    Cpal(cpal::Stream),
    #[cfg(windows)]
    Wasapi(backends::windows_wasapi::WasapiOutputStream),
}

fn start_output_backend(
    launch: AudioThreadLaunch,
    music_ring: Arc<internal::SpscRingI16>,
) -> Result<(OutputBackend, OutputBackendReady, Sender<QueuedSfx>), String> {
    let AudioThreadLaunch {
        cpal,
        #[cfg(windows)]
        wasapi,
    } = launch;

    #[cfg(windows)]
    if let Some(wasapi) = wasapi {
        match backends::windows_wasapi::prepare(
            wasapi.device_id.clone(),
            wasapi.device_name.clone(),
            wasapi.requested_rate_hz,
        ) {
            Ok(prep) => {
                let ready = prep.ready();
                let (sfx_sender, sfx_receiver) = channel::<QueuedSfx>();
                match backends::windows_wasapi::start(prep, music_ring.clone(), sfx_receiver) {
                    Ok(stream) => {
                        return Ok((OutputBackend::Wasapi(stream), ready, sfx_sender));
                    }
                    Err(err) => {
                        warn!(
                            "Failed to start native WASAPI output for '{}': {err}. Falling back to CPAL.",
                            wasapi.device_name
                        );
                    }
                }
            }
            Err(err) => {
                warn!(
                    "Failed to prepare native WASAPI output for '{}': {err}. Falling back to CPAL.",
                    wasapi.device_name
                );
            }
        }
    }

    backends::cpal::start_output(cpal, music_ring)
}

/// Manager thread: builds the output backend, mixes SFX, and forwards music via ring.
fn audio_manager_thread(
    command_receiver: Receiver<AudioCommand>,
    ready_sender: Sender<Result<OutputBackendReady, String>>,
    launch: AudioThreadLaunch,
) {
    let mut music_stream: Option<MusicStream> = None;
    let music_ring = internal::ring_new(internal::RING_CAP_SAMPLES);
    let (mut _backend, _ready, sfx_sender) = match start_output_backend(launch, music_ring.clone())
    {
        Ok(output) => output,
        Err(err) => {
            let _ = ready_sender.send(Err(err));
            return;
        }
    };
    if ready_sender.send(Ok(_ready)).is_err() {
        return;
    }

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
                music_stream = Some(resample::spawn_music_decoder_thread(
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
            Err(_) => break,
        }
    }

    if let Some(old) = music_stream.take() {
        old.stop_signal
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = old.thread.join();
    }
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
