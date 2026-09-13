// Frozen from 34aca2540 (0.5.1190), before this performance pass.
use super::super::*;

pub(super) fn install_speedmod_state_method(
    lua: &Lua,
    table: &Table,
    name: &str,
    initial: Value,
) -> mlua::Result<()> {
    let owner = table.clone();
    let key = name.to_ascii_lowercase();
    let value_key = format!("__songlua_speedmod_{key}");
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

            let active = owner.raw_get::<Option<String>>("__songlua_speedmod_active")?;
            if active
                .as_deref()
                .is_some_and(|active| active != key.as_str())
            {
                return Ok(Value::Nil);
            }
            if active.as_deref() == Some(key.as_str()) {
                return Ok(owner
                    .raw_get::<Option<f32>>(value_key.as_str())?
                    .map_or(Value::Nil, |value| Value::Number(f64::from(value))));
            }
            Ok(initial.clone())
        })?,
    )
}
