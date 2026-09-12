// Frozen from 619d10d4a340b2d5aa9cebe90ea38dc080059338; only test visibility/formatting differs.

use super::*;

pub(super) fn restore_scalar_globals(
    lua: &Lua,
    snapshot: Vec<(String, Value)>,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let keys = snapshot
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<HashSet<_>>();
    let mut remove = Vec::new();
    for pair in globals.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(key) = key else {
            continue;
        };
        let key = key.to_str()?;
        if is_scalar_lua_value(&value) && !keys.contains(key.as_ref()) {
            remove.push(key.to_string());
        }
    }
    for key in remove {
        globals.set(key, Value::Nil)?;
    }
    for (key, value) in snapshot {
        globals.set(key, value)?;
    }
    Ok(())
}
