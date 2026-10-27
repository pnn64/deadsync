/// Order shared with the on-disk ITG, EX and Hard EX histories.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GhostScoreType {
    #[default]
    Itg,
    Ex,
    HardEx,
}

impl GhostScoreType {
    pub const fn index(self) -> usize {
        match self {
            Self::Itg => 0,
            Self::Ex => 1,
            Self::HardEx => 2,
        }
    }

    pub fn possible_points(self, itg: i32, steps: u32, holds: u32, rolls: u32) -> f64 {
        match self {
            Self::Itg => f64::from(itg),
            Self::Ex | Self::HardEx => f64::from(steps) * 3.5 + f64::from(holds) + f64::from(rolls),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GameplayGhostTarget {
    pub score_type: GhostScoreType,
    pub points: Vec<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct GameplayGhostState {
    pub recorded: [Vec<f64>; 3],
    target: GameplayGhostTarget,
    possible_points: f64,
    recording: bool,
}

impl GameplayGhostState {
    pub fn new(capacity: usize, mut target: GameplayGhostTarget, possible_points: f64) -> Self {
        if target.points.len() != capacity || target.points.iter().any(|p| !p.is_finite()) {
            target.points.clear();
        }
        Self {
            recorded: std::array::from_fn(|_| Vec::with_capacity(capacity)),
            target,
            possible_points,
            recording: capacity > 0,
        }
    }

    pub fn target_percent(&self) -> Option<f64> {
        (self.possible_points > 0.0)
            .then(|| {
                self.target
                    .points
                    .last()
                    .map(|p| p / self.possible_points * 100.0)
            })
            .flatten()
    }

    pub fn record(&mut self, points: [f64; 3]) {
        if !self.recording {
            return;
        }
        // All storage is reserved at song load. Unexpected extra events invalidate
        // the history instead of allocating on the gameplay thread.
        if self.recorded[0].len() == self.recorded[0].capacity() {
            self.recording = false;
            self.target.points.clear();
            for history in &mut self.recorded {
                history.clear();
            }
            return;
        }
        for (history, point) in self.recorded.iter_mut().zip(points) {
            history.push(point);
        }
    }

    pub fn difference_percent(&self) -> Option<f64> {
        if self.possible_points <= 0.0 {
            return None;
        }
        let history = &self.recorded[self.target.score_type.index()];
        let index = history.len().checked_sub(1)?;
        let target = self.target.points.get(index)?;
        Some((history[index] - target) / self.possible_points * 100.0)
    }
}

impl<Profile: GameplayProfileData, OverlayActor, CapturedActor, StateDelta>
    GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>
{
    fn record_ghost_judgment(&mut self, player: usize) {
        if self.autoplay_blocks_scoring() {
            return;
        }
        let data = self.stage_scored_ex_score_data(player);
        let points = [
            f64::from(self.players_runtime.players[player].earned_grade_points),
            judgment::ex_score_points(&data),
            judgment::hard_ex_score_points(&data),
        ];
        self.players_runtime.players[player].ghost.record(points);
    }

    pub fn ghost_difference_percent(&self, player: usize) -> Option<f64> {
        self.players_runtime
            .players
            .get(player)?
            .ghost
            .difference_percent()
    }

    pub fn ghost_history_for_save(&self, player: usize) -> Option<&[Vec<f64>; 3]> {
        if player >= self.num_players()
            || self.autoplay_used()
            || self.progress.replay.mode
            || !self.score_valid_for_player(player)
        {
            return None;
        }
        let outcome = self.individual_song_outcome(player);
        if outcome.is_failing || !outcome.song_completed_naturally {
            return None;
        }
        let totals = self.stage_totals_for_player(player);
        let expected = totals.total_steps as usize
            + totals.holds_total as usize
            + totals.rolls_total as usize
            + totals.mines_total as usize;
        let histories = &self.players_runtime.players[player].ghost.recorded;
        (expected > 0 && histories.iter().all(|h| h.len() == expected)).then_some(histories)
    }
}

#[cfg(test)]
mod ghost_tests {
    use super::*;

    #[test]
    fn follows_recorded_progress_instead_of_uniform_pace() {
        for score_type in [
            GhostScoreType::Itg,
            GhostScoreType::Ex,
            GhostScoreType::HardEx,
        ] {
            let mut state = GameplayGhostState::new(
                3,
                GameplayGhostTarget {
                    score_type,
                    points: vec![4.0, 10.0, 12.0],
                },
                15.0,
            );
            assert_eq!(state.target_percent(), Some(80.0));
            assert_eq!(state.difference_percent(), None);
            state.record([5.0; 3]);
            assert!((state.difference_percent().unwrap() - 100.0 / 15.0).abs() < 1e-10);
            state.record([9.0; 3]);
            assert!((state.difference_percent().unwrap() + 100.0 / 15.0).abs() < 1e-10);
            state.record([12.0; 3]);
            assert_eq!(state.difference_percent(), Some(0.0));
        }
    }

    #[test]
    fn missing_mismatched_or_invalid_ghost_falls_back() {
        for points in [vec![], vec![1.0], vec![f64::NAN, 2.0]] {
            let mut state = GameplayGhostState::new(
                2,
                GameplayGhostTarget {
                    points,
                    ..GameplayGhostTarget::default()
                },
                10.0,
            );
            state.record([5.0, 3.5, 3.5]);
            assert_eq!(state.difference_percent(), None);
            assert_eq!(state.target_percent(), None);
            assert_eq!(state.recorded[0], [5.0]);
        }
    }

    #[test]
    fn extra_judgments_invalidate_history_without_growing_buffers() {
        let mut state = GameplayGhostState::new(1, GameplayGhostTarget::default(), 5.0);
        let capacities = state.recorded.each_ref().map(Vec::capacity);
        state.record([5.0; 3]);
        state.record([10.0; 3]);
        assert!(state.recorded.iter().all(Vec::is_empty));
        assert_eq!(state.recorded.each_ref().map(Vec::capacity), capacities);
        state.record([15.0; 3]);
        assert!(state.recorded.iter().all(Vec::is_empty));
    }
}
