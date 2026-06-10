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
