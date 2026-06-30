use mlua::{Lua, Table};

use deadlib_platform::dirs;
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaNoteskinResolver,
    create_noteskin_table as create_crate_noteskin_table,
};

use super::actor_host::create_dummy_actor;

pub(super) fn song_lua_noteskin_resolver() -> SongLuaNoteskinResolver {
    SongLuaNoteskinResolver {
        resolve_path: crate::game::parsing::noteskin::song_lua_noteskin_resolve_path,
        metric: crate::game::parsing::noteskin::song_lua_noteskin_metric,
        metric_f: crate::game::parsing::noteskin::song_lua_noteskin_metric_f,
        metric_b: crate::game::parsing::noteskin::song_lua_noteskin_metric_b,
        exists: crate::game::parsing::noteskin::song_lua_noteskin_exists,
        names: song_lua_noteskin_names,
    }
}

fn song_lua_noteskin_names() -> Vec<String> {
    let roots = dirs::app_dirs().noteskin_roots();
    deadsync_noteskin::itg::discover_skins(&roots, "dance")
}

pub(super) fn create_noteskin_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<Table> {
    create_crate_noteskin_table(
        lua,
        context,
        song_lua_noteskin_resolver(),
        create_dummy_actor,
    )
}
