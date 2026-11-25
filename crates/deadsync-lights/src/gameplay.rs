use crate::cabinet_chart::{CabinetLightEvent, GameplayLightChartKey, cabinet_light_event_enabled};
use crate::{ButtonLight, Manager, Player};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::SongTimeNs;
use deadsync_rules::note::Note;

const LIGHTS_AHEAD_NS: SongTimeNs = 50_000_000;
const LIGHTS_MAX_CATCHUP_NS: SongTimeNs = 500_000_000;

/// Borrowed chart data and clock for one lighting update.
#[derive(Clone, Copy)]
pub struct GameplayLightFrame<'a> {
    pub music_time_ns: SongTimeNs,
    pub notes: &'a [Note],
    pub note_times_ns: &'a [SongTimeNs],
    pub cols_per_player: usize,
}

#[derive(Clone, Debug, Default)]
pub struct GameplayLightTracker {
    pad_notes_ptr: usize,
    pad_notes_len: usize,
    pad_cursor: usize,
    pad_last_time_ns: SongTimeNs,
    cabinet_key: Option<GameplayLightChartKey>,
    cabinet_events: Vec<CabinetLightEvent>,
    cabinet_cursor: usize,
    cabinet_last_time_ns: SongTimeNs,
}

impl GameplayLightTracker {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    #[must_use]
    pub fn cabinet_key_matches(&self, key: &GameplayLightChartKey) -> bool {
        self.cabinet_key.as_ref() == Some(key)
    }

    pub const fn restart_cabinet_chart(&mut self) {
        self.cabinet_cursor = 0;
        self.cabinet_last_time_ns = i64::MAX;
    }

    pub fn set_cabinet_chart(
        &mut self,
        key: GameplayLightChartKey,
        events: Vec<CabinetLightEvent>,
    ) {
        self.cabinet_key = Some(key);
        self.cabinet_events = events;
        self.restart_cabinet_chart();
    }

    pub fn queue_blinks(
        &mut self,
        lights: &mut Manager,
        frame: GameplayLightFrame<'_>,
        simplify_bass: bool,
    ) {
        let now_ns = frame.music_time_ns.saturating_add(LIGHTS_AHEAD_NS);
        self.queue_cabinet_blinks(lights, now_ns, simplify_bass);
        self.queue_pad_blinks(lights, frame, now_ns);
    }

    fn queue_cabinet_blinks(
        &mut self,
        lights: &mut Manager,
        now_ns: SongTimeNs,
        simplify_bass: bool,
    ) {
        let reset = now_ns < self.cabinet_last_time_ns
            || now_ns.saturating_sub(self.cabinet_last_time_ns) > LIGHTS_MAX_CATCHUP_NS;
        if reset {
            self.cabinet_cursor = self
                .cabinet_events
                .partition_point(|event| event.time_ns <= now_ns);
            self.cabinet_last_time_ns = now_ns;
            return;
        }

        while self.cabinet_cursor < self.cabinet_events.len()
            && self.cabinet_events[self.cabinet_cursor].time_ns <= now_ns
        {
            let event = self.cabinet_events[self.cabinet_cursor];
            if cabinet_light_event_enabled(event, simplify_bass) {
                lights.blink_cabinet(event.light);
            }
            self.cabinet_cursor += 1;
        }
        self.cabinet_last_time_ns = now_ns;
    }

    fn queue_pad_blinks(
        &mut self,
        lights: &mut Manager,
        frame: GameplayLightFrame<'_>,
        now_ns: SongTimeNs,
    ) {
        let note_count = frame.notes.len().min(frame.note_times_ns.len());
        let notes_ptr = frame.notes.as_ptr() as usize;
        let reset = self.pad_notes_ptr != notes_ptr
            || self.pad_notes_len != note_count
            || now_ns < self.pad_last_time_ns
            || now_ns.saturating_sub(self.pad_last_time_ns) > LIGHTS_MAX_CATCHUP_NS;
        if reset {
            self.pad_notes_ptr = notes_ptr;
            self.pad_notes_len = note_count;
            self.pad_cursor = frame.note_times_ns[..note_count].partition_point(|&t| t <= now_ns);
            self.pad_last_time_ns = now_ns;
            return;
        }

        while self.pad_cursor < note_count && frame.note_times_ns[self.pad_cursor] <= now_ns {
            let note = &frame.notes[self.pad_cursor];
            if gameplay_note_lights(note)
                && let Some((player, button)) =
                    pad_light_for_col(frame.cols_per_player, note.column)
            {
                lights.blink_button(player, button);
            }
            self.pad_cursor += 1;
        }
        self.pad_last_time_ns = now_ns;
    }
}

const fn gameplay_note_lights(note: &Note) -> bool {
    note.can_be_judged
        && !note.is_fake
        && matches!(
            note.note_type,
            NoteType::Tap | NoteType::Hold | NoteType::Roll
        )
}

fn pad_light_for_col(cols_per_player: usize, column: usize) -> Option<(Player, ButtonLight)> {
    if cols_per_player == 0 {
        return None;
    }
    let local = column % cols_per_player;
    let pump = matches!(cols_per_player, 5 | 10);
    let lanes_per_pad = if pump { 5 } else { 4 };
    // A field spanning both pads (doubles, or each Routine participant).
    let (player, local_col) = if cols_per_player > lanes_per_pad {
        let player = if local < lanes_per_pad {
            Player::P1
        } else {
            Player::P2
        };
        (player, local % lanes_per_pad)
    } else {
        let player_ix = column / cols_per_player;
        let player = match player_ix {
            0 => Player::P1,
            1 => Player::P2,
            _ => return None,
        };
        (player, local)
    };
    button_light_for_col(pump, local_col).map(|button| (player, button))
}

const fn button_light_for_col(pump: bool, local_col: usize) -> Option<ButtonLight> {
    if pump {
        match local_col {
            0 => Some(ButtonLight::Right),
            1 => Some(ButtonLight::Up),
            2 => Some(ButtonLight::Left),
            3 => Some(ButtonLight::Down),
            4 => Some(ButtonLight::Select),
            _ => None,
        }
    } else {
        match local_col {
            0 => Some(ButtonLight::Left),
            1 => Some(ButtonLight::Down),
            2 => Some(ButtonLight::Up),
            3 => Some(ButtonLight::Right),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::lights::{DEFAULT_LITBOARD_PORT, DriverKind, PacDriveLightOrdering};

    #[test]
    fn borrowed_chart_frame_preserves_blink_timing_and_pad_mapping() {
        let mut lights = Manager::new(
            DriverKind::Off,
            DEFAULT_LITBOARD_PORT,
            PacDriveLightOrdering::default(),
        );
        let mut tracker = GameplayLightTracker::default();
        let mut notes = [
            test_note(NoteType::Tap, true, false),
            test_note(NoteType::Tap, true, false),
        ];
        notes[0].column = 5;
        let note_times_ns = [100_000_000];
        let mut frame = GameplayLightFrame {
            music_time_ns: 0,
            notes: &notes,
            note_times_ns: &note_times_ns,
            cols_per_player: 8,
        };
        tracker.queue_blinks(&mut lights, frame, false);
        frame.music_time_ns = 49_999_999;
        tracker.queue_blinks(&mut lights, frame, false);
        assert_eq!(lights.chart_directions(), [[false; 4]; 2]);

        frame.music_time_ns = 50_000_000;
        tracker.queue_blinks(&mut lights, frame, false);
        assert_eq!(
            lights.chart_directions(),
            [[false; 4], [false, true, false, false]]
        );

        lights.clear_blinks();
        frame.music_time_ns = 0;
        tracker.queue_blinks(&mut lights, frame, false);
        assert_eq!(lights.chart_directions(), [[false; 4]; 2]);
        frame.music_time_ns = 50_000_000;
        tracker.queue_blinks(&mut lights, frame, false);
        assert_eq!(
            lights.chart_directions(),
            [[false; 4], [false, true, false, false]]
        );
    }

    #[test]
    fn gameplay_note_lights_only_judged_visible_step_notes() {
        assert!(gameplay_note_lights(&test_note(NoteType::Tap, true, false)));
        assert!(gameplay_note_lights(&test_note(
            NoteType::Hold,
            true,
            false
        )));
        assert!(gameplay_note_lights(&test_note(
            NoteType::Roll,
            true,
            false
        )));
        assert!(!gameplay_note_lights(&test_note(
            NoteType::Mine,
            true,
            false
        )));
        assert!(!gameplay_note_lights(&test_note(
            NoteType::Tap,
            false,
            false
        )));
        assert!(!gameplay_note_lights(&test_note(NoteType::Tap, true, true)));
    }

    fn test_note(note_type: NoteType, can_be_judged: bool, is_fake: bool) -> Note {
        Note {
            participant: None,
            beat: 0.0,
            quantization_idx: 0,
            column: 0,
            note_type,
            row_index: 0,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake,
            can_be_judged,
        }
    }

    #[test]
    fn button_light_mapping_uses_four_panel_order() {
        assert_eq!(button_light_for_col(false, 0), Some(ButtonLight::Left));
        assert_eq!(button_light_for_col(false, 1), Some(ButtonLight::Down));
        assert_eq!(button_light_for_col(false, 2), Some(ButtonLight::Up));
        assert_eq!(button_light_for_col(false, 3), Some(ButtonLight::Right));
        assert_eq!(button_light_for_col(false, 4), None);
    }

    #[test]
    fn pump_columns_cover_all_fusion_panels() {
        assert_eq!(button_light_for_col(true, 0), Some(ButtonLight::Right));
        assert_eq!(button_light_for_col(true, 1), Some(ButtonLight::Up));
        assert_eq!(button_light_for_col(true, 2), Some(ButtonLight::Left));
        assert_eq!(button_light_for_col(true, 3), Some(ButtonLight::Down));
        assert_eq!(button_light_for_col(true, 4), Some(ButtonLight::Select));
    }
}
