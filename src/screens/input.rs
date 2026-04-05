use crate::act;
use crate::engine::input::{
    InputEvent, InputSource, PadEvent, RawKeyboardEvent, VirtualAction, with_keymap,
};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_height, screen_width};
use crate::game::profile;
use crate::screens::components::shared::{heart_bg, test_input};
use crate::screens::{Screen, ScreenAction};
use std::time::{Duration, Instant};
/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;
const BACK_HOLD_SECONDS: f32 = 0.33;
const MENU_LR_CHORD_WINDOW: Duration = Duration::from_millis(75);
const MENU_LR_LEFT: u8 = 1 << 0;
const MENU_LR_RIGHT: u8 = 1 << 1;

#[derive(Clone, Copy, Debug, Default)]
struct MenuLrChordSideState {
    held_mask: u8,
    left_pressed_at: Option<Instant>,
    right_pressed_at: Option<Instant>,
    fired: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MenuLrChordTracker {
    p1: MenuLrChordSideState,
    p2: MenuLrChordSideState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreeKeyMenuAction {
    Prev,
    Next,
    Confirm,
    Cancel,
}

#[inline(always)]
pub const fn player_side_ix(side: profile::PlayerSide) -> usize {
    match side {
        profile::PlayerSide::P1 => 0,
        profile::PlayerSide::P2 => 1,
    }
}

#[inline(always)]
pub fn dedicated_three_key_nav_enabled() -> bool {
    let cfg = crate::config::get();
    cfg.three_key_navigation && cfg.only_dedicated_menu_buttons
}

#[inline(always)]
pub const fn menu_lr_side(action: VirtualAction) -> Option<profile::PlayerSide> {
    match action {
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_right => Some(profile::PlayerSide::P1),
        VirtualAction::p2_left
        | VirtualAction::p2_menu_left
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => Some(profile::PlayerSide::P2),
        _ => None,
    }
}

#[inline(always)]
const fn menu_lr_bit(action: VirtualAction) -> Option<u8> {
    match action {
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => Some(MENU_LR_LEFT),
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => Some(MENU_LR_RIGHT),
        _ => None,
    }
}

#[inline(always)]
fn menu_lr_times_are_simultaneous(a: Option<Instant>, b: Option<Instant>) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return false;
    };
    if a >= b {
        a.duration_since(b) <= MENU_LR_CHORD_WINDOW
    } else {
        b.duration_since(a) <= MENU_LR_CHORD_WINDOW
    }
}

impl MenuLrChordTracker {
    #[inline(always)]
    fn side_state_mut(&mut self, side: profile::PlayerSide) -> &mut MenuLrChordSideState {
        match side {
            profile::PlayerSide::P1 => &mut self.p1,
            profile::PlayerSide::P2 => &mut self.p2,
        }
    }

    pub fn update(&mut self, ev: &InputEvent) -> Option<profile::PlayerSide> {
        let Some(side) = menu_lr_side(ev.action) else {
            return None;
        };
        let Some(bit) = menu_lr_bit(ev.action) else {
            return None;
        };
        let side_state = self.side_state_mut(side);
        if ev.pressed {
            side_state.held_mask |= bit;
            if bit == MENU_LR_LEFT {
                side_state.left_pressed_at = Some(ev.timestamp);
            } else {
                side_state.right_pressed_at = Some(ev.timestamp);
            }
            if !side_state.fired
                && side_state.held_mask == (MENU_LR_LEFT | MENU_LR_RIGHT)
                && menu_lr_times_are_simultaneous(
                    side_state.left_pressed_at,
                    side_state.right_pressed_at,
                )
            {
                side_state.fired = true;
                return Some(side);
            }
        } else {
            side_state.held_mask &= !bit;
            if bit == MENU_LR_LEFT {
                side_state.left_pressed_at = None;
            } else {
                side_state.right_pressed_at = None;
            }
            if side_state.held_mask != (MENU_LR_LEFT | MENU_LR_RIGHT) {
                side_state.fired = false;
            }
        }
        None
    }
}

pub fn three_key_menu_action(
    chord: &mut MenuLrChordTracker,
    ev: &InputEvent,
) -> Option<(profile::PlayerSide, ThreeKeyMenuAction)> {
    if !dedicated_three_key_nav_enabled() {
        return None;
    }
    if let Some(side) = chord.update(ev) {
        return Some((side, ThreeKeyMenuAction::Cancel));
    }
    if !ev.pressed {
        return None;
    }
    match ev.action {
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            Some((profile::PlayerSide::P1, ThreeKeyMenuAction::Prev))
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            Some((profile::PlayerSide::P1, ThreeKeyMenuAction::Next))
        }
        VirtualAction::p1_start => Some((profile::PlayerSide::P1, ThreeKeyMenuAction::Confirm)),
        VirtualAction::p2_left | VirtualAction::p2_menu_left => {
            Some((profile::PlayerSide::P2, ThreeKeyMenuAction::Prev))
        }
        VirtualAction::p2_right | VirtualAction::p2_menu_right => {
            Some((profile::PlayerSide::P2, ThreeKeyMenuAction::Next))
        }
        VirtualAction::p2_start => Some((profile::PlayerSide::P2, ThreeKeyMenuAction::Confirm)),
        _ => None,
    }
}

pub struct State {
    pub active_color_index: i32,
    bg: heart_bg::State,
    test_input: test_input::State,
    back_hold_active: bool,
    back_hold_secs: f32,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: heart_bg::State::new(),
        test_input: test_input::State::default(),
        back_hold_active: false,
        back_hold_secs: 0.0,
    }
}

/* ------------------------------- update ------------------------------- */

pub fn update(state: &mut State, dt: f32) -> Option<ScreenAction> {
    if !state.back_hold_active {
        state.back_hold_secs = 0.0;
        return None;
    }
    state.back_hold_secs += dt;
    if state.back_hold_secs < BACK_HOLD_SECONDS {
        return None;
    }
    state.back_hold_active = false;
    state.back_hold_secs = 0.0;
    Some(ScreenAction::Navigate(Screen::Options))
}

/* ----------------------------- transitions ----------------------------- */

pub fn in_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1100):
        linear(TRANSITION_IN_DURATION): alpha(0.0):
        linear(0.0): visible(false)
    );
    (vec![actor], TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

/* ------------------------------- input -------------------------------- */

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if ev.pressed
        && ev.source == InputSource::Gamepad
        && matches!(ev.action, VirtualAction::p1_back | VirtualAction::p2_back)
    {
        return ScreenAction::Navigate(Screen::Options);
    }
    test_input::apply_virtual_input(&mut state.test_input, ev);
    ScreenAction::None
}

pub fn handle_raw_key_event(state: &mut State, key_event: &RawKeyboardEvent) -> ScreenAction {
    test_input::apply_raw_key_event(&mut state.test_input, key_event);
    if key_event.pressed && key_event.repeat {
        return ScreenAction::None;
    }
    let is_back = with_keymap(|km| {
        km.raw_key_event_has_action(key_event, |action| {
            matches!(action, VirtualAction::p1_back | VirtualAction::p2_back)
        })
    });
    if !is_back {
        return ScreenAction::None;
    }
    if key_event.pressed {
        if !state.back_hold_active {
            state.back_hold_active = true;
            state.back_hold_secs = 0.0;
        }
    } else {
        state.back_hold_active = false;
        state.back_hold_secs = 0.0;
    }
    ScreenAction::None
}

/// Raw pad events are used to approximate Simply Love's "unmapped" device list.
pub fn handle_raw_pad_event(state: &mut State, pad_event: &PadEvent) {
    test_input::apply_raw_pad_event(&mut state.test_input, pad_event);
}

/* ------------------------------- drawing ------------------------------- */

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(56);

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    actors.extend(test_input::build_test_input_screen_content(
        &state.test_input,
    ));
    actors
}

#[cfg(test)]
mod tests {
    use super::{MenuLrChordTracker, menu_lr_side};
    use crate::engine::input::{InputEvent, InputSource, VirtualAction};
    use crate::game::profile::PlayerSide;
    use std::time::{Duration, Instant};

    fn input_event(action: VirtualAction, pressed: bool, timestamp: Instant) -> InputEvent {
        InputEvent {
            action,
            pressed,
            source: InputSource::Keyboard,
            timestamp,
            timestamp_host_nanos: 0,
            stored_at: timestamp,
            emitted_at: timestamp,
        }
    }

    #[test]
    fn menu_lr_side_maps_players() {
        assert_eq!(
            menu_lr_side(VirtualAction::p1_menu_left),
            Some(PlayerSide::P1)
        );
        assert_eq!(menu_lr_side(VirtualAction::p2_right), Some(PlayerSide::P2));
        assert_eq!(menu_lr_side(VirtualAction::p1_start), None);
    }

    #[test]
    fn menu_lr_tracker_fires_once_per_chord() {
        let mut tracker = MenuLrChordTracker::default();
        let t0 = Instant::now();
        assert_eq!(
            tracker.update(&input_event(VirtualAction::p1_menu_left, true, t0)),
            None
        );
        assert_eq!(
            tracker.update(&input_event(
                VirtualAction::p1_menu_right,
                true,
                t0 + Duration::from_millis(10),
            )),
            Some(PlayerSide::P1)
        );
        assert_eq!(
            tracker.update(&input_event(
                VirtualAction::p1_menu_left,
                true,
                t0 + Duration::from_millis(20),
            )),
            None
        );
        assert_eq!(
            tracker.update(&input_event(
                VirtualAction::p1_menu_left,
                false,
                t0 + Duration::from_millis(30),
            )),
            None
        );
        assert_eq!(
            tracker.update(&input_event(
                VirtualAction::p1_menu_right,
                false,
                t0 + Duration::from_millis(35),
            )),
            None
        );
        assert_eq!(
            tracker.update(&input_event(
                VirtualAction::p1_menu_left,
                true,
                t0 + Duration::from_millis(50),
            )),
            None
        );
        assert_eq!(
            tracker.update(&input_event(
                VirtualAction::p1_menu_right,
                true,
                t0 + Duration::from_millis(55),
            )),
            Some(PlayerSide::P1)
        );
    }

    #[test]
    fn menu_lr_tracker_rejects_wide_gap() {
        let mut tracker = MenuLrChordTracker::default();
        let t0 = Instant::now();
        assert_eq!(
            tracker.update(&input_event(VirtualAction::p2_left, true, t0)),
            None
        );
        assert_eq!(
            tracker.update(&input_event(
                VirtualAction::p2_right,
                true,
                t0 + Duration::from_millis(120),
            )),
            None
        );
    }
}
