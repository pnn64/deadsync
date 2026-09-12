// Frozen from 619d10d4a340b2d5aa9cebe90ea38dc080059338; only test visibility/formatting differs.

use super::*;

pub(super) fn snapshot_function_action_table(
    table: Table,
) -> mlua::Result<FunctionActionTableSnapshot> {
    let entries = table
        .clone()
        .pairs::<Value, Value>()
        .collect::<mlua::Result<Vec<_>>>()?;
    Ok(FunctionActionTableSnapshot { table, entries })
}

pub(super) fn snapshot_function_action_tables(
    lua: &Lua,
    function: &Function,
) -> mlua::Result<Vec<FunctionActionTableSnapshot>> {
    let globals = lua.globals();
    let mut tables = vec![globals.clone()];
    if let Some(environment) = function.environment() {
        let target = environment
            .raw_get::<Option<Table>>("__songlua_env_target")?
            .unwrap_or(environment);
        if tables
            .iter()
            .all(|table| table.to_pointer() != target.to_pointer())
        {
            tables.push(target);
        }
    }
    tables
        .into_iter()
        .map(snapshot_function_action_table)
        .collect()
}
