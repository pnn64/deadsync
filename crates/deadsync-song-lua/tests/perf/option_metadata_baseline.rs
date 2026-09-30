// Frozen functions from b27c63730 (0.5.1634).
use super::*;

pub(super) fn create_player_option_method(
    lua: &Lua,
    owner: &Table,
    name: &str,
) -> mlua::Result<Function> {
    let owner = owner.clone();
    let name = name.to_ascii_lowercase();
    lua.create_function(
        move |lua, (_self, value, speed): (Option<Value>, Option<Value>, Option<f32>)| {
            let state = player_option_state(lua, &owner)?;
            if let Some(value) = value {
                state.set(
                    name.as_str(),
                    normalize_player_option_value(lua, &name, value)?,
                )?;
                let speeds = player_option_speeds(lua, &owner)?;
                let speed = speed
                    .or(speeds.get::<Option<f32>>(name.as_str())?)
                    .unwrap_or(1.0);
                speeds.set(name.as_str(), speed.max(0.0))?;
                return Ok(Value::Table(owner.clone()));
            }
            Ok(match state.get::<Option<Value>>(name.as_str())? {
                Some(value) => value,
                None => default_player_option_value(lua, &name)?,
            })
        },
    )
}
