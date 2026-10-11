//! Replay actual native clock inputs through the owned gameplay Lua screen.
//! Native expectations are observations; they are never fed into gameplay.
use crate::paths;

use deadsync_gameplay::{
    GameplayAudioSnapshot, GameplayConfig, GameplaySession, GameplayStreamClockSnapshot,
    GameplayViewport,
};
use deadsync_theme_simply_love::screens::gameplay;
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

fn read_json(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&std::fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

fn number(value: &Value, key: &str) -> Result<f64, String> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("missing finite native {key}"))
}

fn broadcast_event(event: &deadsync_song_lua::SongLuaLiveBroadcast, simfile: &Path) -> Value {
    fn normalize(value: &mut Value, song_dir: &str) {
        match value {
            Value::String(value) => {
                if let Some((_, relative)) = value.split_once(song_dir) {
                    *value = format!("song:/{relative}");
                }
            }
            Value::Array(values) => values
                .iter_mut()
                .for_each(|value| normalize(value, song_dir)),
            Value::Object(values) => values
                .values_mut()
                .for_each(|value| normalize(value, song_dir)),
            _ => {}
        }
    }
    let mut args = vec![json!(event.name)];
    if let Ok(Some(params)) = &event.params {
        let mut params = params.clone();
        let song_dir = format!(
            "{}/",
            simfile
                .parent()
                .unwrap_or(Path::new("."))
                .to_string_lossy()
                .replace('\\', "/")
        );
        normalize(&mut params, &song_dir);
        args.push(params);
    }
    json!({"seconds":event.music_seconds,"beat":event.beat,"name":event.name,
        "args":args,"params_error":event.params.as_ref().err()})
}

fn native_fail_type(trace: &Value) -> Result<deadsync_gameplay::GameplayFailType, String> {
    use deadsync_gameplay::GameplayFailType;
    use deadsync_rules::{note, timing};
    let preferences = &trace["input"]["gameplay_preferences"];
    if trace["capabilities"]["native_gameplay_preferences"] != true
        || preferences["initialization"] != "theme-game-mode"
        || preferences["game_mode"] != "ITG"
        || preferences["mod_level"] != "ModsLevel_Preferred"
    {
        return Err(
            "native capture must initialize and record Simply Love ITG gameplay inputs".into(),
        );
    }
    if number(preferences, "timing_window_scale")? != 1.0
        || number(preferences, "timing_window_add")? as f32 != timing::TIMING_WINDOW_ADD_S
        || number(preferences, "max_input_latency_seconds")? != 0.0
        || number(preferences, "music_rate")? != 1.0
    {
        return Err(
            "nondefault native timing scale, addition, latency or rate remains pending".into(),
        );
    }
    let windows = &preferences["timing_windows"];
    let add = timing::TIMING_WINDOW_ADD_S;
    for (key, expected) in [
        ("W1", timing::BASE_W1_S),
        ("W2", timing::BASE_W2_S),
        ("W3", timing::BASE_W3_S),
        ("W4", timing::BASE_W4_S),
        ("W5", timing::BASE_W5_S),
        ("Mine", timing::BASE_MINE_S),
    ] {
        if number(windows, key)? as f32 != expected {
            return Err(format!(
                "native {key} timing does not match the independent gameplay rules"
            ));
        }
    }
    for (key, expected) in [
        ("Hold", note::TIMING_WINDOW_SECONDS_HOLD),
        ("Roll", note::TIMING_WINDOW_SECONDS_ROLL),
    ] {
        if number(windows, key)? as f32 + add != expected {
            return Err(format!(
                "native {key} timing does not match the independent gameplay rules"
            ));
        }
    }
    for key in ["Attack", "Checkpoint"] {
        if number(windows, key)? as f32 + add > note::TIMING_WINDOW_SECONDS_ROLL {
            return Err(format!(
                "native {key} extends the resolution window; this context remains pending"
            ));
        }
    }
    let players = preferences["players"]
        .as_array()
        .filter(|players| players.len() == 1)
        .ok_or("native gameplay inputs must contain exactly one player")?;
    let player = &players[0];
    if player["player"] != "PlayerNumber_P1"
        || player["disabled_timing_windows"] != json!([])
        || player["min_tns_to_hide_notes"] != "TapNoteScore_W3"
    {
        return Err("native player timing modifiers differ; this context remains pending".into());
    }
    match player["fail_setting"].as_str() {
        Some("FailType_Immediate") => Ok(GameplayFailType::Immediate),
        Some("FailType_ImmediateContinue") => Ok(GameplayFailType::ImmediateContinue),
        Some("FailType_EndOfSong") => Ok(GameplayFailType::EndOfSong),
        Some("FailType_Off") => Ok(GameplayFailType::Off),
        _ => Err("native preferred player fail setting is missing or unknown".into()),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Full,
    Entry,
}

pub(crate) fn replay(
    trace_path: &Path,
    simfile: &Path,
    scope: Scope,
    mut progress: impl FnMut(usize, usize),
) -> Result<Value, String> {
    let trace = read_json(trace_path)?;
    if trace["oracle"] != "itgmania_song_lua_semantic_trace"
        || trace["capabilities"]["native_stage_statistics"] != true
        || trace["capabilities"]["native_machine_preferences"] != true
        || trace["capabilities"]["native_update_frames"] != true
        || trace["capabilities"]["native_screen_lifecycle"] != true
        || trace["dropped_update_frames"] != 0
        || trace["dropped_events"] != 0
        || trace["dropped_stage_samples"] != 0
        || trace["completion"]["reason"] != "gameplay-exit"
        || trace["completion"]["stage_committed"] != true
        || trace["update_frame_phase"] != "gameplay-overlay-probe"
        || trace["input"]["injected_inputs"] != false
    {
        return Err("a complete executable capture with actual frame clocks is required".into());
    }
    if trace["input"]["autoplay"] != 0 || trace["style"] != "single" {
        return Err(
            "live replay currently supports no-input dance-single; other contexts remain pending"
                .into(),
        );
    }
    let frames = trace["update_frames"]
        .as_array()
        .filter(|frames| !frames.is_empty())
        .ok_or("native update frames are empty")?;
    let last_scene = frames
        .last()
        .and_then(|frame| frame["scene"].as_u64())
        .ok_or("missing final scene")?;
    // Stop just after the first reload, or at the first nonnegative input
    // clock for a song that does not reload. Every preceding frame is retained.
    let frame_count = if scope == Scope::Entry {
        frames
            .iter()
            .position(|frame| frame["scene"].as_u64().is_some_and(|scene| scene > 1))
            .or_else(|| {
                frames
                    .iter()
                    .position(|frame| frame["seconds"].as_f64().is_some_and(|time| time >= 0.0))
            })
            .map_or(frames.len(), |index| index + 1)
    } else {
        frames.len()
    };
    progress(0, frame_count);
    let samples = trace["stage_samples"]
        .as_array()
        .ok_or("missing native stage observations")?;
    let difficulty = trace["input"]["requested_difficulty"].as_str();
    let preferences = &trace["input"]["preferences"];
    let sync = match preferences["default_sync_offset"].as_str() {
        Some("SyncOffset_NULL") => deadsync_chart::SyncPref::Null,
        Some("SyncOffset_ITG") => deadsync_chart::SyncPref::Itg,
        _ => return Err("native default sync setting is missing or unknown".into()),
    };
    let config = GameplayConfig {
        default_fail_type: native_fail_type(&trace)?,
        global_offset_seconds: number(preferences, "global_offset_seconds")? as f32,
        visual_delay_seconds: number(preferences, "visual_delay_seconds")? as f32,
        machine_default_sync_pref: sync,
        machine_pack_ini_offsets: true,
        ..Default::default()
    };
    paths::init();
    let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(
        simfile,
        config.global_offset_seconds,
    )?);
    let initial = song
        .charts
        .iter()
        .position(|chart| {
            chart.chart_type == "dance-single"
                && difficulty.is_none_or(|wanted| chart.difficulty.eq_ignore_ascii_case(wanted))
        })
        .ok_or("requested native starting chart is absent")?;
    let first = Arc::new(song.charts[initial].clone());
    let charts = [first.clone(), first];
    let initial_payload = Arc::new(
        deadsync_simfile::app_runtime::load_gameplay_charts(
            &song,
            &[initial],
            config.global_offset_seconds,
        )?
        .remove(0),
    );
    let payloads = [initial_payload.clone(), initial_payload];
    let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
    let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
    let session = GameplaySession::default();
    let viewport = GameplayViewport::new(
        number(&trace["display"], "logical_width")? as f32,
        number(&trace["display"], "logical_height")? as f32,
    );
    let prepared = deadsync_shell::gameplay_entry::prepare_song_lua(
        &song,
        &charts,
        std::array::from_fn(|p| &payloads[p].timing),
        &profiles,
        &scroll,
        1.0,
        viewport,
        (
            number(&trace["display"], "width")? as u32,
            number(&trace["display"], "height")? as u32,
        ),
        &session,
        &config,
        "null",
        1,
        true,
    );
    let init_scene = |prepared: deadsync_song_lua::playback::PreparedGameplaySongLua<
        deadsync_assets::noteskin::SpriteSlot,
    >|
     -> Result<Box<gameplay::State>, String> {
        let mut charts = charts.clone();
        let mut payloads = payloads.clone();
        deadsync_shell::gameplay_entry::apply_song_lua_reload(
            &song,
            &mut charts,
            &mut payloads,
            &prepared,
            &session,
            config.global_offset_seconds,
        )?;
        let state = Box::new(gameplay::init(
            song.clone(),
            charts,
            payloads,
            viewport,
            session.clone(),
            config,
            5,
            1.0,
            scroll,
            profiles.clone(),
            None,
            None,
            None,
            Arc::from("EVENT"),
            None,
            None,
            None,
            None,
            None,
            [deadsync_gameplay::CourseLifeConfig::Bar; 2],
            false,
            None,
            None,
            [0; 2],
            prepared,
            Default::default(),
        ));
        if state.song_lua.is_none() {
            return Err("preparation did not retain a Lua scene".into());
        }
        Ok(state)
    };
    let mut state = init_scene(prepared)?;
    let startup = state.take_song_lua_output();
    let startup_scenes = startup
        .broadcasts
        .iter()
        .filter(|(_, name, _)| name == "ScreenChanged")
        .count();
    let mut mismatches = Vec::new();
    let mut scene_count = startup_scenes as u64;
    let mut pending_reload = false;
    let mut errors = startup.errors;
    let mut broadcasts: Vec<_> = startup
        .source_broadcasts
        .iter()
        .map(|event| broadcast_event(event, simfile))
        .collect();
    let mut observations = Vec::new();
    let mut requests = Vec::new();
    let mut first_navigation = None;
    let mut frames_after_navigation = 0;
    let mut checked_samples = 0;
    let mut compared_fields = 0;
    let mut sample_cursor = 0;
    let mut previous_wall = None;
    let mut used_frames = 0;
    for (index, frame) in frames.iter().take(frame_count).enumerate() {
        if first_navigation.is_some() {
            frames_after_navigation += 1;
        }
        if frame["sequence"].as_u64() != Some(index as u64 + 1) {
            return Err("native frame sequence is incomplete".into());
        }
        if pending_reload {
            let scene = state
                .song_lua
                .take()
                .ok_or("reload has no retained Lua scene")?;
            let prepared = deadsync_song_lua::playback::reload_song_lua(
                &song,
                scene,
                deadsync_assets::song_lua::reload_song_lua,
            )?;
            state = init_scene(prepared)?;
            let startup = state.take_song_lua_output();
            errors.extend(startup.errors);
            broadcasts.extend(
                startup
                    .source_broadcasts
                    .iter()
                    .map(|event| broadcast_event(event, simfile)),
            );
            scene_count += 1;
        }
        if frame["scene"].as_u64() != Some(scene_count) {
            return Err(format!(
                "native scene {} differs from independent scene {scene_count} at frame {}",
                frame["scene"], frame["sequence"]
            ));
        }
        let seconds = number(frame, "seconds")?;
        let delta = number(frame, "delta")?;
        let wall = number(frame, "wall_seconds")?;
        if delta < 0.0 || previous_wall.is_some_and(|previous| wall < previous) {
            return Err("invalid native input-clock order".into());
        }
        previous_wall = Some(wall);
        // Preparation already performs the real startup Update(0). Set the
        // input clock without running those callbacks a second time.
        if delta == 0.0 {
            state.set_current_music_time_ns((seconds * 1e9).round() as i64);
        }
        let host = (wall * 1e9).round() as u64;
        let snapshot = GameplayAudioSnapshot {
            stream_clock: GameplayStreamClockSnapshot {
                music_nanos: (seconds * 1e9).round() as i64,
                has_music_mapping: true,
                valid_at_host_nanos: host,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut effects = Vec::new();
        if delta > 0.0 {
            gameplay::update(&mut state, delta as f32, snapshot, || host, &mut effects);
        }
        used_frames += 1;
        progress(used_frames, frame_count);
        pending_reload = effects.iter().any(|effect| {
            matches!(
                effect,
                deadsync_theme_simply_love::SimplyLoveEffect::Runtime(
                    deadsync_theme_simply_love::SimplyLoveRuntimeRequest::ReloadSongLua
                )
            )
        });
        if first_navigation.is_none()
            && effects.iter().any(|effect| {
                matches!(
                    effect,
                    deadsync_theme_simply_love::SimplyLoveEffect::Navigate(_)
                        | deadsync_theme_simply_love::SimplyLoveEffect::NavigateNoFade(_)
                )
            })
        {
            first_navigation = Some(seconds);
        }
        let output = state.take_song_lua_output();
        broadcasts.extend(
            output
                .source_broadcasts
                .iter()
                .map(|event| broadcast_event(event, simfile)),
        );
        errors.extend(output.errors);
        if let Some(request) = output.request {
            let (operation, name) = match request {
                deadsync_song_lua::SongLuaLiveRequest::Transition(name) => {
                    ("StartTransitioningScreen", name)
                }
                deadsync_song_lua::SongLuaLiveRequest::NewScreen(name) => {
                    if name != "ScreenGameplay" {
                        return Err(format!("unsupported reload target {name}"));
                    }
                    if !pending_reload {
                        return Err(
                            "gameplay reload did not emit the production runtime effect".into()
                        );
                    }
                    ("SetNewScreen", name)
                }
            };
            requests.push(json!({"seconds":seconds, "operation":operation, "args":[name]}));
        }
        // Compare independently simulated state only at the recorded observer
        // phase. Native stats are never fed back into gameplay or the Lua VM.
        while sample_cursor < samples.len() {
            let sample = &samples[sample_cursor];
            let at = number(sample, "seconds")?;
            if !matches!(
                sample["phase"].as_str(),
                Some("screen-active" | "progress" | "screen-end" | "gameplay-exit")
            ) || sample["scene"]
                .as_u64()
                .is_some_and(|scene| scene < scene_count)
            {
                sample_cursor += 1;
                continue;
            }
            if sample["scene"].as_u64() != Some(scene_count) {
                break;
            }
            if at > seconds + 1e-6 {
                break;
            }
            sample_cursor += 1;
            if (at - seconds).abs() > 1e-6 {
                mismatches
                    .push(json!({"field":"stage_sample_clock", "native":at,"deadsync":seconds}));
                continue;
            }
            checked_samples += 1;
            let player = &state.players_runtime.players[0];
            let score = state
                .stage_itg_score_inputs(0)
                .ok_or("missing active player score inputs")?;
            let selected = state
                .song_lua
                .as_ref()
                .ok_or("missing Lua metadata")?
                .selected_context()
                .map_err(|error| error.to_string())?
                .player_step_indices[0]
                .ok_or("GAMESTATE has no selected P1 chart")?;
            let metadata = song
                .charts
                .get(selected)
                .ok_or("GAMESTATE selected an absent chart")?;
            let actual = json!({"possible":score.possible_grade_points, "current_possible":player.current_possible_points,
                "actual":player.earned_grade_points, "misses":player.judgment_counts[5],
                "w1":player.judgment_counts[0], "w2":player.judgment_counts[1],
                "failed":player.fail_time.is_some(), "life_latest":player.life,
                "difficulty":format!("Difficulty_{}", metadata.difficulty),
                "description":metadata.description, "meter":metadata.meter});
            let native = &sample["players"][0];
            for key in [
                "possible",
                "current_possible",
                "actual",
                "misses",
                "w1",
                "w2",
                "failed",
                "life_latest",
                "difficulty",
                "description",
                "meter",
            ] {
                compared_fields += 1;
                let equal = if key == "life_latest" {
                    (number(native, key)? - number(&actual, key)?).abs() <= 1e-6
                } else {
                    native[key] == actual[key]
                };
                if !equal {
                    mismatches.push(json!({"phase":sample["phase"],"seconds":seconds, "field":key,"native":native[key],"deadsync":actual[key]}));
                }
            }
            observations.push(
                json!({"phase":sample["phase"],"scene":scene_count,"seconds":seconds,"beat":state.clock.song_position.current_beat,
                "screen_seconds":state.boundary.total_elapsed_in_screen,"player":actual}),
            );
        }
    }
    if scope == Scope::Entry {
        return Ok(
            json!({"diagnostic_only":true,"whole_song_parity_proven":false,"archive_accepted":false,
            "scope":"entry","native_trace":trace_path,"source_simfile":simfile,
            "native_frames":frames.len(),"replayed_frames":used_frames,"replayed_scenes":scene_count,
            "checked_stage_samples":checked_samples,"compared_stage_fields":compared_fields,
            "mismatches":mismatches,"errors":errors,"lua_requests":requests,
            "stage_observations":observations,
            "pending_coverage":["complete song replay", "overlay-init observations",
                "command/query timing", "complete stage statistics", "actor/render/resource state"]}),
        );
    }
    let expected_samples = samples
        .iter()
        .filter(|sample| {
            matches!(
                sample["phase"].as_str(),
                Some("screen-active" | "progress" | "screen-end" | "gameplay-exit")
            )
        })
        .count();
    if expected_samples != checked_samples {
        mismatches.push(json!({"field":"stage_sample_coverage", "native":expected_samples,"deadsync":checked_samples}));
    }
    if scene_count != last_scene {
        mismatches
            .push(json!({"field":"scene_coverage", "native":last_scene,"deadsync":scene_count}));
    }
    let native_requests: Vec<_> = trace["events"].as_array().ok_or("missing native events")?.iter()
        .filter_map(|event| {
            let operation = event["operation"].as_str()?;
            if !(operation.ends_with(".StartTransitioningScreen") || operation == "ScreenManager.SetNewScreen") {
                return None;
            }
            Some(json!({"seconds":event["seconds"], "operation":operation.rsplit('.').next(), "args":event["args"]}))
        }).collect();
    let actual_requests: Vec<_> = requests.iter().collect();
    if native_requests.len() != actual_requests.len() {
        mismatches.push(json!({"field":"source_request_count", "native":native_requests.len(), "deadsync":actual_requests.len()}));
    }
    for (native, actual) in native_requests.iter().zip(actual_requests) {
        if native["operation"] != actual["operation"]
            || native["args"] != actual["args"]
            || (number(native, "seconds")? - number(actual, "seconds")?).abs() > 1e-6
        {
            mismatches.push(json!({"field":"source_request", "native":native,"deadsync":actual}));
        }
    }
    if native_requests.iter().any(|request| {
        request["operation"] == "StartTransitioningScreen"
            && request["args"] == json!(["SM_DoNextScreen"])
    }) {
        let end = samples
            .iter()
            .find(|sample| {
                sample["phase"] == "screen-end" && sample["scene"].as_u64() == Some(last_scene)
            })
            .ok_or("native transition has no final screen-end observation")?;
        let native = number(end, "seconds")?;
        if first_navigation.is_none_or(|actual: f64| (actual - native).abs() > 1e-6) {
            mismatches.push(json!({"field":"screen_end_navigation_clock","native":native,"deadsync":first_navigation}));
        }
    }
    let native_broadcasts: Vec<_> = trace["events"].as_array().ok_or("missing native events")?.iter()
        .filter(|event| event["operation"] == "MessageManager.Broadcast")
        .map(|event| json!({"seconds":event["seconds"],"beat":event["beat"],"name":event["args"][0],"args":event["args"]})).collect();
    if native_broadcasts.len() != broadcasts.len() {
        mismatches.push(json!({"field":"source_broadcast_count","native":native_broadcasts.len(),"deadsync":broadcasts.len()}));
    }
    for (native, actual) in native_broadcasts.iter().zip(&broadcasts) {
        if !actual["params_error"].is_null() {
            mismatches
                .push(json!({"field":"source_broadcast_params_unsupported", "deadsync":actual}));
        }
        if native["args"] != actual["args"]
            || (number(native, "seconds")? - number(actual, "seconds")?).abs() > 1e-6
            || (number(native, "beat")? - number(actual, "beat")?).abs() > 1e-6
        {
            mismatches.push(json!({"field":"source_broadcast","native":native,"deadsync":actual}));
        }
    }
    let report = json!({"diagnostic_only":true,"whole_song_parity_proven":false,"archive_accepted":false,
        "source_simfile":simfile,"native_trace":trace_path,"native_frames":frames.len(),"replayed_frames":used_frames,
        "startup_scenes":startup_scenes,"replayed_scenes":scene_count,"selected_description":state.source.charts[0].description,
        "checked_stage_samples":checked_samples,"compared_stage_fields":compared_fields,"mismatches":mismatches,
        "errors":errors,"lua_requests":requests,"native_transition_requests":native_requests,
        "lua_broadcasts":broadcasts,"native_broadcasts":native_broadcasts,
        "first_domain_navigation_seconds":first_navigation, "continued_after_domain_navigation": frames_after_navigation > 0,
        "frames_after_domain_navigation":frames_after_navigation,
        "stage_observations":observations,
        "broadcast_params_compared":true,
        "pending_coverage":["overlay-init lifecycle observations", "native command/query delivery timing", "complex or truncated broadcast values",
            "full theme stage commitment and history", "score/alive-time getters", "all actor/render/resource state",
            "additional game/style/input/course contexts"]});

    eprintln!(
        "replayed {used_frames} actual frames; {checked_samples} stage samples; {} mismatches; {} callback errors",
        report["mismatches"].as_array().map_or(0, Vec::len),
        report["errors"].as_array().map_or(0, Vec::len)
    );
    Ok(report)
}
