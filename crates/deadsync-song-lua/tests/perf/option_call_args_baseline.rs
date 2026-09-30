// Frozen callback functions from a960e1c6f (0.5.1633).
use super::*;

pub(super) fn create_song_options_table(lua: &Lua, music_rate: f32) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("__songlua_music_rate", music_rate.max(0.0))?;
    table.set(
        "MusicRate",
        lua.create_function(move |_, args: MultiValue| {
            let Some(owner) = args.front().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return Ok(1.0_f32);
            };
            if let Some(rate) = method_arg(&args, 0).cloned().and_then(read_f32) {
                owner.set("__songlua_music_rate", rate.max(0.0))?;
                return Ok(rate.max(0.0));
            }
            Ok(owner
                .get::<Option<f32>>("__songlua_music_rate")?
                .unwrap_or(1.0_f32))
        })?,
    )?;
    Ok(table)
}

pub(super) fn install_speedmod_state_method(
    lua: &Lua,
    table: &Table,
    name: &str,
    initial: Value,
) -> mlua::Result<()> {
    let owner = table.clone();
    let key = name.to_ascii_lowercase();
    let value_key = format!("__songlua_speedmod_{key}");
    // Retain the small set of values written by speedmod setters. Comparing Lua
    // handles avoids both owned text and BorrowedStr's shared-reference allocation.
    let active_keys = [
        key.as_str(),
        "xmod",
        "cmod",
        "mmod",
        "amod",
        "camod",
        "none",
        "",
    ]
    .into_iter()
    .map(|key| {
        let key = lua.create_string(key)?;
        let identity = key.to_pointer() as usize;
        Ok((key, identity))
    })
    .collect::<mlua::Result<Vec<_>>>()?
    .into_boxed_slice();
    table.set(
        name,
        lua.create_function(move |lua, args: MultiValue| {
            if let Some(value) = method_arg(&args, 0).cloned() {
                if matches!(value, Value::Nil) {
                    set_player_speedmod_with_key(&owner, &key, &value_key, None)?;
                } else if let Some(value) = read_f32(value) {
                    set_player_speedmod_with_key(&owner, &key, &value_key, Some(value))?;
                    if matches!(key.as_str(), "xmod" | "cmod" | "mmod") {
                        let speed = method_arg(&args, 1).cloned().and_then(read_f32);
                        set_player_speed_approaches(lua, &owner, speed)?;
                    }
                }
                return Ok(Value::Table(owner.clone()));
            }

            let active = player_speedmod_is_active(&owner, &key, &active_keys)?;
            if active == Some(false) {
                return Ok(Value::Nil);
            }
            if active == Some(true) {
                return Ok(owner
                    .raw_get::<Option<f32>>(value_key.as_str())?
                    .map_or(Value::Nil, |value| Value::Number(f64::from(value))));
            }
            Ok(initial.clone())
        })?,
    )
}
