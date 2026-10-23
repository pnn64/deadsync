use super::*;

pub(crate) fn forward_inputs(lua: &Lua, class: &Table, name: &'static str) -> mlua::Result<()> {
    class.set(
        name,
        lua.create_function(move |lua, args: ForwardArgs| {
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
            let observed = matches!(
                name,
                "GetChild"
                    | "GetName"
                    | "GetParent"
                    | "GetNumChildren"
                    | "name"
                    | "aux"
                    | "getaux"
                    | "x"
                    | "y"
                    | "zoomx"
                    | "vibrate"
                    | "effectmagnitude"
                    | "Center"
                    | "FullScreen"
                    | "animate"
                    | "decelerate"
                    | "diffuse"
                    | "diffusealpha"
                    | "effectperiod"
                    | "linear"
                    | "queuemessage"
                    | "rainbow"
                    | "rotationz"
                    | "spin"
                    | "visible"
                    | "zoom"
                    | "zoomto"
                    | "SetUpdateFunction"
                    | "SetTarget"
                    | "GetTexture"
                    | "loop"
                    | "rate"
            ) && crate::live::active(lua)
                && log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace);
            if observed {
                // Trace reporting keeps the complete MultiValue path.
                let args = args.into_lua_multi(lua)?;
                let trace_args = args.clone();
                crate::live::query_phase(lua, true);
                let result = method.call::<MultiValue>(args);
                crate::live::query_phase(lua, false);
                crate::live::trace_bound_method(lua, name, &trace_args, &result);
                result
            } else {
                method.call::<MultiValue>(args)
            }
        })?,
    )
}

pub(crate) fn forward_outputs(lua: &Lua, class: &Table, name: &'static str) -> mlua::Result<()> {
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
            let observed = matches!(
                name,
                "GetChild"
                    | "GetName"
                    | "GetParent"
                    | "GetNumChildren"
                    | "name"
                    | "aux"
                    | "getaux"
                    | "x"
                    | "y"
                    | "zoomx"
                    | "vibrate"
                    | "effectmagnitude"
                    | "Center"
                    | "FullScreen"
                    | "animate"
                    | "decelerate"
                    | "diffuse"
                    | "diffusealpha"
                    | "effectperiod"
                    | "linear"
                    | "queuemessage"
                    | "rainbow"
                    | "rotationz"
                    | "spin"
                    | "visible"
                    | "zoom"
                    | "zoomto"
                    | "SetUpdateFunction"
                    | "SetTarget"
                    | "GetTexture"
                    | "loop"
                    | "rate"
            ) && crate::live::active(lua)
                && log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace);
            if observed {
                // Trace reporting keeps the complete MultiValue path.
                let args = args.into_lua_multi(lua)?;
                let trace_args = args.clone();
                crate::live::query_phase(lua, true);
                let result = method.call::<MultiValue>(args);
                crate::live::query_phase(lua, false);
                crate::live::trace_bound_method(lua, name, &trace_args, &result);
                result.map(ForwardResults::from)
            } else {
                method.call::<ForwardResults>(args)
            }
        })?,
    )
}
