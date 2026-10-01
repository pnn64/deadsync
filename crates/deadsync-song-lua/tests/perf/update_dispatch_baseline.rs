// Frozen from fddc06faa (0.5.1625), before this performance pass.
use super::*;

#[derive(Clone)]
enum SongLuaCompileUpdateJob {
    Recurring { actor: Table, rate: f64 },
    Callback { actor: Table, rate: f64 },
}

struct SongLuaCompileUpdatePlan {
    jobs: Vec<SongLuaCompileUpdateJob>,
}

fn collect_compile_update_jobs(
    lua: &Lua,
    actor: &Table,
    parent_rate: f64,
    jobs: &mut Vec<SongLuaCompileUpdateJob>,
) -> mlua::Result<()> {
    let rate = parent_rate * actor_update_rate(actor)?;
    if actor
        .get::<Option<String>>("__songlua_recurring_update_command")?
        .is_some()
    {
        jobs.push(SongLuaCompileUpdateJob::Recurring {
            actor: actor.clone(),
            rate,
        });
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        collect_compile_update_jobs(lua, &child, rate, jobs)?;
    }
    if let Some(stream) = song_meter_stream_child(lua, actor)? {
        collect_compile_update_jobs(lua, &stream, rate, jobs)?;
    }
    if actor
        .get::<Option<Function>>("__songlua_update_function")?
        .is_some()
    {
        jobs.push(SongLuaCompileUpdateJob::Callback {
            actor: actor.clone(),
            rate,
        });
    }
    Ok(())
}

fn compile_update_jobs(lua: &Lua, root: &Table) -> mlua::Result<Vec<SongLuaCompileUpdateJob>> {
    if lua.app_data_ref::<SongLuaCompileUpdatePlan>().is_none() {
        let mut jobs = Vec::new();
        collect_compile_update_jobs(lua, root, 1.0, &mut jobs)?;
        lua.set_app_data(SongLuaCompileUpdatePlan { jobs });
    }
    Ok(lua
        .app_data_ref::<SongLuaCompileUpdatePlan>()
        .map(|plan| plan.jobs.clone())
        .unwrap_or_default())
}

pub(crate) fn run_actor_compile_update_functions_with_delta(
    lua: &Lua,
    root: &Value,
    delta_seconds: f64,
) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    for job in compile_update_jobs(lua, root)? {
        match job {
            SongLuaCompileUpdateJob::Recurring { actor, rate } => {
                run_recurring_update(lua, &actor, delta_seconds * rate, true)?;
            }
            SongLuaCompileUpdateJob::Callback { actor, rate } => {
                run_update_callback(lua, &actor, delta_seconds * rate)?;
            }
        }
    }
    Ok(())
}

fn song_meter_stream_child(lua: &Lua, actor: &Table) -> mlua::Result<Option<Table>> {
    if !actor_type_is(actor, "SongMeterDisplay")? {
        return Ok(None);
    }
    let Some(stream) = actor_named_children(lua, actor)?.get::<Option<Table>>("Stream")? else {
        return Ok(None);
    };
    stream.set("__songlua_parent", actor.clone())?;
    Ok(Some(stream))
}

pub(super) fn run_recurring_update(
    lua: &Lua,
    actor: &Table,
    delta_seconds: f64,
    enabled: bool,
) -> mlua::Result<()> {
    let Some(command) = actor.get::<Option<String>>("__songlua_recurring_update_command")? else {
        return Ok(());
    };
    if !enabled {
        return Ok(());
    }
    let mut interval = actor
        .get::<Option<f64>>("__songlua_recurring_update_interval")?
        .unwrap_or(0.0);
    if interval <= f64::EPSILON {
        if let Err(err) = run_actor_named_command(lua, actor, &command) {
            report_update_error(lua, actor, UPDATE_CMD_ERROR_KEY, &command, &err)?;
        }
        return Ok(());
    }

    // The headless ITGmania oracle advances its source-derived tween queue with
    // double-precision frame times. A command behind a sleep only runs when
    // that frame has positive delta left after completing the sleep; exact
    // equality leaves the zero-time command queued until the next frame.
    let mut time_left = actor
        .get::<Option<f64>>("__songlua_recurring_update_time_left")?
        .unwrap_or(interval);
    let mut delta = delta_seconds.max(0.0);
    let mut runs = 0usize;
    while delta > 0.0 && runs < 64 {
        if time_left > 0.0 {
            let elapsed = time_left.min(delta);
            time_left -= elapsed;
            delta -= elapsed;
            if time_left <= 1.0e-7 {
                time_left = 0.0;
            }
            if delta == 0.0 {
                break;
            }
        }

        if let Err(err) = run_actor_named_command(lua, actor, &command) {
            report_update_error(lua, actor, UPDATE_CMD_ERROR_KEY, &command, &err)?;
        }
        runs += 1;
        interval = actor
            .get::<Option<f64>>("__songlua_recurring_update_interval")?
            .unwrap_or(0.0);
        if interval <= f64::EPSILON {
            break;
        }
        time_left = interval;
    }
    actor.set("__songlua_recurring_update_time_left", time_left)?;
    Ok(())
}

pub(super) fn actor_type_is(actor: &Table, expected: &str) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<String>>("__songlua_actor_type")?
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case(expected)))
}

pub(super) fn actor_is_bitmap_text(actor: &Table) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<String>>("__songlua_actor_type")?
        .as_deref()
        .is_some_and(|kind| {
            kind.eq_ignore_ascii_case("BitmapText") || kind.eq_ignore_ascii_case("RollingNumbers")
        }))
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

pub(super) fn invalidate(lua: &Lua) {
    lua.remove_app_data::<SongLuaCompileUpdatePlan>();
}
