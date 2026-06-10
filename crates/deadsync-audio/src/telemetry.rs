use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

use crate::output::{AudioOutputMode, OutputBackendReady, OutputTimingSnapshot};

pub const AUDIO_STUTTER_DIAG_EVENT_COUNT: usize = 64;

struct AudioDiagEventSlot {
    version: AtomicU64,
    at_host_nanos: AtomicU64,
    kind: AtomicU8,
    value_ns: AtomicU64,
    sample_rate_hz: AtomicU32,
    buffer_frames: AtomicU32,
    padding_frames: AtomicU32,
    queued_frames: AtomicU32,
    device_period_ns: AtomicU64,
    estimated_output_delay_ns: AtomicU64,
    timing_quality: AtomicU8,
}

impl AudioDiagEventSlot {
    #[inline(always)]
    const fn new() -> Self {
        Self {
            version: AtomicU64::new(0),
            at_host_nanos: AtomicU64::new(0),
            kind: AtomicU8::new(0),
            value_ns: AtomicU64::new(0),
            sample_rate_hz: AtomicU32::new(0),
            buffer_frames: AtomicU32::new(0),
            padding_frames: AtomicU32::new(0),
            queued_frames: AtomicU32::new(0),
            device_period_ns: AtomicU64::new(0),
            estimated_output_delay_ns: AtomicU64::new(0),
            timing_quality: AtomicU8::new(OutputTimingQuality::Unknown as u8),
        }
    }

    fn load(&self) -> Option<(u64, StutterDiagAudioEvent)> {
        let version_start = self.version.load(Ordering::Acquire);
        if version_start == 0 || version_start & 1 != 0 {
            return None;
        }
        let event = StutterDiagAudioEvent {
            at_host_nanos: self.at_host_nanos.load(Ordering::Relaxed),
            kind: StutterDiagAudioEventKind::from_bits(self.kind.load(Ordering::Relaxed))?,
            value_ns: self.value_ns.load(Ordering::Relaxed),
            sample_rate_hz: self.sample_rate_hz.load(Ordering::Relaxed),
            buffer_frames: self.buffer_frames.load(Ordering::Relaxed),
            padding_frames: self.padding_frames.load(Ordering::Relaxed),
            queued_frames: self.queued_frames.load(Ordering::Relaxed),
            device_period_ns: self.device_period_ns.load(Ordering::Relaxed),
            estimated_output_delay_ns: self.estimated_output_delay_ns.load(Ordering::Relaxed),
            timing_quality: OutputTimingQuality::from_bits(
                self.timing_quality.load(Ordering::Relaxed),
            ),
        };
        let version_end = self.version.load(Ordering::Acquire);
        (version_start == version_end).then_some((version_end >> 1, event))
    }
}

struct AudioTelemetryState {
    stutter_diag_event_head: AtomicU64,
    stutter_diag_events: [AudioDiagEventSlot; AUDIO_STUTTER_DIAG_EVENT_COUNT],
    output_timing_backend: AtomicU8,
    output_timing_requested_mode: AtomicU8,
    output_timing_native_fallback: AtomicBool,
    output_timing_clock: AtomicU8,
    output_timing_quality: AtomicU8,
    output_timing_sample_rate_hz: AtomicU32,
    output_timing_device_period_ns: AtomicU64,
    output_timing_stream_latency_ns: AtomicU64,
    output_timing_buffer_frames: AtomicU32,
    output_timing_padding_frames: AtomicU32,
    output_timing_queued_frames: AtomicU32,
    output_timing_est_delay_ns: AtomicU64,
    output_timing_clock_fallbacks: AtomicU64,
    output_timing_sanity_failures: AtomicU64,
    output_timing_underruns: AtomicU64,
}

impl AudioTelemetryState {
    const fn new() -> Self {
        Self {
            stutter_diag_event_head: AtomicU64::new(0),
            stutter_diag_events: [const { AudioDiagEventSlot::new() };
                AUDIO_STUTTER_DIAG_EVENT_COUNT],
            output_timing_backend: AtomicU8::new(OutputTelemetryBackend::Unknown as u8),
            output_timing_requested_mode: AtomicU8::new(AudioOutputMode::Auto.bits()),
            output_timing_native_fallback: AtomicBool::new(false),
            output_timing_clock: AtomicU8::new(OutputTelemetryClock::Unknown as u8),
            output_timing_quality: AtomicU8::new(OutputTimingQuality::Unknown as u8),
            output_timing_sample_rate_hz: AtomicU32::new(0),
            output_timing_device_period_ns: AtomicU64::new(0),
            output_timing_stream_latency_ns: AtomicU64::new(0),
            output_timing_buffer_frames: AtomicU32::new(0),
            output_timing_padding_frames: AtomicU32::new(0),
            output_timing_queued_frames: AtomicU32::new(0),
            output_timing_est_delay_ns: AtomicU64::new(0),
            output_timing_clock_fallbacks: AtomicU64::new(0),
            output_timing_sanity_failures: AtomicU64::new(0),
            output_timing_underruns: AtomicU64::new(0),
        }
    }

    fn output_timing_snapshot(&self) -> OutputTimingSnapshot {
        OutputTimingSnapshot {
            backend: OutputTelemetryBackend::from_bits(
                self.output_timing_backend.load(Ordering::Relaxed),
            ),
            requested_output_mode: AudioOutputMode::from_bits(
                self.output_timing_requested_mode.load(Ordering::Relaxed),
            ),
            fallback_from_native: self.output_timing_native_fallback.load(Ordering::Relaxed),
            timing_clock: OutputTelemetryClock::from_bits(
                self.output_timing_clock.load(Ordering::Relaxed),
            ),
            timing_quality: self.current_output_timing_quality(),
            sample_rate_hz: self.output_timing_sample_rate_hz.load(Ordering::Relaxed),
            device_period_ns: self.output_timing_device_period_ns.load(Ordering::Relaxed),
            stream_latency_ns: self.output_timing_stream_latency_ns.load(Ordering::Relaxed),
            buffer_frames: self.output_timing_buffer_frames.load(Ordering::Relaxed),
            padding_frames: self.output_timing_padding_frames.load(Ordering::Relaxed),
            queued_frames: self.output_timing_queued_frames.load(Ordering::Relaxed),
            estimated_output_delay_ns: self.output_timing_est_delay_ns.load(Ordering::Relaxed),
            clock_fallback_count: self.output_timing_clock_fallbacks.load(Ordering::Relaxed),
            timing_sanity_failure_count: self.output_timing_sanity_failures.load(Ordering::Relaxed),
            underrun_count: self.output_timing_underruns.load(Ordering::Relaxed),
        }
    }

    fn publish_output_backend_ready(&self, ready: OutputBackendReady) {
        self.output_timing_backend.store(
            OutputTelemetryBackend::from_backend_name(ready.backend_name) as u8,
            Ordering::Relaxed,
        );
        self.output_timing_requested_mode
            .store(ready.requested_output_mode.bits(), Ordering::Relaxed);
        self.output_timing_native_fallback
            .store(ready.fallback_from_native, Ordering::Relaxed);
        self.output_timing_clock
            .store(ready.timing_clock as u8, Ordering::Relaxed);
        self.output_timing_quality
            .store(ready.timing_quality as u8, Ordering::Relaxed);
        self.output_timing_sample_rate_hz
            .store(ready.device_sample_rate, Ordering::Relaxed);
        self.output_timing_device_period_ns
            .store(0, Ordering::Relaxed);
        self.output_timing_stream_latency_ns
            .store(0, Ordering::Relaxed);
        self.output_timing_buffer_frames.store(0, Ordering::Relaxed);
        self.output_timing_padding_frames
            .store(0, Ordering::Relaxed);
        self.output_timing_queued_frames.store(0, Ordering::Relaxed);
        self.output_timing_est_delay_ns.store(0, Ordering::Relaxed);
        self.output_timing_clock_fallbacks
            .store(0, Ordering::Relaxed);
        self.output_timing_sanity_failures
            .store(0, Ordering::Relaxed);
        self.output_timing_underruns.store(0, Ordering::Relaxed);
    }

    fn publish_output_timing(
        &self,
        sample_rate_hz: u32,
        device_period_ns: u64,
        stream_latency_ns: u64,
        buffer_frames: u32,
        padding_frames: u32,
        queued_frames: u32,
        estimated_output_delay_ns: u64,
    ) {
        self.output_timing_sample_rate_hz
            .store(sample_rate_hz, Ordering::Relaxed);
        self.output_timing_device_period_ns
            .store(device_period_ns, Ordering::Relaxed);
        self.output_timing_stream_latency_ns
            .store(stream_latency_ns, Ordering::Relaxed);
        self.output_timing_buffer_frames
            .store(buffer_frames, Ordering::Relaxed);
        self.output_timing_padding_frames
            .store(padding_frames, Ordering::Relaxed);
        self.output_timing_queued_frames
            .store(queued_frames, Ordering::Relaxed);
        self.output_timing_est_delay_ns
            .store(estimated_output_delay_ns, Ordering::Relaxed);
    }

    fn publish_output_timing_quality(&self, quality: OutputTimingQuality) {
        self.output_timing_quality
            .store(quality as u8, Ordering::Relaxed);
    }

    fn note_output_underrun(&self, at_host_nanos: u64, stutter_diag_enabled: bool) {
        self.output_timing_underruns.fetch_add(1, Ordering::Relaxed);
        if stutter_diag_enabled {
            self.record_stutter_diag_event(
                StutterDiagAudioEventKind::Underrun,
                at_host_nanos,
                0,
                self.current_output_timing_quality(),
            );
        }
    }

    fn note_output_timing_sanity_failure(
        &self,
        quality: OutputTimingQuality,
        at_host_nanos: u64,
        stutter_diag_enabled: bool,
    ) {
        self.publish_output_timing_quality(quality);
        self.output_timing_sanity_failures
            .fetch_add(1, Ordering::Relaxed);
        if stutter_diag_enabled && !matches!(quality, OutputTimingQuality::Fallback) {
            self.record_stutter_diag_event(
                StutterDiagAudioEventKind::TimingSanity,
                at_host_nanos,
                0,
                quality,
            );
        }
    }

    fn note_output_clock_fallback(&self, at_host_nanos: u64, stutter_diag_enabled: bool) {
        self.note_output_timing_sanity_failure(
            OutputTimingQuality::Fallback,
            at_host_nanos,
            stutter_diag_enabled,
        );
        self.output_timing_clock_fallbacks
            .fetch_add(1, Ordering::Relaxed);
        if stutter_diag_enabled {
            self.record_stutter_diag_event(
                StutterDiagAudioEventKind::ClockFallback,
                at_host_nanos,
                0,
                OutputTimingQuality::Fallback,
            );
        }
    }

    fn record_stutter_diag_event(
        &self,
        kind: StutterDiagAudioEventKind,
        at_host_nanos: u64,
        value_ns: u64,
        timing_quality: OutputTimingQuality,
    ) {
        let seq = self.stutter_diag_event_head.fetch_add(1, Ordering::Relaxed) + 1;
        let slot = &self.stutter_diag_events[(seq as usize - 1) % AUDIO_STUTTER_DIAG_EVENT_COUNT];
        slot.version.store((seq << 1) | 1, Ordering::Relaxed);
        slot.at_host_nanos.store(at_host_nanos, Ordering::Relaxed);
        slot.kind.store(kind as u8, Ordering::Relaxed);
        slot.value_ns.store(value_ns, Ordering::Relaxed);
        slot.sample_rate_hz.store(
            self.output_timing_sample_rate_hz.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        slot.buffer_frames.store(
            self.output_timing_buffer_frames.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        slot.padding_frames.store(
            self.output_timing_padding_frames.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        slot.queued_frames.store(
            self.output_timing_queued_frames.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        slot.device_period_ns.store(
            self.output_timing_device_period_ns.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        slot.estimated_output_delay_ns.store(
            self.output_timing_est_delay_ns.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        slot.timing_quality
            .store(timing_quality as u8, Ordering::Relaxed);
        slot.version.store(seq << 1, Ordering::Release);
    }

    fn stutter_diag_trigger_seq(&self) -> u64 {
        self.stutter_diag_event_head.load(Ordering::Acquire)
    }

    fn collect_stutter_diag_events(
        &self,
        now_host_nanos: u64,
        window_ns: u64,
        out: &mut Vec<StutterDiagAudioEvent>,
    ) {
        let head = self.stutter_diag_event_head.load(Ordering::Acquire);
        let start = head.saturating_sub(AUDIO_STUTTER_DIAG_EVENT_COUNT as u64);
        for seq in (start + 1)..=head {
            let slot =
                &self.stutter_diag_events[(seq as usize - 1) % AUDIO_STUTTER_DIAG_EVENT_COUNT];
            let Some((loaded_seq, event)) = slot.load() else {
                continue;
            };
            if loaded_seq != seq || event.at_host_nanos == 0 {
                continue;
            }
            if now_host_nanos.saturating_sub(event.at_host_nanos) <= window_ns {
                out.push(event);
            }
        }
    }

    fn stutter_diag_callback_gap_threshold_ns(&self) -> u64 {
        let device_period_ns = self.output_timing_device_period_ns.load(Ordering::Relaxed);
        if device_period_ns > 0 {
            return device_period_ns.saturating_mul(2).max(5_000_000);
        }
        let sample_rate_hz = self.output_timing_sample_rate_hz.load(Ordering::Relaxed);
        let buffer_frames = self.output_timing_buffer_frames.load(Ordering::Relaxed);
        if sample_rate_hz > 0 && buffer_frames > 0 {
            let buffer_ns = (u64::from(buffer_frames) * 1_000_000_000)
                .saturating_div(u64::from(sample_rate_hz));
            return buffer_ns.saturating_mul(2).max(5_000_000);
        }
        10_000_000
    }

    fn current_output_timing_quality(&self) -> OutputTimingQuality {
        OutputTimingQuality::from_bits(self.output_timing_quality.load(Ordering::Relaxed))
    }
}

static AUDIO_TELEMETRY: AudioTelemetryState = AudioTelemetryState::new();

pub fn get_output_timing_snapshot() -> OutputTimingSnapshot {
    AUDIO_TELEMETRY.output_timing_snapshot()
}

#[inline(always)]
pub fn publish_output_backend_ready(ready: OutputBackendReady) {
    AUDIO_TELEMETRY.publish_output_backend_ready(ready);
}

#[inline(always)]
pub fn publish_output_timing(
    sample_rate_hz: u32,
    device_period_ns: u64,
    stream_latency_ns: u64,
    buffer_frames: u32,
    padding_frames: u32,
    queued_frames: u32,
    estimated_output_delay_ns: u64,
) {
    AUDIO_TELEMETRY.publish_output_timing(
        sample_rate_hz,
        device_period_ns,
        stream_latency_ns,
        buffer_frames,
        padding_frames,
        queued_frames,
        estimated_output_delay_ns,
    );
}

#[inline(always)]
pub fn publish_output_timing_quality(quality: OutputTimingQuality) {
    AUDIO_TELEMETRY.publish_output_timing_quality(quality);
}

#[inline(always)]
pub fn note_output_underrun(at_host_nanos: u64, stutter_diag_enabled: bool) {
    AUDIO_TELEMETRY.note_output_underrun(at_host_nanos, stutter_diag_enabled);
}

#[inline(always)]
pub fn note_output_timing_sanity_failure(
    quality: OutputTimingQuality,
    at_host_nanos: u64,
    stutter_diag_enabled: bool,
) {
    AUDIO_TELEMETRY.note_output_timing_sanity_failure(quality, at_host_nanos, stutter_diag_enabled);
}

#[inline(always)]
pub fn note_output_clock_fallback(at_host_nanos: u64, stutter_diag_enabled: bool) {
    AUDIO_TELEMETRY.note_output_clock_fallback(at_host_nanos, stutter_diag_enabled);
}

#[inline(always)]
pub fn record_stutter_diag_event(
    kind: StutterDiagAudioEventKind,
    at_host_nanos: u64,
    value_ns: u64,
    timing_quality: OutputTimingQuality,
) {
    AUDIO_TELEMETRY.record_stutter_diag_event(kind, at_host_nanos, value_ns, timing_quality);
}

pub fn stutter_diag_trigger_seq() -> u64 {
    AUDIO_TELEMETRY.stutter_diag_trigger_seq()
}

pub fn collect_stutter_diag_events(
    now_host_nanos: u64,
    window_ns: u64,
    out: &mut Vec<StutterDiagAudioEvent>,
) {
    AUDIO_TELEMETRY.collect_stutter_diag_events(now_host_nanos, window_ns, out);
}

pub fn stutter_diag_callback_gap_threshold_ns() -> u64 {
    AUDIO_TELEMETRY.stutter_diag_callback_gap_threshold_ns()
}

#[inline(always)]
pub fn current_output_timing_quality() -> OutputTimingQuality {
    AUDIO_TELEMETRY.current_output_timing_quality()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OutputTelemetryBackend {
    Unknown = 0,
    #[cfg(target_os = "linux")]
    AlsaShared = 1,
    #[cfg(target_os = "linux")]
    AlsaExclusive = 2,
    #[cfg(windows)]
    WasapiShared = 1,
    #[cfg(windows)]
    WasapiExclusive = 2,
    #[cfg(target_os = "linux")]
    #[cfg(has_pulse_audio)]
    PulseAudioShared = 3,
    #[cfg(target_os = "freebsd")]
    FreeBsdPcm = 1,
    #[cfg(target_os = "linux")]
    #[cfg(has_jack_audio)]
    JackShared = 4,
    #[cfg(target_os = "macos")]
    CoreAudioShared = 1,
    #[cfg(target_os = "linux")]
    #[cfg(has_pipewire_audio)]
    PipeWireShared = 5,
}

impl OutputTelemetryBackend {
    #[inline(always)]
    pub fn from_backend_name(name: &'static str) -> Self {
        match name {
            #[cfg(target_os = "linux")]
            "alsa-shared" => Self::AlsaShared,
            #[cfg(target_os = "linux")]
            "alsa-exclusive" => Self::AlsaExclusive,
            #[cfg(windows)]
            "wasapi-shared" => Self::WasapiShared,
            #[cfg(windows)]
            "wasapi-exclusive" => Self::WasapiExclusive,
            #[cfg(target_os = "linux")]
            #[cfg(has_pulse_audio)]
            "pulse-shared" => Self::PulseAudioShared,
            #[cfg(target_os = "freebsd")]
            "freebsd-pcm" => Self::FreeBsdPcm,
            #[cfg(target_os = "linux")]
            #[cfg(has_jack_audio)]
            "jack-shared" => Self::JackShared,
            #[cfg(target_os = "macos")]
            "coreaudio-shared" => Self::CoreAudioShared,
            #[cfg(target_os = "linux")]
            #[cfg(has_pipewire_audio)]
            "pipewire-shared" => Self::PipeWireShared,
            _ => Self::Unknown,
        }
    }

    #[inline(always)]
    pub const fn from_bits(bits: u8) -> Self {
        match bits {
            #[cfg(target_os = "linux")]
            1 => Self::AlsaShared,
            #[cfg(target_os = "linux")]
            2 => Self::AlsaExclusive,
            #[cfg(windows)]
            1 => Self::WasapiShared,
            #[cfg(windows)]
            2 => Self::WasapiExclusive,
            #[cfg(target_os = "linux")]
            #[cfg(has_pulse_audio)]
            3 => Self::PulseAudioShared,
            #[cfg(target_os = "freebsd")]
            1 => Self::FreeBsdPcm,
            #[cfg(target_os = "linux")]
            #[cfg(has_jack_audio)]
            4 => Self::JackShared,
            #[cfg(target_os = "macos")]
            1 => Self::CoreAudioShared,
            #[cfg(target_os = "linux")]
            #[cfg(has_pipewire_audio)]
            5 => Self::PipeWireShared,
            _ => Self::Unknown,
        }
    }

    #[inline(always)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            #[cfg(target_os = "linux")]
            Self::AlsaShared => "alsa-shared",
            #[cfg(target_os = "linux")]
            Self::AlsaExclusive => "alsa-exclusive",
            #[cfg(windows)]
            Self::WasapiShared => "wasapi-shared",
            #[cfg(windows)]
            Self::WasapiExclusive => "wasapi-exclusive",
            #[cfg(target_os = "linux")]
            #[cfg(has_pulse_audio)]
            Self::PulseAudioShared => "pulse-shared",
            #[cfg(target_os = "freebsd")]
            Self::FreeBsdPcm => "freebsd-pcm",
            #[cfg(target_os = "linux")]
            #[cfg(has_jack_audio)]
            Self::JackShared => "jack-shared",
            #[cfg(target_os = "macos")]
            Self::CoreAudioShared => "coreaudio-shared",
            #[cfg(target_os = "linux")]
            #[cfg(has_pipewire_audio)]
            Self::PipeWireShared => "pipewire-shared",
        }
    }
}

impl std::fmt::Display for OutputTelemetryBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OutputTelemetryClock {
    Unknown = 0,
    Callback = 1,
    #[cfg(target_os = "macos")]
    HostTime = 2,
    #[cfg(all(unix, not(target_os = "macos")))]
    Monotonic = 2,
    #[cfg(all(unix, not(target_os = "macos")))]
    MonotonicRaw = 3,
    #[cfg(windows)]
    DeviceQpc = 4,
}

impl OutputTelemetryClock {
    #[inline(always)]
    pub const fn from_bits(bits: u8) -> Self {
        match bits {
            1 => Self::Callback,
            #[cfg(target_os = "macos")]
            2 => Self::HostTime,
            #[cfg(all(unix, not(target_os = "macos")))]
            2 => Self::Monotonic,
            #[cfg(all(unix, not(target_os = "macos")))]
            3 => Self::MonotonicRaw,
            #[cfg(windows)]
            4 => Self::DeviceQpc,
            _ => Self::Unknown,
        }
    }

    #[inline(always)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Callback => "callback",
            #[cfg(target_os = "macos")]
            Self::HostTime => "host_time",
            #[cfg(all(unix, not(target_os = "macos")))]
            Self::Monotonic => "monotonic",
            #[cfg(all(unix, not(target_os = "macos")))]
            Self::MonotonicRaw => "monotonic_raw",
            #[cfg(windows)]
            Self::DeviceQpc => "device+qpc",
        }
    }
}

impl std::fmt::Display for OutputTelemetryClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OutputTimingQuality {
    Unknown = 0,
    Trusted = 1,
    Degraded = 2,
    Fallback = 3,
}

impl OutputTimingQuality {
    #[inline(always)]
    pub const fn from_bits(bits: u8) -> Self {
        match bits {
            1 => Self::Trusted,
            2 => Self::Degraded,
            3 => Self::Fallback,
            _ => Self::Unknown,
        }
    }

    #[inline(always)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Trusted => "trusted",
            Self::Degraded => "degraded",
            Self::Fallback => "fallback",
        }
    }
}

impl std::fmt::Display for OutputTimingQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum StutterDiagAudioEventKind {
    Underrun = 1,
    CallbackGap = 2,
    TimingSanity = 3,
    ClockFallback = 4,
}

impl StutterDiagAudioEventKind {
    #[inline(always)]
    pub const fn from_bits(bits: u8) -> Option<Self> {
        match bits {
            1 => Some(Self::Underrun),
            2 => Some(Self::CallbackGap),
            3 => Some(Self::TimingSanity),
            4 => Some(Self::ClockFallback),
            _ => None,
        }
    }

    #[inline(always)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Underrun => "underrun",
            Self::CallbackGap => "callback_gap",
            Self::TimingSanity => "timing_sanity",
            Self::ClockFallback => "clock_fallback",
        }
    }
}

impl std::fmt::Display for StutterDiagAudioEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StutterDiagAudioEvent {
    pub at_host_nanos: u64,
    pub kind: StutterDiagAudioEventKind,
    pub value_ns: u64,
    pub sample_rate_hz: u32,
    pub buffer_frames: u32,
    pub padding_frames: u32,
    pub queued_frames: u32,
    pub device_period_ns: u64,
    pub estimated_output_delay_ns: u64,
    pub timing_quality: OutputTimingQuality,
}
