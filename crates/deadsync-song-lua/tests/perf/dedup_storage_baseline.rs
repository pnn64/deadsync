//! Frozen from cbc8023529ef1a076816d3d7dc0d83047aaaab38; only function visibility and formatting differ.

use super::*;

// Lua's mixed integer/float comparison is not transitive above 2^53.
// Keep exact integers and their rounded float representations separately.
#[derive(Eq, Hash, PartialEq)]
enum DeduplicateKey {
    Boolean(bool),
    Integer(i64),
    Number(u64),
    IntegerAsNumber(u64),
    String(mlua::LuaString),
    Table(usize),
}

#[expect(
    clippy::mutable_key_type,
    reason = "Lua strings hash immutable bytes; only their reference counts are mutable"
)]
fn insert_deduplicate_value(
    seen: &mut std::collections::HashSet<DeduplicateKey>,
    value: &Value,
) -> bool {
    fn number_bits(value: f64) -> u64 {
        if value == 0.0 { 0 } else { value.to_bits() }
    }
    match value {
        Value::Boolean(value) => seen.insert(DeduplicateKey::Boolean(*value)),
        Value::Integer(value) => {
            let bits = number_bits(*value as f64);
            if seen.contains(&DeduplicateKey::Number(bits))
                || !seen.insert(DeduplicateKey::Integer(*value))
            {
                return false;
            }
            seen.insert(DeduplicateKey::IntegerAsNumber(bits));
            true
        }
        Value::Number(value) if !value.is_nan() => {
            let bits = number_bits(*value);
            !seen.contains(&DeduplicateKey::IntegerAsNumber(bits))
                && seen.insert(DeduplicateKey::Number(bits))
        }
        Value::String(value) if value.to_str().is_ok() => {
            seen.insert(DeduplicateKey::String(value.clone()))
        }
        Value::Table(value) => seen.insert(DeduplicateKey::Table(value.to_pointer() as usize)),
        // NaNs, invalid UTF-8 strings and opaque values never compare equal in
        // lua_values_equal. Sequence iteration stops before a nil value.
        _ => true,
    }
}

pub(super) fn deduplicate_lua_table(lua: &Lua, table: &Table) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    let mut seen = Vec::new();
    let mut indexed = None;
    let mut out_index = 1;
    for value in table.sequence_values::<Value>() {
        let value = value?;
        if let Some(index) = &mut indexed {
            if !insert_deduplicate_value(index, &value) {
                continue;
            }
        } else {
            if seen.iter().any(|seen| lua_values_equal(seen, &value)) {
                continue;
            }
            // Short lists retain the inexpensive linear path. Only accepted
            // values enter the index, preserving order-dependent comparisons.
            // Insert the 33rd value directly so the discarded Vec never grows.
            if seen.len() == 32 && table.raw_len() >= 128 {
                #[expect(
                    clippy::mutable_key_type,
                    reason = "Lua strings hash immutable bytes; only their reference counts are mutable"
                )]
                let mut index = std::collections::HashSet::with_capacity(64);
                for value in &seen {
                    insert_deduplicate_value(&mut index, value);
                }
                insert_deduplicate_value(&mut index, &value);
                seen = Vec::new();
                indexed = Some(index);
            } else {
                seen.push(value.clone());
            }
        }
        out.raw_set(out_index, value)?;
        out_index += 1;
    }
    Ok(out)
}
