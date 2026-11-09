"""Make the output's scene owner generic while preserving the legacy API alias."""
from pathlib import Path
import re
r=Path(__file__).resolve().parents[3]
def edit(name,f):
 p=r/'crates/deadsync-song-lua'/name;b=p.read_bytes();s=b.decode();nl='\r\n' if '\r\n' in s else '\n';s=s.replace('\r\n','\n');s=f(s);p.write_bytes(s.replace('\n',nl).encode())
def lib(s):
 a=s.index('#[derive(Debug)]\npub struct CompiledSongLua<OverlayActor> {');b=s.index('\nimpl<OverlayActor> CompiledSongLua<OverlayActor> {',a)
 block=s[a:b];fields=re.findall(r'(?m)^    pub (\w+):',block)
 cfg={'overlay_writes','boolean_writes','speed_writes','state_calls','noteskin_writes'}
 debug='impl<A: std::fmt::Debug, L: std::fmt::Debug> std::fmt::Debug for CompiledSongLuaData<A, L> {\n    fn fmt(&self, f: &mut std::fmt::Formatter<\'_>) -> std::fmt::Result {\n        let mut out = f.debug_struct("CompiledSongLua");\n'
 for field in fields:
  if field in cfg:debug+='        #[cfg(feature = "test-support")]\n'
  debug+=f'        out.field("{field}", &self.{field});\n'
 debug+='        out.finish()\n    }\n}\n'
 block=block.replace('#[derive(Debug)]\npub struct CompiledSongLua<OverlayActor>', '/// Shared compiled payload with an engine-owned live scene.\npub struct CompiledSongLuaData<OverlayActor, LiveScene>').replace('pub live_scene: Option<SongLuaLiveScene>', 'pub live_scene: Option<LiveScene>')
 s=s[:a]+'pub type CompiledSongLua<OverlayActor> = CompiledSongLuaData<OverlayActor, SongLuaLiveScene>;\n\n'+block+'\n'+debug+s[b:]
 s=s.replace('impl<OverlayActor> CompiledSongLua<OverlayActor> {','impl<OverlayActor, LiveScene> CompiledSongLuaData<OverlayActor, LiveScene> {').replace('impl<OverlayActor> Default for CompiledSongLua<OverlayActor> {','impl<OverlayActor, LiveScene> Default for CompiledSongLuaData<OverlayActor, LiveScene> {')
 s=s.replace('pub fn sort_compiled_song_lua<OverlayActor>(compiled: &mut CompiledSongLua<OverlayActor>) {','pub fn sort_compiled_song_lua<OverlayActor, LiveScene>(compiled: &mut CompiledSongLuaData<OverlayActor, LiveScene>) {')
 return s
edit('src/lib.rs',lib)
def native(s):
 s=s.replace('type DefaultCompiledSongLua<NoteskinSlot, ModelVertex> = CompiledSongLua<','type DefaultCompiledSongLua<NoteskinSlot, ModelVertex, LiveScene = crate::SongLuaLiveScene> = crate::CompiledSongLuaData<',1)
 old='    SongLuaOverlayActor<SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>>,\n>;'
 assert old in s;s=s.replace(old,old.replace('\n>;', '\n    LiveScene,\n>;'),1)
 s=s.replace('pub(crate) fn split_compiled_song_lua<NoteskinSlot, ModelVertex>(', 'pub(crate) fn split_compiled_song_lua<NoteskinSlot, ModelVertex, LiveScene>(')
 a=s.index('pub(crate) fn split_compiled_song_lua<');s=s[:a]+s[a:].replace('DefaultCompiledSongLua<NoteskinSlot, ModelVertex>', 'DefaultCompiledSongLua<NoteskinSlot, ModelVertex, LiveScene>')
 return s
edit('src/compile.rs',native)
def rust(s):
 s=s.replace('type Output<S, V> = CompiledSongLua<SongLuaOverlayActor<Kind<S, V>>>;', 'type Output<S, V, L = crate::SongLuaLiveScene> = crate::CompiledSongLuaData<SongLuaOverlayActor<Kind<S, V>>, L>;\nmod live;\npub use live::{compile_song_lua_layers, reload_song_lua};')
 a=s.index('pub fn compile_prepared_song_lua_layers<S, V>(');b=s.index('    if paths.is_empty()',a);header=s[a:b]
 wrapper=header.replace('    mut visual:', '    visual:')+'    prepare::<S, V, crate::SongLuaLiveScene>(paths, primary, context, noteskins, slots, layer, visual)\n}\n\n'
 internal=header.replace('pub fn compile_prepared_song_lua_layers<S, V>', 'fn prepare<S, V, L>').replace('Vec<Output<S, V>>','Vec<Output<S, V, L>>')
 s=s[:a]+wrapper+internal+s[b:];s=s.replace('let mut out = Output::<S, V> {','let mut out = Output::<S, V, L> {',1)
 s=s.replace('fn finalize_tracked<S, V>(', 'fn finalize_tracked<S, V, L>(').replace('fn remove_untouched_screen<S, V>(', 'fn remove_untouched_screen<S, V, L>(').replace('out: &mut Output<S, V>,','out: &mut Output<S, V, L>,')
 return s
edit('src/rust_engine/compiler.rs',rust)
edit('src/rust_engine/observations.rs', lambda s:s.replace('fn apply<A>(self, output: &mut crate::CompiledSongLua<A>)','fn apply<A, L>(self, output: &mut crate::CompiledSongLuaData<A, L>)'))
edit('src/rust_engine.rs',lambda s:s.replace('pub use compiler::compile_prepared_song_lua_layers;', 'pub use compiler::{compile_prepared_song_lua_layers, compile_song_lua_layers, reload_song_lua};'))
print('generic scene owner and finite preparation wrapper installed')
