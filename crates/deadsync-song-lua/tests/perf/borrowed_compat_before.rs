// Frozen callback bodies from be95399d3; unchanged downstream helpers remain shared.
use super::*;

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let callbacks = SongLuaCompatCallbacks {
        current_gamestate_player_value,
        current_gamestate_value,
        current_song_value,
        current_steps_value,
        retarget_loader_env,
    };
    globals.set(
        "VarBefore",
        lua.create_function(move |lua, args: MultiValue| {
            let name = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            if name == "GameCommand" {
                return Ok(Value::Table(create_game_command_table(lua, callbacks)?));
            }
            Ok(Value::String(lua.create_string(&name)?))
        })?,
    )?;
    globals.set(
        "cleanGSubBefore",
        lua.create_function(|lua, args: MultiValue| {
            let text = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let needle = args
                .get(1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let replacement = args
                .get(2)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(Value::String(
                lua.create_string(text.replace(&needle, &replacement))?,
            ))
        })?,
    )?;
    globals.set(
        "GetVersionPartsBefore",
        lua.create_function(|lua, args: MultiValue| {
            let version = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_else(|| SONG_LUA_PRODUCT_VERSION.to_string());
            create_version_parts_table(lua, &version)
        })?,
    )?;
    Ok(())
}
