//! Gameplay startup with resolved song, noteskin, and Lua assets.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use deadsync_chart::{ChartData, GameplayChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    CourseDisplayCarry, CourseDisplayTiming, CourseDisplayTotals, CourseLifeConfig, GameplayConfig,
    GameplayMiniIndicatorData, GameplaySession, GameplayViewport, LeadInTiming, ReplayInputEdge,
    ReplayOffsetSnapshot, init_gameplay_runtime,
};
use deadsync_profile::Profile;
use deadsync_profile_gameplay::{
    GameplayProfile, gameplay_crossover_annotations_for_player, gameplay_pack_data,
    gameplay_runtime_profile_data,
};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_song_lua::SongLuaLiveScene;
use deadsync_song_lua::gameplay_session::{GameplayStage, SongLuaGameplaySession};
use deadsync_song_lua::playback::song_lua_sound_paths;

use crate::gameplay_noteskin::{GameplayNoteskinAssets, gameplay_noteskin_assets};
use crate::noteskin::SpriteSlot;

pub type GameplayCoreState =
    deadsync_song_lua::playback::GameplayCoreState<GameplayProfile, SpriteSlot>;
pub type PreparedGameplaySongLua = deadsync_song_lua::playback::PreparedGameplaySongLua<SpriteSlot>;

pub struct GameplayStageInput {
    pub song: Arc<SongData>,
    pub charts: [Arc<ChartData>; MAX_PLAYERS],
    pub gameplay_charts: [Arc<GameplayChartData>; MAX_PLAYERS],
    pub viewport: GameplayViewport,
    pub session: GameplaySession,
    pub config: GameplayConfig,
    pub active_color_index: i32,
    pub music_rate: f32,
    pub scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
    pub player_profiles: [Profile; MAX_PLAYERS],
    pub replay_edges: Option<Vec<ReplayInputEdge>>,
    pub replay_offsets: Option<ReplayOffsetSnapshot>,
    pub lead_in_timing: Option<LeadInTiming>,
    pub course_display_carry: Option<[CourseDisplayCarry; MAX_PLAYERS]>,
    pub course_display_totals: Option<[CourseDisplayTotals; MAX_PLAYERS]>,
    pub course_display_timing: Option<CourseDisplayTiming>,
    pub course_modifiers: Option<Arc<str>>,
    pub course_life_config: [CourseLifeConfig; MAX_PLAYERS],
    pub include_post_fail_passes: bool,
    pub combo_carry: [u32; MAX_PLAYERS],
    pub song_lua_data: PreparedGameplaySongLua,
    pub mini_indicator: GameplayMiniIndicatorData,
}

pub struct PreparedGameplayStage {
    pub gameplay: GameplayCoreState,
    pub noteskin_assets: GameplayNoteskinAssets,
    pub song_lua: SongLuaGameplaySession,
    pub song_lua_sound_paths: Vec<PathBuf>,
    pub hide_song_intro: bool,
    pub pack_group: Arc<str>,
    pub pack_banner_path: Option<PathBuf>,
}

/// Resolved assets and metadata used to construct a stage's presentation.
pub struct GameplayStageAssets {
    pub noteskin_assets: GameplayNoteskinAssets,
    pub hide_song_intro: bool,
    pub pack_group: Arc<str>,
    pub pack_banner_path: Option<PathBuf>,
}

impl PreparedGameplayStage {
    /// Select authored and random cues against the master chart installed in
    /// the prepared simulation, after any Lua chart replacement or side mapping.
    pub fn resolve_song_backgrounds(&mut self, mode: deadsync_config::theme::RandomBackgroundMode) {
        use deadsync_profile_gameplay::stage_charts::stage_background_changes;

        let changes = stage_background_changes(
            self.gameplay.song(),
            &self.gameplay.gameplay_charts()[0],
            matches!(
                mode,
                deadsync_config::theme::RandomBackgroundMode::RandomMovies
            ),
        );
        self.gameplay.replace_song_background_changes(changes);
    }

    pub fn into_parts(self) -> (GameplayStage<GameplayCoreState>, GameplayStageAssets) {
        (
            GameplayStage::with_song_playback(
                self.gameplay,
                self.song_lua,
                self.song_lua_sound_paths,
            ),
            GameplayStageAssets {
                noteskin_assets: self.noteskin_assets,
                hide_song_intro: self.hide_song_intro,
                pack_group: self.pack_group,
                pack_banner_path: self.pack_banner_path,
            },
        )
    }

    /// Prepare lifecycle state for a caller-supplied simulation.
    pub fn from_gameplay(
        mut gameplay: GameplayCoreState,
        noteskin_assets: GameplayNoteskinAssets,
    ) -> Self {
        let song_lua = SongLuaGameplaySession::for_gameplay(&mut gameplay);
        Self {
            gameplay,
            noteskin_assets,
            song_lua,
            song_lua_sound_paths: Vec::new(),
            hide_song_intro: false,
            pack_group: Arc::from(""),
            pack_banner_path: None,
        }
    }
}

/// Build the simulation and resolve its song-lifetime assets before presentation.
pub fn prepare_gameplay_stage(
    input: GameplayStageInput,
) -> (PreparedGameplayStage, Option<SongLuaLiveScene>) {
    let GameplayStageInput {
        song,
        charts,
        gameplay_charts,
        viewport,
        session,
        config,
        active_color_index,
        music_rate,
        scroll_speed,
        player_profiles,
        replay_edges,
        replay_offsets,
        lead_in_timing,
        course_display_carry,
        course_display_totals,
        course_display_timing,
        course_modifiers,
        course_life_config,
        include_post_fail_passes,
        combo_carry,
        mut song_lua_data,
        mini_indicator,
    } = input;
    let cols_per_player = session.play_style.cols_per_player();
    let num_players = session.play_style.player_count();
    let runtime_profiles = gameplay_runtime_profile_data(&player_profiles, &session);
    let startup = song_lua_data.startup();
    let live_scene = song_lua_data.take_live_scene().unwrap_or_else(|error| {
        log::warn!("Cannot attach song Lua for '{}': {error}", song.title);
        None
    });
    let noteskin_assets = gameplay_noteskin_assets(
        cols_per_player,
        num_players,
        &runtime_profiles,
        song.simfile_path.parent().unwrap_or_else(|| Path::new(".")),
        &startup.noteskins,
        gameplay_charts[0]
            .timing
            .get_beat_for_time(song.first_second),
    );
    let lead_in_timing = startup
        .min_seconds_to_music
        .map_or(lead_in_timing, |seconds| {
            let mut timing = lead_in_timing.unwrap_or_default();
            timing.min_seconds_to_music = timing.min_seconds_to_music.max(seconds);
            Some(timing)
        });
    let noteskin_data =
        noteskin_assets.gameplay_data(cols_per_player, num_players, &runtime_profiles);
    let player_profiles = player_profiles.map(GameplayProfile::from);
    let song_lua_sound_paths = song_lua_sound_paths(&song_lua_data);
    let pack_data = gameplay_pack_data(&song, None, None);
    let mut gameplay = init_gameplay_runtime(
        song,
        charts,
        gameplay_charts,
        viewport,
        session,
        config,
        pack_data.sync_pref,
        mini_indicator,
        noteskin_data,
        song_lua_data,
        gameplay_crossover_annotations_for_player,
        active_color_index,
        music_rate,
        scroll_speed,
        player_profiles,
        replay_edges,
        replay_offsets,
        lead_in_timing,
        course_display_carry,
        course_display_totals,
        course_display_timing,
        course_modifiers,
        course_life_config,
        include_post_fail_passes,
        combo_carry,
    );
    let song_lua = SongLuaGameplaySession::for_gameplay(&mut gameplay);
    (
        PreparedGameplayStage {
            gameplay,
            noteskin_assets,
            song_lua,
            song_lua_sound_paths,
            hide_song_intro: startup.hide_in,
            pack_group: pack_data.pack_group,
            pack_banner_path: pack_data.pack_banner_path,
        },
        live_scene,
    )
}

/// Attach authored callbacks after the theme has built its initial frame state.
/// The optional warmup runs at the controller's callback-install boundary.
pub fn attach_prepared_song_lua(
    gameplay: &mut GameplayCoreState,
    session: &mut SongLuaGameplaySession,
    scene: Option<SongLuaLiveScene>,
    warm_dynamic_scene: impl FnOnce(),
) {
    if let Some(scene) = scene
        && let Err(error) = session.attach(gameplay, scene, warm_dynamic_scene)
    {
        gameplay.mods.song_lua_compile_succeeded = false;
        log::warn!("Cannot attach prepared song Lua: {error}");
    }
}
