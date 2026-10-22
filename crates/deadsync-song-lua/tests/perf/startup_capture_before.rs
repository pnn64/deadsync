// Snapshot helpers frozen from 31a7355f5 with parent visibility.
use super::*;

pub(super) fn snapshot_function_action_table(lua: &Lua, table: Table) -> mlua::Result<Table> {
    let entries = lua.create_table()?;
    let mut count = 0;
    // Keep the traversal key on Lua's stack. `pairs` clones it for its Rust
    // cursor, allocating a shared handle for each reference-valued key.
    table.for_each::<Value, Value>(|key, value| {
        count += 1;
        entries.raw_set(count, lua.create_sequence_from([key, value])?)?;
        Ok(())
    })?;
    lua.create_sequence_from([table, entries])
}

pub(super) fn snapshot_function_action_tables(
    lua: &Lua,
    functions: &Table,
) -> mlua::Result<FunctionActionSnapshot> {
    let globals = lua.globals();
    // Resolve environments before capturing their shared pre-probe state.
    let targets = lua.create_table()?;
    let mut seen = HashSet::from([globals.to_pointer() as usize]);
    for function in functions.sequence_values::<Function>() {
        let function = function?;
        if let Some(environment) = function.environment() {
            let target = environment
                .raw_get::<Option<Table>>("__songlua_env_target")?
                .unwrap_or(environment);
            if seen.insert(target.to_pointer() as usize) {
                targets.raw_set(targets.raw_len() + 1, target)?;
            }
        }
    }
    let snapshots = lua.create_table()?;
    snapshots.raw_set(1, snapshot_function_action_table(lua, globals)?)?;
    for target in targets.sequence_values::<Table>() {
        snapshots.raw_set(snapshots.raw_len() + 1, snapshot_function_action_table(lua, target?)?)?;
    }
    snapshot_function_locals(lua, functions, snapshots)
}

pub(super) fn snapshot_function_locals(
    lua: &Lua,
    functions: &Table,
    snapshots: Table,
) -> mlua::Result<FunctionActionSnapshot> {
    // Command probes may edit shared local tables or replace upvalue cells.
    // Preserve their identities, including cycles and aliases. Actor state
    // belongs to the existing action capture scope; C closures own host data.
    let cells = lua.create_table()?;
    let mut cell_count = 0;
    let mut seen = HashSet::new();
    // Global bindings are shallow snapshots. Preserve referenced table contents
    // alongside locals without following the entire host environment through _G.
    seen.insert(lua.globals().to_pointer() as usize);
    for snapshot in snapshots.sequence_values::<Table>() {
        seen.insert(snapshot?.raw_get::<Table>(1)?.to_pointer() as usize);
    }
    // Lua owns both entries and snapshot rows. Even two Rust Table handles per
    // saved table exhaust mlua's 8000-slot Lua 5.1 auxiliary stack for large
    // scripts such as Bad Trails. Keep only the current row in Rust.
    let pending = functions.clone();
    let mut pending_count = pending.raw_len();
    while pending_count > 0 {
        let value = pending.raw_get::<Value>(pending_count)?;
        pending.raw_set(pending_count, Value::Nil)?;
        pending_count -= 1;
        match value {
            Value::Function(function)
                if function.info().what != "C" && seen.insert(function.to_pointer() as usize) =>
            {
                let chunk = function.dump(true);
                let globals = lua.globals();
                let environment = function
                    .environment()
                    .map(|environment| {
                        Ok::<_, mlua::Error>(
                            environment
                                .raw_get::<Option<Table>>("__songlua_env_target")?
                                .unwrap_or(environment),
                        )
                    })
                    .transpose()?
                    .filter(|environment| environment.to_pointer() != globals.to_pointer());
                for namespace in [Some(globals), environment].into_iter().flatten() {
                    namespace.for_each::<Value, Value>(|key, value| {
                        if let Value::String(name) = key
                            && matches!(value, Value::Table(_) | Value::Function(_))
                        {
                            let name = name.as_bytes();
                            if !name.is_empty()
                                && chunk
                                    .windows(name.len())
                                    .any(|bytes| bytes == name.as_ref())
                            {
                                pending_count += 1;
                                pending.raw_set(pending_count, value)?;
                            }
                        }
                        Ok(())
                    })?;
                }
                for index in 1..=function.info().num_upvalues {
                    let (name, value) = read_function_upvalue(lua, function.clone(), index.into())?;
                    if matches!(&name, Value::String(name) if name.to_str()?.as_ref() == "_ENV") {
                        continue;
                    }
                    pending_count += 1;
                    pending.raw_set(pending_count, value.clone())?;
                    cell_count += 1;
                    cells.raw_set(
                        cell_count,
                        lua.create_sequence_from([
                            Value::Function(function.clone()),
                            Value::Integer(index.into()),
                            value,
                        ])?,
                    )?;
                }
            }
            Value::Table(table)
                if seen.insert(table.to_pointer() as usize)
                    && table.raw_get::<Value>("__songlua_actor_type")?.is_nil() =>
            {
                let snapshot = snapshot_function_action_table(lua, table)?;
                for entry in snapshot.raw_get::<Table>(2)?.sequence_values::<Table>() {
                    pending_count += 1;
                    pending.raw_set(pending_count, entry?.raw_get::<Value>(2)?)?;
                }
                snapshots.raw_set(snapshots.raw_len() + 1, snapshot)?;
            }
            _ => {}
        }
    }
    Ok(FunctionActionSnapshot {
        tables: snapshots,
        cells,
    })
}

pub(super) fn restore_function_action_tables(lua: &Lua, snapshot: FunctionActionSnapshot) -> mlua::Result<()> {
    for cell in snapshot.cells.sequence_values::<Table>() {
        let cell = cell?;
        let function = cell.raw_get::<Function>(1)?;
        let index = cell.raw_get::<i64>(2)?;
        let value = cell.raw_get::<Value>(3)?;
        // SAFETY: exec_raw owns the function, index and saved value on its
        // stack. lua_setupvalue consumes only the saved value; no Lua stack
        // references escape this call.
        unsafe {
            lua.exec_raw::<()>((function, index, value), |state| {
                ffi::lua_setupvalue(state, 1, ffi::lua_tointeger(state, 2) as c_int);
                ffi::lua_settop(state, 0);
            })?;
        }
    }
    for row in snapshot.tables.sequence_values::<Table>() {
        let row = row?;
        let table = row.raw_get::<Table>(1)?;
        table.clear()?;
        for entry in row.raw_get::<Table>(2)?.sequence_values::<Table>() {
            let entry = entry?;
            table.raw_set(entry.raw_get::<Value>(1)?, entry.raw_get::<Value>(2)?)?;
        }
    }
    Ok(())
}
