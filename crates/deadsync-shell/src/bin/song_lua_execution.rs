//! Headless smoke check of the application build's Lua scene ownership and updates.
//!
//! Run as a shell binary, not a Cargo test/example: those unify dev-dependency
//! `test-support` features and can hide differences in the shipped gameplay path.
use deadsync_gameplay::{GameplayAudioSnapshot, GameplayStreamClockSnapshot};
use deadsync_song_lua::SongLuaOverlayKind;
use deadsync_theme_simply_love::screens::gameplay;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Instant};

fn snapshot(state: &gameplay::State) -> Value {
    let visuals = &state.gameplay.mods.song_lua_visuals;
    let actors = visuals
        .overlays
        .iter()
        .chain(
            visuals
                .background_visual_layers
                .iter()
                .flat_map(|layer| &layer.overlays),
        )
        .chain(
            visuals
                .foreground_visual_layers
                .iter()
                .flat_map(|layer| &layer.overlays),
        );
    Value::Array(
        actors
            .map(|actor| {
                let state = &actor.initial_state;
                let vertices = match &actor.kind {
                    SongLuaOverlayKind::ActorMultiVertex { vertices, .. } => vertices
                        .iter()
                        .map(|vertex| json!([vertex.pos[0], vertex.pos[1], vertex.z]))
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                json!({"name":actor.name,"x":state.x,"y":state.y,"z":state.z,
            "visible":state.visible,"vertices":vertices})
            })
            .collect(),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("song_lua_execution <song.ssc> <result.json>".into());
    }
    let simfile = PathBuf::from(&args[0]).canonicalize()?;
    let bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let data = bundle.join("target/song-lua-execution/data");
    let dirs = deadsync_config::dirs::AppDirs {
        cache_dir: data.join("cache"),
        data_dir: data,
        exe_dir: bundle,
        portable: false,
    };
    dirs.ensure_dirs_exist();
    deadsync_shell::app::init_paths(&dirs)?;
    deadsync_theme_simply_love::i18n::init(deadsync_assets::language::load("en"));
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    deadlib_present::space::set_current_window_px(1280, 720);
    let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(
        &simfile, 0.0,
    )?);
    let index = song
        .charts
        .iter()
        .position(|chart| chart.difficulty.eq_ignore_ascii_case("challenge"))
        .unwrap_or(0);
    let chart = Arc::new(song.charts[index].clone());
    let mut charts = [chart.clone(), chart];
    let mut loaded = deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[index], 0.0)?;
    let payload = Arc::new(loaded.remove(0));
    let mut payloads = [payload.clone(), payload];
    let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
    let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
    let viewport = deadsync_gameplay::GameplayViewport::design();
    let mut session = deadsync_gameplay::GameplaySession::default();
    let config = deadsync_gameplay::GameplayConfig {
        global_offset_seconds: 0.0,
        ..Default::default()
    };
    let start = Instant::now();
    let prepared = deadsync_shell::gameplay_entry::prepare_song_lua(
        &song,
        &charts,
        [&payloads[0].timing, &payloads[1].timing],
        &profiles,
        &scroll,
        1.0,
        viewport,
        (1280, 720),
        16.0 / 9.0,
        0.0,
        &session,
        &config,
        "Vulkan",
        1,
    );
    deadsync_shell::gameplay_entry::apply_song_lua_reload(
        &song,
        &mut charts,
        &mut payloads,
        &prepared,
        &mut session,
        config.global_offset_seconds,
    )?;
    // Transfer the prepared scene through the real screen initializer. Merely
    // calling take_live_scene here would miss a rejection at screen attachment.
    let mut state = gameplay::init(
        song.clone(),
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
    let prepare_ms = start.elapsed().as_secs_f64() * 1000.0;
    let retained_lua_vm = state.song_lua.is_some();
    let initial = snapshot(&state);
    let mut previous = initial.clone();
    let mut changed_frames = 0;
    let mut frames = 0;
    let mut actor_sound_operations = 0;
    let mut errors = Vec::new();
    if retained_lua_vm {
        for frame in 0..241 {
            let seconds = frame as f64 / 60.0;
            let host = 1_000_000_000 + (seconds * 1e9).round() as u64;
            gameplay::update(
                &mut state,
                if frame == 0 { 0.0 } else { 1.0 / 60.0 },
                GameplayAudioSnapshot {
                    stream_clock: GameplayStreamClockSnapshot {
                        music_nanos: (seconds * 1e9).round() as i64,
                        has_music_mapping: true,
                        valid_at_host_nanos: host,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                || host,
                &mut Vec::new(),
            );
            let output = state.take_song_lua_output();
            errors.extend(output.errors);
            actor_sound_operations += output.actor_sounds.len();
            let current = snapshot(&state);
            changed_frames += usize::from(current != previous);
            previous = current;
            frames += 1;
        }
    }
    let passed = retained_lua_vm && errors.is_empty() && frames == 241 && changed_frames > 1;
    let result = json!({"song":song.title,"source_simfile":simfile,"retained_lua_vm":retained_lua_vm,
        "execution_path":"application-gameplay-init-and-update","prepare_ms":prepare_ms,
        "frames":frames,"changed_visual_frames":changed_frames,"actor_sound_operations":actor_sound_operations,
        "errors":errors,"initial_actors":initial,"final_actors":previous,
        "startup_smoke_passed":passed,"native_parity_checked":false,"pixel_parity_proven":false});
    std::fs::write(&args[1], serde_json::to_vec_pretty(&result)?)?;
    println!(
        "retained_lua_vm={retained_lua_vm}, frames={frames}, changed_visual_frames={changed_frames}, startup_smoke_passed={passed}"
    );
    if !passed {
        return Err(format!("gameplay Lua startup failed; see {}", args[1]).into());
    }
    Ok(())
}
