//! Paired real scene input/native actor execution and exact state traces.
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
    // SAFETY: changes only this disposable benchmark worker's affinity.
    assert_ne!(
        unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
        0
    );
}
#[cfg(not(windows))]
fn pin() {}

fn load(path: &Path) -> SongLuaLiveScene {
    let mut context = SongLuaCompileContext::new(path.parent().unwrap(), "actor input benchmark");
    context.live = true;
    context.music_length_seconds = 1.0;
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

fn tracked(
    rows: impl Iterator<Item = (SongLuaTrackedActorTarget, SongLuaOverlayState)>,
) -> Vec<(usize, usize, SongLuaOverlayState)> {
    rows.map(|(target, state)| {
        use SongLuaTrackedActorTarget::*;
        let (kind, index) = match target {
            Player(p) => (0, p),
            PlayerJudgment(p) => (1, p),
            PlayerCombo(p) => (2, p),
            SongForeground => (3, 0),
            ScreenLayer(p) => (4, p),
        };
        (kind, index, state)
    })
    .collect()
}

fn event(index: usize) -> SongLuaInputEvent<'static> {
    SongLuaInputEvent {
        player: if index % 4 == 0 {
            None
        } else {
            Some(index % 2)
        },
        controller: Some(0),
        button: ["Left", "Right", "Other"][index % 3],
        game_button: ["Left", "Right", "Other"][index % 3],
        kind: [
            SongLuaInputType::FirstPress,
            SongLuaInputType::Repeat,
            SongLuaInputType::Release,
        ][index % 3],
        device: "InputDevice_Key",
        device_button: "DeviceButton_left",
        level: [-0.0, 0.25, 0.5, 1.0, f32::from_bits(0x7fc0_1234)][index % 5],
        z: index as i32 - 30,
        down: index % 4 < 2,
        ago: -0.0,
        is_joystick: false,
        is_mouse: false,
    }
}

fn main() {
    pin();
    let out = std::env::args().nth(1).expect("actor_input REPORT");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = std::env::args()
        .nth(2)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("tests/fixtures/actor-input.lua"));
    let mut live = load(&path);
    let mut native = live
        .compile_actor_inputs()
        .expect("visibility input lowering");
    let mut trace = Vec::new();
    for index in 0..60 {
        let input = event(index);
        let (handled, output) = live.input(&input).unwrap();
        assert!(output.errors.is_empty());
        let mut actions = Vec::new();
        assert_eq!(
            native.input_with_actions(&input, |action| actions.push(action)),
            handled
        );
        assert_eq!(native.states(), live.states());
        assert_eq!(
            tracked(native.tracked_states()),
            tracked(live.tracked_states())
        );
        trace.push(serde_json::json!({"handled":handled,"actions":format!("{actions:?}"),
            "states":format!("{:?}",native.states()),"tracked":format!("{:?}",tracked(native.tracked_states()))}));
    }
    let memory = serde_json::json!({"native_payload_bytes":native.retained_bytes(),
        "retained_lua_allocator_bytes":live.lua_memory_bytes(),
        "scope":"native owned input code, all overlay/tracked states and target identities; allocator/Arc bookkeeping excluded"});
    let mut rows = Vec::new();
    let runtime_samples = if std::env::args().nth(3).as_deref() == Some("prepare") {
        0
    } else {
        10
    };
    for sample in 0..runtime_samples {
        for current in [sample % 2 == 0, sample % 2 != 0] {
            let measure = metrics::Measurement::start();
            let mut digest = 0u64;
            for _ in 0..128 {
                for index in 0..60 {
                    let input = event(index);
                    let handled = if current {
                        native.input(black_box(&input))
                    } else {
                        live.input(black_box(&input)).unwrap().0
                    };
                    digest = digest.wrapping_mul(31).wrapping_add(u64::from(handled));
                }
            }
            let mut row = measure.finish();
            black_box(digest);
            if sample == 0 {
                continue;
            }
            row["workload"] = "actor_input_runtime".into();
            row["current"] = current.into();
            row["sample"] = sample.into();
            row["operations"] = (128 * 60).into();
            rows.push(row);
        }
    }
    for sample in 0..10 {
        let measure = metrics::Measurement::start();
        for _ in 0..128 {
            black_box(live.compile_actor_inputs().unwrap());
        }
        let mut row = measure.finish();
        if sample == 0 {
            continue;
        }
        row["workload"] = "actor_input_snapshot_prepare".into();
        row["operations"] = 128.into();
        row["sample"] = sample.into();
        rows.push(row);
    }
    // A Lua finalizer can run only when the actual loading VM closes. The
    // marker is rooted globally and never captured by the input callbacks.
    let marker =
        std::env::temp_dir().join(format!("deadsync-input-vm-drop-{}.txt", std::process::id()));
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("input.lua");
    let source = format!(
        "local proxy=newproxy(true); getmetatable(proxy).__gc=function() local f=assert(io.open([[{}]],'w'));f:write('closed');f:close() end; _G.input_drop_marker=proxy;\n{}",
        marker.to_string_lossy().replace('\\', "/"),
        std::fs::read_to_string(&path).unwrap()
    );
    std::fs::write(&file, source).unwrap();
    let scene = load(&file);
    let mut detached = scene.compile_actor_inputs().unwrap();
    if marker.exists() {
        std::fs::remove_file(&marker).unwrap();
    }
    assert!(!marker.exists());
    drop(scene);
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "closed");
    std::fs::remove_file(&marker).unwrap();
    for index in 0..60 {
        assert_eq!(
            detached.input(&event(index)),
            trace[index]["handled"].as_bool().unwrap()
        );
    }
    std::fs::write(out,serde_json::to_vec_pretty(&serde_json::json!({"rows":rows,"trace":trace,
        "retained_memory":memory,"loading_vm_destroyed":true,
        "validation":"Exact real-scene Lua/native actions and complete actor states over 60 inputs; native interaction runs after loading VM finalizer. Other scene lifecycle remains Lua. No pixel claim."})).unwrap()).unwrap();
}
