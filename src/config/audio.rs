use std::str::FromStr;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioMixLevels {
    pub master_volume: u8,
    pub music_volume: u8,
    pub sfx_volume: u8,
    pub assist_tick_volume: u8,
}

#[inline(always)]
pub(crate) const fn pack_audio_mix_levels(
    master_volume: u8,
    music_volume: u8,
    sfx_volume: u8,
    assist_tick_volume: u8,
) -> u32 {
    u32::from_le_bytes([master_volume, music_volume, sfx_volume, assist_tick_volume])
}

#[inline(always)]
pub(crate) const fn unpack_audio_mix_levels(packed: u32) -> AudioMixLevels {
    let [master_volume, music_volume, sfx_volume, assist_tick_volume] = packed.to_le_bytes();
    AudioMixLevels {
        master_volume,
        music_volume,
        sfx_volume,
        assist_tick_volume,
    }
}
