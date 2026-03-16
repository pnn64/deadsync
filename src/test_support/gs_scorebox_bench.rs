use crate::game::profile;
use crate::game::scores::{
    CachedPlayerLeaderboardData, LeaderboardEntry, LeaderboardPane, PlayerLeaderboardData,
};
use crate::screens::components::shared::gs_scorebox;
use crate::ui::actors::Actor;

pub const SCENARIO_NAME: &str = "gs-scorebox";

pub struct GsScoreboxBenchFixture {
    snapshot: CachedPlayerLeaderboardData,
    side: profile::PlayerSide,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    elapsed_s: f32,
}

impl GsScoreboxBenchFixture {
    pub fn build(&self) -> Vec<Actor> {
        gs_scorebox::gameplay_scorebox_actors_from_cached_snapshot(
            self.side,
            &self.snapshot,
            self.center_x,
            self.center_y,
            self.zoom,
            self.elapsed_s,
        )
    }
}

pub fn fixture() -> GsScoreboxBenchFixture {
    GsScoreboxBenchFixture {
        snapshot: CachedPlayerLeaderboardData {
            loading: false,
            error: None,
            data: Some(PlayerLeaderboardData {
                panes: vec![
                    leaderboard_pane("GrooveStats", false, scores_itg()),
                    leaderboard_pane("GrooveStats", true, scores_ex()),
                    leaderboard_pane("ArrowCloud", false, scores_hard_ex()),
                    leaderboard_pane("Stamina RPG 9", false, scores_rpg()),
                    leaderboard_pane("ITL Online 2024", false, scores_itl()),
                ],
            }),
        },
        side: profile::PlayerSide::P1,
        center_x: 704.0,
        center_y: 108.0,
        zoom: 1.0,
        elapsed_s: 17.75,
    }
}

fn leaderboard_pane(name: &str, is_ex: bool, entries: Vec<LeaderboardEntry>) -> LeaderboardPane {
    LeaderboardPane {
        name: name.to_string(),
        entries,
        is_ex,
        disabled: false,
    }
}

fn entry(rank: u32, name: &str, score: f64, is_rival: bool, is_self: bool) -> LeaderboardEntry {
    LeaderboardEntry {
        rank,
        name: name.to_string(),
        machine_tag: Some(name.to_string()),
        score,
        date: "2026-03-12".to_string(),
        is_rival,
        is_self,
        is_fail: false,
    }
}

fn scores_itg() -> Vec<LeaderboardEntry> {
    vec![
        entry(1, "WOLF", 9998.44, false, false),
        entry(2, "YOU", 9993.12, false, true),
        entry(3, "RIV1", 9987.01, true, false),
        entry(4, "RIV2", 9984.77, true, false),
        entry(5, "RIV3", 9979.34, true, false),
    ]
}

fn scores_ex() -> Vec<LeaderboardEntry> {
    vec![
        entry(1, "WOLF", 99.84, false, false),
        entry(2, "YOU", 99.31, false, true),
        entry(3, "RIV1", 98.70, true, false),
        entry(4, "RIV2", 98.47, true, false),
        entry(5, "RIV3", 97.93, true, false),
    ]
}

fn scores_hard_ex() -> Vec<LeaderboardEntry> {
    vec![
        entry(1, "WOLF", 99.12, false, false),
        entry(2, "RIV1", 98.55, true, false),
        entry(4, "RIV2", 97.98, true, false),
        entry(7, "YOU", 97.54, false, true),
        entry(8, "ALT1", 97.12, false, false),
        entry(11, "RIV3", 96.87, true, false),
        entry(15, "ALT2", 96.51, false, false),
    ]
}

fn scores_rpg() -> Vec<LeaderboardEntry> {
    vec![
        entry(1, "WOLF", 99.77, false, false),
        entry(2, "YOU", 99.21, false, true),
        entry(3, "RIV1", 98.98, true, false),
        entry(4, "RIV2", 98.43, true, false),
        entry(5, "RIV3", 98.31, true, false),
    ]
}

fn scores_itl() -> Vec<LeaderboardEntry> {
    vec![
        entry(1, "WOLF", 99.65, false, false),
        entry(2, "YOU", 99.03, false, true),
        entry(3, "RIV1", 98.72, true, false),
        entry(4, "RIV2", 98.18, true, false),
        entry(5, "RIV3", 97.84, true, false),
    ]
}
