//! Headless opening-effect regression and Lua-frame profile using a native trace's clocks.
//! Run with the unchanged 7th Gear directory and the retained opening pose fixture.
use deadsync_song_lua::*;
use std::{path::PathBuf, sync::Arc, time::Instant};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(
        args.len(),
        2,
        "seventh_profile <song directory> <native frame/pose fixture>"
    );
    let fixture: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&args[1]).unwrap()).unwrap();
    let display = &fixture["display"];
    let dir = PathBuf::from(&args[0]);
    let mut context = SongLuaCompileContext::new(&dir, "7th Gear");
    context.live = true;
    context.defer_initial_live_update = true;
    context.layer_start_beats = vec![0.0];
    context.video_renderers = display["video_renderers"].as_str().unwrap().into();
    context.screen_width = display["logical_width"].as_f64().unwrap() as f32;
    context.screen_height = display["logical_height"].as_f64().unwrap() as f32;
    context.display_width = display["width"].as_f64().unwrap() as f32;
    context.display_height = display["height"].as_f64().unwrap() as f32;
    context.players[1].enabled = false;
    context.players[0].step_statistics_enabled = false;
    context.music_length_seconds = 131.0;
    type Compiled = CompiledSongLua<
        SongLuaOverlayActor<SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>>,
    >;
    let mut compiled: Compiled = compile_song_lua_with_default_host(
        &dir.join("lua/default.lua"),
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .unwrap();
    let samples = fixture["samples"].as_array().unwrap();
    let probes: Vec<_> = samples[0]["poses"]
        .as_object()
        .unwrap()
        .keys()
        .map(|name| {
            let i = compiled
                .overlays
                .iter()
                .position(|actor| actor.name.as_ref() == Some(name))
                .unwrap();
            (name.clone(), compiled.live_actor_indices()[i])
        })
        .collect();
    let mut scene = compiled.live_scene.take().unwrap();
    let mut times = Vec::new();
    let mut checks = 0;
    for sample in samples {
        let f = &sample["frame"];
        let seconds = f["seconds"].as_f64().unwrap();
        let start = Instant::now();
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: f["wall_seconds"].as_f64().unwrap(),
                music_seconds: seconds,
                beat: f["beat"].as_f64().unwrap(),
                delta_seconds: f["delta"].as_f64().unwrap(),
                ..Default::default()
            })
            .unwrap();
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        for (name, index) in &probes {
            let actual = &scene.states()[*index];
            for (field, expected) in sample["poses"][name].as_object().unwrap() {
                if field == "visible" {
                    assert_eq!(
                        actual.visible,
                        expected.as_bool().unwrap(),
                        "{name}.{field} at {seconds}"
                    );
                } else {
                    let actual = match field.as_str() {
                        "x" => actual.x,
                        "skewx" => actual.skew_x,
                        "croptop" => actual.croptop,
                        "cropbottom" => actual.cropbottom,
                        _ => panic!("unsupported pose field: {field}"),
                    };
                    let expected = expected.as_f64().unwrap() as f32;
                    assert!(
                        (actual - expected).abs() <= 0.0001,
                        "{name}.{field} at {seconds}: expected {expected}, got {actual}"
                    );
                }
                checks += 1;
            }
        }
    }
    assert!(checks > 0);
    times.sort_by(f64::total_cmp);
    println!(
        "PASS opening_actor_pose_checks={checks}; pixel_parity_proven=false; full_song_parity_proven=false"
    );
    println!(
        "PROFILE frames={} mean_ms={} p95_ms={} worst_ms={}",
        times.len(),
        times.iter().sum::<f64>() / times.len() as f64,
        times[times.len() * 95 / 100],
        times.last().unwrap()
    );
}
