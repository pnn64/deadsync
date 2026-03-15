use super::super::{
    OutputBackendReady, OutputTelemetryClock, OutputTimingQuality, QueuedSfx, RenderState,
    internal, note_output_clock_fallback, note_output_underrun, publish_output_timing,
    publish_output_timing_quality,
};
use crate::core::host_time::now_nanos;
use alsa::pcm::{Access, Format, HwParams, PCM, State, SwParams, TstampType};
use alsa::{Ctl, Direction, ValueOr};
use libc::timespec;
use log::{info, warn};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AlsaAccessMode {
    Shared,
    Exclusive,
}

impl AlsaAccessMode {
    #[inline(always)]
    const fn backend_name(self) -> &'static str {
        match self {
            Self::Shared => "alsa-shared",
            Self::Exclusive => "alsa-exclusive",
        }
    }
}

pub(crate) struct AlsaOutputPrep {
    pcm_id: String,
    device_name: String,
    sample_rate_hz: u32,
    channels: usize,
    period_frames: u32,
    buffer_frames: u32,
    host_clock: AlsaHostClock,
    mode: AlsaAccessMode,
}

impl AlsaOutputPrep {
    pub(crate) fn ready(&self) -> OutputBackendReady {
        OutputBackendReady {
            device_sample_rate: self.sample_rate_hz,
            device_channels: self.channels,
            device_name: self.device_name.clone(),
            backend_name: self.mode.backend_name(),
            requested_output_mode: match self.mode {
                AlsaAccessMode::Shared => crate::config::AudioOutputMode::Shared,
                AlsaAccessMode::Exclusive => crate::config::AudioOutputMode::Exclusive,
            },
            fallback_from_native: false,
            timing_clock: self.host_clock.telemetry_clock(),
            timing_quality: OutputTimingQuality::Trusted,
        }
    }
}

pub(crate) struct AlsaOutputStream {
    stop_flag: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for AlsaOutputStream {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(crate) struct AlsaOutputDevice {
    pub pcm_id: String,
    pub name: String,
    pub sample_rates_hz: Vec<u32>,
    pub default_rate_hz: u32,
    pub channels: usize,
    pub is_default: bool,
}

struct PhysicalOutputDevice {
    card_index: u32,
    card_name: String,
    device_index: u32,
    device_name: String,
}

pub(crate) fn enumerate_output_devices() -> Vec<AlsaOutputDevice> {
    let default_pcm_id = default_hw_pcm_id();
    let mut devices = physical_output_devices()
        .into_iter()
        .map(|device| build_output_device(device, default_pcm_id.as_deref()))
        .collect::<Vec<_>>();
    if !devices.iter().any(|device| device.is_default) {
        if let Some(device) = devices.first_mut() {
            device.is_default = true;
        }
    }
    devices
}

fn default_hw_pcm_id() -> Option<String> {
    resolved_hw_pcm_id("default").or_else(|| resolved_hw_pcm_id("sysdefault"))
}

fn physical_output_devices() -> Vec<PhysicalOutputDevice> {
    let mut devices = Vec::new();
    for card in alsa::card::Iter::new().filter_map(Result::ok) {
        let card_index = card.get_index() as u32;
        let Ok(ctl) = Ctl::new(&format!("hw:{card_index}"), false) else {
            continue;
        };
        let card_name = ctl
            .card_info()
            .ok()
            .and_then(|info| info.get_name().ok().map(str::to_string))
            .or_else(|| card.get_name().ok())
            .unwrap_or_else(|| format!("Card {card_index}"));
        for device_index in alsa::ctl::DeviceIter::new(&ctl) {
            let Ok(playback_info) = ctl.pcm_info(device_index as u32, 0, Direction::Playback)
            else {
                continue;
            };
            let device_name = playback_info
                .get_name()
                .map(str::to_string)
                .unwrap_or_else(|_| format!("Device {}", device_index));
            devices.push(PhysicalOutputDevice {
                card_index,
                card_name: card_name.clone(),
                device_index: device_index as u32,
                device_name,
            });
        }
    }
    devices
}

fn build_output_device(
    device: PhysicalOutputDevice,
    default_pcm_id: Option<&str>,
) -> AlsaOutputDevice {
    let pcm_id = format!("hw:CARD={},DEV={}", device.card_index, device.device_index);
    let name = format!("{}, {}", device.card_name, device.device_name);
    let (sample_rates_hz, default_rate_hz, channels) = match probe_output_device(&pcm_id) {
        Ok(probed) => probed,
        Err(err) => {
            warn!(
                "Failed to probe ALSA playback device '{}' ({}): {err}. Listing it with fallback defaults.",
                name, pcm_id
            );
            (Vec::new(), 48_000, 2)
        }
    };
    AlsaOutputDevice {
        is_default: default_pcm_id == Some(pcm_id.as_str()),
        name,
        pcm_id,
        sample_rates_hz,
        default_rate_hz,
        channels,
    }
}

fn probe_output_device(pcm_id: &str) -> Result<(Vec<u32>, u32, usize), String> {
    let pcm = open_pcm(pcm_id)?;
    let hw = HwParams::any(&pcm)
        .map_err(|e| format!("failed to create ALSA hw params for '{pcm_id}': {e}"))?;
    hw.test_access(Access::RWInterleaved)
        .map_err(|e| format!("missing RW interleaved access for '{pcm_id}': {e}"))?;
    hw.test_format(Format::s16())
        .map_err(|e| format!("missing signed 16-bit format for '{pcm_id}': {e}"))?;
    let channels = preferred_channels(&hw).ok_or_else(|| {
        format!("failed to determine a usable channel count for ALSA PCM '{pcm_id}'")
    })?;
    let default_rate_hz = preferred_rate_hz(&hw).ok_or_else(|| {
        format!("failed to determine a usable sample rate for ALSA PCM '{pcm_id}'")
    })?;
    Ok((
        supported_sample_rates(&hw, default_rate_hz),
        default_rate_hz,
        channels,
    ))
}

fn preferred_channels(hw: &HwParams<'_>) -> Option<usize> {
    for channels in [2u32, 1] {
        if hw.test_channels(channels).is_ok() {
            return Some(channels as usize);
        }
    }
    let min_channels = hw.get_channels_min().ok()?.max(1);
    hw.test_channels(min_channels).ok()?;
    Some(min_channels as usize)
}

fn preferred_rate_hz(hw: &HwParams<'_>) -> Option<u32> {
    const COMMON_SAMPLE_RATES: [u32; 11] = [
        48000, 44100, 96000, 88200, 192000, 176400, 32000, 22050, 16000, 11025, 384000,
    ];
    for sample_rate_hz in COMMON_SAMPLE_RATES {
        if hw.test_rate(sample_rate_hz).is_ok() {
            return Some(sample_rate_hz);
        }
    }
    let min_rate_hz = hw.get_rate_min().ok()?.max(1);
    if hw.test_rate(min_rate_hz).is_ok() {
        return Some(min_rate_hz);
    }
    let max_rate_hz = hw.get_rate_max().ok()?.max(min_rate_hz);
    hw.test_rate(max_rate_hz).ok()?;
    Some(max_rate_hz)
}

fn supported_sample_rates(hw: &HwParams<'_>, default_rate_hz: u32) -> Vec<u32> {
    const COMMON_SAMPLE_RATES: [u32; 11] = [
        11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000, 384000,
    ];
    let mut sample_rates_hz = Vec::with_capacity(COMMON_SAMPLE_RATES.len() + 3);
    if default_rate_hz > 0 {
        sample_rates_hz.push(default_rate_hz);
    }
    for sample_rate_hz in COMMON_SAMPLE_RATES {
        if hw.test_rate(sample_rate_hz).is_ok() {
            sample_rates_hz.push(sample_rate_hz);
        }
    }
    if let Ok(min_rate_hz) = hw.get_rate_min() {
        sample_rates_hz.push(min_rate_hz);
    }
    if let Ok(max_rate_hz) = hw.get_rate_max() {
        sample_rates_hz.push(max_rate_hz);
    }
    sample_rates_hz.sort_unstable();
    sample_rates_hz.dedup();
    sample_rates_hz
}

pub(crate) fn prepare(
    pcm_id: Option<String>,
    device_name: String,
    sample_rate_hz: u32,
    channels: usize,
    mode: AlsaAccessMode,
) -> Result<AlsaOutputPrep, String> {
    let pcm_id = selected_pcm_id(pcm_id, &device_name, mode)?;
    let pcm = open_pcm(&pcm_id)?;
    let actual = configure_probe(&pcm, sample_rate_hz, channels.max(1), mode)?;
    Ok(AlsaOutputPrep {
        pcm_id,
        device_name,
        sample_rate_hz: actual.sample_rate_hz,
        channels: actual.channels,
        period_frames: actual.period_frames,
        buffer_frames: actual.buffer_frames,
        host_clock: actual.host_clock,
        mode,
    })
}

pub(crate) fn start(
    prep: AlsaOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
) -> Result<AlsaOutputStream, String> {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_thread = stop_flag.clone();
    let (ready_tx, ready_rx) = channel::<Result<(), String>>();
    let thread = thread::Builder::new()
        .name("alsa_out".to_string())
        .spawn(move || render_thread(prep, music_ring, sfx_receiver, stop_flag_thread, ready_tx))
        .map_err(|e| format!("failed to spawn ALSA render thread: {e}"))?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(AlsaOutputStream {
            stop_flag,
            thread: Some(thread),
        }),
        Ok(Err(err)) => {
            let _ = thread.join();
            Err(err)
        }
        Err(_) => {
            let _ = thread.join();
            Err("ALSA render thread exited during startup".to_string())
        }
    }
}

struct AlsaParams {
    sample_rate_hz: u32,
    channels: usize,
    period_frames: u32,
    buffer_frames: u32,
    host_clock: AlsaHostClock,
}

#[derive(Clone, Copy)]
enum AlsaHostClock {
    Monotonic,
    MonotonicRaw,
}

impl AlsaHostClock {
    #[inline(always)]
    const fn telemetry_clock(self) -> OutputTelemetryClock {
        match self {
            Self::Monotonic => OutputTelemetryClock::Monotonic,
            Self::MonotonicRaw => OutputTelemetryClock::MonotonicRaw,
        }
    }
}

const ALSA_STATUS_FUTURE_TOLERANCE_NS: u64 = 5_000_000;
const ALSA_STATUS_STALE_TOLERANCE_NS: u64 = 50_000_000;
const ALSA_STATUS_REGRESSION_TOLERANCE_NS: u64 = 1_000_000;
const ALSA_DEGRADED_HOLD_SAMPLES: u32 = 48;

struct AlsaClockHealth {
    last_status_host_nanos: u64,
    last_playback_host_nanos: u64,
    degraded_samples_remaining: u32,
}

impl AlsaClockHealth {
    #[inline(always)]
    const fn new() -> Self {
        Self {
            last_status_host_nanos: 0,
            last_playback_host_nanos: 0,
            degraded_samples_remaining: 0,
        }
    }

    #[inline(always)]
    fn note_fallback(&mut self) {
        self.degraded_samples_remaining = ALSA_DEGRADED_HOLD_SAMPLES;
    }

    #[inline(always)]
    const fn should_warn_on_failure(&self) -> bool {
        self.degraded_samples_remaining == 0
    }

    #[inline(always)]
    fn note_success(
        &mut self,
        status_host_nanos: u64,
        playback_host_nanos: u64,
    ) -> OutputTimingQuality {
        self.last_status_host_nanos = status_host_nanos;
        self.last_playback_host_nanos = playback_host_nanos;
        if self.degraded_samples_remaining != 0 {
            self.degraded_samples_remaining -= 1;
            OutputTimingQuality::Degraded
        } else {
            OutputTimingQuality::Trusted
        }
    }
}

fn render_thread(
    prep: AlsaOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
    stop_flag: Arc<AtomicBool>,
    ready_tx: Sender<Result<(), String>>,
) {
    if let Err(err) = render_thread_inner(prep, music_ring, sfx_receiver, &stop_flag, &ready_tx) {
        let _ = ready_tx.send(Err(err));
    }
}

fn render_thread_inner(
    prep: AlsaOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
    stop_flag: &AtomicBool,
    ready_tx: &Sender<Result<(), String>>,
) -> Result<(), String> {
    let pcm = open_pcm(&prep.pcm_id)?;
    let actual = configure_run(
        &pcm,
        prep.sample_rate_hz,
        prep.channels,
        prep.period_frames,
        prep.buffer_frames,
        prep.mode,
    )?;
    let io = pcm.io_i16().map_err(|e| {
        format!(
            "failed to create ALSA i16 IO for '{}': {e}",
            prep.device_name
        )
    })?;
    let period_ns = frames_to_nanos(actual.sample_rate_hz, actual.period_frames);
    let buffer_ns = frames_to_nanos(actual.sample_rate_hz, actual.buffer_frames);
    info!(
        "ALSA '{}' using {} output with {} timing.",
        prep.device_name,
        prep.mode.backend_name(),
        actual.host_clock.telemetry_clock()
    );
    publish_output_timing(
        actual.sample_rate_hz,
        period_ns,
        buffer_ns,
        actual.buffer_frames,
        0,
        0,
        0,
    );
    if ready_tx.send(Ok(())).is_err() {
        return Ok(());
    }

    let mut render = RenderState::new(music_ring, sfx_receiver, actual.channels);
    let mut mix = vec![0i16; actual.period_frames as usize * actual.channels];
    let mut clock_health = AlsaClockHealth::new();
    while !stop_flag.load(Ordering::Relaxed) {
        let timing_before = playback_status_timing(
            &pcm,
            actual.sample_rate_hz,
            actual.host_clock,
            &mut clock_health,
        );
        render.render_i16_host_nanos(&mut mix, timing_before.playback_host_nanos);
        write_period(
            &pcm,
            &io,
            &mix,
            actual.channels,
            stop_flag,
            &prep.device_name,
        )?;
        let timing_after = playback_status_timing(
            &pcm,
            actual.sample_rate_hz,
            actual.host_clock,
            &mut clock_health,
        );
        publish_output_timing_quality(worst_quality(
            timing_before.timing_quality,
            timing_after.timing_quality,
        ));
        publish_output_timing(
            actual.sample_rate_hz,
            period_ns,
            buffer_ns,
            actual.buffer_frames,
            timing_after.delay_frames,
            timing_after.delay_frames,
            timing_after.estimated_output_delay_ns,
        );
    }
    let _ = pcm.drop();
    Ok(())
}

fn selected_pcm_id(
    pcm_id: Option<String>,
    device_name: &str,
    mode: AlsaAccessMode,
) -> Result<String, String> {
    let pcm_id = pcm_id.unwrap_or_else(|| "default".to_string());
    match mode {
        AlsaAccessMode::Shared => Ok(shared_pcm_id(&pcm_id)),
        AlsaAccessMode::Exclusive => exclusive_pcm_id(&pcm_id).ok_or_else(|| {
            format!(
                "ALSA exclusive output for '{}' requires a direct hw/plughw device or a resolvable hardware-backed alias, got '{}'",
                device_name, pcm_id
            )
        }),
    }
}

#[inline(always)]
fn shared_pcm_id(pcm_id: &str) -> String {
    if let Some(rest) = pcm_id.strip_prefix("hw:") {
        return format!("plughw:{rest}");
    }
    pcm_id.to_string()
}

#[inline(always)]
fn exclusive_pcm_id(pcm_id: &str) -> Option<String> {
    if let Some(pcm_id) = direct_pcm_id(pcm_id) {
        return Some(pcm_id);
    }
    resolved_hw_pcm_id(pcm_id)
}

#[inline(always)]
fn direct_pcm_id(pcm_id: &str) -> Option<String> {
    if pcm_id.starts_with("hw:") {
        return Some(pcm_id.to_string());
    }
    pcm_id
        .strip_prefix("plughw:")
        .map(|rest| format!("hw:{rest}"))
}

fn resolved_hw_pcm_id(pcm_id: &str) -> Option<String> {
    let pcm = open_pcm(pcm_id).ok()?;
    let info = pcm.info().ok()?;
    let card = info.get_card();
    (card >= 0).then(|| format!("hw:CARD={card},DEV={}", info.get_device()))
}

#[inline(always)]
fn open_pcm(pcm_id: &str) -> Result<PCM, String> {
    PCM::new(pcm_id, Direction::Playback, false)
        .map_err(|e| format!("failed to open ALSA PCM '{pcm_id}': {e}"))
}

fn configure_probe(
    pcm: &PCM,
    sample_rate_hz: u32,
    channels: usize,
    mode: AlsaAccessMode,
) -> Result<AlsaParams, String> {
    configure_pcm(
        pcm,
        sample_rate_hz,
        channels,
        mode,
        suggested_period_frames(sample_rate_hz),
        None,
    )
}

fn configure_run(
    pcm: &PCM,
    sample_rate_hz: u32,
    channels: usize,
    period_frames: u32,
    buffer_frames: u32,
    mode: AlsaAccessMode,
) -> Result<AlsaParams, String> {
    configure_pcm(
        pcm,
        sample_rate_hz,
        channels,
        mode,
        period_frames,
        Some(buffer_frames),
    )
}

fn configure_pcm(
    pcm: &PCM,
    sample_rate_hz: u32,
    channels: usize,
    mode: AlsaAccessMode,
    period_frames: u32,
    buffer_frames: Option<u32>,
) -> Result<AlsaParams, String> {
    let hw = HwParams::any(pcm).map_err(|e| format!("ALSA hw params query failed: {e}"))?;
    hw.set_access(Access::RWInterleaved)
        .map_err(|e| format!("ALSA interleaved access failed: {e}"))?;
    hw.set_format(Format::s16())
        .map_err(|e| format!("ALSA S16 format setup failed: {e}"))?;
    hw.set_rate_resample(matches!(mode, AlsaAccessMode::Shared))
        .map_err(|e| format!("ALSA rate-resample setup failed: {e}"))?;
    hw.set_channels(channels as u32)
        .map_err(|e| format!("ALSA channel setup failed: {e}"))?;
    let actual_rate_hz = match mode {
        AlsaAccessMode::Shared => hw
            .set_rate_near(sample_rate_hz.max(1), ValueOr::Nearest)
            .map_err(|e| format!("ALSA sample-rate setup failed: {e}"))?,
        AlsaAccessMode::Exclusive => {
            hw.set_rate(sample_rate_hz.max(1), ValueOr::Nearest)
                .map_err(|e| format!("ALSA exclusive sample-rate setup failed: {e}"))?;
            sample_rate_hz.max(1)
        }
    };
    let target_period = period_frames.max(64) as alsa::pcm::Frames;
    let actual_period = hw
        .set_period_size_near(target_period, ValueOr::Nearest)
        .map_err(|e| format!("ALSA period setup failed: {e}"))?;
    let target_buffer = buffer_frames
        .unwrap_or_else(|| (actual_period as u32).saturating_mul(2))
        .max(actual_period as u32) as alsa::pcm::Frames;
    let _actual_buffer = hw
        .set_buffer_size_near(target_buffer)
        .map_err(|e| format!("ALSA buffer setup failed: {e}"))?;
    pcm.hw_params(&hw)
        .map_err(|e| format!("ALSA hw params apply failed: {e}"))?;
    let hw_current = pcm
        .hw_params_current()
        .map_err(|e| format!("ALSA current hw params query failed: {e}"))?;
    let sw = pcm
        .sw_params_current()
        .map_err(|e| format!("ALSA sw params query failed: {e}"))?;
    let buffer_frames = hw_current
        .get_buffer_size()
        .map_err(|e| format!("ALSA buffer size query failed: {e}"))?
        .max(1) as u32;
    let period_frames = hw_current
        .get_period_size()
        .map_err(|e| format!("ALSA period size query failed: {e}"))?
        .max(1) as u32;
    let host_clock = apply_sw_params(&sw, pcm, period_frames, buffer_frames)?;
    Ok(AlsaParams {
        sample_rate_hz: hw_current
            .get_rate()
            .unwrap_or(actual_rate_hz.max(1))
            .max(1),
        channels: hw_current.get_channels().unwrap_or(channels as u32).max(1) as usize,
        period_frames,
        buffer_frames,
        host_clock,
    })
}

fn apply_sw_params(
    sw: &SwParams<'_>,
    pcm: &PCM,
    period_frames: u32,
    buffer_frames: u32,
) -> Result<AlsaHostClock, String> {
    sw.set_start_threshold(period_frames.max(1) as alsa::pcm::Frames)
        .map_err(|e| format!("ALSA start-threshold setup failed: {e}"))?;
    sw.set_avail_min(period_frames.max(1) as alsa::pcm::Frames)
        .map_err(|e| format!("ALSA avail-min setup failed: {e}"))?;
    sw.set_stop_threshold(buffer_frames.max(period_frames) as alsa::pcm::Frames)
        .map_err(|e| format!("ALSA stop-threshold setup failed: {e}"))?;
    sw.set_tstamp_mode(true)
        .map_err(|e| format!("ALSA timestamp-mode setup failed: {e}"))?;
    if sw.set_tstamp_type(TstampType::MonotonicRaw).is_ok() && pcm.sw_params(sw).is_ok() {
        return Ok(AlsaHostClock::MonotonicRaw);
    }
    sw.set_tstamp_type(TstampType::Monotonic)
        .map_err(|e| format!("ALSA monotonic timestamp setup failed: {e}"))?;
    pcm.sw_params(sw)
        .map_err(|e| format!("ALSA sw params apply failed: {e}"))?;
    Ok(AlsaHostClock::Monotonic)
}

fn write_period(
    pcm: &PCM,
    io: &alsa::pcm::IO<'_, i16>,
    mix: &[i16],
    channels: usize,
    stop_flag: &AtomicBool,
    device_name: &str,
) -> Result<(), String> {
    let total_frames = mix.len() / channels.max(1);
    let mut written_frames = 0usize;
    while written_frames < total_frames {
        if stop_flag.load(Ordering::Relaxed) {
            return Ok(());
        }
        let start = written_frames * channels;
        match io.writei(&mix[start..]) {
            Ok(0) => {
                let _ = pcm.wait(Some(100));
            }
            Ok(frames) => {
                written_frames = written_frames.saturating_add(frames);
            }
            Err(err) => {
                note_output_underrun();
                pcm.try_recover(err, true).map_err(|recover_err| {
                    format!(
                        "ALSA write failed for '{device_name}' and could not recover: {recover_err}"
                    )
                })?;
                if pcm.state() == State::Prepared {
                    let _ = pcm.start();
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct PlaybackStatusTiming {
    playback_host_nanos: u64,
    delay_frames: u32,
    estimated_output_delay_ns: u64,
    timing_quality: OutputTimingQuality,
}

#[derive(Clone, Copy)]
struct ClockSample {
    host_nanos: u64,
    clock_nanos: u64,
}

#[inline(always)]
fn playback_status_timing(
    pcm: &PCM,
    sample_rate_hz: u32,
    host_clock: AlsaHostClock,
    clock_health: &mut AlsaClockHealth,
) -> PlaybackStatusTiming {
    let delay_frames_fallback = current_delay_frames_fallback(pcm);
    let delay_ns_fallback = frames_to_nanos(sample_rate_hz, delay_frames_fallback);
    let Some(status) = pcm.status().ok() else {
        return fallback_timing(delay_frames_fallback, delay_ns_fallback, clock_health);
    };
    let delay_frames = status.get_delay().max(0) as u32;
    let delay_ns = frames_to_nanos(sample_rate_hz, delay_frames);
    let Some(sample) = sample_host_clock(host_clock) else {
        return fallback_timing(delay_frames, delay_ns, clock_health);
    };
    let Some(status_clock_nanos) = timespec_nanos(status.get_htstamp()) else {
        return fallback_timing(delay_frames, delay_ns, clock_health);
    };
    let status_host_nanos = host_nanos_from_clock(status_clock_nanos, sample);
    let playback_host_nanos = status_host_nanos.saturating_add(delay_ns);
    if status_host_nanos
        > sample
            .host_nanos
            .saturating_add(ALSA_STATUS_FUTURE_TOLERANCE_NS)
        || sample.host_nanos.saturating_sub(status_host_nanos) > ALSA_STATUS_STALE_TOLERANCE_NS
        || (clock_health.last_status_host_nanos != 0
            && status_host_nanos.saturating_add(ALSA_STATUS_REGRESSION_TOLERANCE_NS)
                < clock_health.last_status_host_nanos)
        || (clock_health.last_playback_host_nanos != 0
            && playback_host_nanos.saturating_add(ALSA_STATUS_REGRESSION_TOLERANCE_NS)
                < clock_health.last_playback_host_nanos)
    {
        if clock_health.should_warn_on_failure() {
            warn!(
                "ALSA timestamp sanity check failed: status={} sample={} delay={}ns last_status={} last_playback={}; falling back to host-time anchor.",
                status_host_nanos,
                sample.host_nanos,
                delay_ns,
                clock_health.last_status_host_nanos,
                clock_health.last_playback_host_nanos
            );
        }
        return fallback_timing(delay_frames, delay_ns, clock_health);
    }
    PlaybackStatusTiming {
        playback_host_nanos,
        delay_frames,
        estimated_output_delay_ns: delay_ns,
        timing_quality: clock_health.note_success(status_host_nanos, playback_host_nanos),
    }
}

#[inline(always)]
fn fallback_timing(
    delay_frames: u32,
    delay_ns: u64,
    clock_health: &mut AlsaClockHealth,
) -> PlaybackStatusTiming {
    note_output_clock_fallback();
    clock_health.note_fallback();
    PlaybackStatusTiming {
        playback_host_nanos: now_nanos().saturating_add(delay_ns),
        delay_frames,
        estimated_output_delay_ns: delay_ns,
        timing_quality: OutputTimingQuality::Fallback,
    }
}

#[inline(always)]
const fn worst_quality(a: OutputTimingQuality, b: OutputTimingQuality) -> OutputTimingQuality {
    if (a as u8) >= (b as u8) { a } else { b }
}

#[inline(always)]
fn current_delay_frames_fallback(pcm: &PCM) -> u32 {
    pcm.delay().map(|delay| delay.max(0) as u32).unwrap_or(0)
}

#[inline(always)]
fn sample_host_clock(host_clock: AlsaHostClock) -> Option<ClockSample> {
    Some(ClockSample {
        host_nanos: now_nanos(),
        clock_nanos: current_clock_nanos(host_clock)?,
    })
}

#[inline(always)]
fn current_clock_nanos(host_clock: AlsaHostClock) -> Option<u64> {
    let clock_id = match host_clock {
        AlsaHostClock::Monotonic => libc::CLOCK_MONOTONIC,
        AlsaHostClock::MonotonicRaw => libc::CLOCK_MONOTONIC_RAW,
    };
    let mut ts = timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let rc = unsafe { libc::clock_gettime(clock_id, &mut ts) };
    if rc != 0 {
        return None;
    }
    timespec_nanos(ts)
}

#[inline(always)]
fn timespec_nanos(ts: timespec) -> Option<u64> {
    if ts.tv_sec < 0 || ts.tv_nsec < 0 {
        return None;
    }
    Some((ts.tv_sec as u64).saturating_mul(1_000_000_000) + ts.tv_nsec as u64)
}

#[inline(always)]
fn host_nanos_from_clock(target_nanos: u64, sample: ClockSample) -> u64 {
    if target_nanos >= sample.clock_nanos {
        sample
            .host_nanos
            .saturating_add(target_nanos.saturating_sub(sample.clock_nanos))
    } else {
        sample
            .host_nanos
            .saturating_sub(sample.clock_nanos.saturating_sub(target_nanos))
    }
}

#[inline(always)]
fn frames_to_nanos(sample_rate_hz: u32, frames: u32) -> u64 {
    if sample_rate_hz == 0 || frames == 0 {
        return 0;
    }
    ((u128::from(frames) * 1_000_000_000u128) / u128::from(sample_rate_hz))
        .min((u64::MAX - 1) as u128) as u64
}

#[inline(always)]
fn suggested_period_frames(sample_rate_hz: u32) -> u32 {
    let frames = sample_rate_hz.max(1) / 200;
    frames.clamp(128, 1024)
}
