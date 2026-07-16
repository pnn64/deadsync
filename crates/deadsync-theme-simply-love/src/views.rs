use deadlib_render::{ClockDomainTrace, PresentModeTrace};
use deadsync_assets::noteskin::Noteskin;
use deadsync_input::Keymap;
use deadsync_profile::{PlayMode, PlayStyle, PlayerSide};
use std::path::PathBuf;
use std::sync::Arc;

pub use deadsync_config::frame_pacing::VisibleStutterSample;
pub use deadsync_theme::views::{
    AudioTimingView, CourseGraphStageView, CourseStageView, DensityGraphView, EvaluationView,
    FrameStatsSample, FrameStatsSummary, OverlayAnchor, OverlayStyle, SelectedCourseView,
    TimingHealthView,
};

/// Concrete evaluation view used by the Simply Love screens.
pub type ScoreInfo = EvaluationView<Arc<Noteskin>, PlayerSide>;
pub type CourseGraphStage = CourseGraphStageView;
pub type CourseStagePlan = CourseStageView;
pub type SelectedCoursePlan = SelectedCourseView;
pub type DensityGraphSource = DensityGraphView;
pub type TimingHealth = TimingHealthView<PresentModeTrace, ClockDomainTrace, AudioTimingView>;

/// Shell-owned presentation policy used while constructing Gameplay state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameplayPolicyView {
    pub translated_titles: bool,
    pub random_background_movies: bool,
    pub center_single_notefield: bool,
}

impl Default for GameplayPolicyView {
    fn default() -> Self {
        let config = deadsync_config::prelude::Config::default();
        Self {
            translated_titles: config.translated_titles,
            random_background_movies: matches!(
                config.random_background_mode,
                deadsync_config::prelude::RandomBackgroundMode::RandomMovies
            ),
            center_single_notefield: config.center_1player_notefield,
        }
    }
}

/// Live shell-prepared session and lobby state consumed by Gameplay's screen
/// logic, separate from the deterministic gameplay simulation state.
#[derive(Clone, Debug)]
pub struct GameplayRuntimeView {
    pub policy: GameplayPolicyView,
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub joined: [bool; 2],
    pub lobby: SimplyLoveLobbyRuntimeView,
}

impl Default for GameplayRuntimeView {
    fn default() -> Self {
        Self {
            policy: GameplayPolicyView::default(),
            play_style: PlayStyle::default(),
            player_side: PlayerSide::default(),
            joined: [true, false],
            lobby: SimplyLoveLobbyRuntimeView::default(),
        }
    }
}

/// Shell-prepared song-lifetime HUD identity plus the initial live Gameplay
/// context.
#[derive(Clone, Debug)]
pub struct GameplayInitView {
    pub runtime: GameplayRuntimeView,
    pub hud: deadsync_profile::GameplayHudSnapshot,
}

impl Default for GameplayInitView {
    fn default() -> Self {
        Self {
            runtime: GameplayRuntimeView::default(),
            hud: deadsync_profile::GameplayHudSnapshot {
                play_style: PlayStyle::default(),
                player_side: PlayerSide::default(),
                p1: deadsync_profile::GameplayHudPlayerSnapshot {
                    joined: true,
                    guest: true,
                    ..Default::default()
                },
                p2: deadsync_profile::GameplayHudPlayerSnapshot::default(),
            },
        }
    }
}

/// Shell-owned machine/runtime policy consumed by Player Options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerOptionsPolicyView {
    pub allow_per_player_global_offsets: bool,
    pub heart_rate_monitors: bool,
    pub arcade_navigation: bool,
    pub smx_input: bool,
    pub smx_panel_lights: bool,
    pub scorebox_available: bool,
}

impl Default for PlayerOptionsPolicyView {
    fn default() -> Self {
        let config = deadsync_config::prelude::Config::default();
        Self {
            allow_per_player_global_offsets: config.machine_allow_per_player_global_offsets,
            heart_rate_monitors: config.machine_enable_heart_rate_monitors,
            arcade_navigation: config.arcade_options_navigation,
            smx_input: config.smx_input,
            smx_panel_lights: config.smx_panel_lights,
            scorebox_available: false,
        }
    }
}

/// Shell-prepared session and profile snapshot used for one Player Options
/// state. Option edits remain local to the screen and use the existing typed
/// persistence callbacks.
#[derive(Clone, Debug)]
pub struct PlayerOptionsInitView {
    pub policy: PlayerOptionsPolicyView,
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub joined: [bool; 2],
    pub music_rate: f32,
    pub profiles: [deadsync_profile::Profile; 2],
}

impl Default for PlayerOptionsInitView {
    fn default() -> Self {
        Self {
            policy: PlayerOptionsPolicyView::default(),
            play_style: PlayStyle::default(),
            player_side: PlayerSide::default(),
            joined: [true, false],
            music_rate: 1.0,
            profiles: std::array::from_fn(|_| deadsync_profile::Profile::default()),
        }
    }
}

/// Shell-prepared keymap state consumed by Simply Love's mappings screen.
#[derive(Clone, Debug)]
pub struct MappingsRuntimeView {
    pub keymap: Keymap,
    pub input_debounce_seconds: f32,
    pub dedicated_three_key_nav: bool,
}

impl Default for MappingsRuntimeView {
    fn default() -> Self {
        Self {
            keymap: deadsync_input::default_keymap(),
            input_debounce_seconds: 0.02,
            dedicated_three_key_nav: false,
        }
    }
}

/// Shell-prepared player data shared by the pre-song selection screens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectFlowPlayerView {
    pub joined: bool,
    pub guest: bool,
    pub display_name: String,
    pub avatar_texture_key: Option<String>,
}

/// Shell-prepared config and session state for Select Color, Style, and Mode.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectFlowRuntimeView {
    pub players: [SelectFlowPlayerView; 2],
    pub play_style: PlayStyle,
    pub play_mode: PlayMode,
    pub color_index: i32,
}

/// Shell-prepared profile and score data shared by the post-song screens.
#[derive(Clone, Debug, Default)]
pub struct PostSongPlayerView {
    pub joined: bool,
    pub guest: bool,
    pub display_name: String,
    pub player_initials: String,
    pub avatar_texture_key: Option<String>,
    pub calories_burned_today: f32,
    pub ignore_step_count_calories: bool,
    pub total_songs_played: u32,
}

#[derive(Clone, Debug, Default)]
pub struct PostSongRuntimeView {
    pub players: [PostSongPlayerView; 2],
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub machine_font: deadsync_config::prelude::MachineFont,
    pub translated_titles: bool,
    pub zmod_rating_box_text: bool,
    pub three_key_navigation: bool,
    pub machine_leaderboards:
        std::collections::HashMap<String, Vec<deadsync_score::LeaderboardEntry>>,
}

/// One player's shell-prepared local records and online eligibility state used
/// while constructing an Evaluation screen.
#[derive(Clone, Debug, Default)]
pub struct EvaluationInitPlayerView {
    pub machine_records: Vec<deadsync_score::LeaderboardEntry>,
    pub personal_records: Vec<deadsync_score::LeaderboardEntry>,
    pub groovestats: deadsync_score::GrooveStatsEvalState,
    pub itl: deadsync_score::ItlEvalState,
}

/// Shell-owned Evaluation policy copied into theme state at screen entry and
/// refreshed while the screen is active.
#[derive(Clone, Copy, Debug)]
pub struct EvaluationPolicyView {
    pub enable_groovestats: bool,
    pub enable_arrowcloud: bool,
    pub autosubmit_course_scores_individually: bool,
    pub submit_arrowcloud_fails: bool,
    pub smooth_histogram: bool,
    pub shade_scatterplot_judgments: bool,
    pub only_dedicated_menu_buttons: bool,
    pub three_key_navigation: bool,
    pub machine_nice_sound: bool,
    pub show_gameplay_timer: bool,
    pub translated_titles: bool,
    pub zmod_rating_box_text: bool,
    pub breakdown_style: deadsync_config::prelude::BreakdownStyle,
}

impl Default for EvaluationPolicyView {
    fn default() -> Self {
        let config = deadsync_config::prelude::Config::default();
        Self {
            enable_groovestats: config.enable_groovestats,
            enable_arrowcloud: config.enable_arrowcloud,
            autosubmit_course_scores_individually: config.autosubmit_course_scores_individually,
            submit_arrowcloud_fails: config.submit_arrowcloud_fails,
            smooth_histogram: config.smooth_histogram,
            shade_scatterplot_judgments: config.shade_scatterplot_judgments,
            only_dedicated_menu_buttons: config.only_dedicated_menu_buttons,
            three_key_navigation: config.three_key_navigation,
            machine_nice_sound: config.machine_nice_sound,
            show_gameplay_timer: config.show_select_music_gameplay_timer,
            translated_titles: config.translated_titles,
            zmod_rating_box_text: config.zmod_rating_box_text,
            breakdown_style: config.select_music_breakdown_style,
        }
    }
}

/// One Evaluation footer/online-availability player prepared by the shell.
#[derive(Clone, Debug, Default)]
pub struct EvaluationPlayerView {
    pub joined: bool,
    pub guest: bool,
    pub avatar_texture_key: Option<String>,
    pub display_name: String,
    pub groovestats_linked: bool,
    pub arrowcloud_linked: bool,
}

/// Session and machine context needed by Evaluation's pure screen logic.
#[derive(Clone, Debug)]
pub struct EvaluationContextView {
    pub policy: EvaluationPolicyView,
    pub play_style: deadsync_profile::PlayStyle,
    pub player_side: deadsync_profile::PlayerSide,
    pub players: [EvaluationPlayerView; 2],
}

impl Default for EvaluationContextView {
    fn default() -> Self {
        let mut players = std::array::from_fn(|_| EvaluationPlayerView::default());
        players[0].joined = true;
        players[0].guest = true;
        Self {
            policy: EvaluationPolicyView::default(),
            play_style: deadsync_profile::PlayStyle::default(),
            player_side: deadsync_profile::PlayerSide::default(),
            players,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct EvaluationInitView {
    pub players: [EvaluationInitPlayerView; 2],
    pub context: EvaluationContextView,
}

/// One normalized local score used by Simply Love's selected-chart scorebox.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScoreboxLocalView {
    pub score_10000: f64,
    pub failed: bool,
}

/// One normalized machine record used by Simply Love's selected-chart pane.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScoreboxMachineView {
    pub name: String,
    pub score_10000: f64,
    pub failed: bool,
}

/// Shell-prepared score and leaderboard data for one Select Music side.
#[derive(Clone, Debug, Default)]
pub struct ScoreboxSideView {
    pub joined: bool,
    pub chart_hash: Option<String>,
    pub groovestats_active: bool,
    pub show_ex_score: bool,
    pub display_name: String,
    pub groovestats_username: String,
    pub player_initials: String,
    pub local_itg: Option<ScoreboxLocalView>,
    pub local_ex: Option<ScoreboxLocalView>,
    pub local_hard_ex: Option<ScoreboxLocalView>,
    pub local_itl: Option<ScoreboxLocalView>,
    pub machine_itg: Option<ScoreboxMachineView>,
    pub leaderboards: Option<deadsync_score::CachedPlayerLeaderboardData>,
}

/// Selected player-slot charts whose scorebox data shell should prepare.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectMusicScoreboxRequest {
    pub chart_hashes: [Option<String>; 2],
    pub leaderboards_allowed: bool,
    pub max_entries: usize,
}

/// On-demand chart selection whose full leaderboard overlay shell should
/// prepare. Hidden overlays return no request and perform no leaderboard work.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectMusicLeaderboardRequest {
    pub chart_hashes: [Option<String>; 2],
    pub max_entries: usize,
}

/// Shell-prepared runtime data for one side of the Select Music leaderboard
/// overlay. Simply Love retains pane filtering, labels, cycling, and layout.
#[derive(Clone, Debug, Default)]
pub struct SelectMusicLeaderboardSideView {
    pub chart_hash: Option<String>,
    pub machine_entries: Vec<deadsync_score::LeaderboardEntry>,
    pub leaderboards: Option<deadsync_score::CachedPlayerLeaderboardData>,
}

#[derive(Clone, Debug, Default)]
pub struct SelectMusicLeaderboardView {
    pub sides: [SelectMusicLeaderboardSideView; 2],
}

pub const MUSIC_WHEEL_SLOT_COUNT: usize = 19;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MusicWheelRankSource {
    #[default]
    None,
    Chart,
    Overall,
}

/// One borrowed wheel entry whose runtime data shell should prepare. Requests
/// live only for the pre-compose handoff and do not clone song or chart data.
#[derive(Clone, Copy, Debug, Default)]
pub enum MusicWheelSlotRuntimeRequest<'a> {
    #[default]
    Empty,
    Pack {
        key: Option<&'a str>,
    },
    Song {
        song: &'a deadsync_chart::SongData,
        chart_hashes: [Option<&'a str>; 2],
        is_srpg_event: bool,
    },
}

/// Selected-chart fetch work required before preparing a music wheel view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MusicWheelSideRuntimeRequest<'a> {
    pub chart_hash: Option<&'a str>,
    pub fetch_itl_rank: bool,
    pub fetch_itl_score: bool,
    pub fetch_srpg_score: bool,
}

/// Borrowed pre-compose request emitted by a concrete Simply Love wheel.
#[derive(Clone, Copy, Debug, Default)]
pub struct MusicWheelRuntimeRequest<'a> {
    pub read_scores: bool,
    pub rank_source: MusicWheelRankSource,
    pub read_itl_scores: bool,
    pub sides: [MusicWheelSideRuntimeRequest<'a>; 2],
    pub slots: [MusicWheelSlotRuntimeRequest<'a>; MUSIC_WHEEL_SLOT_COUNT],
}

/// Shell-prepared runtime presentation data for one player side in one slot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicWheelSideRuntimeView {
    pub score: Option<deadsync_score::CachedScore>,
    pub itl_rank: Option<u32>,
    pub srpg_pass_rate_hundredths: Option<u32>,
    pub local_itl: Option<deadsync_score::CachedItlScore>,
    pub online_itl_ex_hundredths: Option<u32>,
    pub online_itl_points: Option<u32>,
    pub srpg_itl_ex_hundredths: Option<u32>,
    pub favorite: bool,
    pub locked: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicWheelSlotRuntimeView {
    pub sides: [MusicWheelSideRuntimeView; 2],
}

/// Fixed-size shell-prepared snapshot consumed by shared wheel composition.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicWheelRuntimeView {
    pub joined: [bool; 2],
    pub play_style: deadsync_profile::PlayStyle,
    pub slots: [MusicWheelSlotRuntimeView; MUSIC_WHEEL_SLOT_COUNT],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectCourseScoreRequest<'a> {
    pub course_hash: Option<&'a str>,
}

/// Shell-prepared local record data used by the Select Course score pane.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectCourseScoreView {
    pub mode_show_ex_score: bool,
    pub pane_show_ex_score: bool,
    pub player_initials: String,
    pub player_score_percent: Option<f64>,
    pub machine_initials: Option<String>,
    pub machine_score_percent: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectCourseRuntimeView {
    pub context: SelectCourseContextView,
    pub music_wheel: MusicWheelRuntimeView,
    pub score: SelectCourseScoreView,
}

/// Shell-owned policy used by Select Course input and presentation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectCoursePolicyView {
    pub show_random_courses: bool,
    pub show_most_played_courses: bool,
    pub music_wheel_switch_speed: u8,
    pub global_offset_seconds: f32,
    pub dedicated_three_key_nav: bool,
}

impl Default for SelectCoursePolicyView {
    fn default() -> Self {
        let config = deadsync_config::prelude::Config::default();
        Self {
            show_random_courses: config.show_random_courses,
            show_most_played_courses: config.show_most_played_courses,
            music_wheel_switch_speed: config.music_wheel_switch_speed,
            global_offset_seconds: config.global_offset_seconds,
            dedicated_three_key_nav: config.three_key_navigation
                && config.only_dedicated_menu_buttons,
        }
    }
}

/// Session and machine context consumed by Select Course's pure screen logic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectCourseContextView {
    pub policy: SelectCoursePolicyView,
    pub play_style: deadsync_profile::PlayStyle,
    pub player_side: deadsync_profile::PlayerSide,
    pub music_rate: f32,
}

impl Default for SelectCourseContextView {
    fn default() -> Self {
        Self {
            policy: SelectCoursePolicyView::default(),
            play_style: deadsync_profile::PlayStyle::default(),
            player_side: deadsync_profile::PlayerSide::default(),
            music_rate: 1.0,
        }
    }
}

/// Shell-prepared cache, policy, and profile state used while resolving
/// dynamic courses.
#[derive(Clone, Debug, Default)]
pub struct SelectCourseInitView {
    pub song_packs: Vec<deadsync_chart::SongPack>,
    pub courses: Vec<deadsync_simfile::runtime_cache::CourseData>,
    pub played_chart_counts: Vec<(String, u32)>,
    pub translated_titles: bool,
    pub last_course_path: Option<PathBuf>,
    pub last_course_difficulty: Option<String>,
    pub context: SelectCourseContextView,
}

/// Live session routing used throughout Select Music without reading the
/// process-global profile session from theme code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectMusicSessionView {
    pub play_style: deadsync_profile::PlayStyle,
    pub player_side: deadsync_profile::PlayerSide,
    pub joined: [bool; 2],
    pub guest: [bool; 2],
    pub music_rate: f32,
}

impl SelectMusicSessionView {
    #[inline(always)]
    pub const fn side_joined(self, side: deadsync_profile::PlayerSide) -> bool {
        self.joined[deadsync_profile::player_side_index(side)]
    }

    #[inline(always)]
    pub const fn side_guest(self, side: deadsync_profile::PlayerSide) -> bool {
        self.guest[deadsync_profile::player_side_index(side)]
    }
}

impl Default for SelectMusicSessionView {
    fn default() -> Self {
        Self {
            play_style: Default::default(),
            player_side: Default::default(),
            joined: [true, false],
            guest: [true, true],
            music_rate: 1.0,
        }
    }
}

/// Shell-prepared profile identity and pad routing used by Select Music.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectMusicProfileView {
    pub display_names: [String; 2],
    pub local_profile_ids: [Option<String>; 2],
    pub pad_profile_ids: [Option<String>; 2],
}

impl SelectMusicProfileView {
    #[inline(always)]
    pub fn display_name(&self, side: deadsync_profile::PlayerSide) -> &str {
        &self.display_names[deadsync_profile::player_side_index(side)]
    }

    #[inline(always)]
    pub fn local_profile_id(&self, side: deadsync_profile::PlayerSide) -> Option<&str> {
        self.local_profile_ids[deadsync_profile::player_side_index(side)].as_deref()
    }

    #[inline(always)]
    pub fn pad_profile_id(&self, pad: usize) -> Option<&str> {
        self.pad_profile_ids.get(pad).and_then(Option::as_deref)
    }
}

/// Last selection persisted by the active machine profile and prepared at
/// Select Music entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectMusicLastPlayedView {
    pub song_music_path: Option<String>,
    pub chart_hash: Option<String>,
    pub difficulty_index: usize,
}

impl Default for SelectMusicLastPlayedView {
    fn default() -> Self {
        Self {
            song_music_path: None,
            chart_hash: None,
            difficulty_index: 2,
        }
    }
}

/// Shell-prepared runtime data consumed by Simply Love's Select Music screen.
#[derive(Clone, Debug)]
pub struct SelectMusicRuntimeView {
    pub session: SelectMusicSessionView,
    pub profiles: SelectMusicProfileView,
    /// Refreshed only when the active local profile IDs change.
    pub favorites: Option<deadsync_profile::FavoriteSnapshot>,
    pub audio_playback: deadsync_theme::views::AudioPlaybackView,
    pub lobby: SimplyLoveLobbyRuntimeView,
    pub downloads: Vec<SelectMusicDownloadView>,
    pub srpg_shop: Arc<deadsync_online::srpg_shop::SrpgShopSnapshot>,
    /// Beat offset applied to the selection arrow bounce animation.
    pub arrow_bounce_offset: f32,
    pub policy: SelectMusicPolicyView,
    pub music_wheel: MusicWheelRuntimeView,
    pub scoreboxes: [ScoreboxSideView; 2],
    pub leaderboard: SelectMusicLeaderboardView,
    pub unlock_downloads_available: bool,
    pub ready_song_reload_dirs: Vec<PathBuf>,
    pub sync_graph_mode: deadsync_config::prelude::SyncGraphMode,
    pub sync_graph_orientation: deadsync_config::prelude::GraphOrientation,
    pub sync_confidence_percent: u8,
}

impl Default for SelectMusicRuntimeView {
    fn default() -> Self {
        Self {
            session: Default::default(),
            profiles: Default::default(),
            favorites: None,
            audio_playback: Default::default(),
            lobby: Default::default(),
            downloads: Vec::new(),
            srpg_shop: Default::default(),
            arrow_bounce_offset: 0.0,
            policy: Default::default(),
            music_wheel: Default::default(),
            scoreboxes: Default::default(),
            leaderboard: Default::default(),
            unlock_downloads_available: false,
            ready_song_reload_dirs: Vec::new(),
            sync_graph_mode: deadsync_config::prelude::SyncGraphMode::PostKernelFingerprint,
            sync_graph_orientation: deadsync_config::prelude::GraphOrientation::Vertical,
            sync_confidence_percent: 80,
        }
    }
}

/// One shell-prepared unlock download row rendered by Select Music.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectMusicDownloadView {
    pub name: String,
    pub current_bytes: u64,
    pub total_bytes: u64,
    pub complete: bool,
    pub error_message: Option<String>,
}

/// Shell-prepared online lobby state consumed by Simply Love lobby-aware screens.
#[derive(Clone, Debug, PartialEq)]
pub struct SimplyLoveLobbyRuntimeView {
    pub snapshot: std::sync::Arc<deadsync_online::lobbies::Snapshot>,
    pub reconnect_status_text: Option<String>,
    pub disconnect_hold_seconds: f32,
}

impl Default for SimplyLoveLobbyRuntimeView {
    fn default() -> Self {
        Self {
            snapshot: Default::default(),
            reconnect_status_text: None,
            disconnect_hold_seconds: 5.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SimplyLoveGrooveStatsService {
    #[default]
    GrooveStats,
    BoogieStats,
}

/// One player's shell-prepared score-submission state for Evaluation.
#[derive(Clone, Debug, Default)]
pub struct EvaluationSubmissionView {
    pub groovestats_status: Option<deadsync_score::GrooveStatsSubmitUiStatus>,
    pub arrowcloud_status: Option<deadsync_score::ArrowCloudSubmitUiStatus>,
    pub event_progress: Vec<deadsync_score::EventProgress>,
    pub record_banner: Option<deadsync_score::GrooveStatsSubmitRecordBanner>,
    pub groovestats_next_retry_secs: Option<u32>,
    pub arrowcloud_next_retry_secs: Option<u32>,
    pub groovestats_next_retry_is_auto: bool,
    pub arrowcloud_next_retry_is_auto: bool,
}

/// Shell-prepared runtime data consumed by Simply Love's Evaluation screen.
#[derive(Clone, Debug, Default)]
pub struct EvaluationRuntimeView {
    pub context: EvaluationContextView,
    pub lobby: SimplyLoveLobbyRuntimeView,
    pub groovestats_service: SimplyLoveGrooveStatsService,
    pub submissions: [EvaluationSubmissionView; 2],
    pub scoreboxes: [ScoreboxSideView; 2],
    pub favorites: [bool; 2],
}

/// One playlist file loaded by the shell for Simply Love's playlist wheel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectMusicPlaylistView {
    pub id: String,
    pub owner: Option<String>,
    pub name: String,
    pub text: String,
}

/// Shell-prepared play history for one Select Music player side.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectMusicHistorySideView {
    pub available: bool,
    pub played_chart_counts: Vec<(String, u32)>,
    pub recent_chart_hashes: Vec<String>,
    /// Merged local/online best scores sorted by chart hash.
    pub cached_scores: Vec<(String, deadsync_score::CachedScore)>,
}

/// Cache-derived play history used by Simply Love's popularity and recent
/// sorts. Ranking and wheel presentation remain theme-owned.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectMusicHistoryView {
    pub machine_played_chart_counts: Vec<(String, u32)>,
    pub machine_recent_chart_hashes: Vec<String>,
    pub sides: [SelectMusicHistorySideView; 2],
}

/// Filesystem- and runtime-derived data prepared when Select Music is entered.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectMusicInitView {
    pub songs_root: PathBuf,
    pub courses_root: PathBuf,
    pub playlists: Vec<SelectMusicPlaylistView>,
    pub history: SelectMusicHistoryView,
    pub policy: SelectMusicPolicyView,
    pub session: SelectMusicSessionView,
    pub profiles: SelectMusicProfileView,
    pub last_played: SelectMusicLastPlayedView,
    pub favorites: deadsync_profile::FavoriteSnapshot,
    pub known_packs: deadsync_profile::KnownPackSnapshot,
}

/// Shell-prepared song packs used by Simply Love's Options import/sync UI.
#[derive(Clone, Debug)]
pub struct OptionsSongPackView {
    pub group_name: String,
    pub display_name: String,
    pub songs: Vec<Arc<deadsync_chart::SongData>>,
}

/// Shell-prepared media policy consumed by Simply Love's Select Music screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectMusicMediaPolicyView {
    pub show_banners: bool,
    pub show_cdtitles: bool,
    pub show_folder_stats: bool,
    pub show_previews: bool,
    pub preview_loop: bool,
    pub preview_starts_immediately: bool,
    pub show_preview_marker: bool,
    pub replay_gain: bool,
    pub song_select_bg_mode: deadsync_config::prelude::SelectMusicSongSelectBgMode,
}

/// Shell-prepared score and tournament policy consumed by Simply Love's wheel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectMusicWheelPolicyView {
    pub show_grades: bool,
    pub show_lamps: bool,
    pub itl_rank_mode: deadsync_config::prelude::SelectMusicItlRankMode,
    pub itl_score_mode: deadsync_config::prelude::SelectMusicItlWheelMode,
}

impl Default for SelectMusicWheelPolicyView {
    fn default() -> Self {
        Self {
            show_grades: false,
            show_lamps: false,
            itl_rank_mode: deadsync_config::prelude::SelectMusicItlRankMode::None,
            itl_score_mode: deadsync_config::prelude::SelectMusicItlWheelMode::Score,
        }
    }
}

/// Shell-prepared wheel and menu interaction policy for Select Music.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectMusicInteractionPolicyView {
    pub wheel_switch_speed: u8,
    pub wheel_style: deadsync_config::prelude::SelectMusicWheelStyle,
    pub sort_by_series: bool,
    pub new_pack_mode: deadsync_config::prelude::NewPackMode,
    pub show_srpg_shop: bool,
    pub practice_shortcut: deadsync_input::KeyCode,
    pub song_search_shortcut: deadsync_input::KeyCode,
    pub reload_shortcut: deadsync_input::KeyCode,
    pub test_input_shortcut: deadsync_input::KeyCode,
}

impl Default for SelectMusicInteractionPolicyView {
    fn default() -> Self {
        Self {
            wheel_switch_speed: deadsync_config::prelude::DEFAULT_MUSIC_WHEEL_SWITCH_SPEED,
            wheel_style: deadsync_config::prelude::SelectMusicWheelStyle::Itg,
            sort_by_series: false,
            new_pack_mode: deadsync_config::prelude::NewPackMode::Disabled,
            show_srpg_shop: deadsync_config::prelude::DEFAULT_SHOW_SRPG_SHOP,
            practice_shortcut: deadsync_input::KeyCode::KeyP,
            song_search_shortcut: deadsync_input::KeyCode::KeyS,
            reload_shortcut: deadsync_input::KeyCode::KeyL,
            test_input_shortcut: deadsync_input::KeyCode::KeyT,
        }
    }
}

/// Shell-prepared visibility and layout policy for Select Music composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectMusicPresentationPolicyView {
    pub show_scorebox: bool,
    pub scorebox_cycle_enabled: bool,
    pub scorebox_in_step_pane: bool,
    pub show_stage_display: bool,
    pub show_gameplay_timer: bool,
    pub step_artist_expanded: bool,
    pub breakdown_style: deadsync_config::prelude::BreakdownStyle,
    pub pattern_info_mode: deadsync_config::prelude::SelectMusicPatternInfoMode,
    pub chart_info_peak_nps: bool,
    pub chart_info_effective_bpm: bool,
    pub chart_info_matrix_rating: bool,
    pub show_breakdown: bool,
    pub pack_ini_offsets: bool,
    pub default_sync_offset: deadsync_config::prelude::DefaultSyncOffset,
}

impl Default for SelectMusicPresentationPolicyView {
    fn default() -> Self {
        Self {
            show_scorebox: deadsync_config::prelude::DEFAULT_SHOW_SELECT_MUSIC_SCOREBOX,
            scorebox_cycle_enabled:
                deadsync_config::prelude::DEFAULT_SELECT_MUSIC_SCOREBOX_CYCLE_ITG
                    || deadsync_config::prelude::DEFAULT_SELECT_MUSIC_SCOREBOX_CYCLE_EX
                    || deadsync_config::prelude::DEFAULT_SELECT_MUSIC_SCOREBOX_CYCLE_HARD_EX
                    || deadsync_config::prelude::DEFAULT_SELECT_MUSIC_SCOREBOX_CYCLE_TOURNAMENTS,
            scorebox_in_step_pane: false,
            show_stage_display: deadsync_config::prelude::DEFAULT_SHOW_SELECT_MUSIC_STAGE_DISPLAY,
            show_gameplay_timer: deadsync_config::prelude::DEFAULT_SHOW_SELECT_MUSIC_GAMEPLAY_TIMER,
            step_artist_expanded: false,
            breakdown_style: deadsync_config::prelude::BreakdownStyle::Sl,
            pattern_info_mode: deadsync_config::prelude::SelectMusicPatternInfoMode::Tech,
            chart_info_peak_nps: deadsync_config::prelude::DEFAULT_SELECT_MUSIC_CHART_INFO_PEAK_NPS,
            chart_info_effective_bpm:
                deadsync_config::prelude::DEFAULT_SELECT_MUSIC_CHART_INFO_EFFECTIVE_BPM,
            chart_info_matrix_rating:
                deadsync_config::prelude::DEFAULT_SELECT_MUSIC_CHART_INFO_MATRIX_RATING,
            show_breakdown: deadsync_config::prelude::DEFAULT_SHOW_SELECT_MUSIC_BREAKDOWN,
            pack_ini_offsets: deadsync_config::prelude::DEFAULT_MACHINE_PACK_INI_OFFSETS,
            default_sync_offset: deadsync_config::prelude::DefaultSyncOffset::Null,
        }
    }
}

/// Runtime/config policy used to expose Select Music features and input paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectMusicPolicyView {
    pub dedicated_menu_only: bool,
    pub fsr_profiles: bool,
    pub replays: bool,
    pub profile_switch: bool,
    pub keyboard_features: bool,
    pub media: SelectMusicMediaPolicyView,
    pub wheel: SelectMusicWheelPolicyView,
    pub interaction: SelectMusicInteractionPolicyView,
    pub presentation: SelectMusicPresentationPolicyView,
}

/// Simply Love's two density-graph texture targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplyLoveDensityGraphSlot {
    SelectMusicP1,
    SelectMusicP2,
}

/// Simply Love compatibility name used inside its concrete screen modules.
pub(crate) type DensityGraphSlot = SimplyLoveDensityGraphSlot;

/// Runtime capabilities used to decide which concrete Simply Love options
/// rows are available on the current host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SimplyLoveUpdaterCapabilities {
    pub app_update: bool,
    pub ffmpeg_install: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuGrooveError {
    Disabled,
    MachineOffline,
    CannotConnect,
    TimedOut,
    InvalidResponse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuGrooveStatus {
    Pending {
        boogie: bool,
    },
    Error {
        boogie: bool,
        kind: MainMenuGrooveError,
    },
    Connected {
        boogie: bool,
        get_scores: bool,
        leaderboard: bool,
        auto_submit: bool,
    },
}

impl Default for MainMenuGrooveStatus {
    fn default() -> Self {
        Self::Pending { boogie: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuArrowCloudError {
    Disabled,
    TimedOut,
    HostBlocked,
    CannotConnect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MainMenuArrowCloudStatus {
    #[default]
    Pending,
    Connected,
    Error(MainMenuArrowCloudError),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MainMenuSmxConflictView {
    pub color_rgb: [f32; 3],
}

/// Shell-prepared runtime data consumed by Simply Love's concrete main menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MainMenuRuntimeView {
    pub allow_shutdown_host: bool,
    pub song_count: usize,
    pub pack_count: usize,
    pub course_count: usize,
    pub groovestats: MainMenuGrooveStatus,
    pub arrowcloud: MainMenuArrowCloudStatus,
    pub smx_conflict: Option<MainMenuSmxConflictView>,
}

/// Coarse updater failure category used by Simply Love's localized overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplyLoveUpdateErrorKind {
    Network,
    RateLimited,
    HttpStatus,
    Parse,
    NoAssetForHost,
    Checksum,
    Io,
}

/// Release metadata needed by Simply Love's updater presentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimplyLoveReleaseView {
    pub tag: String,
    pub html_url: String,
    pub published_at: Option<String>,
}

/// Release-asset metadata shown by Simply Love before a download begins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimplyLoveReleaseAssetView {
    pub size: u64,
    pub digest: Option<String>,
}

/// Prepared app-update state rendered by Simply Love.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimplyLoveUpdatePhase {
    Idle,
    Checking,
    ConfirmDownload {
        info: SimplyLoveReleaseView,
        asset: SimplyLoveReleaseAssetView,
    },
    UpToDate {
        tag: String,
    },
    RollbackChecking,
    RollbackPick {
        candidates: Vec<SimplyLoveReleaseView>,
        selected: usize,
    },
    RollbackEmpty,
    AvailableNoInstall {
        info: SimplyLoveReleaseView,
    },
    Downloading {
        info: SimplyLoveReleaseView,
        written: u64,
        total: Option<u64>,
        eta_secs: Option<u64>,
    },
    Ready {
        info: SimplyLoveReleaseView,
    },
    Applying {
        info: SimplyLoveReleaseView,
    },
    AppliedRestartRequired {
        info: SimplyLoveReleaseView,
        detail: String,
    },
    Error {
        kind: SimplyLoveUpdateErrorKind,
        detail: String,
    },
}

/// Prepared FFmpeg-install state rendered by Simply Love.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimplyLoveFfmpegPhase {
    Idle,
    Checking,
    Confirm {
        version: String,
        origin: String,
        total: Option<u64>,
        already_available: bool,
    },
    Downloading {
        version: String,
        written: u64,
        total: Option<u64>,
        eta_secs: Option<u64>,
        speed_bps: Option<u64>,
    },
    Extracting {
        version: String,
    },
    Installed {
        version: String,
    },
    Unsupported,
    AlreadyAvailable,
    Error {
        kind: SimplyLoveUpdateErrorKind,
        detail: String,
    },
}

/// One shell-prepared snapshot of the updater services used by Options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimplyLoveUpdaterView {
    pub update: SimplyLoveUpdatePhase,
    pub ffmpeg: SimplyLoveFfmpegPhase,
}

impl Default for SimplyLoveUpdaterView {
    fn default() -> Self {
        Self {
            update: SimplyLoveUpdatePhase::Idle,
            ffmpeg: SimplyLoveFfmpegPhase::Idle,
        }
    }
}

/// Number of bins in Simply Love's frame-interval histogram.
pub const HISTOGRAM_BINS: usize = 32;

/// Fill `out` with Simply Love's frame-interval histogram. The final bin
/// absorbs overflow.
pub fn frame_histogram(
    samples: &[FrameStatsSample],
    out: &mut [u32; HISTOGRAM_BINS],
    bin_width_us: u32,
) {
    *out = [0; HISTOGRAM_BINS];
    let bin_width_us = bin_width_us.max(1);
    for sample in samples {
        if sample.is_empty() {
            continue;
        }
        let idx = (sample.frame_us / bin_width_us).min(HISTOGRAM_BINS as u32 - 1) as usize;
        out[idx] = out[idx].saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_ignores_empty_samples_and_absorbs_overflow() {
        let samples = [
            FrameStatsSample::empty(),
            FrameStatsSample {
                host_nanos: 1,
                frame_us: 1_500,
                ..FrameStatsSample::empty()
            },
            FrameStatsSample {
                host_nanos: 2,
                frame_us: 100_000,
                ..FrameStatsSample::empty()
            },
        ];
        let mut bins = [0; HISTOGRAM_BINS];
        frame_histogram(&samples, &mut bins, 1_000);
        assert_eq!(bins[1], 1);
        assert_eq!(bins[HISTOGRAM_BINS - 1], 1);
    }
}
