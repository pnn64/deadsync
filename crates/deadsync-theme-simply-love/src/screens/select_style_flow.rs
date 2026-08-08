use crate::screens::Screen;
use deadsync_config::prelude::GameFlag;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile::PlayStyle;

pub const CHOICE_COUNT: usize = 3;
pub const CHOICE_ZOOM_UNFOCUSED: f32 = 0.5;
pub const CHOICE_ZOOM_FOCUSED: f32 = 1.0;
pub const CHOICE_ZOOM_TWEEN_SECONDS: f32 = 0.125;
pub const CONFIRM_EXIT_SECONDS: f32 = 0.415;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Single,
    Versus,
    Double,
}

impl Choice {
    #[inline(always)]
    pub const fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Single,
            1 => Self::Versus,
            _ => Self::Double,
        }
    }

    #[inline(always)]
    pub const fn play_style(self, game: GameFlag) -> PlayStyle {
        match (game, self) {
            (GameFlag::Dance, Self::Single) => PlayStyle::Single,
            (GameFlag::Dance, Self::Versus) => PlayStyle::Versus,
            (GameFlag::Dance, Self::Double) => PlayStyle::Double,
            (GameFlag::Pump, Self::Single) => PlayStyle::PumpSingle,
            (GameFlag::Pump, Self::Versus) => PlayStyle::PumpVersus,
            (GameFlag::Pump, Self::Double) => PlayStyle::PumpDouble,
        }
    }

    #[inline(always)]
    pub const fn index_for_style(style: PlayStyle) -> usize {
        match style {
            PlayStyle::Single => 0,
            PlayStyle::Versus => 1,
            PlayStyle::Double => 2,
            PlayStyle::PumpSingle => 0,
            PlayStyle::PumpVersus => 1,
            PlayStyle::PumpDouble => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEffect {
    None,
    Move,
    Confirm(PlayStyle),
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    selected_index: usize,
    choice_zooms: [f32; CHOICE_COUNT],
    exit_requested: bool,
    exit_chosen_anim: bool,
    exit_target: Option<Screen>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            selected_index: 0,
            choice_zooms: [CHOICE_ZOOM_UNFOCUSED; CHOICE_COUNT],
            exit_requested: false,
            exit_chosen_anim: false,
            exit_target: None,
        }
    }
}

impl State {
    #[inline(always)]
    pub const fn selected_index(&self) -> usize {
        self.selected_index
    }

    #[inline(always)]
    pub fn set_selected_index(&mut self, index: usize) {
        debug_assert!(index < CHOICE_COUNT);
        self.selected_index = index;
    }

    #[inline(always)]
    pub fn choice_zoom(&self, index: usize) -> f32 {
        self.choice_zooms[index]
    }

    #[inline(always)]
    pub const fn exit_chosen_anim(&self) -> bool {
        self.exit_chosen_anim
    }
}

pub fn update(state: &mut State, dt: f32, confirm_exit_elapsed: f32) -> Option<Screen> {
    if state.exit_requested {
        if let Some(target) = state.exit_target
            && confirm_exit_elapsed >= CONFIRM_EXIT_SECONDS
        {
            state.exit_target = None;
            return Some(target);
        }
        return None;
    }

    let speed = (CHOICE_ZOOM_FOCUSED - CHOICE_ZOOM_UNFOCUSED) / CHOICE_ZOOM_TWEEN_SECONDS;
    let max_step = speed * dt.max(0.0);
    for (index, zoom) in state.choice_zooms.iter_mut().enumerate() {
        let target = if index == state.selected_index {
            CHOICE_ZOOM_FOCUSED
        } else {
            CHOICE_ZOOM_UNFOCUSED
        };
        let delta = target - *zoom;
        if delta.abs() <= max_step {
            *zoom = target;
        } else {
            *zoom += delta.signum() * max_step;
        }
    }
    None
}

fn confirm(state: &mut State, game: GameFlag) -> InputEffect {
    state.exit_requested = true;
    state.exit_chosen_anim = true;
    state.exit_target = Some(Screen::SelectPlayMode);
    InputEffect::Confirm(Choice::from_index(state.selected_index).play_style(game))
}

pub fn handle_input(state: &mut State, ev: &InputEvent, game: GameFlag) -> InputEffect {
    if !ev.pressed || state.exit_requested {
        return InputEffect::None;
    }

    match ev.action {
        VirtualAction::p1_left
        | VirtualAction::p2_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_menu_left => {
            state.selected_index = (state.selected_index + CHOICE_COUNT - 1) % CHOICE_COUNT;
            InputEffect::Move
        }
        VirtualAction::p1_right
        | VirtualAction::p2_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_menu_right => {
            state.selected_index = (state.selected_index + 1) % CHOICE_COUNT;
            InputEffect::Move
        }
        VirtualAction::p1_up
        | VirtualAction::p2_up
        | VirtualAction::p1_down
        | VirtualAction::p2_down
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_menu_up
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_menu_down => InputEffect::None,
        VirtualAction::p1_start | VirtualAction::p2_start => confirm(state, game),
        VirtualAction::p1_back | VirtualAction::p2_back => {
            state.exit_requested = true;
            state.exit_chosen_anim = false;
            state.exit_target = None;
            InputEffect::Back
        }
        _ => InputEffect::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_game_selects_the_style_family() {
        assert_eq!(
            Choice::Single.play_style(GameFlag::Dance),
            PlayStyle::Single
        );
        assert_eq!(
            Choice::Versus.play_style(GameFlag::Dance),
            PlayStyle::Versus
        );
        assert_eq!(
            Choice::Double.play_style(GameFlag::Dance),
            PlayStyle::Double
        );
        assert_eq!(
            Choice::Single.play_style(GameFlag::Pump),
            PlayStyle::PumpSingle
        );
        assert_eq!(
            Choice::Versus.play_style(GameFlag::Pump),
            PlayStyle::PumpVersus
        );
        assert_eq!(
            Choice::Double.play_style(GameFlag::Pump),
            PlayStyle::PumpDouble
        );
    }

    #[test]
    fn dance_and_pump_styles_share_three_choice_positions() {
        assert_eq!(CHOICE_COUNT, 3);
        assert_eq!(Choice::index_for_style(PlayStyle::Single), 0);
        assert_eq!(Choice::index_for_style(PlayStyle::PumpSingle), 0);
        assert_eq!(Choice::index_for_style(PlayStyle::Versus), 1);
        assert_eq!(Choice::index_for_style(PlayStyle::PumpVersus), 1);
        assert_eq!(Choice::index_for_style(PlayStyle::Double), 2);
        assert_eq!(Choice::index_for_style(PlayStyle::PumpDouble), 2);
    }
}
