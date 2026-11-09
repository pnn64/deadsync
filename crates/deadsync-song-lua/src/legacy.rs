//! Explicit native oracle API for existing differential controls.
//! Production callers use the crate facade and the shared Rust engine.
pub use crate::actor_input::{SongLuaCompiledActorInputs, SongLuaCompiledActorInteractions};
pub use crate::compile::{
    compile_song_lua_layers_with_actors, compile_song_lua_layers_with_default_host,
    compile_song_lua_with_actors, compile_song_lua_with_default_host,
    reload_song_lua_with_default_host,
};
pub use crate::input::SongLuaCompiledInput;
pub use crate::live::SongLuaLiveScene;
pub type CompiledSongLua<A> = crate::CompiledSongLuaData<A, SongLuaLiveScene>;
