// Helpers frozen from 1c744eebf; parent visibility only.
use super::*;

pub(super) fn install_screen_manager_globals(
    lua: &Lua,
    top_screen: Table,
    players: [Table; LUA_PLAYERS],
) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set("__songlua_top_screen_player_1", players[0].clone())?;
    globals.set("__songlua_top_screen_player_2", players[1].clone())?;
    globals.set("__songlua_top_screen", top_screen.clone())?;
    let player_1_actor = players[0].clone();
    let player_2_actor = players[1].clone();
    globals.set(
        "GetPlayerAF",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Table(match player {
                0 => player_1_actor.clone(),
                1 => player_2_actor.clone(),
                _ => return Ok(Value::Nil),
            }))
        })?,
    )?;

    // SCREENMAN is process-owned; Lua references to it survive screen changes.
    if matches!(globals.raw_get::<Value>("SCREENMAN")?, Value::Table(_)) {
        return Ok(());
    }

    let screenman = lua.create_table()?;
    screenman.set(
        "GetTopScreen",
        lua.create_function(move |lua, _args: MultiValue| {
            if lua
                .globals()
                .raw_get::<Option<bool>>("__songlua_screen_loading")?
                .unwrap_or(false)
            {
                Ok(Value::Nil)
            } else {
                lua.globals().raw_get::<Value>("__songlua_top_screen")
            }
        })?,
    )?;
    screenman.set(
        "SystemMessage",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    screenman.set(
        "SetNewScreen",
        lua.create_function(|lua, args: MultiValue| {
            if let Some(_name) = method_arg(&args, 0).cloned().and_then(read_string) {
                #[cfg(feature = "test-support")]
                record_state_call(
                    lua,
                    "ScreenManager.SetNewScreen",
                    vec![serde_json::json!(_name)],
                )?;
                crate::live::capture_source_request(
                    lua,
                    crate::live::SongLuaLiveRequest::NewScreen(_name.clone()),
                )?;
                lua.set_app_data(SongLuaScreenRequest(_name));
                // Native ScreenManager queues the request; the current
                // screen keeps its identity through this command/update.
            }
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    screenman.set(
        "AddNewScreenToTop",
        lua.create_function(move |lua, args: MultiValue| {
            if let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) {
                let top_screen = lua.globals().get::<Table>("__songlua_top_screen")?;
                top_screen.set("Name", name)?;
            }
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    screenman.set(
        "set_input_redirected",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    globals.set("SCREENMAN", screenman)
}
