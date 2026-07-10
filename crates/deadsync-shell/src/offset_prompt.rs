use deadsync_gameplay::gameplay_offset_prompt_choice_delta;
use deadsync_input::VirtualAction;
use deadsync_screens::Screen;
use deadsync_simfile::sync_offset;

#[derive(Clone, Copy, Debug)]
pub struct GameplayOffsetSnapshot {
    pub initial_global_seconds: f32,
    pub global_seconds: f32,
    pub initial_song_seconds: f32,
    pub song_seconds: f32,
    pub song_writable: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameplayOffsetSaveTargets {
    pub global_seconds: Option<f32>,
    pub song_delta_seconds: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayOffsetSavePrompt {
    pub target: Screen,
    pub navigate_no_fade: bool,
    pub active_choice: u8,
}

impl GameplayOffsetSavePrompt {
    pub const fn new(target: Screen, navigate_no_fade: bool) -> Self {
        Self {
            target,
            navigate_no_fade,
            active_choice: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffsetPromptInput {
    Consumed,
    ChoiceChanged,
    Decide(bool),
}

#[inline(always)]
pub fn gameplay_offset_changed(snapshot: GameplayOffsetSnapshot) -> bool {
    sync_offset::sync_offset_changed(snapshot.initial_global_seconds, snapshot.global_seconds)
        || sync_offset::sync_offset_changed(snapshot.initial_song_seconds, snapshot.song_seconds)
}

#[inline(always)]
pub fn gameplay_offset_saveable_changed(snapshot: GameplayOffsetSnapshot) -> bool {
    sync_offset::gameplay_sync_offset_saveable_changed(
        snapshot.initial_global_seconds,
        snapshot.global_seconds,
        snapshot.initial_song_seconds,
        snapshot.song_seconds,
        snapshot.song_writable,
    )
}

pub fn gameplay_offset_save_targets(snapshot: GameplayOffsetSnapshot) -> GameplayOffsetSaveTargets {
    GameplayOffsetSaveTargets {
        global_seconds: sync_offset::sync_offset_target_seconds(
            snapshot.initial_global_seconds,
            snapshot.global_seconds,
        ),
        song_delta_seconds: snapshot
            .song_writable
            .then(|| {
                sync_offset::sync_offset_delta_seconds(
                    snapshot.initial_song_seconds,
                    snapshot.song_seconds,
                )
            })
            .flatten(),
    }
}

pub fn gameplay_offset_prompt_text(song_title: &str, snapshot: GameplayOffsetSnapshot) -> String {
    sync_offset::gameplay_sync_prompt_text(sync_offset::GameplaySyncPromptText {
        song_title,
        song_writable: snapshot.song_writable,
        initial_global_offset_seconds: snapshot.initial_global_seconds,
        global_offset_seconds: snapshot.global_seconds,
        initial_song_offset_seconds: snapshot.initial_song_seconds,
        song_offset_seconds: snapshot.song_seconds,
    })
}

#[inline(always)]
pub fn gameplay_offset_prompt_needed(
    from: Screen,
    course_active: bool,
    snapshot: GameplayOffsetSnapshot,
) -> bool {
    from == Screen::Gameplay
        && !course_active
        && gameplay_offset_changed(snapshot)
        && gameplay_offset_saveable_changed(snapshot)
}

pub fn route_offset_prompt_input(
    prompt: &mut GameplayOffsetSavePrompt,
    pressed: bool,
    action: VirtualAction,
    only_dedicated_menu_buttons: bool,
) -> OffsetPromptInput {
    if !pressed {
        return OffsetPromptInput::Consumed;
    }
    match gameplay_offset_prompt_choice_delta(action, only_dedicated_menu_buttons) {
        Some(-1) if prompt.active_choice > 0 => {
            prompt.active_choice -= 1;
            OffsetPromptInput::ChoiceChanged
        }
        Some(1) if prompt.active_choice < 1 => {
            prompt.active_choice += 1;
            OffsetPromptInput::ChoiceChanged
        }
        Some(_) => OffsetPromptInput::Consumed,
        None => match action {
            VirtualAction::p1_start
            | VirtualAction::p2_start
            | VirtualAction::p1_select
            | VirtualAction::p2_select => OffsetPromptInput::Decide(prompt.active_choice == 0),
            _ => OffsetPromptInput::Consumed,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(song_writable: bool) -> GameplayOffsetSnapshot {
        GameplayOffsetSnapshot {
            initial_global_seconds: 0.0,
            global_seconds: 0.01,
            initial_song_seconds: 0.0,
            song_seconds: 0.02,
            song_writable,
        }
    }

    #[test]
    fn save_targets_respect_read_only_song_paths() {
        let writable = gameplay_offset_save_targets(snapshot(true));
        assert!((writable.global_seconds.unwrap() - 0.01).abs() < 1e-6);
        assert!((writable.song_delta_seconds.unwrap() - 0.02).abs() < 1e-6);

        let read_only = gameplay_offset_save_targets(snapshot(false));
        assert!((read_only.global_seconds.unwrap() - 0.01).abs() < 1e-6);
        assert_eq!(read_only.song_delta_seconds, None);
    }

    #[test]
    fn prompt_requires_gameplay_non_course_saveable_changes() {
        assert!(gameplay_offset_prompt_needed(
            Screen::Gameplay,
            false,
            snapshot(true),
        ));
        assert!(!gameplay_offset_prompt_needed(
            Screen::Gameplay,
            true,
            snapshot(true),
        ));
        assert!(!gameplay_offset_prompt_needed(
            Screen::Evaluation,
            false,
            snapshot(true),
        ));
    }

    #[test]
    fn input_moves_choice_and_returns_confirmation() {
        let mut prompt = GameplayOffsetSavePrompt::new(Screen::SelectMusic, false);
        assert_eq!(
            route_offset_prompt_input(&mut prompt, true, VirtualAction::p1_right, false),
            OffsetPromptInput::ChoiceChanged,
        );
        assert_eq!(prompt.active_choice, 1);
        assert_eq!(
            route_offset_prompt_input(&mut prompt, true, VirtualAction::p1_start, false),
            OffsetPromptInput::Decide(false),
        );
    }
}
