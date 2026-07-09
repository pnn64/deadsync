use crate::cabinet_chart::{CabinetLightEvent, GameplayLightChartKey, cabinet_light_event_enabled};
use crate::{ButtonLight, Manager, Player};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::SongTimeNs;
use deadsync_gameplay::{GameplayProfileData, GameplayRuntimeState};
use deadsync_rules::note::Note;

const LIGHTS_AHEAD_NS: SongTimeNs = 50_000_000;
const LIGHTS_MAX_CATCHUP_NS: SongTimeNs = 500_000_000;

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

    pub fn cabinet_key_matches(&self, key: &GameplayLightChartKey) -> bool {
        self.cabinet_key.as_ref() == Some(key)
    }

    pub fn restart_cabinet_chart(&mut self) {
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

    pub fn queue_blinks<Profile, OverlayActor, CapturedActor, StateDelta>(
        &mut self,
        lights: &mut Manager,
        state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
        simplify_bass: bool,
    ) where
        Profile: GameplayProfileData,
    {
        let now_ns = state
            .current_music_time_ns()
            .saturating_add(LIGHTS_AHEAD_NS);
        self.queue_cabinet_blinks(lights, now_ns, simplify_bass);
        self.queue_pad_blinks(lights, state, now_ns);
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

    fn queue_pad_blinks<Profile, OverlayActor, CapturedActor, StateDelta>(
        &mut self,
        lights: &mut Manager,
        state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
        now_ns: SongTimeNs,
    ) where
        Profile: GameplayProfileData,
    {
        let notes = state.notes();
        let notes_ptr = notes.as_ptr() as usize;
        let reset = self.pad_notes_ptr != notes_ptr
            || self.pad_notes_len != notes.len()
            || now_ns < self.pad_last_time_ns
            || now_ns.saturating_sub(self.pad_last_time_ns) > LIGHTS_MAX_CATCHUP_NS;
        if reset {
            self.pad_notes_ptr = notes_ptr;
            self.pad_notes_len = notes.len();
            self.pad_cursor = state.note_time_cache_ns().partition_point(|&t| t <= now_ns);
            self.pad_last_time_ns = now_ns;
            return;
        }

        let note_time_cache_ns = state.note_time_cache_ns();
        while self.pad_cursor < notes.len() && note_time_cache_ns[self.pad_cursor] <= now_ns {
            let note = &notes[self.pad_cursor];
            if gameplay_note_lights(note) {
                blink_pad_lights(lights, state, note.column);
            }
            self.pad_cursor += 1;
        }
        self.pad_last_time_ns = now_ns;
    }
}

fn gameplay_note_lights(note: &Note) -> bool {
    note.can_be_judged
        && !note.is_fake
        && matches!(
            note.note_type,
            NoteType::Tap | NoteType::Hold | NoteType::Roll
        )
}

fn blink_pad_lights<Profile, OverlayActor, CapturedActor, StateDelta>(
    lights: &mut Manager,
    state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    column: usize,
) where
    Profile: GameplayProfileData,
{
    if let Some((player, button)) = pad_light_for_col(state, column) {
        lights.blink_button(player, button);
    }
}

fn pad_light_for_col<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    column: usize,
) -> Option<(Player, ButtonLight)>
where
    Profile: GameplayProfileData,
{
    if state.cols_per_player() == 0 {
        return None;
    }
    let local = column % state.cols_per_player();
    let (player, local_col) = if state.cols_per_player() >= 8 && state.num_players() == 1 {
        let player = if local < 4 { Player::P1 } else { Player::P2 };
        (player, local % 4)
    } else {
        let player_ix = column / state.cols_per_player();
        let player = match player_ix {
            0 => Player::P1,
            1 => Player::P2,
            _ => return None,
        };
        (player, local)
    };
    button_light_for_col(local_col).map(|button| (player, button))
}

const fn button_light_for_col(local_col: usize) -> Option<ButtonLight> {
    match local_col {
        0 => Some(ButtonLight::Left),
        1 => Some(ButtonLight::Down),
        2 => Some(ButtonLight::Up),
        3 => Some(ButtonLight::Right),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(button_light_for_col(0), Some(ButtonLight::Left));
        assert_eq!(button_light_for_col(1), Some(ButtonLight::Down));
        assert_eq!(button_light_for_col(2), Some(ButtonLight::Up));
        assert_eq!(button_light_for_col(3), Some(ButtonLight::Right));
        assert_eq!(button_light_for_col(4), None);
    }
}
