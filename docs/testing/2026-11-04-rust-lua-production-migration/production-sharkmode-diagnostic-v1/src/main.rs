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
    log::set_max_level(log::LevelFilter::Warn);
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
    let dir = out
        .parent()
        .unwrap()
        .join("production-sharkmode-input-review-v1/inputs/song");
    let path = dir.join("default.lua");
    let mut context =
        SongLuaCompileContext::new(&dir, "existing Sharkmode initialization diagnostic");
    context.music_length_seconds = 4.;
    context.song_display_bpms = [60.; 2];
    context.song_timing_bpms = vec![(0., 60.)];
    context.screen_width = 854.;
    context.screen_height = 480.;
    context.display_width = 854.;
    context.display_height = 480.;
    context.display_aspect_ratio = 854. / 480.;
    context.video_renderers = "opengl,software".into();
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
        "limit":"Same committed scripts, four-second representative context. Diagnoses initialization; not the original full shell regression or a performance measurement."});
    std::fs::write(
        out.join("output.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
