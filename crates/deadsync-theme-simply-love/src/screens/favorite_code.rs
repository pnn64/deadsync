use deadsync_input::{PadDir, VirtualAction, pad_dir_from_action};
use deadsync_profile::{PlayerSide, player_side_index};
use std::time::{Duration, Instant};

// Simply Love [ScreenSelectMusic] / [ScreenEvaluation] Code* metrics:
// Favorite1 = "Right,Down,Left,Up,Right"
// Favorite2 = "Left,Down,Right,Up,Left"
const CODE_RIGHT: [PadDir; 5] = [
    PadDir::Right,
    PadDir::Down,
    PadDir::Left,
    PadDir::Up,
    PadDir::Right,
];
const CODE_LEFT: [PadDir; 5] = [
    PadDir::Left,
    PadDir::Down,
    PadDir::Right,
    PadDir::Up,
    PadDir::Left,
];
// InputQueueCode gives a five-press code (5 - 1) * 0.6 seconds total.
const CODE_TIMEOUT: Duration = Duration::from_millis(2_400);

#[derive(Clone, Copy, Debug, Default)]
struct CodeState {
    index: usize,
    started_at: Option<Instant>,
}

/// Tracks the Simply Love Favorite1/Favorite2 pad code sequences for both
/// players simultaneously. Embed one instance in each screen's State that
/// supports the favorite hotkey (ScreenSelectMusic, ScreenEvaluation).
#[derive(Clone, Debug, Default)]
pub struct FavoriteCodeTracker {
    sides: [[CodeState; 2]; 2],
}

impl FavoriteCodeTracker {
    /// Feed a gameplay-arrow press. Both mirrored codes belong to the player
    /// who entered them; `Favorite1` and `Favorite2` are code names, not sides.
    pub fn check(&mut self, action: VirtualAction, timestamp: Instant) -> Option<PlayerSide> {
        let side = Self::input_side(action)?;
        let side_index = player_side_index(side);
        let Some(dir) = pad_dir_from_action(action) else {
            self.sides[side_index] = Default::default();
            return None;
        };
        let states = &mut self.sides[side_index];
        let matched_right = Self::check_one(&CODE_RIGHT, &mut states[0], dir, timestamp);
        let matched_left = Self::check_one(&CODE_LEFT, &mut states[1], dir, timestamp);
        if matched_right || matched_left {
            *states = Default::default();
            Some(side)
        } else {
            None
        }
    }

    const fn input_side(action: VirtualAction) -> Option<PlayerSide> {
        match action {
            VirtualAction::p1_left
            | VirtualAction::p1_right
            | VirtualAction::p1_up
            | VirtualAction::p1_down
            | VirtualAction::p1_start
            | VirtualAction::p1_back
            | VirtualAction::p1_menu_up
            | VirtualAction::p1_menu_down
            | VirtualAction::p1_menu_left
            | VirtualAction::p1_menu_right
            | VirtualAction::p1_select
            | VirtualAction::p1_operator
            | VirtualAction::p1_restart => Some(PlayerSide::P1),
            VirtualAction::p2_left
            | VirtualAction::p2_right
            | VirtualAction::p2_up
            | VirtualAction::p2_down
            | VirtualAction::p2_start
            | VirtualAction::p2_back
            | VirtualAction::p2_menu_up
            | VirtualAction::p2_menu_down
            | VirtualAction::p2_menu_left
            | VirtualAction::p2_menu_right
            | VirtualAction::p2_select
            | VirtualAction::p2_operator
            | VirtualAction::p2_restart => Some(PlayerSide::P2),
            VirtualAction::system_fast_forward | VirtualAction::system_slow_down => None,
        }
    }

    fn check_one(
        code: &[PadDir; 5],
        state: &mut CodeState,
        dir: PadDir,
        timestamp: Instant,
    ) -> bool {
        if state.started_at.is_some_and(|started_at| {
            timestamp
                .checked_duration_since(started_at)
                .is_none_or(|elapsed| elapsed > CODE_TIMEOUT)
        }) {
            *state = Default::default();
        }

        if code[state.index] == dir {
            if state.index == 0 {
                state.started_at = Some(timestamp);
            }
            state.index += 1;
            if state.index == code.len() {
                *state = Default::default();
                return true;
            }
        } else if code[0] == dir {
            state.index = 1;
            state.started_at = Some(timestamp);
        } else {
            *state = Default::default();
        }
        false
    }
}

#[inline(always)]
pub const fn toggle_sfx(is_now_favorite: bool) -> &'static str {
    if is_now_favorite {
        "assets/sounds/start.ogg"
    } else {
        "assets/sounds/common_invalid.ogg"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P1_RIGHT: [VirtualAction; 5] = [
        VirtualAction::p1_right,
        VirtualAction::p1_down,
        VirtualAction::p1_left,
        VirtualAction::p1_up,
        VirtualAction::p1_right,
    ];
    const P1_LEFT: [VirtualAction; 5] = [
        VirtualAction::p1_left,
        VirtualAction::p1_down,
        VirtualAction::p1_right,
        VirtualAction::p1_up,
        VirtualAction::p1_left,
    ];
    const P2_RIGHT: [VirtualAction; 5] = [
        VirtualAction::p2_right,
        VirtualAction::p2_down,
        VirtualAction::p2_left,
        VirtualAction::p2_up,
        VirtualAction::p2_right,
    ];

    fn enter(
        tracker: &mut FavoriteCodeTracker,
        code: [VirtualAction; 5],
        start: Instant,
        step: Duration,
    ) -> Option<PlayerSide> {
        code.into_iter()
            .enumerate()
            .find_map(|(index, action)| tracker.check(action, start + step * index as u32))
    }

    #[test]
    fn either_mirror_toggles_the_entering_player() {
        let now = Instant::now();
        assert_eq!(
            enter(
                &mut FavoriteCodeTracker::default(),
                P1_LEFT,
                now,
                Duration::from_millis(100),
            ),
            Some(PlayerSide::P1)
        );
        assert_eq!(
            enter(
                &mut FavoriteCodeTracker::default(),
                P2_RIGHT,
                now,
                Duration::from_millis(100),
            ),
            Some(PlayerSide::P2)
        );
    }

    #[test]
    fn player_queues_do_not_combine() {
        let mut tracker = FavoriteCodeTracker::default();
        let now = Instant::now();
        let mixed = [
            VirtualAction::p1_right,
            VirtualAction::p2_down,
            VirtualAction::p1_left,
            VirtualAction::p2_up,
            VirtualAction::p1_right,
        ];
        for (index, action) in mixed.into_iter().enumerate() {
            assert_eq!(
                tracker.check(action, now + Duration::from_millis(index as u64 * 100)),
                None
            );
        }
    }

    #[test]
    fn code_must_fit_itgmania_total_window() {
        let mut tracker = FavoriteCodeTracker::default();
        let now = Instant::now();
        for (index, action) in P1_RIGHT.into_iter().enumerate() {
            let offset = [0, 500, 1_000, 1_500, 2_401][index];
            assert_eq!(
                tracker.check(action, now + Duration::from_millis(offset)),
                None
            );
        }
    }

    #[test]
    fn other_buttons_break_partial_code() {
        let mut tracker = FavoriteCodeTracker::default();
        let now = Instant::now();
        assert_eq!(tracker.check(P1_RIGHT[0], now), None);
        assert_eq!(
            tracker.check(P1_RIGHT[1], now + Duration::from_millis(100)),
            None
        );
        assert_eq!(
            tracker.check(VirtualAction::p1_start, now + Duration::from_millis(200)),
            None
        );
        for (index, action) in P1_RIGHT[2..].iter().copied().enumerate() {
            assert_eq!(
                tracker.check(
                    action,
                    now + Duration::from_millis(300 + index as u64 * 100)
                ),
                None
            );
        }
    }
}
