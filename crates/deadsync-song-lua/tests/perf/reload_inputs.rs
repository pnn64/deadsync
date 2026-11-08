use super::*;

#[path = "reload_before.rs"]
mod before;

fn models(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}
fn model(_: &()) -> Option<SongLuaOverlayModelLayer<()>> {
    None
}
fn visual(
    _: &SongLuaCompileContext,
    _: &str,
) -> Option<(
    SongLuaOverlayKind<(), (), TextAttribute>,
    SongLuaOverlayState,
)> {
    None
}

fn initial(layers: usize) -> crate::SongLuaLiveScene {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/perf/navigation_inputs.lua");
    let paths = vec![entry.as_path(); layers];
    let mut context = SongLuaCompileContext::new(entry.parent().unwrap(), "reload input fixture");
    context.live = true;
    context.defer_initial_live_update = true;
    context.music_length_seconds = 1.0;
    let mut compiled = compile_song_lua_layers_with_default_host(
        &paths,
        layers - 1,
        &context,
        SongLuaNoteskinResolver::default(),
        models,
        model,
        visual,
    )
    .unwrap();
    compiled
        .iter_mut()
        .find_map(|c| c.live_scene.take())
        .expect("retained scene")
}

fn reload(
    current: bool,
    scene: crate::SongLuaLiveScene,
) -> Result<Vec<DefaultCompiledSongLua<(), ()>>, String> {
    if current {
        super::reload_song_lua_with_default_host(scene, models, model, visual)
    } else {
        before::reload_song_lua_with_default_host(scene, models, model, visual)
    }
}

#[test]
fn moved_reload_paths_keep_layer_order_startup_output_and_scene() {
    for layers in [1, 2, 8] {
        let expected = reload(false, initial(layers)).unwrap();
        let actual = reload(true, initial(layers)).unwrap();
        assert_eq!(format!("{expected:?}"), format!("{actual:?}"));
        assert_eq!(actual.len(), layers);
        assert_eq!(actual.iter().filter(|c| c.live_scene.is_some()).count(), 1);
        let expected_scene = expected.into_iter().find_map(|c| c.live_scene).unwrap();
        let actual_scene = actual.into_iter().find_map(|c| c.live_scene).unwrap();
        assert_eq!(expected_scene.entry_paths, actual_scene.entry_paths);
        assert_eq!(actual_scene.primary_index, layers - 1);
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_reload_path_moves() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, layers) in [
        ("reload_one_layer", 1),
        ("reload_two_layers", 2),
        ("reload_eight_layers", 8),
    ] {
        for current in order {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let scene = initial(layers);
                let start = Instant::now();
                let begin = cycles();
                let (result, churn) = allocations::measure(|| reload(current, scene));
                let used = cycles() - begin;
                let nanos = start.elapsed().as_nanos();
                black_box(result.unwrap());
                if sample > 0 {
                    samples.push(
                        serde_json::json!({"ns": nanos, "thread_cycles": used, "units": 1,
                        "allocations": churn.allocs, "reallocations": churn.reallocs,
                        "allocated_bytes": churn.allocated_bytes, "frees": churn.frees}),
                    );
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload": name,
                "variant": if current { "after" } else { "before" }, "unit": "reloads",
                "baseline_commit": "82decba51", "samples": samples})
            );
        }
    }
}
