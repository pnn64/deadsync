// Helpers frozen from 4277c1b63 with parent visibility.
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
                && function.environment().is_none()
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

pub(super) fn lookup(lua: &Lua, (actor, key): (Table, Value)) -> mlua::Result<Value> {
    let Value::String(name) = &key else {
        return Ok(Value::Nil);
    };
    let name = name.to_str()?;
    let kind = actor
        .raw_get::<Option<crate::lua_util::LuaFieldText<64>>>("__songlua_actor_type")?;
    let kind = kind.as_ref().map_or("Actor", |kind| kind.as_str());
    if let Some(class) = lua.globals().get::<Option<Table>>(class_name(kind))? {
        let method = class.get::<Value>(key.clone())?;
        if !method.is_nil() { return Ok(method); }
        if is_core(kind) { return Ok(Value::Nil); }
    }
    // Unlinked classes retain their existing own adapters until their native
    // inventories are audited. Frame callbacks are never inherited by Actor.
    if !is_frame(kind)
        && matches!(
            name.as_ref(),
            "SetUpdateFunction" | "SetDrawFunction" | "GetDrawFunction"
        )
    {
        return Ok(Value::Nil);
    }
    match actor.raw_get::<Option<Table>>(METHODS_KEY)? {
        Some(methods) => methods.raw_get(key),
        None => Ok(Value::Nil),
    }
}

pub(super) fn assign(_: &Lua, (actor, key, value): (Table, Value, Value)) -> mlua::Result<()> {
    if let (Value::String(name), Value::Function(function)) = (&key, &value) {
        let name = name.to_str()?;
        if !name.starts_with("__songlua_")
            && !name.ends_with("Command")
            && function.environment().is_none()
            && let Some(methods) = actor.raw_get::<Option<Table>>(METHODS_KEY)?
        {
            return methods.raw_set(key, value);
        }
    }
    actor.raw_set(key, value)
}
