use std::{path::PathBuf, fs};
use deadsync_song_lua::{SongLuaCompileContext,SongLuaDifficulty,SongLuaSpeedMod,SongLuaNoteskinResolver};
struct Logger;
impl log::Log for Logger {
 fn enabled(&self, _: &log::Metadata<'_>)->bool { true }
 fn log(&self, r:&log::Record<'_>){ eprintln!("{} {}",r.level(),r.args()); }
 fn flush(&self){}
}
static LOGGER: Logger=Logger;
fn resolver()->SongLuaNoteskinResolver { SongLuaNoteskinResolver {
 resolve_path:deadsync_assets::noteskin::song_lua_noteskin_resolve_path,
 metric:deadsync_assets::noteskin::song_lua_noteskin_metric,
 metric_f:deadsync_assets::noteskin::song_lua_noteskin_metric_f,
 metric_b:deadsync_assets::noteskin::song_lua_noteskin_metric_b,
 exists:deadsync_assets::noteskin::song_lua_noteskin_exists,
 names:deadsync_assets::noteskin::song_lua_noteskin_names,
}}
fn main(){
 log::set_logger(&LOGGER).unwrap();log::set_max_level(log::LevelFilter::Warn);
 let root=PathBuf::from(r"C:/GitHub/deadsync");let out=PathBuf::from(env!("CARGO_MANIFEST_DIR"));
 let data=out.join("data");let dirs=deadsync_config::dirs::AppDirs {cache_dir:data.join("cache"),data_dir:data.clone(),exe_dir:root.clone(),portable:false};dirs.ensure_dirs_exist();
 deadsync_config::runtime::init_paths(dirs.config_path(),dirs.judgment_palettes_path()).unwrap();
 deadsync_assets::init_paths(dirs.asset_paths(Some(&root))).unwrap();
 for (name, single) in [
 ("native_overlay_preserves_theme_tree_and_capture_probe_input",true),
 ("native_gameplay_decorations_retain_children_while_hibernating",false),
 ("native_gameplay_transitions_retain_their_loaded_auto_actors",false),
 ("native_multivertex_texture_getter_preserves_runtime_and_observation_scope",false)] {
 let entry=out.join(format!("{name}.lua"));
 let mut c=SongLuaCompileContext::new(&out,name);c.live=true;if single{c.players[1].enabled=false;}
 eprintln!("CASE {name}");
 let native=deadsync_song_lua::legacy::compile_song_lua_with_default_host::<(),(),_>(&entry,&c,Default::default(),|_,_,_|Ok(std::sync::Arc::from([])),|_|None,|_,_|None);
 eprintln!("NATIVE {:?}",native.map(|_|"OK"));
 let rust=deadsync_song_lua::compile_song_lua_with_default_host::<(),(),_>(&entry,&c,Default::default(),|_,_,_|Ok(std::sync::Arc::from([])),|_|None,|_,_|None);
 eprintln!("RUST {:?}",rust.map(|_|"OK"));
 }
}
