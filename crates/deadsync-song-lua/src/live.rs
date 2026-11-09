use crate::LUA_PLAYERS;

pub(crate) const TAP_SCORES: [&str; 11] = [
    "None",
    "HitMine",
    "AvoidMine",
    "CheckpointMiss",
    "Miss",
    "W5",
    "W4",
    "W3",
    "W2",
    "W1",
    "CheckpointHit",
];

#[derive(Clone, Copy, Debug, Default)]
pub struct SongLuaLivePlayer {
    pub possible_points: i32,
    pub actual_points: i32,
    pub current_possible_points: i32,
    pub life: f32,
    /// PlayerStageStats history is separate from the current LifeMeter value.
    pub life_record_latest: Option<f32>,
    pub failed: bool,
    pub combo: u32,
    pub miss_combo: u32,
    /// Native TapNoteScore order: None, HitMine, AvoidMine, CheckpointMiss,
    /// Miss, W5, W4, W3, W2, W1, CheckpointHit.
    pub tap_scores: [u32; 11],
    /// Actual gameplay feedback survives the native autoplay scoring bypass.
    pub last_judgment: Option<deadsync_gameplay::JudgmentRenderInfo>,
    pub hold_judgments:
        [Option<deadsync_gameplay::HoldJudgmentRenderInfo>; deadsync_core::input::MAX_COLS],
    pub autoplay: bool,
    /// Native ScreenGameplay suppresses Dead and Danger when FailType is Off.
    pub fail_enabled: bool,
}

#[cfg(feature = "test-support")]
#[derive(Clone, Debug)]
pub struct SongLuaOverlayStageObservation {
    pub beat: f32,
    pub music_seconds: f32,
    pub players: [SongLuaLivePlayer; LUA_PLAYERS],
    pub player_step_indices: [Option<usize>; LUA_PLAYERS],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SongLuaLivePlayerWrite {
    SetLife { player: usize, life: f32 },
    RecordLife { player: usize, life: f32 },
    Fail { player: usize },
}

pub(crate) fn apply_player_writes<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
    writes: impl IntoIterator<Item = SongLuaLivePlayerWrite>,
    game: &mut deadsync_gameplay::GameplayRuntimeState<P, O, C, D>,
) {
    for write in writes {
        match write {
            SongLuaLivePlayerWrite::SetLife { player, life } => {
                game.set_player_life_from_script(player, life)
            }
            SongLuaLivePlayerWrite::RecordLife { player, life } => {
                game.record_player_life_from_script(player, life)
            }
            SongLuaLivePlayerWrite::Fail { player } => {
                game.mark_player_stage_failed_from_script(player)
            }
        }
    }
}

pub(crate) fn judgment_score(judgment: deadsync_rules::judgment::Judgment) -> &'static str {
    use deadsync_rules::judgment::JudgeGrade::*;
    match judgment.grade {
        Fantastic => "TapNoteScore_W1",
        Excellent => "TapNoteScore_W2",
        Great => "TapNoteScore_W3",
        Decent => "TapNoteScore_W4",
        WayOff => "TapNoteScore_W5",
        Miss => "TapNoteScore_Miss",
    }
}

#[derive(Clone, Debug, Default)]
pub struct SongLuaLiveFrame {
    pub beat: f64,
    pub music_seconds: f64,
    pub screen_seconds: f64,
    pub delta_seconds: f64,
    /// Independent process clock; music can remain frozen while this advances.
    pub uptime_seconds: Option<f64>,
    /// The gameplay core reached its outro and removed active attacks.
    pub gameplay_outro: bool,
    /// The actual gameplay-owned attack schedule, independent of Lua writes.
    pub chart_attacks:
        Option<[std::sync::Arc<[deadsync_gameplay::ChartAttackWindow]>; LUA_PLAYERS]>,
    pub players: [SongLuaLivePlayer; LUA_PLAYERS],
}

impl SongLuaLivePlayer {
    pub fn from_player(player: &deadsync_gameplay::PlayerRuntime, possible_points: i32) -> Self {
        let [w1, w2, w3, w4, w5, miss] = player.judgment_counts;
        Self {
            possible_points,
            actual_points: player.earned_grade_points,
            current_possible_points: player.current_possible_points,
            life: player.life,
            life_record_latest: player.life_history.last().map(|&(_, life)| life),
            failed: player.stage_failed || player.fail_time.is_some(),
            fail_enabled: player.fail_type != deadsync_gameplay::GameplayFailType::Off,
            combo: player.combo,
            miss_combo: player.miss_combo,
            tap_scores: [
                0,
                player.mines_hit,
                player.mines_avoided,
                player.checkpoints_missed,
                miss,
                w5,
                w4,
                w3,
                w2,
                w1,
                player.checkpoints_hit,
            ],
            last_judgment: player.last_judgment,
            hold_judgments: [None; deadsync_core::input::MAX_COLS],
            autoplay: false,
        }
    }
}

impl SongLuaLiveFrame {
    /// Gameplay stores a sole cabinet P2 in runtime slot zero, as does the
    /// preparation context's Lua P1. No course carry or frozen display counts
    /// are substituted for the current stage's actual progress.
    pub fn from_gameplay<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
        game: &deadsync_gameplay::GameplayRuntimeState<P, O, C, D>,
        delta_seconds: f32,
    ) -> Self {
        Self {
            beat: f64::from(game.clock.song_position.current_beat),
            music_seconds: game.clock.song_position.current_music_time_ns as f64 / 1_000_000_000.0,
            screen_seconds: f64::from(game.boundary.total_elapsed_in_screen),
            delta_seconds: f64::from(delta_seconds),
            uptime_seconds: None,
            gameplay_outro: game.mods.attacks.cleared_for_outro,
            chart_attacks: Some(game.mods.attacks.option_windows.clone()),
            players: std::array::from_fn(|player| {
                game.stage_itg_score_inputs(player).map_or_else(
                    SongLuaLivePlayer::default,
                    |score| {
                        let mut value = SongLuaLivePlayer::from_player(
                            &game.players_runtime.players[player],
                            score.possible_grade_points,
                        );
                        value.autoplay = game.live_autoplay_enabled();
                        let columns = game.cols_per_player();
                        for (column, render) in
                            value.hold_judgments.iter_mut().enumerate().take(columns)
                        {
                            *render = game.hold_judgment(player * columns + column);
                        }
                        value
                    },
                )
            }),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SongLuaLiveRequest {
    NewScreen(String),
    Transition(String),
}

#[derive(Debug)]
pub struct SongLuaLiveSourceRequest {
    pub music_seconds: f64,
    pub request: SongLuaLiveRequest,
}

#[derive(Debug, Clone, Copy)]
pub enum SongLuaLiveTarget {
    Root(usize),
    Background(usize, usize),
    Foreground(usize, usize),
}

#[derive(Debug)]
pub struct SongLuaLiveBroadcast {
    pub beat: f32,
    pub music_seconds: f64,
    pub name: String,
    /// Snapshot before listeners run. Unsupported trace values remain an
    /// explicit diagnostic gap; they do not interrupt message delivery.
    pub params: Result<Option<serde_json::Value>, String>,
}

#[derive(Debug, Default)]
pub struct SongLuaLiveOutput {
    /// Ordered actor-owned audio operations emitted by authored Lua callbacks.
    pub actor_sounds: Vec<SongLuaActorSound>,
    pub request: Option<SongLuaLiveRequest>,
    /// Authored calls retain their multiplicity while the screen manager
    /// consumes only the final queued screen target.
    pub source_requests: Vec<SongLuaLiveSourceRequest>,
    pub broadcasts: Vec<(f32, String, bool)>,
    /// Explicit Lua MESSAGEMAN calls, including calls from engine-message
    /// listeners. Native source traces omit the engine-generated messages.
    pub source_broadcasts: Vec<SongLuaLiveBroadcast>,
    pub sounds: Vec<crate::SongLuaSoundEvent>,
    /// Native-style callback errors retain partial state and are reported once.
    pub errors: Vec<String>,
    /// Missing observational inputs, separate from callback execution errors.
    pub coverage_gaps: Vec<String>,
    pub player_writes: Vec<SongLuaLivePlayerWrite>,
    /// Frozen command/update Draw submissions, in their execution order.
    pub draw_frames: Vec<crate::SongLuaDrawFrame>,
}

impl SongLuaLiveOutput {
    pub fn append(&mut self, mut next: Self) {
        self.actor_sounds.append(&mut next.actor_sounds);
        if next.request.is_some() {
            self.request = next.request;
        }
        self.source_requests.append(&mut next.source_requests);
        self.broadcasts.append(&mut next.broadcasts);
        self.source_broadcasts.append(&mut next.source_broadcasts);
        self.sounds.append(&mut next.sounds);
        self.errors.append(&mut next.errors);
        self.coverage_gaps.append(&mut next.coverage_gaps);
        self.player_writes.append(&mut next.player_writes);
        self.draw_frames.append(&mut next.draw_frames);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SongLuaActorSound {
    Play {
        owner: u64,
        path: std::path::PathBuf,
    },
    Stop {
        owner: u64,
    },
    Pause {
        owner: u64,
        paused: bool,
    },
}
