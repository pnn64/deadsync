use crate::{InputEvent, VirtualAction};
use std::time::{Duration, Instant};

const MENU_LR_CHORD_WINDOW: Duration = Duration::from_millis(75);
const MENU_LR_LEFT: u8 = 1 << 0;
const MENU_LR_RIGHT: u8 = 1 << 1;

/// Tracks left and right edges for one player. The caller selects the player
/// and decides whether dance arrows count as menu buttons.
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuLrChord {
    held_mask: u8,
    left_pressed_at: Option<Instant>,
    right_pressed_at: Option<Instant>,
    fired: bool,
}

impl MenuLrChord {
    /// Returns true once per press cycle, when both presses fall within 75 ms.
    pub fn update(&mut self, ev: &InputEvent) -> bool {
        let Some(bit) = menu_lr_bit(ev.action) else {
            return false;
        };
        if ev.pressed {
            self.held_mask |= bit;
            if bit == MENU_LR_LEFT {
                self.left_pressed_at = Some(ev.timestamp);
            } else {
                self.right_pressed_at = Some(ev.timestamp);
            }
            if !self.fired
                && self.both_held()
                && menu_lr_times_are_simultaneous(self.left_pressed_at, self.right_pressed_at)
            {
                self.fired = true;
                return true;
            }
        } else {
            self.held_mask &= !bit;
            if bit == MENU_LR_LEFT {
                self.left_pressed_at = None;
            } else {
                self.right_pressed_at = None;
            }
            if !self.both_held() {
                self.fired = false;
            }
        }
        false
    }

    #[inline(always)]
    #[must_use]
    pub const fn both_held(&self) -> bool {
        self.held_mask == (MENU_LR_LEFT | MENU_LR_RIGHT)
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
