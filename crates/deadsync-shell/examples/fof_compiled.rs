//! Inspect Fof Guys on the normal compiled gameplay path, without a retained Lua VM.
use deadsync_song_lua::{SongLuaOverlayKind, SongLuaOverlayUpdateTarget};
use std::{path::PathBuf, sync::Arc, time::Instant};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "fof_compiled <fof.ssc> <result.json>");
    let simfile = PathBuf::from(&args[0]).canonicalize().unwrap();
    let bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let data = bundle.join("target/fof-compiled/data");
    let dirs = deadsync_config::dirs::AppDirs {
        cache_dir: data.join("cache"),
        data_dir: data,
        exe_dir: bundle,
        portable: false,
    };
    dirs.ensure_dirs_exist();
    deadsync_shell::app::init_paths(&dirs).unwrap();
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    deadlib_present::space::set_current_window_px(1280, 720);
    let song = deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap();
    let index = song
        .charts
        .iter()
        .position(|chart| chart.difficulty.eq_ignore_ascii_case("challenge"))
        .unwrap();
    let chart = Arc::new(song.charts[index].clone());
    let charts = [chart.clone(), chart];
    let payloads =
        deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[index, index], 0.0).unwrap();
    let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
    let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
    let start = Instant::now();
    let mut prepared = deadsync_shell::gameplay_entry::prepare_song_lua(
        &song,
        &charts,
        [&payloads[0].timing, &payloads[1].timing],
        &profiles,
        &scroll,
        1.0,
        deadsync_gameplay::GameplayViewport::design(),
        (1280, 720),
        16.0 / 9.0,
        1.0,
        &deadsync_gameplay::GameplaySession::default(),
        &deadsync_gameplay::GameplayConfig::default(),
        "OpenGL",
        1,
    );
    assert!(
        prepared.take_live_scene().unwrap().is_none(),
        "production retained a Lua VM"
    );
    let compiled_scene = prepared.take_compiled_scene().unwrap();
    assert!(
        compiled_scene.is_some(),
        "production did not lower the interactive scene"
    );
    let compile_ms = start.elapsed().as_secs_f64() * 1000.0;
    let layers = prepared.compiled_layers().map(|visuals| {
    let balls = visuals.overlays.iter().enumerate().filter_map(|(index, actor)| {
        let SongLuaOverlayKind::Sprite { texture_path, .. } = &actor.kind else { return None; };
        if !texture_path.file_name()?.to_string_lossy().starts_with("ball") { return None; }
        let tracks = visuals.overlay_updates.iter().filter(|track| track.overlay_index == index)
            .map(|track| serde_json::json!({"target":format!("{:?}",track.target),"samples":track.samples.len(),
                "first":format!("{:?}",track.samples.first()),"last":format!("{:?}",track.samples.last())})).collect::<Vec<_>>();
        Some(serde_json::json!({"index":index,"texture":texture_path,"initial_visible":actor.initial_state.visible,
            "initial_x":actor.initial_state.x,"initial_y":actor.initial_state.y,"tracks":tracks,
            "can_become_visible":actor.initial_state.visible || visuals.overlay_updates.iter().any(|track|
                track.overlay_index == index && track.target == SongLuaOverlayUpdateTarget::Visible && track.samples.iter().any(|sample|
                    matches!(sample.value, deadsync_song_lua::SongLuaOverlayUpdateValue::Bool(true))))}))
    }).collect::<Vec<_>>();
    let meshes = visuals.overlays.iter().filter_map(|actor| match &actor.kind {
        SongLuaOverlayKind::ActorMultiVertex { vertices, .. } => Some(vertices.len()), _ => None,
    }).collect::<Vec<_>>();
    serde_json::json!({"overlays":visuals.overlays.len(),"update_tracks":visuals.overlay_updates.len(),"explicit_draw_frames":visuals.draw_frames.len(),
        "balls":balls,"mesh_vertex_counts":meshes})
    }).collect::<Vec<_>>();
    let result = serde_json::json!({"song":song.title,"chart_hash":charts[0].short_hash,"retained_lua_vm":false,
        "execution_path":"compiled-rust","dynamic_compiled_scene":compiled_scene.is_some(),
        "compile_ms":compile_ms,"layers":layers,"inspection_only":true,"production_preparation_exercised":true,
        "production_parity_proven":false,"whole_song_parity_proven":false,"pixel_parity_proven":false});
    std::fs::write(&args[1], serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    println!("{result}");
}
