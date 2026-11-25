/// Game mixer volume levels, expressed as percentages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioMixLevels {
    pub master_volume: u8,
    pub music_volume: u8,
    pub sfx_volume: u8,
    pub assist_tick_volume: u8,
}
