use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{GameplayProfileData, GameplayRuntimeState};
use deadsync_profile::{MiniIndicatorScoreType, player_side_index};
use deadsync_score::{
    CachedPlayerLeaderboardData, GameplayScoreboxProfileSnapshot, LeaderboardPane,
};

use crate::profile_side_from_gameplay;

/// Pending leaderboard requests retained with the stage that started them.
/// Completion clears a bit. The request profiles are released with the stage.
#[derive(Default)]
pub struct GameplayScoreRefresh {
    profiles: [GameplayScoreboxProfileSnapshot; MAX_PLAYERS],
    rival_score_types: [Option<MiniIndicatorScoreType>; MAX_PLAYERS],
    pending_sides: u8,
}

impl GameplayScoreRefresh {
    pub fn new(
        profiles: [GameplayScoreboxProfileSnapshot; MAX_PLAYERS],
        snapshots: &[Option<CachedPlayerLeaderboardData>; MAX_PLAYERS],
        rival_score_types: [Option<MiniIndicatorScoreType>; MAX_PLAYERS],
    ) -> Self {
        let mut pending_sides = 0;
        for (side, profile) in profiles.iter().enumerate() {
            let rival_score_type = rival_score_types[side];
            if (profile.display_scorebox || rival_score_type.is_some())
                && profile.online_active()
                && snapshots[side]
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.loading)
            {
                pending_sides |= 1 << side;
            }
        }
        Self {
            profiles,
            rival_score_types,
            pending_sides,
        }
    }

    #[inline(always)]
    pub const fn has_pending(&self) -> bool {
        self.pending_sides != 0
    }

    /// Apply finished rival scores to the simulation and return completed boards by side.
    pub fn poll<P: GameplayProfileData, O, C, D>(
        &mut self,
        gameplay: &mut GameplayRuntimeState<P, O, C, D>,
        mut fetch: impl FnMut(
            &str,
            &GameplayScoreboxProfileSnapshot,
        ) -> Option<CachedPlayerLeaderboardData>,
    ) -> [Option<CachedPlayerLeaderboardData>; MAX_PLAYERS] {
        let mut updates = [const { None }; MAX_PLAYERS];
        if !self.has_pending() {
            return updates;
        }
        for player in 0..gameplay.num_players() {
            let side = player_side_index(profile_side_from_gameplay(
                gameplay.runtime_player_side(player),
            ));
            let side_bit = 1 << side;
            if self.pending_sides & side_bit == 0 {
                continue;
            }
            let chart_hash = gameplay.charts()[player].short_hash.trim();
            if chart_hash.is_empty() {
                continue;
            }
            let Some(snapshot) =
                fetch(chart_hash, &self.profiles[side]).filter(|snapshot| !snapshot.loading)
            else {
                continue;
            };
            if let Some(score_type) = self.rival_score_types[side]
                && let Some(score) = mini_indicator_rival_score(Some(&snapshot), score_type)
            {
                gameplay.set_mini_indicator_rival_score_percent(player, score);
            }
            self.pending_sides &= !side_bit;
            updates[side] = Some(snapshot);
        }
        updates
    }
}

fn leaderboard_matches_score_type(
    pane: &LeaderboardPane,
    score_type: MiniIndicatorScoreType,
) -> bool {
    match score_type {
        MiniIndicatorScoreType::Itg => !pane.is_ex && !pane.is_hard_ex(),
        MiniIndicatorScoreType::Ex => pane.is_ex && !pane.is_hard_ex(),
        MiniIndicatorScoreType::HardEx => pane.is_hard_ex(),
    }
}

pub fn mini_indicator_rival_score(
    snapshot: Option<&CachedPlayerLeaderboardData>,
    score_type: MiniIndicatorScoreType,
) -> Option<f64> {
    let panes = snapshot?
        .data
        .as_ref()?
        .panes
        .iter()
        .filter(|pane| !pane.disabled && leaderboard_matches_score_type(pane, score_type));
    let mut rival = None::<f64>;
    let mut world_record = None::<f64>;
    for entry in panes
        .flat_map(|pane| pane.entries.iter())
        .filter(|entry| !entry.is_fail && entry.score.is_finite())
    {
        if entry.is_rival || entry.is_self {
            rival = Some(rival.map_or(entry.score, |score| score.max(entry.score)));
        }
        if entry.rank == 1 {
            world_record = Some(world_record.map_or(entry.score, |score| score.max(entry.score)));
        }
    }
    rival
        .or(world_record)
        .map(|score_10000| (score_10000 / 100.0).clamp(0.0, 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scorebox_polling_requires_a_loading_board_and_a_consumer() {
        let mut profiles = std::array::from_fn(|_| GameplayScoreboxProfileSnapshot::default());
        profiles[0].display_scorebox = true;
        profiles[0].gs_active = true;
        let mut snapshots = [Some(CachedPlayerLeaderboardData::loading()), None];
        let mut rival_score_types = [None; MAX_PLAYERS];
        let pending =
            |profiles: &[GameplayScoreboxProfileSnapshot; MAX_PLAYERS], snapshots: &_, types| {
                GameplayScoreRefresh::new((*profiles).clone(), snapshots, types).has_pending()
            };
        assert!(pending(&profiles, &snapshots, rival_score_types));
        snapshots[0] = Some(CachedPlayerLeaderboardData::error("offline".into()));
        assert!(!pending(&profiles, &snapshots, rival_score_types));

        snapshots[0] = Some(CachedPlayerLeaderboardData::loading());
        profiles[0].display_scorebox = false;
        assert!(!pending(&profiles, &snapshots, rival_score_types));
        rival_score_types[0] = Some(MiniIndicatorScoreType::Itg);
        assert!(pending(&profiles, &snapshots, rival_score_types));

        profiles[0].display_scorebox = true;
        profiles[0].gs_active = false;
        assert!(!pending(&profiles, &snapshots, rival_score_types));
        profiles[0].ac_active = true;
        assert!(pending(&profiles, &snapshots, rival_score_types));
    }
}
