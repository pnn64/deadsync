use std::path::PathBuf;

use crate::CabinetLight;
use deadsync_core::song_time::SongTimeNs;

const LIGHTS_QUARTER_ROWS: usize = deadsync_core::timing::ROWS_PER_BEAT as usize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameplayLightChartKey {
    pub simfile_path: PathBuf,
    pub source_hashes: Vec<String>,
    pub global_offset_us: i32,
    pub pack_sync_offset_us: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct CabinetLightEvent {
    pub time_ns: SongTimeNs,
    pub row_index: usize,
    pub light: CabinetLight,
    pub simplify_bass_candidate: bool,
}

#[must_use]
pub const fn cabinet_light_event_enabled(event: CabinetLightEvent, simplify_bass: bool) -> bool {
    !simplify_bass
        || !event.simplify_bass_candidate
        || event.row_index.is_multiple_of(LIGHTS_QUARTER_ROWS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplified_bass_light_events_only_use_quarter_rows() {
        let quarter = CabinetLightEvent {
            time_ns: 0,
            row_index: LIGHTS_QUARTER_ROWS * 2,
            light: CabinetLight::BassLeft,
            simplify_bass_candidate: true,
        };
        let eighth = CabinetLightEvent {
            time_ns: 0,
            row_index: LIGHTS_QUARTER_ROWS * 2 + LIGHTS_QUARTER_ROWS / 2,
            light: CabinetLight::BassLeft,
            simplify_bass_candidate: true,
        };
        let explicit = CabinetLightEvent {
            time_ns: 0,
            row_index: LIGHTS_QUARTER_ROWS * 2 + LIGHTS_QUARTER_ROWS / 2,
            light: CabinetLight::BassLeft,
            simplify_bass_candidate: false,
        };

        assert!(cabinet_light_event_enabled(quarter, true));
        assert!(!cabinet_light_event_enabled(eighth, true));
        assert!(cabinet_light_event_enabled(eighth, false));
        assert!(cabinet_light_event_enabled(explicit, true));
    }
}
