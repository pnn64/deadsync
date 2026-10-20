//! Compile/extraction measurements with complete DTO snapshots outside timing.
use deadsync_song_lua::*;
use mlua::{Lua, Value};
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
};

#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;

type Kind = SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;
type Compiled = CompiledSongLua<SongLuaOverlayActor<Kind>>;
type Extracted = SongLuaOverlayCompileActor<Kind>;

#[cfg(windows)]
fn pin() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
    }
    // SAFETY: the pseudo-handle identifies this worker; CPU 2 is used by the paired harness.
    assert_ne!(
        unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
        0
    );
}
#[cfg(not(windows))]
fn pin() {}

fn measure<T>(
    iterations: usize,
    batch: usize,
    mut work: impl FnMut() -> T,
    snapshot: impl Fn(&T) -> String,
) -> serde_json::Value {
    let expected = snapshot(&work());
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let measurement = metrics::Measurement::start();
        let mut output = None;
        for _ in 0..batch {
            output = Some(work());
        }
        let measurement = measurement.finish();
        assert_eq!(
            snapshot(output.as_ref().unwrap()),
            expected,
            "fixture output changed between samples"
        );
        drop(output);
        samples.push(measurement);
    }
    serde_json::json!({"operations_per_sample":batch,"samples":samples,"snapshot":expected})
}

fn read_models(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}
fn model_layer(_: &()) -> Option<SongLuaOverlayModelLayer<()>> {
    None
}
fn visual(_: &SongLuaCompileContext, _: &str) -> Option<(Kind, SongLuaOverlayState)> {
    None
}

fn run() {
    pin();
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 4, "compile_pipeline MODE INPUT REPORT SAMPLES");
    let mode = &args[0];
    let input = PathBuf::from(&args[1]);
    let samples: usize = args[3].parse().unwrap();
    assert!(samples > 0);
    let mut context =
        SongLuaCompileContext::new(input.parent().unwrap_or(Path::new("")), "pipeline fixture");
    context.live = mode != "offline";
    context.music_length_seconds = 1.0;
    if mode == "timing" || mode.starts_with("context_") {
        context.song_timing_bpms = (0..4096).map(|index| (index as f32 * 4.0, 120.0)).collect();
    }
    let report = if mode.starts_with("context_") || mode.starts_with("extract_") {
        let lua = Lua::new();
        let root = lua.create_table().unwrap();
        let count = if mode.starts_with("extract_") { 128 } else { 0 };
        let mut first = None;
        for index in 0..count {
            let actor = lua.create_table().unwrap();
            actor.raw_set("__songlua_actor_type", "Quad").unwrap();
            actor.raw_set("Name", format!("Item{index}")).unwrap();
            if index == 0 {
                first = Some(actor.clone());
            }
            root.raw_set(index + 1, actor).unwrap();
        }
        if mode == "extract_proxy" {
            let proxy = lua.create_table().unwrap();
            proxy.raw_set("__songlua_actor_type", "ActorProxy").unwrap();
            proxy
                .raw_set("__songlua_proxy_target_kind", "actor")
                .unwrap();
            proxy
                .raw_set("__songlua_proxy_target_actor", first.unwrap())
                .unwrap();
            root.raw_set(count + 1, proxy).unwrap();
        }
        let root = Value::Table(root);
        measure(
            samples,
            64,
            || {
                let owned;
                let reader = if mode == "context_owned" {
                    owned = context.clone();
                    &owned
                } else {
                    &context
                };
                let result: Vec<Extracted> = read_overlay_compile_actors(
                    &lua,
                    &root,
                    reader,
                    |_| Ok(None),
                    |_, _| Ok(None),
                    |_| {},
                )
                .unwrap();
                result
            },
            |output| {
                let mut text = String::new();
                for actor in output {
                    writeln!(&mut text, "{:?}", actor.actor).unwrap();
                }
                text
            },
        )
    } else {
        assert!(matches!(
            mode.as_str(),
            "offline" | "live" | "two_live" | "timing"
        ));
        let paths: Vec<_> =
            std::iter::repeat_n(input.as_path(), if mode == "two_live" { 2 } else { 1 }).collect();
        measure(
            samples,
            1,
            || {
                let output: Vec<Compiled> = compile_song_lua_layers_with_default_host(
                    &paths,
                    0,
                    &context,
                    SongLuaNoteskinResolver::default(),
                    read_models,
                    model_layer,
                    visual,
                )
                .unwrap();
                output
            },
            |output| format!("{output:?}"),
        )
    };
    let report = serde_json::json!({"mode":mode,"input":input,"iterations":samples,
        "measurement":report,
        "scope":"Compile: prepared context, disk read through returned compiled DTOs; output formatting/final destruction excluded. Extraction/context: prepared Lua tables, 64 operations; prior batch results dropped inside timing, final result dropped afterward. One warm operation before samples.",
        "cfg":"Normal library and example build; no song-lua test-support or performance feature."});
    std::fs::write(&args[2], serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}

fn main() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
}
