use deadlib_audio_core::{MixBus, MixControls};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

pub(crate) const EFFECT_BUS: MixBus = MixBus::new(0);
pub(crate) const SCREEN_BUS: MixBus = MixBus::new(1);
pub(crate) const ASSIST_TICK_BUS: MixBus = MixBus::new(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioMixLevels {
    pub master_volume: u8,
    pub music_volume: u8,
    pub sfx_volume: u8,
    pub assist_tick_volume: u8,
}

const DEFAULT_LEVELS: AudioMixLevels = AudioMixLevels {
    master_volume: 90,
    music_volume: 100,
    sfx_volume: 100,
    assist_tick_volume: 100,
};

static LEVELS: AtomicU32 = AtomicU32::new(pack_levels(DEFAULT_LEVELS));
static CONTROLS: OnceLock<Arc<MixControls>> = OnceLock::new();

#[inline(always)]
const fn pack_levels(levels: AudioMixLevels) -> u32 {
    u32::from_le_bytes([
        levels.master_volume,
        levels.music_volume,
        levels.sfx_volume,
        levels.assist_tick_volume,
    ])
}

#[inline(always)]
const fn unpack_levels(packed: u32) -> AudioMixLevels {
    let [master_volume, music_volume, sfx_volume, assist_tick_volume] = packed.to_le_bytes();
    AudioMixLevels {
        master_volume,
        music_volume,
        sfx_volume,
        assist_tick_volume,
    }
}

#[inline(always)]
fn apply_levels(controls: &MixControls, levels: AudioMixLevels) {
    let master = f32::from(levels.master_volume) * 0.01;
    controls.set_stream_gain(master * f32::from(levels.music_volume) * 0.01);
    let sfx_gain = master * f32::from(levels.sfx_volume) * 0.01;
    controls.set_bus_gain(EFFECT_BUS, sfx_gain);
    controls.set_bus_gain(SCREEN_BUS, sfx_gain);
    controls.set_bus_gain(
        ASSIST_TICK_BUS,
        master * f32::from(levels.assist_tick_volume) * 0.01,
    );
}

pub(crate) fn init_controls() -> Arc<MixControls> {
    CONTROLS
        .get_or_init(|| {
            let controls = Arc::new(MixControls::new());
            apply_levels(&controls, audio_mix_levels());
            controls
        })
        .clone()
}

#[inline(always)]
pub fn set_audio_mix_levels(levels: AudioMixLevels) {
    LEVELS.store(pack_levels(levels), Ordering::Release);
    if let Some(controls) = CONTROLS.get() {
        apply_levels(controls, levels);
    }
}

#[inline(always)]
pub fn audio_mix_levels() -> AudioMixLevels {
    unpack_levels(LEVELS.load(Ordering::Acquire))
}

#[inline(always)]
pub(crate) fn stop_screen_bus() {
    if let Some(controls) = CONTROLS.get() {
        controls.stop_bus(SCREEN_BUS);
    }
}

#[inline(always)]
pub(crate) fn stop_assist_tick_bus() {
    if let Some(controls) = CONTROLS.get() {
        controls.stop_bus(ASSIST_TICK_BUS);
    }
}

#[inline(always)]
pub(crate) fn assist_tick_generation() -> u64 {
    CONTROLS
        .get()
        .map_or(0, |controls| controls.bus_generation(ASSIST_TICK_BUS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_sync_levels_map_to_generic_mixer_buses() {
        let controls = MixControls::new();
        apply_levels(
            &controls,
            AudioMixLevels {
                master_volume: 50,
                music_volume: 80,
                sfx_volume: 60,
                assist_tick_volume: 40,
            },
        );
        assert!((controls.stream_gain() - 0.4).abs() <= f32::EPSILON);
        assert!((controls.bus_gain(EFFECT_BUS) - 0.3).abs() <= f32::EPSILON);
        assert!((controls.bus_gain(SCREEN_BUS) - 0.3).abs() <= f32::EPSILON);
        assert!((controls.bus_gain(ASSIST_TICK_BUS) - 0.2).abs() <= f32::EPSILON);
    }
}
