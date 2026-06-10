use std::str::FromStr;

use crate::telemetry::{OutputTelemetryBackend, OutputTelemetryClock, OutputTimingQuality};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioOutputMode {
    Auto,
    Shared,
    Exclusive,
}

impl AudioOutputMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Shared => "Shared",
            Self::Exclusive => "Exclusive",
        }
    }

    #[inline(always)]
    pub const fn from_bits(bits: u8) -> Self {
        match bits {
            2 => Self::Shared,
            3 => Self::Exclusive,
            _ => Self::Auto,
        }
    }

    #[inline(always)]
    pub const fn bits(self) -> u8 {
        match self {
            Self::Auto => 1,
            Self::Shared => 2,
            Self::Exclusive => 3,
        }
    }
}

impl FromStr for AudioOutputMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "shared" => Ok(Self::Shared),
            "exclusive" => Ok(Self::Exclusive),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxAudioBackend {
    Auto,
    PipeWire,
    PulseAudio,
    Jack,
    Alsa,
}

impl LinuxAudioBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::PipeWire => "PipeWire",
            Self::PulseAudio => "PulseAudio",
            Self::Jack => "JACK",
            Self::Alsa => "ALSA",
        }
    }
}

impl FromStr for LinuxAudioBackend {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "pipewire" | "pipe-wire" | "pw" => Ok(Self::PipeWire),
            "pulseaudio" | "pulse" => Ok(Self::PulseAudio),
            "jack" => Ok(Self::Jack),
            "alsa" => Ok(Self::Alsa),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct OutputDeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub sample_rates_hz: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct OutputBackendReady {
    pub device_sample_rate: u32,
    pub device_channels: usize,
    pub device_name: String,
    pub backend_name: &'static str,
    pub requested_output_mode: AudioOutputMode,
    pub fallback_from_native: bool,
    pub timing_clock: OutputTelemetryClock,
    pub timing_quality: OutputTimingQuality,
}

#[derive(Clone, Copy, Debug)]
pub struct OutputTimingSnapshot {
    pub backend: OutputTelemetryBackend,
    pub requested_output_mode: AudioOutputMode,
    pub fallback_from_native: bool,
    pub timing_clock: OutputTelemetryClock,
    pub timing_quality: OutputTimingQuality,
    pub sample_rate_hz: u32,
    pub device_period_ns: u64,
    pub stream_latency_ns: u64,
    pub buffer_frames: u32,
    pub padding_frames: u32,
    pub queued_frames: u32,
    pub estimated_output_delay_ns: u64,
    pub clock_fallback_count: u64,
    pub timing_sanity_failure_count: u64,
    pub underrun_count: u64,
}

impl OutputTimingSnapshot {
    #[inline(always)]
    pub const fn has_measurement(self) -> bool {
        !matches!(self.backend, OutputTelemetryBackend::Unknown)
            || self.device_period_ns != 0
            || self.stream_latency_ns != 0
            || self.buffer_frames != 0
            || self.padding_frames != 0
            || self.queued_frames != 0
            || self.estimated_output_delay_ns != 0
            || self.clock_fallback_count != 0
            || self.timing_sanity_failure_count != 0
            || self.underrun_count != 0
    }
}
