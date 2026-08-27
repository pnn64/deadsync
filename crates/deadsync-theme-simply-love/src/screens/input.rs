use crate::screens::components::shared::{test_input, transitions, visual_style_bg};
use crate::screens::{Screen, ThemeEffect};
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadsync_config::prelude::GameFlag;
use deadsync_core::input::InputSource;
use deadsync_input::RawKeyboardEvent;
use deadsync_input::{InputEvent, PadEvent, VirtualAction, with_keymap};
use deadsync_profile::PlayerSide;
use std::time::{Duration, Instant};

const MENU_LR_CHORD_WINDOW: Duration = Duration::from_millis(75);
const MENU_LR_LEFT: u8 = 1 << 0;
const MENU_LR_RIGHT: u8 = 1 << 1;

#[inline(always)]
pub const fn reset_hold_repeat(
    held_for: &mut Duration,
    next_repeat_at: &mut Duration,
    initial_delay: Duration,
) {
    *held_for = Duration::ZERO;
    *next_repeat_at = initial_delay;
}

pub fn advance_hold_repeat(
    held_for: &mut Duration,
    next_repeat_at: &mut Duration,
    repeat_interval: Duration,
    dt: f32,
) -> bool {
    if dt <= 0.0 || !dt.is_finite() {
        return false;
    }
    *held_for = held_for.saturating_add(Duration::from_secs_f32(dt));
    if *held_for <= *next_repeat_at {
        return false;
    }
    if repeat_interval == Duration::ZERO {
        *next_repeat_at = *held_for;
        return true;
    }
    while *next_repeat_at <= *held_for {
        *next_repeat_at = next_repeat_at.saturating_add(repeat_interval);
    }
    true
}

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

/// Apply `ITGmania`'s game-button-to-menu-button mapping.
///
/// Pump panels use `ITGmania`'s secondary menu mapping. Like `ITGmania`, the
/// secondary mapping is disabled when only dedicated menu buttons are allowed.
#[inline(always)]
#[must_use]
pub const fn menu_action(
    action: VirtualAction,
    game: GameFlag,
    only_dedicated_menu_buttons: bool,
) -> VirtualAction {
    match (game, only_dedicated_menu_buttons, action) {
        (GameFlag::Pump, false, VirtualAction::p1_down) => VirtualAction::p1_menu_up,
        (GameFlag::Pump, false, VirtualAction::p1_up) => VirtualAction::p1_menu_down,
        (GameFlag::Pump, false, VirtualAction::p1_center) => VirtualAction::p1_start,
        (GameFlag::Pump, false, VirtualAction::p2_down) => VirtualAction::p2_menu_up,
        (GameFlag::Pump, false, VirtualAction::p2_up) => VirtualAction::p2_menu_down,
        (GameFlag::Pump, false, VirtualAction::p2_center) => VirtualAction::p2_start,
        _ => action,
    }
}

#[inline(always)]
#[must_use]
pub const fn dedicated_blocks_arrow(
    action: VirtualAction,
    only_dedicated_menu_buttons: bool,
) -> bool {
    only_dedicated_menu_buttons && action.is_gameplay_arrow()
}

#[inline(always)]
#[must_use]
pub const fn menu_lr_side(action: VirtualAction) -> Option<PlayerSide> {
    match action {
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_right => Some(PlayerSide::P1),
        VirtualAction::p2_left
        | VirtualAction::p2_menu_left
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => Some(PlayerSide::P2),
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
    const fn side_state(&self, side: PlayerSide) -> &MenuLrChordSideState {
        match side {
            PlayerSide::P1 => &self.p1,
            PlayerSide::P2 => &self.p2,
        }
    }

    #[inline(always)]
    const fn side_state_mut(&mut self, side: PlayerSide) -> &mut MenuLrChordSideState {
        match side {
            PlayerSide::P1 => &mut self.p1,
            PlayerSide::P2 => &mut self.p2,
        }
    }

    pub fn update(&mut self, ev: &InputEvent) -> Option<PlayerSide> {
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

    #[inline(always)]
    pub fn track(&mut self, ev: &InputEvent) {
        let _ = self.update(ev);
    }

    #[inline(always)]
    #[must_use]
    pub const fn both_held(&self, side: PlayerSide) -> bool {
        self.side_state(side).held_mask == (MENU_LR_LEFT | MENU_LR_RIGHT)
    }
}

pub(super) fn three_key_menu_action_enabled(
    chord: &mut MenuLrChordTracker,
    ev: &InputEvent,
    enabled: bool,
) -> Option<(PlayerSide, ThreeKeyMenuAction)> {
    if !enabled {
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
            Some((PlayerSide::P1, ThreeKeyMenuAction::Prev))
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            Some((PlayerSide::P1, ThreeKeyMenuAction::Next))
        }
        VirtualAction::p1_start => Some((PlayerSide::P1, ThreeKeyMenuAction::Confirm)),
        VirtualAction::p2_left | VirtualAction::p2_menu_left => {
            Some((PlayerSide::P2, ThreeKeyMenuAction::Prev))
        }
        VirtualAction::p2_right | VirtualAction::p2_menu_right => {
            Some((PlayerSide::P2, ThreeKeyMenuAction::Next))
        }
        VirtualAction::p2_start => Some((PlayerSide::P2, ThreeKeyMenuAction::Confirm)),
        _ => None,
    }
}

#[inline(always)]
pub fn track_menu_lr_chord(chord: &mut MenuLrChordTracker, ev: &InputEvent) {
    chord.track(ev);
}

#[inline(always)]
#[must_use]
pub const fn menu_lr_both_held(chord: &MenuLrChordTracker, side: PlayerSide) -> bool {
    chord.both_held(side)
}
/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;
const BACK_HOLD_SECONDS: f32 = 0.33;

pub fn three_key_menu_action(
    chord: &mut MenuLrChordTracker,
    ev: &InputEvent,
    enabled: bool,
) -> Option<(PlayerSide, ThreeKeyMenuAction)> {
    three_key_menu_action_enabled(chord, ev, enabled)
}

pub struct State {
    pub active_color_index: i32,
    bg: visual_style_bg::State,
    test_input: test_input::State,
    dedicated_three_key_nav: bool,
    raw_back_hold_active: bool,
    select_back_held: [bool; 2],
    back_hold_secs: f32,
}

#[must_use]
pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
        test_input: test_input::State::default(),
        dedicated_three_key_nav: false,
        raw_back_hold_active: false,
        select_back_held: [false; 2],
        back_hold_secs: 0.0,
    }
}

pub const fn on_enter(state: &mut State, dedicated_three_key_nav: bool) {
    state.dedicated_three_key_nav = dedicated_three_key_nav;
    state.raw_back_hold_active = false;
    state.select_back_held = [false; 2];
    state.back_hold_secs = 0.0;
}

#[inline(always)]
fn return_hold_active(state: &State) -> bool {
    state.raw_back_hold_active || state.select_back_held.into_iter().any(|held| held)
}

/* ------------------------------- update ------------------------------- */

pub fn update(state: &mut State, dt: f32) -> Option<ThemeEffect> {
    if !return_hold_active(state) {
        state.back_hold_secs = 0.0;
        return None;
    }
    state.back_hold_secs += dt;
    if state.back_hold_secs < BACK_HOLD_SECONDS {
        return None;
    }
    state.raw_back_hold_active = false;
    state.select_back_held = [false; 2];
    state.back_hold_secs = 0.0;
    Some(ThemeEffect::Navigate(Screen::Options))
}

/* ----------------------------- transitions ----------------------------- */

#[must_use]
pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

#[must_use]
pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

/* ------------------------------- input -------------------------------- */

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    if ev.pressed
        && ev.source == InputSource::Gamepad
        && matches!(ev.action, VirtualAction::p1_back | VirtualAction::p2_back)
    {
        return ThemeEffect::Navigate(Screen::Options);
    }
    test_input::apply_virtual_input(&mut state.test_input, ev);
    let select_side = match ev.action {
        VirtualAction::p1_select => Some(0),
        VirtualAction::p2_select => Some(1),
        _ => None,
    };
    if state.dedicated_three_key_nav
        && let Some(player_idx) = select_side
    {
        let was_active = return_hold_active(state);
        state.select_back_held[player_idx] = ev.pressed;
        if ev.pressed && !was_active {
            state.back_hold_secs = 0.0;
        } else if !return_hold_active(state) {
            state.back_hold_secs = 0.0;
        }
    }
    ThemeEffect::None
}

pub fn handle_raw_key_event(state: &mut State, key_event: &RawKeyboardEvent) -> ThemeEffect {
    test_input::apply_raw_key_event(&mut state.test_input, key_event);
    if key_event.pressed && key_event.repeat {
        return ThemeEffect::None;
    }
    let is_back = with_keymap(|km| {
        km.raw_key_event_has_action(key_event, |action| {
            matches!(action, VirtualAction::p1_back | VirtualAction::p2_back)
        })
    });
    if !is_back {
        return ThemeEffect::None;
    }
    if key_event.pressed {
        if !return_hold_active(state) {
            state.back_hold_secs = 0.0;
        }
        state.raw_back_hold_active = true;
    } else {
        state.raw_back_hold_active = false;
        if !return_hold_active(state) {
            state.back_hold_secs = 0.0;
        }
    }
    ThemeEffect::None
}

/// Raw pad events are used to approximate Simply Love's "unmapped" device list.
pub fn handle_raw_pad_event(state: &mut State, pad_event: &PadEvent) {
    test_input::apply_raw_pad_event(&mut state.test_input, pad_event);
}

/* ------------------------------- drawing ------------------------------- */

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    game: GameFlag,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    actors.reserve(56);

    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
            visual_policy,
        },
    );

    actors.extend(test_input::build_test_input_screen_content(
        &state.test_input,
        game,
        state.active_color_index,
        visual_policy.machine_font,
        state.dedicated_three_key_nav,
    ));
}

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(56);
    push_actors(
        &mut actors,
        state,
        GameFlag::Dance,
        crate::views::SimplyLoveVisualPolicyView::default(),
    );
    actors
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::input::InputSource;
    use std::time::Instant;

    fn input_event(action: VirtualAction, pressed: bool) -> InputEvent {
        let now = Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed,
            source: InputSource::Gamepad,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    #[test]
    fn pump_panels_use_itgmania_menu_mapping() {
        assert_eq!(
            menu_action(VirtualAction::p1_down, GameFlag::Pump, false),
            VirtualAction::p1_menu_up,
        );
        assert_eq!(
            menu_action(VirtualAction::p1_up, GameFlag::Pump, false),
            VirtualAction::p1_menu_down,
        );
        assert_eq!(
            menu_action(VirtualAction::p1_center, GameFlag::Pump, false),
            VirtualAction::p1_start,
        );
        assert_eq!(
            menu_action(VirtualAction::p2_down, GameFlag::Pump, false),
            VirtualAction::p2_menu_up,
        );
        assert_eq!(
            menu_action(VirtualAction::p2_up, GameFlag::Pump, false),
            VirtualAction::p2_menu_down,
        );
        assert_eq!(
            menu_action(VirtualAction::p2_center, GameFlag::Pump, false),
            VirtualAction::p2_start,
        );
        assert_eq!(
            menu_action(VirtualAction::p1_center, GameFlag::Pump, true),
            VirtualAction::p1_center,
        );
        assert_eq!(
            menu_action(VirtualAction::p1_center, GameFlag::Dance, false),
            VirtualAction::p1_center,
        );
    }

    #[test]
    fn dedicated_three_key_select_hold_returns_to_options() {
        let mut state = init();
        on_enter(&mut state, true);

        assert!(matches!(
            handle_input(&mut state, &input_event(VirtualAction::p1_select, true)),
            ThemeEffect::None
        ));
        assert!(update(&mut state, BACK_HOLD_SECONDS - 0.01).is_none());
        assert!(matches!(
            update(&mut state, 0.02),
            Some(ThemeEffect::Navigate(Screen::Options))
        ));
    }

    #[test]
    fn releasing_three_key_select_cancels_the_return_hold() {
        let mut state = init();
        on_enter(&mut state, true);

        handle_input(&mut state, &input_event(VirtualAction::p1_select, true));
        assert!(update(&mut state, BACK_HOLD_SECONDS - 0.01).is_none());
        handle_input(&mut state, &input_event(VirtualAction::p1_select, false));

        assert!(update(&mut state, BACK_HOLD_SECONDS + 0.1).is_none());
    }

    #[test]
    fn select_does_not_leave_test_input_outside_dedicated_three_key_mode() {
        let mut state = init();
        on_enter(&mut state, false);

        handle_input(&mut state, &input_event(VirtualAction::p1_select, true));

        assert!(update(&mut state, BACK_HOLD_SECONDS + 0.1).is_none());
    }
}
