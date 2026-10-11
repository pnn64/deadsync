//! Replay actual native clock inputs through the owned gameplay Lua screen.
//! This diagnostic exposes live-domain gaps; it does not accept archives.
#[path = "support/paths.rs"]
mod paths;

use deadsync_gameplay::{
    GameplayAudioSnapshot, GameplayConfig, GameplaySession, GameplayStreamClockSnapshot,
    GameplayViewport,
};
use deadsync_theme_simply_love::screens::gameplay;
use serde_json::{Value, json};
use std::{path::Path, process::ExitCode, sync::Arc};

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

fn run(trace_path: &Path, simfile: &Path, out: &Path) -> Result<(), String> {
    let trace = read_json(trace_path)?;
    if trace["oracle"] != "itgmania_song_lua_semantic_trace"
        || trace["capabilities"]["native_stage_statistics"] != true
        || trace["capabilities"]["native_machine_preferences"] != true
        || trace["capabilities"]["native_update_frames"] != true
        || trace["dropped_update_frames"] != 0
        || trace["dropped_events"] != 0
        || trace["dropped_stage_samples"] != 0
        || trace["completion"]["reason"] != "gameplay-exit"
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
    let mut charts = [first.clone(), first];
    let initial_payload = Arc::new(
        deadsync_simfile::app_runtime::load_gameplay_charts(
            &song,
            &[initial],
            config.global_offset_seconds,
        )?
        .remove(0),
    );
    let mut payloads = [initial_payload.clone(), initial_payload];
    let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
    let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
    let session = GameplaySession::default();
    let viewport = GameplayViewport::new(
        number(&trace["display"], "logical_width")? as f32,
        number(&trace["display"], "logical_height")? as f32,
    );
    let mut context = deadsync_profile_gameplay::song_lua_play_context(
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
    );
    context.live = true;
    context.song_step_timing =
        deadsync_simfile::app_runtime::load_chart_timings(&song, config.global_offset_seconds)?;
    let group_offset =
        deadsync_chart::song::sync_pref_offset(deadsync_chart::SyncPref::Default, sync);
    for timing in &mut context.song_step_timing {
        timing.shift_song_offset_seconds(group_offset);
    }
    let mut prepared = deadsync_song_lua::playback::prepare_song_lua(
        &song,
        &context,
        deadsync_assets::song_lua::compile_song_lua,
        deadsync_assets::song_lua::compile_song_lua_layers,
    );
    if let Some(indices) = prepared.loaded_steps() {
        let selected = indices[0].ok_or("Lua reload did not select P1 steps")?;
        let chart = Arc::new(
            song.charts
                .get(selected)
                .ok_or("Lua selected an absent chart")?
                .clone(),
        );
        let payload = Arc::new(
            deadsync_simfile::app_runtime::load_gameplay_charts(
                &song,
                &[selected],
                config.global_offset_seconds,
            )?
            .remove(0),
        );
        charts = [chart.clone(), chart];
        payloads = [payload.clone(), payload];
    }
    let scene = prepared
        .take_live_scene()?
        .ok_or("preparation did not retain a Lua scene")?;
    let mut state = gameplay::init(
        song,
        charts,
        payloads,
        viewport,
        session,
        config,
        5,
        1.0,
        scroll,
        profiles,
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
    );
    state.attach_song_lua(scene)?;
    let startup = state.take_song_lua_output();
    let scene_count = startup
        .broadcasts
        .iter()
        .filter(|(_, name, _)| name == "ScreenChanged")
        .count();
    let mut mismatches = Vec::new();
    if scene_count as u64 != last_scene {
        mismatches
            .push(json!({"field":"startup_scenes", "native":last_scene, "deadsync":scene_count}));
    }
    let mut errors = startup.errors;
    let mut observations = Vec::new();
    let mut requests = Vec::new();
    let mut first_navigation = None;
    let mut checked_samples = 0;
    let mut compared_fields = 0;
    let mut sample_cursor = 0;
    let mut previous_wall = None;
    let mut previous_seconds = None;
    let mut used_frames = 0;
    for (index, frame) in frames.iter().enumerate() {
        if frame["sequence"].as_u64() != Some(index as u64 + 1) {
            return Err("native frame sequence is incomplete".into());
        }
        if frame["scene"].as_u64() != Some(last_scene) {
            continue;
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
        errors.extend(output.errors);
        if let Some(request) = output.request {
            let (operation, name) = match request {
                deadsync_song_lua::SongLuaLiveRequest::Transition(name) => {
                    ("StartTransitioningScreen", name)
                }
                deadsync_song_lua::SongLuaLiveRequest::NewScreen(name) => ("SetNewScreen", name),
            };
            requests.push(json!({"seconds":seconds, "operation":operation, "args":[name]}));
        }
        let player = &state.players_runtime.players[0];
        let score = state
            .stage_itg_score_inputs(0)
            .ok_or("missing active player score inputs")?;
        let actual = json!({"possible":score.possible_grade_points, "current_possible":player.current_possible_points,
            "actual":player.earned_grade_points, "misses":player.judgment_counts[5],
            "w1":player.judgment_counts[0], "w2":player.judgment_counts[1],
            "failed":player.fail_time.is_some(), "life_latest":player.life,
            "difficulty":format!("Difficulty_{}", state.source.charts[0].difficulty),
            "description":state.source.charts[0].description, "meter":state.source.charts[0].meter});
        // Compare independently simulated state only at the recorded observer
        // phase. Native stats are never fed back into gameplay or the Lua VM.
        while sample_cursor < samples.len() {
            let sample = &samples[sample_cursor];
            let at = number(sample, "seconds")?;
            if sample["scene"].as_u64() != Some(last_scene) || sample["phase"] != "progress" {
                sample_cursor += 1;
                continue;
            }
            if at > seconds + 1e-6 {
                break;
            }
            sample_cursor += 1;
            if previous_seconds.is_some_and(|previous| at <= previous) {
                continue;
            }
            if (at - seconds).abs() > 1e-6 {
                mismatches
                    .push(json!({"field":"stage_sample_clock", "native":at,"deadsync":seconds}));
                continue;
            }
            checked_samples += 1;
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
                    mismatches.push(json!({"seconds":seconds, "field":key,"native":native[key],"deadsync":actual[key]}));
                }
            }
            observations.push(
                json!({"seconds":seconds,"beat":state.clock.song_position.current_beat,
                "screen_seconds":state.boundary.total_elapsed_in_screen,"player":actual}),
            );
        }
        previous_seconds = Some(seconds);
    }
    let expected_samples = samples
        .iter()
        .filter(|sample| {
            sample["scene"].as_u64() == Some(last_scene) && sample["phase"] == "progress"
        })
        .count();
    if expected_samples != checked_samples {
        mismatches.push(json!({"field":"stage_sample_coverage", "native":expected_samples,"deadsync":checked_samples}));
    }
    let native_requests: Vec<_> = trace["events"].as_array().ok_or("missing native events")?.iter()
        .filter(|event| event["operation"].as_str().is_some_and(|operation| operation.ends_with(".StartTransitioningScreen")))
        .map(|event| json!({"seconds":event["seconds"], "operation":"StartTransitioningScreen", "args":event["args"]})).collect();
    let actual_requests: Vec<_> = requests
        .iter()
        .filter(|request| request["operation"] == "StartTransitioningScreen")
        .collect();
    if native_requests.len() != actual_requests.len() {
        mismatches.push(json!({"field":"source_transition_count", "native":native_requests.len(), "deadsync":actual_requests.len()}));
    }
    for (native, actual) in native_requests.iter().zip(actual_requests) {
        if native["args"] != actual["args"]
            || (number(native, "seconds")? - number(actual, "seconds")?).abs() > 1e-6
        {
            mismatches
                .push(json!({"field":"source_transition", "native":native,"deadsync":actual}));
        }
    }
    let report = json!({"diagnostic_only":true,"whole_song_parity_proven":false,"archive_accepted":false,
        "source_simfile":simfile,"native_trace":trace_path,"native_frames":frames.len(),"replayed_frames":used_frames,
        "startup_scenes":scene_count,"selected_description":state.source.charts[0].description,
        "checked_stage_samples":checked_samples,"compared_stage_fields":compared_fields,"mismatches":mismatches,
        "errors":errors,"lua_requests":requests,"native_transition_requests":native_requests,
        "first_domain_navigation_seconds":first_navigation, "continued_after_domain_navigation": first_navigation.is_some(),
        "stage_observations":observations,
        "pending_coverage":["all lifecycle stage observations", "native command/query delivery timing",
            "full theme stage commitment and history", "score/alive-time getters", "all actor/render/resource state",
            "additional game/style/input/course contexts"]});
    std::fs::write(
        out,
        serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    eprintln!(
        "replayed {used_frames} actual frames; {checked_samples} stage samples; {} mismatches; {} callback errors",
        report["mismatches"].as_array().map_or(0, Vec::len),
        report["errors"].as_array().map_or(0, Vec::len)
    );
    eprintln!(
        "diagnostic saved to {}; remaining coverage prevents archive acceptance",
        out.display()
    );
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "cargo test --test live_song_lua -- <native-trace.json> <simfile> <diagnostic.json>"
        );
        return ExitCode::SUCCESS;
    }
    if args.len() != 3 {
        eprintln!("expected native trace, original simfile and output path");
        return ExitCode::FAILURE;
    }
    match run(
        Path::new(&args[0]),
        Path::new(&args[1]),
        Path::new(&args[2]),
    ) {
        Ok(()) => ExitCode::from(2), // A diagnostic never certifies whole-song parity.
        Err(error) => {
            eprintln!("live replay failed: {error}");
            ExitCode::FAILURE
        }
    }
}
