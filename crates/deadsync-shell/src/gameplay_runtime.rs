use deadsync_chart::{ChartData, GameplayChartData, SongBackgroundChange, SongData};
use deadsync_config::prelude as config;
use deadsync_gameplay::{
    GameplayAudioCommand, GameplayAudioSnapshot, GameplayMiniIndicatorData, GameplayMusicCut,
    GameplaySession, GameplaySessionCommand, GameplayStreamClockSnapshot, gameplay_runtime_charts,
};
use deadsync_input::{InputEvent, RawKeyboardEvent};
use deadsync_online::score_compat as scores;
use deadsync_profile as profile_data;
use deadsync_profile::compat as profile;
use deadsync_profile_gameplay::{
    gameplay_runtime_profile_data, profile_side_from_gameplay, profile_tick_mode_from_gameplay,
};
use deadsync_theme_simply_love::SimplyLoveEffect as ThemeEffect;
use deadsync_theme_simply_love::screens::{gameplay, practice};
use deadsync_theme_simply_love::views::{
    GameplayInitView, GameplayPolicyView, GameplayRuntimeView, GameplayScoreInitView,
    GameplayScoreRuntimeView, PracticeRuntimeView, SimplyLoveLobbyRuntimeView,
};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

const GAMEPLAY_SCOREBOX_ENTRIES: usize = 5;

fn smx_profile_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("DEADSYNC_SMX_PROFILE").is_ok_and(|value| !value.is_empty() && value != "0")
    })
}

fn policy_view(config: &config::Config) -> GameplayPolicyView {
    GameplayPolicyView {
        translated_titles: config.translated_titles,
        center_single_notefield: config.center_1player_notefield,
        background_brightness: config.bg_brightness,
        background_color: config.gameplay_bg_color,
        smx_input: config.smx_input,
        zmod_rating_box_text: config.zmod_rating_box_text,
        show_bpm_decimal: config.show_bpm_decimal,
        bpm_position: config.gameplay_bpm_position,
        machine_font: config.machine_font,
        scorebox_pane_filter: deadsync_score::SelectMusicScoreboxFilter {
            itg: config.select_music_scorebox_cycle_itg,
            ex: config.select_music_scorebox_cycle_ex,
            hard_ex: config.select_music_scorebox_cycle_hard_ex,
            tournaments: config.select_music_scorebox_cycle_tournaments,
        },
        srpg10_scorebox: matches!(config.srpg_variant, config::SrpgVariant::Srpg10)
            && config.visual_style.is_srpg(),
        smx_profile_enabled: smx_profile_enabled(),
    }
}

fn background_changes(
    config: &config::Config,
    song: &SongData,
    gameplay_charts: &[Arc<GameplayChartData>; 2],
    session: &GameplaySession,
) -> Vec<SongBackgroundChange> {
    let random_movie_paths = deadsync_simfile::app_runtime::random_movie_paths(
        song,
        matches!(
            config.random_background_mode,
            config::RandomBackgroundMode::RandomMovies
        ),
    );
    let chart = background_chart(gameplay_charts, session);
    deadsync_simfile::app_runtime::gameplay_background_changes(song, chart, random_movie_paths)
}

fn background_chart<'a>(
    gameplay_charts: &'a [Arc<GameplayChartData>; 2],
    session: &GameplaySession,
) -> &'a GameplayChartData {
    if session.p2_runtime_player() {
        &gameplay_charts[1]
    } else {
        &gameplay_charts[0]
    }
}

pub(crate) fn runtime_view(
    config: &config::Config,
    lobby: SimplyLoveLobbyRuntimeView,
) -> GameplayRuntimeView {
    let session = profile::get_session_snapshot();
    GameplayRuntimeView {
        policy: policy_view(config),
        play_style: session.play_style,
        player_side: session.player_side,
        joined: std::array::from_fn(|idx| {
            session.side_joined(profile_data::player_side_for_index(idx))
        }),
        lobby,
    }
}

fn mini_indicator_personal_best(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    score_type: profile_data::MiniIndicatorScoreType,
) -> Option<f64> {
    match score_type {
        profile_data::MiniIndicatorScoreType::Itg => {
            scores::get_cached_score_for_side(chart_hash, side)
                .map(|score| (score.score_percent * 100.0).clamp(0.0, 100.0))
        }
        profile_data::MiniIndicatorScoreType::Ex => {
            scores::get_cached_local_ex_score_for_side(chart_hash, side)
                .map(|score| score.percent.clamp(0.0, 100.0))
        }
        profile_data::MiniIndicatorScoreType::HardEx => {
            scores::get_cached_local_hard_ex_score_for_side(chart_hash, side)
                .map(|score| score.percent.clamp(0.0, 100.0))
        }
    }
}

fn mini_indicator_machine_best(
    chart_hash: &str,
    score_type: profile_data::MiniIndicatorScoreType,
) -> Option<f64> {
    match score_type {
        profile_data::MiniIndicatorScoreType::Itg => scores::get_machine_record_local(chart_hash)
            .map(|(_, score)| (score.score_percent * 100.0).clamp(0.0, 100.0)),
        profile_data::MiniIndicatorScoreType::Ex | profile_data::MiniIndicatorScoreType::HardEx => {
            None
        }
    }
}

fn mini_indicator_view(
    charts: &[Arc<ChartData>; 2],
    profiles: &[profile_data::Profile; 2],
    session: &GameplaySession,
) -> GameplayMiniIndicatorData {
    let mut view = GameplayMiniIndicatorData::default();
    for player in 0..session.play_style.player_count() {
        let side = profile_side_from_gameplay(session.runtime_player_side(player));
        let chart_hash = charts[player].short_hash.as_str();
        let score_type = profiles[player].mini_indicator_score_type;
        view.personal_best_percent[player] =
            mini_indicator_personal_best(chart_hash, side, score_type);
        view.machine_best_percent[player] = mini_indicator_machine_best(chart_hash, score_type);
    }
    view
}

fn scorebox_profiles(
    profiles: &[profile_data::Profile; 2],
    session: &GameplaySession,
) -> [deadsync_score::GameplayScoreboxProfileSnapshot; 2] {
    let mut snapshots = std::array::from_fn(|_| Default::default());
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

fn score_init_view(
    charts: &[Arc<ChartData>; 2],
    profiles: &[profile_data::Profile; 2],
    session: &GameplaySession,
) -> GameplayScoreInitView {
    let runtime_charts = gameplay_runtime_charts(charts, session);
    let runtime_profiles = gameplay_runtime_profile_data(profiles, session);
    let scorebox_profiles = scorebox_profiles(&runtime_profiles, session);
    let mut scorebox_snapshots = std::array::from_fn(|_| None);
    for player in 0..session.play_style.player_count() {
        let side = profile_side_from_gameplay(session.runtime_player_side(player));
        let side_idx = profile_data::player_side_index(side);
        let profile = &scorebox_profiles[side_idx];
        let chart_hash = runtime_charts[player].short_hash.trim();
        if profile.display_scorebox && profile.gs_active && !chart_hash.is_empty() {
            scorebox_snapshots[side_idx] = scores::get_or_fetch_player_leaderboards_for_profile(
                chart_hash,
                profile,
                GAMEPLAY_SCOREBOX_ENTRIES,
            );
        }
    }
    GameplayScoreInitView {
        mini_indicator: mini_indicator_view(&runtime_charts, &runtime_profiles, session),
        scorebox_profiles,
        scorebox_snapshots,
    }
}

pub(crate) fn init_view(
    config: &config::Config,
    lobby: SimplyLoveLobbyRuntimeView,
    song: &SongData,
    charts: &[Arc<ChartData>; 2],
    gameplay_charts: &[Arc<GameplayChartData>; 2],
    profiles: &[profile_data::Profile; 2],
    session: &GameplaySession,
) -> GameplayInitView {
    GameplayInitView {
        runtime: runtime_view(config, lobby),
        hud: profile::gameplay_hud_snapshot(),
        scores: score_init_view(charts, profiles, session),
        background_changes: background_changes(config, song, gameplay_charts, session),
    }
}

pub(crate) fn score_runtime_view(state: &gameplay::State) -> GameplayScoreRuntimeView {
    let mut scorebox_updates = std::array::from_fn(|_| None);
    for player in 0..state.num_players() {
        let side = profile_side_from_gameplay(state.runtime_player_side(player));
        let side_idx = profile_data::player_side_index(side);
        let profile = gameplay::scorebox_profile_for_side(state, side);
        if !profile.display_scorebox
            || !profile.gs_active
            || !gameplay::scorebox_snapshot_for_side(state, side)
                .is_some_and(|snapshot| snapshot.loading)
        {
            continue;
        }
        let chart_hash = state.charts()[player].short_hash.trim();
        if chart_hash.is_empty() {
            continue;
        }
        if let Some(snapshot) = scores::get_or_fetch_player_leaderboards_for_profile(
            chart_hash,
            profile,
            GAMEPLAY_SCOREBOX_ENTRIES,
        ) && !snapshot.loading
        {
            scorebox_updates[side_idx] = Some(snapshot);
        }
    }
    GameplayScoreRuntimeView {
        scorebox_updates,
        itl_cmod_warning: std::array::from_fn(|player| {
            player < state.num_players() && scores::should_warn_cmod_for_itl_chart(state, player)
        }),
    }
}

pub(crate) fn sync_scores(state: &mut gameplay::State) {
    let view = score_runtime_view(state);
    gameplay::sync_score_runtime_view(state, view);
}

pub(crate) fn practice_view(
    config: &config::Config,
    gameplay: &GameplayInitView,
) -> PracticeRuntimeView {
    PracticeRuntimeView {
        only_dedicated_menu_buttons: config.only_dedicated_menu_buttons,
        three_key_navigation: config.three_key_navigation,
        tab_acceleration: config.tab_acceleration,
        hud: gameplay.hud.clone(),
    }
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
) -> gameplay::SmxSensorPadView {
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
        gameplay::SmxSensorPanelView {
            threshold,
            value: data.and_then(|data| smx_sensor_value(data, panel, fsr)),
        }
    });
    gameplay::SmxSensorPadView { fsr, panels }
}

fn update_smx_sensor_view(
    mut view: gameplay::SmxSensorPadView,
    data: Option<&deadsync_smx::SensorTestData>,
) -> gameplay::SmxSensorPadView {
    for (panel, panel_view) in view.panels.iter_mut().enumerate() {
        panel_view.value = data.and_then(|data| smx_sensor_value(data, panel, view.fsr));
    }
    view
}

fn enter_smx_sensors(state: &mut gameplay::State, smx_input: bool) {
    let plan = gameplay::smx_sensor_pad_plan(state, smx_input);
    for (store_idx, sdk_pad) in plan.into_iter().flatten() {
        deadsync_smx::set_test_mode(sdk_pad, deadsync_smx::SensorTestMode::CalibratedValues);
        let view = deadsync_smx::get_config(sdk_pad)
            .as_ref()
            .map(|config| smx_sensor_view(config, None));
        gameplay::set_smx_sensor_pad_view(state, store_idx, view);
    }
}

fn refresh_smx_sensors(state: &mut gameplay::State, delta_time: f32, smx_input: bool) {
    if !gameplay::smx_sensor_refresh_due(state, delta_time) {
        gameplay::report_smx_sensor_profile(state);
        return;
    }
    let profile_started = gameplay::smx_sensor_profile_enabled(state).then(Instant::now);
    let plan = gameplay::smx_sensor_pad_plan(state, smx_input);
    for (store_idx, sdk_pad) in plan.into_iter().flatten() {
        let data = deadsync_smx::get_test_data(sdk_pad);
        let view = if let Some(view) = gameplay::smx_sensor_pad_view(state, store_idx) {
            Some(update_smx_sensor_view(view, data.as_ref()))
        } else {
            deadsync_smx::get_config(sdk_pad)
                .as_ref()
                .map(|config| smx_sensor_view(config, data.as_ref()))
        };
        gameplay::set_smx_sensor_pad_view(state, store_idx, view);
    }
    if let Some(started) = profile_started {
        gameplay::record_smx_sensor_read_ns(state, started.elapsed().as_nanos() as u64);
    }
    gameplay::report_smx_sensor_profile(state);
}

pub(crate) fn exit(state: &mut gameplay::State) {
    // Always clear both pads, including one whose config was unavailable on
    // enter. Otherwise the SDK keeps streaming sensor data after gameplay.
    for pad in 0..2 {
        deadsync_smx::set_test_mode(pad, deadsync_smx::SensorTestMode::Off);
    }
    gameplay::on_exit(state);
}

#[inline(always)]
fn audio_cut(cut: GameplayMusicCut) -> deadsync_audio_stream::Cut {
    deadsync_audio_stream::Cut {
        start_sec: cut.start_sec,
        length_sec: cut.length_sec,
        fade_in_sec: cut.fade_in_sec,
        fade_out_sec: cut.fade_out_sec,
    }
}

pub(crate) fn snapshot() -> GameplayAudioSnapshot {
    let stream_clock = deadsync_audio_stream::get_music_stream_clock_snapshot();
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

pub(crate) fn drain_core(state: &mut gameplay::GameplayCoreState) {
    for command in state.drain_audio_commands() {
        match command {
            GameplayAudioCommand::StopMusic => {
                if deadsync_audio_stream::is_initialized() {
                    deadsync_audio_stream::stop_music();
                }
            }
            GameplayAudioCommand::SetMusicRate(rate) => {
                deadsync_audio_stream::set_music_rate(rate);
            }
            GameplayAudioCommand::PlayMusic {
                path,
                cut,
                looping,
                rate,
            } => deadsync_audio_stream::play_music(path, audio_cut(cut), looping, rate),
            GameplayAudioCommand::PlayPreloadedSfx(path) => {
                deadsync_audio_stream::play_preloaded_sfx(path);
            }
            GameplayAudioCommand::PlayPreloadedAssistTick(path) => {
                deadsync_audio_stream::play_preloaded_assist_tick(path);
            }
            GameplayAudioCommand::PlayAssistTickAtMusicTime {
                path,
                music_seconds,
            } => {
                if let Some(frame) =
                    deadsync_audio_stream::assist_tick_stream_frame_for_music_seconds(music_seconds)
                {
                    deadsync_audio_stream::play_scheduled_assist_tick(path, frame);
                } else {
                    deadsync_audio_stream::play_preloaded_assist_tick(path);
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
pub(crate) fn drain(state: &mut gameplay::State) {
    drain_core(&mut state.gameplay);
}

#[inline(always)]
fn play_song_lua_sfx(path: &Path) {
    let key = path.to_string_lossy();
    deadsync_audio_stream::play_preloaded_sfx(key.as_ref());
}

pub(crate) fn enter(state: &mut gameplay::State, smx_input: bool) {
    gameplay::on_enter(state);
    enter_smx_sensors(state, smx_input);
    drain(state);
}

fn sequence_effects(first: ThemeEffect, second: ThemeEffect) -> ThemeEffect {
    match (first, second) {
        (ThemeEffect::None, second) => second,
        (first, ThemeEffect::None) => first,
        (ThemeEffect::Batch(mut effects), second) => {
            effects.push(second);
            ThemeEffect::Batch(effects)
        }
        (first, second) => ThemeEffect::Batch(vec![first, second]),
    }
}

pub(crate) fn update(state: &mut gameplay::State, delta_time: f32, smx_input: bool) -> ThemeEffect {
    crate::heart_rate::refresh_gameplay(state);
    let (run_core, lobby_effect) = gameplay::prepare_update(state);
    if !run_core {
        return lobby_effect;
    }

    refresh_smx_sensors(state, delta_time, smx_input);

    // A lobby can queue stage music during `prepare_update`. Execute it before
    // taking the clock snapshot so the deterministic update sees the new
    // stream mapping on the same frame, matching the pre-boundary behavior.
    drain(state);
    let previous_song_lua_time = state.current_music_time_display();
    let effect = gameplay::update(state, delta_time, snapshot(), || {
        deadlib_platform::host_time::instant_nanos(Instant::now())
    });
    sync_scores(state);
    drain(state);
    gameplay::for_each_song_lua_sound_event(
        state,
        previous_song_lua_time,
        state.current_music_time_display(),
        play_song_lua_sfx,
    );
    sequence_effects(lobby_effect, effect)
}

pub(crate) fn handle_input(state: &mut gameplay::State, ev: &InputEvent) -> ThemeEffect {
    let effect = gameplay::handle_input(state, ev);
    drain(state);
    effect
}

pub(crate) fn update_practice(state: &mut practice::State, delta_time: f32) -> ThemeEffect {
    let effect = practice::update(
        state,
        delta_time,
        snapshot(),
        || deadlib_platform::host_time::instant_nanos(Instant::now()),
        deadsync_audio_stream::snap_music_start_sec,
    );
    sync_scores(&mut state.gameplay);
    drain(&mut state.gameplay);
    effect
}

pub(crate) fn enter_practice(state: &mut practice::State) {
    practice::on_enter(state);
    drain(&mut state.gameplay);
}

pub(crate) fn handle_practice_input(state: &mut practice::State, ev: &InputEvent) -> ThemeEffect {
    let effect = practice::handle_input(state, ev, deadsync_audio_stream::snap_music_start_sec);
    drain(&mut state.gameplay);
    effect
}

pub(crate) fn handle_practice_raw_key(
    state: &mut practice::State,
    ev: &RawKeyboardEvent,
) -> (bool, ThemeEffect) {
    let result =
        practice::handle_raw_key_event(state, ev, deadsync_audio_stream::snap_music_start_sec);
    drain(&mut state.gameplay);
    result
}

#[cfg(test)]
mod tests {
    use super::{background_chart, policy_view, scorebox_profiles, smx_sensor_value};
    use deadsync_chart::GameplayChartData;
    use deadsync_gameplay::GameplaySession;
    use deadsync_profile as profile_data;
    use deadsync_profile_gameplay::gameplay_runtime_profile_data;
    use std::sync::Arc;

    #[test]
    fn gameplay_policy_carries_presentation_fields() {
        let config = deadsync_config::prelude::Config {
            machine_font: deadsync_config::prelude::MachineFont::Mega,
            bg_brightness: 0.42,
            gameplay_bg_color: deadsync_config::prelude::Color::from_hex("#123456")
                .expect("test color should parse"),
            smx_input: true,
            zmod_rating_box_text: true,
            show_bpm_decimal: true,
            gameplay_bpm_position: deadsync_config::prelude::GameplayBpmPosition::NearField,
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
    fn p2_solo_backgrounds_use_the_runtime_chart() {
        let charts = std::array::from_fn(|_| {
            Arc::new(GameplayChartData {
                notes: Vec::new(),
                parsed_notes: Vec::new(),
                row_to_beat: Vec::new(),
                timing_segments: Default::default(),
                timing: Default::default(),
                chart_attacks: None,
            })
        });
        let p1 = GameplaySession::default();
        let p2 = GameplaySession {
            play_style: deadsync_profile_gameplay::gameplay_play_style_from_profile(
                profile_data::PlayStyle::Single,
            ),
            player_side: deadsync_profile_gameplay::gameplay_player_side_from_profile(
                profile_data::PlayerSide::P2,
            ),
            joined_sides: [false, true],
            ..Default::default()
        };

        assert!(std::ptr::eq(
            background_chart(&charts, &p1),
            charts[0].as_ref()
        ));
        assert!(std::ptr::eq(
            background_chart(&charts, &p2),
            charts[1].as_ref()
        ));
    }

    #[test]
    fn p2_solo_scorebox_uses_runtime_profile_identity() {
        for play_style in [
            profile_data::PlayStyle::Single,
            profile_data::PlayStyle::Double,
        ] {
            let mut profiles: [profile_data::Profile; 2] =
                std::array::from_fn(|_| Default::default());
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
