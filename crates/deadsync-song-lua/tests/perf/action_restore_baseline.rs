// Frozen from e7501fe239507bde3eaa96cd30c4affaacad10db; only test visibility/formatting differs.

use super::*;

pub(super) fn restore_function_action_tables(
    snapshots: Vec<FunctionActionTableSnapshot>,
) -> mlua::Result<()> {
    for snapshot in snapshots {
        let keys = snapshot
            .table
            .clone()
            .pairs::<Value, Value>()
            .map(|pair| pair.map(|(key, _)| key))
            .collect::<mlua::Result<Vec<_>>>()?;
        for key in keys {
            snapshot.table.raw_set(key, Value::Nil)?;
        }
        for (key, value) in snapshot.entries {
            snapshot.table.raw_set(key, value)?;
        }
    }
    Ok(())
}
