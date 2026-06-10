use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitConfig {
    pub output_device_index: Option<u16>,
    pub output_mode: AudioOutputMode,
    #[cfg(target_os = "linux")]
    pub linux_backend: LinuxAudioBackend,
    pub sample_rate_hz: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioMixLevels {
    pub master_volume: u8,
    pub music_volume: u8,
    pub sfx_volume: u8,
    pub assist_tick_volume: u8,
}

const DEFAULT_AUDIO_MIX_LEVELS: AudioMixLevels = AudioMixLevels {
    master_volume: 90,
    music_volume: 100,
    sfx_volume: 100,
    assist_tick_volume: 100,
};

static AUDIO_MIX_LEVELS_PACKED: AtomicU32 = AtomicU32::new(pack_audio_mix_levels(
    DEFAULT_AUDIO_MIX_LEVELS.master_volume,
    DEFAULT_AUDIO_MIX_LEVELS.music_volume,
    DEFAULT_AUDIO_MIX_LEVELS.sfx_volume,
    DEFAULT_AUDIO_MIX_LEVELS.assist_tick_volume,
));

#[inline(always)]
pub const fn pack_audio_mix_levels(
    master_volume: u8,
    music_volume: u8,
    sfx_volume: u8,
    assist_tick_volume: u8,
) -> u32 {
    u32::from_le_bytes([master_volume, music_volume, sfx_volume, assist_tick_volume])
}

#[inline(always)]
pub const fn unpack_audio_mix_levels(packed: u32) -> AudioMixLevels {
    let [master_volume, music_volume, sfx_volume, assist_tick_volume] = packed.to_le_bytes();
    AudioMixLevels {
        master_volume,
        music_volume,
        sfx_volume,
        assist_tick_volume,
    }
}

#[inline(always)]
pub fn set_audio_mix_levels(levels: AudioMixLevels) {
    AUDIO_MIX_LEVELS_PACKED.store(
        pack_audio_mix_levels(
            levels.master_volume,
            levels.music_volume,
            levels.sfx_volume,
            levels.assist_tick_volume,
        ),
        Ordering::Release,
    );
}

#[inline(always)]
pub fn audio_mix_levels() -> AudioMixLevels {
    unpack_audio_mix_levels(AUDIO_MIX_LEVELS_PACKED.load(Ordering::Acquire))
}

#[inline(always)]
pub fn mix_level_gains(levels: AudioMixLevels) -> (f32, f32, f32) {
    let master_vol = f32::from(levels.master_volume) * 0.01;
    let music_vol = f32::from(levels.music_volume) * 0.01;
    let sfx_vol = f32::from(levels.sfx_volume) * 0.01;
    let assist_tick_vol = f32::from(levels.assist_tick_volume) * 0.01;
    (
        master_vol * music_vol,
        master_vol * sfx_vol,
        master_vol * assist_tick_vol,
    )
}

#[inline(always)]
pub fn audio_mix_level_gains() -> (f32, f32, f32) {
    mix_level_gains(audio_mix_levels())
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
