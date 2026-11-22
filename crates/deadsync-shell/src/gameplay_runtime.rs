use crate::{ActiveTheme, ThemeScreen};
use deadlib_platform::input::RawKeyboardEvent;
use deadsync_assets::gameplay_stage::{
    ActiveGameplayStage, GameplayCoreState, GameplayHost, GameplayStageAssets, GameplayStagePresenter,
    PracticeStagePresenter, PreparedGameplayStage,
};
use deadsync_chart::ChartData;
use deadsync_config as config;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    GameplayAction, GameplayAudioCommand, GameplayAudioSnapshot, GameplayMiniIndicatorData, GameplayMusicCut,
    GameplayInputPlayerSide, GameplayInputPlayStyle, GameplaySession, GameplaySessionCommand,
    GameplayStreamClockSnapshot, gameplay_runtime_charts,
};
use deadsync_gameplay::practice::{
    PracticeDestination, PracticeHost, PracticeViewRequest,
    editor::{PracticeEditorConfig, PracticeEditorSnapshot},
};
use deadsync_input::InputEvent;
use deadsync_input::practice::PracticeKeyEvent;
use deadsync_online::gameplay_lobby::{
    GameplayLobbyAction, GameplayLobbyInput, GameplayLobbySession, LobbyRuntimeView,
};
use deadsync_online::score_compat as scores;
use deadsync_profile as profile_data;
use deadsync_profile::compat as profile;
use deadsync_profile_gameplay::{
    gameplay_runtime_profile_data, profile_side_from_gameplay, profile_tick_mode_from_gameplay,
};
use deadsync_profile_gameplay::stage_scores::{
    GameplayScoreRefresh, GameplayScoreSnapshots, mini_indicator_rival_score,
};
use deadsync_song_lua::gameplay_presentation::{FramePresentationContext, SongFrameContent};
use deadsync_song_lua::gameplay_session::SongLuaGameplayEffect;
use deadsync_theme::ThemeScreenFlow;
use deadsync_theme::views::{
    GameplayLivePresentation, GameplayStageView, SmxSensorPadView, SmxSensorPanelView,
};
use deadsync_theme_simply_love::SimplyLoveEffect as ThemeEffect;
use deadsync_theme_simply_love::screens::{gameplay, practice};
use deadsync_theme_simply_love::views::{
    GameplayInitView, GameplayPolicyView, GameplayRuntimeView,
};
use smallvec::SmallVec;
use std::sync::Arc;

const GAMEPLAY_SCOREBOX_ENTRIES: usize = 5;

pub(crate) type GameplayState = GameplayHost<GameplayLobbySession, gameplay::GameplayPresentation>;
pub(crate) type PracticeState = PracticeHost<GameplayState, practice::PracticePresentation>;

#[derive(Clone, Copy, Debug)]
pub(crate) struct PracticeEditSnapshot {
    editor: PracticeEditorSnapshot,
    edit_scroll_speed_index: usize,
}

pub(crate) fn practice_edit_snapshot(state: &PracticeState) -> PracticeEditSnapshot {
    PracticeEditSnapshot {
        editor: state.session.editor().snapshot(),
        edit_scroll_speed_index: state.presentation.edit_scroll_speed_index(),
    }
}

pub(crate) fn restore_practice_edit_snapshot(state: &mut PracticeState, snapshot: PracticeEditSnapshot) {
    state.session.restore(&state.gameplay.stage.gameplay, snapshot.editor);
    state.session.snap_display_to_cursor(&mut state.gameplay.stage.gameplay);
    state.presentation.restore_edit_scroll_speed(snapshot.edit_scroll_speed_index);
}

/// Game-thread-owned cursor for the current Gameplay or Practice song's
/// asynchronous leaderboard snapshots. It is single-threaded and retains one
/// lock-free generation for the song lifetime, seeded before initial requests.
/// Completed songs read only the retained pending flag; while loading, stable
/// frames read one atomic and take no cache locks or allocations. A changed
/// generation performs at most two leaderboard-cache lookups and cheap `Arc`
/// clones; it never retries a completed error, scans, evicts, or performs I/O.
/// Replacement and destruction happen at screen transitions, and existing
/// frame-update timing accounts for the bounded miss cost.
#[derive(Debug, Default)]
pub(crate) struct ScoreRuntimeCursor {
    generation: Option<u64>,
}

#[inline(always)]
fn score_generation_changed(cached: Option<u64>, current: u64) -> bool {
    cached != Some(current)
}

impl ScoreRuntimeCursor {
    #[inline(always)]
    fn begin_song(&mut self) {
        self.generation = Some(deadsync_score::runtime_player_leaderboard_generation());
    }

    #[inline(always)]
    fn sync_if_dirty<P: GameplayLivePresentation<GameplayCoreState>>(
        &mut self, state: &mut GameplayHost<GameplayLobbySession, P>,
    ) {
        if !state.score_refresh.has_pending() {
            return;
        }
        let generation = deadsync_score::runtime_player_leaderboard_generation();
        if !score_generation_changed(self.generation, generation) {
            return;
        }

        // Capture the generation before taking cache locks. A completion racing
        // this refresh can cause one redundant pass, but cannot be hidden.
        let updates = state.score_refresh.poll(&mut state.stage.gameplay, |chart_hash, profile| {
            scores::get_or_fetch_player_leaderboards_for_profile(
                chart_hash,
                profile,
                GAMEPLAY_SCOREBOX_ENTRIES,
            )
        });
        self.generation = Some(generation);
        state.presentation.sync_leaderboards(updates);
    }
}

fn policy_view(config: &config::app_config::Config) -> GameplayPolicyView {
    GameplayPolicyView {
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
        scorebox_pane_filter: crate::app::scorebox_pane_filter(config),
    }
}

fn mini_indicator_personal_best(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    score_type: profile_data::MiniIndicatorScoreType,
) -> Option<f64> {
    // zmod targets the local profile. EX and Hard EX are DeadSync extensions
    // that follow the same local-only rule with their matching scalar records.
    match score_type {
        profile_data::MiniIndicatorScoreType::Itg => {
            scores::get_cached_local_score_for_side(chart_hash, side)
                .and_then(|score| mini_indicator_percent(score.score_percent * 100.0))
        }
        profile_data::MiniIndicatorScoreType::Ex => {
            scores::get_cached_local_ex_score_for_side(chart_hash, side)
                .and_then(|score| mini_indicator_percent(score.percent))
        }
        profile_data::MiniIndicatorScoreType::HardEx => {
            scores::get_cached_local_hard_ex_score_for_side(chart_hash, side)
                .and_then(|score| mini_indicator_percent(score.percent))
        }
    }
}

fn mini_indicator_percent(percent: f64) -> Option<f64> {
    (percent.is_finite() && percent > 0.0).then(|| percent.min(100.0))
}

fn mini_indicator_machine_best(
    chart_hash: &str,
    score_type: profile_data::MiniIndicatorScoreType,
) -> Option<f64> {
    match score_type {
        profile_data::MiniIndicatorScoreType::Itg => scores::get_machine_record_local(chart_hash)
            .and_then(|(_, score)| mini_indicator_percent(score.score_percent * 100.0)),
        profile_data::MiniIndicatorScoreType::Ex => {
            scores::get_machine_scalar_record_local(chart_hash, false)
                .and_then(|(_, score)| mini_indicator_percent(score.percent))
        }
        profile_data::MiniIndicatorScoreType::HardEx => {
            scores::get_machine_scalar_record_local(chart_hash, true)
                .and_then(|(_, score)| mini_indicator_percent(score.percent))
        }
    }
}

fn mini_indicator_view(
    charts: &[Arc<ChartData>; 2],
    profiles: &[profile_data::Profile; 2],
    session: &GameplaySession,
    scorebox_snapshots: &[Option<deadsync_score::CachedPlayerLeaderboardData>; 2],
) -> GameplayMiniIndicatorData {
    let mut view = GameplayMiniIndicatorData::default();
    for player in 0..session.play_style.player_count() {
        let side = profile_side_from_gameplay(session.runtime_player_side(player));
        let chart_hash = charts[player].short_hash.as_str();
        let score_type = profiles[player].mini_indicator_score_type;
        if profiles[player].target_score == profile_data::TargetScoreSetting::GhostData {
            let ghost_type = match score_type {
                profile_data::MiniIndicatorScoreType::Itg => deadsync_gameplay::GhostScoreType::Itg,
                profile_data::MiniIndicatorScoreType::Ex => deadsync_gameplay::GhostScoreType::Ex,
                profile_data::MiniIndicatorScoreType::HardEx => {
                    deadsync_gameplay::GhostScoreType::HardEx
                }
            };
            if let Some(profile_id) = profile_data::runtime_active_local_profile_id_for_side(side) {
                let dir = profile_data::app_runtime::local_profile_dir(&profile_id);
                match deadsync_score::ghost::load(&dir, chart_hash) {
                    Ok(data) => {
                        let mut histories = data.into_histories();
                        view.ghost_targets[player] = deadsync_gameplay::GameplayGhostTarget {
                            score_type: ghost_type,
                            points: std::mem::take(&mut histories[ghost_type.index()]),
                        };
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        log::warn!("Could not load ghost history for {chart_hash}: {error}")
                    }
                }
            }
        }
        view.specified_target_percent[player] =
            f64::from(profiles[player].target_score_percent.min(100));
        view.personal_best_percent[player] =
            mini_indicator_personal_best(chart_hash, side, score_type);
        view.machine_best_percent[player] = mini_indicator_machine_best(chart_hash, score_type);
        let side_idx = profile_data::player_side_index(side);
        view.rival_score_percent[player] =
            mini_indicator_rival_score(scorebox_snapshots[side_idx].as_ref(), score_type);
    }
    view
}

fn scorebox_profiles(
    profiles: &[profile_data::Profile; 2],
    session: &GameplaySession,
) -> [deadsync_score::GameplayScoreboxProfileSnapshot; 2] {
    let mut snapshots =
        std::array::from_fn(|_| deadsync_score::GameplayScoreboxProfileSnapshot::default());
    for player in 0..session.play_style.player_count() {
        let gameplay_side = session.runtime_player_side(player);
        let side = profile_side_from_gameplay(gameplay_side);
        snapshots[profile_data::player_side_index(side)] = scores::scorebox_profile_snapshot(
            &profiles[player],
            session.side_joined(gameplay_side),
            session.active_profile_id_for_side(gameplay_side),
        );
    }
    snapshots
}

fn prepare_scores(
    charts: &[Arc<ChartData>; 2],
    profiles: &[profile_data::Profile; 2],
    session: &GameplaySession,
) -> (GameplayMiniIndicatorData, GameplayScoreRefresh, GameplayScoreSnapshots) {
    let runtime_charts = gameplay_runtime_charts(charts, session);
    let runtime_profiles = gameplay_runtime_profile_data(profiles, session);
    let scorebox_profiles = scorebox_profiles(&runtime_profiles, session);
    let mut scorebox_snapshots = std::array::from_fn(|_| None);
    let mut rival_score_types = [None; 2];
    for player in 0..session.play_style.player_count() {
        let side = profile_side_from_gameplay(session.runtime_player_side(player));
        let side_idx = profile_data::player_side_index(side);
        let profile = &scorebox_profiles[side_idx];
        let chart_hash = runtime_charts[player].short_hash.trim();
        let wants_rival =
            runtime_profiles[player].mini_indicator == profile_data::MiniIndicator::RivalScoring;
        rival_score_types[side_idx] =
            wants_rival.then_some(runtime_profiles[player].mini_indicator_score_type);
        if (profile.display_scorebox || wants_rival)
            && profile.online_active()
            && !chart_hash.is_empty()
        {
            scorebox_snapshots[side_idx] = scores::get_or_fetch_player_leaderboards_for_profile(
                chart_hash,
                profile,
                GAMEPLAY_SCOREBOX_ENTRIES,
            );
        }
    }
    let mini_indicator = mini_indicator_view(
        &runtime_charts,
        &runtime_profiles,
        session,
        &scorebox_snapshots,
    );
    let scorebox_visible = scorebox_profiles
        .each_ref()
        .map(|profile| profile.display_scorebox && profile.online_active());
    let score_refresh = GameplayScoreRefresh::new(
        scorebox_profiles,
        &scorebox_snapshots,
        rival_score_types,
    );
    (
        mini_indicator,
        score_refresh,
        GameplayScoreSnapshots { scorebox_visible, scorebox_snapshots },
    )
}

pub(crate) struct GameplayStartup {
    pub mini_indicator: GameplayMiniIndicatorData,
    pub lobby_session: GameplayLobbySession,
    pub score_refresh: GameplayScoreRefresh,
    pub scores: GameplayScoreSnapshots,
}

pub(crate) fn prepare_startup(
    lobby: LobbyRuntimeView,
    charts: &[Arc<ChartData>; 2],
    profiles: &[profile_data::Profile; 2],
    session: &GameplaySession,
    score_cursor: &mut ScoreRuntimeCursor,
) -> GameplayStartup {
    // Seed before preparing scores so even an immediate worker completion
    // publishes a generation newer than this song's cursor.
    score_cursor.begin_song();
    let lobby_session = GameplayLobbySession::new(
        lobby,
        session.joined_sides,
        profile_side_from_gameplay(session.player_side),
    );
    let (mini_indicator, score_refresh, scores) = prepare_scores(charts, profiles, session);
    GameplayStartup { mini_indicator, lobby_session, score_refresh, scores }
}

pub(crate) fn presentation_init(
    config: &config::app_config::Config,
    profiles: &[profile_data::Profile; 2],
) -> GameplayInitView {
    let hud_session = profile::get_session_snapshot();
    let runtime = GameplayRuntimeView {
        policy: policy_view(config),
        play_style: hud_session.play_style,
        player_side: hud_session.player_side,
        joined: std::array::from_fn(|idx| {
            hud_session.side_joined(profile_data::player_side_for_index(idx))
        }),
    };
    let hud = profile::gameplay_hud_snapshot();
    let judgment_palettes = {
        let catalog = deadsync_config::judgment_palettes::runtime_catalog(
            deadsync_theme_simply_love::color::JUDGMENT_PRESET,
        );
        std::array::from_fn(|player| catalog.resolve(profiles[player].judgment_palette_id.as_deref()))
    };
    GameplayInitView { runtime, hud, judgment_palettes, scores: Default::default() }
}

pub(crate) fn refresh_chart_scores(
    startup: &mut GameplayStartup,
    charts: &[Arc<ChartData>; 2],
    profiles: &[profile_data::Profile; 2],
    session: &GameplaySession,
) {
    (startup.mini_indicator, startup.score_refresh, startup.scores) =
        prepare_scores(charts, profiles, session);
}

pub(crate) fn present_stage<P>(
    prepared: PreparedGameplayStage,
    scene: Option<deadsync_song_lua::SongLuaLiveScene>,
    view: GameplayStageView,
    startup: GameplayStartup,
    present: impl FnOnce(
        &ActiveGameplayStage,
        GameplayStageAssets,
        &GameplayStageView,
        GameplayScoreSnapshots,
    ) -> P,
) -> GameplayHost<GameplayLobbySession, P> {
    let course_name = view.course.as_ref().map(|course| course.title.as_str());
    prepared.into_host(
        scene,
        startup.lobby_session,
        startup.score_refresh,
        course_name,
        |stage, assets| present(stage, assets, &view, startup.scores),
    )
}

/// Prepare song-invariant score state once, before gameplay begins. In
/// particular, resolving the ITL warning must never become part of the live
/// score-polling path: its app-runtime adapter may inspect profile and catalog
/// state, while the result cannot change during a stage.
pub(crate) fn sync_initial_scores(state: &mut GameplayState) {
    let warnings = std::array::from_fn(|player| {
        player < state.stage.num_players() && scores::should_warn_cmod_for_itl_chart(&state.stage, player)
    });
    state.presentation.set_itl_cmod_warning(warnings);
}

pub(crate) struct PracticeInit {
    editor: PracticeEditorConfig,
    hud: profile_data::GameplayHudSnapshot,
}

pub(crate) fn practice_init(
    config: &config::app_config::Config,
    hud: &profile_data::GameplayHudSnapshot,
) -> PracticeInit {
    PracticeInit {
        editor: PracticeEditorConfig {
            only_dedicated_menu_buttons: config.only_dedicated_menu_buttons,
            three_key_navigation: config.three_key_navigation,
            tab_acceleration: config.tab_acceleration,
        },
        hud: hud.clone(),
    }
}

pub(crate) fn prepare_practice(gameplay: GameplayState, init: PracticeInit) -> PracticeState {
    gameplay.into_practice(init.editor, |stage, presentation| {
        practice::PracticePresentation::new(stage, presentation, init.hud)
    })
}

fn smx_sensor_value(data: &deadsync_smx::SensorTestData, panel: usize, fsr: bool) -> Option<u16> {
    data.have_data_from_panel[panel].then(|| {
        if fsr {
            data.sensor_level[panel]
                .iter()
                .map(|&value| if value <= 0 { 0 } else { (value >> 2) as u16 })
                .max()
                .unwrap_or(0)
        } else {
            data.sensor_level[panel]
                .iter()
                .map(|&value| value.clamp(0, 500) as u16)
                .max()
                .unwrap_or(0)
        }
    })
}

fn smx_sensor_view(
    config: &deadsync_smx::SmxConfig,
    data: Option<&deadsync_smx::SensorTestData>,
) -> SmxSensorPadView {
    let fsr = deadsync_smx::is_fsr(config);
    let panels = std::array::from_fn(|panel| {
        let threshold = if fsr {
            config.panel_settings[panel]
                .fsr_high_threshold
                .iter()
                .copied()
                .map(u16::from)
                .max()
                .unwrap_or(0)
        } else {
            u16::from(config.panel_settings[panel].load_cell_high_threshold)
        };
        SmxSensorPanelView {
            threshold,
            value: data.and_then(|data| smx_sensor_value(data, panel, fsr)),
        }
    });
    SmxSensorPadView { fsr, panels }
}

fn update_smx_sensor_view(
    mut view: SmxSensorPadView,
    data: Option<&deadsync_smx::SensorTestData>,
) -> SmxSensorPadView {
    for (panel, panel_view) in view.panels.iter_mut().enumerate() {
        panel_view.value = data.and_then(|data| smx_sensor_value(data, panel, view.fsr));
    }
    view
}

// The SDK streams at about 30 Hz. Read at 60 Hz to bound lock traffic even
// on high-refresh frames, and retain at most one unconsumed sample interval.
const SMX_SAMPLE_INTERVAL: f32 = 1.0 / 60.0;

pub(crate) struct GameplayTelemetryCursor {
    smx_elapsed: f32,
    smx_views: [Option<SmxSensorPadView>; MAX_PLAYERS],
    heart_rate_generation: (u64, u64),
}

impl Default for GameplayTelemetryCursor {
    fn default() -> Self {
        Self {
            smx_elapsed: SMX_SAMPLE_INTERVAL,
            smx_views: [None; MAX_PLAYERS],
            heart_rate_generation: (u64::MAX, u64::MAX),
        }
    }
}

impl GameplayTelemetryCursor {
    fn smx_refresh_due(&mut self, delta_time: f32) -> bool {
        self.smx_elapsed += delta_time;
        if self.smx_elapsed < SMX_SAMPLE_INTERVAL {
            return false;
        }
        self.smx_elapsed = (self.smx_elapsed - SMX_SAMPLE_INTERVAL).min(SMX_SAMPLE_INTERVAL);
        true
    }
}

/// Pairs of snapshot slot and SDK pad. Singles use profile slot zero even
/// on the right pad, while Doubles expose both physical pads to one player.
fn smx_sensor_pad_plan(
    play_style: GameplayInputPlayStyle,
    side: GameplayInputPlayerSide,
    num_players: usize,
    fsr_display: [bool; MAX_PLAYERS],
    smx_input: bool,
) -> [Option<(usize, usize)>; MAX_PLAYERS] {
    if !smx_input || !fsr_display.into_iter().any(|enabled| enabled) {
        return [None; MAX_PLAYERS];
    }
    if play_style.is_double() {
        return [fsr_display[0].then_some((0, 0)), fsr_display[0].then_some((1, 1))];
    }
    if num_players >= MAX_PLAYERS {
        return std::array::from_fn(|idx| fsr_display[idx].then_some((idx, idx)));
    }
    let sdk_pad = usize::from(side == GameplayInputPlayerSide::P2);
    [(num_players == 1 && fsr_display[0]).then_some((0, sdk_pad)), None]
}

fn stage_smx_sensor_pad_plan(
    stage: &ActiveGameplayStage,
    smx_input: bool,
) -> [Option<(usize, usize)>; MAX_PLAYERS] {
    let session = &stage.gameplay.setup.session;
    smx_sensor_pad_plan(
        session.play_style,
        session.player_side,
        stage.num_players(),
        stage.profiles().each_ref().map(|profile| profile.smx_fsr_display),
        smx_input,
    )
}

fn enter_smx_sensors<P: GameplayLivePresentation<GameplayCoreState>>(
    state: &mut GameplayHost<GameplayLobbySession, P>,
    cursor: &mut GameplayTelemetryCursor,
    smx_input: bool,
) {
    let plan = stage_smx_sensor_pad_plan(&state.stage, smx_input);
    for (store_idx, sdk_pad) in plan.into_iter().flatten() {
        deadsync_smx::set_test_mode(sdk_pad, deadsync_smx::SensorTestMode::CalibratedValues);
        cursor.smx_views[store_idx] = deadsync_smx::get_config(sdk_pad)
            .as_ref()
            .map(|config| smx_sensor_view(config, None));
    }
    state.presentation.sync_smx_sensors(cursor.smx_views);
}

fn refresh_smx_sensors<P: GameplayLivePresentation<GameplayCoreState>>(
    state: &mut GameplayHost<GameplayLobbySession, P>,
    cursor: &mut GameplayTelemetryCursor,
    delta_time: f32,
    smx_input: bool,
) {
    if !cursor.smx_refresh_due(delta_time) {
        return;
    }
    let plan = stage_smx_sensor_pad_plan(&state.stage, smx_input);
    if plan.iter().all(Option::is_none) {
        return;
    }
    for (store_idx, sdk_pad) in plan.into_iter().flatten() {
        let data = deadsync_smx::get_test_data(sdk_pad);
        cursor.smx_views[store_idx] = if let Some(view) = cursor.smx_views[store_idx] {
            Some(update_smx_sensor_view(view, data.as_ref()))
        } else {
            deadsync_smx::get_config(sdk_pad)
                .as_ref()
                .map(|config| smx_sensor_view(config, data.as_ref()))
        };
    }
    state.presentation.sync_smx_sensors(cursor.smx_views);
}

pub(crate) fn exit<P: GameplayLivePresentation<GameplayCoreState>>(
    state: &mut GameplayHost<GameplayLobbySession, P>,
    cursor: &mut GameplayTelemetryCursor,
) {
    // Always clear both pads, including one whose config was unavailable on
    // enter. Otherwise the SDK keeps streaming sensor data after gameplay.
    for pad in 0..2 {
        deadsync_smx::set_test_mode(pad, deadsync_smx::SensorTestMode::Off);
    }
    cursor.smx_views = [None; MAX_PLAYERS];
    state.presentation.sync_smx_sensors(cursor.smx_views);
}

#[inline(always)]
const fn audio_cut(cut: GameplayMusicCut) -> deadlib_audio::stream::Cut {
    deadlib_audio::stream::Cut {
        start_sec: cut.start_sec,
        length_sec: cut.length_sec,
        fade_in_sec: cut.fade_in_sec,
        fade_out_sec: cut.fade_out_sec,
    }
}

pub(crate) fn snapshot(
    music_clock: &mut deadsync_audio_stream::MusicClock,
) -> GameplayAudioSnapshot {
    let stream_clock = music_clock.snapshot();
    let output_timing = deadsync_audio_stream::get_output_timing_snapshot();
    GameplayAudioSnapshot {
        stream_clock: GameplayStreamClockSnapshot {
            stream_seconds: stream_clock.stream_seconds,
            music_nanos: stream_clock.music_nanos,
            music_seconds_per_second: stream_clock.music_seconds_per_second,
            has_music_mapping: stream_clock.has_music_mapping,
            valid_at: stream_clock.valid_at,
            valid_at_host_nanos: stream_clock.valid_at_host_nanos,
        },
        assist_sfx_generation: deadsync_audio_stream::assist_sfx_generation(),
        output_delay_seconds: output_timing.estimated_output_delay_ns as f32 * 1e-9,
        timing_diag_enabled: deadsync_audio_stream::timing_diag_enabled(),
        timing_diag_callback_gap_ns: deadsync_audio_stream::timing_diag_last_callback_gap_ns(),
    }
}

pub(crate) fn drain_core(
    state: &mut deadsync_song_lua::playback::GameplayCoreState<
        deadsync_profile_gameplay::GameplayProfile,
        deadsync_assets::noteskin::SpriteSlot,
    >,
    audio: &mut deadsync_audio_stream::AudioControl,
    sounds: &crate::gameplay_prewarm::GameplaySfx,
    music_clock: &mut deadsync_audio_stream::MusicClock,
) {
    for command in state.drain_audio_commands() {
        match command {
            GameplayAudioCommand::StopMusic => {
                audio.stop_music();
            }
            GameplayAudioCommand::PauseMusic(paused) => audio.pause_music(paused),
            GameplayAudioCommand::SetMusicRate(rate) => {
                audio.set_music_rate(rate);
            }
            GameplayAudioCommand::PlayMusic {
                path,
                cut,
                looping,
                rate,
            } => audio.play_music(path, audio_cut(cut), looping, rate),
            GameplayAudioCommand::PlayPreloadedMineSfx { path, player, two_players } => {
                if let Some(sound) = sounds.resolve(path) {
                    audio.play_mine_sfx(sound, player, two_players);
                }
            }
            GameplayAudioCommand::PlayPreloadedAssistTick(path) => {
                if let Some(sound) = sounds.resolve(path) {
                    audio.play_resolved_assist_tick(sound);
                }
            }
            GameplayAudioCommand::PlayAssistTickAtMusicTime {
                path,
                music_seconds,
            } => {
                if let Some(sound) = sounds.resolve(path) {
                    if let Some(frame) = music_clock.assist_tick_stream_frame(music_seconds) {
                        audio.play_scheduled_assist_tick(sound, frame);
                    } else {
                        audio.play_resolved_assist_tick(sound);
                    }
                }
            }
        }
    }

    for command in state.drain_session_commands() {
        match command {
            GameplaySessionCommand::SetTimingTickMode(mode) => {
                deadsync_profile::compat::set_session_timing_tick_mode(
                    profile_tick_mode_from_gameplay(mode),
                );
            }
        }
    }
}

#[inline(always)]
pub(crate) fn drain<P>(
    state: &mut GameplayHost<GameplayLobbySession, P>,
    audio: &mut deadsync_audio_stream::AudioControl,
    sounds: &crate::gameplay_prewarm::GameplaySfx,
    music_clock: &mut deadsync_audio_stream::MusicClock,
) {
    drain_core(&mut state.stage.gameplay, audio, sounds, music_clock);
}

pub(crate) fn enter<P: GameplayLivePresentation<GameplayCoreState>>(
    state: &mut GameplayHost<GameplayLobbySession, P>,
    telemetry: &mut GameplayTelemetryCursor,
    smx_input: bool,
    audio: &mut deadsync_audio_stream::AudioControl,
    sounds: &crate::gameplay_prewarm::GameplaySfx,
    music_clock: &mut deadsync_audio_stream::MusicClock,
) {
    let (stage, lobby) = state.stage_and_lobby();
    lobby.on_enter(&mut stage.gameplay);
    *telemetry = GameplayTelemetryCursor::default();
    enter_smx_sensors(state, telemetry, smx_input);
    drain(state, audio, sounds, music_clock);
}

const MAX_UPDATE_EFFECTS: usize = 6;

pub(crate) enum GameplayFrameEffect<S = ThemeScreen> {
    Lobby(GameplayLobbyAction),
    Session(SongLuaGameplayEffect<S>),
}

pub(crate) type GameplayFrameEffects<S = ThemeScreen> = SmallVec<[GameplayFrameEffect<S>; MAX_UPDATE_EFFECTS]>;

pub(crate) fn update<T, P>(
    state: &mut GameplayHost<GameplayLobbySession, P>,
    delta_time: f32,
    smx_input: bool,
    audio: &mut deadsync_audio_stream::AudioControl,
    sounds: &crate::gameplay_prewarm::GameplaySfx,
    music_clock: &mut deadsync_audio_stream::MusicClock,
    score_cursor: &mut ScoreRuntimeCursor,
    telemetry: &mut GameplayTelemetryCursor,
) -> GameplayFrameEffects<T::Screen>
where
    T: ThemeScreenFlow,
    P: GameplayLivePresentation<GameplayCoreState>,
{
    let mut effects = GameplayFrameEffects::<T::Screen>::new();
    crate::heart_rate::refresh_gameplay(
        &mut state.presentation,
        &mut telemetry.heart_rate_generation,
    );
    let (stage, lobby) = state.stage_and_lobby();
    if !lobby.prepare_update(&mut stage.gameplay, |action| {
        effects.push(GameplayFrameEffect::Lobby(action));
    }) {
        debug_assert!(effects.len() <= MAX_UPDATE_EFFECTS);
        return effects;
    }

    refresh_smx_sensors(state, telemetry, delta_time, smx_input);

    // A lobby can queue stage music during `prepare_update`. Execute it before
    // taking the clock snapshot so the deterministic update sees the new
    // stream mapping on the same frame, matching the pre-boundary behavior.
    drain(state, audio, sounds, music_clock);
    let previous_song_lua_time = state.current_music_time_display();
    let stage = &mut state.stage;
    let presentation = &mut state.presentation;
    stage.update(
        delta_time,
        snapshot(music_clock),
        deadlib_platform::host_time::now_nanos,
        crate::navigation::song_lua_presentation_screen::<T>,
        |effect| effects.push(GameplayFrameEffect::Session(effect)),
        |game, delta| presentation.update_gameplay_visuals(game, delta),
    );
    let output = stage.take_output();
    for command in output.actor_sounds {
        use deadsync_song_lua::SongLuaActorSound;
        match command {
            SongLuaActorSound::Play { owner, path } => {
                // Most assets are prewarmed. A callback may also author a new
                // path at runtime; do not silently discard that valid sound.
                let sound = sounds.resolve_path(&path).cloned().or_else(|| audio.prepare_sfx(&path));
                if let Some(sound) = sound { audio.play_actor_sfx(owner, &sound); }
            }
            SongLuaActorSound::Stop { owner } => audio.stop_actor_sfx(owner),
            SongLuaActorSound::Pause { owner, paused } => audio.pause_actor_sfx(owner, paused),
        }
    }
    for error in output.errors { log::warn!("Song Lua: {error}"); }
    for event in output.sounds {
        if let Some(sound) = sounds.resolve_path(&event.path) {
            audio.play_sfx(sound);
        } else {
            if let Some(sound) = audio.prepare_sfx(&event.path) { audio.play_sfx(&sound); }
        }
    }
    if let Some(request) = output.request {
        match request {
            deadsync_song_lua::SongLuaLiveRequest::Transition(message) if message == "SM_DoNextScreen" => {
                // The game session advances the native out clock,
                // dispatches End and navigates after its completion frame.
            }
            deadsync_song_lua::SongLuaLiveRequest::NewScreen(screen) if screen == "ScreenGameplay" => {
                // The game session emits ReloadSongLua at the next update boundary.
            }
            request => log::warn!("Unsupported live song Lua screen request: {request:?}"),
        }
    }
    score_cursor.sync_if_dirty(state);
    drain(state, audio, sounds, music_clock);
    state
        .stage
        .for_each_scheduled_song_sound(previous_song_lua_time, |path| {
            if let Some(sound) = sounds.resolve_path(path) {
                audio.play_sfx(sound);
            }
        });
    debug_assert!(effects.len() <= MAX_UPDATE_EFFECTS);
    effects
}

pub(crate) fn handle_input<P: GameplayLivePresentation<GameplayCoreState>>(
    state: &mut GameplayHost<GameplayLobbySession, P>,
    ev: &InputEvent,
    physical: Option<deadsync_input::PhysicalInput>,
    audio: &mut deadsync_audio_stream::AudioControl,
    sounds: &crate::gameplay_prewarm::GameplaySfx,
    music_clock: &mut deadsync_audio_stream::MusicClock,
) -> GameplayAction {
    let (stage, lobby) = state.stage_and_lobby();
    let action = match lobby.route_input(ev) {
        GameplayLobbyInput::PassThrough => stage.handle_input(ev, physical),
        GameplayLobbyInput::Consumed => GameplayAction::None,
        GameplayLobbyInput::ToggleHud => {
            state.presentation.toggle_lobby_hud();
            GameplayAction::None
        }
    };
    drain(state, audio, sounds, music_clock);
    action
}

pub(crate) fn update_practice(
    state: &mut PracticeState,
    delta_time: f32,
    audio: &mut deadsync_audio_stream::AudioControl,
    sounds: &crate::gameplay_prewarm::GameplaySfx,
    music_clock: &mut deadsync_audio_stream::MusicClock,
    score_cursor: &mut ScoreRuntimeCursor,
    effects: &mut Vec<ThemeEffect>,
) {
    let audio_snapshot = snapshot(music_clock);
    practice::advance_flash(&mut state.presentation, delta_time);
    let mut feedback = state.presentation.editor_feedback();
    let playback = state.session.update(
        &mut state.gameplay.stage.gameplay,
        delta_time,
        audio_snapshot,
        deadlib_platform::host_time::now_nanos,
        deadlib_audio::stream::snap_music_start_sec,
        |game, event| feedback.emit(game, event),
    );
    feedback.finish(&state.gameplay.stage, &mut state.gameplay.presentation);
    practice::apply_playback_update(&mut state.presentation, playback);
    score_cursor.sync_if_dirty(&mut state.gameplay);
    drain(&mut state.gameplay, audio, sounds, music_clock);
    apply_practice_output(state, ThemeEffect::None, effects);
}

pub(crate) fn enter_practice(
    state: &mut PracticeState,
    audio: &mut deadsync_audio_stream::AudioControl,
    sounds: &crate::gameplay_prewarm::GameplaySfx,
    music_clock: &mut deadsync_audio_stream::MusicClock,
) {
    state.session.on_enter(&mut state.gameplay.stage.gameplay);
    drain(&mut state.gameplay, audio, sounds, music_clock);
}

pub(crate) fn handle_practice_input(
    state: &mut PracticeState,
    ev: &InputEvent,
    audio: &mut deadsync_audio_stream::AudioControl,
    sounds: &crate::gameplay_prewarm::GameplaySfx,
    music_clock: &mut deadsync_audio_stream::MusicClock,
    effects: &mut Vec<ThemeEffect>,
) {
    let mut feedback = state.presentation.editor_feedback();
    let request = state.session.handle_input(
        &mut state.gameplay.stage.gameplay,
        ev,
        deadlib_audio::stream::snap_music_start_sec,
        |game, event| feedback.emit(game, event),
    );
    feedback.finish(&state.gameplay.stage, &mut state.gameplay.presentation);
    let effect = practice_request_effect(&mut state.presentation, request);
    drain(&mut state.gameplay, audio, sounds, music_clock);
    apply_practice_output(state, effect, effects);
}

pub(crate) fn handle_practice_raw_key(
    state: &mut PracticeState,
    ev: &RawKeyboardEvent,
    audio: &mut deadsync_audio_stream::AudioControl,
    sounds: &crate::gameplay_prewarm::GameplaySfx,
    music_clock: &mut deadsync_audio_stream::MusicClock,
    effects: &mut Vec<ThemeEffect>,
) -> bool {
    let mut feedback = state.presentation.editor_feedback();
    let (consumed, request) = state.session.handle_raw_key(
        &mut state.gameplay.stage.gameplay,
        &PracticeKeyEvent::from_raw(ev),
        deadlib_audio::stream::snap_music_start_sec,
        |game, event| feedback.emit(game, event),
    );
    feedback.finish(&state.gameplay.stage, &mut state.gameplay.presentation);
    let effect = practice_request_effect(&mut state.presentation, request);
    drain(&mut state.gameplay, audio, sounds, music_clock);
    apply_practice_output(state, effect, effects) || consumed
}

fn practice_request_effect(
    presentation: &mut practice::PracticePresentation,
    request: PracticeViewRequest,
) -> ThemeEffect {
    practice::apply_view_request(presentation, request);
    match request {
        PracticeViewRequest::Navigate(PracticeDestination::PlayerOptions) => {
            ThemeEffect::Navigate(ActiveTheme::SCREENS.player_options)
        }
        PracticeViewRequest::Navigate(PracticeDestination::SelectMusic) => {
            ThemeEffect::Navigate(ActiveTheme::SCREENS.song_selection)
        }
        _ => ThemeEffect::None,
    }
}

fn apply_practice_output(
    state: &mut PracticeState,
    effect: ThemeEffect,
    effects: &mut Vec<ThemeEffect>,
) -> bool {
    let rates = state.session.drain_music_rates();
    let changed_rate = rates.len() != 0;
    for rate in rates {
        profile::set_session_music_rate(rate);
    }
    let start_len = effects.len();
    practice::append_presentation_effects(&mut state.presentation, effect, effects);
    changed_rate || effects.len() != start_len
}

pub(crate) fn push_actors<L, P: GameplayStagePresenter<L>>(
    actors: &mut Vec<deadlib_present::actors::Actor>,
    state: &mut GameplayHost<L, P>,
    assets: &deadlib_assets::AssetManager,
    view: P::View,
    uptime_seconds: f64,
    policy: P::Policy,
) -> deadsync_song_lua::playback::GameplayActorSegments {
    state.compose_gameplay_frame(
        actors,
        FramePresentationContext {
            assets,
            view,
            uptime_seconds,
            policy,
        },
        SongFrameContent::PLAYBACK,
    )
}

pub(crate) fn push_practice_actors<L, P, Q>(
    actors: &mut Vec<deadlib_present::actors::Actor>,
    state: &mut PracticeHost<GameplayHost<L, P>, Q>,
    assets: &deadlib_assets::AssetManager,
    arrow_effect_time_s: f64,
    visual_policy: P::Policy,
) -> deadsync_song_lua::playback::GameplayActorSegments
where
    P: GameplayStagePresenter<L>,
    Q: PracticeStagePresenter<L, P>,
{
    actors.reserve(128);
    let view = state.presentation.frame_view(&state.session);
    let segments = state.gameplay.compose_gameplay_frame(
        actors,
        FramePresentationContext {
            assets,
            view,
            uptime_seconds: arrow_effect_time_s,
            policy: visual_policy,
        },
        SongFrameContent::for_practice(&state.session),
    );
    state
        .presentation
        .append_practice_overlay(actors, &state.gameplay, &state.session);
    segments
}

#[cfg(test)]
mod tests {
    use super::{
        GameplayTelemetryCursor, SMX_SAMPLE_INTERVAL, mini_indicator_percent,
        mini_indicator_rival_score, policy_view, score_generation_changed, scorebox_profiles,
        smx_sensor_pad_plan, smx_sensor_value,
    };
    use deadsync_gameplay::GameplaySession;
    use deadsync_profile as profile_data;
    use deadsync_profile_gameplay::gameplay_runtime_profile_data;

    fn leaderboard_entry(
        rank: u32,
        score: f64,
        is_self: bool,
        is_rival: bool,
    ) -> deadsync_score::LeaderboardEntry {
        deadsync_score::LeaderboardEntry {
            rank,
            name: format!("player-{rank}"),
            machine_tag: None,
            score,
            date: String::new(),
            is_rival,
            is_self,
            is_fail: false,
        }
    }

    fn leaderboard_snapshot(
        panes: Vec<deadsync_score::LeaderboardPane>,
    ) -> deadsync_score::CachedPlayerLeaderboardData {
        deadsync_score::CachedPlayerLeaderboardData::ready(deadsync_score::PlayerLeaderboardData {
            panes,
            srpg_self_score: None,
            itl_self_score: None,
            itl_self_rank: None,
        })
    }

    #[test]
    fn score_refresh_waits_for_a_new_completion_generation() {
        assert!(!score_generation_changed(Some(7), 7));
        assert!(score_generation_changed(Some(7), 8));
        assert!(score_generation_changed(None, 7));
    }

    #[test]
    fn smx_sensor_plan_uses_stage_style_and_physical_side() {
        use deadsync_gameplay::{GameplayInputPlayStyle as Style, GameplayInputPlayerSide as Side};

        for (style, side, players, display, enabled, expected) in [
            (Style::Single, Side::P2, 1, [true, false], true, [Some((0, 1)), None]),
            (Style::Double, Side::P2, 1, [true, false], true, [Some((0, 0)), Some((1, 1))]),
            (Style::Routine, Side::P2, 2, [true, true], true, [Some((0, 0)), Some((1, 1))]),
            (Style::Versus, Side::P1, 2, [false, true], true, [None, Some((1, 1))]),
            (Style::Double, Side::P1, 1, [false, true], true, [None, None]),
            (Style::Single, Side::P2, 1, [true, false], false, [None, None]),
        ] {
            assert_eq!(smx_sensor_pad_plan(style, side, players, display, enabled), expected);
        }
    }

    #[test]
    fn smx_sensor_sampling_retains_fractional_time_and_caps_catch_up() {
        let mut cursor = GameplayTelemetryCursor::default();
        assert!(cursor.smx_refresh_due(0.0));
        assert!(!cursor.smx_refresh_due(SMX_SAMPLE_INTERVAL / 2.0));
        assert!(cursor.smx_refresh_due(SMX_SAMPLE_INTERVAL / 2.0));
        assert!(cursor.smx_refresh_due(10.0));
        assert!(cursor.smx_refresh_due(0.0));
        assert!(!cursor.smx_refresh_due(0.0));
    }

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
        let policy = policy_view(&config);

        assert_eq!(policy.machine_font, config.machine_font);
        assert_eq!(policy.background_brightness, config.bg_brightness);
        assert_eq!(policy.background_color, config.gameplay_bg_color);
        assert_eq!(policy.smx_input, config.smx_input);
        assert_eq!(policy.zmod_rating_box_text, config.zmod_rating_box_text);
        assert_eq!(policy.show_bpm_decimal, config.show_bpm_decimal);
        assert_eq!(policy.bpm_position, config.gameplay_bpm_position);
    }

    #[test]
    fn p2_solo_scorebox_uses_runtime_profile_identity() {
        for play_style in [
            profile_data::PlayStyle::Single,
            profile_data::PlayStyle::Double,
        ] {
            let mut profiles: [profile_data::Profile; 2] =
                std::array::from_fn(|_| deadsync_profile::Profile::default());
            profiles[0].display_scorebox = false;
            profiles[1].display_scorebox = true;
            profiles[1].show_ex_score = true;
            profiles[1].groovestats_username = "p2-user".to_owned();
            let session = GameplaySession {
                play_style: deadsync_profile_gameplay::gameplay_play_style_from_profile(play_style),
                player_side: deadsync_profile_gameplay::gameplay_player_side_from_profile(
                    profile_data::PlayerSide::P2,
                ),
                joined_sides: [false, true],
                active_profile_ids: [None, Some("p2-profile".to_owned())],
                ..Default::default()
            };
            let runtime_profiles = gameplay_runtime_profile_data(&profiles, &session);
            let snapshots = scorebox_profiles(&runtime_profiles, &session);
            let p1 = profile_data::player_side_index(profile_data::PlayerSide::P1);
            let p2 = profile_data::player_side_index(profile_data::PlayerSide::P2);

            assert!(!snapshots[p1].display_scorebox);
            assert!(snapshots[p2].display_scorebox);
            assert!(snapshots[p2].show_ex_score);
            assert_eq!(snapshots[p2].gs_username(), "p2-user");
            assert_eq!(snapshots[p2].persistent_profile_id(), Some("p2-profile"));
        }
    }

    #[test]
    fn rival_score_prefers_marked_player_then_world_record() {
        let pane = deadsync_score::leaderboard_pane(
            "GrooveStats",
            vec![
                leaderboard_entry(1, 9_990.0, false, false),
                leaderboard_entry(2, 9_750.0, false, true),
            ],
            false,
        )
        .expect("non-empty pane");
        let snapshot = leaderboard_snapshot(vec![pane.clone()]);
        assert_eq!(
            mini_indicator_rival_score(Some(&snapshot), profile_data::MiniIndicatorScoreType::Itg,),
            Some(97.5),
        );

        let mut pane_without_rival = pane;
        for entry in &mut pane_without_rival.entries {
            entry.is_rival = false;
        }
        let snapshot = leaderboard_snapshot(vec![pane_without_rival]);
        assert_eq!(
            mini_indicator_rival_score(Some(&snapshot), profile_data::MiniIndicatorScoreType::Itg,),
            Some(99.9),
        );
    }

    #[test]
    fn mini_indicator_target_percent_rejects_missing_scores() {
        assert_eq!(mini_indicator_percent(91.23), Some(91.23));
        assert_eq!(mini_indicator_percent(101.0), Some(100.0));
        assert_eq!(mini_indicator_percent(0.0), None);
        assert_eq!(mini_indicator_percent(-1.0), None);
        assert_eq!(mini_indicator_percent(f64::NAN), None);
        assert_eq!(mini_indicator_percent(f64::INFINITY), None);
    }

    #[test]
    fn zero_personal_best_uses_s_pacemaker_target() {
        let target = deadsync_gameplay::resolve_target_score_percent(
            deadsync_gameplay::GameplayTargetScoreSetting::PersonalBest,
            100.0,
            mini_indicator_percent(0.0),
            None,
        );
        let output = deadsync_notefield::zmod_mini_indicator_output(
            &deadsync_notefield::MiniIndicatorProgress {
                judged_any: true,
                current_score_percent: 0.5,
                current_possible_ratio: 0.005,
                ..Default::default()
            },
            deadsync_notefield::ZmodMiniIndicatorParams {
                mode: deadsync_notefield::MiniIndicatorMode::Pacemaker,
                color_style: deadsync_notefield::MiniIndicatorColorStyle::Default,
                subtractive_display:
                    deadsync_notefield::MiniIndicatorSubtractiveDisplay::CountThenPercent,
                score_type: deadsync_notefield::MiniIndicatorScoreType::Itg,
                combo_color: [1.0; 4],
                is_failing: false,
                life: 1.0,
                rival_score_percent: 0.0,
                target_score_percent: target,
                stream_completion: None,
            },
        );

        assert_eq!(target, 92.0);
        assert_eq!(
            output.map(|output| output.text),
            Some(deadsync_notefield::ZmodMiniIndicatorText::SignedPercent {
                value: 0.04,
                negative: false,
            })
        );
    }

    #[test]
    fn rival_score_uses_matching_score_type() {
        let itg = deadsync_score::leaderboard_pane(
            "GrooveStats",
            vec![leaderboard_entry(1, 9_900.0, true, false)],
            false,
        )
        .expect("non-empty pane");
        let ex = deadsync_score::leaderboard_pane(
            "EX",
            vec![leaderboard_entry(1, 9_500.0, true, false)],
            true,
        )
        .expect("non-empty pane");
        let hard_ex = deadsync_score::arrowcloud_hard_ex_leaderboard_pane(
            vec![leaderboard_entry(1, 9_000.0, true, false)],
            true,
        );
        let snapshot = leaderboard_snapshot(vec![itg, ex, hard_ex]);

        assert_eq!(
            mini_indicator_rival_score(Some(&snapshot), profile_data::MiniIndicatorScoreType::Itg,),
            Some(99.0),
        );
        assert_eq!(
            mini_indicator_rival_score(Some(&snapshot), profile_data::MiniIndicatorScoreType::Ex,),
            Some(95.0),
        );
        assert_eq!(
            mini_indicator_rival_score(
                Some(&snapshot),
                profile_data::MiniIndicatorScoreType::HardEx,
            ),
            Some(90.0),
        );
    }

    #[test]
    fn smx_sensor_value_matches_fsr_calibration() {
        let mut data = deadsync_smx::SensorTestData::default();
        data.have_data_from_panel[3] = true;
        data.sensor_level[3] = [-4, 400, 996, 1000];

        assert_eq!(smx_sensor_value(&data, 3, true), Some(250));
    }

    #[test]
    fn smx_sensor_value_matches_load_cell_clamping() {
        let mut data = deadsync_smx::SensorTestData::default();
        data.have_data_from_panel[7] = true;
        data.sensor_level[7] = [-4, 249, 500, 800];

        assert_eq!(smx_sensor_value(&data, 7, false), Some(500));
    }

    #[test]
    fn smx_sensor_value_hides_panels_without_data() {
        let data = deadsync_smx::SensorTestData::default();

        assert_eq!(smx_sensor_value(&data, 1, true), None);
    }
}
