use deadlib_render_core::{ClockDomainTrace, PresentModeTrace};
use deadsync_assets::noteskin::Noteskin;
use deadsync_input::Keymap;
use deadsync_profile::{PlayMode, PlayStyle, PlayerSide};
use std::path::PathBuf;
use std::sync::Arc;

use deadsync_theme::views::{
    AudioTimingView, CourseGraphStageView, CourseStageView, DensityGraphView, EvaluationView,
    FrameStatsSample, ScoreboxSideView, SelectedCourseView, TimingHealthView,
};

/// Concrete evaluation view used by the Simply Love screens.
pub type ScoreInfo = EvaluationView<Arc<Noteskin>, PlayerSide>;
pub type CourseGraphStage = CourseGraphStageView;
pub type CourseStagePlan = CourseStageView;
pub type SelectedCoursePlan = SelectedCourseView;
pub type DensityGraphSource = DensityGraphView;
pub type TimingHealth = TimingHealthView<PresentModeTrace, ClockDomainTrace, AudioTimingView>;

/// Borrowed post-stage results selected by shell-owned display indices.
#[derive(Clone, Copy, Debug)]
pub struct PostSelectStageView<'a> {
    stages: &'a [deadsync_score::stage_stats::StageSummary],
    indices: &'a [usize],
}

impl<'a> PostSelectStageView<'a> {
    #[must_use]
    pub const fn new(
        stages: &'a [deadsync_score::stage_stats::StageSummary],
        indices: &'a [usize],
    ) -> Self {
        Self { stages, indices }
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.indices.len()
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.indices.is_empty()
    }

    #[must_use]
    pub fn get(self, index: usize) -> Option<&'a deadsync_score::stage_stats::StageSummary> {
        self.indices
            .get(index)
            .and_then(|&stage_index| self.stages.get(stage_index))
    }

    pub fn iter(self) -> impl Iterator<Item = &'a deadsync_score::stage_stats::StageSummary> + 'a {
        self.indices
            .iter()
            .filter_map(move |&index| self.stages.get(index))
    }
}

/// Shell-prepared visual policy shared by screen backgrounds and screen bars.
#[derive(Clone, Copy, Debug)]
pub struct SimplyLoveVisualPolicyView {
    pub background: VisualBackgroundView,
    pub assets: &'static crate::visual_styles::Assets,
    pub machine_font: deadsync_config::theme::MachineFont,
    pub title_logo_texture_key: Option<&'static str>,
    pub srpg10_tint: bool,
    pub screen_bar: ScreenBarBackgroundView,
}

impl PartialEq for SimplyLoveVisualPolicyView {
    /// Same fields and order as a derived comparison. Style assets come from a
    /// static table, so frame caches usually match by pointer before comparing
    /// every asset path.
    fn eq(&self, other: &Self) -> bool {
        self.background == other.background
            && (std::ptr::eq(self.assets, other.assets) || self.assets == other.assets)
            && self.machine_font == other.machine_font
            && self.title_logo_texture_key == other.title_logo_texture_key
            && self.srpg10_tint == other.srpg10_tint
            && self.screen_bar == other.screen_bar
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VisualBackgroundView {
    #[default]
    Tiled,
    Technique,
    Srpg,
    Ecs15,
}

impl SimplyLoveVisualPolicyView {
    pub fn from_config(config: &deadsync_config::app_config::Config) -> Self {
        use crate::{color, visual_styles};

        let srpg10 = config.visual_style.is_srpg()
            && matches!(config.srpg_variant, deadsync_config::theme::SrpgVariant::Srpg10);
        let ecs15 = visual_styles::ecs15_active(config.visual_style, config.srpg_variant);
        let background = match config.visual_style {
            _ if ecs15 => VisualBackgroundView::Ecs15,
            deadsync_config::theme::VisualStyle::Technique => VisualBackgroundView::Technique,
            deadsync_config::theme::VisualStyle::Srpg9 => VisualBackgroundView::Srpg,
            _ => VisualBackgroundView::Tiled,
        };
        let screen_bar = match config
            .machine_bar_color
            .resolve(config.visual_style.presentation_style(config.srpg_variant))
        {
            deadsync_config::theme::MachineBarColor::Default => ScreenBarBackgroundView::Default,
            deadsync_config::theme::MachineBarColor::Colored => ScreenBarBackgroundView::Colored(if srpg10 {
                color::srpg10_rgba(config.theme_color)
            } else if ecs15 {
                color::decorative_rgba(config.theme_color)
            } else {
                color::srpg9_rgba(config.theme_color)
            }),
            deadsync_config::theme::MachineBarColor::Transparent => ScreenBarBackgroundView::Transparent,
        };

        Self {
            background,
            assets: visual_styles::for_style_and_variant(config.visual_style, config.srpg_variant),
            machine_font: config.machine_font,
            title_logo_texture_key: if ecs15 {
                Some(visual_styles::ECS15_TITLE_LOGO)
            } else {
                srpg10.then_some(visual_styles::SRPG10_TITLE_LOGO)
            },
            srpg10_tint: srpg10,
            screen_bar,
        }
    }

    /// The selected style's shared background movie, if video is enabled.
    #[must_use]
    pub fn background_video(self, enabled: bool) -> Option<&'static str> {
        self.assets.shared_background_video.filter(|_| enabled)
    }

    /// ECS15 replaces menu backdrops but retains Technique on music selection.
    #[must_use]
    pub fn music_selection_background(self) -> Self {
        if self.background == VisualBackgroundView::Ecs15 {
            Self {
                background: VisualBackgroundView::Technique,
                assets: crate::visual_styles::for_style(
                    deadsync_config::theme::VisualStyle::Technique,
                ),
                ..self
            }
        } else {
            self
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ScreenBarBackgroundView {
    #[default]
    Default,
    Colored([f32; 4]),
    Transparent,
}

impl Default for SimplyLoveVisualPolicyView {
    fn default() -> Self {
        Self {
            background: VisualBackgroundView::Tiled,
            assets: crate::visual_styles::for_style(deadsync_config::theme::VisualStyle::Hearts),
            machine_font: deadsync_config::theme::MachineFont::default(),
            title_logo_texture_key: None,
            srpg10_tint: false,
            screen_bar: ScreenBarBackgroundView::Default,
        }
    }
}

/// Simply Love presentation preferences used while constructing Gameplay state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayPolicyView {
    pub translated_titles: bool,
    pub center_single_notefield: bool,
    pub background_brightness: f32,
    pub background_color: deadlib_present::color::Color,
    pub smx_input: bool,
    pub zmod_rating_box_text: bool,
    pub difficulty_color_scheme: deadsync_theme::color::DifficultyColorScheme,
    pub show_bpm_decimal: bool,
    pub bpm_position: deadsync_config::theme::GameplayBpmPosition,
    pub machine_font: deadsync_config::theme::MachineFont,
    pub scorebox_pane_filter: deadsync_score::SelectMusicScoreboxFilter,
}

impl Default for GameplayPolicyView {
    fn default() -> Self {
        Self::from_config(&deadsync_config::app_config::Config::default(), Default::default())
    }
}

impl GameplayPolicyView {
    pub fn from_config(
        config: &deadsync_config::app_config::Config,
        scorebox_pane_filter: deadsync_score::SelectMusicScoreboxFilter,
    ) -> Self {
        Self {
            translated_titles: config.translated_titles,
            center_single_notefield: config.center_1player_notefield,
            background_brightness: config.bg_brightness,
            background_color: config.gameplay_bg_color,
            smx_input: config.smx_input,
            zmod_rating_box_text: config.zmod_rating_box_text,
            difficulty_color_scheme: config.difficulty_color_scheme,
            show_bpm_decimal: config.show_bpm_decimal,
            bpm_position: config.gameplay_bpm_position,
            machine_font: config.machine_font,
            scorebox_pane_filter,
        }
    }
}

pub use deadsync_profile_gameplay::stage_scores::GameplayScoreSnapshots as GameplayScoreInitView;

/// Shell-prepared presentation policy and local HUD identity.
#[derive(Clone, Debug)]
pub struct GameplayRuntimeView {
    pub policy: GameplayPolicyView,
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub joined: [bool; 2],
}

impl Default for GameplayRuntimeView {
    fn default() -> Self {
        Self {
            policy: GameplayPolicyView::default(),
            play_style: PlayStyle::default(),
            player_side: PlayerSide::default(),
            joined: [true, false],
        }
    }
}

/// Shell-prepared song-lifetime HUD identity plus the initial live Gameplay
/// context.
#[derive(Clone, Debug)]
pub struct GameplayInitView {
    pub runtime: GameplayRuntimeView,
    pub hud: deadsync_profile::GameplayHudSnapshot,
    pub judgment_palettes: [deadsync_theme::color::JudgmentPalette; 2],
    pub scores: GameplayScoreInitView,
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
            judgment_palettes: [crate::color::SIMPLY_LOVE_JUDGMENT_PALETTE; 2],
            scores: GameplayScoreInitView::default(),
        }
    }
}

/// Shell-prepared runtime policy and HUD routing consumed by Practice.
#[derive(Clone, Debug)]
pub struct PracticeRuntimeView {
    pub only_dedicated_menu_buttons: bool,
    pub three_key_navigation: bool,
    pub tab_acceleration: bool,
    pub hud: deadsync_profile::GameplayHudSnapshot,
}

impl Default for PracticeRuntimeView {
    fn default() -> Self {
        Self {
            only_dedicated_menu_buttons: false,
            three_key_navigation: false,
            tab_acceleration: true,
            hud: GameplayInitView::default().hud,
        }
    }
}

pub use deadsync_profile_gameplay::stage_options::{
    JudgmentPaletteChoiceView, PlayerOptionsPlayerView, PlayerOptionsPolicyView,
};

/// Standalone Simply Love initializer with the theme's legacy defaults.
#[derive(Clone, Debug)]
pub struct PlayerOptionsInitView {
    pub policy: PlayerOptionsPolicyView,
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub joined: [bool; 2],
    pub music_rate: f32,
    pub players: [PlayerOptionsPlayerView; 2],
    pub judgment_palettes: Vec<JudgmentPaletteChoiceView>,
}

impl Default for PlayerOptionsInitView {
    fn default() -> Self {
        Self {
            policy: PlayerOptionsPolicyView::default(),
            play_style: PlayStyle::default(),
            player_side: PlayerSide::default(),
            joined: [true, false],
            music_rate: 1.0,
            players: std::array::from_fn(|_| PlayerOptionsPlayerView::default()),
            judgment_palettes: vec![JudgmentPaletteChoiceView {
                id: crate::color::JUDGMENT_PRESET.id.to_owned(),
                name: crate::color::JUDGMENT_PRESET.name.to_owned(),
                palette: crate::color::SIMPLY_LOVE_JUDGMENT_PALETTE,
            }],
        }
    }
}

impl From<PlayerOptionsInitView> for deadsync_profile_gameplay::stage_options::PlayerOptionsView {
    fn from(view: PlayerOptionsInitView) -> Self {
        Self {
            policy: view.policy,
            play_style: view.play_style,
            player_side: view.player_side,
            joined: view.joined,
            music_rate: view.music_rate,
            players: view.players,
            judgment_palettes: view.judgment_palettes,
        }
    }
}

/// Shell-prepared keymap state consumed by Simply Love's mappings screen.
#[derive(Clone, Debug)]
pub struct MappingsRuntimeView {
    pub keymap: Keymap,
    pub game: deadsync_config::theme::GameFlag,
    pub machine_font: deadsync_config::theme::MachineFont,
    pub input_debounce_seconds: f32,
    pub dedicated_three_key_nav: bool,
}

impl Default for MappingsRuntimeView {
    fn default() -> Self {
        Self {
            keymap: deadsync_input::default_keymap(),
            game: deadsync_config::theme::GameFlag::default(),
            machine_font: deadsync_config::theme::MachineFont::default(),
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
    pub primary_side: usize,
    pub players: [SelectFlowPlayerView; 2],
    pub game: deadsync_config::theme::GameFlag,
    pub play_style: PlayStyle,
    pub play_mode: PlayMode,
    pub premium_free_available: bool,
    pub color_index: i32,
}

/// Shell-prepared profile and score data shared by the post-song screens.
#[derive(Clone, Debug)]
pub struct PostSongPlayerView {
    pub joined: bool,
    pub guest: bool,
    pub name_entry_eligible: bool,
    pub display_name: String,
    pub player_initials: String,
    pub avatar_texture_key: Option<String>,
    pub calories_burned_today: f32,
    pub ignore_step_count_calories: bool,
    pub total_songs_played: u32,
    pub judgment_palette: deadsync_theme::color::JudgmentPalette,
}

impl Default for PostSongPlayerView {
    fn default() -> Self {
        Self {
            joined: false,
            guest: false,
            name_entry_eligible: false,
            display_name: String::new(),
            player_initials: String::new(),
            avatar_texture_key: None,
            calories_burned_today: 0.0,
            ignore_step_count_calories: false,
            total_songs_played: 0,
            judgment_palette: crate::color::SIMPLY_LOVE_JUDGMENT_PALETTE,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PostSongRuntimeView {
    pub players: [PostSongPlayerView; 2],
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub machine_font: deadsync_config::theme::MachineFont,
    pub translated_titles: bool,
    pub zmod_rating_box_text: bool,
    pub difficulty_color_scheme: deadsync_theme::color::DifficultyColorScheme,
    pub three_key_navigation: bool,
    pub srpg10_visuals: bool,
    pub machine_leaderboards:
        std::collections::HashMap<String, Vec<deadsync_score::LeaderboardEntry>>,
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
    pub machine_easter_eggs: bool,
    pub machine_nice_sound: bool,
    pub show_gameplay_timer: bool,
    pub translated_titles: bool,
    pub transparent_panels: bool,
    pub srpg10_visuals: bool,
    pub machine_font: deadsync_config::theme::MachineFont,
    pub zmod_rating_box_text: bool,
    pub difficulty_color_scheme: deadsync_theme::color::DifficultyColorScheme,
    pub breakdown_style: deadsync_config::theme::BreakdownStyle,
}

impl Default for EvaluationPolicyView {
    fn default() -> Self {
        let config = deadsync_config::app_config::Config::default();
        Self {
            enable_groovestats: config.enable_groovestats,
            enable_arrowcloud: config.enable_arrowcloud,
            autosubmit_course_scores_individually: config.autosubmit_course_scores_individually,
            submit_arrowcloud_fails: config.submit_arrowcloud_fails,
            smooth_histogram: config.smooth_histogram,
            shade_scatterplot_judgments: config.shade_scatterplot_judgments,
            only_dedicated_menu_buttons: config.only_dedicated_menu_buttons,
            three_key_navigation: config.three_key_navigation,
            machine_easter_eggs: config.machine_easter_eggs,
            machine_nice_sound: config.machine_nice_sound,
            show_gameplay_timer: config.show_select_music_gameplay_timer,
            translated_titles: config.translated_titles,
            transparent_panels: matches!(
                config
                    .machine_evaluation_style
                    .resolve(config.visual_style.presentation_style(config.srpg_variant)),
                deadsync_config::theme::MachineEvaluationStyle::Transparent
            ),
            srpg10_visuals: config.visual_style.is_srpg()
                && matches!(
                    config.srpg_variant,
                    deadsync_config::theme::SrpgVariant::Srpg10
                ),
            machine_font: config.machine_font,
            zmod_rating_box_text: config.zmod_rating_box_text,
            difficulty_color_scheme: config.difficulty_color_scheme,
            breakdown_style: config.select_music_breakdown_style,
        }
    }
}

/// One Evaluation footer/online-availability player prepared by the shell.
#[derive(Clone, Debug)]
pub struct EvaluationPlayerView {
    pub joined: bool,
    pub guest: bool,
    pub avatar_texture_key: Option<String>,
    pub display_name: String,
    pub groovestats_linked: bool,
    pub arrowcloud_linked: bool,
    pub judgment_palette: deadsync_theme::color::JudgmentPalette,
}

impl Default for EvaluationPlayerView {
    fn default() -> Self {
        Self {
            joined: false,
            guest: false,
            avatar_texture_key: None,
            display_name: String::new(),
            groovestats_linked: false,
            arrowcloud_linked: false,
            judgment_palette: crate::color::SIMPLY_LOVE_JUDGMENT_PALETTE,
        }
    }
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

/// Selected player-slot charts whose scorebox data shell should prepare.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectMusicScoreboxRequest<'a> {
    pub chart_hashes: [Option<&'a str>; 2],
    pub leaderboards_allowed: bool,
    pub max_entries: usize,
}

/// On-demand chart selection whose full leaderboard overlay shell should
/// prepare. Hidden overlays return no request and perform no leaderboard work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectMusicLeaderboardRequest<'a> {
    pub chart_hashes: [Option<&'a str>; 2],
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
    Series {
        key: &'a str,
    },
    Song {
        song: &'a Arc<deadsync_chart::SongData>,
        chart_hashes: [Option<&'a str>; 2],
        golf_pars: [Option<f64>; 2],
        has_edit: bool,
        is_srpg_event: bool,
        is_itl_event: bool,
        unlock_song_dir: Option<&'a str>,
        sync_pref: deadsync_chart::SyncPref,
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
    pub score_mode: deadsync_config::theme::SelectMusicWheelScoreMode,
    pub score_type: deadsync_config::theme::SelectMusicWheelScoreType,
    pub show_failed_scores: bool,
    pub sides: [MusicWheelSideRuntimeRequest<'a>; 2],
    pub slots: [MusicWheelSlotRuntimeRequest<'a>; MUSIC_WHEEL_SLOT_COUNT],
}

/// Resolved wheel percentage; fail status belongs to this exact score/metric.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MusicWheelScoreView {
    pub hundredths: u32,
    pub score_type: deadsync_config::theme::SelectMusicWheelScoreType,
    pub failed: bool,
    pub itl_points: Option<u32>,
}

/// Shell-prepared runtime presentation data for one player side in one slot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicWheelSideRuntimeView {
    pub score: Option<deadsync_score::CachedScore>,
    pub lamp: Option<deadsync_score::CachedLamp>,
    pub itl_rank: Option<u32>,
    pub srpg_pass_rate_hundredths: Option<u32>,
    pub percentage: Option<MusicWheelScoreView>,
    pub favorite: bool,
    pub locked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MusicWheelSlotRuntimeView {
    pub golf_pars: [Option<f64>; 2],
    pub sides: [MusicWheelSideRuntimeView; 2],
    pub has_edit: bool,
    pub is_srpg_event: bool,
    pub sync_pref: deadsync_chart::SyncPref,
}

impl Default for MusicWheelSlotRuntimeView {
    fn default() -> Self {
        Self {
            sides: Default::default(),
            golf_pars: [None; 2],
            has_edit: false,
            is_srpg_event: false,
            sync_pref: deadsync_chart::SyncPref::Default,
        }
    }
}

/// Fixed-size shell-prepared snapshot consumed by shared wheel composition.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicWheelRuntimeView {
    pub joined: [bool; 2],
    pub play_style: deadsync_profile::PlayStyle,
    pub translated_titles: bool,
    pub song_bg_dimmed: bool,
    pub section_bg_dimmed: bool,
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
    /// Replacement profile footer emitted only when profile state changes.
    pub players: Option<[SelectFlowPlayerView; 2]>,
    /// Replacement fixed wheel emitted only when its source data changes.
    pub music_wheel: Option<MusicWheelRuntimeView>,
    /// Replacement score pane emitted with the corresponding wheel data.
    pub score: Option<SelectCourseScoreView>,
}

/// Shell-owned policy used by Select Course input and presentation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectCoursePolicyView {
    pub show_random_courses: bool,
    pub show_most_played_courses: bool,
    pub music_wheel_switch_speed: u8,
    pub global_offset_seconds: f32,
    pub dedicated_three_key_nav: bool,
    pub difficulty_color_scheme: deadsync_theme::color::DifficultyColorScheme,
}

impl Default for SelectCoursePolicyView {
    fn default() -> Self {
        let config = deadsync_config::app_config::Config::default();
        Self {
            show_random_courses: config.show_random_courses,
            show_most_played_courses: config.show_most_played_courses,
            music_wheel_switch_speed: config.music_wheel_switch_speed,
            global_offset_seconds: config.global_offset_seconds,
            dedicated_three_key_nav: config.three_key_navigation
                && config.only_dedicated_menu_buttons,
            difficulty_color_scheme: config.difficulty_color_scheme,
        }
    }
}

/// Session and machine context consumed by Select Course's pure screen logic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectCourseContextView {
    pub policy: SelectCoursePolicyView,
    pub play_style: deadsync_profile::PlayStyle,
    pub player_side: deadsync_profile::PlayerSide,
    pub joined: [bool; 2],
    pub music_rate: f32,
}

impl Default for SelectCourseContextView {
    fn default() -> Self {
        Self {
            policy: SelectCoursePolicyView::default(),
            play_style: deadsync_profile::PlayStyle::default(),
            player_side: deadsync_profile::PlayerSide::default(),
            joined: [true, false],
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
    pub chart_grades: Vec<(String, u8)>,
    pub translated_titles: bool,
    pub last_course_path: Option<PathBuf>,
    pub last_course_difficulty: Option<String>,
    pub context: SelectCourseContextView,
}

/// Presentation inputs applied to the resolved game catalog.
#[derive(Clone, Debug, Default)]
pub struct SelectCoursePresentationInitView {
    pub translated_titles: bool,
    pub last_course_path: Option<PathBuf>,
    pub last_course_difficulty: Option<String>,
    pub context: SelectCourseContextView,
}

pub use deadsync_theme::profile_picker::{LocalProfileView, ManageLocalProfilesView};
pub use deadsync_profile::picker::{ProfilePickerEntryView, ProfilePickerView};

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
    #[must_use]
    pub const fn side_joined(self, side: deadsync_profile::PlayerSide) -> bool {
        self.joined[deadsync_profile::player_side_index(side)]
    }

    #[inline(always)]
    #[must_use]
    pub const fn side_guest(self, side: deadsync_profile::PlayerSide) -> bool {
        self.guest[deadsync_profile::player_side_index(side)]
    }
}

impl Default for SelectMusicSessionView {
    fn default() -> Self {
        Self {
            play_style: PlayStyle::default(),
            player_side: PlayerSide::default(),
            joined: [true, false],
            guest: [true, true],
            music_rate: 1.0,
        }
    }
}

/// Shell-prepared profile identity and pad routing used by Select Music.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectMusicProfileView {
    pub display_names: [Arc<str>; 2],
    pub avatar_texture_keys: [Option<Arc<str>>; 2],
    pub local_profile_ids: [Option<Arc<str>>; 2],
}

/// One shell-prepared saved pad config shown in Select Music's quick menu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectMusicPadProfileView {
    pub name: String,
    pub is_default: bool,
}

impl SelectMusicProfileView {
    #[inline(always)]
    #[must_use]
    pub fn display_name(&self, side: deadsync_profile::PlayerSide) -> &str {
        &self.display_names[deadsync_profile::player_side_index(side)]
    }

    #[inline(always)]
    #[must_use]
    pub fn local_profile_id(&self, side: deadsync_profile::PlayerSide) -> Option<&str> {
        self.local_profile_ids[deadsync_profile::player_side_index(side)].as_deref()
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

/// Config-derived Select Music state replaced only when the compiled frame
/// policy changes or the screen is re-entered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectMusicSettingsView {
    pub arrow_bounce_offset: f32,
    pub policy: SelectMusicPolicyView,
    pub sync_graph_mode: deadsync_config::theme::SyncGraphMode,
    pub sync_graph_orientation: null_or_die::GraphOrientation,
    pub sync_graph_origin: deadsync_config::null_or_die::GraphOrigin,
    pub sync_confidence_percent: u8,
}

/// Shell-prepared runtime data consumed by Simply Love's Select Music screen.
#[derive(Clone, Debug)]
pub struct SelectMusicRuntimeView {
    /// Replacement session routing emitted only when its profile snapshot changes.
    pub session: Option<SelectMusicSessionView>,
    /// Replacement profile identity emitted only when its snapshot changes.
    pub profiles: Option<SelectMusicProfileView>,
    /// Refreshed only when the active local profile IDs change.
    pub favorites: Option<deadsync_profile::FavoriteSnapshot>,
    /// Refreshed on demand from the shell-owned pad-config cache.
    pub pad_profiles: Option<[Vec<SelectMusicPadProfileView>; 2]>,
    pub audio_playback: deadsync_theme::views::AudioPlaybackView,
    /// Replacement lobby view emitted on worker changes or reconnect deadlines.
    pub lobby: Option<SimplyLoveLobbyRuntimeView>,
    /// Replacement rows emitted only when the download runtime generation changes.
    pub downloads: Option<Vec<SelectMusicDownloadView>>,
    /// Replacement snapshot emitted only when the shop runtime changes.
    pub srpg_shop: Option<Arc<deadsync_online::srpg_shop::SrpgShopSnapshot>>,
    /// Replacement config state emitted only when the compiled policy changes.
    pub settings: Option<SelectMusicSettingsView>,
    /// Replacement snapshot emitted only when wheel inputs or score data change.
    pub music_wheel: Option<MusicWheelRuntimeView>,
    /// Replacement panes emitted only when chart, profile, or score data changes.
    pub scoreboxes: Option<[ScoreboxSideView; 2]>,
    /// Replacement overlay data emitted only when its request or cache changes.
    pub leaderboard: Option<SelectMusicLeaderboardView>,
    /// Replacement availability emitted when `GrooveStats` status or policy changes.
    pub unlock_downloads_available: Option<bool>,
    /// Completed download event emitted only when the worker queue changes.
    pub ready_song_reload_dirs: Option<Vec<PathBuf>>,
}

impl Default for SelectMusicRuntimeView {
    fn default() -> Self {
        Self {
            session: None,
            profiles: None,
            favorites: None,
            pad_profiles: None,
            audio_playback: deadsync_theme::views::AudioPlaybackView::default(),
            lobby: None,
            downloads: None,
            srpg_shop: None,
            settings: None,
            music_wheel: None,
            scoreboxes: None,
            leaderboard: None,
            unlock_downloads_available: None,
            ready_song_reload_dirs: None,
        }
    }
}

pub use deadsync_assets::progress::{
    ApplyReplayGainEvent as SimplyLoveApplyReplayGainEvent,
    ContentReloadEvent as SimplyLoveContentReloadEvent,
    ContentReloadPhase as SimplyLoveContentReloadPhase,
};

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
pub use deadsync_online::gameplay_lobby::LobbyRuntimeView as SimplyLoveLobbyRuntimeView;

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
    /// Replacement values are present only when their source generation or
    /// explicit refresh deadline changes. The screen retains prior values.
    pub context: Option<EvaluationContextView>,
    pub lobby: Option<SimplyLoveLobbyRuntimeView>,
    pub groovestats_service: Option<SimplyLoveGrooveStatsService>,
    pub submissions: Option<[EvaluationSubmissionView; 2]>,
    pub scoreboxes: Option<[ScoreboxSideView; 2]>,
    pub favorites: Option<[bool; 2]>,
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
#[derive(Clone, Debug, Default)]
pub struct SelectMusicInitView {
    pub songs_root: PathBuf,
    pub song_scan_roots: Vec<PathBuf>,
    pub courses_root: PathBuf,
    pub song_packs: Vec<deadsync_chart::SongPack>,
    pub playlists: Vec<SelectMusicPlaylistView>,
    pub history: SelectMusicHistoryView,
    pub policy: SelectMusicPolicyView,
    pub session: SelectMusicSessionView,
    pub profiles: SelectMusicProfileView,
    pub profile_picker: ProfilePickerView,
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

/// Shell-prepared profile selection used when Options builds Pack Sync targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionsPackSyncView {
    pub target_chart_type: String,
    pub preferred_difficulty_index: usize,
}

/// Shell-prepared machine state used to construct Simply Love's Options
/// screen. Options legitimately edits nearly the full config surface, so the
/// copied config is kept whole instead of duplicating its fields in another DTO.
#[derive(Clone, Debug)]
pub struct OptionsInitView {
    pub config: deadsync_config::app_config::Config,
    /// Mount basic and advanced pages over the same graphics settings.
    pub split_graphics: bool,
    pub service_order: &'static [deadsync_theme::views::SystemOptionsServiceRow],
    pub page_timing: Option<deadsync_theme::views::SystemOptionsTiming>,
    pub judgment_palettes: deadsync_config::judgment_palettes::JudgmentPaletteCatalog,
    pub updater_capabilities: SimplyLoveUpdaterCapabilities,
    pub app_paths: deadsync_theme::views::AppPathsView,
    pub audio: deadsync_theme::views::AudioOptionsView,
    pub graphics: deadsync_theme::views::GraphicsOptionsView,
    pub song_packs: Vec<OptionsSongPackView>,
    pub pack_sync: OptionsPackSyncView,
    pub noteskins: deadsync_theme::views::NoteskinCatalogView,
    pub machine_player_options: deadsync_profile::PlayerOptionsData,
    pub smx_assignment: deadsync_theme::views::SmxAssignmentView,
    pub smx_gifs: deadsync_theme::views::SmxGifCatalogView,
    pub score_import_profiles: Vec<crate::SimplyLoveScoreImportProfile>,
    pub bookkeeping: BookkeepingView,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BookkeepingView {
    pub coins_inserted: u64,
    pub credits_spent: u64,
    pub plays_started: u64,
    pub stages_played: u64,
}

/// Shell-prepared saved values edited by the Overscan Adjustment screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OverscanAdjustmentView {
    pub translate_x: i32,
    pub translate_y: i32,
    pub add_width: i32,
    pub add_height: i32,
}

impl Default for OptionsPackSyncView {
    fn default() -> Self {
        Self {
            target_chart_type: deadsync_profile::PlayStyle::default()
                .chart_type()
                .to_owned(),
            preferred_difficulty_index: 2,
        }
    }
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
    pub song_select_bg_mode: deadsync_config::theme::SelectMusicSongSelectBgMode,
}

/// Shell-prepared score and tournament policy consumed by Simply Love's wheel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectMusicWheelPolicyView {
    pub show_grades: bool,
    pub show_lamps: bool,
    pub itl_rank_mode: deadsync_config::theme::SelectMusicItlRankMode,
    pub score_mode: deadsync_config::theme::SelectMusicWheelScoreMode,
    pub score_type: deadsync_config::theme::SelectMusicWheelScoreType,
    pub show_failed_scores: bool,
    pub show_itl_points: bool,
}

impl Default for SelectMusicWheelPolicyView {
    fn default() -> Self {
        Self {
            show_grades: false,
            show_lamps: false,
            itl_rank_mode: deadsync_config::theme::SelectMusicItlRankMode::None,
            score_mode: deadsync_config::theme::SelectMusicWheelScoreMode::Events,
            score_type: deadsync_config::theme::SelectMusicWheelScoreType::Itg,
            show_failed_scores: false,
            show_itl_points: false,
        }
    }
}

/// Shell-prepared wheel and menu interaction policy for Select Music.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectMusicInteractionPolicyView {
    pub wheel_switch_speed: u8,
    pub wheel_style: deadsync_config::theme::SelectMusicWheelStyle,
    pub show_random_entries: bool,
    pub hide_inactive_series: bool,
    pub initial_sort: deadsync_config::theme::SelectMusicSort,
    pub remember_last_sort: bool,
    pub series_source: deadsync_config::theme::SelectMusicSeriesSource,
    pub new_pack_mode: deadsync_config::theme::NewPackMode,
    pub show_srpg_shop: bool,
    pub srpg10_visuals: bool,
    pub practice_shortcut: deadlib_platform::input::KeyCode,
    pub song_search_shortcut: deadlib_platform::input::KeyCode,
    pub reload_shortcut: deadlib_platform::input::KeyCode,
    pub test_input_shortcut: deadlib_platform::input::KeyCode,
}

impl Default for SelectMusicInteractionPolicyView {
    fn default() -> Self {
        Self {
            wheel_switch_speed: deadsync_config::defaults::DEFAULT_MUSIC_WHEEL_SWITCH_SPEED,
            wheel_style: deadsync_config::theme::SelectMusicWheelStyle::Itg,
            show_random_entries: false,
            hide_inactive_series: false,
            initial_sort: deadsync_config::theme::SelectMusicSort::Series,
            remember_last_sort: false,
            series_source: deadsync_config::theme::SelectMusicSeriesSource::PackIni,
            new_pack_mode: deadsync_config::theme::NewPackMode::Disabled,
            show_srpg_shop: deadsync_config::defaults::DEFAULT_SHOW_SRPG_SHOP,
            srpg10_visuals: false,
            practice_shortcut: deadlib_platform::input::KeyCode::KeyP,
            song_search_shortcut: deadlib_platform::input::KeyCode::KeyS,
            reload_shortcut: deadlib_platform::input::KeyCode::KeyL,
            test_input_shortcut: deadlib_platform::input::KeyCode::KeyT,
        }
    }
}

/// Saved display preferences and available services for Select Music.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectMusicPresentationPolicyView {
    pub show_golf_pars: bool,
    pub show_scorebox: bool,
    pub scorebox_cycle_enabled: bool,
    pub scorebox_in_step_pane: bool,
    pub show_stage_display: bool,
    pub show_gameplay_timer: bool,
    pub step_artist_box_mode: deadsync_config::theme::SelectMusicStepArtistBoxMode,
    pub breakdown_style: deadsync_config::theme::BreakdownStyle,
    pub pattern_info_mode: deadsync_config::theme::SelectMusicPatternInfoMode,
    pub chart_info_peak_nps: bool,
    pub chart_info_effective_bpm: bool,
    pub chart_info_matrix_rating: bool,
    pub show_breakdown: bool,
    pub pack_ini_offsets: bool,
    pub default_sync_offset: deadsync_config::theme::DefaultSyncOffset,
    pub difficulty_color_scheme: deadsync_theme::color::DifficultyColorScheme,
}

impl Default for SelectMusicPresentationPolicyView {
    fn default() -> Self {
        Self {
            show_golf_pars: false,
            show_scorebox: deadsync_config::defaults::DEFAULT_SHOW_SELECT_MUSIC_SCOREBOX,
            scorebox_cycle_enabled: deadsync_score::select_music_scorebox_filter_has_any(
                deadsync_score::SelectMusicScoreboxFilter {
                    groovestats_itg: deadsync_config::defaults::DEFAULT_GROOVESTATS_SCOREBOX_ITG,
                    groovestats_ex: deadsync_config::defaults::DEFAULT_GROOVESTATS_SCOREBOX_EX,
                    groovestats_events:
                        deadsync_config::defaults::DEFAULT_GROOVESTATS_SCOREBOX_EVENTS,
                    arrowcloud_regular: deadsync_config::options::arrowcloud_regular_boards_shown(
                        None,
                        deadsync_config::defaults::DEFAULT_ENABLE_GROOVESTATS,
                    ),
                    arrowcloud_events: deadsync_config::defaults::DEFAULT_ARROWCLOUD_EVENT_BOARDS,
                    golf: false,
                    events_first: true,
                },
            ),
            scorebox_in_step_pane: false,
            show_stage_display: deadsync_config::defaults::DEFAULT_SHOW_SELECT_MUSIC_STAGE_DISPLAY,
            show_gameplay_timer:
                deadsync_config::defaults::DEFAULT_SHOW_SELECT_MUSIC_GAMEPLAY_TIMER,
            step_artist_box_mode: deadsync_config::theme::SelectMusicStepArtistBoxMode::Default,
            breakdown_style: deadsync_config::theme::BreakdownStyle::Sl,
            pattern_info_mode: deadsync_config::theme::SelectMusicPatternInfoMode::Tech,
            chart_info_peak_nps:
                deadsync_config::defaults::DEFAULT_SELECT_MUSIC_CHART_INFO_PEAK_NPS,
            chart_info_effective_bpm:
                deadsync_config::defaults::DEFAULT_SELECT_MUSIC_CHART_INFO_EFFECTIVE_BPM,
            chart_info_matrix_rating:
                deadsync_config::defaults::DEFAULT_SELECT_MUSIC_CHART_INFO_MATRIX_RATING,
            show_breakdown: deadsync_config::defaults::DEFAULT_SHOW_SELECT_MUSIC_BREAKDOWN,
            pack_ini_offsets: deadsync_config::defaults::DEFAULT_MACHINE_PACK_INI_OFFSETS,
            default_sync_offset: deadsync_config::theme::DefaultSyncOffset::Null,
            difficulty_color_scheme: deadsync_theme::color::DifficultyColorScheme::SimplyLove,
        }
    }
}

/// Runtime/config policy used to expose Select Music features and input paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectMusicPolicyView {
    pub machine_font: deadsync_config::theme::MachineFont,
    pub dedicated_menu_only: bool,
    pub three_key_navigation: bool,
    pub fsr_profiles: bool,
    pub replays: bool,
    pub profile_switch: bool,
    pub keyboard_features: bool,
    pub allow_song_deletion: bool,
    pub media: SelectMusicMediaPolicyView,
    pub wheel: SelectMusicWheelPolicyView,
    pub interaction: SelectMusicInteractionPolicyView,
    pub presentation: SelectMusicPresentationPolicyView,
}

pub use deadsync_theme::select_music::DensityGraphSlot as SimplyLoveDensityGraphSlot;
pub(crate) use deadsync_theme::select_music::DensityGraphSlot;

/// Runtime capabilities used to decide which concrete Simply Love options
/// rows are available on the current host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SimplyLoveUpdaterCapabilities {
    pub app_update: bool,
    pub ffmpeg_install: bool,
}

pub use deadsync_theme::main_menu::{
    MainMenuArrowCloudError, MainMenuArrowCloudStatus, MainMenuGrooveError, MainMenuGrooveStatus,
    MainMenuSmxConflictView,
};
pub type MainMenuRuntimeView = deadsync_theme::main_menu::MainMenuRuntimeView<
    deadsync_config::theme::GameFlag,
    deadsync_config::coin::CoinMode,
>;

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

/// Workshop installation progress prepared by the shell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SimplyLoveWorkshopPhase {
    #[default]
    Idle,
    Downloading {
        written: u64,
        total: u64,
    },
    Preparing {
        done: usize,
        total: usize,
    },
    Publishing,
    Cancelling,
    Installed,
    Error {
        detail: String,
    },
}

/// One shell-prepared snapshot of the updater services used by Options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimplyLoveUpdaterView {
    pub update: SimplyLoveUpdatePhase,
    pub ffmpeg: SimplyLoveFfmpegPhase,
    pub workshop: SimplyLoveWorkshopPhase,
}

impl Default for SimplyLoveUpdaterView {
    fn default() -> Self {
        Self {
            update: SimplyLoveUpdatePhase::Idle,
            ffmpeg: SimplyLoveFfmpegPhase::Idle,
            workshop: SimplyLoveWorkshopPhase::Idle,
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
    fn gameplay_policy_carries_presentation_fields() {
        let config = deadsync_config::app_config::Config {
            machine_font: deadsync_config::theme::MachineFont::Mega,
            bg_brightness: 0.42,
            gameplay_bg_color: deadlib_present::color::Color::from_hex("#123456")
                .expect("test color should parse"),
            smx_input: true,
            zmod_rating_box_text: true,
            show_bpm_decimal: true,
            gameplay_bpm_position: deadsync_config::theme::GameplayBpmPosition::NearField,
            ..Default::default()
        };
        let policy = GameplayPolicyView::from_config(&config, Default::default());

        assert_eq!(policy.machine_font, config.machine_font);
        assert_eq!(policy.background_brightness, config.bg_brightness);
        assert_eq!(policy.background_color, config.gameplay_bg_color);
        assert_eq!(policy.smx_input, config.smx_input);
        assert_eq!(policy.zmod_rating_box_text, config.zmod_rating_box_text);
        assert_eq!(policy.show_bpm_decimal, config.show_bpm_decimal);
        assert_eq!(policy.bpm_position, config.gameplay_bpm_position);
    }

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

#[cfg(test)]
mod visual_policy_tests {
    use super::*;
    use crate::{color, visual_styles};

    #[test]
    fn visual_policy_resolves_runtime_style_and_bar_choices() {
        let mut config = deadsync_config::app_config::Config {
            visual_style: deadsync_config::theme::VisualStyle::Technique,
            machine_bar_color: deadsync_config::theme::MachineBarColor::Transparent,
            ..Default::default()
        };
        let technique = SimplyLoveVisualPolicyView::from_config(&config);
        assert_eq!(technique.background, VisualBackgroundView::Technique);
        assert_eq!(technique.screen_bar, ScreenBarBackgroundView::Transparent);
        assert!(!technique.srpg10_tint);

        config.visual_style = deadsync_config::theme::VisualStyle::Srpg9;
        config.srpg_variant = deadsync_config::theme::SrpgVariant::Srpg10;
        config.machine_bar_color = deadsync_config::theme::MachineBarColor::Colored;
        config.theme_color = 4;
        config.machine_font = deadsync_config::theme::MachineFont::Mega;
        let srpg10 = SimplyLoveVisualPolicyView::from_config(&config);
        assert_eq!(srpg10.background, VisualBackgroundView::Srpg);
        assert_eq!(
            srpg10.assets.shared_background,
            visual_styles::for_style_and_variant(config.visual_style, config.srpg_variant)
                .shared_background
        );
        assert_eq!(
            srpg10.screen_bar,
            ScreenBarBackgroundView::Colored(color::srpg10_rgba(4))
        );
        assert!(srpg10.srpg10_tint);
        assert_eq!(srpg10.machine_font, deadsync_config::theme::MachineFont::Mega);
        assert_eq!(
            srpg10.title_logo_texture_key,
            Some(visual_styles::SRPG10_TITLE_LOGO)
        );
    }

    #[test]
    fn ecs15_visual_policy_uses_curtains_logo_and_technique_defaults() {
        let mut config = deadsync_config::app_config::Config {
            visual_style: deadsync_config::theme::VisualStyle::Srpg9,
            srpg_variant: deadsync_config::theme::SrpgVariant::Ecs15,
            ..Default::default()
        };
        let policy = SimplyLoveVisualPolicyView::from_config(&config);
        assert_eq!(policy.background, VisualBackgroundView::Ecs15);
        assert_eq!(policy.assets, &visual_styles::ECS15_ASSETS);
        assert_eq!(
            policy.title_logo_texture_key,
            Some(visual_styles::ECS15_TITLE_LOGO)
        );
        assert_eq!(
            policy.screen_bar,
            ScreenBarBackgroundView::Transparent
        );
        assert!(!policy.srpg10_tint);
        assert!(policy.background_video(true).is_none());
        config.machine_bar_color = deadsync_config::theme::MachineBarColor::Colored;
        assert_eq!(
            SimplyLoveVisualPolicyView::from_config(&config).screen_bar,
            ScreenBarBackgroundView::Colored(color::decorative_rgba(config.theme_color))
        );
        config.visual_style = deadsync_config::theme::VisualStyle::Hearts;
        assert_eq!(
            SimplyLoveVisualPolicyView::from_config(&config).background,
            VisualBackgroundView::Tiled
        );
        assert!(
            SimplyLoveVisualPolicyView::from_config(&config)
                .title_logo_texture_key
                .is_none()
        );
    }
}
