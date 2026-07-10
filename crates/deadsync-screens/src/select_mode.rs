use crate::Screen;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile::PlayMode;

pub const CHOICE_COUNT: usize = 2;
pub const CHOICE_ZOOM_FOCUSED: f32 = 0.75;
pub const CHOICE_ZOOM_UNFOCUSED: f32 = 0.3;
pub const CURSOR_HEIGHT: f32 = 40.0;
pub const EXIT_TOTAL_SECONDS: f32 = 0.9;

const CHOICE_ZOOM_TWEEN_SECONDS: f32 = 0.1;
const CURSOR_TWEEN_SECONDS: f32 = 0.1;
const CURSOR_START_Y: f32 = -60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Regular,
    Marathon,
}

impl Choice {
    #[inline(always)]
    pub const fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Regular,
            _ => Self::Marathon,
        }
    }

    #[inline(always)]
    pub const fn play_mode(self) -> PlayMode {
        match self {
            Self::Regular => PlayMode::Regular,
            Self::Marathon => PlayMode::Marathon,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEffect {
    None,
    Move,
    Confirm(PlayMode),
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    selected_index: usize,
    cursor_y: f32,
    choice_zooms: [f32; CHOICE_COUNT],
    demo_time: f32,
    exit_requested: bool,
    exit_target: Option<Screen>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            selected_index: 0,
            cursor_y: CURSOR_START_Y,
            choice_zooms: [CHOICE_ZOOM_UNFOCUSED; CHOICE_COUNT],
            demo_time: 0.0,
            exit_requested: false,
            exit_target: None,
        }
    }
}

impl State {
    pub fn reset(&mut self, play_mode: PlayMode) {
        self.selected_index = match play_mode {
            PlayMode::Regular => 0,
            PlayMode::Marathon => 1,
        };
        self.demo_time = 0.0;
        self.cursor_y = cursor_target_y(self.selected_index);
        for (index, zoom) in self.choice_zooms.iter_mut().enumerate() {
            *zoom = if index == self.selected_index {
                CHOICE_ZOOM_FOCUSED
            } else {
                CHOICE_ZOOM_UNFOCUSED
            };
        }
        self.exit_requested = false;
        self.exit_target = None;
    }

    #[inline(always)]
    pub const fn selected_index(&self) -> usize {
        self.selected_index
    }

    #[inline(always)]
    pub const fn cursor_y(&self) -> f32 {
        self.cursor_y
    }

    #[inline(always)]
    pub fn choice_zoom(&self, index: usize) -> f32 {
        self.choice_zooms[index]
    }

    #[inline(always)]
    pub const fn demo_time(&self) -> f32 {
        self.demo_time
    }

    #[inline(always)]
    pub const fn exit_requested(&self) -> bool {
        self.exit_requested
    }
}

#[inline(always)]
fn cursor_target_y(index: usize) -> f32 {
    CURSOR_HEIGHT.mul_add(index as f32, CURSOR_START_Y)
}

pub fn update(state: &mut State, dt: f32) {
    let dt = dt.max(0.0);
    state.demo_time = (state.demo_time + dt).rem_euclid(60.0);

    let target_y = cursor_target_y(state.selected_index);
    let max_step = CURSOR_HEIGHT / CURSOR_TWEEN_SECONDS * dt;
    let delta_y = target_y - state.cursor_y;
    if delta_y.abs() <= max_step {
        state.cursor_y = target_y;
    } else {
        state.cursor_y += delta_y.signum() * max_step;
    }

    let zoom_speed = (CHOICE_ZOOM_FOCUSED - CHOICE_ZOOM_UNFOCUSED) / CHOICE_ZOOM_TWEEN_SECONDS;
    let zoom_max_step = zoom_speed * dt;
    for (index, zoom) in state.choice_zooms.iter_mut().enumerate() {
        let target = if index == state.selected_index {
            CHOICE_ZOOM_FOCUSED
        } else {
            CHOICE_ZOOM_UNFOCUSED
        };
        let delta = target - *zoom;
        if delta.abs() <= zoom_max_step {
            *zoom = target;
        } else {
            *zoom += delta.signum() * zoom_max_step;
        }
    }
}

pub fn finish_exit(state: &mut State, exit_elapsed: f32) -> Option<Screen> {
    if state.exit_requested
        && let Some(target) = state.exit_target
        && exit_elapsed >= EXIT_TOTAL_SECONDS
    {
        state.exit_target = None;
        return Some(target);
    }
    None
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> InputEffect {
    if !ev.pressed || state.exit_requested {
        return InputEffect::None;
    }

    match ev.action {
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => {
            state.selected_index = (state.selected_index + CHOICE_COUNT - 1) % CHOICE_COUNT;
            state.demo_time = 0.0;
            InputEffect::Move
        }
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => {
            state.selected_index = (state.selected_index + 1) % CHOICE_COUNT;
            state.demo_time = 0.0;
            InputEffect::Move
        }
        VirtualAction::p1_start | VirtualAction::p2_start => {
            state.exit_requested = true;
            state.exit_target = Some(Screen::ProfileLoad);
            InputEffect::Confirm(Choice::from_index(state.selected_index).play_mode())
        }
        VirtualAction::p1_back | VirtualAction::p2_back => {
            state.exit_requested = true;
            state.exit_target = Some(Screen::Menu);
            InputEffect::Back
        }
        _ => InputEffect::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::input::InputSource;
    use std::time::Instant;

    fn input(action: VirtualAction, pressed: bool) -> InputEvent {
        let now = Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed,
            source: InputSource::Keyboard,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    #[test]
    fn reset_snaps_to_persisted_mode() {
        let mut state = State::default();
        state.reset(PlayMode::Marathon);
        assert_eq!(state.selected_index(), 1);
        assert_eq!(state.cursor_y(), -20.0);
        assert_eq!(state.choice_zoom(0), CHOICE_ZOOM_UNFOCUSED);
        assert_eq!(state.choice_zoom(1), CHOICE_ZOOM_FOCUSED);
        assert_eq!(state.demo_time(), 0.0);
        assert!(!state.exit_requested());
    }

    #[test]
    fn navigation_wraps_and_resets_demo() {
        let mut state = State::default();
        update(&mut state, 1.5);
        assert_eq!(state.demo_time(), 1.5);
        assert_eq!(
            handle_input(&mut state, &input(VirtualAction::p1_left, true)),
            InputEffect::Move
        );
        assert_eq!(state.selected_index(), 1);
        assert_eq!(state.demo_time(), 0.0);
        assert_eq!(
            handle_input(&mut state, &input(VirtualAction::p2_right, true)),
            InputEffect::Move
        );
        assert_eq!(state.selected_index(), 0);
    }

    #[test]
    fn cursor_and_zoom_interpolate_without_overshoot() {
        let mut state = State::default();
        let _ = handle_input(&mut state, &input(VirtualAction::p1_right, true));
        update(&mut state, 0.05);
        assert_eq!(state.cursor_y(), -40.0);
        assert!((state.choice_zoom(0) - 0.3).abs() < f32::EPSILON);
        assert!((state.choice_zoom(1) - 0.525).abs() < f32::EPSILON);
        update(&mut state, 1.0);
        assert_eq!(state.cursor_y(), -20.0);
        assert_eq!(state.choice_zoom(1), CHOICE_ZOOM_FOCUSED);
    }

    #[test]
    fn confirm_maps_mode_and_waits_for_exit_animation() {
        let mut state = State::default();
        let _ = handle_input(&mut state, &input(VirtualAction::p1_right, true));
        assert_eq!(
            handle_input(&mut state, &input(VirtualAction::p1_start, true)),
            InputEffect::Confirm(PlayMode::Marathon)
        );
        update(&mut state, 0.0);
        assert_eq!(finish_exit(&mut state, EXIT_TOTAL_SECONDS - 0.001), None);
        assert_eq!(
            finish_exit(&mut state, EXIT_TOTAL_SECONDS),
            Some(Screen::ProfileLoad)
        );
        assert_eq!(finish_exit(&mut state, 10.0), None);
    }

    #[test]
    fn back_waits_for_exit_and_locks_further_input() {
        let mut state = State::default();
        assert_eq!(
            handle_input(&mut state, &input(VirtualAction::p2_back, true)),
            InputEffect::Back
        );
        assert_eq!(
            handle_input(&mut state, &input(VirtualAction::p1_right, true)),
            InputEffect::None
        );
        update(&mut state, 0.0);
        assert_eq!(
            finish_exit(&mut state, EXIT_TOTAL_SECONDS),
            Some(Screen::Menu)
        );
    }
}
