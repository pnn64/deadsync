pub use deadsync_audio::{AudioOutputMode, LinuxAudioBackend};

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
