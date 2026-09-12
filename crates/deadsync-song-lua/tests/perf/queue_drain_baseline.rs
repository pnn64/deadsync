// Frozen from ff42a7005d492d6bf2b732ea8a80bcbfa40654b4; only test visibility/formatting differs.

use super::*;

pub(super) fn drain_actor_command_queue(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let queue = actor_command_queue(lua, actor)?;
    if let Some(mut startup) = lua.app_data_mut::<SongLuaStartupQueues>() {
        if queue.raw_len() > 0
            && !startup
                .0
                .iter()
                .any(|queued| queued.to_pointer() == actor.to_pointer())
        {
            startup.0.push(actor.clone());
        }
        return Ok(());
    }
    while queue.raw_len() > 0 {
        let Some(name) = queue.raw_get::<Option<String>>(1)? else {
            break;
        };
        let len = queue.raw_len();
        for index in 1..len {
            let value = queue.raw_get::<Value>(index + 1)?;
            queue.raw_set(index, value)?;
        }
        queue.raw_set(len, Value::Nil)?;
        // Effect setters bypass the tween queue, but a queued command itself
        // begins at the current queue cursor. Preserve that command-local now.
        let immediate_start_key = "__songlua_capture_immediate_start";
        let previous_immediate_start = actor.get::<Value>(immediate_start_key)?;
        actor.set(
            immediate_start_key,
            actor
                .get::<Option<f32>>("__songlua_capture_cursor")?
                .unwrap_or(0.0),
        )?;
        let result = run_actor_message_with_params(lua, actor, &format!("{name}Command"), None);
        actor.set(immediate_start_key, previous_immediate_start)?;
        result?;
    }
    Ok(())
}

pub(super) fn run_actor_message_with_params(
    lua: &Lua,
    actor: &Table,
    name: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    run_actor_named_command_with_drain_and_params(lua, actor, name, true, params.clone())?;
    if actor_type_is(actor, "ActorFrame")? || actor_type_is(actor, "ActorFrameTexture")? {
        for child in actor_direct_children(lua, actor)? {
            run_actor_message_with_params(lua, &child, name, params.clone())?;
        }
    }
    Ok(())
}

pub(super) fn run_actor_named_command_with_drain_and_params(
    lua: &Lua,
    actor: &Table,
    name: &str,
    drain_queue: bool,
    params: Option<Value>,
) -> mlua::Result<()> {
    let Some(command) = actor.get::<Option<Function>>(name)? else {
        return Ok(());
    };
    run_guarded_actor_command(lua, actor, name, &command, drain_queue, params)
}

pub(super) fn run_guarded_actor_command(
    lua: &Lua,
    actor: &Table,
    name: &str,
    command: &Function,
    drain_queue: bool,
    params: Option<Value>,
) -> mlua::Result<()> {
    let active = actor_active_commands(lua, actor)?;
    if active.get::<Option<bool>>(name)?.unwrap_or(false) {
        return Ok(());
    }
    active.set(name, true)?;
    let recurring_cursor_key = "__songlua_recurring_update_start_cursor";
    let recurring_exact_key = "__songlua_recurring_update_exact_interval";
    let previous_recurring_state = {
        let previous_cursor = actor.get::<Value>(recurring_cursor_key)?;
        let previous_exact = actor.get::<Value>(recurring_exact_key)?;
        actor.set(
            recurring_cursor_key,
            actor
                .get::<Option<f32>>("__songlua_capture_cursor")?
                .unwrap_or(0.0),
        )?;
        actor.set(recurring_exact_key, 0.0_f64)?;
        (previous_cursor, previous_exact)
    };
    let result = call_actor_function(lua, actor, command, params)
        .map_err(|err| {
            mlua::Error::external(format!(
                "{} failed for {}: {err}",
                name,
                actor_debug_label(actor)
            ))
        })
        .and_then(|()| {
            if drain_queue {
                drain_actor_command_queue(lua, actor)?;
            }
            Ok(())
        });
    actor.set(recurring_cursor_key, previous_recurring_state.0)?;
    actor.set(recurring_exact_key, previous_recurring_state.1)?;
    active.set(name, Value::Nil)?;
    result
}
