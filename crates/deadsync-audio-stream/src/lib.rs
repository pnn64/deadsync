//! Game playback decisions, assist timing, bus levels, and ReplayGain policy.
#![forbid(unsafe_code)]

mod clock;
mod mix;
mod music_map;
mod runtime;
mod sfx_cache;

pub use clock::timing_diag_enabled;
pub use mix::{AudioMixLevels, audio_mix_levels, set_audio_mix_levels};
pub use music_map::MusicClock;
#[cfg(target_os = "linux")]
pub use runtime::available_linux_backends;
pub use runtime::{
    AudioControl, assist_sfx_generation, collect_stutter_diag_events, get_output_timing_snapshot,
    init, preserve_pitch_enabled, replaygain_enabled, set_preserve_pitch_enabled,
    set_replaygain_enabled, stutter_diag_trigger_seq, timing_diag_last_callback_gap_ns,
};
pub use sfx_cache::SfxId;
