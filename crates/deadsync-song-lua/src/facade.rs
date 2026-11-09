//! Unconditional application entry points over the shared Rust Lua engine.
use crate::{
    CompiledSongLua, SongLuaCompileContext, SongLuaLiveScene, SongLuaNoteskinResolver,
    SongLuaOverlayActor, SongLuaOverlayKind, SongLuaOverlayModelLayer, SongLuaOverlayState,
};
use deadlib_present::actors::TextAttribute;
use std::{path::Path, sync::Arc};

type Kind<S, V> = SongLuaOverlayKind<S, V, TextAttribute>;
type Output<S, V> = CompiledSongLua<SongLuaOverlayActor<Kind<S, V>>>;
type ModelSlots<S> = fn(&Path, &Path, &Path) -> Result<Arc<[S]>, String>;

pub fn compile_song_lua_with_default_host<S, V, F>(
    path: &Path,
    context: &SongLuaCompileContext,
    noteskins: SongLuaNoteskinResolver,
    slots: ModelSlots<S>,
    layer: fn(&S) -> Option<SongLuaOverlayModelLayer<V>>,
    visual: F,
) -> Result<Output<S, V>, String>
where
    F: FnMut(&SongLuaCompileContext, &str) -> Option<(Kind<S, V>, SongLuaOverlayState)>,
{
    compile_song_lua_layers_with_default_host(&[path], 0, context, noteskins, slots, layer, visual)?
        .into_iter()
        .next()
        .ok_or_else(|| "song lua compiler returned no result for one entry".to_string())
}

pub fn compile_song_lua_layers_with_default_host<S, V, F>(
    paths: &[&Path],
    primary: usize,
    context: &SongLuaCompileContext,
    noteskins: SongLuaNoteskinResolver,
    slots: ModelSlots<S>,
    layer: fn(&S) -> Option<SongLuaOverlayModelLayer<V>>,
    visual: F,
) -> Result<Vec<Output<S, V>>, String>
where
    F: FnMut(&SongLuaCompileContext, &str) -> Option<(Kind<S, V>, SongLuaOverlayState)>,
{
    crate::rust_engine::compile_song_lua_layers(
        paths, primary, context, noteskins, slots, layer, visual,
    )
}

pub fn reload_song_lua_with_default_host<S, V, F>(
    scene: SongLuaLiveScene,
    slots: ModelSlots<S>,
    layer: fn(&S) -> Option<SongLuaOverlayModelLayer<V>>,
    _visual: F,
) -> Result<Vec<Output<S, V>>, String>
where
    F: FnMut(&SongLuaCompileContext, &str) -> Option<(Kind<S, V>, SongLuaOverlayState)>,
{
    crate::rust_engine::reload_song_lua(scene, slots, layer)
}
