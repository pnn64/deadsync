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
 let song=out.join("fixture");fs::create_dir_all(song.join("multitap")).unwrap();let entry=song.join("default.lua");
 let factory=include_str!(r"C:/GitHub/deadsync/tests/fixtures/song-lua/multitap-factory.lua").replace("Font=\"_komika axis 42px.ini\"","Font=\"Common Normal\"");
 fs::write(song.join("multitap/Default.lua"),factory).unwrap();
 fs::write(song.join("multitap/multitap_data.lua"),"multitaps = {_version={0,9}, Challenge={{lane=1,taps={8,12},peak=6},{lane=2,taps={}},{lane=3,taps={}},{lane=4,taps={}}}}").unwrap();
 fs::write(&entry,"SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):y(10)\nreturn LoadActor('multitap/Default.lua')\n").unwrap();
 for skin in ["cyber","cel"] { for live in [false,true] {
 let mut c=SongLuaCompileContext::new(&song,"multitap boundaries");c.players[1].enabled=false;c.players[0].difficulty=SongLuaDifficulty::Challenge;c.players[0].speedmod=SongLuaSpeedMod::X(1.);c.song_timing_bpms=vec![(0.,137.)];c.music_length_seconds=6.;c.players[0].noteskin_name=skin.into();c.live=live;
 eprintln!("CASE {skin} live={live}");
 let native=deadsync_song_lua::legacy::compile_song_lua_layers_with_default_host(&[&entry],0,&c,resolver(),deadsync_assets::noteskin::load_itg_model_slots,deadsync_assets::song_lua::model_layer_from_slot,|_,_|None);
 match native {Ok(v)=>eprintln!("NATIVE {:?}",v[0].overlays.iter().map(|a|&a.name).collect::<Vec<_>>()),Err(e)=>eprintln!("NATIVE ERROR {e}")};
 let rust=deadsync_assets::song_lua::compile_song_lua_layers(&[&entry],0,&c);
 match rust {Ok(v)=>eprintln!("RUST {:?}",v[0].overlays.iter().map(|a|&a.name).collect::<Vec<_>>()),Err(e)=>eprintln!("RUST ERROR {e}")};
 let mut runtime=deadsync_song_lua::rust_engine::RustSongRuntime::new_with_noteskin_resolver(&c,resolver()).unwrap();
 if live {runtime.begin_live_song_load();}runtime.begin_song_compilation().unwrap();let(s,selected)=runtime.construct_gameplay_screen(&c).unwrap();runtime.load_gameplay_theme(s,&selected).unwrap();
 let probe=song.join("resolve-probe.lua");fs::write(&probe,r#"local old=ResolveRelativePath
ResolveRelativePath=function(path,level,optional)
 local frames={}
 for i=1,12 do local info=debug.getinfo(i,'S');if not info then break end;frames[#frames+1]=i..':'..info.source end
 Warn('PATH_PROBE '..tostring(path)..' level='..tostring(level)..' '..table.concat(frames,' | '))
 local result=old(path,level+1,optional);return result
end"#).unwrap();runtime.execute_file(&probe,&[]).unwrap();

 match runtime.execute_file(&entry,&[]){Ok(v)=>eprintln!("DIRECT OK {v:?}"),Err(e)=>eprintln!("DIRECT ERROR {e:?}")};
 }}
}
