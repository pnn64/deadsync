use deadsync_input::PadDir;
use deadsync_profile as profile_data;
use std::time::{Duration, Instant};

// Simply Love [ScreenSelectMusic] / [ScreenEvaluation] Code* metrics:
// Favorite1 = "Right,Down,Left,Up,Right"
// Favorite2 = "Left,Down,Right,Up,Left"
const CODE_P1: [PadDir; 5] = [
    PadDir::Right,
    PadDir::Down,
    PadDir::Left,
    PadDir::Up,
    PadDir::Right,
];
const CODE_P2: [PadDir; 5] = [
    PadDir::Left,
    PadDir::Down,
    PadDir::Right,
    PadDir::Up,
    PadDir::Left,
];
const CODE_TIMEOUT: Duration = Duration::from_secs(2);

/// Tracks the Simply Love Favorite1/Favorite2 pad code sequences for both
/// players simultaneously. Embed one instance in each screen's State that
/// supports the favorite hotkey (ScreenSelectMusic, ScreenEvaluation).
#[derive(Clone, Debug)]
pub struct FavoriteCodeTracker {
    p1_index: usize,
    p1_last_input: Option<Instant>,
    p2_index: usize,
    p2_last_input: Option<Instant>,
}

impl Default for FavoriteCodeTracker {
    fn default() -> Self {
        Self {
            p1_index: 0,
            p1_last_input: None,
            p2_index: 0,
            p2_last_input: None,
        }
    }
}

impl FavoriteCodeTracker {
    /// Feed a pad direction press. Returns `Some(side)` when a code sequence
    /// completes, indicating which player's favorite should be toggled.
    pub fn check(&mut self, dir: PadDir, timestamp: Instant) -> Option<profile_data::PlayerSide> {
        let p1_result = Self::check_one(
            &CODE_P1,
            &mut self.p1_index,
            &mut self.p1_last_input,
            dir,
            timestamp,
        );
        let p2_result = Self::check_one(
            &CODE_P2,
            &mut self.p2_index,
            &mut self.p2_last_input,
            dir,
            timestamp,
        );
        if p1_result {
            Some(profile_data::PlayerSide::P1)
        } else if p2_result {
            Some(profile_data::PlayerSide::P2)
        } else {
            None
        }
    }

    fn check_one(
        code: &[PadDir; 5],
        index: &mut usize,
        last_input: &mut Option<Instant>,
        dir: PadDir,
        timestamp: Instant,
    ) -> bool {
        if let Some(last) = *last_input {
            if timestamp.duration_since(last) > CODE_TIMEOUT {
                *index = 0;
            }
        }

        if code[*index] == dir {
            *index += 1;
            *last_input = Some(timestamp);
            if *index >= code.len() {
                *index = 0;
                *last_input = None;
                return true;
            }
        } else if code[0] == dir {
            *index = 1;
            *last_input = Some(timestamp);
        } else {
            *index = 0;
            *last_input = None;
        }
        false
    }
}
