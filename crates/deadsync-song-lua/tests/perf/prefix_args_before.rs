use super::*;

pub fn make_actor_wrap_width_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        let wrap = value as i32;
        if wrap >= 0 {
            capture_block_set_i32(lua, &actor, "wrap_width_pixels", wrap)?;
        }
        Ok(actor.clone())
    })
}

pub fn make_actor_set_size_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        let Some(height) = method_arg(&args, 1).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        capture_block_set_size(lua, &actor, [width, height])?;
        Ok(actor.clone())
    })
}

pub fn make_actor_tween_method(
    lua: &Lua,
    actor: &Table,
    easing: Option<&'static str>,
) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let duration = args
            .get(1)
            .cloned()
            .and_then(read_f32)
            .unwrap_or(0.0)
            .max(0.0);
        let exact_duration = args
            .get(1)
            .cloned()
            .and_then(read_f64)
            .unwrap_or(0.0)
            .max(0.0);
        begin_actor_tween(lua, &actor, duration, exact_duration, easing)
    })
}
