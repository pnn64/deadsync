// Binding helpers frozen from 31a7355f5 with parent visibility.
use super::*;

pub(super) fn bind(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let bound = match actor.raw_get::<Option<Table>>(METHODS_KEY)? {
        Some(bound) => bound,
        None => lua.create_table()?,
    };
    let mut methods = Vec::new();
    actor.for_each::<Value, Value>(|key, value| {
        if let (Value::String(name), Value::Function(function)) = (&key, &value) {
            let name = name.to_str()?;
            if !name.starts_with("__songlua_")
                && !name.ends_with("Command")
                && function.info().what == "C"
            {
                methods.push((key, value));
            }
        }
        Ok(())
    })?;
    for (key, value) in methods {
        bound.raw_set(key.clone(), value)?;
        actor.raw_set(key, Value::Nil)?;
    }
    actor.raw_set(METHODS_KEY, bound)
}

pub(super) fn assign(_: &Lua, (actor, key, value): (Table, Value, Value)) -> mlua::Result<()> {
    if let (Value::String(name), Value::Function(function)) = (&key, &value) {
        let name = name.to_str()?;
        if !name.starts_with("__songlua_")
            && !name.ends_with("Command")
            && function.info().what == "C"
            && let Some(methods) = actor.raw_get::<Option<Table>>(METHODS_KEY)?
        {
            return methods.raw_set(key, value);
        }
    }
    actor.raw_set(key, value)
}
