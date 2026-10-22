use super::*;

pub(crate) fn install_native(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    for (name, base, methods) in [
        ("Actor", None, ACTOR),
        ("ActorFrame", Some("Actor"), FRAME),
        ("Sprite", Some("Actor"), SPRITE),
        ("ActorFrameTexture", Some("ActorFrame"), AFT),
        ("ActorMultiVertex", Some("Actor"), AMV),
        ("ActorProxy", Some("Actor"), &["SetTarget", "GetTarget"][..]),
        ("BitmapText", Some("Actor"), BITMAP),
    ] {
        let class = lua.create_table()?;
        for method in methods {
            forward(lua, &class, method)?;
        }
        if let Some(base) = base {
            let mt = lua.create_table()?;
            mt.set("__index", globals.get::<Table>(base)?)?;
            class.set_metatable(Some(mt))?;
        }
        globals.set(name, class)?;
    }
    Ok(())
}

pub(crate) fn bind(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let bound = match actor.raw_get::<Option<Table>>(METHODS_KEY)? {
        Some(bound) => bound,
        None => lua.create_table()?,
    };
    let mut methods = Vec::new();
    actor.for_each::<Value, Value>(|key, value| {
        if let (Value::String(name), Value::Function(function)) = (&key, &value) {
            let (native, _) = method_name_flags(lua, name)?;
            if native && function.environment().is_none() {
                methods.push((key, value));
            }
        }
        Ok(())
    })?;
    for (key, value) in methods {
        bound.raw_set(&key, value)?;
        actor.raw_set(key, Value::Nil)?;
    }
    actor.raw_set(METHODS_KEY, bound)
}
