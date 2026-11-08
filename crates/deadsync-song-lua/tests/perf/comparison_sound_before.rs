// Frozen from 142996521; unchanged helpers remain shared.
use super::*;

pub(super) fn record_song_lua_sound_path(
    lua: &Lua,
    song_dir: &Path,
    args: &MultiValue,
    capture_call: bool,
) -> mlua::Result<()> {
    let Some(raw_path) = method_arg(args, 0).cloned().and_then(read_string) else {
        return Ok(());
    };
    let Ok(path) = resolve_script_path(lua, song_dir, &raw_path) else {
        return Ok(());
    };
    if !path.is_file() || !is_song_lua_audio_path(path.as_path()) {
        return Ok(());
    }

    let key = path.to_string_lossy();
    if capture_call {
        crate::lua_util::capture_runtime_sound(lua, path.clone());
        if let Some(calls) = lua
            .globals()
            .get::<Option<Table>>(SONG_LUA_SOUND_CALLS_KEY)?
        {
            calls.raw_set(calls.raw_len() + 1, key.as_ref())?;
        }
    }
    let paths = song_lua_sound_paths_table(lua)?;
    for existing in paths.sequence_values::<String>() {
        if existing? == key.as_ref() {
            return Ok(());
        }
    }
    paths.raw_set(paths.raw_len() + 1, key.as_ref())?;
    Ok(())
}
