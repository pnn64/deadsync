use super::*;

pub(crate) fn forward(lua: &Lua, class: &Table, name: &'static str) -> mlua::Result<()> {
    class.set(
        name,
        lua.create_function(move |lua, args: MultiValue| {
            let Some(Value::Table(actor)) = args.front() else {
                return Err(mlua::Error::RuntimeError(format!(
                    "actor method {name} requires an actor"
                )));
            };
            let bound = actor.raw_get::<Option<Table>>(METHODS_KEY)?;
            let method = match bound {
                Some(bound) => bound.raw_get::<Value>(name)?,
                None => actor.raw_get::<Value>(name)?,
            };
            let Value::Function(method) = method else {
                return Err(mlua::Error::RuntimeError(format!(
                    "native actor method {name} has no semantic implementation"
                )));
            };
            let observed = matches!(name, "GetChild" | "GetName" | "GetParent" | "GetNumChildren"
                | "name" | "aux" | "getaux" | "x" | "y" | "zoomx" | "vibrate" | "effectmagnitude"
                | "Center" | "FullScreen" | "animate" | "decelerate" | "diffuse" | "diffusealpha"
                | "effectperiod" | "linear" | "queuemessage" | "rainbow" | "rotationz"
                | "spin" | "visible" | "zoom" | "zoomto"
                | "SetUpdateFunction" | "SetTarget" | "GetTexture" | "loop" | "rate")
                && crate::live::active(lua)
                && log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace);
            let trace_args = observed.then(|| args.clone());
            if observed { crate::live::query_phase(lua, true); }
            let result = method.call::<MultiValue>(args);
            if let Some(args) = trace_args {
                crate::live::query_phase(lua, false);
                crate::live::trace_bound_method(lua, name, &args, &result);
            }
            result
        })?,
    )
}

pub(crate) fn bind(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let bound = match actor.raw_get::<Option<Table>>(METHODS_KEY)? {
        Some(bound) => bound,
        None => lua.create_table()?,
    };
    let mut methods = Vec::new();
    actor.for_each::<Value, Value>(|key, value| {
        if let (Value::String(name), Value::Function(function)) = (key, value) {
            let (native, _) = method_name_flags(lua, &name)?;
            if native && function.environment().is_none() {
                methods.push((name, function));
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
