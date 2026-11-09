use deadsync_song_lua::SongLuaCompileContext;
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

struct Logger;
impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        eprintln!("{} {}", record.level(), record.args());
    }
    fn flush(&self) {}
}
static LOGGER: Logger = Logger;

fn summary<A, S>(value: deadsync_song_lua::CompiledSongLuaData<A, S>) -> serde_json::Value {
    json!({"overlays":value.overlays.len(),"draw_frames":value.draw_frames.len(),
        "overlay_update_tracks":value.overlay_updates.len(),"info":format!("{:?}",value.info)})
}
fn main() {
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(if std::env::args().any(|v| v == "--trace") {
        log::LevelFilter::Trace
    } else {
        log::LevelFilter::Warn
    });
    let root = PathBuf::from("C:/GitHub/deadsync");
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let data = out.join("data");
    let dirs = deadsync_config::dirs::AppDirs {
        cache_dir: data.join("cache"),
        data_dir: data,
        exe_dir: root.clone(),
        portable: false,
    };
    dirs.ensure_dirs_exist();
    deadsync_config::runtime::init_paths(dirs.config_path(), dirs.judgment_palettes_path())
        .unwrap();
    deadsync_assets::init_paths(dirs.asset_paths(Some(&root))).unwrap();
    let path = out.join("texture-control.lua");
    let mut context =
        SongLuaCompileContext::new(&out, "existing copied texture observation diagnostic");
    context.live = true;
    let native = deadsync_song_lua::legacy::compile_song_lua_with_default_host::<(), (), _>(
        &path,
        &context,
        Default::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    );
    let native = native.map(summary).map_err(|e| e.to_string());
    eprintln!("NATIVE {native:?}");
    let rust = deadsync_song_lua::compile_song_lua_with_default_host::<(), (), _>(
        &path,
        &context,
        Default::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    );
    let rust = rust.map(summary).map_err(|e| e.to_string());
    eprintln!("RUST {rust:?}");
    let report = json!({"native":native,"rust":rust,
        "limit":"Original copied-AMV operation with raw-field diagnostics; compare trace logging off/on. Not original root regression acceptance."});
    std::fs::write(
        out.join("output.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
