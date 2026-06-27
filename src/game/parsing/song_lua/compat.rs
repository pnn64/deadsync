use mlua::Lua;
use std::path::Path;

use deadsync_song_lua::{
    SongLuaCompatCallbacks, install_stdlib_compat as install_crate_stdlib_compat,
};

use super::actor_host::{
    current_gamestate_player_value, current_gamestate_value, current_song_value,
    current_steps_value, retarget_loader_env,
};

pub(super) fn install_stdlib_compat(lua: &Lua, song_dir: &Path) -> mlua::Result<()> {
    install_crate_stdlib_compat(
        lua,
        song_dir,
        SongLuaCompatCallbacks {
            current_gamestate_player_value,
            current_gamestate_value,
            current_song_value,
            current_steps_value,
            retarget_loader_env,
        },
    )
}
