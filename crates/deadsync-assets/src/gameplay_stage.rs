//! Gameplay startup with resolved song, noteskin, and Lua assets.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use deadlib_present::actors::Actor;
use deadsync_chart::{ChartData, GameplayChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::practice::editor::{PracticeEditorConfig, PracticeEditorFeedback};
use deadsync_gameplay::practice::{
    PracticeHost, PracticePlaybackUpdate, PracticeSession, PracticeViewRequest,
};
use deadsync_gameplay::{
    CourseDisplayCarry, CourseDisplayTiming, CourseDisplayTotals, CourseLifeConfig, GameplayConfig,
    GameplayMiniIndicatorData, GameplayProfileData, GameplaySession, GameplayViewport, LeadInTiming,
    ReplayInputEdge, ReplayOffsetSnapshot, hold_explosion_enabled_for_options, init_gameplay_runtime,
};
use deadsync_notefield::{MeasureLineMode, NotefieldSettings};
use deadsync_profile::{MeasureLines, Profile};
use deadsync_profile_gameplay::{
    GameplayProfile, gameplay_crossover_annotations_for_player, gameplay_pack_data,
    gameplay_runtime_profile_data,
};
use deadsync_profile_gameplay::stage_scores::GameplayScoreRefresh;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_song_lua::SongLuaLiveScene;
use deadsync_song_lua::gameplay_presentation::{FramePresentationContext, SongFrameContent};
use deadsync_song_lua::gameplay_session::{GameplayStage, SongLuaGameplaySession};
use deadsync_song_lua::playback::{FrameScratch, GameplayActorSegments, song_lua_sound_paths};

use crate::gameplay_noteskin::{GameplayNoteskinAssets, gameplay_noteskin_assets};
use crate::noteskin::SpriteSlot;

pub type GameplayCoreState =
    deadsync_song_lua::playback::GameplayCoreState<GameplayProfile, SpriteSlot>;
pub type ActiveGameplayStage = GameplayStage<GameplayCoreState, GameplayNoteskinAssets>;
pub type PreparedGameplaySongLua = deadsync_song_lua::playback::PreparedGameplaySongLua<SpriteSlot>;

/// Owns a live stage and its session data alongside a replaceable presentation.
pub struct GameplayHost<L, P> {
    pub stage: ActiveGameplayStage,
    pub lobby_session: L,
    pub pack_group: Arc<str>,
    pub score_refresh: GameplayScoreRefresh,
    pub presentation: P,
}

/// Presentation hooks for the retained stage's gameplay frame.
pub trait GameplayStagePresenter<L> {
    type View: Copy;
    type Policy: Copy;

    fn compose_stage_frame(
        &mut self,
        stage: &ActiveGameplayStage,
        lobby: &L,
        song: &mut FrameScratch,
        actors: &mut Vec<Actor>,
        context: FramePresentationContext<'_, Self::View, Self::Policy>,
        content: SongFrameContent,
    ) -> GameplayActorSegments;

    fn append_song_lua_exit(&self, _actors: &mut Vec<Actor>, _fade_progress: f32) {}
}

/// Presentation hooks for the practice editor around a normal stage frame.
pub trait PracticeStagePresenter<L, P: GameplayStagePresenter<L>> {
    fn frame_view(&self, session: &PracticeSession) -> P::View;

    fn append_practice_overlay(
        &mut self,
        actors: &mut Vec<Actor>,
        gameplay: &GameplayHost<L, P>,
        session: &PracticeSession,
    );
}

/// Presentation hooks for a live, game-owned practice session.
pub trait PracticeLivePresentation<P> {
    type Feedback<'a>: PracticeEditorPresentation<P>
    where
        Self: 'a;
    type Effect;

    fn editor_feedback(&mut self) -> Self::Feedback<'_>;
    fn advance_visuals(&mut self, _delta_time: f32) {}
    fn apply_playback_update(&mut self, _update: PracticePlaybackUpdate) {}
    fn apply_view_request(&mut self, _request: PracticeViewRequest) {}
    fn append_effects(&mut self, _effects: &mut Vec<Self::Effect>) {}
}

/// Receives read-only game facts while the editor operates and refreshes the HUD afterward.
pub trait PracticeEditorPresentation<P>: Sized {
    fn emit(&mut self, game: &GameplayCoreState, feedback: PracticeEditorFeedback);
    fn finish(self, _stage: &ActiveGameplayStage, _gameplay_hud: &mut P) {}
}

impl<L, P> GameplayHost<L, P> {
    pub fn stage_and_lobby(&mut self) -> (&mut ActiveGameplayStage, &mut L) {
        (&mut self.stage, &mut self.lobby_session)
    }

    pub fn into_practice<Q>(
        mut self,
        editor: PracticeEditorConfig,
        present: impl FnOnce(&ActiveGameplayStage, &mut P) -> Q,
    ) -> PracticeHost<Self, Q> {
        let mut session = PracticeSession::new(&mut self.stage.gameplay, editor);
        let presentation = present(&self.stage, &mut self.presentation);
        session.snap_display_to_cursor(&mut self.stage.gameplay);
        PracticeHost {
            gameplay: self,
            session,
            presentation,
        }
    }
}

impl<L, P: GameplayStagePresenter<L>> GameplayHost<L, P> {
    pub fn compose_gameplay_frame(
        &mut self,
        actors: &mut Vec<Actor>,
        context: FramePresentationContext<'_, P::View, P::Policy>,
        content: SongFrameContent,
    ) -> GameplayActorSegments {
        let segments = self.stage.with_frame_scratch(|stage, song| {
            self.presentation.compose_stage_frame(
                stage,
                &self.lobby_session,
                song,
                actors,
                context,
                content,
            )
        });
        if let Some(exit) = self.stage.song_lua.out_transition() {
            self.presentation.append_song_lua_exit(actors, exit.fade_progress());
        }
        segments
    }
}

impl<L, P> Deref for GameplayHost<L, P> {
    type Target = ActiveGameplayStage;

    fn deref(&self) -> &Self::Target {
        &self.stage
    }
}

impl<L, P> DerefMut for GameplayHost<L, P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stage
    }
}

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

/// Song intro and pack details used to construct a stage's presentation.
pub struct GameplayStageAssets {
    pub hide_song_intro: bool,
    pub pack_group: Arc<str>,
    pub pack_banner_path: Option<PathBuf>,
}

impl GameplayStageAssets {
    pub fn score_pack_group(&self, course_name: Option<&Arc<str>>) -> Arc<str> {
        Arc::clone(course_name.unwrap_or(&self.pack_group))
    }
}

impl PreparedGameplayStage {
    /// Present the prepared stage, then attach its live callbacks before returning the host.
    pub fn into_host<L, P>(
        self,
        scene: Option<SongLuaLiveScene>,
        lobby_session: L,
        score_refresh: GameplayScoreRefresh,
        course_name: Option<&str>,
        present: impl FnOnce(&ActiveGameplayStage, GameplayStageAssets) -> P,
    ) -> GameplayHost<L, P> {
        let (stage, assets) = self.into_parts();
        let pack_group = match course_name {
            Some("") => Arc::from("Untitled Course"),
            Some(name) => Arc::from(name),
            None => Arc::clone(&assets.pack_group),
        };
        let presentation = present(&stage, assets);
        let mut host = GameplayHost {
            stage,
            lobby_session,
            pack_group,
            score_refresh,
            presentation,
        };
        let (gameplay, session, warm_dynamic_scene) = host.stage.song_lua_attachment_parts();
        attach_prepared_song_lua(gameplay, session, scene, warm_dynamic_scene);
        host
    }

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

    pub fn into_parts(self) -> (ActiveGameplayStage, GameplayStageAssets) {
        let notefield_settings = self.gameplay.profiles().each_ref().map(notefield_settings);
        (
            GameplayStage::with_song_playback_and_noteskins(
                self.gameplay,
                self.song_lua,
                self.song_lua_sound_paths,
                self.noteskin_assets,
                notefield_settings,
            ),
            GameplayStageAssets {
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

fn notefield_settings(profile: &GameplayProfile) -> NotefieldSettings {
    NotefieldSettings {
        mine_size_scale: profile.mine_size_percent.clamp(10, 200) as f32 / 100.0,
        measure_line_mode: match profile.measure_lines {
            MeasureLines::Off => MeasureLineMode::Off,
            MeasureLines::Measure => MeasureLineMode::Measure,
            MeasureLines::Quarter => MeasureLineMode::Quarter,
            MeasureLines::Eighth => MeasureLineMode::Eighth,
        },
        measure_cues: profile.measure_cues,
        column_cues: profile.column_cues,
        crossover_cues: profile.crossover_cues,
        crossover_countdown: profile.column_countdown,
        column_flash: profile.column_flash_options().enabled,
        notefield_offset: [
            profile.note_field_offset_x.clamp(0, 50) as f32,
            profile.note_field_offset_y.clamp(-50, 50) as f32,
        ],
        fallback_mini_percent: profile.mini_percent(),
        hide_targets: profile.hide_targets,
        hold_explosion_enabled: hold_explosion_enabled_for_options(profile.tap_explosion_options()),
        hide_tap_explosion_noteskin: profile.tap_explosion_noteskin_hidden(),
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

#[cfg(test)]
mod tests {
    use super::{GameplayProfile, Profile, notefield_settings};
    use deadsync_profile::TapExplosionMask;

    #[test]
    fn hold_explosion_option_uses_holding_mask() {
        let mut profile = GameplayProfile(Profile::default());
        assert!(notefield_settings(&profile).hold_explosion_enabled);

        profile
            .tap_explosion_active_mask
            .remove(TapExplosionMask::HOLDING);
        assert!(!notefield_settings(&profile).hold_explosion_enabled);

        profile
            .tap_explosion_active_mask
            .insert(TapExplosionMask::HOLDING);
        profile
            .tap_explosion_active_mask
            .remove(TapExplosionMask::HELD);
        assert!(notefield_settings(&profile).hold_explosion_enabled);
    }
}
