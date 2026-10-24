//! Snapshot immutable arrays of raw held-state tables for native callbacks.
use super::*;

pub(super) fn capture_array(
    root: &Table,
    object: u32,
    first_state_object: u32,
    actors: &FxHashMap<usize, usize>,
    reserved: &[usize],
    ids: &mut FxHashMap<usize, u32>,
    bindings: &mut program::Bindings,
) -> mlua::Result<()> {
    let count = root.raw_len();
    if count == 0 || count > 4096 {
        return Err(mlua::Error::runtime("unsupported native state array size"));
    }
    let mut children = vec![None; count];
    root.for_each::<InspectionValue, InspectionValue>(|key, value| {
        let index = match key {
            InspectionValue::Integer(index) if index >= 1 && index as u64 <= count as u64 => {
                index as usize
            }
            InspectionValue::Number(index)
                if index >= 1.0 && index <= count as f64 && index.fract() == 0.0 =>
            {
                index as usize
            }
            _ => {
                return Err(mlua::Error::runtime(
                    "native state array needs dense numeric keys",
                ));
            }
        };
        let InspectionValue::Table(table) = value else {
            return Err(mlua::Error::runtime(
                "native state array needs table references",
            ));
        };
        children[index - 1] = Some(table);
        Ok(())
    })?;
    let mut references = Vec::with_capacity(count);
    for child in children {
        let child = child.ok_or_else(|| mlua::Error::runtime("native state array has a hole"))?;
        let pointer = child.to_pointer() as usize;
        if actors.contains_key(&pointer)
            || reserved.contains(&pointer)
            || child.metatable().is_some()
        {
            return Err(mlua::Error::runtime(
                "unsupported native state array target",
            ));
        }
        let id = if let Some(&id) = ids.get(&pointer) {
            if !bindings.boolean_tables.iter().any(|map| map.object == id) {
                return Err(mlua::Error::runtime(
                    "native state table ownership overlaps",
                ));
            }
            id
        } else {
            let id = u32::try_from(ids.len())
                .ok()
                .and_then(|offset| first_state_object.checked_add(offset))
                .ok_or_else(|| mlua::Error::runtime("too many native state tables"))?;
            let mut entries = Vec::new();
            child.for_each::<InspectionValue, InspectionValue>(|key, value| {
                let (InspectionValue::Bytes(key), InspectionValue::Boolean(value)) = (key, value)
                else {
                    return Err(mlua::Error::runtime(
                        "native Boolean state needs byte keys and Boolean values",
                    ));
                };
                entries.push((key, value));
                Ok(())
            })?;
            entries.sort();
            bindings.boolean_tables.push(program::BooleanTable {
                object: id,
                entries,
            });
            ids.insert(pointer, id);
            id
        };
        references.push(id);
    }
    bindings
        .boolean_table_arrays
        .push(program::BooleanTableArray {
            object,
            tables: references,
        });
    Ok(())
}
