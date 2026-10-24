//! Actual input/update callback execution, exact state traces and VM release.
use deadsync_song_lua::*;
use std::{hint::black_box, path::Path, sync::Arc};
#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;
type Kind = SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;
#[cfg(windows)]
fn pin() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
    }
    // SAFETY: changes only this disposable worker thread's affinity.
    assert_ne!(
        unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
        0
    );
}
#[cfg(not(windows))]
fn pin() {}
fn load(path: &Path) -> SongLuaLiveScene {
    let mut context = SongLuaCompileContext::new(path.parent().unwrap(), "timed input benchmark");
    context.live = true;
    context.music_length_seconds = 1_000_000.0;
    let mut compiled = compile_song_lua_layers_with_default_host(
        &[path],
        0,
        &context,
        SongLuaNoteskinResolver::default(),
        |_: &Path, _: &Path, _: &Path| Ok(Arc::<[()]>::from([])),
        |_: &()| None,
        |_: &SongLuaCompileContext, _: &str| None::<(Kind, SongLuaOverlayState)>,
    )
    .unwrap();
    compiled[0].live_scene.take().unwrap()
}
fn event(index: usize) -> SongLuaInputEvent<'static> {
    SongLuaInputEvent {
        player: Some(index % 2),
        controller: Some(0),
        button: ["Right", "Other", "Left"][(index / 4) % 3],
        game_button: ["Right", "Other", "Left"][(index / 4) % 3],
        kind: [
            SongLuaInputType::FirstPress,
            SongLuaInputType::Repeat,
            SongLuaInputType::Release,
        ][index % 3],
        device: "InputDevice_Key",
        device_button: "DeviceButton_right",
        level: [-0.5, -0.0, 0.25, 0.75][index % 4],
        z: 0,
        down: index % 3 != 2,
        ago: -0.0,
        is_joystick: false,
        is_mouse: false,
    }
}
fn delta(index: usize) -> f64 {
    [0.03125, 1.0 / 60.0, 0.0, -0.0, 0.125, 0.001][index % 6]
}
fn frame(index: usize, seconds: f64) -> SongLuaLiveFrame {
    SongLuaLiveFrame {
        delta_seconds: delta(index),
        beat: seconds * 2.0,
        music_seconds: seconds,
        screen_seconds: seconds,
        ..Default::default()
    }
}
fn scalars(owner: &SongLuaCompiledActorInteractions) -> Vec<(usize, Vec<u8>, u64)> {
    let mut result = owner
        .scalar_states()
        .map(|(table, key, value)| {
            (
                table,
                key.to_vec(),
                match value {
                    SongLuaInputScalar::Boolean(v) => u64::from(v),
                    SongLuaInputScalar::Number(v) => v.to_bits(),
                },
            )
        })
        .collect::<Vec<_>>();
    result.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    result
}
fn main() {
    pin();
    let report = std::env::args().nth(1).expect("timed_input REPORT");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/timed-input.lua");
    let mut scene = load(&path);
    let mut native = scene
        .compile_actor_interactions()
        .expect("input/update lowering");
    let mut trace = Vec::new();
    let mut seconds = 0.0;
    for index in 0..96 {
        let input = event(index);
        let (handled, out) = scene.input(&input).unwrap();
        assert!(out.errors.is_empty());
        let mut actions = Vec::new();
        assert_eq!(
            native.input_with_actions(&input, |a| actions.push(a)),
            handled
        );
        assert_eq!(native.states(), scene.states());
        let expected = scene.compile_actor_interactions().unwrap();
        assert_eq!(scalars(&native), scalars(&expected));
        trace.push(serde_json::json!({"phase":"input","handled":handled,"actions":format!("{actions:?}"),"states":format!("{:?}",native.states()),"scalars":scalars(&native)}));
        seconds += delta(index);
        let out = scene.update(frame(index, seconds)).unwrap();
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        let mut actions = Vec::new();
        native.update_with_actions(delta(index), |a| actions.push(a));
        assert_eq!(native.states(), scene.states());
        let expected = scene.compile_actor_interactions().unwrap();
        assert_eq!(scalars(&native), scalars(&expected));
        trace.push(serde_json::json!({"phase":"update","actions":format!("{actions:?}"),"states":format!("{:?}",native.states()),"scalars":scalars(&native)}));
    }
    let memory = serde_json::json!({"native_payload_bytes":native.retained_bytes(),"current_lua_allocator_bytes":scene.lua_memory_bytes(),"scope":"Native input/update code, scalar cells, actor states and schedule payload. Allocator/Arc bookkeeping excluded; Lua bytes include pending garbage without forced GC and exclude Rust payload."});
    let mut rows = Vec::new();
    for sample in 0..10 {
        let mut expected = None;
        for current in [sample % 2 == 0, sample % 2 != 0] {
            let measure = metrics::Measurement::start();
            let mut digest = 0u64;
            for _ in 0..16 {
                for index in 0..96 {
                    let input = event(index);
                    let handled = if current {
                        let h = native.input(black_box(&input));
                        native.update(black_box(delta(index)));
                        h
                    } else {
                        let h = scene.input(black_box(&input)).unwrap().0;
                        seconds += delta(index);
                        scene.update(black_box(frame(index, seconds))).unwrap();
                        h
                    };
                    digest = digest.wrapping_mul(31).wrapping_add(u64::from(handled));
                }
            }
            let mut row = measure.finish();
            black_box(digest);
            if let Some(value) = expected {
                assert_eq!(value, digest)
            } else {
                expected = Some(digest)
            };
            if sample == 0 {
                continue;
            }
            row["current"] = current.into();
            row["sample"] = sample.into();
            row["operations"] = (16 * 96).into();
            row["workload"] = "input_update_pair".into();
            rows.push(row);
        }
    }
    for sample in 0..10 {
        let measure = metrics::Measurement::start();
        for _ in 0..128 {
            black_box(scene.compile_actor_interactions().unwrap());
        }
        let mut row = measure.finish();
        if sample == 0 {
            continue;
        }
        row["operations"] = 128.into();
        row["sample"] = sample.into();
        row["workload"] = "input_update_prepare".into();
        rows.push(row);
    }
    let marker =
        std::env::temp_dir().join(format!("deadsync-timed-vm-drop-{}.txt", std::process::id()));
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("input.lua");
    let source = format!(
        "local p=newproxy(true);getmetatable(p).__gc=function()local f=assert(io.open([[{}]],'w'));f:write('closed');f:close()end;_G.vm_drop_marker=p;\n{}",
        marker.to_string_lossy().replace('\\', "/"),
        std::fs::read_to_string(path).unwrap()
    );
    std::fs::write(&file, source).unwrap();
    let source = load(&file);
    let mut detached = source.compile_actor_interactions().unwrap();
    if marker.exists() {
        std::fs::remove_file(&marker).unwrap();
    }
    assert!(!marker.exists());
    drop(source);
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "closed");
    std::fs::remove_file(&marker).unwrap();
    for index in 0..96 {
        assert_eq!(
            detached.input(&event(index)),
            trace[index * 2]["handled"].as_bool().unwrap()
        );
        assert_eq!(
            format!("{:?}", detached.states()),
            trace[index * 2]["states"].as_str().unwrap()
        );
        detached.update(delta(index));
        assert_eq!(
            format!("{:?}", detached.states()),
            trace[index * 2 + 1]["states"].as_str().unwrap()
        );
        assert_eq!(
            serde_json::to_value(scalars(&detached)).unwrap(),
            trace[index * 2 + 1]["scalars"]
        );
    }
    std::fs::write(report,serde_json::to_vec_pretty(&serde_json::json!({"rows":rows,"trace":trace,"retained_memory":memory,"loading_vm_destroyed":true})).unwrap()).unwrap();
}
