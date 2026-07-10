use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile::PlayerSide;
use std::time::{Duration, Instant};

const MENU_LR_CHORD_WINDOW: Duration = Duration::from_millis(75);
const MENU_LR_LEFT: u8 = 1 << 0;
const MENU_LR_RIGHT: u8 = 1 << 1;

#[inline(always)]
pub fn reset_hold_repeat(
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

#[inline(always)]
pub const fn dedicated_blocks_arrow(
    action: VirtualAction,
    only_dedicated_menu_buttons: bool,
) -> bool {
    only_dedicated_menu_buttons && action.is_gameplay_arrow()
}

#[inline(always)]
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
    fn side_state(&self, side: PlayerSide) -> &MenuLrChordSideState {
        match side {
            PlayerSide::P1 => &self.p1,
            PlayerSide::P2 => &self.p2,
        }
    }

    #[inline(always)]
    fn side_state_mut(&mut self, side: PlayerSide) -> &mut MenuLrChordSideState {
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
    pub fn both_held(&self, side: PlayerSide) -> bool {
        self.side_state(side).held_mask == (MENU_LR_LEFT | MENU_LR_RIGHT)
    }
}

pub fn three_key_menu_action(
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
pub fn menu_lr_both_held(chord: &MenuLrChordTracker, side: PlayerSide) -> bool {
    chord.both_held(side)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::input::InputSource;

    fn input_event(action: VirtualAction, pressed: bool, timestamp: Instant) -> InputEvent {
        InputEvent {
            action,
            input_slot: 0,
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
    fn dedicated_blocks_arrows() {
        assert!(dedicated_blocks_arrow(VirtualAction::p1_left, true));
        assert!(dedicated_blocks_arrow(VirtualAction::p2_down, true));
        assert!(!dedicated_blocks_arrow(VirtualAction::p1_menu_left, true));
        assert!(!dedicated_blocks_arrow(VirtualAction::p1_start, true));
        assert!(!dedicated_blocks_arrow(VirtualAction::p1_left, false));
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
