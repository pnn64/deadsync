use deadsync_gameplay::{
    GameplayAudioCommand, GameplayAudioSnapshot, GameplayMusicCut, GameplaySessionCommand,
    GameplayStreamClockSnapshot,
};
use deadsync_input::{InputEvent, RawKeyboardEvent};
use deadsync_profile_gameplay::profile_tick_mode_from_gameplay;
use deadsync_theme_simply_love::SimplyLoveEffect as ThemeEffect;
use deadsync_theme_simply_love::screens::{gameplay, practice};
use std::path::Path;
use std::time::Instant;

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
        gameplay::report_smx_sensor_profile();
        return;
    }
    let profile_started = gameplay::smx_sensor_profile_enabled().then(Instant::now);
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
        gameplay::record_smx_sensor_read_ns(started.elapsed().as_nanos() as u64);
    }
    gameplay::report_smx_sensor_profile();
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
    use super::smx_sensor_value;

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
