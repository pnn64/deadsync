// Frozen from a82c06196b06e11cd809dd5cd8972ecc007d6999; only test visibility differs.

use super::*;

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
        lua.create_function(move |_, args: MultiValue| {
            if let Some(value) = method_arg(&args, 0).cloned() {
                if matches!(value, Value::Nil) {
                    set_player_speedmod(&owner, key.as_str(), None)?;
                } else if let Some(value) = read_f32(value) {
                    set_player_speedmod(&owner, key.as_str(), Some(value))?;
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

pub(super) fn set_player_speedmod(
    owner: &Table,
    key: &str,
    value: Option<f32>,
) -> mlua::Result<()> {
    let value_key = format!("__songlua_speedmod_{key}");
    if let Some(value) = value {
        owner.raw_set("__songlua_speedmod_active", key)?;
        owner.raw_set(value_key.as_str(), value)?;
    } else {
        owner.raw_set("__songlua_speedmod_active", "none")?;
        owner.raw_set(value_key.as_str(), Value::Nil)?;
    }
    Ok(())
}
